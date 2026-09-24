//! `boop tui`: launch a harness's own interactive TUI, register the pane as
//! that harness's coordinator route, and project while it runs.

use std::io::Write;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use boop::bus::Route;
use boop::harness::{Harness, HarnessId, NativeTuiEvent, NativeTuiPlan, NativeTuiSpec};
use boop::registry::Registry;
use tracing::{info, warn};

use crate::cli::{line, mail_dir, pad, write_route};

/// How long a fresh TUI is given to appear in its harness's own live-session
/// registry. The route is written either way; an unresolved session leaves the
/// `sessionId` field empty rather than carrying a guess.
const SESSION_WAIT: Duration = Duration::from_secs(10);

/// How often the wrapper re-pushes its route's held mail through the door.
const DRAIN_EVERY: Duration = Duration::from_secs(5);

/// Respawn ceiling for one wrapper process; past it a dying TUI is a defect
/// to look at, never a loop to ride.
const RESPAWN_MAX: u32 = 3;

/// Below this uptime a nonzero exit is a launch defect (bad flag, bad
/// config) and a respawn would loop on it.
const RESPAWN_MIN_UPTIME: Duration = Duration::from_secs(10);

/// Whether a failed launch earns another process against the same session.
/// None denotes backend/observer failure while the frontend is still alive.
/// A frontend signal death is a deliberate kill; both gates stop restart loops.
fn respawn_wanted(
    status: Option<std::process::ExitStatus>,
    respawns: u32,
    uptime: Duration,
) -> bool {
    status.is_none_or(|status| status.code().is_some_and(|code| code != 0))
        && respawns < RESPAWN_MAX
        && uptime >= RESPAWN_MIN_UPTIME
}

