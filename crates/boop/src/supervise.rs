//! The lane supervisor: the process a lane pane actually runs. It owns one
//! `LaneChannel` and the lane's inbox, so every harness is messaged the same.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{debug, error, info, warn};

use crate::bus;
use crate::channel::{Delivery, LaneChannel, TurnEvent};

/// How often the inbox is re-read while a turn runs.
const POLL: Duration = Duration::from_millis(700);
/// Provider-flake resumes per lane; deepinfra measured ~98% per-request uptime,
/// so a multi-hundred-request lane sees several drops.
const FLAKE_RESUME_CAP: u32 = 5;
/// A turn with no store write AT ALL by this deadline is the silent-stall
/// class (repro: empty stdout+stderr forever); healthy turns write in seconds.
const FIRST_SIGNAL_LIMIT: Duration = Duration::from_secs(30);
/// Mid-turn quiet bound. Measured over a week of healthy traffic: 261
/// in-message gaps over 120s (long tool calls), so 30s here would false-kill.
const STALL_LIMIT: Duration = Duration::from_secs(5 * 60);
/// The text a resumed conversation opens with instead of the full brief.
const RESUME_NUDGE: &str = "The previous turn ended on a provider error you never saw. \
     Re-read your last steps and continue the brief from where you left off.";

/// What the next turn re-opens with after a retryable end. A lane with no
/// pinned conversation re-feeds the full brief: its TUI window was respawned
/// blank, so a nudge would tell the harness to continue work it never saw.
fn resume_text(conversation: Option<String>, brief: &str) -> String {
    match conversation {
        Some(_) => RESUME_NUDGE.to_owned(),
        None => brief.to_owned(),
    }
}

/// What one lane run needs.
pub struct LaneRun {
    pub lane: String,
    pub brief: PathBuf,
    pub mail_dir: PathBuf,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub resume: Option<String>,
}

/// One inbox message the supervisor has taken responsibility for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hail {
    pub id: String,
    pub from: String,
    pub body: String,
}

/// Every unacked message addressed to `lane` whose id is not already in
/// `seen`. Reading is enough to own it; the ack is written on delivery.
pub fn pending(dir: &Path, lane: &str, seen: &BTreeSet<String>) -> Result<Vec<Hail>> {
    let mut rows = Vec::new();
    for path in bus::read_boxes(dir)? {
        rows.extend(bus::parse_box(&path));
    }
    Ok(bus::unacked(&rows)
        .into_iter()
        .filter(|row| row.to == lane)
        .filter(|row| !seen.contains(&row.id))
        .filter(|row| deliverable(&row.kind))
        .map(|row| Hail {
            id: row.id,
            from: row.from,
            body: row.body,
        })
        .collect())
}

/// A lane acts on requests and hails. Its own dispatch row and result rows are
/// bookkeeping and would loop straight back into the agent's context.
fn deliverable(kind: &str) -> bool {
    matches!(kind, "request" | "hail" | "note" | "retry" | "resume")
}

/// The text one hail becomes inside the agent's conversation.
pub fn hail_text(hail: &Hail) -> String {
    format!("[boop hail {} from {}] {}", hail.id, hail.from, hail.body)
}

/// How one lane's supervision ended.
struct Ended {
    exit_code: i32,
    /// The last turn's reason, carried out when the lane ended on a provider
    /// flake so the result row names what killed it.
    detail: Option<String>,
}

/// Run the lane to completion and return the exit code the pane re-raises.
/// Every exit path here writes the result row; the pane epilogue may not run.
pub fn run(lane: LaneRun, channel: &mut dyn LaneChannel) -> Result<i32> {
    let _span = tracing::info_span!(
        "lane.supervise",
        lane = lane.lane,
        cwd = %lane.cwd.display(),
        model = lane.model.as_deref().unwrap_or_default(),
        resume = lane.resume.as_deref().unwrap_or_default(),
    )
    .entered();
    let ended = supervise(&lane, channel);
    let ended = match ended {
        Ok(ended) => ended,
        Err(error) => {
            record_result(&lane, 1, Some(&format!("supervisor error: {error}")));
            return Err(error);
        }
    };
    record_result(&lane, ended.exit_code, ended.detail.as_deref());
    Ok(ended.exit_code)
}

