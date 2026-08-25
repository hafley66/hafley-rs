use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

use boop::bus::Route;
use boop::harness::{HarnessId, VariantSupport};
use boop::mailwait::Watch;
use boop::proc::ProcReader;
use boop::registry::Registry;
use boop::{bus, config, identity, lane, mailwait, proc, tmux};
use tracing::{error, info, warn};

use crate::cli::db::{resolve_harness, run_harnesses};
use crate::cli::debug::default_preset_for_harness;
use crate::cli::mail::{all_messages, run_list};
use crate::cli::me::run_adopt;
use crate::cli::{append_ack, append_message, line, mail_dir, pad, route_to_json, write_route};
use crate::{AgentCmd, BeepCmd, HarnessCmd, LaneCmd, LaneMessageCmd, MessageCmd, PstreeFormat};

// ---------------------------------------------------------------------------
// dispatch (layer 1 + bus)
// ---------------------------------------------------------------------------

pub(crate) struct DispatchArgs {
    pub(crate) to: String,
    pub(crate) cwd: String,
    pub(crate) cmd: String,
    pub(crate) from: Option<String>,
    pub(crate) harness: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) model: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) tmux: Option<String>,
    pub(crate) socket: Option<String>,
    pub(crate) body: Option<String>,
    pub(crate) r#ref: Option<String>,
    pub(crate) mail_dir: Option<PathBuf>,
    pub(crate) resolve_wait: u64,
    pub(crate) main_tree: bool,
    pub(crate) base_sha: Option<String>,
    /// Reasoning effort from the preset row, spelled as the harness's own
    /// config flag rather than an `@suffix` on the model.
    pub(crate) effort: Option<String>,
    /// opencode reasoning-effort variant, threaded from `lane create`.
    pub(crate) variant: Option<String>,
    /// The executable the harness runs as, threaded from `lane create`.
    pub(crate) bin: Option<String>,
    /// Overrides the branch name derived from `tmux`/`to`; `lane create`
    /// sets this from its own `--branch` flag.
    pub(crate) branch: Option<String>,
    /// The worktree to create; `None` spawns in `cwd` (`main_tree` decides
    /// whether that's a fast-forward check or a plain directory).
    pub(crate) worktree_dir: Option<PathBuf>,
    /// The lane that summoned this one; written to the route's `parent`.
    pub(crate) parent: Option<String>,
    /// What the lane is running toward; written to the route and dispatch mail.
    pub(crate) goal: Option<String>,
    /// Shell appended after the harness command; `lane create --parent` and
    /// foreground `lane create --wait` compose the completion hail here.
    pub(crate) on_exit: Option<String>,
    /// Run the repo's `boop-start` recipe in a new worktree before spawning.
    pub(crate) warm_start: bool,
}

pub(crate) fn run_dispatch(registry: &Registry, args: DispatchArgs) -> Result<()> {
    let adapter = resolve_dispatch_harness(registry, args.harness.as_deref())?;
    let harness_id = adapter.id();
    info!(
        lane = args.to,
        harness = harness_id.as_str(),
        model = args.model.as_deref().unwrap_or_default(),
        cwd = args.cwd,
        tmux_target = args.tmux.as_deref().unwrap_or_default(),
        "lane dispatch starting"
    );
    let branch = args
        .branch
        .clone()
        .unwrap_or_else(|| args.tmux.clone().unwrap_or_else(|| args.to.clone()));
    let base_sha = match &args.base_sha {
        Some(sha) => sha.clone(),
        None => git_head(&args.cwd)?.unwrap_or_else(|| "HEAD".into()),
    };
    let dir = mail_dir(args.mail_dir.as_deref())?;
    let mut body = args.body.clone().unwrap_or_else(|| args.cmd.clone());
    // A dispatch's goal rides the route's `goal` field; embed it in the mail
    // row body too so history states the goal without a registry lookup.
    if let Some(goal) = &args.goal {
        body = format!("{body}\n[goal] {goal}");
    }

    let message = bus::Message {
        id: bus::mint_id(),
        from: args.from.clone().unwrap_or_else(|| "coordinator".into()),
        to: args.to.clone(),
        from_timestamp: bus::now_iso(),
        to_timestamp: None,
        kind: "dispatch".into(),
        reply_to: None,
        body,
        r#ref: args.r#ref.clone(),
        rc: None,
        detail: None,
    };

    let spec = boop::harness::SpawnSpec {
        harness: harness_id,
        branch,
        base_sha,
        main_tree: args.main_tree,
        setup: Vec::new(),
        prompt: args.cmd.clone(),
        resume_session: args.session_id.clone(),
        socket: args.socket.clone(),
        worktree_dir: args.worktree_dir.clone(),
        repo: std::path::PathBuf::from(&args.cwd),
        env_stamp: Some(spawn_env_stamp(
            &args.to,
            harness_id.as_str(),
            args.parent.as_deref(),
        )),
        model: args.model.clone(),
        effort: args.effort.clone(),
        variant: args.variant.clone(),
        bin: args.bin.clone(),
        on_exit: args.on_exit.clone(),
        tmux: args.tmux.clone(),
        lane: args.to.clone(),
        mail_dir: dir.clone(),
        warm_start: args.warm_start,
    };
    let session = adapter.spawn(&spec)?;

    // The route's cwd is where the harness actually runs (the worktree when
    // one was made): session-id resolution joins opencode.db on directory.
    let route = Route {
        kind: "lane".into(),
        harness: Some(harness_id),
        tmux: session.tmux.clone(),
        cwd: session.cwd.clone().or_else(|| Some(args.cwd.clone())),
        model: args.model.clone(),
        mode: args.mode.clone(),
        session_id: args.session_id.clone(),
        source_path: None,
        parent: args.parent.clone(),
        goal: args.goal.clone(),
        registered_at: Some(bus::now_iso()),
        base_sha: Some(spec.base_sha.clone()),
        worktree_dir: args
            .worktree_dir
            .clone()
            .map(|dir| dir.display().to_string()),
        app_server_socket: None,
    };
    write_route(&dir, &args.to, route)?;
    append_message(&dir, &message)?;
    info!(
        lane = args.to,
        harness = adapter.id().as_str(),
        tmux_target = session.tmux.as_deref().unwrap_or_default(),
        conversation_id = session.session_id,
        conversation_id_kind = "spawn_handle",
        "lane dispatch registered"
    );
    println!(
        "dispatched {} -> {} (tmux {})",
        message.id,
        args.to,
        session.tmux.as_deref().unwrap_or("-")
    );
    std::thread::sleep(std::time::Duration::from_secs(args.resolve_wait));
    Ok(())
}

/// The environment a spawn's command carries: a UTF-8 locale, then the child's
/// own identity. The pane's inherited locale is the tmux server's, not a shell's.
pub(crate) fn spawn_env_stamp(
    lane_id: &str,
    harness_id: &str,
    parent_lane: Option<&str>,
) -> String {
    format!(
        "{} {}",
        lane::locale_stamp(),
        identity::child_stamp(lane_id, lane_id, harness_id, parent_lane)
    )
}

/// The registered harness adapter for a dispatched `--harness`. A named
/// harness must resolve exactly; an unnamed one takes the first registered
/// adapter. A named harness resolving to a different harness is a capability
/// lie, so an unregistered name is a hard error that lists the registered set.
pub(crate) fn resolve_dispatch_harness<'a>(
    registry: &'a Registry,
    id: Option<&str>,
) -> Result<&'a dyn boop::harness::Harness> {
    let Some(id) = id else {
        return registry
            .all()
            .first()
            .map(|boxed| boxed.as_ref())
            .ok_or_else(|| anyhow::anyhow!("no harness registered"));
    };
    match registry.by_name(id) {
        Some(adapter) => Ok(adapter),
        None => {
            let registered = registry
                .all()
                .iter()
                .map(|harness| harness.id().as_str())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!("unregistered harness `{id}`; registered harnesses: {registered}")
        }
    }
}

/// The registered harness adapter for a `--harness` filter, or the first
/// registered one when the id is absent.
pub(crate) fn harness_by_id<'a>(
    registry: &'a Registry,
    id: &str,
) -> Result<&'a dyn boop::harness::Harness> {
    registry
        .by_name(id)
        .or_else(|| registry.all().first().map(|b| b.as_ref()))
        .ok_or_else(|| anyhow::anyhow!("no harness registered"))
}

pub(crate) fn git_head(repo: &str) -> Result<Option<String>> {
    let output = std::process::Command::new("git")
        .args(["-C", repo, "rev-parse", "HEAD"])
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_owned(),
    ))
}

// ---------------------------------------------------------------------------
// resolve
// ---------------------------------------------------------------------------

pub(crate) fn run_resolve(to: &str, mail_dir_arg: Option<&Path>) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let route = match routes.get(to) {
        Some(route) => route,
        None => {
            println!("unresolved {to}: no registry route");
            return Ok(());
        }
    };
    if route.session_id.is_some() {
        println!(
            "resolved {to} -> {} (self-reported)",
            route.session_id.as_deref().unwrap()
        );
        return Ok(());
    }
    let harness = route.harness.map_or("-", HarnessId::as_str);
    let Some(cwd) = route.cwd.as_deref() else {
        println!("unresolved {to}: no cwd in registry route");
        return Ok(());
    };
    match resolve_harness_binary(harness, cwd) {
        Some(session_id) => {
            let mut updated = route.clone();
            updated.session_id = Some(session_id.clone());
            println!("resolved {to} -> {session_id}");
            let path = dir.join("registry.json");
            bus::cas_update_json(&path, |current| {
                current.insert(to.to_owned(), route_to_json(&updated));
                Ok(())
            })?;
            Ok(())
        }
        None => {
            println!("unresolved {to}: no {harness} session for {cwd} yet");
            Ok(())
        }
    }
}

/// Resolve via the instant-harness binary when it exists (the same binary
/// `bus` shells out to); `None` when the binary is absent or finds nothing.
pub(crate) fn resolve_harness_binary(harness: &str, cwd: &str) -> Option<String> {
    let root = dirs::home_dir()?.join("projects/instant");
    let candidates = [
        root.join("src-tauri/target/debug/instant-harness"),
        root.join("src-tauri/target/release/instant-harness"),
    ];
    let binary = candidates.iter().find(|path| path.exists())?;
    let output = Command::new(binary)
        .args(["resolve", "--harness", harness, "--cwd", cwd])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_session_id(&String::from_utf8_lossy(&output.stdout))
}

pub(crate) fn parse_session_id(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    value
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// Drive one lane to completion inside its pane, then exit with the harness's
/// own code so the pane re-raises a true rc.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_lane_supervisor(
    registry: &Registry,
    lane: &str,
    harness_id: &str,
    brief: &Path,
    model: Option<&str>,
    effort: Option<&str>,
    resume: Option<&str>,
    variant: Option<&str>,
    bin: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    info!(
        lane,
        harness = harness_id,
        model = model.unwrap_or_default(),
        effort = effort.unwrap_or_default(),
        cwd = %std::env::current_dir().unwrap_or_default().display(),
        resume = resume.unwrap_or_default(),
        variant = variant.unwrap_or_default(),
        bin = bin.unwrap_or_default(),
        "lane supervisor starting"
    );
    let adapter = harness_by_id(registry, harness_id)?;
    let dir = mail_dir(mail_dir_arg)?;
    let cwd = std::env::current_dir().context("read the current directory")?;
    // A respawned lane continues its pinned conversation instead of cold-
    // starting a new one with the full brief.
    let resume = resume
        .map(str::to_owned)
        .or_else(|| boop::supervise::pinned_conversation(&dir, lane));
    let resume = resume.as_deref();
    let spec = boop::channel::ChannelSpec {
        effort: effort.map(str::to_owned),
        model: model.map(str::to_owned),
        cwd: cwd.clone(),
        resume: resume.map(str::to_owned),
        lane: Some(lane.to_owned()),
        executable: bin.map(str::to_owned),
    };
    let run = boop::supervise::LaneRun {
        lane: lane.to_owned(),
        // The warm-up's outcome and the setup sentence lead the first turn.
        brief: boop::lane::brief_with_preamble(&dir, lane, brief),
        mail_dir: dir,
        cwd,
        model: model.map(str::to_owned),
        resume: resume.map(str::to_owned),
    };
    // A handshake that fails here happens before the supervisor exists, so
    // nothing else would tell the parent this lane never opened. A rejected
    // model spelling looked exactly like a lane still starting up.
    let mut channel = match adapter.open_channel(&spec) {
        Ok(channel) => channel,
        Err(error) => {
            error!(lane, harness = harness_id, error = %error, "lane channel open failed");
            boop::supervise::report_open_failure(&run, &error.to_string());
            return Err(error);
        }
    };
    // Process-global, so it is armed here and not inside the library call.
    boop::supervise::arm_signal_trail(&run);
    let code = boop::supervise::run(run, channel.as_mut()).inspect_err(|error| {
        error!(lane, harness = harness_id, error = %error, "lane supervisor failed");
    })?;
    info!(
        lane,
        harness = harness_id,
        exit_code = code,
        "lane supervisor finished"
    );
    println!("[boop] lane {lane} finished rc={code}");
    std::process::exit(code);
}

/// Write what the lane was told to do, including the brief bytes as of now:
/// the file on disk is edited afterward and then nothing recovers the text.
#[allow(clippy::too_many_arguments)]
pub(crate) fn record_lane_purpose(
    lane: &str,
    trace: &str,
    harness: &str,
    branch: &str,
    repo: &Path,
    model: Option<&str>,
    parent: Option<&str>,
    goal: Option<&str>,
    brief: &Path,
) {
    let Ok(store) = boop::Store::default_path().and_then(boop::Store::open) else {
        return;
    };
    let spawn = boop::ident::LaneSpawn {
        lane: lane.to_owned(),
        trace: Some(trace.to_owned()),
        harness: Some(harness.to_owned()),
        branch: Some(branch.to_owned()),
        cwd: Some(repo.display().to_string()),
        model: model.map(str::to_owned),
        parent: parent.map(str::to_owned),
        goal: goal.map(str::to_owned),
        brief_path: Some(brief.display().to_string()),
        brief_body: std::fs::read_to_string(brief).ok(),
        ts: boop::channel::now_ms(),
    };
    if let Err(error) = store.record_lane_spawn(&spawn) {
        eprintln!("[boop] lane purpose not recorded: {error}");
    }
    let _ = store.attach_trace(lane, trace, "lane-create", boop::channel::now_ms());
}