/// The session this launch opened or resumed, read from an exact process or
/// pane relation in the harness's own live registry.
fn opened_session(
    adapter: &dyn Harness,
    wait: Duration,
    my_pane: &str,
    pid: u32,
    cwd: &Path,
    launched_ms: u64,
) -> Option<String> {
    let names_processes = adapter.capabilities().registry_names_processes;
    let deadline = std::time::Instant::now() + wait;
    loop {
        let live = adapter.live().live_sessions().unwrap_or_default();
        if let Some(session) =
            session_for_native_frontend(adapter.live(), &live, pid, Some(my_pane))
        {
            return Some(
                session
                    .parent_session
                    .clone()
                    .unwrap_or_else(|| session.session_id.clone()),
            );
        }
        // A harness whose registry names no process (kimi keeps only
        // transcripts) cannot match by pid or pane. It is identified by the
        // worktree the wrapper was launched in: the newest session whose cwd
        // is that directory and whose transcript appeared at or after launch.
        if !names_processes {
            if let Some(session) = session_for_cwd(&live, cwd, launched_ms) {
                return Some(
                    session
                        .parent_session
                        .clone()
                        .unwrap_or_else(|| session.session_id.clone()),
                );
            }
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Among `live` sessions in `cwd` whose newest observation is at or after
/// `launched_ms`, the most recently observed. Pid- and pane-naming harnesses
/// never reach here; a harness that names neither is found by its worktree.
fn session_for_cwd<'a>(
    live: &'a [boop::live::LiveSession],
    cwd: &Path,
    launched_ms: u64,
) -> Option<&'a boop::live::LiveSession> {
    let wanted = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    live.iter()
        .filter(|session| session.observed_ms >= launched_ms)
        .filter(|session| {
            session.cwd.as_deref().is_some_and(|held| {
                std::fs::canonicalize(held).unwrap_or_else(|_| PathBuf::from(held)) == wanted
            })
        })
        .max_by_key(|session| session.observed_ms)
}

fn session_for_pid(live: &[boop::live::LiveSession], pid: u32) -> Option<&boop::live::LiveSession> {
    let mut matches = live.iter().filter(|session| session.pid == Some(pid));
    let session = matches.next()?;
    matches.next().is_none().then_some(session)
}

/// Resolve a native frontend using an exact pid first, then the harness's
/// pane endpoint. The endpoint may use process or terminal evidence that is
/// intentionally absent from the list projection.
fn session_for_native_frontend(
    registry: &dyn boop::live::LiveSessions,
    live: &[boop::live::LiveSession],
    pid: u32,
    pane: Option<&str>,
) -> Option<boop::live::LiveSession> {
    if let Some(session) = session_for_pid(live, pid) {
        return Some(session.clone());
    }
    pane.filter(|pane| !pane.trim().is_empty())
        .and_then(|pane| registry.live_session_in_pane(pane).ok().flatten())
}

/// Holds the pane in the terminal's alternate screen for a harness whose own
/// TUI never asks for it. codex 0.151.0 parses `[tui] alternate_screen` and
/// then renders inline anyway (openai/codex#24552), so its repaints scroll into
/// tmux history and the input box walks up the pane instead of staying pinned.
/// Writing the switch here makes tmux flip the pane to its alternate buffer for
/// the child's whole life: input at the bottom, `history_size` stays 0. `Drop`
/// restores the primary screen on every exit path, bail included.
struct AlternateScreen;

impl AlternateScreen {
    fn enter(wanted: bool) -> Option<Self> {
        if !wanted {
            return None;
        }
        let mut out = std::io::stdout();
        out.write_all(b"\x1b[?1049h").ok()?;
        out.flush().ok()?;
        Some(Self)
    }
}

impl Drop for AlternateScreen {
    fn drop(&mut self) {
        let mut out = std::io::stdout();
        let _ = out.write_all(b"\x1b[?1049l");
        let _ = out.flush();
    }
}

/// Stop the launch's backend on every exit path, including a panic or an early
/// `?` between the backend start and the loop's own cleanup. The wrapper
/// registers SIGINT so a pane Ctrl-C leaves it alive; if the TUI then dies, the
/// frontend-exit path or this guard takes the backend down with it.
struct StopBackend(NativeTuiPlan);

impl Deref for StopBackend {
    type Target = NativeTuiPlan;

    fn deref(&self) -> &NativeTuiPlan {
        &self.0
    }
}

impl DerefMut for StopBackend {
    fn deref_mut(&mut self) -> &mut NativeTuiPlan {
        &mut self.0
    }
}

impl Drop for StopBackend {
    fn drop(&mut self) {
        self.0.stop();
    }
}

/// Per-wrapper observations survive a conversation close/clear, but reset
/// when the frontend process changes. PID alone cannot identify a lifetime.
struct NativeSessionHistory {
    lane: String,
    process: Option<(u32, Option<u64>)>,
    session: Option<String>,
    sequence: u64,
}

impl NativeSessionHistory {
    fn new(lane: &str) -> Self {
        Self {
            lane: lane.into(),
            process: None,
            session: None,
            sequence: 0,
        }
    }
}

/// One binding path for adapter-observed sessions and native control events.
fn bind_native_session(
    store: &boop::Store,
    route: &mut Route,
    trace: &mut Option<String>,
    history: &mut NativeSessionHistory,
    session: &str,
    pid: u32,
) -> anyhow::Result<()> {
    use boop::proc::ProcReader;
    let ts = boop::live::now_ms();
    if history.process.map(|(observed, _)| observed) != Some(pid) {
        let started = boop::proc::SysinfoSnapshot::capture()?
            .process(pid)
            .map(|process| process.start_time_secs)
            .filter(|start| *start > 0);
        history.process = Some((pid, started));
        history.session = None;
        history.sequence = 0;
    }
    let previous = history
        .session
        .as_deref()
        .filter(|previous| *previous != session);
    if let Some(previous) = previous {
        store.record_status(
            previous,
            ts,
            "detached",
            Some(i64::from(pid)),
            route.tmux.as_deref(),
        )?;
    }
    *trace = Some(
        store
            .trace_of(session)?
            .or_else(|| trace.clone())
            .unwrap_or_else(|| format!("trace-{session}")),
    );
    store.attach_trace(session, trace.as_deref().unwrap(), "native-tui-session", ts)?;
    store.set_session_attr(session, "process_pid", &pid.to_string(), ts)?;
    if let Some((_, Some(started))) = history.process {
        store.set_session_attr(session, "process_start_secs", &started.to_string(), ts)?;
        if let Some(previous) = previous {
            let detail = serde_json::json!({
                "previous_session": previous, "session": session,
                "pid": pid, "process_start_secs": started,
                "boundary": "conversation-changed"
            });
            store.record_trace_event(&boop::ident::TraceEvent {
                event_key: format!(
                    "native-session:{}:{pid}:{started}:{}",
                    history.lane, history.sequence
                ),
                lane: history.lane.clone(),
                trace: trace.clone(),
                session: Some(session.into()),
                kind: "session-boundary".into(),
                from_lane: Some(previous.into()),
                to_lane: Some(session.into()),
                started_ts: Some(ts),
                finished_ts: Some(ts),
                delivery_state: None,
                classification: Some("same-process".into()),
                detail: detail.to_string(),
                created_ts: ts,
            })?;
            store.set_session_attr(session, "process_previous_session", previous, ts)?;
            history.sequence += 1;
        }
    }
    history.session = Some(session.into());
    route.session_id = Some(session.to_owned());
    store.record_status(
        session,
        ts,
        "live",
        Some(i64::from(pid)),
        route.tmux.as_deref(),
    )
}

/// Release only the transport and process observation still owned by this
/// wrapper. A concurrent resume may already have rebound the same route.
fn release_native_route(
    store: &boop::Store,
    dir: &Path,
    name: &str,
    route: &Route,
    pid: u32,
) -> Result<()> {
    if let Some(session) = route.session_id.as_deref() {
        store.detach_process(session, pid, boop::live::now_ms())?;
    }
    boop::bus::cas_update_json(&dir.join("registry.json"), |routes| {
        if let Some(current) = routes
            .get_mut(name)
            .and_then(serde_json::Value::as_object_mut)
        {
            if current
                .get("appServerSocket")
                .and_then(serde_json::Value::as_str)
                == route.app_server_socket.as_deref()
                && current.get("sessionId").and_then(serde_json::Value::as_str)
                    == route.session_id.as_deref()
                && current
                    .get("registeredAt")
                    .and_then(serde_json::Value::as_str)
                    == route.registered_at.as_deref()
            {
                current.remove("appServerSocket");
            }
        }
        Ok(())
    })
}

/// Apply only observed session transitions; late settings from a previous
/// thread cannot change the route that now names a new thread.
fn apply_native_event(
    store: &boop::Store,
    route: &mut Route,
    trace: &mut Option<String>,
    history: &mut NativeSessionHistory,
    event: NativeTuiEvent,
    pid: u32,
) -> Result<()> {
    let ts = boop::live::now_ms();
    let (session_id, model, effort) = match event {
        NativeTuiEvent::Session {
            session_id,
            model,
            effort,
        } => {
            bind_native_session(store, route, trace, history, &session_id, pid)?;
            (session_id, model, effort)
        }
        NativeTuiEvent::Settings {
            session_id,
            model,
            effort,
        } if route.session_id.as_deref() == Some(&session_id) => (session_id, model, effort),
        NativeTuiEvent::Closed { session_id }
            if route.session_id.as_deref() == Some(&session_id) =>
        {
            store.record_status(
                &session_id,
                ts,
                "closed",
                Some(i64::from(pid)),
                route.tmux.as_deref(),
            )?;
            route.session_id = None;
            route.model = None;
            return Ok(());
        }
        NativeTuiEvent::Failed(error) => anyhow::bail!("native TUI observation failed: {error}"),
        _ => return Ok(()),
    };
    route.model = model;
    match effort {
        Some(effort) => store.set_session_attr(&session_id, "effort", &effort, ts)?,
        None => {
            store.clear_session_attr(&session_id, "effort")?;
        }
    }
    Ok(())
}

/// Ask the selected harness adapter to prepare its native process, register
/// this pane as its coordinator, then run the ordinary interactive TUI.
pub(crate) fn native_route_name(harness: &str) -> String {
    match std::env::var("TMUX_PANE")
        .ok()
        .filter(|pane| !pane.is_empty())
    {
        Some(pane) => format!("{harness}-{}", pane.trim_start_matches('%')),
        None => format!("{harness}-process-{}", std::process::id()),
    }
}

pub(crate) fn run_native_tui(
    registry: &Registry,
    adapter: &dyn Harness,
    name: Option<&str>,
    cwd: &Path,
    mail_dir_arg: Option<&Path>,
    executable: Option<&str>,
    initial_prompt: Option<&str>,
    initial_effort: Option<&str>,
    tui_args: &[String],
) -> Result<()> {
    let executable = executable.unwrap_or(adapter.id().as_str());
    if !adapter.uses_native_tui(tui_args) {
        use std::os::unix::process::CommandExt;
        return Err(Command::new(executable)
            .args(tui_args)
            .current_dir(cwd)
            .exec())
        .with_context(|| format!("execute {executable}"));
    }
    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGHUP,
        signal_hook::consts::SIGTERM,
        // Registering SIGINT only overrides the wrapper's default kill. The
        // TUI shares the pane's process group, so the same Ctrl-C reaches it;
        // the wrapper ignores its copy and stops the backend once the TUI exits.
        signal_hook::consts::SIGINT,
    ])?;
    let pane = std::env::var("TMUX_PANE")
        .ok()
        .filter(|pane| !pane.is_empty());
    anyhow::ensure!(
        initial_prompt.is_none() || pane.is_some(),
        "--initial-prompt requires a tmux pane"
    );
    let default_name = native_route_name(adapter.id().as_str());
    let name = name.unwrap_or(&default_name);
    let dir = mail_dir(mail_dir_arg)?;
    let tui_trail_root = boop::trail::lanes_root().ok();
    let store = boop::bus::open_store(&dir)?;
    let existing = boop::bus::read_routes(&dir)?.remove(name);
    if let Some(existing) = &existing {
        anyhow::ensure!(
            existing.kind != "lane",
            "route {name} belongs to a lane supervisor"
        );
        if let Some(owner) = live_session_owner(registry, &dir, name, existing)? {
            anyhow::bail!("route {name} is live: {owner}");
        }
    }
    let _ownership = boop::bus::try_route_lock(&boop::bus::db_path(&dir)?, name, "native-tui")?
        .with_context(|| format!("route {name} already has a native TUI wrapper"))?;
    let parent = existing
        .as_ref()
        .and_then(|route| route.parent.clone())
        .or_else(|| {
            boop::identity::from_env()
                .and_then(|identity| identity.session)
                .filter(|caller| caller != name)
        });
    let spec = NativeTuiSpec {
        executable: executable.into(),
        cwd: cwd.to_path_buf(),
        args: tui_args.to_vec(),
        env: vec![
            ("BOOP_SESSION".into(), name.into()),
            ("BOOP_LANE".into(), name.into()),
            ("BOOP_HARNESS".into(), adapter.id().as_str().into()),
            ("BOOP_PARENT".into(), parent.clone().unwrap_or_default()),
            ("BOOP_MAIL_DIR".into(), dir.display().to_string()),
            (crate::cli::TUI_PANE_ENV.into(), name.into()),
            (
                "BOOP_DB".into(),
                boop::bus::db_path(&dir)?.display().to_string(),
            ),
        ],
    };
    let launch_started = std::time::Instant::now();
    let mut plan = StopBackend(adapter.door().tui_launch(&spec)?);
    if let Some(effort) = initial_effort {
        let mut settings_route = existing
            .clone()
            .context("--initial-effort requires a registered fork route")?;
        settings_route.session_id = plan.session_id.clone();
        settings_route.app_server_socket = plan.app_server_socket.clone();
        let model = settings_route
            .model
            .as_deref()
            .context("fork route has no model")?;
        adapter
            .door()
            .change_native_settings(&settings_route, model, effort)?;
    }
    let launch_ms = launch_started.elapsed().as_millis() as u64;
    if launch_ms >= 2_000 {
        tracing::warn!(harness = %adapter.id(), elapsed_ms = launch_ms, "slow native TUI launch (backend start before the screen)");
    }
    let known_started = std::time::Instant::now();
    let mut known = adapter
        .capabilities()
        .native_tui_projector
        .then(|| store.known_sessions())
        .transpose()?;
    let known_ms = known_started.elapsed().as_millis() as u64;
    if known_ms >= 1_000 {
        tracing::warn!(
            elapsed_ms = known_ms,
            "slow known-session read before the TUI screen"
        );
    }
    let _alternate_screen =
        AlternateScreen::enter(adapter.capabilities().wrapper_owns_alternate_screen);
    // A harness that names no process in its registry (kimi) is bound by the
    // transcript this launch writes, so remember the instant the pane opened.
    let launched_ms = boop::live::now_ms();
    // The stamp every `boop` call inside this TUI reads as its identity,
    // inherited by the harness's own shell and native subagents.
    plan.frontend = Some(
        Command::new(&plan.program)
            .args(&plan.args)
            .envs(spec.env.iter().cloned())
            .current_dir(cwd)
            .spawn()
            .with_context(|| format!("start native {} TUI", adapter.id()))?,
    );
    let mut frontend_pid = plan.frontend.as_ref().unwrap().id();
    let mut respawns: u32 = 0;
    let mut spawned_at = std::time::Instant::now();
    if plan.session_id.is_none() && plan.observer.is_none() {
        plan.session_id = opened_session(
            adapter,
            SESSION_WAIT,
            pane.as_deref().unwrap_or(""),
            frontend_pid,
            cwd,
            launched_ms,
        );
        if let Some(session) = plan.session_id.clone() {
            plan.source_path = Some(format!("native-session={session}"));
            info!(%session, harness = %adapter.id(), "native session route resolved");
        }
    }
    let mut route = Route {
        kind: "coordinator".into(),
        harness: Some(adapter.id()),
        tmux: pane.clone(),
        cwd: Some(cwd.display().to_string()),
        model: existing.as_ref().and_then(|route| route.model.clone()),
        mode: Some(plan.mode.clone()),
        session_id: plan.session_id.clone(),
        source_path: stamp_executable(plan.source_path.clone(), executable),
        parent,
        goal: existing.as_ref().and_then(|route| route.goal.clone()),
        registered_at: existing
            .as_ref()
            .and_then(|route| route.registered_at.clone())
            .or_else(|| Some(boop::bus::now_iso())),
        base_sha: existing.as_ref().and_then(|route| route.base_sha.clone()),
        worktree_dir: existing
            .as_ref()
            .and_then(|route| route.worktree_dir.clone()),
        app_server_socket: plan.app_server_socket.clone(),
    };
    let mut trace = store.trace_of(name)?;
    let mut history = NativeSessionHistory::new(name);
    if let Some(session) = route.session_id.clone() {
        bind_native_session(
            &store,
            &mut route,
            &mut trace,
            &mut history,
            &session,
            frontend_pid,
        )?;
    }
    write_route(&dir, name, route.clone())?;
    // Child exit observation stays responsive while transcript projection is
    // independently bounded. The global known-session join ran once above;
    // every pass below reuses and incrementally updates that resident cache.
    const EXIT_POLL: Duration = Duration::from_millis(250);
    let parent_project_every = std::env::var("BOOP_NATIVE_PROJECT_EVERY_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(1));
    let discover_every = std::env::var("BOOP_NATIVE_DISCOVER_EVERY_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(30));
    let mut last_parent_project = std::time::Instant::now() - parent_project_every;
    let mut last_discover = std::time::Instant::now() - discover_every;
    let mut last_drain = std::time::Instant::now() - DRAIN_EVERY;
    let mut pending_prompt = initial_prompt;
    let outcome = (|| -> Result<()> {
        loop {
            if let (Some(prompt), Some(pane)) = (pending_prompt, pane.as_deref()) {
                let screen = boop::tmux::mux().capture_pane(None, pane, None)?;
                let rows = screen.lines().collect::<Vec<_>>();
                if adapter.terminal_input_region(&rows).is_some() {
                    crate::cli::paste::send_keys(pane, &[prompt], true)?;
                    crate::cli::paste::send_keys(pane, &["Enter"], false)?;
                    pending_prompt = None;
                }
            }
            if let Some(signal) = signals.pending().next() {
                // SIGINT is the pane's Ctrl-C, meant for the TUI child. The
                // wrapper's copy is dropped; SIGHUP and SIGTERM still end it.
                if signal != signal_hook::consts::SIGINT {
                    anyhow::bail!("native TUI stopped by signal {signal}");
                }
            }
            let mut observation_failure = None;
            if let Some(observer) = plan.observer.as_ref() {
                for event in observer.events.try_iter() {
                    if let NativeTuiEvent::Failed(error) = event {
                        observation_failure = Some(error);
                    } else {
                        apply_native_event(
                            &store,
                            &mut route,
                            &mut trace,
                            &mut history,
                            event,
                            frontend_pid,
                        )?;
                        boop::bus::update_native_route(&store, name, &mut route)?;
                    }
                }
            }
            let frontend_exit = plan
                .frontend
                .as_mut()
                .unwrap()
                .try_wait()
                .context("observe native TUI exit")?;
            let backend_exit = plan
                .backend
                .as_mut()
                .map(|child| child.try_wait())
                .transpose()
                .context("observe native backend exit")?
                .flatten();
            if frontend_exit.is_some() || backend_exit.is_some() || observation_failure.is_some() {
                if frontend_exit.is_some_and(|status| status.success()) {
                    return Ok(());
                }
                let next = match route.session_id.as_deref() {
                    Some(session)
                        if respawn_wanted(frontend_exit, respawns, spawned_at.elapsed()) =>
                    {
                        plan.stop();
                        store.detach_process(session, frontend_pid, boop::live::now_ms())?;
                        adapter.door().tui_relaunch(
                            &spec,
                            session,
                            route.model.as_deref(),
                            store.session_attr(session, "effort")?.as_deref(),
                        )?
                    }
                    _ => None,
                };
                let Some(mut next) = next else {
                    anyhow::bail!("native {} launch ended: frontend={frontend_exit:?}, backend={backend_exit:?}, observer={observation_failure:?}", adapter.id());
                };
                respawns += 1;
                warn!(
                    route = name,
                    ?frontend_exit,
                    ?backend_exit,
                    ?observation_failure,
                    attempt = respawns,
                    "native TUI died; respawning against its session"
                );
                next.frontend = Some(
                    Command::new(&next.program)
                        .args(&next.args)
                        .envs(spec.env.iter().cloned())
                        .current_dir(cwd)
                        .spawn()
                        .with_context(|| format!("respawn native {} TUI", adapter.id()))?,
                );
                frontend_pid = next.frontend.as_ref().unwrap().id();
                history = NativeSessionHistory::new(name);
                spawned_at = std::time::Instant::now();
                route.app_server_socket = next.app_server_socket.clone();
                route.source_path = stamp_executable(next.source_path.clone(), executable);
                route.session_id = next.session_id.clone();
                *plan = next;
                boop::bus::update_native_route(&store, name, &mut route)?;
                continue;
            }
            // A fresh TUI opens its session at its first prompt, after the route
            // was written; the route learns the id the first tick it exists.
            if route.session_id.is_none() && plan.observer.is_none() {
                match opened_session(
                    adapter,
                    Duration::ZERO,
                    pane.as_deref().unwrap_or(""),
                    frontend_pid,
                    cwd,
                    launched_ms,
                ) {
                    Some(session) => {
                        route.source_path = Some(format!("native-session={session}"));
                        bind_native_session(
                            &store,
                            &mut route,
                            &mut trace,
                            &mut history,
                            &session,
                            frontend_pid,
                        )?;
                        boop::bus::update_native_route(&store, name, &mut route)?;
                        info!(route = name, %session, "native session route recovered after launch");
                    }
                    None => {
                        std::thread::sleep(EXIT_POLL);
                        continue;
                    }
                }
            }
            // Held mail pushes itself: rows parked before this pane had a
            // session go out the door on the first tick after one binds.
            if route.session_id.is_some() && last_drain.elapsed() >= DRAIN_EVERY {
                last_drain = std::time::Instant::now();
                let pushed = boop::mail::drain_route_held_mail(&dir, registry, &store, name);
                if pushed > 0 {
                    info!(route = name, pushed, "held mail drained through the door");
                }
            }
            if plan.observer.is_none() && last_parent_project.elapsed() >= parent_project_every {
                // Native registries that identify the process also identify a
                // clear/new transition within that process. Cwd and pane alone
                // cannot distinguish concurrent conversations.
                let observed = adapter.live().live_sessions()?;
                if let Some(session) = session_for_native_frontend(
                    adapter.live(),
                    &observed,
                    frontend_pid,
                    pane.as_deref(),
                ) {
                    if route.session_id.as_deref() != Some(session.session_id.as_str()) {
                        bind_native_session(
                            &store,
                            &mut route,
                            &mut trace,
                            &mut history,
                            &session.session_id,
                            frontend_pid,
                        )?;
                        route.model = None;
                        route.source_path = Some(format!("native-session={}", session.session_id));
                        boop::bus::update_native_route(&store, name, &mut route)?;
                    }
                }
                if let Some(session) = route
                    .session_id
                    .as_deref()
                    .and_then(|id| adapter.session_by_id(id, route.cwd.as_deref()))
                {
                    if let Some(NativeTuiEvent::Settings {
                        session_id,
                        model,
                        effort,
                    }) = adapter.native_settings(&session)
                    {
                        if route.model != model
                            || store.session_attr(&session_id, "effort")? != effort
                        {
                            apply_native_event(
                                &store,
                                &mut route,
                                &mut trace,
                                &mut history,
                                NativeTuiEvent::Settings {
                                    session_id,
                                    model,
                                    effort,
                                },
                                frontend_pid,
                            )?;
                            boop::bus::update_native_route(&store, name, &mut route)?;
                        }
                    }
                }
            }
            if let Some(known) = known.as_mut() {
                if last_discover.elapsed() >= discover_every {
                    last_discover = std::time::Instant::now();
                    if let Err(error) = crate::cli::db::sync_native_child_route_once(
                        &store,
                        known,
                        adapter,
                        name,
                        &dir,
                        |message| {
                            crate::cli::mail::deliver_hail_to_tui_trail(
                                registry,
                                &dir,
                                message,
                                tui_trail_root.as_deref(),
                                name,
                            )
                        },
                    ) {
                        warn!(%error, route = name, "native discovery projector pass failed");
                    }
                }
                if last_parent_project.elapsed() >= parent_project_every {
                    if let Err(error) = crate::cli::db::sync_native_parent_route_once(
                        &store, known, adapter, name, &dir,
                    ) {
                        warn!(%error, route = name, "native parent projector pass failed");
                    }
                }
            }
            if last_parent_project.elapsed() >= parent_project_every {
                last_parent_project = std::time::Instant::now();
            }
            std::thread::sleep(EXIT_POLL);
        }
    })();
    drop(plan);
    let cleanup = release_native_route(&store, &dir, name, &route, frontend_pid);
    if let Err(error) = &cleanup {
        warn!(%error, route = name, "native route cleanup failed");
    }
    outcome.and(cleanup)
}

// --- revive: a coordinator pane killed without /exit -----------------------

/// How long a revived pane has to re-register its route on the same session.
const REVIVE_WAIT: Duration = Duration::from_secs(60);

/// How often the revive wait re-reads the route.
const REVIVE_POLL: Duration = Duration::from_millis(200);

/// How long the dying wrapper has to drop its route lock.
const LOCK_WAIT: Duration = Duration::from_secs(15);

/// The `source_path` field naming the executable a native TUI ran as.
const EXECUTABLE_FIELD: &str = "native-executable=";

/// Every route records the binary its pane ran, so a revive spawns `ccz` again
/// and not plain `claude`. A door that wrote the field keeps its spelling.
fn stamp_executable(source_path: Option<String>, executable: &str) -> Option<String> {
    let field = format!("{EXECUTABLE_FIELD}{executable}");
    match source_path {
        Some(path)
            if path
                .split(';')
                .any(|part| part.starts_with(EXECUTABLE_FIELD)) =>
        {
            Some(path)
        }
        Some(path) => Some(format!("{field};{path}")),
        None => Some(field),
    }
}

/// The executable a route's pane ran as; `None` when it recorded none.
pub(crate) fn route_executable(source_path: Option<&str>) -> Option<&str> {
    source_path?
        .split(';')
        .find_map(|part| part.strip_prefix(EXECUTABLE_FIELD))
        .filter(|executable| !executable.is_empty())
}

/// Why a route cannot come back, `None` when it can. A revive replays `boop tui
/// <harness> --cwd <cwd> -- <resume args>` and needs all three fields.
pub(crate) fn revive_blocker(route: &Route) -> Option<&'static str> {
    if route.kind != "coordinator" {
        return Some("not a coordinator route");
    }
    if route.harness.is_none() {
        return Some("no harness recorded");
    }
    if route.session_id.as_deref().is_none_or(str::is_empty) {
        return Some("no session id recorded");
    }
    if route.cwd.as_deref().is_none_or(str::is_empty) {
        return Some("no cwd recorded");
    }
    None
}

