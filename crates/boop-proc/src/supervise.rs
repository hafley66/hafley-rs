//! The lane supervisor: the process a lane pane actually runs. It owns one
//! `LaneChannel` and the lane's inbox, so every harness is messaged the same.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::{debug, error, info, warn};

use boop_acp::channel::{Delivery, LaneChannel, TurnEvent};
use boop_store::bus;

/// How often the inbox is re-read while a turn runs.
const POLL: Duration = Duration::from_millis(700);
/// Provider-flake resumes per lane; deepinfra measured ~98% per-request uptime,
/// so a multi-hundred-request lane sees several drops.
const FLAKE_RESUME_CAP: u32 = 5;
/// Config key: whole-turn quiet bound in seconds. Unset/unparsable falls back
/// to `DEFAULT_STALL_LIMIT`.
const STALL_LIMIT_ENV: &str = "BOOP_STALL_LIMIT_SECS";
/// Raised from 5 minutes: a turn legitimately waiting on a background build
/// was killed mid-wait at the old bound.
const DEFAULT_STALL_LIMIT: Duration = Duration::from_secs(30 * 60);

/// How long this turn has been quiet. `activity` is the newest harness write of
/// this turn; without one the clock runs from the turn's own start.
fn idle_ms(now_ms: u64, turn_started: u64, activity: Option<u64>) -> u64 {
    now_ms.saturating_sub(activity.unwrap_or(turn_started))
}

/// Seconds a parked lane (result row written, no mail arriving) stays
/// resident before it closes its channel and exits. A parked claude lane
/// holds a 130-165 MB ACP child; 17 of them sat for three days on 2026-08-27.
const IDLE_SHUTDOWN_ENV: &str = "BOOP_IDLE_SHUTDOWN_SECS";
const DEFAULT_IDLE_SHUTDOWN: Duration = Duration::from_secs(60);

/// `IDLE_SHUTDOWN_ENV` parsed; `0` disables the shutdown.
fn parse_idle_shutdown(raw: Option<&str>) -> Option<Duration> {
    match raw.and_then(|value| value.parse::<u64>().ok()) {
        Some(0) => None,
        Some(secs) => Some(Duration::from_secs(secs)),
        None => Some(DEFAULT_IDLE_SHUTDOWN),
    }
}

fn idle_shutdown() -> Option<Duration> {
    parse_idle_shutdown(std::env::var(IDLE_SHUTDOWN_ENV).ok().as_deref())
}

/// The residency a lane records when it leaves on the idle shutdown: its
/// conversation is pinned on the route and `lane create --resume` re-opens it.
pub const RESIDENCY_RETIRED: &str = "retired";

/// `STALL_LIMIT_ENV` parsed, isolated from the process environment so a test
/// never mutates global state to exercise it.
fn parse_stall_limit(raw: Option<&str>) -> Duration {
    raw.and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_STALL_LIMIT)
}

/// The stall bound for this process, read once per turn rather than per poll.
fn stall_limit() -> Duration {
    parse_stall_limit(std::env::var(STALL_LIMIT_ENV).ok().as_deref())
}

/// Whether a quiet RUNNING turn is past the point its child is treated as
/// gone. A parked lane between turns never calls this.
fn stalled(idle_ms: u64, limit: Duration) -> bool {
    idle_ms > limit.as_millis() as u64
}

/// The text a resumed conversation opens with instead of the full brief.
const RESUME_NUDGE: &str = "The previous turn ended on a provider error you never saw. \
     Re-read your last steps and continue the brief from where you left off.";

/// The opening turn of a revived lane that finds no mail waiting.
const REVIVE_TEXT: &str = "You were paused after finishing and are now resumed. \
     No new instruction has arrived yet; reply with the single word ready.";

/// What the next turn re-opens with after a retryable end. Until the brief
/// turn has completed, a channel id is insufficient evidence that the
/// harness saw the brief, so the full brief is re-fed.
fn resume_text(brief_completed: bool, conversation: Option<String>, brief: &str) -> String {
    match (brief_completed, conversation) {
        (true, Some(_)) => RESUME_NUDGE.to_owned(),
        _ => brief.to_owned(),
    }
}

pub use boop_store::session::ParentDeathPolicy;

/// The lane-to-policy map beside the registry. Not a registry route field: the
/// spawn rewrites a route wholesale, dropping anything recorded before it.
const PARENT_POLICY_FILE: &str = "parent-policy.json";

/// Record one lane's parent-death policy, so its supervisor reads back the
/// spelling the spawn was given.
pub fn record_parent_policy(dir: &Path, lane: &str, policy: ParentDeathPolicy) -> Result<()> {
    std::fs::create_dir_all(dir).context("create mail dir")?;
    let lane = lane.to_owned();
    bus::cas_update_json(&dir.join(PARENT_POLICY_FILE), |map| {
        map.insert(
            lane.clone(),
            serde_json::Value::String(policy.as_str().to_owned()),
        );
        Ok(())
    })
}

/// Record the policy under the lane id `lane create` derives. The repo path
/// only shapes a worktree directory this call never reads.
pub fn record_spawn_policy(
    dir: &Path,
    branch: Option<&str>,
    lane: Option<&str>,
    policy: ParentDeathPolicy,
) -> Result<()> {
    let identity = crate::lane::derive(Path::new(""), branch, lane, None)?;
    record_parent_policy(dir, &identity.lane, policy)
}

/// The policy the spawn recorded. An absent or unreadable file is `Orphan`,
/// today's behavior for every lane.
pub fn parent_policy(dir: &Path, lane: &str) -> ParentDeathPolicy {
    let Ok(raw) = std::fs::read_to_string(dir.join(PARENT_POLICY_FILE)) else {
        return ParentDeathPolicy::Orphan;
    };
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()
        .and_then(|map| map.get(lane)?.as_str()?.parse().ok())
        .unwrap_or_default()
}

/// The lane-to-residency map beside the registry. A lane this file has never
/// heard of falls back to tmux-only liveness.
pub const RESIDENCY_FILE: &str = "lane-residency.json";

/// One lane's residency between `lane list`'s `live` and `dead`: mid-turn, or
/// parked waiting on its mailbox. Dead is a tmux fact, never written here.
pub const RESIDENCY_LIVE: &str = "live";
pub const RESIDENCY_IDLE: &str = "idle";

/// Record this lane's residency for `lane list` to read back. Best-effort: a
/// write failure never blocks the turn it is reporting on.
pub fn record_residency(dir: &Path, lane: &str, state: &str) {
    let lane_key = lane.to_owned();
    let state = state.to_owned();
    if let Err(error) = bus::cas_update_json(&dir.join(RESIDENCY_FILE), |map| {
        map.insert(lane_key.clone(), serde_json::Value::String(state.clone()));
        Ok(())
    }) {
        warn!(lane, error = %error, "lane residency not recorded");
    }
}

/// The residency `lane list` reads back. `None` for a lane this file has
/// never heard of, or an unreadable file.
pub fn read_residency(dir: &Path, lane: &str) -> Option<String> {
    let raw = std::fs::read_to_string(dir.join(RESIDENCY_FILE)).ok()?;
    serde_json::from_str::<serde_json::Value>(&raw)
        .ok()?
        .get(lane)?
        .as_str()
        .map(str::to_owned)
}

/// What one lane run needs. Cloned into the signal thread, which owns nothing
/// else and must still address the lane's result row.
#[derive(Clone)]
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
    pub kind: String,
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
            kind: row.kind,
            body: row.body,
        })
        .collect())
}

/// A lane acts on requests and hails. Its own dispatch row and result rows are
/// bookkeeping and would loop straight back into the agent's context.
fn deliverable(kind: &str) -> bool {
    matches!(kind, "request" | "hail" | "note" | "retry" | "resume")
}

/// One piece of mail as its receiver reads it. The template is the receiver's
/// effective mood; the four placeholders are all a mood may name.
pub fn render_mail(template: &str, kind: &str, id: &str, from: &str, body: &str) -> String {
    template
        .replace("{kind}", kind)
        .replace("{id}", id)
        .replace("{from}", from)
        .replace("{body}", body)
}

/// The template mail addressed to `receiver` renders through. A store that
/// cannot open leaves the default shape rather than dropping the mail.
pub fn mood_template(receiver: &str) -> String {
    boop_store::Store::default_path()
        .and_then(boop_store::Store::open)
        .and_then(|store| store.effective_mood(receiver))
        .map(|mood| mood.template)
        .unwrap_or_else(|error| {
            warn!(receiver, error = %error, "effective mood unresolved");
            boop_store::ident::DEFAULT_MOOD_TEMPLATE.to_owned()
        })
}

/// The text one hail becomes inside the agent's conversation.
pub fn hail_text(hail: &Hail, template: &str) -> String {
    render_mail(template, &hail.kind, &hail.id, &hail.from, &hail.body)
}

/// How one lane's supervision ended.
struct Ended {
    exit_code: i32,
    /// The last turn's reason, carried out when the lane ended on a provider
    /// flake so the result row names what killed it.
    detail: Option<String>,
    /// The lane left on the idle shutdown after its result row was already
    /// mailed; `run` writes no second row.
    retired: bool,
}

/// What a lane exits with when its parent died under the `kill` policy. The
/// typed reason rides `detail`; nothing new is asked of the rc.
const PARENT_DIED_EXIT: i32 = 1;

/// Whether `parent` is still addressable: a registry route, and for a
/// pane-backed one a live pane or a live recorded pid.
fn parent_alive(dir: &Path, parent: &str, multiplexer: &dyn boop_store::tmux::Multiplexer) -> bool {
    // An unreadable registry is not evidence that anyone died.
    let Ok(routes) = bus::read_routes(dir) else {
        return true;
    };
    let Some(route) = routes.get(parent) else {
        return false;
    };
    let Some(target) = route.tmux.as_deref() else {
        // A pane-less coordinator or native row is addressable for its whole
        // registration; its death is `agent done`, never a missing pane.
        return matches!(route.kind.as_str(), "coordinator" | "native");
    };
    if multiplexer.target_alive(None, target) {
        return true;
    }
    match multiplexer.pane_pid(None, target) {
        Some(pid) => boop_store::proc::SysinfoSnapshot::capture()
            .map(|snapshot| boop_store::proc::ProcReader::is_alive(&snapshot, pid))
            .unwrap_or(true),
        None => false,
    }
}