/// Set the child's mood at spawn. No `agent_session` row exists yet: the
/// transcript sync writes that later, so the attribute is keyed on the lane
/// name's `dict_session` id, which is the same id `agent_lane` records.
pub(crate) fn record_lane_mood(lane: &str, mood: &str) -> Result<()> {
    let store = boop::Store::open(boop::Store::default_path()?)?;
    store.set_session_mood(lane, mood, boop::channel::now_ms())
}

// ---------------------------------------------------------------------------
// wait
// ---------------------------------------------------------------------------

/// How often the mailbox is re-read. boop carries no file-watch dependency
/// (notify lives in soopy), and a mail wait is measured in minutes.
pub(crate) const WAIT_POLL: std::time::Duration = std::time::Duration::from_secs(1);

pub(crate) fn run_wait(
    id: Option<&str>,
    me: bool,
    as_name: Option<&str>,
    timeout_secs: u64,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let watch = match (id, me) {
        (Some(id), _) => Watch::Reply { id: id.to_owned() },
        (None, _) => Watch::Inbox {
            name: waiting_as(&dir, as_name)?,
        },
    };
    let idle = id.and_then(|id| {
        report_delivery(id);
        watch_turn_end(&dir, id, timeout_secs)
    });
    wait_and_exit(&dir, watch, timeout_secs, as_name, mail_dir_arg, idle)
}

/// For a message the ledger says went through a door, a receiver that fires
/// once the recipient's turn ends. A door whose harness reports no idle (kimi)
/// yields nothing, and the wait stays on the mailbox alone.
pub(crate) fn watch_turn_end(
    dir: &Path,
    message_id: &str,
    timeout_secs: u64,
) -> Option<std::sync::mpsc::Receiver<String>> {
    let store = boop::Store::default_path()
        .and_then(boop::Store::open)
        .ok()?;
    let rows = store.delivery_rows(message_id).ok()?;
    let routes = bus::read_routes(dir).ok()?;
    let registry = std::sync::Arc::new(Registry::discover());
    let (sender, receiver) = std::sync::mpsc::channel();
    let mut armed = 0usize;
    for row in rows {
        // Only a rung that put the body itself in front of the recipient has
        // a turn to end. A held or pasted row waits on the mailbox alone.
        if !boop::DeliveryState::parse(&row.outcome)
            .is_some_and(|state| state == boop::DeliveryState::AcceptedByHarness)
        {
            continue;
        }
        let Some(route) = routes.get(&row.route).cloned() else {
            continue;
        };
        let Some(harness_id) = route.harness else {
            continue;
        };
        let Ok(Some(live)) =
            boop::mail::live_session(registry.get(harness_id), &store, &route, harness_id)
        else {
            continue;
        };
        let registry = registry.clone();
        let sender = sender.clone();
        let route_name = row.route.clone();
        let timeout = std::time::Duration::from_secs(timeout_secs);
        std::thread::spawn(move || {
            if let Ok(notice) = registry.get(harness_id).door().notify_idle(&live, timeout) {
                let status = notice.status_line.unwrap_or_else(|| "idle".into());
                let _ = sender.send(format!("{route_name} turn ended ({status})"));
            }
        });
        armed += 1;
    }
    (armed > 0).then_some(receiver)
}

/// The delivery history of the message being waited on: one line per recorded
/// transition, oldest first. An unreadable store costs the lines and nothing
/// else, because the wait itself reads the mailbox.
fn report_delivery(message_id: &str) {
    let rows = boop::Store::default_path()
        .and_then(boop::Store::open)
        .and_then(|store| store.delivery_rows(message_id));
    let Ok(rows) = rows else {
        return;
    };
    for row in rows {
        let code = row
            .error_code
            .as_deref()
            .map(|code| format!(" [{code}]"))
            .unwrap_or_default();
        line(&format!(
            "{message_id} -> {} #{} {} ({}){code}",
            row.route, row.sequence, row.outcome, row.detail
        ));
    }
}

/// Whose inbox `--me` watches: the name given, else the identity ladder's lane
/// or session. An unresolved caller is told to name itself, never guessed at.
pub(crate) fn waiting_as(dir: &Path, as_name: Option<&str>) -> Result<String> {
    let identity = identity::require(as_name);
    let from_env = identity.rung == Some(identity::Rung::Env);
    let name = identity
        .lane
        .or(identity.session)
        .context(identity::UNRESOLVED)?;
    if from_env {
        let routes = bus::read_routes(dir).unwrap_or_default();
        refuse_a_shared_stamp(&routes, &name)?;
    }
    Ok(name)
}

/// A native subagent runs inside its lane's process and inherits the lane's
/// `BOOP_SESSION`, so an env-resolved lane name that has live native children
/// could be any of them. Rather than watch the lane's mailbox while the
/// native's own rows sit unread (native-subagent-identity), name the
/// candidates and stop.
pub(crate) fn refuse_a_shared_stamp(routes: &BTreeMap<String, Route>, name: &str) -> Result<()> {
    if routes.get(name).map(|route| route.kind.as_str()) != Some("lane") {
        return Ok(());
    }
    let natives: Vec<&str> = routes
        .iter()
        .filter(|(_, route)| route.kind == "native" && route.parent.as_deref() == Some(name))
        .map(|(child, _)| child.as_str())
        .collect();
    if natives.is_empty() {
        return Ok(());
    }
    anyhow::bail!(
        "the env stamp names lane `{name}`, which has native subagents sharing its process ({}); \
         say which mailbox this is: `--as {name}` for the lane itself, or `--as <native>`",
        natives.join(", ")
    )
}

/// Block until the watch is satisfied, print what arrived, take delivery of it,
/// and exit. A timeout exits 124 with the re-run line on both streams.
/// The ledger half of the `--me` unread test (defect 3, addendum 2026-08-25).
/// The mailbox alone cannot say whether a rung already put a row in front of
/// its recipient; the delivery transitions can, so an inbox wait reads them
/// and hands back only what nothing else took. A `Reply` watch is unfiltered:
/// its row is the answer to a question this caller asked, and the caller is
/// the one taking delivery.
fn drop_rows_already_delivered(watch: &Watch, arrivals: Vec<bus::Message>) -> Vec<bus::Message> {
    if !matches!(watch, Watch::Inbox { .. }) {
        return arrivals;
    }
    let Ok(store) = boop::Store::default_path().and_then(boop::Store::open) else {
        return arrivals;
    };
    arrivals
        .into_iter()
        .filter(|message| {
            let Ok(rows) = store.delivery_rows(&message.id) else {
                return true;
            };
            !mailwait::already_in_front_of_the_recipient(
                rows.iter().map(|row| row.outcome.as_str()),
            )
        })
        .collect()
}

pub(crate) fn wait_and_exit(
    dir: &Path,
    watch: Watch,
    timeout_secs: u64,
    as_name: Option<&str>,
    mail_dir_arg: Option<&Path>,
    idle: Option<std::sync::mpsc::Receiver<String>>,
) -> Result<()> {
    let command = watch.command(
        timeout_secs,
        as_name,
        mail_dir_arg
            .map(|path| path.display().to_string())
            .as_deref(),
    );
    info!(watching = watch.what(), timeout_secs, "mail wait starting");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        let arrivals = drop_rows_already_delivered(&watch, watch.arrivals(&all_messages(dir)?));
        if !arrivals.is_empty() {
            info!(
                watching = watch.what(),
                rows = arrivals.len(),
                "mail wait answered"
            );
            for message in &arrivals {
                line(&bus::message_line(message));
                append_ack(dir, None, message)?;
            }
            line("re-arm: boop wait --me &");
            return Ok(());
        }
        if let Some(ended) = idle.as_ref().and_then(|receiver| receiver.try_recv().ok()) {
            info!(watching = watch.what(), "recipient turn ended");
            line(&ended);
            line("re-arm: boop wait --me &");
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            let timed_out = mailwait::timeout_line(&watch, timeout_secs, &command);
            info!(
                watching = watch.what(),
                timeout_secs,
                exit_code = 124,
                "mail wait timed out"
            );
            line(&timed_out);
            eprintln!("{timed_out}"); // @eprintln-ok: the re-run line must survive a redirected stdout
            std::process::exit(124);
        }
        std::thread::sleep(WAIT_POLL);
    }
}

// ---------------------------------------------------------------------------
// sweep
// ---------------------------------------------------------------------------

pub(crate) fn run_sweep(
    mail_dir_arg: Option<&Path>,
    box_name: Option<&str>,
    agent: Option<&str>,
    close_routeless: bool,
    max_age_days: u64,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let messages = all_messages(&dir)?;
    let pending = bus::unacked(&messages);
    if pending.is_empty() {
        println!("nothing unacked");
        return Ok(());
    }
    let cutoff_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
        .saturating_sub(max_age_days * 86_400_000);
    let mut acked = 0usize;
    let mut expired = 0usize;
    for message in &pending {
        if let Some(agent_id) = agent {
            if message.to != agent_id {
                continue;
            }
        }
        if parse_iso_ms(&message.from_timestamp).unwrap_or(0) < cutoff_ms {
            append_ack(&dir, box_name, message)?;
            expired += 1;
            println!("expired {}", message.id);
            continue;
        }
        let Some(route) = routes.get(&message.to) else {
            if close_routeless {
                append_ack(&dir, box_name, message)?;
                expired += 1;
                println!(
                    "expired {} -> {}: no registry route",
                    message.id, message.to
                );
            } else {
                println!(
                    "{} -> {}: no registry route, cannot scope the cass query (--close-routeless expires these)",
                    message.id, message.to
                );
            }
            continue;
        };
        if cass_hit(route, &message.id).unwrap_or(false) {
            append_ack(&dir, box_name, message)?;
            acked += 1;
            println!("{} -> {}: acked", message.id, message.to);
        } else {
            println!(
                "{} -> {}: no transcript hit, still unacked",
                message.id, message.to
            );
        }
    }
    println!(
        "swept {} unacked, acked {acked}, expired {expired}",
        pending.len()
    );
    Ok(())
}

/// Ask `cass` whether the envelope id appears in the recipient's transcript.
pub(crate) fn cass_hit(route: &Route, message_id: &str) -> Result<bool> {
    let output = Command::new("cass")
        .args(["search", message_id, "--robot", "--limit", "20"])
        .output();
    let output = match output {
        Ok(output) if output.status.success() => output,
        _ => return Ok(false),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    let hits = value
        .get("hits")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(hits.iter().any(|hit| {
        let source = hit
            .get("source_path")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        scoped_to_agent(route, source)
    }))
}

pub(crate) fn scoped_to_agent(route: &Route, source_path: &str) -> bool {
    if source_path.is_empty() {
        return false;
    }
    if let Some(expected) = route.source_path.as_deref() {
        return source_path == expected;
    }
    route
        .session_id
        .as_deref()
        .map(|session_id| source_path.contains(session_id))
        .unwrap_or(false)
}

pub(crate) fn parse_iso_ms(text: &str) -> Option<u64> {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    OffsetDateTime::parse(text, &Rfc3339)
        .ok()
        .map(|parsed| parsed.unix_timestamp() as u64 * 1000 + parsed.millisecond() as u64)
}

// ---------------------------------------------------------------------------
// lane
// ---------------------------------------------------------------------------

pub(crate) struct LaneArgs {
    pub(crate) name: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) harness: Option<String>,
    pub(crate) brief: Option<PathBuf>,
    pub(crate) model: Option<String>,
    pub(crate) preset: Option<String>,
    pub(crate) variant: Option<String>,
    pub(crate) bin: Option<String>,
    pub(crate) tmux: Option<String>,
    pub(crate) parent: Option<String>,
    pub(crate) branch: Option<String>,
    pub(crate) base_sha: Option<String>,
    pub(crate) socket: Option<String>,
    pub(crate) goal: Option<String>,
    pub(crate) mood: Option<String>,
    pub(crate) trace: Option<String>,
    pub(crate) no_start: bool,
    pub(crate) mail_dir: Option<PathBuf>,
    pub(crate) dry_run: bool,
    pub(crate) wait: bool,
    pub(crate) wait_timeout: u64,
    pub(crate) reclaim: bool,
}

/// Falls back to a `*coordinator*` name match only when no route declares
/// `kind == "coordinator"`, so a pre-`kind` registry row still resolves.
pub(crate) fn resolve_parent_with_legacy_fallback(
    explicit: Option<&str>,
    caller_lane: Option<&str>,
    routes: &BTreeMap<String, Route>,
) -> lane::ParentPick {
    let picked = lane::resolve_parent(explicit, caller_lane, routes);
    if picked.parent.is_some() || routes.values().any(|route| route.kind == "coordinator") {
        return picked;
    }
    let mut legacy = routes.keys().filter(|name| name.contains("coordinator"));
    match (legacy.next(), legacy.next()) {
        (Some(only), None) => lane::ParentPick {
            parent: Some(only.clone()),
            source: "registry-legacy",
        },
        _ => picked,
    }
}

/// What the warm-up will do to a fresh worktree of `repo`, for `--dry-run`.
pub(crate) fn start_plan(repo: &Path, no_start: bool) -> Result<String> {
    let recipe = boop::worktree::find_start_recipe(repo)?;
    Ok(match (no_start, recipe) {
        (true, _) => "boop-start: skipped (--no-start)".to_owned(),
        (false, Some(recipe)) => format!("boop-start: will run from {}", recipe.justfile.display()),
        (false, None) => format!(
            "boop-start: no recipe in {}, nothing to warm",
            repo.display()
        ),
    })
}