/// Whether a dead row earns the REVIVABLE mark in `lane list`.
pub(crate) fn revivable(route: &Route) -> bool {
    revive_blocker(route).is_none()
}

/// The one command a revived coordinator pane runs, the hand-typed recovery of
/// 2026-09-12 as code. `--mail-dir` is explicit so the pane cannot drift stores.
pub(crate) fn revive_command(
    boop: &str,
    harness: HarnessId,
    name: &str,
    cwd: &str,
    executable: Option<&str>,
    mail_dir: &Path,
    resume: &[String],
) -> String {
    use boop::harness::shell_quote;
    let mut command = format!(
        "{} tui {} --name {} --cwd {} --mail-dir {}",
        shell_quote(boop),
        harness.as_str(),
        shell_quote(name),
        shell_quote(cwd),
        shell_quote(&mail_dir.display().to_string()),
    );
    if let Some(executable) = executable {
        command.push_str(&format!(" --bin {}", shell_quote(executable)));
    }
    command.push_str(" --");
    for argument in resume {
        command.push(' ');
        command.push_str(&shell_quote(argument));
    }
    command
}

/// Wrapper locks are released by the kernel on exit and reboot. A persisted
/// pid or tmux pane id can belong to an unrelated process after either event.
pub(crate) fn live_session_owner(
    registry: &Registry,
    dir: &Path,
    name: &str,
    route: &Route,
) -> Result<Option<String>> {
    let db = boop::bus::db_path(dir)?;
    if boop::bus::try_route_lock(&db, name, "native-tui")?.is_none() {
        return Ok(Some("native TUI wrapper holds the route lock".into()));
    }
    // A conversation may have been resumed under another route name.
    for (other_name, other) in boop::bus::read_routes(dir)? {
        if other_name != name
            && route.session_id.is_some()
            && other.harness == route.harness
            && other.session_id == route.session_id
            && boop::bus::try_route_lock(&db, &other_name, "native-tui")?.is_none()
        {
            return Ok(Some(format!("native TUI wrapper holds route {other_name}")));
        }
    }
    let (Some(harness), Some(session)) = (route.harness, route.session_id.as_deref()) else {
        return Ok(None);
    };
    // Native registries that supply a process can also own a conversation
    // outside boop. Codex's historical thread rows have no pid and do not
    // establish ownership merely by existing.
    Ok(registry
        .get(harness)
        .live()
        .live_sessions()?
        .into_iter()
        .find(|live| live.session_id == session && live.pid.is_some_and(boop::live::pid_alive))
        .and_then(|live| live.pid)
        .map(|pid| format!("harness session runs as process {pid}")))
}