/// Rewrite the lane's parent edge onto the one registered coordinator and mail
/// it. `None` leaves the lane orphaned: no live coordinator answered.
fn reparent(lane: &LaneRun, dead: &str) -> Option<String> {
    let routes = bus::read_routes(&lane.mail_dir).ok()?;
    let adopter = crate::lane::resolve_parent(None, None, &routes).parent?;
    if adopter == dead || !parent_alive(&lane.mail_dir, &adopter, boop_store::tmux::mux()) {
        warn!(
            lane = lane.lane,
            dead, "no live coordinator to reparent onto"
        );
        return None;
    }
    let lane_id = lane.lane.clone();
    let edge = adopter.clone();
    if let Err(error) = bus::cas_update_json(&lane.mail_dir.join("registry.json"), |map| {
        if let Some(object) = map
            .get_mut(&lane_id)
            .and_then(serde_json::Value::as_object_mut)
        {
            object.insert("parent".into(), serde_json::Value::String(edge.clone()));
        }
        Ok(())
    }) {
        warn!(lane = lane.lane, adopter, error = %error, "parent edge rewrite failed");
        return None;
    }
    let row = bus::Message {
        id: bus::mint_id(),
        from: lane.lane.clone(),
        to: adopter.clone(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: boop_store::trail::REPARENTED.to_owned(),
        reply_to: None,
        body: format!("lane {} reparented to {adopter}: {dead} is gone", lane.lane),
        r#ref: None,
        rc: None,
        detail: None,
    };
    if let Err(error) = append_row(&lane.mail_dir, &row) {
        warn!(lane = lane.lane, adopter, error = %error, "reparent row write failed");
    }
    info!(
        lane = lane.lane,
        adopter, dead, "lane parent edge rewritten"
    );
    println!("[boop] parent {dead} is gone; reparented to {adopter}");
    Some(adopter)
}

/// One lane's parent watch: whom it watches, and what it does when that route
/// stops answering.
struct ParentWatch {
    policy: ParentDeathPolicy,
    parent: Option<String>,
    /// The pane this supervisor runs in. A killed pane reparents the process
    /// to init without a signal; without this probe the supervisor parked on
    /// the mailbox forever and kept its harness children with it.
    own_pane: Option<String>,
}

impl ParentWatch {
    fn new(dir: &Path, lane: &str) -> Self {
        ParentWatch {
            policy: parent_policy(dir, lane),
            parent: registered_parent(dir, lane),
            own_pane: std::env::var("TMUX_PANE")
                .ok()
                .filter(|pane| !pane.is_empty()),
        }
    }

    /// One probe. `Some` ends the lane; the `orphan` default probes nothing
    /// about the parent, so an unchanged spawn pays one tmux call per poll
    /// for its own pane and nothing more.
    fn probe(
        &mut self,
        lane: &LaneRun,
        multiplexer: &dyn boop_store::tmux::Multiplexer,
    ) -> Option<Ended> {
        if let Some(pane) = self.own_pane.as_deref() {
            if !multiplexer.target_alive(None, pane) {
                warn!(lane = lane.lane, pane, "own pane gone; ending the lane");
                println!("[boop] pane {pane} is gone; ending the lane");
                return Some(Ended {
                    exit_code: PARENT_DIED_EXIT,
                    detail: Some(format!("{}: {pane}", boop_store::trail::PANE_GONE)),
                    retired: false,
                });
            }
        }
        if self.policy == ParentDeathPolicy::Orphan {
            return None;
        }
        let parent = self.parent.clone()?;
        if parent_alive(&lane.mail_dir, &parent, multiplexer) {
            return None;
        }
        match self.policy {
            ParentDeathPolicy::Kill => {
                warn!(lane = lane.lane, parent, "parent gone; ending the lane");
                println!("[boop] parent {parent} is gone; ending the lane");
                Some(Ended {
                    exit_code: PARENT_DIED_EXIT,
                    detail: Some(format!("{}: {parent}", boop_store::trail::PARENT_DIED)),
                    retired: false,
                })
            }
            ParentDeathPolicy::Reparent => {
                match reparent(lane, &parent) {
                    Some(adopter) => self.parent = Some(adopter),
                    // Nobody to adopt it: stop probing rather than re-reading a
                    // dead route every poll.
                    None => self.policy = ParentDeathPolicy::Orphan,
                }
                None
            }
            ParentDeathPolicy::Orphan => None,
        }
    }
}

struct TraceRecorder {
    lane: String,
    trace: Option<String>,
    run_id: String,
    sequence: u64,
    store: Option<boop_store::Store>,
}

impl TraceRecorder {
    fn new(lane: &str) -> Self {
        let store = boop_store::Store::default_path()
            .and_then(boop_store::Store::open)
            .map_err(|error| {
                warn!(lane, error = %error, "open trace event store failed");
            })
            .ok();
        let trace = store
            .as_ref()
            .and_then(|store| store.trace_of(lane).ok().flatten());
        Self {
            lane: lane.to_owned(),
            trace,
            run_id: bus::mint_id(),
            sequence: 0,
            store,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn record(
        &mut self,
        kind: &str,
        session: Option<String>,
        started_ts: Option<u64>,
        finished_ts: Option<u64>,
        delivery_state: Option<&str>,
        classification: Option<&str>,
        from_lane: Option<&str>,
        to_lane: Option<&str>,
        detail: &str,
    ) {
        self.sequence += 1;
        let trace_prefix = self.trace.as_deref().unwrap_or("trace-unknown");
        let event = boop_store::TraceEvent {
            event_key: format!(
                "{trace_prefix}/lane/{}/run/{}/event/{}",
                self.lane, self.run_id, self.sequence
            ),
            lane: self.lane.clone(),
            trace: self.trace.clone(),
            session,
            kind: kind.to_owned(),
            from_lane: from_lane.map(str::to_owned),
            to_lane: to_lane.map(str::to_owned),
            started_ts,
            finished_ts,
            delivery_state: delivery_state.map(str::to_owned),
            classification: classification.map(str::to_owned),
            detail: detail.to_owned(),
            created_ts: boop_acp::channel::now_ms(),
        };
        if let Some(store) = &self.store {
            if let Err(error) = store.record_trace_event(&event) {
                warn!(lane = self.lane, kind, error = %error, "trace event write failed");
            }
        }
    }

    fn session(channel: &dyn LaneChannel) -> Option<String> {
        channel.conversation_id()
    }
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
    let mut events = TraceRecorder::new(&lane.lane);
    events.record(
        "supervisor-start",
        TraceRecorder::session(channel),
        None,
        None,
        None,
        Some("starting"),
        None,
        None,
        "lane supervisor started",
    );
    // A panic unwinds past `record_result`, and the pane epilogue that would
    // have covered it dies with the pane.
    let ended = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        supervise(&lane, channel, &mut events)
    }));
    let ended = match ended {
        Err(payload) => {
            let text = panic_text(&payload);
            error!(lane = lane.lane, panic = text, "lane supervisor panicked");
            record_result(&lane, PANIC_EXIT, Some(&format!("panic: {text}")));
            anyhow::bail!("supervisor panic: {text}");
        }
        Ok(ended) => ended,
    };
    let ended = match ended {
        Ok(ended) => ended,
        Err(error) => {
            events.record(
                "error",
                TraceRecorder::session(channel),
                None,
                Some(boop_acp::channel::now_ms()),
                None,
                Some("failed"),
                None,
                None,
                "supervisor error",
            );
            events.record(
                "supervisor-exit",
                TraceRecorder::session(channel),
                None,
                Some(boop_acp::channel::now_ms()),
                None,
                Some("failed"),
                None,
                None,
                "supervisor exited with error",
            );
            record_result(&lane, 1, Some(&format!("supervisor error: {error}")));
            return Err(error);
        }
    };
    let classification = if ended.exit_code == 0 {
        "completed"
    } else {
        "failed"
    };
    events.record(
        "supervisor-exit",
        TraceRecorder::session(channel),
        None,
        Some(boop_acp::channel::now_ms()),
        None,
        Some(classification),
        None,
        None,
        ended.detail.as_deref().unwrap_or("supervisor exited"),
    );
    if !ended.retired {
        record_result(&lane, ended.exit_code, ended.detail.as_deref());
    }
    Ok(ended.exit_code)
}

/// What rustc's own runtime exits with on an unwinding panic.
const PANIC_EXIT: i32 = 101;

/// The panic payload as text. `panic!` carries either of these two shapes and
/// nothing else reaches here.
fn panic_text(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        return (*text).to_owned();
    }
    if let Some(text) = payload.downcast_ref::<String>() {
        return text.clone();
    }
    "unknown panic payload".to_owned()
}

/// The signals a killed pane sends. A default disposition ends the process
/// with no result row and no line in the trail.
const TRAILED_SIGNALS: [i32; 3] = [
    signal_hook::consts::SIGHUP,
    signal_hook::consts::SIGTERM,
    signal_hook::consts::SIGINT,
];

/// Write the result row for a signal death and return the exit code. Split out
/// of the handler thread so the row's shape is tested without ending the test
/// process.
fn signal_exit(lane: &LaneRun, signal: i32) -> i32 {
    let name = signal_hook::low_level::signal_name(signal).unwrap_or("unknown");
    warn!(lane = lane.lane, signal, name, "lane supervisor signalled");
    record_result(lane, 128 + signal, Some(&format!("killed by {name}")));
    128 + signal
}