/// Register and spawn a lane. No match on harness id here; the adapter's own
/// `spawn`/`preview_command` decides how `prompt` becomes a real invocation.
pub(crate) fn run_lane(registry: &Registry, args: LaneArgs) -> Result<()> {
    let config_path = config::default_path()?;
    let config = config::load(&config_path)?;
    let model_given = args.model.is_some();
    // One row decides harness, model and effort. `--model` is the hidden
    // alias: it reads as a preset of one row that names no harness.
    let requested = match (args.model.as_deref(), args.preset.as_deref()) {
        (Some(model), _) => Some(config::ModelPreset::from_model(model)?),
        (None, Some(preset)) => Some(config::resolve_preset(preset, &config_path)?),
        (None, None) => None,
    };
    let requested_model = requested.as_ref().map(|preset| preset.model.clone());
    // A preset names its harness as surely as --harness does: choosing
    // `--preset opus` is the opt-in a claude tmux lane asks for.
    let preset_harness = match (model_given, &requested) {
        (false, Some(preset)) => lane::harness_for_preset(preset)?,
        _ => None,
    };
    let harness_id = lane::harness_for_spawn(
        registry,
        args.harness
            .as_deref()
            .or(preset_harness.map(boop::harness::HarnessId::as_str)),
        requested_model.as_deref(),
    )?;
    let adapter = registry.get(harness_id);
    let repo = match &args.cwd {
        Some(cwd) => PathBuf::from(cwd),
        None => lane::repo_root(&std::env::current_dir().context("read the current directory")?)?,
    };
    let identity = lane::derive(
        &repo,
        args.branch.as_deref(),
        args.name.as_deref(),
        args.tmux.as_deref(),
    )?;
    // The binary's own sha rides the first line of every spawn: a lane that
    // dies is otherwise impossible to tie to the boop that spawned it.
    info!(
        lane = identity.lane,
        tmux_target = identity.tmux,
        harness = harness_id.as_str(),
        cwd = %repo.display(),
        boop_build = boop::BUILD,
        "lane create resolved"
    );
    let worktree_mode = identity.worktree_dir.is_some();
    let brief = args.brief.clone().unwrap_or_else(|| repo.join("brief.md"));
    if !brief.is_absolute() {
        anyhow::bail!("brief path must be absolute: {}", brief.display());
    }
    if !brief.exists() {
        anyhow::bail!("brief path does not exist: {}", brief.display());
    }
    // A mood name is checked before anything spawns: a typo must not reach a
    // pane that then mails its coordinator in the default shape.
    if let Some(mood) = args.mood.as_deref() {
        boop::Store::open(boop::Store::default_path()?)?.check_mood_name(mood)?;
    }
    // The row that spawns: the one named, else the harness's default preset.
    // An explicit --model opts out of the default-preset lookup entirely.
    let default_preset = default_preset_for_harness(&config, &config_path, harness_id)?;
    let spawning = match (&requested, default_preset.as_deref()) {
        (Some(preset), _) => Some(preset.clone()),
        (None, Some(name)) => Some(config::resolve_preset(name, &config_path)?),
        (None, None) => None,
    };
    let model = spawning.as_ref().map(|preset| preset.model.clone());
    // Effort reaches the harness as its own config; the model string stays
    // bare (presets-only-model-spelling, luna open_failed 02:10:31).
    let effort = spawning.as_ref().and_then(|preset| preset.effort.clone());
    let variant = match args.variant {
        Some(variant) => Some(variant),
        None => spawning.as_ref().and_then(|preset| preset.variant.clone()),
    };
    let bin = match args.bin {
        Some(bin) => Some(bin),
        None => spawning.as_ref().and_then(|preset| preset.bin.clone()),
    };
    if variant.is_some() && adapter.capabilities().variant != VariantSupport::Flag {
        anyhow::bail!(
            "--variant is opencode-only; the codex channel sets reasoning effort via the \
             `model@effort` suffix instead"
        );
    }
    let prompt = brief.display().to_string();
    // A worktree branches from origin/main unless pinned; the repo-tree shape
    // keeps its own HEAD, where a base of origin/main would be a merge.
    let base = match (&args.base_sha, worktree_mode) {
        (Some(sha), _) => lane::BaseSha {
            sha: sha.clone(),
            rev: "--base-sha".to_owned(),
        },
        (None, true) => lane::default_base_sha(&repo)?,
        (None, false) => lane::BaseSha {
            sha: git_head(&repo.display().to_string())?.unwrap_or_else(|| "HEAD".into()),
            rev: "HEAD".to_owned(),
        },
    };
    let hail_mail_dir = mail_dir(args.mail_dir.as_deref())?;
    let routes = bus::read_routes(&hail_mail_dir)?;
    let caller = identity::resolve(&routes)?;
    let caller_lane = caller.lane.clone().filter(|lane| *lane != identity.lane);
    let parent = resolve_parent_with_legacy_fallback(
        args.parent.as_deref(),
        caller_lane.as_deref(),
        &routes,
    );
    // A parentless foreground waiter owns a private route parent. The
    // supervisor remains the sole completion-row writer, including when the
    // pane is killed before its route-only epilogue runs.
    let result_recipient =
        completion_recipient(parent.parent.as_deref(), args.wait, &identity.lane);
    let on_exit = result_recipient
        .as_ref()
        .map(|_| lane::pane_epilogue(&identity.lane, &hail_mail_dir));

    if args.dry_run {
        info!(
            lane = identity.lane,
            harness = harness_id.as_str(),
            "lane create dry run"
        );
        let spec = boop::harness::SpawnSpec {
            effort: effort.clone(),
            harness: harness_id,
            branch: identity.branch.clone(),
            base_sha: base.sha.clone(),
            main_tree: !worktree_mode,
            setup: Vec::new(),
            prompt: prompt.clone(),
            resume_session: None,
            socket: args.socket.clone(),
            worktree_dir: identity.worktree_dir.clone(),
            repo: repo.clone(),
            env_stamp: Some(spawn_env_stamp(
                &identity.lane,
                harness_id.as_str(),
                parent.parent.as_deref(),
            )),
            model: model.clone(),
            variant: variant.clone(),
            bin: bin.clone(),
            on_exit: on_exit.clone(),
            tmux: Some(identity.tmux.clone()),
            lane: identity.lane.clone(),
            mail_dir: hail_mail_dir.clone(),
            warm_start: !args.no_start,
        };
        let command = adapter
            .preview_command(&spec)
            .unwrap_or_else(|| format!("{} {}", adapter.id(), shell_quote(&prompt)));
        println!("cmd: {command}");
        println!("to: {}", identity.lane);
        println!("cwd: {}", repo.display());
        println!("harness: {harness_id}");
        if let Some(effort) = &effort {
            println!("effort: {effort}");
        }
        match lane::kind_of(&identity.branch) {
            Some(kind) => println!("branch: {} (kind {kind})", identity.branch),
            None => println!("branch: {}", identity.branch),
        }
        if let Some(worktree_dir) = &identity.worktree_dir {
            println!("worktree: {}", worktree_dir.display());
        }
        println!("{}", start_plan(&repo, args.no_start)?);
        println!("base-sha: {} (from {})", base.sha, base.rev);
        println!("tmux: {}", identity.tmux);
        match &parent.parent {
            Some(name) => println!(
                "parent: {name} (from {}; completion hail appended on exit)",
                parent.source
            ),
            None if args.wait => {
                println!("parent: - (foreground wait owns the completion receipt)")
            }
            None => println!("parent: - (no completion hail; pass --parent <lane>)"),
        }
        if let Some(bin) = &bin {
            println!("bin: {bin}");
        }
        if let Some(goal) = &args.goal {
            println!("goal: {goal}");
        }
        if let Some(mood) = &args.mood {
            println!("mood: {mood}");
        }
        if args.wait {
            println!(
                "wait: for {} result, timeout {}s",
                identity.lane, args.wait_timeout
            );
        }
        if args.reclaim {
            println!("reclaim: worktree and branch removed first, if the name is dead");
        }
        return Ok(());
    }
    if args.reclaim {
        let removed = lane::reclaim_for_spawn(&repo, &identity, &routes, |target| {
            tmux::mux().target_alive(None, target)
        })?;
        for line in removed.lines() {
            println!("reclaim: {line}");
        }
    }
    let lane_id = identity.lane.clone();
    let trace = args
        .trace
        .clone()
        .unwrap_or_else(|| format!("trace-{}", identity.lane));
    record_lane_purpose(
        &identity.lane,
        &trace,
        harness_id.as_str(),
        &identity.branch,
        &repo,
        model.as_deref(),
        parent.parent.as_deref(),
        args.goal.as_deref(),
        &brief,
    );
    if let Some(mood) = args.mood.as_deref() {
        record_lane_mood(&identity.lane, mood)?;
    }
    run_dispatch(
        registry,
        DispatchArgs {
            to: identity.lane,
            cwd: repo.display().to_string(),
            cmd: prompt,
            from: None,
            harness: Some(harness_id.as_str().to_owned()),
            session_id: None,
            model,
            effort,
            mode: Some("auto".into()),
            tmux: Some(identity.tmux),
            socket: args.socket,
            body: Some(format!(
                "Read and execute the lane brief at {}",
                brief.display()
            )),
            r#ref: Some(brief.display().to_string()),
            mail_dir: args.mail_dir,
            resolve_wait: 3,
            main_tree: !worktree_mode,
            base_sha: Some(base.sha),
            branch: Some(identity.branch),
            worktree_dir: identity.worktree_dir,
            parent: result_recipient,
            goal: args.goal.clone(),
            on_exit,
            warm_start: !args.no_start,
            variant: variant.clone(),
            bin: bin.clone(),
        },
    )?;
    info!(
        lane = lane_id,
        harness = harness_id.as_str(),
        "lane create dispatched"
    );
    if args.wait {
        // Same code path as `beep lane wait`, which exits with the lane's rc.
        return run_lane_wait(Some(&hail_mail_dir), &lane_id, args.wait_timeout);
    }
    Ok(())
}

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

pub(crate) fn completion_recipient(parent: Option<&str>, wait: bool, lane: &str) -> Option<String> {
    parent
        .map(str::to_owned)
        .or_else(|| wait.then(|| format!("__wait__{lane}")))
}

// ---------------------------------------------------------------------------
// beep
// ---------------------------------------------------------------------------

pub(crate) fn run_beep(registry: &Registry, cmd: BeepCmd) -> Result<()> {
    match cmd {
        BeepCmd::Harness { cmd } => match cmd {
            HarnessCmd::List => run_harnesses(registry),
            HarnessCmd::Get { harness } => run_harness_get(registry, &harness),
        },
        BeepCmd::Lane { cmd } => run_beep_lane(registry, cmd),
        BeepCmd::Agent { cmd } => run_agent(cmd),
        BeepCmd::Message { cmd } => match cmd {
            MessageCmd::Ack {
                lane,
                box_,
                close_routeless,
                max_age_days,
                mail_dir,
            } => run_sweep(
                mail_dir.as_deref(),
                box_.as_deref(),
                lane.as_deref(),
                close_routeless,
                max_age_days,
            ),
        },
        BeepCmd::Ps {
            lane,
            all,
            mail_dir,
        } => run_ps(mail_dir.as_deref(), lane.as_deref(), all),
        BeepCmd::Pstree {
            all,
            format,
            mail_dir,
        } => run_pstree(mail_dir.as_deref(), all, format),
    }
}

pub(crate) fn run_agent(cmd: AgentCmd) -> Result<()> {
    match cmd {
        AgentCmd::Register {
            name,
            kind,
            parent,
            on_parent_death,
            worktree,
            mail_dir: mail_dir_arg,
        } => {
            if !matches!(kind.as_str(), "coordinator" | "native") {
                anyhow::bail!("agent kind must be coordinator or native")
            }
            if let Some(tree) = worktree.as_deref().filter(|tree| !tree.is_dir()) {
                anyhow::bail!("no worktree at {}", tree.display());
            }
            let dir = mail_dir(mail_dir_arg.as_deref())?;
            boop::supervise::record_parent_policy(&dir, &name, on_parent_death)?;
            let started = worktree
                .as_deref()
                .map(boop::worktree::warm_start)
                .transpose()?;
            write_route(
                &dir,
                &name,
                Route {
                    kind,
                    harness: None,
                    tmux: None,
                    cwd: worktree.as_ref().map(|dir| dir.display().to_string()),
                    model: None,
                    mode: None,
                    session_id: None,
                    source_path: None,
                    parent,
                    goal: None,
                    registered_at: Some(bus::now_iso()),
                    base_sha: None,
                    worktree_dir: worktree.as_ref().map(|dir| dir.display().to_string()),
                    app_server_socket: None,
                },
            )?;
            println!("registered {name}");
            if let Some(outcome) = started {
                print!("{}", boop::lane::start_preamble(&outcome.status));
            }
            // Last line. A native shares its spawner's process, so no export
            // reaches it; every verb it runs names itself with `--as`, and a
            // bare `--me` under the spawner's stamp is refused as ambiguous.
            println!(
                "pass --as {name} on every boop call: this agent shares its spawner's env stamp"
            );
            Ok(())
        }
        AgentCmd::Done {
            name,
            rc,
            mail_dir: mail_dir_arg,
        } => {
            let dir = mail_dir(mail_dir_arg.as_deref())?;
            let routes = bus::read_routes(&dir)?;
            let route = routes
                .get(&name)
                .with_context(|| format!("no registered native route for `{name}`"))?;
            if !matches!(route.kind.as_str(), "coordinator" | "native") {
                anyhow::bail!("route `{name}` is not a native agent route")
            }
            let parent = route
                .parent
                .as_deref()
                .unwrap_or("sprefa-coordinator")
                .to_owned();
            let message = bus::Message {
                id: bus::mint_id(),
                from: name.clone(),
                to: parent,
                from_timestamp: bus::now_iso(),
                to_timestamp: None,
                kind: "result".into(),
                reply_to: None,
                body: format!("lane {name} done rc={rc}"),
                r#ref: None,
                rc: Some(rc),
                detail: None,
            };
            append_message(&dir, &message)?;
            let path = dir.join("registry.json");
            bus::cas_update_json(&path, |current| {
                current.remove(&name);
                Ok(())
            })?;
            println!("{}", message.body);
            Ok(())
        }
    }
}