/// Width of every message cell in the `--dead` table.
const CELL: usize = 60;

/// claude writes an explicit `/exit` as a user row. Codex rollouts and the
/// opencode session table record no exit at all (searched 2026-09-12).
const EXIT_COMMAND: &str = "<command-name>/exit";

/// A user row the harness wrote itself: a slash command or codex's context
/// preamble. Never what the operator typed, so never a table cell.
const HARNESS_WRITTEN: [&str; 5] = [
    "<command-name>",
    "<command-message>",
    "<local-command-stdout>",
    "<environment_context>",
    "<user_instructions>",
];

/// What one session's transcript says: the three messages the table shows, and
/// whether the pane was closed on purpose.
#[derive(Clone, Default)]
pub(crate) struct SessionDigest {
    pub(crate) started: String,
    pub(crate) last_user: String,
    pub(crate) last_bot: String,
    pub(crate) exited: bool,
}

/// One offered row: a dead coordinator route that can come back.
pub(crate) struct ReviveCandidate {
    pub(crate) name: String,
    pub(crate) route: Route,
    pub(crate) harness: HarnessId,
    pub(crate) session: String,
    pub(crate) cwd: String,
    pub(crate) last_activity_ms: u64,
    pub(crate) digest: SessionDigest,
}

/// The three messages and the exit verdict, from projected turns in store
/// order. A `<local-command-stdout>` row is a command's own echo, never a pick.
pub(crate) fn digest_from_turns(rows: &[serde_json::Value]) -> SessionDigest {
    let mut digest = SessionDigest::default();
    let mut last_user_row = String::new();
    for row in rows {
        let role = row["role"].as_str().unwrap_or_default();
        let said = row["said"].as_str().unwrap_or_default().trim();
        if said.is_empty() {
            continue;
        }
        match role {
            "user" => {
                if said.starts_with("<local-command-stdout>") {
                    continue;
                }
                last_user_row = said.to_owned();
                if HARNESS_WRITTEN
                    .iter()
                    .any(|prefix| said.starts_with(prefix))
                {
                    continue;
                }
                if digest.started.is_empty() {
                    digest.started = said.to_owned();
                }
                digest.last_user = said.to_owned();
            }
            "assistant" => digest.last_bot = said.to_owned(),
            _ => {}
        }
    }
    digest.exited = last_user_row.starts_with(EXIT_COMMAND);
    digest
}