fn supervise(lane: &LaneRun, channel: &mut dyn LaneChannel) -> Result<Ended> {
    let brief = std::fs::read_to_string(&lane.brief)
        .with_context(|| format!("read lane brief {}", lane.brief.display()))?;
    info!(brief = %lane.brief.display(), "lane brief loaded");
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut held: Vec<Hail> = Vec::new();
    // `conversation_id` may already exist for a freshly opened channel. Codex
    // app-server returns its new thread id from `thread/start` before the first
    // turn, so only the caller's explicit resume input proves that the thread
    // already holds the brief.
    let mut turn = match &lane.resume {
        Some(conversation) => {
            info!(
                conversation_id = conversation,
                "lane resuming pinned conversation"
            );
            println!("[boop] resuming conversation {conversation}");
            RESUME_NUDGE.to_owned()
        }
        None => brief.clone(),
    };
    let mut flake_resumes = 0u32;
    loop {
        info!(turn_bytes = turn.len(), "lane turn starting");
        let turn_started = crate::channel::now_ms();
        channel.start_turn(&turn)?;
        remember_conversation(lane, channel);
        let end = loop {
            match channel.next_event(POLL)? {
                Some(TurnEvent::Started) | None => {}
                Some(end) => break end,
            }
            let this_turn_activity = channel
                .last_activity_ms()
                .filter(|written| *written >= turn_started);
            let (idle_ms, limit) = match this_turn_activity {
                Some(written) => (
                    crate::channel::now_ms().saturating_sub(written),
                    STALL_LIMIT,
                ),
                None => (
                    crate::channel::now_ms().saturating_sub(turn_started),
                    FIRST_SIGNAL_LIMIT,
                ),
            };
            if idle_ms > limit.as_millis() as u64 {
                warn!(idle_ms, "lane turn stalled; killing the harness child");
                println!("[boop] turn stalled ({}s idle), retrying", idle_ms / 1000);
                channel.close().inspect_err(|error| {
                    error!(error = %error, "stalled lane channel close failed");
                })?;
                break TurnEvent::flaked(format!(
                    "stalled: {}s with no harness activity",
                    idle_ms / 1000
                ));
            }
            for hail in pending(&lane.mail_dir, &lane.lane, &seen)? {
                seen.insert(hail.id.clone());
                match channel.steer(&hail_text(&hail))? {
                    Delivery::MidTurn => {
                        println!("[boop] hail {} delivered midturn", hail.id);
                        info!(
                            hail_id = hail.id,
                            from = hail.from,
                            delivery = "midturn",
                            "lane hail delivered"
                        );
                        record_delivery(&lane.mail_dir, &lane.lane, &hail, Delivery::MidTurn);
                    }
                    Delivery::NextTurn => {
                        println!("[boop] hail {} held for the next turn", hail.id);
                        info!(
                            hail_id = hail.id,
                            from = hail.from,
                            delivery = "nextturn",
                            "lane hail held"
                        );
                        held.push(hail);
                    }
                }
            }
        };
        println!("[boop] turn ended: {}", end.detail());
        info!(
            turn_end_reason = end.detail(),
            turn_ok = end.is_done(),
            retryable = end.retryable(),
            "lane turn ended"
        );
        remember_conversation(lane, channel);
        if end.retryable() && flake_resumes < FLAKE_RESUME_CAP {
            flake_resumes += 1;
            println!("[boop] provider flake, resuming ({flake_resumes}/{FLAKE_RESUME_CAP})");
            warn!(
                flake_resumes,
                flake_resume_cap = FLAKE_RESUME_CAP,
                "lane provider flake; resuming"
            );
            turn = resume_text(channel.conversation_id(), &brief);
            continue;
        }
        for hail in pending(&lane.mail_dir, &lane.lane, &seen)? {
            seen.insert(hail.id.clone());
            held.push(hail);
        }
        if held.is_empty() {
            channel.close().inspect_err(|error| {
                error!(error = %error, "lane channel close failed");
            })?;
            let exit_code = match end.is_done() {
                true => 0,
                false => 1,
            };
            info!(exit_code, "lane supervision complete");
            return Ok(Ended {
                exit_code,
                detail: end.retryable().then(|| end.detail().to_owned()),
            });
        }
        turn = held
            .drain(..)
            .map(|hail| {
                record_delivery(&lane.mail_dir, &lane.lane, &hail, Delivery::NextTurn);
                hail_text(&hail)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
    }
}

/// The lane's parent per the registry. The pane epilogue addresses its result
/// hail the same way, so both rows answer the same wait.
fn registered_parent(dir: &Path, lane: &str) -> Option<String> {
    bus::read_routes(dir).ok()?.get(lane)?.parent.clone()
}

/// The result row body. `rc=<code>` is the token `lane wait` parses out; the
/// flake reason rides behind it for a human reading the mailbox.
fn result_body(lane: &str, exit_code: i32, detail: Option<&str>) -> String {
    match detail {
        Some(detail) => format!("lane {lane} done rc={exit_code} ({detail})"),
        None => format!("lane {lane} done rc={exit_code}"),
    }
}

/// Write the lane's result row before the pane can evaporate: a killed pane
/// never runs its epilogue, and the waiter reads only this mailbox.
fn record_result(lane: &LaneRun, exit_code: i32, detail: Option<&str>) {
    let Some(parent) = registered_parent(&lane.mail_dir, &lane.lane) else {
        debug!(
            lane = lane.lane,
            exit_code, "lane has no registered parent; no result row written"
        );
        return;
    };
    let row = bus::Message {
        id: bus::mint_id(),
        from: lane.lane.clone(),
        to: parent.clone(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: "result".into(),
        reply_to: None,
        body: result_body(&lane.lane, exit_code, detail),
        r#ref: None,
    };
    match append_row(&lane.mail_dir, &row) {
        Ok(()) => {
            info!(
                lane = lane.lane,
                parent, exit_code, "lane result row written"
            );
            println!("[boop] result rc={exit_code} hailed to {parent}");
        }
        Err(error) => {
            error!(lane = lane.lane, parent, error = %error, "lane result row write failed");
            println!("[boop] result row write failed: {error}");
        }
    }
}

/// Append one row to the lane's mailbox, the same file and line format the
/// `boop hail` verb appends to.
fn append_row(dir: &Path, row: &bus::Message) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(dir)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("bus.ndjson"))?;
    writeln!(file, "{}", bus::message_line(row))
}

/// Pin the harness's current conversation to the lane route and to the lane's
/// trace. The id MOVES on `/clear`, on compaction and on resume; the trace does
/// not, so every id this lane ever wears lands under one trace.
fn remember_conversation(lane: &LaneRun, channel: &dyn LaneChannel) {
    let Some(id) = channel.conversation_id() else {
        debug!(lane = lane.lane, "lane channel has no conversation id yet");
        return;
    };
    info!(
        lane = lane.lane,
        conversation_id = id,
        conversation_id_kind = channel.conversation_id_kind(),
        "lane conversation resolved"
    );
    record_conversation(&lane.mail_dir, &lane.lane, &id);
    let store = match crate::Store::default_path().and_then(crate::Store::open) {
        Ok(store) => store,
        Err(error) => {
            warn!(lane = lane.lane, error = %error, "open trace store failed");
            return;
        }
    };
    let trace = store
        .trace_of(&lane.lane)
        .ok()
        .flatten()
        .unwrap_or_else(|| format!("trace-{}", lane.lane));
    if let Err(error) = store.attach_trace(&lane.lane, &trace, "lane-run", crate::channel::now_ms())
    {
        warn!(lane = lane.lane, trace, error = %error, "lane trace attachment failed");
    }
    if let Err(error) = store.attach_trace(
        &id,
        &trace,
        "supervisor-conversation",
        crate::channel::now_ms(),
    ) {
        warn!(lane = lane.lane, conversation_id = id, trace, error = %error, "conversation trace attachment failed");
    } else {
        info!(
            lane = lane.lane,
            conversation_id = id,
            trace,
            "conversation trace attached"
        );
    }
}

/// The conversation id a previous supervisor pinned for this lane, if any.
/// Read by the cold-restart path so a respawn continues instead of restarting.
pub fn pinned_conversation(dir: &Path, lane: &str) -> Option<String> {
    let raw = std::fs::read_to_string(dir.join("registry.json")).ok()?;
    let map: serde_json::Value = serde_json::from_str(&raw).ok()?;
    map.get(lane)?.get("sessionId")?.as_str().map(str::to_owned)
}

/// Write the harness's own conversation id onto the lane's registry route so a
/// later resume finds it without a transcript scan.
fn record_conversation(dir: &Path, lane: &str, conversation: &str) {
    let path = dir.join("registry.json");
    let lane = lane.to_owned();
    let conversation = conversation.to_owned();
    if let Err(error) = bus::cas_update_json(&path, |map| {
        let entry = map
            .entry(lane.clone())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        if let Some(object) = entry.as_object_mut() {
            object.insert(
                "sessionId".into(),
                serde_json::Value::String(conversation.clone()),
            );
        }
        Ok(())
    }) {
        warn!(lane, conversation_id = conversation, error = %error, "conversation route update failed");
    } else {
        info!(lane, conversation_id = conversation, mail_dir = %dir.display(), "conversation route updated");
    }
}

/// Stamp the mailbox row delivered so no later read re-offers it.
pub fn ack(dir: &Path, hail: &Hail) {
    let mut rows = Vec::new();
    for path in bus::read_boxes(dir).unwrap_or_default() {
        rows.extend(bus::parse_box(&path));
    }
    let Some(mut row) = bus::fold(&rows).into_iter().find(|row| row.id == hail.id) else {
        return;
    };
    row.to_timestamp = Some(bus::now_iso());
    let line = bus::message_line(&row);
    let path = dir.join("bus.ndjson");
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    else {
        return;
    };
    use std::io::Write;
    let _ = writeln!(file, "{line}");
}

/// Ack plus a store edge naming the tier, so `boop db` answers "did the lane
/// get it, and did it land mid-turn".
fn record_delivery(dir: &Path, lane: &str, hail: &Hail, tier: Delivery) {
    ack(dir, hail);
    let store = match crate::Store::default_path().and_then(crate::Store::open) {
        Ok(store) => store,
        Err(error) => {
            warn!(lane, hail_id = hail.id, error = %error, "open store for delivery edge failed");
            return;
        }
    };
    if let Err(error) = store.add_edge_at(
        &hail.from,
        lane,
        &format!("deliver-{}", tier.as_str()),
        crate::channel::now_ms(),
    ) {
        warn!(lane, hail_id = hail.id, delivery = tier.as_str(), error = %error, "delivery edge write failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_box(dir: &Path, rows: &[bus::Message]) {
        use std::io::Write;
        let mut file = std::fs::File::create(dir.join("bus.ndjson")).unwrap();
        for row in rows {
            writeln!(file, "{}", bus::message_line(row)).unwrap();
        }
    }

    fn message(id: &str, to: &str, kind: &str) -> bus::Message {
        bus::Message {
            id: id.into(),
            from: "coordinator".into(),
            to: to.into(),
            from_timestamp: "2026-08-12T00:00:00.000Z".into(),
            to_timestamp: None,
            kind: kind.into(),
            reply_to: None,
            body: format!("body of {id}"),
            r#ref: None,
        }
    }

    // FAIL-PRE-FIX: a respawned supervisor had no route read-back, so every
    // cold restart opened a fresh session with the full brief.
    #[test]
    fn a_pinned_conversation_round_trips_through_the_registry_route() {
        let dir = tempdir();
        assert_eq!(pinned_conversation(&dir, "mine"), None);
        record_conversation(&dir, "mine", "ses_route_1");
        assert_eq!(
            pinned_conversation(&dir, "mine").as_deref(),
            Some("ses_route_1")
        );
        assert_eq!(pinned_conversation(&dir, "other"), None);
    }

    #[test]
    fn pending_takes_only_this_lane_s_unacked_actionable_rows() {
        let dir = tempdir();
        let mut acked = message("m3", "mine", "request");
        acked.to_timestamp = Some("2026-08-12T00:00:01.000Z".into());
        write_box(
            &dir,
            &[
                message("m1", "mine", "request"),
                message("m2", "other", "request"),
                acked,
                message("m4", "mine", "result"),
                message("m5", "mine", "hail"),
            ],
        );
        let rows = pending(&dir, "mine", &BTreeSet::new()).unwrap();
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            vec!["m1", "m5"]
        );
    }

    #[test]
    fn a_seen_id_is_not_offered_twice() {
        let dir = tempdir();
        write_box(&dir, &[message("m1", "mine", "request")]);
        let seen: BTreeSet<String> = ["m1".to_owned()].into_iter().collect();
        assert!(pending(&dir, "mine", &seen).unwrap().is_empty());
    }

    #[test]
    fn hail_text_names_the_id_and_the_sender() {
        let text = hail_text(&Hail {
            id: "m1".into(),
            from: "coordinator".into(),
            body: "stop and write /tmp/x".into(),
        });
        assert_eq!(
            text,
            "[boop hail m1 from coordinator] stop and write /tmp/x"
        );
    }

    #[test]
    fn result_and_dispatch_rows_never_reach_the_agent() {
        assert!(!deliverable("result"));
        assert!(!deliverable("dispatch"));
        assert!(deliverable("request"));
        assert!(deliverable("hail"));
    }

    #[test]
    fn ack_stamps_the_row_so_the_next_read_skips_it() {
        let dir = tempdir();
        write_box(&dir, &[message("m1", "mine", "request")]);
        let hail = pending(&dir, "mine", &BTreeSet::new()).unwrap().remove(0);
        ack(&dir, &hail);
        assert!(pending(&dir, "mine", &BTreeSet::new()).unwrap().is_empty());
    }

    /// A provider stream that is already gone when the first turn opens: the
    /// error path out of `run`, before any conversation id exists.
    struct DeadChannel;

    impl LaneChannel for DeadChannel {
        fn conversation_id(&self) -> Option<String> {
            None
        }
        fn start_turn(&mut self, _text: &str) -> Result<()> {
            anyhow::bail!("provider stream closed")
        }
        fn steer(&mut self, _text: &str) -> Result<Delivery> {
            unreachable!("the turn never opened")
        }
        fn next_event(&mut self, _timeout: Duration) -> Result<Option<TurnEvent>> {
            unreachable!("the turn never opened")
        }
        fn close(&mut self) -> Result<()> {
            Ok(())
        }
    }

    fn parented_lane(dir: &Path, lane: &str, parent: &str) -> LaneRun {
        std::fs::write(
            dir.join("registry.json"),
            serde_json::json!({ lane: { "kind": "lane", "parent": parent } }).to_string(),
        )
        .unwrap();
        let brief = dir.join("brief.md");
        std::fs::write(&brief, "do the work\n").unwrap();
        LaneRun {
            lane: lane.to_owned(),
            brief,
            mail_dir: dir.to_owned(),
            cwd: dir.to_owned(),
            model: None,
            resume: None,
        }
    }

    fn result_rows(dir: &Path) -> Vec<bus::Message> {
        let mut rows = Vec::new();
        for path in bus::read_boxes(dir).unwrap_or_default() {
            rows.extend(bus::parse_box(&path));
        }
        rows.into_iter()
            .filter(|row| row.kind == "result")
            .collect()
    }

    // FAIL-PRE-FIX: the result row lived only in the pane's shell epilogue, so
    // a lane that died inside the supervisor never reported and the waiter hung.
    #[test]
    fn a_supervisor_error_still_writes_the_lane_s_result_row() {
        let dir = tempdir();
        let lane = parented_lane(&dir, "mine", "coordinator");
        assert!(run(lane, &mut DeadChannel).is_err());
        let rows = result_rows(&dir);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].from, "mine");
        assert_eq!(rows[0].to, "coordinator");
        assert!(
            rows[0]
                .body
                .starts_with("lane mine done rc=1 (supervisor error:"),
            "{}",
            rows[0].body
        );
    }

    /// A lane spawned without `--parent` has nobody to report to, and a row
    /// addressed to the empty string would never match a wait.
    #[test]
    fn a_parentless_lane_writes_no_result_row() {
        let dir = tempdir();
        let lane = parented_lane(&dir, "mine", "coordinator");
        std::fs::write(dir.join("registry.json"), r#"{"mine":{"kind":"lane"}}"#).unwrap();
        assert!(run(lane, &mut DeadChannel).is_err());
        assert!(result_rows(&dir).is_empty());
    }

    /// The flake reason rides behind the `rc=` token the waiter parses, so a
    /// stall-killed lane says so in the mailbox.
    #[test]
    fn a_flake_detail_rides_behind_the_rc_token() {
        let body = result_body("mine", 1, Some("stalled: 300s with no harness activity"));
        assert_eq!(
            body,
            "lane mine done rc=1 (stalled: 300s with no harness activity)"
        );
        assert_eq!(
            body.split_whitespace()
                .find_map(|token| token.strip_prefix("rc=")),
            Some("1")
        );
        assert_eq!(result_body("mine", 0, None), "lane mine done rc=0");
    }

    // FAIL-PRE-FIX: the flake resume always fed RESUME_NUDGE, so a lane whose
    // TUI window died before its first turn settled respawned blank and lost
    // the brief: the harness was told to continue work it never saw.
    #[test]
    fn a_resume_without_a_pinned_conversation_refeeds_the_brief() {
        assert_eq!(resume_text(None, "do the work"), "do the work");
        assert_eq!(
            resume_text(Some("ses_x".into()), "do the work"),
            RESUME_NUDGE
        );
    }

    struct FreshIdentifiedChannel {
        turns: Vec<String>,
    }

    impl LaneChannel for FreshIdentifiedChannel {
        fn conversation_id(&self) -> Option<String> {
            Some("fresh-thread-id".to_owned())
        }

        fn start_turn(&mut self, text: &str) -> Result<()> {
            self.turns.push(text.to_owned());
            Ok(())
        }

        fn steer(&mut self, _text: &str) -> Result<Delivery> {
            Ok(Delivery::MidTurn)
        }

        fn next_event(&mut self, _timeout: Duration) -> Result<Option<TurnEvent>> {
            Ok(Some(TurnEvent::ok("completed")))
        }

        fn close(&mut self) -> Result<()> {
            Ok(())
        }
    }

    // FAIL-PRE-FIX: Codex app-server returns a thread id during channel open,
    // before any turn contains the brief. The supervisor used the presence of
    // that id as resume evidence and sent RESUME_NUDGE as the first turn.
    #[test]
    fn a_fresh_identified_channel_receives_the_full_brief() {
        let dir = tempdir();
        let lane = parented_lane(&dir, "mine", "coordinator");
        let mut channel = FreshIdentifiedChannel { turns: Vec::new() };

        assert_eq!(run(lane, &mut channel).unwrap(), 0);
        assert_eq!(channel.turns, ["do the work\n"]);
    }

    #[test]
    fn an_explicit_resume_receives_the_resume_nudge() {
        let dir = tempdir();
        let mut lane = parented_lane(&dir, "mine", "coordinator");
        lane.resume = Some("existing-thread-id".to_owned());
        let mut channel = FreshIdentifiedChannel { turns: Vec::new() };

        assert_eq!(run(lane, &mut channel).unwrap(), 0);
        assert_eq!(channel.turns, [RESUME_NUDGE]);
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "boop-supervise-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