pub(crate) fn run_beep_lane(registry: &Registry, cmd: LaneCmd) -> Result<()> {
    match cmd {
        LaneCmd::List {
            state,
            harness,
            all,
            mail_dir,
        } => run_lane_list(
            mail_dir.as_deref(),
            state.as_deref(),
            harness.as_deref().map(str::parse).transpose()?,
            all,
        ),
        LaneCmd::Create {
            lane,
            cwd,
            harness,
            brief,
            model,
            preset,
            variant,
            bin,
            tmux,
            parent,
            branch,
            base_sha,
            socket,
            goal,
            trace,
            no_start,
            mail_dir,
            dry_run,
            wait,
            wait_timeout,
            mood,
            reclaim,
            on_parent_death,
        } => {
            // Recorded before the spawn: the route the dispatch writes replaces
            // whatever is under this lane's key.
            if !dry_run {
                boop::supervise::record_spawn_policy(
                    &crate::mail_dir(mail_dir.as_deref())?,
                    branch.as_deref(),
                    lane.as_deref(),
                    on_parent_death,
                )?;
            }
            run_lane(
                registry,
                LaneArgs {
                    name: lane,
                    cwd,
                    harness,
                    brief,
                    model,
                    preset,
                    variant,
                    bin,
                    tmux,
                    parent,
                    branch,
                    base_sha,
                    socket,
                    goal,
                    mood,
                    trace,
                    no_start,
                    mail_dir,
                    dry_run,
                    wait,
                    wait_timeout,
                    reclaim,
                },
            )
        }
        LaneCmd::Run {
            lane,
            harness,
            brief,
            model,
            effort,
            resume,
            variant,
            bin,
            mail_dir,
        } => run_lane_supervisor(
            registry,
            &lane,
            &harness,
            &brief,
            model.as_deref(),
            effort.as_deref(),
            resume.as_deref(),
            variant.as_deref(),
            bin.as_deref(),
            mail_dir.as_deref(),
        ),
        LaneCmd::Get { lane, mail_dir } => run_lane_get(mail_dir.as_deref(), &lane),
        LaneCmd::Patch {
            lane,
            tmux,
            harness,
            session_id,
            cwd,
            model,
            mode,
            parent,
            goal,
            mail_dir,
            // A lane pane runs a supervisor that reads the mailbox itself, so
            // no hook inbox belongs on it.
        } => run_adopt(
            &lane,
            "lane",
            &tmux,
            harness.as_deref(),
            session_id.as_deref(),
            cwd.as_deref(),
            model.as_deref(),
            mode.as_deref(),
            parent.as_deref(),
            goal.as_deref(),
            mail_dir.as_deref(),
            false,
        ),
        LaneCmd::Delete {
            lane,
            route_only,
            state,
            dry_run,
            mail_dir,
        } => match (lane, state) {
            (Some(lane), _) => run_lane_delete(mail_dir.as_deref(), &lane, route_only),
            (None, Some(_)) => run_lane_bulk_delete(mail_dir.as_deref(), dry_run),
            (None, None) => {
                anyhow::bail!("name a lane to delete, or pass --state dead for a bulk delete")
            }
        },
        LaneCmd::Prune { dry_run, mail_dir } => run_lane_prune(mail_dir.as_deref(), dry_run),
        LaneCmd::Route { lane, mail_dir } => run_resolve(&lane, mail_dir.as_deref()),
        LaneCmd::Pane {
            lane,
            lines,
            socket,
            mail_dir,
        } => run_lane_pane(mail_dir.as_deref(), &lane, lines, socket.as_deref()),
        LaneCmd::Message { cmd } => match cmd {
            LaneMessageCmd::List { lane, mail_dir } => {
                run_list(mail_dir.as_deref(), Some(&lane), true)
            }
        },
    }
}

pub(crate) fn run_harness_get(registry: &Registry, id: &str) -> Result<()> {
    let adapter = resolve_harness(registry, id)?;
    let caps = adapter.control_capabilities();
    println!(
        "{}",
        serde_json::json!({
            "harness": adapter.id(),
            "send_midflight": caps.send_midflight,
            "resume": caps.resume,
            "spawn": caps.spawn,
            "subagent_visible": caps.subagent_visible,
        })
    );
    Ok(())
}

/// Lanes only. `boop list` printed routes and mail together; the two trees
/// split that, so this half never prints a message.
pub(crate) fn run_lane_list(
    mail_dir_arg: Option<&Path>,
    state_filter: Option<&str>,
    harness_filter: Option<HarnessId>,
    all: bool,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let live = tmux::mux().live_sessions(None);
    for (name, route) in &routes {
        let state = lane_state(&dir, name, &live, route, &routes);
        if let Some(want) = state_filter {
            if state != want {
                continue;
            }
        }
        if let Some(want) = harness_filter {
            if route.harness != Some(want) {
                continue;
            }
        }
        let flags = escape_flags(&dir, name);
        let mut suffix = String::new();
        if state == "dead" {
            suffix.push_str(&format!(" DEAD={}", dead_reason_token(&dir, name)));
        }
        if let Some(gone) = gone_parent(&dir, &routes, &live, route) {
            suffix.push_str(&format!(" PARENT-GONE={gone}"));
        }
        if let Some(flags) = &flags {
            if flags.worktree_untouched {
                suffix.push_str(" WORKTREE-UNTOUCHED");
            }
            if !flags.main_commits.is_empty() {
                suffix.push_str(&format!(
                    " MAIN-TREE-COMMIT-SUSPECT={}",
                    flags.main_commits.join(",")
                ));
            }
            for commit in &flags.ambiguous_main_commits {
                suffix.push_str(&format!(
                    " MAIN-TREE-COMMIT-AMBIGUOUS={}:{}",
                    commit.sha,
                    commit.lanes.join("|")
                ));
            }
        }
        line(&format!(
            "{} {} {} {} {} {} {} {}{}",
            pad(state, 4),
            pad(name, 16),
            pad(&route.kind, 12),
            pad(route.harness.map_or("-", HarnessId::as_str), 10),
            pad(route.mode.as_deref().unwrap_or("-"), 6),
            pad(route.model.as_deref().unwrap_or("-"), 46),
            pad(route.tmux.as_deref().unwrap_or("-"), 16),
            route.cwd.as_deref().unwrap_or("-"),
            suffix,
        ));
    }
    if all {
        for name in unregistered_sessions(&routes, &live) {
            line(&format!(
                "{} {} {} {} {} {} {} {}",
                pad("live", 4),
                pad(&name, 16),
                pad("unregistered", 12),
                pad("-", 10),
                pad("-", 6),
                pad("-", 46),
                pad("-", 16),
                "-",
            ));
        }
        for (route_name, route) in &routes {
            if route.harness != Some(HarnessId::Claude) {
                continue;
            }
            let Some(cwd) = route.cwd.as_deref() else {
                continue;
            };
            for (name, path, locked) in claude_agent_worktrees(cwd) {
                let state = if locked { "live" } else { "dead" };
                line(&format!(
                    "{} {} {} {} {} {} {} {} PARENT={}",
                    pad(state, 4),
                    pad(&name, 16),
                    pad("native-claude", 12),
                    pad("-", 10),
                    pad("-", 6),
                    pad("-", 46),
                    pad("-", 16),
                    path,
                    route_name,
                ));
            }
        }
    }
    Ok(())
}

/// The tmux sessions no route claims: not a route `tmux` target and not a
/// route name. These are live sessions boop does not manage, listed under
/// `--all` so the panel's view counts them without pretending they are lanes.
pub(crate) fn unregistered_sessions(
    routes: &BTreeMap<String, Route>,
    live: &Option<tmux::LiveSessions>,
) -> Vec<String> {
    let Some(live) = live else {
        return Vec::new();
    };
    live.names
        .iter()
        .filter(|name| {
            !routes.iter().any(|(route_name, route)| {
                route.tmux.as_deref() == Some(name.as_str()) || route_name == name.as_str()
            })
        })
        .cloned()
        .collect()
}

/// The native Claude Code subagent worktrees linked into the repo at `cwd`.
/// One tuple per `git worktree list --porcelain` block whose path carries
/// `/.claude/worktrees/agent-`: `(agent-<id> name, path, locked)`.
pub(crate) fn claude_agent_worktrees(cwd: &str) -> Vec<(String, String, bool)> {
    let output = Command::new("git")
        .args(["-C", cwd, "worktree", "list", "--porcelain"])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_claude_agent_worktrees(&String::from_utf8_lossy(&output.stdout))
}

fn parse_claude_agent_worktrees(porcelain: &str) -> Vec<(String, String, bool)> {
    let mut result = Vec::new();
    let mut current: Option<(String, bool)> = None;
    for line in porcelain.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some((path, locked)) = current.take() {
                push_claude_agent(&mut result, path, locked);
            }
            current = Some((path.to_owned(), false));
        } else if line == "locked" {
            if let Some((_, locked)) = current.as_mut() {
                *locked = true;
            }
        }
    }
    if let Some((path, locked)) = current {
        push_claude_agent(&mut result, path, locked);
    }
    result
}

fn push_claude_agent(result: &mut Vec<(String, String, bool)>, path: String, locked: bool) {
    const MARKER: &str = "/.claude/worktrees/agent-";
    let Some(idx) = path.find(MARKER) else {
        return;
    };
    let id = path[idx + MARKER.len()..]
        .split('/')
        .next()
        .unwrap_or_default();
    if id.is_empty() {
        return;
    }
    result.push((format!("agent-{id}"), path, locked));
}

/// The parent edge that answers nobody, so a surviving orphan says so on its
/// own row. `None` while the parent route is still addressable.
pub(crate) fn gone_parent<'a>(
    dir: &Path,
    routes: &BTreeMap<String, Route>,
    live: &Option<tmux::LiveSessions>,
    route: &'a Route,
) -> Option<&'a str> {
    let parent = route.parent.as_deref()?;
    match routes.get(parent) {
        Some(parent_route) if lane_state(dir, parent, live, parent_route, routes) != "dead" => None,
        _ => Some(parent),
    }
}

/// Why a dead lane is dead, as one token. A missing home directory is itself an
/// answer: nothing could have been written, so the row says `no-trail`.
pub(crate) fn dead_reason_token(mail_dir: &std::path::Path, lane: &str) -> String {
    let Ok(root) = boop::trail::lanes_root() else {
        return boop::trail::DeadReason::NoTrail.token();
    };
    boop::trail::dead_reason(mail_dir, &root, lane).token()
}

/// `live`/`idle`/`dead`/`?`. `idle` reads the supervisor's residency file; a
/// lane older than that file reads through as `live`.
///
/// A pane-less coordinator or native route has no tmux target of its own to
/// probe, so its liveness is inherited from its parent. The parent hop is
/// measured once and no deeper: a parent that is itself pane-less answers `?`,
/// and a parentless pane-less route answers `?` too.
pub(crate) fn lane_state(
    dir: &Path,
    name: &str,
    live: &Option<tmux::LiveSessions>,
    route: &Route,
    routes: &BTreeMap<String, Route>,
) -> &'static str {
    lane_state_hop(dir, name, live, route, routes, true)
}

/// The shared body of `lane_state`. A pane-less coordinator/native route has
/// no tmux target of its own, so its liveness is inherited from its parent.
/// `allow_parent_hop` bounds the inheritance to one level: a parent that is
/// itself pane-less answers `?` rather than chasing a grandparent, and a
/// parentless pane-less route answers `?` too.
fn lane_state_hop(
    dir: &Path,
    name: &str,
    live: &Option<tmux::LiveSessions>,
    route: &Route,
    routes: &BTreeMap<String, Route>,
    allow_parent_hop: bool,
) -> &'static str {
    if route.tmux.is_none() && matches!(route.kind.as_str(), "coordinator" | "native") {
        if !allow_parent_hop {
            return "?";
        }
        let Some(parent_name) = route.parent.as_deref() else {
            return "?";
        };
        let Some(parent_route) = routes.get(parent_name) else {
            // A parent that never registered is not evidence of death: the
            // native stays live until an explicit done.
            return "live";
        };
        return lane_state_hop(dir, parent_name, live, parent_route, routes, false);
    }
    let tmux_alive = match live {
        None => return "?",
        Some(_) => route
            .tmux
            .as_deref()
            .is_some_and(|target| tmux::mux().target_alive(None, target)),
    };
    if !tmux_alive {
        return "dead";
    }
    match boop::supervise::read_residency(dir, name).as_deref() {
        Some(boop::supervise::RESIDENCY_IDLE) => "idle",
        _ => "live",
    }
}

pub(crate) fn run_lane_get(mail_dir_arg: Option<&Path>, lane: &str) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let Some(route) = routes.get(lane) else {
        anyhow::bail!("no registry route for lane `{lane}`")
    };
    let live = tmux::mux().live_sessions(None);
    println!(
        "{}",
        serde_json::json!({
            "lane": lane,
            "state": lane_state(&dir, lane, &live, route, &routes),
            "harness": route.harness,
            "tmux": route.tmux,
            "cwd": route.cwd,
            "model": route.model,
            "mode": route.mode,
            "session_id": route.session_id,
        })
    );
    Ok(())
}

/// Stop one lane and drop its route. Refuses when tmux is unreachable. `--route-only`
/// drops the registry row and never touches the pane, so the on-exit epilogue can run inside it.
pub(crate) fn run_lane_delete(
    mail_dir_arg: Option<&Path>,
    lane: &str,
    route_only: bool,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let Some(route) = routes.get(lane) else {
        if route_only {
            anyhow::bail!("no registry route for lane `{lane}`")
        }
        return run_lane_delete_carcass(lane);
    };
    if !route_only {
        if let Some(session) = route.tmux.as_deref() {
            match tmux::mux().has_session(None, session) {
                Ok(true) => tmux::mux().kill_session(None, session)?,
                Ok(false) => {}
                Err(error) => anyhow::bail!("tmux unreachable, refusing to delete {lane}: {error}"),
            }
        }
    }
    let path = dir.join("registry.json");
    bus::cas_update_json(&path, |current| {
        current.remove(lane);
        Ok(())
    })?;
    info!(lane, route_only, "lane route deleted");
    println!("deleted {lane}");
    Ok(())
}

/// Bulk delete: every dead lane route, its own `worktree_dir` and nothing
/// else. A path is removed only when `deletable_worktree` says it is a linked
/// worktree whose top level is itself, so a parent such as `.boop-worktrees`
/// is never a candidate and a live sibling under it is never reached.
pub(crate) fn run_lane_bulk_delete(mail_dir_arg: Option<&Path>, dry_run: bool) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    if tmux::mux().live_sessions(None).is_none() {
        anyhow::bail!("tmux unreachable, cannot tell live from dead");
    }
    let routes = bus::read_routes(&dir)?;
    let snapshot = proc::SysinfoSnapshot::capture()?;
    let dead: Vec<(String, Option<PathBuf>)> = routes
        .iter()
        .filter(|(_, route)| route.kind == "lane")
        .filter(|(_, route)| dead_reason(route, &snapshot).is_some())
        .map(|(name, route)| {
            let worktree = route
                .worktree_dir
                .as_deref()
                .map(Path::new)
                .and_then(boop::worktree::deletable_worktree);
            (name.clone(), worktree)
        })
        .collect();

    for (name, worktree) in &dead {
        match worktree {
            Some(path) => line(&format!("lane {name}: route, worktree {}", path.display())),
            None => line(&format!("lane {name}: route only, no worktree of its own")),
        }
    }
    if dry_run {
        line(&format!(
            "{} lane(s) and {} worktree(s) would be removed (dry run)",
            dead.len(),
            dead.iter().filter(|(_, path)| path.is_some()).count()
        ));
        return Ok(());
    }

    let mut removed_worktrees = 0usize;
    for (name, worktree) in &dead {
        let Some(path) = worktree else { continue };
        let Some(repo) = boop::worktree::worktree_owner(path) else {
            line(&format!(
                "lane {name}: {} names no owning repo, left alone",
                path.display()
            ));
            continue;
        };
        match remove_one_worktree(&repo, path) {
            Ok(()) => {
                removed_worktrees += 1;
                line(&format!("removed worktree {}", path.display()));
            }
            Err(error) => line(&format!("lane {name}: {} kept, {error}", path.display())),
        }
    }
    let path = dir.join("registry.json");
    bus::cas_update_json(&path, |current| {
        for (name, _) in &dead {
            current.remove(name);
        }
        Ok(())
    })?;
    info!(
        routes_deleted = dead.len(),
        worktrees_removed = removed_worktrees,
        mail_dir = %dir.display(),
        "lane routes bulk deleted"
    );
    line(&format!(
        "{} lane(s) deleted, {removed_worktrees} worktree(s) removed",
        dead.len()
    ));
    Ok(())
}