/// The transcript's own words, through the harness reader and projected turns;
/// the CLI parses no transcript. Second value: the transcript mtime.
fn session_digest(
    store: &boop::Store,
    known: &boop::harness::KnownSessions,
    adapter: &dyn Harness,
    session: &str,
    listed: &mut Option<Vec<boop::harness::SessionRef>>,
) -> (SessionDigest, Option<u64>) {
    let mut modified_ms = None;
    // A pane killed in its first minute was never projected, so the store knows
    // no cursor for it; the harness's own session list still names it.
    let reference = adapter
        .sync_candidate(known, session)
        .ok()
        .flatten()
        .or_else(|| {
            listed
                .get_or_insert_with(|| adapter.sessions().unwrap_or_default())
                .iter()
                .find(|candidate| candidate.session_id == session)
                .cloned()
        });
    if let Some(reference) = reference {
        modified_ms = Some(reference.modified_ms);
        if let Err(error) = boop::harness::sync_session(store, adapter, &reference) {
            warn!(%error, session, "revive table could not project the transcript");
        }
    }
    let rows = store
        .query_turns(&boop::ident::TurnQuery {
            session: Some(session.to_owned()),
            ..boop::ident::TurnQuery::default()
        })
        .unwrap_or_default();
    (digest_from_turns(&rows), modified_ms)
}

/// Every dead coordinator route worth offering after a tmux server death. A
/// lane is never offered: a revived coordinator revives its own lanes.
pub(crate) fn revive_candidates(
    registry: &Registry,
    dir: &Path,
    _socket: Option<&str>,
    since: Duration,
    report_skips: bool,
) -> Result<Vec<ReviveCandidate>> {
    let routes = boop::bus::read_routes(dir)?;
    let newest_mail = crate::cli::job::newest_lane_activity(&boop::bus::read_messages(dir)?);
    let store = boop::bus::open_store(dir)?;
    let known = store.known_sessions()?;
    let now = boop::live::now_ms();
    let window = since.as_millis() as u64;
    let mut out = Vec::new();
    let mut listed: std::collections::HashMap<HarnessId, Option<Vec<boop::harness::SessionRef>>> =
        std::collections::HashMap::new();
    for (name, route) in &routes {
        if route.kind != "coordinator" {
            continue;
        }
        if let Some(blocker) = revive_blocker(route) {
            if report_skips {
                println!("skip {name}: {blocker}");
            }
            continue;
        }
        let (Some(harness), Some(session), Some(cwd)) =
            (route.harness, route.session_id.clone(), route.cwd.clone())
        else {
            continue;
        };
        if let Some(owner) = live_session_owner(registry, dir, name, route)? {
            if report_skips {
                println!("skip {name}: {owner}");
            }
            continue;
        }
        let (digest, transcript_ms) = session_digest(
            &store,
            &known,
            registry.get(harness),
            &session,
            listed.entry(harness).or_default(),
        );
        // A registered conversation remains recoverable before its first
        // human turn or when transcript projection has not caught up.
        let last_activity_ms = [
            route
                .registered_at
                .as_deref()
                .and_then(crate::cli::job::parse_iso_ms),
            newest_mail.get(name).copied(),
            transcript_ms,
        ]
        .into_iter()
        .flatten()
        .max()
        .unwrap_or(0);
        if now.saturating_sub(last_activity_ms) > window {
            continue;
        }
        out.push(ReviveCandidate {
            name: name.clone(),
            route: route.clone(),
            harness,
            session,
            cwd,
            last_activity_ms,
            digest,
        });
    }
    out.sort_by(|left, right| {
        right
            .last_activity_ms
            .cmp(&left.last_activity_ms)
            .then(left.name.cmp(&right.name))
    });
    // One row per session: a session re-registered under a second route name
    // (a hand-typed resume) keeps its newest route only.
    let mut seen = std::collections::HashSet::new();
    out.retain(|candidate| seen.insert(candidate.session.clone()));
    Ok(out)
}

/// One cell: whitespace flattened, cut to `width` with a visible cut mark.
pub(crate) fn cut(text: &str, width: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= width {
        return flat;
    }
    let kept: String = flat.chars().take(width.saturating_sub(3)).collect();
    format!("{kept}...")
}

/// How long ago, in the coarsest unit that still reads true.
pub(crate) fn age(elapsed_ms: u64) -> String {
    let seconds = elapsed_ms / 1000;
    match seconds {
        0..=90 => format!("{seconds}s"),
        91..=5400 => format!("{}m", seconds / 60),
        5401..=172_800 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86_400),
    }
}

/// The table the operator picks from.
fn print_candidates(candidates: &[ReviveCandidate], now: u64) {
    line(&format!(
        "{} {} {} {} {} {} {} {}",
        pad("#", 3),
        pad("name", 20),
        pad("harness", 8),
        pad("cwd", 40),
        pad("started", CELL),
        pad("last user", CELL),
        pad("last bot", CELL),
        "age",
    ));
    for (index, candidate) in candidates.iter().enumerate() {
        line(&format!(
            "{} {} {} {} {} {} {} {}",
            pad(&format!("{}", index + 1), 3),
            pad(&cut(&candidate.name, 20), 20),
            pad(candidate.harness.as_str(), 8),
            pad(&cut(&candidate.cwd, 40), 40),
            pad(&cut(&candidate.digest.started, CELL), CELL),
            pad(&cut(&candidate.digest.last_user, CELL), CELL),
            pad(&cut(&candidate.digest.last_bot, CELL), CELL),
            format!(
                "{}{}",
                age(now.saturating_sub(candidate.last_activity_ms)),
                if candidate.digest.exited {
                    " exited"
                } else {
                    ""
                }
            ),
        ));
    }
}

/// The same rows as JSON, uncut, for a reader that renders its own table.
fn candidates_json(candidates: &[ReviveCandidate], now: u64) -> Result<String> {
    let rows: Vec<serde_json::Value> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            serde_json::json!({
                "n": index + 1,
                "name": candidate.name,
                "harness": candidate.harness.as_str(),
                "cwd": candidate.cwd,
                "session_id": candidate.session,
                "started": candidate.digest.started,
                "last_user": candidate.digest.last_user,
                "last_bot": candidate.digest.last_bot,
                "exited": candidate.digest.exited,
                "last_activity_ms": candidate.last_activity_ms,
                "age": age(now.saturating_sub(candidate.last_activity_ms)),
            })
        })
        .collect();
    Ok(serde_json::to_string_pretty(&rows)?)
}

/// The operator's answer to `revive [all|1,3,5|none]:`, as row indexes.
pub(crate) fn parse_selection(answer: &str, count: usize) -> Result<Vec<usize>> {
    let answer = answer.trim();
    if answer.is_empty() || answer.eq_ignore_ascii_case("none") {
        return Ok(Vec::new());
    }
    if answer.eq_ignore_ascii_case("all") {
        return Ok((0..count).collect());
    }
    let mut picked = Vec::new();
    for token in answer
        .split([',', ' '])
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let index: usize = token
            .parse()
            .map_err(|_| anyhow::anyhow!("`{token}` is not a row number, `all` or `none`"))?;
        anyhow::ensure!(
            (1..=count).contains(&index),
            "row {index} is not one of 1..={count}"
        );
        if !picked.contains(&(index - 1)) {
            picked.push(index - 1);
        }
    }
    Ok(picked)
}

/// Ask once, on the terminal the operator is standing at.
fn read_selection(count: usize) -> Result<Vec<usize>> {
    use std::io::BufRead;
    print!("revive [all|1,3,5|none]: ");
    std::io::stdout().flush().ok();
    let mut answer = String::new();
    if std::io::stdin().lock().read_line(&mut answer)? == 0 {
        return Ok(Vec::new());
    }
    parse_selection(&answer, count)
}

