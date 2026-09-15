//! `boop tui`: launch a harness's own interactive TUI, register the pane as
//! that harness's coordinator route, and project while it runs.

use std::io::Write;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use boop::bus::Route;
use boop::harness::{Harness, NativeTuiEvent, NativeTuiPlan, NativeTuiSpec};
use boop::registry::Registry;
use tracing::{info, warn};

use crate::cli::{mail_dir, write_route};

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

/// One binding path for adapter-observed sessions and native control events.
fn bind_native_session(
    store: &boop::Store,
    route: &mut Route,
    trace: &mut Option<String>,
    session: &str,
    pid: u32,
) -> anyhow::Result<()> {
    let ts = boop::live::now_ms();
    if let Some(previous) = route
        .session_id
        .as_deref()
        .filter(|previous| *previous != session)
    {
        store.record_status(previous, ts, "detached", None, None)?;
    }
    *trace = Some(
        store
            .trace_of(session)?
            .or_else(|| trace.clone())
            .unwrap_or_else(|| format!("trace-{session}")),
    );
    store.attach_trace(session, trace.as_deref().unwrap(), "native-tui-session", ts)?;
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
            bind_native_session(store, route, trace, &session_id, pid)?;
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
            store.record_status(&session_id, ts, "closed", None, None)?;
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
    let _ownership = boop::bus::try_route_lock(&boop::bus::db_path(&dir)?, name, "native-tui")?
        .with_context(|| format!("route {name} already has a native TUI wrapper"))?;
    let store = boop::bus::open_store(&dir)?;
    let existing = boop::bus::read_routes(&dir)?.remove(name);
    if let Some(existing) = &existing {
        anyhow::ensure!(
            existing.kind != "lane",
            "route {name} belongs to a lane supervisor"
        );
        if let Some(session) = existing.session_id.as_deref() {
            let live_pid = store
                .live_row(session)?
                .and_then(|row| row.pid)
                .and_then(|pid| u32::try_from(pid).ok())
                .filter(|pid| boop::live::pid_alive(*pid));
            anyhow::ensure!(
                live_pid.is_none(),
                "route {name} still owns live process {live_pid:?}"
            );
        }
    }
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
        source_path: plan.source_path.clone(),
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
    if let Some(session) = route.session_id.clone() {
        bind_native_session(&store, &mut route, &mut trace, &session, frontend_pid)?;
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
                        apply_native_event(&store, &mut route, &mut trace, event, frontend_pid)?;
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
                spawned_at = std::time::Instant::now();
                route.app_server_socket = next.app_server_socket.clone();
                route.source_path = next.source_path.clone();
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

#[cfg(test)]
mod tests {
    use super::{respawn_wanted, session_for_native_frontend, RESPAWN_MIN_UPTIME};
    use anyhow::Result;
    use boop::live::LiveSessions;
    use std::os::unix::process::ExitStatusExt;
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

        fn live_session_in_pane(
            &self,
            pane: &str,
        ) -> Result<Option<boop::live::LiveSession>> {
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
            super::apply_native_event(&store, &mut route, &mut trace, event, 123).unwrap();
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