/// One `git worktree remove --force` against exactly `worktree`, run from the
/// owning repo. Nothing else on disk is touched.
fn remove_one_worktree(repo: &Path, worktree: &Path) -> Result<()> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["worktree", "remove", "--force"])
        .arg(worktree)
        .output()
        .with_context(|| format!("run git worktree remove in {}", repo.display()))?;
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr).trim().to_owned()
    );
    Ok(())
}

/// A DOA spawn's epilogue drops the route before the driver can delete the
/// lane, so the worktree and branch are all that is left to remove.
pub(crate) fn run_lane_delete_carcass(lane: &str) -> Result<()> {
    let here = std::env::current_dir().context("read the current directory")?;
    let repo = lane::repo_root(&here)?;
    let removed =
        lane::delete_carcass(&repo, lane, |target| tmux::mux().target_alive(None, target))?;
    for line in removed.lines() {
        println!("deleted {lane}: {line}");
    }
    if removed.nothing_removed() {
        println!("deleted {lane}: nothing left to remove");
    }
    info!(lane, "lane carcass deleted");
    Ok(())
}

/// Bulk-drop dead rows. Registry-only: bus.ndjson is never touched.
pub(crate) fn run_lane_prune(mail_dir_arg: Option<&Path>, dry_run: bool) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    if tmux::mux().live_sessions(None).is_none() {
        anyhow::bail!("tmux unreachable, cannot tell live from dead");
    }
    let routes = bus::read_routes(&dir)?;
    let snapshot = proc::SysinfoSnapshot::capture()?;
    let dead: Vec<(String, String, String)> = routes
        .iter()
        .filter(|(_, route)| route.kind == "lane")
        .filter_map(|(name, route)| {
            let why = dead_reason(route, &snapshot)?;
            Some((
                name.clone(),
                route.tmux.clone().unwrap_or_else(|| "-".into()),
                why,
            ))
        })
        .collect();
    for (name, tmux_name, why) in &dead {
        line(&format!("lane {name} {tmux_name} {why}"));
    }
    if dry_run {
        line(&format!("{} lane(s) would be pruned (dry run)", dead.len()));
        return Ok(());
    }
    let path = dir.join("registry.json");
    bus::cas_update_json(&path, |current| {
        for (name, _, _) in &dead {
            current.remove(name);
        }
        Ok(())
    })?;
    line(&format!("{} lane(s) pruned", dead.len()));
    Ok(())
}

/// `None` when the route's tmux target is live; `Some(reason)` when the tmux
/// target is gone and its resolvable pid, if any, is also not alive.
pub(crate) fn dead_reason(route: &Route, snapshot: &proc::SysinfoSnapshot) -> Option<String> {
    let Some(target) = route.tmux.as_deref() else {
        return Some("no tmux session recorded".to_owned());
    };
    if tmux::mux().target_alive(None, target) {
        return None;
    }
    match tmux::mux().pane_pid(None, target) {
        Some(pid) if snapshot.is_alive(pid) => None,
        Some(pid) => Some(format!("tmux session gone, pid {pid} not alive")),
        None => Some("tmux session gone, no pid recorded".to_owned()),
    }
}

pub(crate) fn run_lane_pane(
    mail_dir_arg: Option<&Path>,
    lane: &str,
    lines: Option<u32>,
    socket: Option<&str>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let Some(route) = routes.get(lane) else {
        anyhow::bail!("no registry route for lane `{lane}`")
    };
    let Some(target) = route.tmux.as_deref() else {
        anyhow::bail!("lane `{lane}` has no tmux session to capture")
    };
    print!("{}", tmux::mux().capture_pane(socket, target, lines)?);
    Ok(())
}

/// `beep lane wait`: poll for a `kind=result` row from `lane`, exit with its
/// rc; `--timeout` seconds exits 124, a pre-existing row returns immediately.
/// A route that goes dead with no result row exits 3.
pub(crate) fn run_lane_wait(
    mail_dir_arg: Option<&Path>,
    lane: &str,
    timeout_secs: u64,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let deadline = if timeout_secs == 0 {
        None
    } else {
        Some(std::time::Duration::from_secs(timeout_secs))
    };
    info!(lane, timeout_secs, "lane result wait starting");
    match wait_for_outcome(
        &dir,
        lane,
        deadline,
        std::time::Duration::from_secs(1),
        &route_liveness,
    ) {
        WaitOutcome::Result(rc) => {
            info!(lane, exit_code = rc, "lane result received");
            if let Some(flags) = escape_flags(&dir, lane) {
                print_escape_flags(lane, &flags);
            }
            std::process::exit(rc)
        }
        WaitOutcome::Died => {
            warn!(lane, exit_code = 3, "lane route died with no result row");
            line(&format!(
                "lane {lane} died without a result (see its worktree and opencode session for the trail)"
            ));
            std::process::exit(3)
        }
        WaitOutcome::TimedOut => {
            info!(lane, exit_code = 124, "lane result wait timed out");
            std::process::exit(124)
        }
    }
}

/// What one wait resolved to.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WaitOutcome {
    Result(i32),
    /// The route stopped being live and no result row for this spawn exists.
    Died,
    TimedOut,
}

/// Whether the lane's route is still backed by a live tmux session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RouteLiveness {
    Live,
    Dead,
    /// No route row, no tmux target, or tmux itself unreachable. None of the
    /// three is evidence of death, so none of them ends a wait.
    Unknown,
}

/// Consecutive dead observations before a wait calls the lane dead. A route is
/// written before its session answers, so one observation is never enough.
pub(crate) const DEAD_POLLS: u32 = 5;

/// The route's liveness through the same probe `lane list` prints.
pub(crate) fn route_liveness(dir: &std::path::Path, lane: &str) -> RouteLiveness {
    let Ok(routes) = bus::read_routes(dir) else {
        return RouteLiveness::Unknown;
    };
    let Some(route) = routes.get(lane) else {
        return RouteLiveness::Unknown;
    };
    if route.tmux.is_none() && matches!(route.kind.as_str(), "coordinator" | "native") {
        return RouteLiveness::Live;
    }
    if route.tmux.is_none() {
        return RouteLiveness::Unknown;
    }
    match lane_state(dir, lane, &tmux::mux().live_sessions(None), route, &routes) {
        "live" | "idle" => RouteLiveness::Live,
        "dead" => RouteLiveness::Dead,
        _ => RouteLiveness::Unknown,
    }
}

/// The rc from the lane's most recent `kind=result` mailbox row (`lane <id>
/// done rc=N`), `None` when no result row for that lane exists yet. Without a
/// route row the wait is after-the-fact: any result row satisfies.
#[cfg(test)]
pub(crate) fn lane_result_rc(dir: &std::path::Path, lane: &str) -> Option<i32> {
    lane_result_rc_since(dir, lane, None)
}

/// As `lane_result_rc`, but only a result row at or after `since` (ms since
/// epoch) satisfies; older rows belong to a previous spawn and are skipped.
pub(crate) fn lane_result_rc_since(
    dir: &std::path::Path,
    lane: &str,
    since: Option<u64>,
) -> Option<i32> {
    let mut messages = Vec::new();
    for box_path in bus::read_boxes(dir).unwrap_or_default() {
        messages.extend(bus::parse_box(&box_path));
    }
    let folded = bus::fold(&messages);
    folded
        .iter()
        .rev()
        // The supervisor hails `--to <parent> --from <lane>`, so the lane that
        // finished is the sender; `to` matches a hand-addressed row.
        .find(|message| {
            if message.kind != "result" || (message.from != lane && message.to != lane) {
                return false;
            }
            match since {
                Some(boundary) => parse_iso_ms(&message.from_timestamp).unwrap_or(0) >= boundary,
                None => true,
            }
        })
        .and_then(|message| message.rc)
}

/// The newest moment a result row must postdate to count: the route's
/// registration, or the latest inbound row the lane's supervisor took
/// (`to_timestamp` set), whichever is later. A resume turn starts with such a
/// claim, so the previous turn's done row stops satisfying a wait
/// (wait-stale-result-row).
pub(crate) fn wait_boundary(dir: &std::path::Path, lane: &str) -> Option<u64> {
    let registered = route_registered_at(dir, lane);
    let claimed = lane_last_claim_at(dir, lane);
    match (registered, claimed) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    }
}

/// Epoch millis of the newest inbound row addressed to `lane` that a rung
/// took (`to_timestamp` set); `None` when nothing was ever taken.
pub(crate) fn lane_last_claim_at(dir: &std::path::Path, lane: &str) -> Option<u64> {
    let mut messages = Vec::new();
    for box_path in bus::read_boxes(dir).unwrap_or_default() {
        messages.extend(bus::parse_box(&box_path));
    }
    bus::fold(&messages)
        .iter()
        .filter(|message| message.to == lane && message.kind != "result")
        .filter_map(|message| message.to_timestamp.as_deref())
        .filter_map(parse_iso_ms)
        .max()
}

/// The lane's registration timestamp (ms since epoch) for the spawn that
/// wrote the current route row; `None` when no route row exists.
pub(crate) fn route_registered_at(dir: &std::path::Path, lane: &str) -> Option<u64> {
    bus::read_routes(dir)
        .ok()?
        .get(lane)
        .and_then(|route| route.registered_at.as_deref())
        .and_then(parse_iso_ms)
}

/// The worktree-escape flags for a lane, or `None` when the route records no
/// worktree (a main-tree spawn) or no base sha to compare against.
pub(crate) fn escape_flags(
    dir: &std::path::Path,
    lane: &str,
) -> Option<boop::worktree::EscapeFlags> {
    let routes = bus::read_routes(dir).ok()?;
    let route = routes.get(lane)?;
    let worktree = std::path::Path::new(route.worktree_dir.as_deref()?);
    let base_sha = route.base_sha.as_deref()?;
    if base_sha.is_empty() {
        return None;
    }
    let repo = lane::repo_root(worktree).ok()?;
    let run = lane_window(dir, lane, &routes)?;
    // Every other lane registered against this same repo. Without them a shared
    // main tree makes one lane's commits look like every lane's.
    let siblings: Vec<boop::worktree::LaneWindow> = routes
        .keys()
        .filter(|name| name.as_str() != lane)
        .filter(|name| sibling_repo(name, &routes).as_deref() == Some(repo.as_path()))
        .filter_map(|name| lane_window(dir, name, &routes))
        .collect();
    Some(boop::worktree::detect_escape(
        worktree, &repo, base_sha, &run, &siblings,
    ))
}

/// The repo a lane's registered worktree belongs to.
pub(crate) fn sibling_repo(
    lane: &str,
    routes: &std::collections::BTreeMap<String, Route>,
) -> Option<std::path::PathBuf> {
    let route = routes.get(lane)?;
    lane::repo_root(std::path::Path::new(route.worktree_dir.as_deref()?)).ok()
}

/// When a lane held its repo and on which branch: the spawn's `registered_at`
/// opens the window, its result row closes it, and the worktree names the
/// branch that witnesses reachability.
pub(crate) fn lane_window(
    dir: &std::path::Path,
    lane: &str,
    routes: &std::collections::BTreeMap<String, Route>,
) -> Option<boop::worktree::LaneWindow> {
    let route = routes.get(lane)?;
    let worktree = std::path::Path::new(route.worktree_dir.as_deref()?);
    let start_ms = route.registered_at.as_deref().and_then(parse_iso_ms)?;
    Some(boop::worktree::LaneWindow {
        lane: lane.to_owned(),
        branch: boop::worktree::current_branch(worktree),
        start_secs: (start_ms / 1000) as i64,
        end_secs: lane_result_at_ms(dir, lane, start_ms).map(|ms| (ms / 1000) as i64),
    })
}

/// Epoch millis of the lane's newest result row at or after `since`, which is
/// the moment the lane stopped being able to commit anywhere.
pub(crate) fn lane_result_at_ms(dir: &std::path::Path, lane: &str, since: u64) -> Option<u64> {
    let mut rows = Vec::new();
    for path in bus::read_boxes(dir).unwrap_or_default() {
        rows.extend(bus::parse_box(&path));
    }
    rows.iter()
        .filter(|row| row.kind == "result" && row.from == lane)
        .filter_map(|row| parse_iso_ms(&row.from_timestamp))
        .filter(|written| *written >= since)
        .max()
}

/// Print the loud escape flags to stdout. `WORKTREE-UNTOUCHED` names a lane
/// whose worktree gained no commit; `MAIN-TREE-COMMIT-SUSPECT` lists the shas
/// only this lane's branch or window accounts for, and
/// `MAIN-TREE-COMMIT-AMBIGUOUS` the shas a concurrent lane could equally have
/// made.
pub(crate) fn print_escape_flags(lane: &str, flags: &boop::worktree::EscapeFlags) {
    if flags.worktree_untouched {
        println!("WORKTREE-UNTOUCHED {lane}: no new commits in its registered worktree");
    }
    if !flags.main_commits.is_empty() {
        println!(
            "MAIN-TREE-COMMIT-SUSPECT {lane}: {}",
            flags.main_commits.join(" ")
        );
    }
    for commit in &flags.ambiguous_main_commits {
        println!(
            "MAIN-TREE-COMMIT-AMBIGUOUS {lane}: {} could be any of {}",
            commit.sha,
            commit.lanes.join(" ")
        );
    }
}