/// `boop beep lane revive`: bring back a coordinator pane that died without an
/// `/exit`, on the conversation its route still names.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_lane_revive(
    registry: &Registry,
    name: Option<&str>,
    dead: bool,
    list: bool,
    json: bool,
    yes: bool,
    since: &str,
    socket: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    if let Some(name) = name {
        let routes = boop::bus::read_routes(&dir)?;
        let route = routes
            .get(name)
            .with_context(|| format!("no route named {name}"))?;
        if let Some(blocker) = revive_blocker(route) {
            anyhow::bail!("route {name} cannot revive: {blocker}");
        }
        return revive_route(registry, &dir, name, route, socket);
    }
    anyhow::ensure!(
        dead || list,
        "name a route to revive, or pass --dead (or --list to only look)"
    );
    let window = boop::debug::parse_window(since)?;
    let candidates = revive_candidates(registry, &dir, socket, window, dead && !list)?;
    let now = boop::live::now_ms();
    if json {
        println!("{}", candidates_json(&candidates, now)?);
        return Ok(());
    }
    if candidates.is_empty() {
        println!(
            "no dead coordinator route to revive (coordinator kind, harness and session and cwd set, active within {since})"
        );
        return Ok(());
    }
    print_candidates(&candidates, now);
    if list {
        return Ok(());
    }
    let picked = match yes {
        true => (0..candidates.len()).collect(),
        false => read_selection(candidates.len())?,
    };
    if picked.is_empty() {
        println!("nothing revived");
        return Ok(());
    }
    let mut failures = Vec::new();
    for index in picked {
        let candidate = &candidates[index];
        if let Err(error) = revive_route(registry, &dir, &candidate.name, &candidate.route, socket)
        {
            println!("failed {}: {error}", candidate.name);
            failures.push(candidate.name.clone());
        }
    }
    anyhow::ensure!(
        failures.is_empty(),
        "revive failed for {}",
        failures.join(", ")
    );
    Ok(())
}

/// Spawn one route's pane again and wait for it to rebind its own session.
fn revive_route(
    registry: &Registry,
    dir: &Path,
    name: &str,
    route: &Route,
    socket: Option<&str>,
) -> Result<()> {
    let harness = route.harness.context("route records no harness")?;
    let session = route
        .session_id
        .as_deref()
        .context("route records no session id")?;
    let cwd = route.cwd.as_deref().context("route records no cwd")?;
    if let Some(owner) = live_session_owner(registry, dir, name, route)? {
        anyhow::bail!("route {name} is live: {owner}; stop it before reviving");
    }
    let resume = registry
        .get(harness)
        .door()
        .tui_resume_args(session)
        .with_context(|| format!("harness {harness} names no TUI resume arguments"))?;
    let boop = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "boop".to_owned());
    let command = revive_command(
        &boop,
        harness,
        name,
        cwd,
        route_executable(route.source_path.as_deref()),
        dir,
        &resume,
    );
    wait_for_free_route_lock(dir, name)?;
    println!("revive {name} ({harness} session {session} in {cwd})");
    info!(route = name, %harness, session, command, "reviving a dead coordinator pane");
    // Restored shells and unrelated panes keep their names and contents.
    let mux = boop::tmux::mux();
    let mut target = name.to_owned();
    let mut suffix = 0;
    while mux.has_session(socket, &target)? {
        suffix += 1;
        target = format!("{name}-revived-{suffix}");
    }
    mux.new_detached_session(socket, &target, cwd, &command)?;
    wait_for_revived_route(dir, name, session, &target, socket)
}

/// A wrapper killed with its tmux server drops its route lock as it exits, and
/// a pane spawned into that gap refuses itself and dies. Wait the gap out.
fn wait_for_free_route_lock(dir: &Path, name: &str) -> Result<()> {
    let db = boop::bus::db_path(dir)?;
    let deadline = std::time::Instant::now() + LOCK_WAIT;
    loop {
        if boop::bus::try_route_lock(&db, name, "native-tui")?.is_some() {
            return Ok(());
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "route {name} still has a native TUI wrapper holding its lock after {LOCK_WAIT:?}"
        );
        std::thread::sleep(REVIVE_POLL);
    }
}