/// Take over SIGHUP/SIGTERM/SIGINT for the rest of the process, then exit
/// through the result row instead of the default disposition. Process-global,
/// so the binary arms it once around `run` and `run` itself stays testable.
/// A failure to register is logged and never fatal.
pub fn arm_signal_trail(lane: &LaneRun) {
    let mut signals = match signal_hook::iterator::Signals::new(TRAILED_SIGNALS) {
        Ok(signals) => signals,
        Err(error) => {
            warn!(lane = lane.lane, error = %error, "lane signal trail not armed");
            return;
        }
    };
    let lane = lane.clone();
    std::thread::spawn(move || {
        // The first signal is the last: the row is written and the process
        // ends, so nothing here iterates twice.
        if let Some(signal) = signals.forever().next() {
            let code = signal_exit(&lane, signal);
            // `std::process::exit` from this thread runs atexit and stdio
            // cleanup that wait on locks the parked main thread holds; on
            // 2026-08-25 38 supervisors wrote their row and then sat for two
            // days on that exit. The row is already fsync-free on disk, so
            // leave without unwinding anything.
            // SAFETY: _exit is async-signal-safe and touches no Rust state.
            unsafe { libc::_exit(code) }
        }
    });
}

fn supervise(
    lane: &LaneRun,
    channel: &mut dyn LaneChannel,
    events: &mut TraceRecorder,
) -> Result<Ended> {
    let brief = std::fs::read_to_string(&lane.brief)
        .with_context(|| format!("read lane brief {}", lane.brief.display()))?;
    info!(brief = %lane.brief.display(), "lane brief loaded");
    // The channel re-feeds this text after a respawn that lost its
    // conversation, so it must be the brief and not whichever turn opened.
    channel.set_brief(&brief);
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut held: Vec<Hail> = Vec::new();
    let mut opening_hails: Vec<Hail> = Vec::new();
    // Resolved once: a lane's mood is a spawn-time attribute, and re-reading it
    // per hail would open the store inside the delivery path.
    let mood = mood_template(&lane.lane);
    let mut watch = ParentWatch::new(&lane.mail_dir, &lane.lane);
    // `conversation_id` may already exist for a freshly opened channel. Codex
    // app-server returns its new thread id from `thread/start` before the first
    // turn, so only the caller's explicit resume input proves that the thread
    // already holds the brief.
    let mut brief_completed = lane.resume.is_some();
    let mut brief_turn_pending = lane.resume.is_none();
    // A lane retired on the idle shutdown is revived by a send; the mail that
    // revived it is its opening turn, never the flake nudge.
    let revived = lane.resume.is_some()
        && read_residency(&lane.mail_dir, &lane.lane).as_deref() == Some(RESIDENCY_RETIRED);
    let mut turn = match &lane.resume {
        Some(conversation) if revived => {
            info!(
                conversation_id = conversation,
                "lane revived from retirement"
            );
            println!("[boop] revived conversation {conversation}");
            let arrived = pending(&lane.mail_dir, &lane.lane, &seen)?;
            for hail in &arrived {
                seen.insert(hail.id.clone());
                record_hail_transition(events, hail, "claimed-by-supervisor", "revive");
            }
            opening_hails = arrived.clone();
            if arrived.is_empty() {
                REVIVE_TEXT.to_owned()
            } else {
                arrived
                    .iter()
                    .map(|hail| hail_text(hail, &mood))
                    .collect::<Vec<_>>()
                    .join("\n\n")
            }
        }
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
    let mut result_written = false;
    let mut head_watch = HeadWatch::new(&lane.cwd);

    events.record(
        "channel-open",
        TraceRecorder::session(channel),
        None,
        None,
        None,
        Some("opened"),
        None,
        None,
        "lane channel opened",
    );
    loop {
        info!(turn_bytes = turn.len(), "lane turn starting");
        record_residency(&lane.mail_dir, &lane.lane, RESIDENCY_LIVE);
        let limit = stall_limit();
        let turn_started = boop_acp::channel::now_ms();
        events.record(
            "turn-start",
            TraceRecorder::session(channel),
            Some(turn_started),
            None,
            None,
            Some("started"),
            None,
            None,
            "turn submitted",
        );
        if let Err(error) = channel.start_turn(&turn) {
            for hail in &opening_hails {
                record_hail_transition(events, hail, "rejected-by-harness", "start turn failed");
            }
            events.record(
                "error",
                TraceRecorder::session(channel),
                Some(turn_started),
                Some(boop_acp::channel::now_ms()),
                None,
                Some("failed"),
                None,
                None,
                "pre-turn launch failed",
            );
            return Err(error);
        }
        for hail in opening_hails.drain(..) {
            record_delivery(events, &lane.mail_dir, &hail, Delivery::NextTurn);
        }
        remember_conversation(lane, channel);
        let end = loop {
            match channel.next_event(POLL) {
                Err(error) => {
                    events.record(
                        "error",
                        TraceRecorder::session(channel),
                        Some(turn_started),
                        Some(boop_acp::channel::now_ms()),
                        None,
                        Some("failed"),
                        None,
                        None,
                        "turn event read failed",
                    );
                    return Err(error);
                }
                Ok(event) => match event {
                    Some(TurnEvent::Started) | None => {}
                    Some(end) => break end,
                },
            }
            head_watch.poll(lane, boop_acp::channel::now_ms());
            let this_turn_activity = channel
                .last_activity_ms()
                .filter(|written| *written >= turn_started);
            let idle_ms = idle_ms(
                boop_acp::channel::now_ms(),
                turn_started,
                this_turn_activity,
            );
            if stalled(idle_ms, limit) {
                warn!(idle_ms, "lane turn stalled; killing the harness child");
                println!("[boop] turn stalled ({}s idle), retrying", idle_ms / 1000);
                if let Err(error) = channel.close() {
                    events.record(
                        "error",
                        TraceRecorder::session(channel),
                        Some(turn_started),
                        Some(boop_acp::channel::now_ms()),
                        None,
                        Some("failed"),
                        None,
                        None,
                        "stalled channel close failed",
                    );
                    return Err(error);
                }
                break TurnEvent::flaked(format!(
                    "stalled: {}s with no harness activity",
                    idle_ms / 1000
                ));
            }
            if let Some(ended) = watch.probe(lane, boop_store::tmux::mux()) {
                if let Err(error) = channel.close() {
                    warn!(lane = lane.lane, error = %error, "close after parent death failed");
                }
                yield_to_parent(
                    lane,
                    ended
                        .detail
                        .as_deref()
                        .unwrap_or(boop_store::trail::PARENT_DIED),
                );
                events.record(
                    "parent-death",
                    TraceRecorder::session(channel),
                    Some(turn_started),
                    Some(boop_acp::channel::now_ms()),
                    None,
                    Some("failed"),
                    None,
                    None,
                    ended
                        .detail
                        .as_deref()
                        .unwrap_or(boop_store::trail::PARENT_DIED),
                );
                return Ok(ended);
            }
            for hail in pending(&lane.mail_dir, &lane.lane, &seen)? {
                seen.insert(hail.id.clone());
                record_hail_transition(events, &hail, "claimed-by-supervisor", "inbox drain");
                let delivery = match channel.steer(&hail_text(&hail, &mood)) {
                    Ok(delivery) => delivery,
                    Err(error) => {
                        record_hail_transition(
                            events,
                            &hail,
                            "rejected-by-harness",
                            &error.to_string(),
                        );
                        return Err(error);
                    }
                };
                match delivery {
                    Delivery::MidTurn => {
                        record_hail_transition(
                            events,
                            &hail,
                            "submitted-to-harness",
                            "mid-turn steer",
                        );
                        println!("[boop] hail {} delivered midturn", hail.id);
                        info!(
                            hail_id = hail.id,
                            from = hail.from,
                            delivery = "midturn",
                            "lane hail delivered"
                        );
                        record_delivery(events, &lane.mail_dir, &hail, Delivery::MidTurn);
                        events.record(
                            "delivery",
                            TraceRecorder::session(channel),
                            None,
                            None,
                            Some(Delivery::MidTurn.as_str()),
                            Some("delivered"),
                            Some(&hail.from),
                            Some(&lane.lane),
                            "hail delivered",
                        );
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
        // Every turn end reports itself. The parent's picture of this lane
        // never depends on the model choosing to run `tell-parent`.
        yield_to_parent(lane, end.detail());
        let finish = boop_acp::channel::now_ms();
        events.record(
            "turn-finish",
            TraceRecorder::session(channel),
            Some(turn_started),
            Some(finish),
            None,
            Some(if end.is_done() {
                "completed"
            } else if end.retryable() {
                "retryable"
            } else {
                "failed"
            }),
            None,
            None,
            end.detail(),
        );
        info!(
            turn_end_reason = end.detail(),
            turn_ok = end.is_done(),
            retryable = end.retryable(),
            "lane turn ended"
        );
        remember_conversation(lane, channel);
        if brief_turn_pending && end.is_done() {
            brief_completed = true;
            brief_turn_pending = false;
        }
        // The marker: a waiter learns the brief is done as soon as it is, not
        // when the lane eventually exits. Written at most once per lane.
        if end.is_done() && !result_written {
            if let Some((exit_code, detail)) = completion_verdict(brief_completed, &end) {
                record_result(lane, exit_code, detail.as_deref());
                result_written = true;
            }
        }
        if end.retryable() && flake_resumes < FLAKE_RESUME_CAP {
            flake_resumes += 1;
            println!("[boop] provider flake, resuming ({flake_resumes}/{FLAKE_RESUME_CAP})");
            warn!(
                flake_resumes,
                flake_resume_cap = FLAKE_RESUME_CAP,
                "lane provider flake; resuming"
            );
            hail_parent_once(lane, RETRYING, flake_resumes, end.detail());
            turn = resume_text(brief_completed, channel.conversation_id(), &brief);
            continue;
        }
        if end.retryable() {
            hail_parent_once(lane, RETRY_BUDGET_EXHAUSTED, flake_resumes, end.detail());
        }
        for hail in pending(&lane.mail_dir, &lane.lane, &seen)? {
            seen.insert(hail.id.clone());
            record_hail_transition(events, &hail, "claimed-by-supervisor", "turn boundary");
            held.push(hail);
        }
        if held.is_empty() && !end.is_done() {
            // A hard failure or an exhausted flake budget: the harness is
            // treated as gone, so this is a real exit, not an idle park.
            if let Err(error) = channel.close() {
                events.record(
                    "error",
                    TraceRecorder::session(channel),
                    None,
                    Some(boop_acp::channel::now_ms()),
                    None,
                    Some("failed"),
                    None,
                    None,
                    "lane channel close failed",
                );
                return Err(error);
            }
            let (exit_code, detail) = completion_verdict(brief_completed, &end)
                .unwrap_or_else(|| (1, Some(end.detail().to_owned())));
            info!(exit_code, "lane supervision complete");
            return Ok(Ended {
                exit_code,
                detail,
                retired: false,
            });
        }
        if held.is_empty() {
            record_residency(&lane.mail_dir, &lane.lane, RESIDENCY_IDLE);
            println!("[boop] lane idle, parked on the mailbox");
            let parked_at = std::time::Instant::now();
            let shutdown = idle_shutdown().filter(|_| result_written);
            loop {
                if let Some(limit) = shutdown.filter(|limit| parked_at.elapsed() >= *limit) {
                    let secs = limit.as_secs();
                    info!(lane = lane.lane, idle_secs = secs, "lane idle shutdown");
                    println!("[boop] no mail for {secs}s after the result row; retiring");
                    if let Err(error) = channel.close() {
                        warn!(lane = lane.lane, error = %error, "close on idle shutdown failed");
                    }
                    record_residency(&lane.mail_dir, &lane.lane, RESIDENCY_RETIRED);
                    let conversation = channel.conversation_id().unwrap_or_default();
                    mail_to_parent_kind(
                        lane,
                        "note",
                        format!(
                            "lane {} retired: idle {secs}s after its result row; \
                             `boop beep {} <body>` revives conversation {conversation}",
                            lane.lane, lane.lane
                        ),
                        Some(RESIDENCY_RETIRED),
                    );
                    events.record(
                        "idle-shutdown",
                        TraceRecorder::session(channel),
                        None,
                        Some(boop_acp::channel::now_ms()),
                        None,
                        Some("retired"),
                        None,
                        None,
                        "lane retired on idle shutdown",
                    );
                    let (exit_code, detail) = completion_verdict(brief_completed, &end)
                        .unwrap_or_else(|| (1, Some(end.detail().to_owned())));
                    return Ok(Ended {
                        exit_code,
                        detail,
                        retired: true,
                    });
                }
                if let Some(ended) = watch.probe(lane, boop_store::tmux::mux()) {
                    if let Err(error) = channel.close() {
                        warn!(lane = lane.lane, error = %error, "close while parked failed");
                    }
                    yield_to_parent(
                        lane,
                        ended
                            .detail
                            .as_deref()
                            .unwrap_or(boop_store::trail::PARENT_DIED),
                    );
                    events.record(
                        "parent-death",
                        TraceRecorder::session(channel),
                        None,
                        Some(boop_acp::channel::now_ms()),
                        None,
                        Some("failed"),
                        None,
                        None,
                        ended
                            .detail
                            .as_deref()
                            .unwrap_or(boop_store::trail::PARENT_DIED),
                    );
                    return Ok(ended);
                }
                head_watch.poll(lane, boop_acp::channel::now_ms());
                let arrived = pending(&lane.mail_dir, &lane.lane, &seen)?;
                if arrived.is_empty() {
                    std::thread::sleep(POLL);
                    continue;
                }
                for hail in arrived {
                    seen.insert(hail.id.clone());
                    record_hail_transition(
                        events,
                        &hail,
                        "claimed-by-supervisor",
                        "parked inbox drain",
                    );
                    held.push(hail);
                }
                break;
            }
        }
        opening_hails = held.clone();
        turn = held
            .drain(..)
            .map(|hail| {
                record_hail_transition(events, &hail, "submitted-to-harness", "resume turn");
                events.record(
                    "delivery",
                    TraceRecorder::session(channel),
                    None,
                    None,
                    Some(Delivery::NextTurn.as_str()),
                    Some("queued"),
                    Some(&hail.from),
                    Some(&lane.lane),
                    "hail held for next turn",
                );
                hail_text(&hail, &mood)
            })
            .collect::<Vec<_>>()
            .join("\n\n");
    }
}

/// `None` means no verdict yet: a clean turn that has not completed the brief
/// leaves the lane idle rather than inventing a failure to report.
fn completion_verdict(brief_completed: bool, end: &TurnEvent) -> Option<(i32, Option<String>)> {
    if end.is_done() {
        return brief_completed.then_some((0, None));
    }
    if end.retryable() {
        return Some((1, Some(end.detail().to_owned())));
    }
    Some((1, None))
}

/// The lane's parent per the registry. The pane epilogue addresses its result
/// hail the same way, so both rows answer the same wait.
fn registered_parent(dir: &Path, lane: &str) -> Option<String> {
    bus::read_routes(dir).ok()?.get(lane)?.parent.clone()
}

/// The result row body, for a human reading the mailbox. The exit code every
/// caller acts on is the row's typed `rc`, never this text.
fn result_body(lane: &str, exit_code: i32, detail: Option<&str>) -> String {
    match detail {
        Some(detail) => format!("lane {lane} done rc={exit_code} ({detail})"),
        None => format!("lane {lane} done rc={exit_code}"),
    }
}

/// The exit code a result row wears when the process exited 0 but the lane's
/// typed expectations were unmet. The task is incomplete; the process did not
/// fail.
const INCOMPLETE_EXIT: i32 = 4;

/// The unmet typed completion assertions for a lane: one string per failed
/// path, subject, or commit-count bound.
pub struct Unmet(pub Vec<String>);

/// Evaluate the lane's completion expectations against its worktree. `base_sha`
/// bounds the commit range (`None` counts every commit on HEAD); `cwd` is the
/// lane worktree.
pub fn evaluate_expect(
    cwd: &Path,
    base_sha: Option<&str>,
    expect: &boop_store::trail::Expect,
) -> Unmet {
    let mut unmet = Vec::new();
    for path in &expect.paths {
        if !cwd.join(path).exists() {
            unmet.push(format!("missing path {path}"));
        }
    }
    let subjects = commit_subjects(cwd, base_sha);
    for subject in &expect.commit_subjects {
        if !subjects.iter().any(|candidate| candidate == subject) {
            unmet.push(format!("no commit with subject '{subject}'"));
        }
    }
    if let Some(at_least) = expect.commits_at_least {
        if (subjects.len() as u32) < at_least {
            unmet.push(format!(
                "{} commit{}, expected at least {}",
                subjects.len(),
                if subjects.len() == 1 { "" } else { "s" },
                at_least
            ));
        }
    }
    Unmet(unmet)
}

/// The commit subject lines in the lane worktree after `base_sha` (every
/// commit on HEAD when `base_sha` is `None`), capped at 200. A git that cannot
/// answer contributes nothing.
fn commit_subjects(cwd: &Path, base_sha: Option<&str>) -> Vec<String> {
    let range = base_sha.map_or_else(|| "HEAD".to_owned(), |sha| format!("{sha}..HEAD"));
    let output = std::process::Command::new("git")
        .args([
            "-C",
            &cwd.display().to_string(),
            "log",
            "--format=%s",
            "--max-count=200",
            &range,
        ])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .filter(|subject| !subject.trim().is_empty())
        .collect()
}

/// Fold the lane's typed expectations into a result: an unmet assertion turns a
/// clean process exit into rc 4 and lists the unmet items in the detail. A
/// process failure keeps its own rc; the detail still names what was missing.
fn apply_expectations(
    lane: &LaneRun,
    exit_code: i32,
    detail: Option<&str>,
) -> (i32, Option<String>) {
    let Some(expect) = boop_store::trail::read_expect(&lane.lane) else {
        return (exit_code, detail.map(str::to_owned));
    };
    let base_sha = bus::read_routes(&lane.mail_dir).ok().and_then(|routes| {
        routes
            .get(&lane.lane)
            .and_then(|route| route.base_sha.clone())
    });
    let unmet = evaluate_expect(&lane.cwd, base_sha.as_deref(), &expect);
    if unmet.0.is_empty() {
        return (exit_code, detail.map(str::to_owned));
    }
    let detail = format!("incomplete: {}", unmet.0.join("; "));
    (
        if exit_code == 0 {
            INCOMPLETE_EXIT
        } else {
            exit_code
        },
        Some(detail),
    )
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
    let (exit_code, detail) = apply_expectations(lane, exit_code, detail);
    let row = bus::Message {
        id: bus::mint_id(),
        from: lane.lane.clone(),
        to: parent.clone(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: "result".into(),
        reply_to: None,
        body: result_body(&lane.lane, exit_code, detail.as_deref()),
        r#ref: None,
        rc: Some(exit_code),
        detail: detail.clone(),
    };
    match append_row(&lane.mail_dir, &row) {
        Ok(()) => {
            info!(
                lane = lane.lane,
                parent, exit_code, "lane result row written"
            );
            let landed = deliver_outbound(lane, &row);
            println!(
                "[boop] result rc={exit_code} hailed to {parent}: {}",
                landed.unwrap_or_else(|| "held in the mailbox".to_owned())
            );
        }
        Err(error) => {
            error!(lane = lane.lane, parent, error = %error, "lane result row write failed");
            println!("[boop] result row write failed: {error}");
        }
    }
    if exit_code != 0 && !ended_on_parent_death(detail.as_deref()) {
        hail_parent_once(
            lane,
            EXITED_WITHOUT_COMPLETION,
            exit_code as u32,
            detail.as_deref().unwrap_or("no completion reported"),
        );
    }
}

/// A lane killed because its parent died: the one nonzero exit whose parent is
/// gone, so a failure hail addressed to it would reach nobody.
fn ended_on_parent_death(detail: Option<&str>) -> bool {
    detail.is_some_and(|detail| detail.starts_with(boop_store::trail::PARENT_DIED))
}

/// The three actionable transitions a parent is told about, each at most once
/// per lane. The completion row stays the only place an rc is written.
pub const RETRYING: &str = "retrying";
pub const RETRY_BUDGET_EXHAUSTED: &str = "retry_budget_exhausted";
pub const EXITED_WITHOUT_COMPLETION: &str = "exited_without_completion";

/// What the parent needs to act on: which lane, on what model, how far in, why,
/// and the command that reads the rest.
fn failure_body(lane: &LaneRun, kind: &str, attempt: u32, reason: &str) -> String {
    format!(
        "lane {} {kind}: {reason} (attempt {attempt}/{FLAKE_RESUME_CAP}, model {}); read: boop beep lane pane {}",
        lane.lane,
        lane.model.as_deref().unwrap_or("-"),
        lane.lane,
    )
}

/// Whether this lane already sent that kind. The mailbox is the dedup store, so
/// a respawned supervisor never re-sends what a previous run already said.
fn already_hailed(dir: &Path, lane: &str, kind: &str) -> bool {
    let mut rows = Vec::new();
    for path in bus::read_boxes(dir).unwrap_or_default() {
        rows.extend(bus::parse_box(&path));
    }
    rows.iter().any(|row| row.from == lane && row.kind == kind)
}

/// The kind a lane that never opened wears. It is not a turn end, so it
/// carries its own word rather than borrowing `yield`'s.
pub const OPEN_FAILED: &str = "open_failed";

/// Mail the parent one row for a lane whose harness channel never opened.
/// The supervisor loop has not started yet, so nothing else in the lane would
/// report it and the parent would read a silent route as a slow start.
pub fn report_open_failure(lane: &LaneRun, reason: &str) {
    let body = format!(
        "lane {} never opened: {reason} (model {})",
        lane.lane,
        lane.model.as_deref().unwrap_or("-"),
    );
    mail_to_parent_kind(lane, OPEN_FAILED, body, Some(reason));
    record_result(lane, 1, Some(reason));
}

/// The kind every progress row wears. A parent filters one word to see where
/// each of its lanes stopped and what the worktree looked like when it did.
pub const YIELD: &str = "yield";

/// The worktree HEAD as a short sha. A directory git cannot answer for reads
/// `unknown` rather than dropping the field the parent greps for.
fn head_sha(cwd: &Path) -> String {
    std::process::Command::new("git")
        .args([
            "-C",
            &cwd.display().to_string(),
            "rev-parse",
            "--short",
            "HEAD",
        ])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
        .filter(|sha| !sha.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// How many paths `git status --porcelain` lists in the worktree. Anything git
/// cannot answer counts as zero, so the row still reports HEAD.
fn dirty_count(cwd: &Path) -> usize {
    std::process::Command::new("git")
        .args(["-C", &cwd.display().to_string(), "status", "--porcelain"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}

/// The body a parked lane mails: which lane, why the turn ended, where HEAD
/// sits, and how many paths are dirty. One line, four fields, greppable.
fn idle_body(lane: &LaneRun, reason: &str) -> String {
    format!(
        "idle {} turn={reason} head={} dirty={}",
        lane.lane,
        head_sha(&lane.cwd),
        dirty_count(&lane.cwd),
    )
}

/// How often the supervisor asks git where HEAD sits. A 700 ms inbox poll
/// would fork git twice a second for a value that moves at commit speed.
const HEAD_POLL: Duration = Duration::from_secs(5);

/// The lane's own progress watcher. It holds the last sha this supervisor
/// mailed the parent, so the parent hears about a commit whether or not the
/// model ever runs `tell-parent`.
struct HeadWatch {
    last_mailed: Option<String>,
    checked_ms: u64,
}

/// Whether `candidate` descends from `ancestor` in this worktree. A git that
/// cannot answer reports `true`, so an unreadable repo raises no diagnostic.
fn descends_from(cwd: &Path, ancestor: &str, candidate: &str) -> bool {
    std::process::Command::new("git")
        .args([
            "-C",
            &cwd.display().to_string(),
            "merge-base",
            "--is-ancestor",
            ancestor,
            candidate,
        ])
        .status()
        .map(|status| status.success())
        .unwrap_or(true)
}

impl HeadWatch {
    /// Seed from where HEAD sits now, so the first row a parent sees names a
    /// move this lane made rather than the base it started from.
    fn new(cwd: &Path) -> HeadWatch {
        HeadWatch {
            last_mailed: match head_sha(cwd).as_str() {
                "unknown" => None,
                sha => Some(sha.to_owned()),
            },
            checked_ms: 0,
        }
    }

    /// Read HEAD at most once per `HEAD_POLL` and mail the parent what moved.
    /// A descendant is a commit row; anything else names both shas as a
    /// rewind, which is how a yielded sha reset off the branch surfaces.
    fn poll(&mut self, lane: &LaneRun, now_ms: u64) {
        if now_ms.saturating_sub(self.checked_ms) < HEAD_POLL.as_millis() as u64 {
            return;
        }
        self.checked_ms = now_ms;
        let head = head_sha(&lane.cwd);
        if head == "unknown" {
            return;
        }
        let Some(previous) = self.last_mailed.clone() else {
            self.last_mailed = Some(head);
            return;
        };
        if previous == head {
            return;
        }
        self.last_mailed = Some(head.clone());
        if descends_from(&lane.cwd, &previous, &head) {
            let body = format!(
                "commit {} {previous}..{head} dirty={}",
                lane.lane,
                dirty_count(&lane.cwd),
            );
            mail_to_parent_kind(lane, YIELD, body, Some("head advanced"));
            return;
        }
        let body = format!(
            "head {} rewound: {head} does not descend from the last reported {previous}",
            lane.lane,
        );
        mail_to_parent_kind(lane, HEAD_REWOUND, body, Some("head rewound"));
    }
}

/// The kind a rewind wears. A parent that holds a receipt for a sha no longer
/// on the branch reads exactly one row naming both shas.
pub const HEAD_REWOUND: &str = "head_rewound";

/// Resolve the parent and mail one row of `kind`. A parentless lane writes
/// nothing, which is the same silence every other parent path keeps.
fn mail_to_parent_kind(lane: &LaneRun, kind: &str, body: String, detail: Option<&str>) {
    let Some(parent) = registered_parent(&lane.mail_dir, &lane.lane) else {
        debug!(lane = lane.lane, kind, "no registered parent; no row");
        return;
    };
    mail_parent(lane, &parent, kind, body, detail);
}

/// Mail the registered parent one `yield` row for this park. Every park sends
/// its own row: the dedup `hail_parent_once` applies to failure kinds would
/// collapse a whole lane's progress into a single line.
fn yield_to_parent(lane: &LaneRun, reason: &str) {
    mail_to_parent_kind(lane, YIELD, idle_body(lane, reason), Some(reason));
}

/// Append one row from this lane to its parent. `hail_parent_once` and
/// `yield_to_parent` share it, so both leave the same shape in the mailbox.
fn mail_parent(lane: &LaneRun, parent: &str, kind: &str, body: String, detail: Option<&str>) {
    let row = bus::Message {
        id: bus::mint_id(),
        from: lane.lane.clone(),
        to: parent.to_owned(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: kind.to_owned(),
        reply_to: None,
        body,
        r#ref: None,
        rc: None,
        detail: detail.map(str::to_owned),
    };
    match append_row(&lane.mail_dir, &row) {
        Ok(()) => {
            info!(lane = lane.lane, parent, kind, "lane parent row written");
            let landed = deliver_outbound(lane, &row);
            println!(
                "[boop] {kind} hailed to {parent}: {}",
                landed.unwrap_or_else(|| "held in the mailbox".to_owned())
            );
        }
        Err(error) => {
            error!(lane = lane.lane, parent, kind, error = %error, "parent row write failed");
        }
    }
}

/// Push one row this lane wrote down the delivery ladder and record where it
/// landed. Appending alone leaves a row nobody owns: that is how a parent was
/// sent yields it never received. Returns the rung, or `None` when the store
/// or the registry could not be read.
fn deliver_outbound(lane: &LaneRun, row: &bus::Message) -> Option<String> {
    let store = bus::open_store(&lane.mail_dir).ok()?;
    let routes = bus::routes_in(&store).ok()?;
    let registry = boop_harness::registry::Registry::discover();
    match crate::deliver::deliver_hail(&registry, &store, &routes, row) {
        Ok(landing) => {
            info!(
                lane = lane.lane,
                message_id = row.id,
                rung = landing.rung.as_str(),
                "lane outbound row delivered"
            );
            Some(landing.line(&row.id, &row.to, "harness"))
        }
        Err(error) => {
            warn!(lane = lane.lane, message_id = row.id, error = %error, "outbound delivery failed");
            None
        }
    }
}

/// Mail the registered parent one typed failure row. A parentless lane and a
/// repeat of the same kind both write nothing.
fn hail_parent_once(lane: &LaneRun, kind: &str, attempt: u32, reason: &str) {
    let Some(parent) = registered_parent(&lane.mail_dir, &lane.lane) else {
        debug!(
            lane = lane.lane,
            kind, "no registered parent; no failure hail"
        );
        return;
    };
    if already_hailed(&lane.mail_dir, &lane.lane, kind) {
        return;
    }
    let row = bus::Message {
        id: bus::mint_id(),
        from: lane.lane.clone(),
        to: parent.clone(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: kind.to_owned(),
        reply_to: None,
        body: failure_body(lane, kind, attempt, reason),
        r#ref: None,
        rc: None,
        detail: Some(reason.to_owned()),
    };
    match append_row(&lane.mail_dir, &row) {
        Ok(()) => {
            info!(lane = lane.lane, parent, kind, "lane failure hail written");
            println!("[boop] {kind} hailed to {parent}");
        }
        Err(error) => {
            error!(lane = lane.lane, parent, kind, error = %error, "failure hail write failed");
        }
    }
}

/// Append one row to the lane's mailbox, through the same store and the same
/// first transition the `boop beep` verb writes.
fn append_row(dir: &Path, row: &bus::Message) -> std::io::Result<()> {
    bus::append(dir, "bus", row)
        .map_err(|error| std::io::Error::other(format!("append mailbox row: {error}")))
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
    let store = match boop_store::Store::default_path().and_then(boop_store::Store::open) {
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
    if let Err(error) =
        store.attach_trace(&lane.lane, &trace, "lane-run", boop_acp::channel::now_ms())
    {
        warn!(lane = lane.lane, trace, error = %error, "lane trace attachment failed");
    }
    if let Err(error) = store.attach_trace(
        &id,
        &trace,
        "supervisor-conversation",
        boop_acp::channel::now_ms(),
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
    bus::read_routes(dir)
        .ok()
        .and_then(|routes| routes.get(lane)?.session_id.clone())
        .or_else(|| boop_store::trail::read_conversation(lane))
}

/// Write the harness's own conversation id onto the lane's registry route so a
/// later resume finds it without a transcript scan.
fn record_conversation(dir: &Path, lane: &str, conversation: &str) {
    if let Err(error) = boop_store::trail::write_conversation(lane, conversation) {
        warn!(lane, conversation_id = conversation, error = %error, "conversation trail write failed");
    }
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
    let Ok(store) = bus::open_store(dir) else {
        return;
    };
    let ids = [hail.id.clone()];
    if let Err(error) = bus::ack_messages(&store, &ids, &bus::now_iso()) {
        warn!(message_id = hail.id, error = %error, "mailbox ack failed");
    }
}

/// Append one lane-supervisor receipt. A failure to observe the receipt does
/// not undo the live channel operation it accompanies.
fn record_hail_transition(events: &TraceRecorder, hail: &Hail, state: &str, detail: &str) {
    let Some(store) = &events.store else {
        return;
    };
    if let Err(error) = store.record_delivery(
        &hail.id,
        &events.lane,
        None,
        state,
        detail,
        boop_acp::channel::now_ms(),
    ) {
        warn!(lane = events.lane, hail_id = hail.id, state, error = %error, "delivery receipt write failed");
    }
}

/// Ack plus an accepted receipt and store edge naming the tier, so `boop db`
/// answers whether the lane received the hail and how it landed.
fn record_delivery(events: &TraceRecorder, dir: &Path, hail: &Hail, tier: Delivery) {
    ack(dir, hail);
    record_hail_transition(events, hail, "accepted-by-harness", tier.as_str());
    let Some(store) = &events.store else {
        return;
    };
    if let Err(error) = store.add_edge_at(
        &hail.from,
        &events.lane,
        &format!("deliver-{}", tier.as_str()),
        boop_acp::channel::now_ms(),
    ) {
        warn!(lane = events.lane, hail_id = hail.id, delivery = tier.as_str(), error = %error, "delivery edge write failed");
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
            rc: None,
            detail: None,
        }
    }

    // FAIL-PRE-FIX: a respawned supervisor had no route read-back, so every
    // cold restart opened a fresh session with the full brief.
    #[test]
    fn the_idle_shutdown_defaults_to_one_minute_and_zero_disables_it() {
        assert_eq!(parse_idle_shutdown(None), Some(Duration::from_secs(60)));
        assert_eq!(
            parse_idle_shutdown(Some("45")),
            Some(Duration::from_secs(45))
        );
        assert_eq!(parse_idle_shutdown(Some("0")), None);
        assert_eq!(
            parse_idle_shutdown(Some("x")),
            Some(Duration::from_secs(60))
        );
    }

    #[test]
    fn a_supervisor_whose_own_pane_is_gone_ends_the_lane() {
        let dir = tempdir();
        let lane = LaneRun {
            lane: "mine".into(),
            brief: dir.join("brief.md"),
            mail_dir: dir.clone(),
            cwd: dir.clone(),
            model: None,
            resume: None,
        };
        let mut watch = ParentWatch {
            policy: ParentDeathPolicy::Orphan,
            parent: None,
            own_pane: Some("%7".into()),
        };
        let alive = boop_store::testing::FakeMux::available(&["s"]).with_pane("%7", "s");
        assert!(watch.probe(&lane, &alive).is_none());
        let gone = boop_store::testing::FakeMux::available(&["s"]);
        let ended = watch.probe(&lane, &gone).expect("pane gone ends the lane");
        assert_eq!(ended.exit_code, PARENT_DIED_EXIT);
        assert_eq!(ended.detail.as_deref(), Some("pane-gone: %7"));
    }

    #[test]
    fn a_pinned_conversation_round_trips_through_the_registry_route() {
        // HOME is process-wide in this test binary and the trail copy lives
        // under it, so the lane name is unique to this test.
        let dir = tempdir();
        assert_eq!(pinned_conversation(&dir, "pinned-round-trip"), None);
        record_conversation(&dir, "pinned-round-trip", "ses_route_1");
        assert_eq!(
            pinned_conversation(&dir, "pinned-round-trip").as_deref(),
            Some("ses_route_1")
        );
        // The trail copy answers when the route is gone.
        std::fs::remove_file(dir.join("registry.json")).ok();
        assert_eq!(
            pinned_conversation(&dir, "pinned-round-trip").as_deref(),
            Some("ses_route_1")
        );
        assert_eq!(pinned_conversation(&dir, "pinned-other"), None);
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

    // FAIL-PRE-FIX: a 30 s first-signal window killed a healthy child that had
    // not spoken yet. A codex lane on a reasoning model emits nothing until its
    // first tool call, so it died at ~70 s and the retry wrote to dead stdin.
    #[test]
    fn a_quiet_opening_gap_is_not_a_stall() {
        let limit = DEFAULT_STALL_LIMIT;
        assert!(
            !stalled(idle_ms(90_000, 0, None), limit),
            "90 s of opening quiet"
        );
        assert!(stalled(idle_ms(1_801_000, 0, None), limit));
        assert!(!stalled(idle_ms(400_000, 0, Some(399_000)), limit));
        assert!(stalled(idle_ms(2_200_000, 0, Some(399_000)), limit));
    }

    // A turn waiting on a background build past the old 5 min bound is not
    // stalled at the new 30 min default; a genuinely quiet 31 min turn is.
    #[test]
    fn a_background_wait_survives_the_old_five_minute_bound() {
        let limit = DEFAULT_STALL_LIMIT;
        assert!(
            !stalled(idle_ms(20 * 60_000, 0, None), limit),
            "20 min quiet, still alive"
        );
        assert!(stalled(idle_ms(31 * 60_000, 0, None), limit));
    }

    #[test]
    fn the_stall_limit_config_key_overrides_the_default() {
        assert_eq!(parse_stall_limit(None), DEFAULT_STALL_LIMIT);
        assert_eq!(parse_stall_limit(Some("garbage")), DEFAULT_STALL_LIMIT);
        assert_eq!(parse_stall_limit(Some("120")), Duration::from_secs(120));
    }

    /// Activity before this turn opened is not this turn's; the clock then runs
    /// from the turn's own start.
    #[test]
    fn activity_from_an_earlier_turn_does_not_reset_the_clock() {
        assert_eq!(idle_ms(90_000, 60_000, None), 30_000);
        assert_eq!(idle_ms(90_000, 60_000, Some(80_000)), 10_000);
    }

    fn hail(kind: &str) -> Hail {
        Hail {
            id: "m1".into(),
            from: "coordinator".into(),
            kind: kind.into(),
            body: "stop and write /tmp/x".into(),
        }
    }

    #[test]
    fn hail_text_names_the_id_and_the_sender() {
        let text = hail_text(&hail("hail"), boop_store::ident::DEFAULT_MOOD_TEMPLATE);
        assert_eq!(text, "[boop m1 from coordinator] stop and write /tmp/x");
    }

    /// The lane pane is one of the three delivery paths a mood reaches; the
    /// kind is a placeholder like the other three.
    #[test]
    fn a_hail_into_a_lane_pane_renders_through_the_lane_mood() {
        let text = hail_text(&hail("request"), "{kind} {from} -> {id}\n{body}");
        assert_eq!(text, "request coordinator -> m1\nstop and write /tmp/x");
    }

    /// A body naming a placeholder is payload, never a second pass: the
    /// substitutions never read their own output.
    #[test]
    fn a_body_that_names_a_placeholder_is_left_alone() {
        let mut row = hail("hail");
        row.body = "{from} is not substituted".into();
        assert_eq!(
            hail_text(&row, boop_store::ident::DEFAULT_MOOD_TEMPLATE),
            "[boop m1 from coordinator] {from} is not substituted"
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

    fn rows_of_kind(dir: &Path, kind: &str) -> Vec<bus::Message> {
        let mut rows = Vec::new();
        for path in bus::read_boxes(dir).unwrap_or_default() {
            rows.extend(bus::parse_box(&path));
        }
        rows.into_iter().filter(|row| row.kind == kind).collect()
    }

    /// A worktree with one commit, so HEAD ancestry has something to answer.
    fn git_repo(dir: &Path) -> String {
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(["-C", &dir.display().to_string()])
                .args(args)
                .output()
                .unwrap()
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "lane@boop"]);
        git(&["config", "user.name", "lane"]);
        std::fs::write(dir.join("one.txt"), "one\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "one"]);
        head_sha(dir)
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
        assert_eq!(
            resume_text(false, Some("ses_x".into()), "do the work"),
            "do the work"
        );
        assert_eq!(
            resume_text(true, Some("ses_x".into()), "do the work"),
            RESUME_NUDGE
        );
    }

    /// A clean turn parks `run`, so a caller reads the shared handle from
    /// another thread and polls; `run` is never joined back.
    fn wait_for(mut ready: impl FnMut() -> bool, timeout: Duration) {
        let start = std::time::Instant::now();
        while !ready() {
            assert!(start.elapsed() < timeout, "condition never became true");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[derive(Clone, Default)]
    struct BriefFlakesThenCompletesChannel {
        turns: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl LaneChannel for BriefFlakesThenCompletesChannel {
        fn conversation_id(&self) -> Option<String> {
            Some("fresh-thread-id".to_owned())
        }

        fn start_turn(&mut self, text: &str) -> Result<()> {
            self.turns.lock().unwrap().push(text.to_owned());
            Ok(())
        }

        fn steer(&mut self, _text: &str) -> Result<Delivery> {
            Ok(Delivery::MidTurn)
        }

        fn next_event(&mut self, _timeout: Duration) -> Result<Option<TurnEvent>> {
            Ok(Some(if self.turns.lock().unwrap().len() == 1 {
                TurnEvent::flaked("aborted stream")
            } else {
                TurnEvent::ok("completed")
            }))
        }

        fn close(&mut self) -> Result<()> {
            Ok(())
        }
    }

    // FAIL-PRE-FIX: the first provider flake saw a conversation id discovered
    // before the brief turn, so the retry received RESUME_NUDGE and the brief
    // was absent from the only turn that could act on it.
    #[test]
    fn a_flaked_brief_turn_is_refed_even_when_the_channel_has_an_id() {
        let dir = tempdir();
        let lane = parented_lane(&dir, "mine", "coordinator");
        let mut channel = BriefFlakesThenCompletesChannel::default();
        let turns = channel.turns.clone();
        std::thread::spawn(move || {
            let _ = run(lane, &mut channel);
        });

        wait_for(|| result_rows(&dir).len() == 1, Duration::from_secs(5));
        assert_eq!(*turns.lock().unwrap(), ["do the work\n", "do the work\n"]);
        assert_eq!(result_rows(&dir)[0].body, "lane mine done rc=0");
    }

    #[derive(Clone, Default)]
    struct FreshIdentifiedChannel {
        turns: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        brief: std::sync::Arc<std::sync::Mutex<Option<String>>>,
    }

    impl LaneChannel for FreshIdentifiedChannel {
        fn conversation_id(&self) -> Option<String> {
            Some("fresh-thread-id".to_owned())
        }

        fn set_brief(&mut self, brief: &str) {
            *self.brief.lock().unwrap() = Some(brief.to_owned());
        }

        fn start_turn(&mut self, text: &str) -> Result<()> {
            self.turns.lock().unwrap().push(text.to_owned());
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

    /// A channel that panics the moment the first turn opens. The panic path
    /// out of `run`, which no `Result` arm ever sees.
    struct PanicChannel;

    impl LaneChannel for PanicChannel {
        fn conversation_id(&self) -> Option<String> {
            None
        }
        fn start_turn(&mut self, _text: &str) -> Result<()> {
            panic!("harness stream vanished mid-frame")
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

    // FAIL-PRE-FIX: Codex app-server returns a thread id during channel open,
    // before any turn contains the brief. The supervisor used the presence of
    // that id as resume evidence and sent RESUME_NUDGE as the first turn.
    #[test]
    fn a_fresh_identified_channel_receives_the_full_brief() {
        let dir = tempdir();
        let lane = parented_lane(&dir, "mine", "coordinator");
        let mut channel = FreshIdentifiedChannel::default();
        let turns = channel.turns.clone();
        std::thread::spawn(move || {
            let _ = run(lane, &mut channel);
        });

        wait_for(|| result_rows(&dir).len() == 1, Duration::from_secs(5));
        assert_eq!(*turns.lock().unwrap(), ["do the work\n"]);
    }

    // SABOTAGE RECEIPT: restore `end.is_done() => Some((0, ..))` unconditionally
    // here, or a nudge-only stop reads as success before the brief finishes.
    #[test]
    fn a_nudge_only_completion_has_no_verdict_yet() {
        assert_eq!(
            completion_verdict(false, &TurnEvent::ok("opencode_stop")),
            None
        );
    }

    #[test]
    fn a_brief_completed_clean_stop_is_verdict_zero() {
        assert_eq!(
            completion_verdict(true, &TurnEvent::ok("opencode_stop")),
            Some((0, None))
        );
    }

    #[test]
    fn a_failed_turn_always_carries_a_verdict() {
        assert_eq!(
            completion_verdict(false, &TurnEvent::failed("boom")),
            Some((1, None))
        );
        assert_eq!(
            completion_verdict(true, &TurnEvent::flaked("dropped")),
            Some((1, Some("dropped".to_owned())))
        );
    }

    #[test]
    fn an_explicit_resume_receives_the_resume_nudge() {
        let dir = tempdir();
        let mut lane = parented_lane(&dir, "mine", "coordinator");
        lane.resume = Some("existing-thread-id".to_owned());
        let mut channel = FreshIdentifiedChannel::default();
        let turns = channel.turns.clone();
        std::thread::spawn(move || {
            let _ = run(lane, &mut channel);
        });

        wait_for(|| result_rows(&dir).len() == 1, Duration::from_secs(5));
        assert_eq!(*turns.lock().unwrap(), [RESUME_NUDGE]);
        assert_eq!(result_rows(&dir)[0].body, "lane mine done rc=0");
    }

    // FAIL-PRE-FIX: a panic inside the supervisor unwound straight past
    // `record_result`, so the waiter sat until its timeout with no row.
    // SABOTAGE RECEIPT: call `supervise(&lane, channel)` directly instead of
    // wrapping it in `catch_unwind` and this test aborts on the escaped panic.
    #[test]
    fn a_supervisor_panic_still_writes_the_lane_s_result_row() {
        let dir = tempdir();
        let lane = parented_lane(&dir, "mine", "coordinator");
        let error = run(lane, &mut PanicChannel).unwrap_err().to_string();
        assert!(error.contains("supervisor panic"), "{error}");
        let rows = result_rows(&dir);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].body,
            "lane mine done rc=101 (panic: harness stream vanished mid-frame)"
        );
    }

    // FAIL-PRE-FIX: a killed pane sent SIGHUP and the default disposition ended
    // the supervisor with no row at all; `boop wait` then hung for its full
    // timeout. SABOTAGE RECEIPT: drop `SIGTERM` from `TRAILED_SIGNALS` and the
    // raise below terminates the test binary instead of being caught.
    #[test]
    fn a_signalled_supervisor_writes_a_typed_result_row() {
        let dir = tempdir();
        let lane = parented_lane(&dir, "mine", "coordinator");
        let mut signals =
            signal_hook::iterator::Signals::new(TRAILED_SIGNALS).expect("register the signals");
        signal_hook::low_level::raise(signal_hook::consts::SIGTERM).unwrap();
        let caught = signals
            .forever()
            .next()
            .expect("the raised signal reaches the iterator");
        assert_eq!(caught, signal_hook::consts::SIGTERM);
        assert_eq!(signal_exit(&lane, caught), 143);
        let rows = result_rows(&dir);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].body, "lane mine done rc=143 (killed by SIGTERM)");
    }

    #[derive(Clone, Default)]
    struct ParksThenWakesChannel {
        turns: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        closed: std::sync::Arc<std::sync::Mutex<bool>>,
    }

    impl LaneChannel for ParksThenWakesChannel {
        fn conversation_id(&self) -> Option<String> {
            Some("resident-thread-id".to_owned())
        }

        fn start_turn(&mut self, text: &str) -> Result<()> {
            self.turns.lock().unwrap().push(text.to_owned());
            Ok(())
        }

        fn steer(&mut self, _text: &str) -> Result<Delivery> {
            Ok(Delivery::MidTurn)
        }

        fn next_event(&mut self, _timeout: Duration) -> Result<Option<TurnEvent>> {
            Ok(Some(TurnEvent::ok("completed")))
        }

        fn close(&mut self) -> Result<()> {
            *self.closed.lock().unwrap() = true;
            Ok(())
        }
    }

    // FAIL-PRE-FIX: a turn with no pending hail closed the channel and
    // returned `Ended`, so the pane exited and a later hail was lost with it.
    #[test]
    fn a_turn_with_no_pending_hail_parks_instead_of_ending() {
        let dir = tempdir();
        let lane = parented_lane(&dir, "mine", "coordinator");
        let mut channel = ParksThenWakesChannel::default();
        let turns = channel.turns.clone();
        let closed = channel.closed.clone();
        std::thread::spawn(move || {
            let _ = run(lane, &mut channel);
        });

        wait_for(|| result_rows(&dir).len() == 1, Duration::from_secs(5));
        assert_eq!(*turns.lock().unwrap(), ["do the work\n"]);
        assert!(!*closed.lock().unwrap(), "a parked lane closed its channel");

        append_row(&dir, &message("wake", "mine", "hail")).unwrap();
        wait_for(|| turns.lock().unwrap().len() == 2, Duration::from_secs(5));
        assert!(turns.lock().unwrap()[1].contains("body of wake"));
        assert!(
            !*closed.lock().unwrap(),
            "waking a parked lane closed its channel"
        );
        assert_eq!(
            result_rows(&dir).len(),
            1,
            "a follow-up turn writes no second done row"
        );
    }

    // FAIL-PRE-FIX: the brief reached the channel only as turn one's text, so a
    // lane opened on the resume nudge left a respawned TUI window with the nudge
    // to re-feed and the brief lost. Sabotage receipt: dropping the
    // `channel.set_brief` call FAILED this on `brief: None`.
    #[test]
    fn the_brief_reaches_the_channel_before_a_resume_nudge_opens_the_lane() {
        let dir = tempdir();
        let mut lane = parented_lane(&dir, "mine", "coordinator");
        lane.resume = Some("existing-thread-id".to_owned());
        let mut channel = FreshIdentifiedChannel::default();
        let turns = channel.turns.clone();
        let brief = channel.brief.clone();
        std::thread::spawn(move || {
            let _ = run(lane, &mut channel);
        });

        wait_for(|| result_rows(&dir).len() == 1, Duration::from_secs(5));
        assert_eq!(*turns.lock().unwrap(), [RESUME_NUDGE]);
        assert_eq!(brief.lock().unwrap().as_deref(), Some("do the work\n"));
    }

    // FAIL-PRE-FIX: an idle park printed one line to its own pane and nothing
    // else, so a parent heard from a working lane exactly once, at exit.
    #[test]
    fn an_idle_park_mails_the_parent_one_yield_row() {
        let dir = tempdir();
        let lane = parented_lane(&dir, "mine", "coordinator");
        let mut channel = ParksThenWakesChannel::default();
        let turns = channel.turns.clone();
        std::thread::spawn(move || {
            let _ = run(lane, &mut channel);
        });

        wait_for(
            || rows_of_kind(&dir, "yield").len() == 1,
            Duration::from_secs(5),
        );
        let parked = rows_of_kind(&dir, "yield");
        assert_eq!(parked.len(), 1, "one yield row per park");
        assert_eq!(parked[0].from, "mine");
        assert_eq!(parked[0].to, "coordinator");
        assert!(
            parked[0].body.starts_with("idle mine turn=completed head="),
            "body: {}",
            parked[0].body
        );
        assert!(
            parked[0].body.contains(" dirty="),
            "body: {}",
            parked[0].body
        );

        append_row(&dir, &message("wake", "mine", "hail")).unwrap();
        wait_for(|| turns.lock().unwrap().len() == 2, Duration::from_secs(5));
        wait_for(
            || rows_of_kind(&dir, "yield").len() == 2,
            Duration::from_secs(5),
        );
        assert_eq!(
            rows_of_kind(&dir, "yield").len(),
            2,
            "the second park mails its own row"
        );
    }

    /// RECEIPT (Item 0). A parentless lane parks with no row to write, and the
    /// park still happens: reporting never gates the lane's own progress.
    #[test]
    fn a_parentless_park_mails_nothing() {
        let dir = tempdir();
        std::fs::write(
            dir.join("registry.json"),
            serde_json::json!({ "mine": { "kind": "lane" } }).to_string(),
        )
        .unwrap();
        let brief = dir.join("brief.md");
        std::fs::write(&brief, "do the work\n").unwrap();
        let lane = LaneRun {
            lane: "mine".into(),
            brief,
            mail_dir: dir.clone(),
            cwd: dir.clone(),
            model: None,
            resume: None,
        };
        yield_to_parent(&lane, "completed");
        assert!(rows_of_kind(&dir, "yield").is_empty());
    }

    /// RECEIPT (Item 0). A worktree git cannot read still reports a body with
    /// both fields, so the parent's grep never loses a column.
    #[test]
    fn an_unreadable_worktree_still_names_head_and_dirty() {
        let dir = tempdir();
        let lane = parented_lane(&dir, "mine", "coordinator");
        assert_eq!(
            idle_body(&lane, "stalled"),
            "idle mine turn=stalled head=unknown dirty=0"
        );
    }

    // FAIL-PRE-FIX: the supervisor never read HEAD, so a lane that committed
    // four times without yielding left its parent with nothing to read.
    #[test]
    fn a_commit_mails_the_parent_the_sha_range() {
        let dir = tempdir();
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let first = git_repo(&work);
        let mut lane = parented_lane(&dir, "mine", "coordinator");
        lane.cwd = work.clone();
        let mut watch = HeadWatch::new(&lane.cwd);
        assert_eq!(watch.last_mailed.as_deref(), Some(first.as_str()));

        std::fs::write(work.join("two.txt"), "two\n").unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(["-C", &work.display().to_string()])
                .args(args)
                .output()
                .unwrap()
        };
        git(&["add", "-A"]);
        git(&["commit", "-qm", "two"]);
        let second = head_sha(&work);

        watch.poll(&lane, HEAD_POLL.as_millis() as u64 + 1);
        let rows = rows_of_kind(&dir, "yield");
        assert_eq!(rows.len(), 1, "one commit row");
        assert_eq!(
            rows[0].body,
            format!("commit mine {first}..{second} dirty=0")
        );
    }

    // FAIL-PRE-FIX: a lane that reset away a sha it had already reported left
    // the parent holding a receipt for a commit no longer on the branch.
    #[test]
    fn a_head_that_does_not_descend_names_both_shas() {
        let dir = tempdir();
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let first = git_repo(&work);
        let mut lane = parented_lane(&dir, "mine", "coordinator");
        lane.cwd = work.clone();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(["-C", &work.display().to_string()])
                .args(args)
                .output()
                .unwrap()
        };
        std::fs::write(work.join("two.txt"), "two\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "two"]);
        let reported = head_sha(&work);
        let mut watch = HeadWatch::new(&lane.cwd);
        assert_eq!(watch.last_mailed.as_deref(), Some(reported.as_str()));

        git(&["reset", "-q", "--hard", "HEAD~1"]);
        watch.poll(&lane, HEAD_POLL.as_millis() as u64 + 1);
        let rows = rows_of_kind(&dir, HEAD_REWOUND);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].body.contains(&reported), "body: {}", rows[0].body);
        assert!(rows[0].body.contains(&first), "body: {}", rows[0].body);
        assert!(descends_from(&work, &first, &reported));
        assert!(!descends_from(&work, &reported, &first));
    }

    // FAIL-PRE-FIX: a lane whose model spelling the harness rejected died
    // before the supervisor existed, so the parent saw a registered route,
    // no rows at all, and no way to tell a dead lane from a slow start.
    #[test]
    fn a_lane_that_never_opened_mails_the_parent_and_writes_its_result() {
        let dir = tempdir();
        let lane = parented_lane(&dir, "mine", "coordinator");
        report_open_failure(&lane, "acp handshake failed: Invalid params");

        let opened = rows_of_kind(&dir, OPEN_FAILED);
        assert_eq!(opened.len(), 1, "one row per failed open");
        assert_eq!(opened[0].to, "coordinator");
        assert!(
            opened[0]
                .body
                .starts_with("lane mine never opened: acp handshake failed"),
            "body: {}",
            opened[0].body
        );
        let results = result_rows(&dir);
        assert_eq!(results.len(), 1, "the waiter still gets its rc");
        assert_eq!(results[0].rc, Some(1));
    }

    /// A repo with one commit on top of its base, subject `docs: foo` and a
    /// file `plans/x.md`. Returns the worktree path and the base sha.
    fn repo_after_base(dir: &Path) -> (PathBuf, String) {
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let base = git_repo(&work);
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(["-C", &work.display().to_string()])
                .args(args)
                .output()
                .unwrap()
        };
        std::fs::create_dir_all(work.join("plans")).unwrap();
        std::fs::write(work.join("plans/x.md"), "x\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "docs: foo"]);
        (work, base)
    }

    #[test]
    fn expectations_all_met_is_empty_unmet() {
        let dir = tempdir();
        let (work, base) = repo_after_base(&dir);
        let expect = boop_store::trail::Expect {
            paths: vec!["plans/x.md".to_owned()],
            commit_subjects: vec!["docs: foo".to_owned()],
            commits_at_least: Some(1),
        };
        assert_eq!(
            evaluate_expect(&work, Some(&base), &expect).0,
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_missing_path_is_named() {
        let dir = tempdir();
        let (work, base) = repo_after_base(&dir);
        let expect = boop_store::trail::Expect {
            paths: vec!["plans/nope.md".to_owned()],
            ..Default::default()
        };
        assert_eq!(
            evaluate_expect(&work, Some(&base), &expect).0,
            vec!["missing path plans/nope.md".to_owned()]
        );
    }

    #[test]
    fn a_wrong_subject_is_named() {
        let dir = tempdir();
        let (work, base) = repo_after_base(&dir);
        let expect = boop_store::trail::Expect {
            commit_subjects: vec!["docs: bar".to_owned()],
            ..Default::default()
        };
        assert_eq!(
            evaluate_expect(&work, Some(&base), &expect).0,
            vec!["no commit with subject 'docs: bar'".to_owned()]
        );
    }

    #[test]
    fn too_few_commits_is_named() {
        let dir = tempdir();
        let (work, base) = repo_after_base(&dir);
        let expect = boop_store::trail::Expect {
            commits_at_least: Some(2),
            ..Default::default()
        };
        assert_eq!(
            evaluate_expect(&work, Some(&base), &expect).0,
            vec!["1 commit, expected at least 2".to_owned()]
        );
    }

    #[test]
    fn an_unmet_expectation_turns_exit_zero_into_rc_four() {
        let dir = tempdir();
        let lane_name = format!(
            "expect-lane-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        );
        let work = dir.join("work");
        std::fs::create_dir_all(&work).unwrap();
        let _ = git_repo(&work);
        let mut lane = parented_lane(&dir, &lane_name, "coordinator");
        lane.cwd = work.clone();
        boop_store::trail::write_expect(
            &lane_name,
            &boop_store::trail::Expect {
                paths: vec!["plans/x.md".to_owned()],
                ..Default::default()
            },
        )
        .unwrap();
        record_result(&lane, 0, None);
        let rows = result_rows(&dir);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].rc, Some(4));
        assert_eq!(
            rows[0].body,
            format!("lane {lane_name} done rc=4 (incomplete: missing path plans/x.md)")
        );
        assert_eq!(
            rows[0].detail.as_deref(),
            Some("incomplete: missing path plans/x.md")
        );
    }

    /// Every test root, and the one store every test in this binary writes.
    /// `TraceRecorder::new` and `mood_template` open `Store::default_path()`,
    /// so without this pin a supervisor test for lane `mine` writes its trace
    /// events into `~/.agent/boop.db` (boop-fixture-lanes-in-live-db: 9150
    /// rows measured 2026-08-25).
    fn tempdir() -> PathBuf {
        static PIN: std::sync::Once = std::sync::Once::new();
        let root = std::env::temp_dir().join(format!("boop-supervise-{}", std::process::id()));
        PIN.call_once(|| {
            std::fs::create_dir_all(root.join("home")).unwrap();
            std::env::set_var("HOME", root.join("home"));
            std::env::set_var("BOOP_DB", root.join("boop.db"));
        });
        let dir = root.join(format!("{:?}", std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