/// Poll `lane_result_rc` every `interval` until a result appears or `deadline`
/// passes. `None` on deadline is a timeout; `since` bounds satisfying rows.
#[cfg(test)]
pub(crate) fn wait_for_result(
    dir: &std::path::Path,
    lane: &str,
    deadline: Option<std::time::Duration>,
    interval: std::time::Duration,
) -> Option<i32> {
    match wait_for_outcome(dir, lane, deadline, interval, &|_, _| {
        RouteLiveness::Unknown
    }) {
        WaitOutcome::Result(rc) => Some(rc),
        WaitOutcome::Died | WaitOutcome::TimedOut => None,
    }
}

/// As `wait_for_result`, plus the liveness probe that turns a vanished lane
/// into `Died` instead of a wait that outlives the process it waits on.
pub(crate) fn wait_for_outcome(
    dir: &std::path::Path,
    lane: &str,
    deadline: Option<std::time::Duration>,
    interval: std::time::Duration,
    liveness: &dyn Fn(&std::path::Path, &str) -> RouteLiveness,
) -> WaitOutcome {
    let since = wait_boundary(dir, lane);
    let start = std::time::Instant::now();
    let mut dead_polls = 0u32;
    loop {
        if let Some(rc) = lane_result_rc_since(dir, lane, since) {
            return WaitOutcome::Result(rc);
        }
        dead_polls = match liveness(dir, lane) {
            RouteLiveness::Dead => dead_polls + 1,
            RouteLiveness::Live | RouteLiveness::Unknown => 0,
        };
        if dead_polls >= DEAD_POLLS {
            return WaitOutcome::Died;
        }
        if deadline.is_some_and(|limit| start.elapsed() >= limit) {
            return WaitOutcome::TimedOut;
        }
        std::thread::sleep(interval);
    }
}

/// `beep ps`, optionally narrowed to one lane.
pub(crate) fn run_ps(mail_dir_arg: Option<&Path>, lane: Option<&str>, all: bool) -> Result<()> {
    let snapshot = proc::SysinfoSnapshot::capture()?;
    run_ps_with(mail_dir_arg, lane, all, &snapshot)
}

/// Takes the `ProcReader` seam rather than the concrete snapshot, so a fake
/// reader can drive this without a real process tree.
pub(crate) fn run_ps_with(
    mail_dir_arg: Option<&Path>,
    lane: Option<&str>,
    all: bool,
    reader: &dyn proc::ProcReader,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    line("lane\tpid\trss_kb\tcpu_pct\tuptime_sec\tchildren");
    for (name, route) in &routes {
        if let Some(want) = lane {
            if name != want {
                continue;
            }
        }
        let pane_pid = route
            .tmux
            .as_deref()
            .and_then(|target| tmux::mux().pane_pid(None, target))
            .unwrap_or(0);
        match proc::tree_sum_of(reader, pane_pid) {
            Some(sum) => {
                let now = now_unix_secs();
                let uptime = proc::uptime_secs(sum.start_time_secs, now);
                println!(
                    "{}\t{}\t{}\t{:.1}\t{}\t{}",
                    name,
                    pane_pid,
                    sum.rss_bytes / 1024,
                    sum.cpu_percent,
                    uptime,
                    reader.descendant_count(pane_pid),
                );
            }
            // A dead route prints only when asked for by name or --all.
            None if all || lane.is_some() => {
                println!("{}\t{}\t-\t-\t-\t-", name, pane_pid);
            }
            None => {}
        }
    }
    Ok(())
}

pub(crate) fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// pstree
// ---------------------------------------------------------------------------

/// The resolved `from -> to` summon edge for a lane. Explicit beats inferred.
#[derive(Clone, Debug)]
pub(crate) struct LaneEdge {
    /// The summoning lane, `None` for a true root.
    parent: Option<String>,
    /// `true` when the edge came from the first dispatch row, not a route
    /// `--parent`.
    inferred: bool,
}

pub(crate) fn resolve_edges(
    routes: &BTreeMap<String, Route>,
    messages: &[bus::Message],
) -> BTreeMap<String, LaneEdge> {
    routes
        .iter()
        .map(|(name, route)| {
            let edge = match &route.parent {
                Some(parent) => LaneEdge {
                    parent: Some(parent.clone()),
                    inferred: false,
                },
                None => {
                    let summoner = messages
                        .iter()
                        .find(|message| message.kind == "dispatch" && message.to == *name)
                        .and_then(|message| {
                            (!message.from.is_empty()).then(|| message.from.clone())
                        });
                    match summoner {
                        Some(parent) => LaneEdge {
                            parent: Some(parent),
                            inferred: true,
                        },
                        None => LaneEdge {
                            parent: None,
                            inferred: false,
                        },
                    }
                }
            };
            (name.clone(), edge)
        })
        .collect()
}

pub(crate) struct LaneMeta {
    pid: u32,
    state: &'static str,
    descendants: Vec<ProcessDesc>,
}

#[derive(Clone)]
pub(crate) struct ProcessDesc {
    pid: u32,
    comm: String,
}

/// One renderable node: a real lane or a `[gone]` phantom for a summoner that
/// is not itself a known lane.
pub(crate) struct LaneNode {
    name: String,
    parent: Option<String>,
    inferred: bool,
    pid: u32,
    state: &'static str,
    descendants: Vec<ProcessDesc>,
    goal: Option<String>,
    gone: bool,
    children: Vec<usize>,
}

pub(crate) fn build_lane_nodes(
    routes: &BTreeMap<String, Route>,
    edges: &BTreeMap<String, LaneEdge>,
    meta: &BTreeMap<String, LaneMeta>,
    include: &BTreeSet<String>,
) -> Vec<LaneNode> {
    let mut nodes: Vec<LaneNode> = Vec::new();
    let mut index: BTreeMap<String, usize> = BTreeMap::new();
    for name in include {
        let lane = meta.get(name).expect("included lane has meta");
        let edge = edges.get(name).expect("included lane has edge");
        let idx = nodes.len();
        nodes.push(LaneNode {
            name: name.clone(),
            parent: edge.parent.clone(),
            inferred: edge.inferred,
            pid: lane.pid,
            state: lane.state,
            descendants: lane.descendants.clone(),
            goal: routes.get(name).and_then(|route| route.goal.clone()),
            gone: false,
            children: Vec::new(),
        });
        index.insert(name.clone(), idx);
    }
    let mut phantom: BTreeSet<String> = BTreeSet::new();
    for name in include {
        if let Some(parent) = edges.get(name).and_then(|edge| edge.parent.as_deref()) {
            if !include.contains(parent) {
                phantom.insert(parent.to_owned());
            }
        }
    }
    for name in phantom {
        let idx = nodes.len();
        nodes.push(LaneNode {
            name: name.clone(),
            parent: None,
            inferred: false,
            pid: 0,
            state: "gone",
            descendants: Vec::new(),
            goal: None,
            gone: true,
            children: Vec::new(),
        });
        index.insert(name, idx);
    }
    for idx in 0..nodes.len() {
        let parent = nodes[idx].parent.clone();
        if let Some(parent) = parent {
            if let Some(&parent_idx) = index.get(&parent) {
                nodes[parent_idx].children.push(idx);
            }
        }
    }
    let names: Vec<String> = nodes.iter().map(|node| node.name.clone()).collect();
    for node in &mut nodes {
        node.children.sort_by_key(|&child| names[child].clone());
    }
    nodes
}

pub(crate) fn run_pstree(
    mail_dir_arg: Option<&Path>,
    all: bool,
    format: PstreeFormat,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let messages = all_messages(&dir)?;
    let edges = resolve_edges(&routes, &messages);
    let snapshot = proc::SysinfoSnapshot::capture()?;
    let mut meta: BTreeMap<String, LaneMeta> = BTreeMap::new();
    let mut include: BTreeSet<String> = BTreeSet::new();
    for (name, route) in &routes {
        let pane_pid = route
            .tmux
            .as_deref()
            .and_then(|target| tmux::mux().pane_pid(None, target))
            .unwrap_or(0);
        // A registered coordinator or native runs no pane of its own, so a
        // pane probe says nothing about it and its registry row is the whole
        // evidence. The rule `route_liveness` already uses. Without it every
        // native dropped out of the tree and reappeared only as a `[gone]`
        // root above its own children, which hid the native-to-native rungs
        // the chain is built from.
        let paneless_agent =
            route.tmux.is_none() && matches!(route.kind.as_str(), "coordinator" | "native");
        let live = paneless_agent || snapshot.process(pane_pid).is_some();
        if !all && !live {
            continue;
        }
        include.insert(name.clone());
        let descendants = snapshot
            .descendants(pane_pid)
            .into_iter()
            .filter_map(|pid| {
                snapshot.process(pid).map(|info| ProcessDesc {
                    pid,
                    comm: info.name,
                })
            })
            .collect();
        meta.insert(
            name.clone(),
            LaneMeta {
                pid: pane_pid,
                state: match (paneless_agent, live) {
                    (true, _) => "registered",
                    (false, true) => "live",
                    (false, false) => "dead",
                },
                descendants,
            },
        );
    }
    let nodes = build_lane_nodes(&routes, &edges, &meta, &include);
    match format {
        PstreeFormat::Text => {
            for output in render_text(&nodes) {
                line(&output);
            }
        }
        PstreeFormat::Ndjson => {
            for output in render_ndjson(&nodes) {
                line(&output);
            }
        }
    }
    Ok(())
}

pub(crate) fn render_text(nodes: &[LaneNode]) -> Vec<String> {
    fn emit(out: &mut Vec<String>, nodes: &[LaneNode], idx: usize, depth: usize) {
        let node = &nodes[idx];
        out.push(format!(
            "{}{}",
            "  ".repeat(depth),
            match node.gone {
                true => format!("{} [gone]", node.name),
                false => {
                    let pid = if node.pid == 0 {
                        "-".to_owned()
                    } else {
                        node.pid.to_string()
                    };
                    format!(
                        "{} ({pid}) [{}]{}{}",
                        node.name,
                        node.state,
                        if node.inferred { " [inferred]" } else { "" },
                        match &node.goal {
                            Some(goal) => format!(" -- {goal}"),
                            None => String::new(),
                        }
                    )
                }
            }
        ));
        if !node.gone {
            for desc in &node.descendants {
                out.push(format!(
                    "{}  {} ({})",
                    "  ".repeat(depth + 1),
                    desc.comm,
                    desc.pid
                ));
            }
        }
        for child in &node.children {
            emit(out, nodes, *child, depth + 1);
        }
    }
    let roots: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| node.parent.is_none())
        .map(|(idx, _)| idx)
        .collect();
    let mut out = Vec::new();
    for root in roots {
        emit(&mut out, nodes, root, 0);
    }
    out
}