/// The revive is done when the new pane has written the route back on the same
/// session. A pane that died first is reported as itself, not as a timeout.
fn wait_for_revived_route(
    dir: &Path,
    name: &str,
    session: &str,
    target: &str,
    socket: Option<&str>,
) -> Result<()> {
    let deadline = std::time::Instant::now() + REVIVE_WAIT;
    let db = boop::bus::db_path(dir)?;
    loop {
        let pane = boop::bus::read_routes(dir)?
            .remove(name)
            .filter(|route| route.session_id.as_deref() == Some(session))
            .and_then(|route| route.tmux)
            .filter(|pane| !pane.is_empty());
        if let Some(pane) = pane {
            if boop::tmux::mux().session_of_pane(socket, &pane).as_deref() == Some(target)
                && boop::bus::try_route_lock(&db, name, "native-tui")?.is_none()
            {
                println!("revived {name} pane {pane}");
                return Ok(());
            }
        }
        anyhow::ensure!(
            boop::tmux::mux().target_alive(socket, target),
            "revive of {name} left no live pane; `boop debug {name}`"
        );
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "revive of {name} did not re-register session {session} within {REVIVE_WAIT:?}; `boop debug {name}`"
        );
        std::thread::sleep(REVIVE_POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        respawn_wanted, revive_blocker, revive_command, route_executable,
        session_for_native_frontend, stamp_executable, RESPAWN_MIN_UPTIME,
    };
    use anyhow::Result;
    use boop::harness::HarnessId;
    use boop::live::LiveSessions;
    use std::os::unix::process::ExitStatusExt;
    use std::path::Path;
    use std::process::ExitStatus;
    use std::sync::Mutex;
    use std::time::Duration;

    fn live(id: &str, pid: Option<u32>, pane: Option<&str>) -> boop::live::LiveSession {
        boop::live::LiveSession {
            harness: boop::harness::HarnessId::Omp,
            session_id: id.into(),
            pid,
            cwd: None,
            tmux_pane: pane.map(str::to_owned),
            status: boop::live::LiveStatus::Unknown,
            door: boop::live::DoorAddress::None,
            observed_ms: 0,
            started_ms: None,
            scope: boop::live::LiveSessionScope::Root,
            parent_session: None,
        }
    }

    struct TtyBoundLive {
        active: Mutex<String>,
    }

    impl LiveSessions for TtyBoundLive {
        fn live_sessions(&self) -> Result<Vec<boop::live::LiveSession>> {
            // This is OMP's list projection: a TTY breadcrumb has no tmux
            // pane, while a stale headless fallback claims the same pane.
            Ok(vec![live("headless-fallback", None, Some("%382"))])
        }

        fn live_session_in_pane(&self, pane: &str) -> Result<Option<boop::live::LiveSession>> {
            if pane.trim_start_matches('%') != "382" {
                return Ok(None);
            }
            Ok(Some(live(&self.active.lock().unwrap(), None, None)))
        }
    }

    #[test]
    fn native_start_and_refresh_use_the_tty_pane_endpoint_over_a_tmux_fallback() {
        let registry = TtyBoundLive {
            active: Mutex::new("tty-first".into()),
        };
        let listed = registry.live_sessions().unwrap();
        assert_eq!(
            session_for_native_frontend(&registry, &listed, 999, Some("%382"))
                .map(|session| session.session_id),
            Some("tty-first".into())
        );
        *registry.active.lock().unwrap() = "tty-switched".into();
        assert_eq!(
            session_for_native_frontend(&registry, &listed, 999, Some("382"))
                .map(|session| session.session_id),
            Some("tty-switched".into())
        );
    }

    #[test]
    fn native_refresh_preserves_pid_precedence() {
        let registry = TtyBoundLive {
            active: Mutex::new("tty-first".into()),
        };
        let listed = vec![live("pid-bound", Some(999), None)];
        assert_eq!(
            session_for_native_frontend(&registry, &listed, 999, Some("%382"))
                .map(|session| session.session_id),
            Some("pid-bound".into())
        );
    }
    /// RECEIPT (incident 2026-09-12). The assembled line is the command the
    /// user typed by hand, one argument at a time.
    #[test]
    fn the_revive_command_replays_the_hand_typed_recovery() {
        let command = revive_command(
            "/usr/local/bin/boop",
            HarnessId::Claude,
            "claude-2344",
            "/Users/c/projects/sprefa",
            Some("ccz"),
            Path::new("/Users/c/.agent"),
            &["--resume".to_owned(), "298b7814".to_owned()],
        );
        assert_eq!(
            command,
            "'/usr/local/bin/boop' tui claude --name 'claude-2344' \
             --cwd '/Users/c/projects/sprefa' --mail-dir '/Users/c/.agent' \
             --bin 'ccz' -- '--resume' '298b7814'"
        );
    }

    /// RECEIPT. Each harness's own resume spelling reaches the command line;
    /// sabotage: one hardcoded `--resume` sends codex into a new thread.
    #[test]
    fn each_harness_spells_its_own_resume_arguments() {
        let registry = boop::registry::Registry::discover();
        let spellings: Vec<(HarnessId, Vec<String>)> = [
            HarnessId::Claude,
            HarnessId::Codex,
            HarnessId::Opencode,
            HarnessId::Kimi,
        ]
        .into_iter()
        .map(|id| {
            (
                id,
                registry
                    .get(id)
                    .door()
                    .tui_resume_args("S-1")
                    .unwrap_or_default(),
            )
        })
        .collect();
        assert_eq!(
            spellings,
            vec![
                (HarnessId::Claude, vec!["--resume".into(), "S-1".into()]),
                (HarnessId::Codex, vec!["resume".into(), "S-1".into()]),
                (HarnessId::Opencode, vec!["--session".into(), "S-1".into()]),
                (HarnessId::Kimi, vec!["--session".into(), "S-1".into()]),
            ]
        );
        let command = revive_command(
            "boop",
            HarnessId::Codex,
            "codex-7",
            "/tmp/w",
            None,
            Path::new("/tmp/mail"),
            &spellings[1].1,
        );
        assert!(command.ends_with("-- 'resume' 'S-1'"), "{command}");
        assert!(!command.contains("--bin"), "{command}");
    }

    /// RECEIPT. The executable survives a round trip through `source_path`, so
    /// a revived pane runs `ccz` and not the harness's own binary name.
    #[test]
    fn the_route_records_the_executable_its_pane_ran() {
        let stamped = stamp_executable(Some("native-session=abc".into()), "ccz");
        assert_eq!(
            stamped.as_deref(),
            Some("native-executable=ccz;native-session=abc")
        );
        assert_eq!(route_executable(stamped.as_deref()), Some("ccz"));
        let door_written = stamp_executable(
            Some("native-executable=ccz;requested-resume=abc".into()),
            "claude",
        );
        assert_eq!(route_executable(door_written.as_deref()), Some("ccz"));
        assert_eq!(
            route_executable(stamp_executable(None, "opencode").as_deref()),
            Some("opencode")
        );
        assert_eq!(route_executable(Some("owned-app-server=/tmp/s.sock")), None);
        assert_eq!(route_executable(None), None);
    }

    /// RECEIPT. A claude pane that typed `/exit` closed on purpose and is not
    /// offered; the `Bye!` echo after it does not hide the command.
    #[test]
    fn an_explicit_exit_is_read_through_its_own_echo() {
        let rows = turns(&[
            ("user", "render the terminal flow"),
            ("assistant", "FIXED_TERMINAL_REPLY"),
            (
                "user",
                "<command-name>/exit</command-name>\n<command-message>exit",
            ),
            ("user", "<local-command-stdout>Bye!</local-command-stdout>"),
        ]);
        let digest = super::digest_from_turns(&rows);
        assert!(digest.exited);
        assert_eq!(digest.started, "render the terminal flow");
        assert_eq!(digest.last_user, "render the terminal flow");
        assert_eq!(digest.last_bot, "FIXED_TERMINAL_REPLY");
    }

    /// RECEIPT. A pane killed mid-turn ends on ordinary rows, so it is offered.
    /// Sabotage: reading the raw last user row marks every `/model` call exited.
    #[test]
    fn a_killed_pane_reads_as_an_unclean_death() {
        let rows = turns(&[
            ("user", "first ask"),
            ("assistant", "first answer"),
            ("user", "<command-name>/model</command-name>"),
            ("user", "second ask"),
            ("assistant", "second answer"),
        ]);
        let digest = super::digest_from_turns(&rows);
        assert!(!digest.exited);
        assert_eq!(digest.started, "first ask");
        assert_eq!(digest.last_user, "second ask");
        assert_eq!(digest.last_bot, "second answer");
    }

    /// RECEIPT. The selection prompt takes `all`, a list, and nothing; a row
    /// number outside the table is refused rather than revived by accident.
    #[test]
    fn the_selection_prompt_reads_all_a_list_and_none() {
        use super::parse_selection;
        assert_eq!(parse_selection("all", 3).unwrap(), vec![0, 1, 2]);
        assert_eq!(parse_selection("none", 3).unwrap(), Vec::<usize>::new());
        assert_eq!(parse_selection("\n", 3).unwrap(), Vec::<usize>::new());
        assert_eq!(parse_selection("1,3", 3).unwrap(), vec![0, 2]);
        assert_eq!(parse_selection("3 1 3", 3).unwrap(), vec![2, 0]);
        assert!(parse_selection("4", 3).is_err());
        assert!(parse_selection("0", 3).is_err());
        assert!(parse_selection("second", 3).is_err());
    }

    /// RECEIPT. A cell is one flat line at the stated width; age reads coarse.
    #[test]
    fn cells_are_cut_to_width_and_ages_read_coarse() {
        use super::{age, cut};
        assert_eq!(cut("one\n  two   three", 60), "one two three");
        assert_eq!(cut(&"x".repeat(80), 60).chars().count(), 60);
        assert!(cut(&"x".repeat(80), 60).ends_with("..."));
        assert_eq!(age(45_000), "45s");
        assert_eq!(age(600_000), "10m");
        assert_eq!(age(7_200_000), "2h");
        assert_eq!(age(3 * 86_400_000), "3d");
    }

    /// Rows shaped like `query_turns` output.
    fn turns(rows: &[(&str, &str)]) -> Vec<serde_json::Value> {
        rows.iter()
            .map(|(role, said)| serde_json::json!({ "role": role, "said": said }))
            .collect()
    }

    /// RECEIPT. The three-field precondition, one blocker per missing field.
    #[test]
    fn a_route_missing_one_of_three_fields_cannot_revive() {
        let mut route = crate::cli::testkit::route_with(None);
        assert_eq!(revive_blocker(&route), Some("not a coordinator route"));
        route.kind = "coordinator".into();
        route.harness = None;
        assert_eq!(revive_blocker(&route), Some("no harness recorded"));
        route.harness = Some(HarnessId::Claude);
        assert_eq!(revive_blocker(&route), Some("no session id recorded"));
        route.session_id = Some(String::new());
        assert_eq!(revive_blocker(&route), Some("no session id recorded"));
        route.session_id = Some("298b7814".into());
        assert_eq!(revive_blocker(&route), Some("no cwd recorded"));
        route.cwd = Some("/tmp/w".into());
        assert_eq!(revive_blocker(&route), None);
    }

    #[test]
    fn native_observation_preserves_a_registered_parent_update() {
        let dir =
            std::env::temp_dir().join(format!("boop-native-parent-update-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut cached = crate::cli::testkit::route_with(Some("old-parent"));
        let mut registered = cached.clone();
        registered.parent = Some("new-parent".into());
        registered.goal = Some("new-goal".into());
        boop::bus::write_route(&dir, "owned", &registered).unwrap();
        cached.model = Some("observed-model".into());
        let store = boop::bus::open_store(&dir).unwrap();
        boop::bus::update_native_route(&store, "owned", &mut cached).unwrap();
        let current = boop::bus::read_routes(&dir)
            .unwrap()
            .remove("owned")
            .unwrap();
        assert_eq!(
            (
                current.parent.as_deref(),
                current.goal.as_deref(),
                current.model.as_deref()
            ),
            (Some("new-parent"), Some("new-goal"), Some("observed-model"))
        );
        assert_eq!(cached.parent, current.parent);
        store
            .connection()
            .execute("DELETE FROM agent_route WHERE route='owned'", [])
            .unwrap();
        assert!(boop::bus::update_native_route(&store, "owned", &mut cached).is_err());
        assert!(boop::bus::routes_in(&store).unwrap().is_empty());
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_named_route_has_one_wrapper_owner_and_can_be_reacquired() {
        let root = std::env::temp_dir().join(format!("boop-native-owner-{}", std::process::id()));
        let db = root.join("fixture.db");
        let first = boop::bus::try_route_lock(&db, "named", "native-tui")
            .unwrap()
            .unwrap();
        assert!(boop::bus::try_route_lock(&db, "named", "native-tui")
            .unwrap()
            .is_none());
        let other = boop::bus::try_route_lock(&db, "other", "native-tui")
            .unwrap()
            .unwrap();
        drop(first);
        let resumed = boop::bus::try_route_lock(&db, "named", "native-tui")
            .unwrap()
            .unwrap();
        drop((other, resumed));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn native_exit_releases_its_transport_and_preserves_a_concurrent_owner() {
        let dir = std::env::temp_dir().join(format!("boop-native-release-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = boop::bus::open_store(&dir).unwrap();
        let mut route = crate::cli::testkit::route_with(Some("parent"));
        route.session_id = Some("release-session".into());
        route.mode = Some("native-owned".into());
        route.app_server_socket = Some("/fixture/old.sock".into());
        boop::bus::write_route(&dir, "release-route", &route).unwrap();
        store
            .record_status("release-session", 1, "live", Some(123), Some("%1"))
            .unwrap();
        super::release_native_route(&store, &dir, "release-route", &route, 123).unwrap();
        let released = boop::bus::read_routes(&dir)
            .unwrap()
            .remove("release-route")
            .unwrap();
        assert_eq!(
            (
                released.session_id.as_deref(),
                released.parent.as_deref(),
                released.app_server_socket
            ),
            (Some("release-session"), Some("parent"), None)
        );
        let row = store.live_row("release-session").unwrap().unwrap();
        assert_eq!(
            (row.status.as_deref(), row.pid, row.tmux_pane),
            (Some("detached"), None, None)
        );
        let mut resumed = route.clone();
        resumed.app_server_socket = Some("/fixture/new.sock".into());
        boop::bus::write_route(&dir, "release-route", &resumed).unwrap();
        store
            .record_status(
                "release-session",
                boop::live::now_ms(),
                "live",
                Some(456),
                Some("%2"),
            )
            .unwrap();
        super::release_native_route(&store, &dir, "release-route", &route, 123).unwrap();
        assert_eq!(
            boop::bus::read_routes(&dir).unwrap()["release-route"].app_server_socket,
            resumed.app_server_socket
        );
        assert_eq!(
            store.live_row("release-session").unwrap().unwrap().pid,
            Some(456)
        );
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn observed_transitions_preserve_parent_and_ignore_late_thread_events() {
        use boop::harness::NativeTuiEvent::{Closed, Session, Settings};
        let store = boop::Store::open(":memory:".into()).unwrap();
        store
            .attach_trace("independent", "separate-trace", "fixture", 1)
            .unwrap();
        let mut route = crate::cli::testkit::route_with(Some("parent"));
        let mut trace = None;
        let mut history = super::NativeSessionHistory {
            lane: "fixture".into(),
            process: Some((123, Some(1000))),
            session: None,
            sequence: 0,
        };
        let mut timeline = Vec::new();
        for event in [
            Session {
                session_id: "first".into(),
                model: Some("model-a".into()),
                effort: Some("low".into()),
            },
            Settings {
                session_id: "first".into(),
                model: Some("model-b".into()),
                effort: Some("high".into()),
            },
            Closed {
                session_id: "first".into(),
            },
            Session {
                session_id: "new".into(),
                model: Some("model-b".into()),
                effort: Some("high".into()),
            },
            Settings {
                session_id: "first".into(),
                model: Some("stale".into()),
                effort: None,
            },
            Closed {
                session_id: "first".into(),
            },
            Session {
                session_id: "independent".into(),
                model: Some("model-c".into()),
                effort: None,
            },
        ] {
            super::apply_native_event(&store, &mut route, &mut trace, &mut history, event, 123)
                .unwrap();
            timeline.push((
                route.session_id.clone(),
                route.model.clone(),
                trace.clone(),
                route.parent.clone(),
            ));
        }
        let expected = [
            (Some("first"), Some("model-a"), "trace-first"),
            (Some("first"), Some("model-b"), "trace-first"),
            (None, None, "trace-first"),
            (Some("new"), Some("model-b"), "trace-first"),
            (Some("new"), Some("model-b"), "trace-first"),
            (Some("new"), Some("model-b"), "trace-first"),
            (Some("independent"), Some("model-c"), "separate-trace"),
        ]
        .map(|(session, model, trace)| {
            (
                session.map(str::to_owned),
                model.map(str::to_owned),
                Some(trace.to_owned()),
                Some("parent".to_owned()),
            )
        });
        assert_eq!(timeline, expected);
        assert_eq!(
            store.trace_of("new").unwrap().as_deref(),
            Some("trace-first")
        );
        assert_eq!(
            store.session_attr("new", "effort").unwrap().as_deref(),
            Some("high")
        );
        assert_eq!(store.session_attr("independent", "effort").unwrap(), None);
        assert_eq!(
            store.live_row("new").unwrap().unwrap().status.as_deref(),
            Some("detached")
        );
    }

    #[test]
    fn clear_boundaries_preserve_process_siblings_without_linking_a_reused_pid() {
        use boop::harness::NativeTuiEvent::{Closed, Session};
        let store = boop::Store::open(":memory:".into()).unwrap();
        let mut route = crate::cli::testkit::route_with(Some("parent"));
        route.session_id = None;
        let mut trace = None;
        let mut history = super::NativeSessionHistory {
            lane: "coordinator".into(),
            process: Some((123, Some(1000))),
            session: None,
            sequence: 0,
        };
        for event in [
            Session {
                session_id: "before-clear".into(),
                model: None,
                effort: None,
            },
            Closed {
                session_id: "before-clear".into(),
            },
            Session {
                session_id: "after-clear".into(),
                model: None,
                effort: None,
            },
            // Refreshes and compaction that retain the conversation id create no sibling.
            Session {
                session_id: "after-clear".into(),
                model: None,
                effort: None,
            },
        ] {
            super::apply_native_event(&store, &mut route, &mut trace, &mut history, event, 123)
                .unwrap();
        }
        let events = store.query_trace_events(Some("coordinator"), 100).unwrap();
        let snapshot: Vec<_> = events
            .iter()
            .map(|event| {
                serde_json::json!({
                    "key":event.event_key, "kind":event.kind, "trace":event.trace,
                    "from":event.from_lane, "to":event.to_lane,
                    "detail":serde_json::from_str::<serde_json::Value>(&event.detail).unwrap()
                })
            })
            .collect();
        assert_eq!(
            snapshot,
            vec![serde_json::json!({
                "key":"native-session:coordinator:123:1000:0",
                "kind":"session-boundary", "trace":"trace-before-clear",
                "from":"before-clear", "to":"after-clear",
                "detail":{"previous_session":"before-clear", "session":"after-clear",
                    "pid":123, "process_start_secs":1000, "boundary":"conversation-changed"}
            })]
        );
        let siblings: Vec<_> = ["before-clear", "after-clear"]
            .into_iter()
            .map(|session| {
                let live = store.live_row(session).unwrap().unwrap();
                (
                    live.pid,
                    live.status,
                    store.session_attr(session, "process_pid").unwrap(),
                    store.session_attr(session, "process_start_secs").unwrap(),
                    store.trace_of(session).unwrap(),
                )
            })
            .collect();
        assert_eq!(
            siblings,
            vec![
                (
                    Some(123),
                    Some("detached".into()),
                    Some("123".into()),
                    Some("1000".into()),
                    Some("trace-before-clear".into())
                ),
                (
                    Some(123),
                    Some("live".into()),
                    Some("123".into()),
                    Some("1000".into()),
                    Some("trace-before-clear".into())
                ),
            ]
        );
        // A new wrapper can observe the same PID after exit/reboot. Its
        // process start time and history are distinct, so no boundary links it.
        let mut history = super::NativeSessionHistory {
            lane: "coordinator".into(),
            process: Some((123, Some(2000))),
            session: None,
            sequence: 0,
        };
        let mut trace = None;
        super::bind_native_session(
            &store,
            &mut route,
            &mut trace,
            &mut history,
            "new-process",
            123,
        )
        .unwrap();
        assert_eq!(
            store.query_trace_events(Some("coordinator"), 100).unwrap(),
            events
        );
        assert_eq!(
            store
                .session_attr("new-process", "process_previous_session")
                .unwrap(),
            None
        );
        assert_eq!(
            store.trace_of("new-process").unwrap(),
            Some("trace-new-process".into())
        );
    }

    #[test]
    fn nonzero_exit_after_min_uptime_respawns() {
        let status = ExitStatus::from_raw(256);
        assert!(respawn_wanted(Some(status), 0, RESPAWN_MIN_UPTIME));
        assert!(respawn_wanted(Some(status), 2, Duration::from_secs(3600)));
        assert!(respawn_wanted(None, 0, RESPAWN_MIN_UPTIME));
        assert!(!respawn_wanted(None, 3, RESPAWN_MIN_UPTIME));
        assert!(!respawn_wanted(None, 0, Duration::from_secs(1)));
    }

    #[test]
    fn signal_death_fast_death_and_exhaustion_end_the_wrapper() {
        let killed = ExitStatus::from_raw(9);
        assert!(!respawn_wanted(Some(killed), 0, Duration::from_secs(3600)));
        let failed = ExitStatus::from_raw(256);
        assert!(!respawn_wanted(Some(failed), 0, Duration::from_secs(1)));
        assert!(!respawn_wanted(Some(failed), 3, Duration::from_secs(3600)));
    }
}