pub(crate) fn render_ndjson(nodes: &[LaneNode]) -> Vec<String> {
    nodes
        .iter()
        .map(|node| {
            serde_json::json!({
                "lane": node.name,
                "parent": node.parent,
                "inferred": node.inferred,
                "pid": if node.gone { None } else { Some(node.pid) },
                "state": node.state,
                "goal": node.goal,
                "children": node.descendants.iter().map(|desc| desc.pid).collect::<Vec<_>>(),
            })
            .to_string()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::testkit::{route_with, temp_mail_dir};
    use boop::bus::{self, read_routes, Route};
    use boop::proc::{ProcReader, ProcessInfo, SysinfoSnapshot};
    use boop::registry::Registry;
    use std::collections::{BTreeMap, BTreeSet};

    /// RECEIPT (native-subagent-identity). pid 34606 sat in `wait --me` for
    /// eight minutes on lane feature-cx-a4's mailbox while four rows to its
    /// native-n1d went unread: the native inherited the lane's env stamp. An
    /// env-resolved lane name with live native children is refused, naming
    /// them; a lane with no natives, a native, and a coordinator pass.
    #[test]
    fn an_env_stamp_shared_with_native_children_is_refused_not_guessed() {
        let mut routes = BTreeMap::new();
        routes.insert("feature-cx-a4".to_owned(), route_with(None));
        let mut native = route_with(Some("feature-cx-a4"));
        native.kind = "native".into();
        routes.insert("native-n1d".to_owned(), native.clone());
        routes.insert("native-n2d".to_owned(), native);
        routes.insert("feature-solo".to_owned(), route_with(None));
        let mut coordinator = route_with(None);
        coordinator.kind = "coordinator".into();
        routes.insert("claude-5".to_owned(), coordinator);

        let error = refuse_a_shared_stamp(&routes, "feature-cx-a4")
            .unwrap_err()
            .to_string();
        assert!(error.contains("native-n1d, native-n2d"), "{error}");
        assert!(error.contains("--as feature-cx-a4"), "{error}");

        assert!(refuse_a_shared_stamp(&routes, "feature-solo").is_ok());
        assert!(refuse_a_shared_stamp(&routes, "native-n1d").is_ok());
        assert!(refuse_a_shared_stamp(&routes, "claude-5").is_ok());
        assert!(refuse_a_shared_stamp(&routes, "unregistered").is_ok());
    }

    #[test]
    fn foreground_wait_owns_a_result_recipient_without_a_parent() {
        assert_eq!(
            completion_recipient(None, true, "feature-a"),
            Some("__wait__feature-a".into())
        );
        assert_eq!(
            completion_recipient(Some("coordinator"), true, "feature-a"),
            Some("coordinator".into())
        );
        assert_eq!(completion_recipient(None, false, "feature-a"), None);
    }

    /// A named harness that is not registered must be refused, never quietly
    /// swapped for the first adapter, which would be a capability lie.
    #[test]
    fn dispatch_refuses_an_unregistered_harness() {
        let registry = Registry::discover();
        let error = match resolve_dispatch_harness(&registry, Some("gemini-cli")) {
            Ok(_) => panic!("unregistered harness must be refused"),
            Err(error) => error,
        };
        let message = error.to_string();
        assert!(message.contains("gemini-cli"), "message: {message}");
        assert!(message.contains("claude"), "registered set: {message}");
        assert!(message.contains("opencode"), "registered set: {message}");
    }

    #[test]
    fn native_registration_stays_live_until_explicit_done_and_done_is_once() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let coord_name = unique_name("boop-native-coord");
        let _session = LiveTmuxSession::new(&coord_name);
        write_route(&dir, "coordinator", tmux_route(&coord_name)).unwrap();
        run_agent(AgentCmd::Register {
            name: "native-child".into(),
            kind: "native".into(),
            parent: Some("coordinator".into()),
            on_parent_death: crate::ParentDeathPolicy::Orphan,
            worktree: None,
            mail_dir: Some(dir.clone()),
        })
        .unwrap();

        let routes = read_routes(&dir).unwrap();
        let route = &routes["native-child"];
        assert_eq!(
            lane_state(
                &dir,
                "native-child",
                &Some(boop::tmux::LiveSessions::default()),
                route,
                &routes,
            ),
            "live"
        );
        assert_eq!(
            route_liveness(&dir, "native-child"),
            super::RouteLiveness::Live
        );

        run_agent(AgentCmd::Done {
            name: "native-child".into(),
            rc: 7,
            mail_dir: Some(dir.clone()),
        })
        .unwrap();
        assert!(!read_routes(&dir).unwrap().contains_key("native-child"));

        let second = run_agent(AgentCmd::Done {
            name: "native-child".into(),
            rc: 7,
            mail_dir: Some(dir.clone()),
        });
        assert!(
            second.is_err(),
            "a completed native route cannot complete twice"
        );
        let messages = bus::read_boxes(&dir)
            .unwrap()
            .into_iter()
            .flat_map(|path| bus::parse_box(&path))
            .filter(|message| message.kind == "result" && message.from == "native-child")
            .collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].to, "coordinator");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// FAIL-PRE-FIX: a pane-less coordinator/native was hardcoded `live` with
    /// no probe, so dead natives read `live` until their parent evaporated.
    /// Liveness now inherits from the parent (one hop): a live parent reads
    /// `live`, a dead parent reads `dead`, and a parentless pane-less route
    /// reads `?`.
    #[test]
    fn pane_less_route_inherits_parent_liveness() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let live_name = unique_name("boop-native-parent-live");
        let live_session = LiveTmuxSession::new(&live_name);
        write_route(&dir, "parent-live", tmux_route(&live_name)).unwrap();
        write_route(
            &dir,
            "parent-dead",
            tmux_route(&unique_name("boop-native-parent-dead")),
        )
        .unwrap();
        let routes = read_routes(&dir).unwrap();
        let live = tmux::mux().live_sessions(None);

        let native = |parent: Option<&str>| Route {
            kind: "native".into(),
            harness: None,
            tmux: None,
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: parent.map(str::to_owned),
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        };

        let child = native(Some("parent-live"));
        assert_eq!(lane_state(&dir, "child", &live, &child, &routes), "live");
        let child = native(Some("parent-dead"));
        assert_eq!(lane_state(&dir, "child", &live, &child, &routes), "dead");
        let child = native(None);
        assert_eq!(lane_state(&dir, "child", &live, &child, &routes), "?");

        drop(live_session);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (native-visibility). A tmux session no route claims is a live
    /// session boop does not manage; a route `tmux` target or route name that
    /// matches a session name is claimed and therefore not unregistered.
    #[test]
    fn unregistered_sessions_names_claimless_tmux_sessions() {
        let mut routes = BTreeMap::new();
        let mut routed = tmux_route("claimed-by-tmux");
        routed.kind = "lane".into();
        routes.insert("claimed-by-tmux".into(), routed);
        routes.insert("claimed-by-name".into(), tmux_route("other-target"));
        let live = Some(boop::tmux::LiveSessions {
            names: [
                "claimed-by-tmux".to_owned(),
                "claimed-by-name".to_owned(),
                "free-session".to_owned(),
            ]
            .into_iter()
            .collect(),
        });
        assert_eq!(
            unregistered_sessions(&routes, &live),
            vec!["free-session".to_owned()]
        );
        assert!(unregistered_sessions(&routes, &None).is_empty());
    }

    /// RECEIPT (native-visibility). A repo's linked `.claude/worktrees/agent-*`
    /// worktrees surface as native Claude subagents: a `locked` porcelain block
    /// reads `live`, an unlocked one reads `dead`.
    #[test]
    fn claude_agent_worktrees_lists_locked_and_unlocked_agents() {
        let base = std::env::temp_dir().join(format!("boop-claude-wt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(&base)
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        };
        run(&["init", "-q"]);
        run(&["config", "user.email", "t@t"]);
        run(&["config", "user.name", "t"]);
        std::fs::write(base.join("seed.txt"), "s").unwrap();
        run(&["add", "-A"]);
        run(&["commit", "-qm", "seed"]);
        std::fs::create_dir_all(base.join(".claude/worktrees")).unwrap();
        run(&["worktree", "add", ".claude/worktrees/agent-abc", "HEAD"]);
        run(&["worktree", "add", ".claude/worktrees/agent-def", "HEAD"]);
        run(&["worktree", "lock", ".claude/worktrees/agent-abc"]);

        let trees = claude_agent_worktrees(base.to_str().unwrap());
        let names = trees
            .iter()
            .map(|(name, _, _)| name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"agent-abc"), "{names:?}");
        assert!(names.contains(&"agent-def"), "{names:?}");
        for (name, path, locked) in &trees {
            assert!(path.contains("/.claude/worktrees/agent-"), "{path}");
            match name.as_str() {
                "agent-abc" => assert!(*locked, "agent-abc must be locked"),
                "agent-def" => assert!(!*locked, "agent-def must be unlocked"),
                other => panic!("unexpected worktree name {other}"),
            }
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// RECEIPT (Job 3b). A `--route-only` delete drops the lane's registry row
    /// without touching pane or tmux, so the on-exit epilogue cleans up in-pane.
    #[test]
    fn route_only_delete_drops_the_registry_row_without_tmux() {
        let dir = temp_mail_dir();
        write_route(
            &dir,
            "l",
            Route {
                kind: "lane".into(),
                harness: Some(HarnessId::Claude),
                tmux: Some("somesession".into()),
                cwd: None,
                model: None,
                mode: None,
                session_id: None,
                source_path: None,
                parent: None,
                goal: None,
                registered_at: None,
                base_sha: None,
                worktree_dir: None,
                app_server_socket: None,
            },
        )
        .unwrap();
        run_lane_delete(Some(&dir), "l", true).unwrap();
        let routes = read_routes(&dir).unwrap();
        assert!(
            !routes.contains_key("l"),
            "a finished lane must leave no registry row"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn unique_name(prefix: &str) -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        format!(
            "{prefix}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
    }

    fn tmux_route(tmux_name: &str) -> Route {
        Route {
            kind: "lane".into(),
            harness: Some(HarnessId::Claude),
            tmux: Some(tmux_name.to_owned()),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        }
    }

    /// A real session on the default tmux server, killed on drop; `lane
    /// prune` hardcodes the default server, so a "live" fixture needs one.
    struct LiveTmuxSession {
        name: String,
    }

    impl LiveTmuxSession {
        fn new(name: &str) -> Self {
            tmux::mux()
                .new_bare_session(None, name)
                .expect("tmux installed and reachable");
            LiveTmuxSession {
                name: name.to_owned(),
            }
        }
    }

    impl Drop for LiveTmuxSession {
        fn drop(&mut self) {
            let _ = tmux::mux().kill_session(None, &self.name);
        }
    }

    /// FAIL-FIRST. Before `run_lane_prune` existed this had no callee to
    /// assert against; now: a dead row is gone, a live row survives.
    #[test]
    fn prune_removes_a_dead_row_and_keeps_a_live_one() {
        let dir = temp_mail_dir();
        let live_name = unique_name("boop-prune-live");
        let _session = LiveTmuxSession::new(&live_name);
        write_route(
            &dir,
            "dead-lane",
            tmux_route(&unique_name("boop-prune-dead")),
        )
        .unwrap();
        write_route(&dir, "live-lane", tmux_route(&live_name)).unwrap();

        run_lane_prune(Some(&dir), false).unwrap();

        let routes = read_routes(&dir).unwrap();
        assert!(
            !routes.contains_key("dead-lane"),
            "a dead row must be pruned"
        );
        assert!(
            routes.contains_key("live-lane"),
            "a live row must survive prune"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. `--dry-run` reports the same rows a real run would prune but
    /// removes nothing.
    ///
    /// The live session is the tmux server, not the subject: `run_lane_prune`
    /// bails before reading a single route when no server answers, and a host
    /// with none running (a CI runner) would otherwise decide this test.
    #[test]
    fn prune_dry_run_removes_nothing() {
        let dir = temp_mail_dir();
        let _session = LiveTmuxSession::new(&unique_name("boop-prune-dryrun"));
        write_route(
            &dir,
            "dead-lane",
            tmux_route(&unique_name("boop-prune-dead")),
        )
        .unwrap();

        run_lane_prune(Some(&dir), true).unwrap();

        let routes = read_routes(&dir).unwrap();
        assert!(
            routes.contains_key("dead-lane"),
            "--dry-run must remove nothing"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// FAIL-PRE-FIX: `lane_state` only answered `live`/`dead`, so a resident
    /// lane parked on its mailbox was indistinguishable from mid-turn.
    #[test]
    fn a_parked_lane_reads_idle_while_its_pane_stays_live() {
        let dir = temp_mail_dir();
        let name = unique_name("boop-idle-lane");
        let session = LiveTmuxSession::new(&name);
        write_route(&dir, "mine", tmux_route(&name)).unwrap();
        let live = Some(boop::tmux::LiveSessions {
            names: [name.clone()].into_iter().collect(),
        });
        let route = read_routes(&dir).unwrap().remove("mine").unwrap();
        let routes = BTreeMap::new();

        assert_eq!(lane_state(&dir, "mine", &live, &route, &routes), "live");

        boop::supervise::record_residency(&dir, "mine", boop::supervise::RESIDENCY_IDLE);
        assert_eq!(lane_state(&dir, "mine", &live, &route, &routes), "idle");

        drop(session);
        let dead_live = tmux::mux().live_sessions(None);
        assert_eq!(
            lane_state(&dir, "mine", &dead_live, &route, &routes),
            "dead"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dead_reason_names_a_gone_session_with_no_pid() {
        let snapshot = SysinfoSnapshot::capture().unwrap();
        let route = tmux_route(&unique_name("boop-prune-nonexistent"));
        assert_eq!(
            dead_reason(&route, &snapshot).as_deref(),
            Some("tmux session gone, no pid recorded")
        );
    }

    #[test]
    fn dead_reason_is_none_for_a_live_session() {
        let name = unique_name("boop-prune-alive");
        let _session = LiveTmuxSession::new(&name);
        let snapshot = SysinfoSnapshot::capture().unwrap();
        assert_eq!(dead_reason(&tmux_route(&name), &snapshot), None);
    }

    #[test]
    fn dead_reason_names_no_recorded_session_when_tmux_is_absent() {
        let snapshot = SysinfoSnapshot::capture().unwrap();
        let route = Route {
            kind: "lane".into(),
            harness: Some(HarnessId::Claude),
            tmux: None,
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        };
        assert_eq!(
            dead_reason(&route, &snapshot).as_deref(),
            Some("no tmux session recorded")
        );
    }

    fn result_message(id: &str, lane: &str, rc: i32) -> boop::bus::Message {
        boop::bus::Message {
            id: id.into(),
            from: lane.into(),
            to: lane.into(),
            from_timestamp: "2026-08-01T00:00:00.000Z".into(),
            to_timestamp: None,
            kind: "result".into(),
            reply_to: None,
            body: format!("lane {lane} done rc={rc}"),
            r#ref: None,
            rc: Some(rc),
            detail: None,
        }
    }

    fn registered_route(ts: &str) -> Route {
        Route {
            kind: "lane".into(),
            harness: Some(HarnessId::Claude),
            tmux: Some("l".into()),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: Some(ts.into()),
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        }
    }

    /// RECEIPT (was wait_returns_rc_from_a_preexisting_result_row; pre-fix
    /// rc=0). Older than the spawn's registration: skipped, times out (124).
    #[test]
    fn wait_skips_a_result_row_older_than_the_current_spawn() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        append_message(&dir, &result_message("m-1", "l", 5)).unwrap();
        write_route(&dir, "l", registered_route("2026-08-02T00:00:00.000Z")).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "l",
                Some(std::time::Duration::from_millis(60)),
                std::time::Duration::from_millis(10),
            ),
            None,
            "an older row is skipped, so the wait times out (exits 124)"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A result row at or after the current spawn's registration satisfies the
    /// wait immediately with the rc its body names.
    #[test]
    fn wait_accepts_a_result_row_after_the_current_spawn() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        write_route(&dir, "l", registered_route("2026-08-01T00:00:00.000Z")).unwrap();
        let mut message = result_message("m-2", "l", 7);
        message.from_timestamp = "2026-08-02T00:00:00.000Z".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "l",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(10),
            ),
            Some(7)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (contract 3). No route row survives, so any result row
    /// satisfies: the after-the-fact read.
    #[test]
    fn wait_accepts_a_result_row_with_no_route_registered() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut message = result_message("m-3", "l", 4);
        message.from_timestamp = "2026-07-01T00:00:00.000Z".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "l",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(10),
            ),
            Some(4)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (wait-stale-result-row, 2026-08-25). Lane chore-alias-cut
    /// ended turn 1 with a question and a done row; the answer was taken as a
    /// resume turn; `boop wait chore-alias-cut` returned that stale done row
    /// while tool calls were still running. A result row older than the
    /// newest taken inbound row no longer satisfies the wait; a newer one does.
    #[test]
    fn a_done_row_older_than_the_taken_resume_row_does_not_satisfy_the_wait() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut stale = result_message("m-done-1", "chore-alias-cut", 0);
        stale.to = "claude-5".into();
        stale.from_timestamp = "2026-08-25T17:10:00.000Z".into();
        append_message(&dir, &stale).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "chore-alias-cut"), Some(0));

        let mut answer = result_message("m-answer", "claude-5", 0);
        answer.to = "chore-alias-cut".into();
        answer.kind = "request".into();
        answer.rc = None;
        answer.from_timestamp = "2026-08-25T17:11:00.000Z".into();
        answer.to_timestamp = Some("2026-08-25T17:11:01.000Z".into());
        append_message(&dir, &answer).unwrap();
        assert_eq!(
            super::wait_boundary(&dir, "chore-alias-cut"),
            parse_iso_ms("2026-08-25T17:11:01.000Z")
        );
        assert_eq!(
            super::wait_for_result(
                &dir,
                "chore-alias-cut",
                Some(std::time::Duration::from_millis(60)),
                std::time::Duration::from_millis(10),
            ),
            None,
            "the pre-resume done row must not end the wait"
        );

        let mut fresh = result_message("m-done-2", "chore-alias-cut", 7);
        fresh.to = "claude-5".into();
        fresh.from_timestamp = "2026-08-25T17:40:00.000Z".into();
        append_message(&dir, &fresh).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "chore-alias-cut",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(10),
            ),
            Some(7)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (Job 3). An empty mailbox times out to `None`, which the verb
    /// maps to exit code 124.
    #[test]
    fn wait_times_out_when_no_result_row_arrives() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let outcome = super::wait_for_result(
            &dir,
            "l",
            Some(std::time::Duration::from_millis(60)),
            std::time::Duration::from_millis(10),
        );
        assert_eq!(
            outcome, None,
            "timeout returns the None the verb exits 124 on"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A non-result row for the lane never satisfies the wait.
    #[test]
    fn a_non_result_row_does_not_satisfy_the_wait() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut message = result_message("m-2", "l", 3);
        message.kind = "note".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "l"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. The completion row is hailed `--to <parent> --from <lane>`, so a
    /// wait keyed on the recipient never saw the row it exists to wait for.
    #[test]
    fn wait_matches_the_row_the_supervisor_actually_writes() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut message = result_message("m-3", "feature-schema-emit", 0);
        message.to = "sprefa-coordinator".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "feature-schema-emit"), Some(0));
        assert_eq!(
            super::wait_for_result(
                &dir,
                "feature-schema-emit",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(10),
            ),
            Some(0)
        );
        assert_eq!(
            super::lane_result_rc(&dir, "some-other-lane"),
            None,
            "another lane's completion never satisfies this wait"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. A lane that fails hands its rc back through the same row, and
    /// an absent row is the 124 timeout `--wait-timeout` exits on.
    #[test]
    fn wait_propagates_a_failing_rc_and_times_out_otherwise() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            super::wait_for_result(
                &dir,
                "feature-schema-emit",
                Some(std::time::Duration::from_millis(40)),
                std::time::Duration::from_millis(10),
            ),
            None,
            "no result row yet: the verb exits 124 on this None"
        );
        let mut message = result_message("m-4", "feature-schema-emit", 17);
        message.to = "sprefa-coordinator".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "feature-schema-emit"), Some(17));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// FAIL-PRE-FIX: a lane whose pane evaporated left `lane wait` polling a
    /// mailbox nothing would write to, forever under `--timeout 0`.
    #[test]
    fn wait_calls_a_lane_dead_when_its_route_stops_being_live() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        write_route(&dir, "l", registered_route("2026-08-01T00:00:00.000Z")).unwrap();
        assert_eq!(
            super::wait_for_outcome(
                &dir,
                "l",
                None,
                std::time::Duration::from_millis(1),
                &|_, _| super::RouteLiveness::Dead,
            ),
            super::WaitOutcome::Died,
            "a dead route with no result row exits 3, never blocks"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Only the supervisor writes the row now, and a mailbox holding a pair
    /// from an older build still answers one rc.
    #[test]
    fn a_duplicate_result_row_leaves_the_wait_unchanged() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut supervisor = result_message("m-supervisor", "l", 2);
        supervisor.to = "sprefa-coordinator".into();
        append_message(&dir, &supervisor).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "l"), Some(2));
        let mut older_build = result_message("m-epilogue", "l", 2);
        older_build.to = "sprefa-coordinator".into();
        append_message(&dir, &older_build).unwrap();
        assert_eq!(super::lane_result_rc(&dir, "l"), Some(2));
        assert_eq!(
            super::wait_for_outcome(
                &dir,
                "l",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(1),
                &|_, _| super::RouteLiveness::Unknown,
            ),
            super::WaitOutcome::Result(2)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A lane that reported is unaffected by the liveness check: its pane is
    /// already gone by the time its row is read.
    #[test]
    fn a_result_row_beats_a_dead_route() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mut message = result_message("m-5", "l", 9);
        message.to = "sprefa-coordinator".into();
        append_message(&dir, &message).unwrap();
        assert_eq!(
            super::wait_for_outcome(
                &dir,
                "l",
                Some(std::time::Duration::from_secs(2)),
                std::time::Duration::from_millis(1),
                &|_, _| super::RouteLiveness::Dead,
            ),
            super::WaitOutcome::Result(9)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An unreachable tmux, a lane with no route and a live lane all read the
    /// same to the wait: keep polling until the deadline.
    #[test]
    fn an_undecidable_route_still_times_out_rather_than_reporting_death() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        for liveness in [super::RouteLiveness::Unknown, super::RouteLiveness::Live] {
            assert_eq!(
                super::wait_for_outcome(
                    &dir,
                    "l",
                    Some(std::time::Duration::from_millis(40)),
                    std::time::Duration::from_millis(10),
                    &|_, _| liveness,
                ),
                super::WaitOutcome::TimedOut,
                "{liveness:?} is not evidence of death"
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A route reads dead for the poll or two between registration and the
    /// session answering, which must not end the wait.
    #[test]
    fn a_single_dead_observation_does_not_end_the_wait() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let polls = std::sync::atomic::AtomicU32::new(0);
        assert_eq!(
            super::wait_for_outcome(
                &dir,
                "l",
                Some(std::time::Duration::from_millis(60)),
                std::time::Duration::from_millis(10),
                &|_, _| match polls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) {
                    0 => super::RouteLiveness::Dead,
                    _ => super::RouteLiveness::Live,
                },
            ),
            super::WaitOutcome::TimedOut
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. A registry row written before the branch-derived names (lane id
    /// with no kind, `lane/*` worktree cwd) still reads, resolves and deletes.
    #[test]
    fn a_pre_branch_registry_row_still_reads_and_deletes() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("registry.json"),
            r#"{
  "boop-sql": {
    "harness": "opencode",
    "tmux": "boop-sql",
    "cwd": "/Users/x/projects/sprefa/.boop-worktrees/lane/boop-sql",
    "model": "openrouter/deepseek/deepseek-v4-flash-0731",
    "mode": "auto",
    "sessionId": "ses_0167"
  },
  "sprefa-coordinator": { "harness": "claude", "tmux": "shell:0.0" }
}"#,
        )
        .unwrap();
        let routes = read_routes(&dir).unwrap();
        let old = &routes["boop-sql"];
        assert_eq!(old.session_id.as_deref(), Some("ses_0167"));
        assert_eq!(old.tmux.as_deref(), Some("boop-sql"));
        assert!(old
            .cwd
            .as_deref()
            .unwrap()
            .contains(".boop-worktrees/lane/"));
        assert_eq!(
            resolve_parent_with_legacy_fallback(None, None, &routes)
                .parent
                .as_deref(),
            Some("sprefa-coordinator"),
            "an old row is still a usable parent default"
        );
        run_lane_delete(Some(&dir), "boop-sql", true).unwrap();
        let after = read_routes(&dir).unwrap();
        assert!(!after.contains_key("boop-sql"));
        assert!(
            after.contains_key("sprefa-coordinator"),
            "deleting one old row leaves the others"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT (boop-coordinator-by-kind compat): a pane-less coordinator is
    /// not inferred, and the legacy fallback cannot replace that decision.
    #[test]
    fn legacy_fallback_never_overrides_an_explicit_coordinator_kind() {
        let mut routes = BTreeMap::new();
        let mut boss = route_with(None);
        boss.kind = "coordinator".into();
        boss.tmux = None;
        routes.insert("boss".into(), boss);
        let pick = resolve_parent_with_legacy_fallback(None, None, &routes);
        assert_eq!(pick.parent, None);
        assert_eq!(pick.source, "none");
    }

    fn dispatch(from: &str, to: &str) -> boop::bus::Message {
        boop::bus::Message {
            id: format!("m-{from}-{to}"),
            from: from.into(),
            to: to.into(),
            from_timestamp: "2026-01-01T00:00:00.000Z".into(),
            to_timestamp: None,
            kind: "dispatch".into(),
            reply_to: None,
            body: "".into(),
            r#ref: None,
            rc: None,
            detail: None,
        }
    }

    fn live_meta(pid: u32) -> super::LaneMeta {
        super::LaneMeta {
            pid,
            state: "live",
            descendants: vec![],
        }
    }

    /// RECEIPT (pstree). A route's explicit `--parent` wins over a mailbox
    /// dispatch edge that names a different summoner.
    #[test]
    fn explicit_parent_beats_inferred_dispatch() {
        let mut routes = BTreeMap::new();
        routes.insert("child".into(), route_with(Some("explicit")));
        let messages = vec![dispatch("mailbox", "child")];
        let edges = super::resolve_edges(&routes, &messages);
        let edge = &edges["child"];
        assert_eq!(edge.parent.as_deref(), Some("explicit"));
        assert!(!edge.inferred);
    }

    /// RECEIPT (pstree). An orphaned route infers its summoner from the FIRST
    /// dispatch row addressed to it, later rows ignored.
    #[test]
    fn orphan_infers_summoner_from_first_dispatch() {
        let mut routes = BTreeMap::new();
        routes.insert("child".into(), route_with(None));
        let messages = vec![
            dispatch("summoner1", "child"),
            dispatch("summoner2", "child"),
        ];
        let edges = super::resolve_edges(&routes, &messages);
        let edge = &edges["child"];
        assert_eq!(edge.parent.as_deref(), Some("summoner1"));
        assert!(edge.inferred);
    }

    /// RECEIPT (pstree). A summoner absent from the registry renders as a
    /// `[gone]` root with the orphan lane hung beneath it.
    #[test]
    fn orphan_root_prints_gone_summoner() {
        let mut routes = BTreeMap::new();
        routes.insert("child".into(), route_with(None));
        let messages = vec![dispatch("coordinator", "child")];
        let edges = super::resolve_edges(&routes, &messages);
        let mut meta = BTreeMap::new();
        meta.insert("child".into(), live_meta(4242));
        let mut include = BTreeSet::new();
        include.insert("child".into());
        let nodes = super::build_lane_nodes(&routes, &edges, &meta, &include);
        let text = super::render_text(&nodes);
        let joined = text.join("\n");
        assert!(joined.contains("coordinator [gone]"), "text:\n{joined}");
        assert!(
            joined.contains("child (4242) [live] [inferred]"),
            "text:\n{joined}"
        );
        let ndjson = super::render_ndjson(&nodes);
        let gone = ndjson
            .iter()
            .find(|row| row.contains("\"lane\":\"coordinator\""))
            .unwrap();
        assert!(gone.contains("\"state\":\"gone\""), "row: {gone}");
        assert!(gone.contains("\"pid\":null"), "row: {gone}");
    }

    /// RECEIPT (pstree). A true root with no parent edge stays a root and is
    /// never inferred from a non-dispatch message.
    #[test]
    fn a_lane_with_no_dispatch_shadow_is_a_root() {
        let mut routes = BTreeMap::new();
        routes.insert("loner".into(), route_with(None));
        let messages = vec![boop::bus::Message {
            kind: "note".into(),
            ..dispatch("whoever", "loner")
        }];
        let edges = super::resolve_edges(&routes, &messages);
        let edge = &edges["loner"];
        assert_eq!(edge.parent, None);
        assert!(!edge.inferred);
    }

    /// RECEIPT (job 2). A route's goal rides the lane line as a ` -- <goal>`
    /// suffix and the ndjson row as a `goal` string.
    #[test]
    fn pstree_carries_the_goal() {
        let mut routes = BTreeMap::new();
        routes.insert(
            "child".into(),
            Route {
                kind: "lane".into(),
                harness: Some(HarnessId::Opencode),
                tmux: Some("lane-x".into()),
                cwd: None,
                model: None,
                mode: None,
                session_id: None,
                source_path: None,
                parent: None,
                goal: Some("ship the edge".into()),
                registered_at: None,
                base_sha: None,
                worktree_dir: None,
                app_server_socket: None,
            },
        );
        let messages = vec![dispatch("coordinator", "child")];
        let edges = super::resolve_edges(&routes, &messages);
        let mut meta = BTreeMap::new();
        meta.insert("child".into(), live_meta(4242));
        let mut include = BTreeSet::new();
        include.insert("child".into());
        let nodes = super::build_lane_nodes(&routes, &edges, &meta, &include);
        let text = super::render_text(&nodes).join("\n");
        assert!(
            text.contains("child (4242) [live] [inferred] -- ship the edge"),
            "text:\n{text}"
        );
        let ndjson = super::render_ndjson(&nodes);
        let row = &ndjson[0];
        assert!(row.contains("\"goal\":\"ship the edge\""), "row: {row}");
    }

    /// RECEIPT (job 2). A lane without a goal renders no text suffix and a
    /// null ndjson goal.
    #[test]
    fn pstree_goal_null_when_absent() {
        let mut routes = BTreeMap::new();
        routes.insert("loner".into(), route_with(None));
        let edges = super::resolve_edges(&routes, &[]);
        let mut meta = BTreeMap::new();
        meta.insert("loner".into(), live_meta(7));
        let mut include = BTreeSet::new();
        include.insert("loner".into());
        let nodes = super::build_lane_nodes(&routes, &edges, &meta, &include);
        let text = super::render_text(&nodes).join("\n");
        assert!(!text.contains(" -- "), "text:\n{text}");
        let row = &super::render_ndjson(&nodes)[0];
        assert!(row.contains("\"goal\":null"), "row: {row}");
    }

    /// A `ProcReader` that never touches the OS; `queried` proves the caller
    /// went through the trait instead of a concrete `SysinfoSnapshot`.
    struct FakeProcReader {
        queried: std::cell::Cell<bool>,
    }

    impl ProcReader for FakeProcReader {
        fn is_alive(&self, _pid: u32) -> bool {
            self.queried.set(true);
            true
        }
        fn process(&self, pid: u32) -> Option<ProcessInfo> {
            self.queried.set(true);
            Some(ProcessInfo {
                pid,
                parent: None,
                name: "fake".into(),
                command: Vec::new(),
                rss_bytes: 4096,
                cpu_percent: 1.5,
                start_time_secs: 0,
                cwd: None,
            })
        }
        fn children(&self, _pid: u32) -> Vec<u32> {
            Vec::new()
        }
        fn descendants(&self, _pid: u32) -> Vec<u32> {
            Vec::new()
        }
        fn descendant_count(&self, _pid: u32) -> usize {
            0
        }
    }

    /// RECEIPT (boop-procreader-bypass): failed to compile pre-fix, `run_ps_with` did not exist yet.
    #[test]
    fn run_ps_with_drives_the_injected_proc_reader() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        write_route(&dir, "fake-lane", tmux_route("boop-procreader-seam-test")).unwrap();
        let reader = FakeProcReader {
            queried: std::cell::Cell::new(false),
        };
        run_ps_with(Some(&dir), None, true, &reader).unwrap();
        assert!(
            reader.queried.get(),
            "run_ps_with must query the injected ProcReader"
        );
    }
}
