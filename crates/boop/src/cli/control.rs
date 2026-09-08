//! `boop tui`: launch a harness's own interactive TUI, register the pane as
//! that harness's coordinator route, and project while it runs.

use std::collections::{BTreeSet, HashMap};
use std::io::Write;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use boop::bus::Route;
use boop::harness::{Harness, NativeTuiEvent, NativeTuiSpec};
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
fn respawn_wanted(status: Option<std::process::ExitStatus>, respawns: u32, uptime: Duration) -> bool {
    status.is_none_or(|status| status.code().is_some_and(|code| code != 0))
        && respawns < RESPAWN_MAX
        && uptime >= RESPAWN_MIN_UPTIME
}

/// The session this launch opened or resumed, read from the harness's own live
/// registry. A new root starts after `opened_ms`; an old root must be the only
/// same-cwd session whose observation advanced from the pre-launch snapshot.
fn opened_session(
    adapter: &dyn Harness,
    dir: &Path,
    cwd: &Path,
    opened_ms: u64,
    prior_observations: &HashMap<String, u64>,
    wait: Duration,
    my_pane: &str,
    pid: u32,
) -> Option<String> {
    let canonical = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let deadline = std::time::Instant::now() + wait;
    loop {
        let live = adapter.live().live_sessions().unwrap_or_default();
        if let Some(session) = session_for_pid(&live, pid) {
            return Some(session.session_id.clone());
        }
        let picked = newest_opened_session(
            live.clone(),
            &canonical,
            opened_ms,
            prior_observations,
            Some(my_pane),
            &boop::mail::claimed_sessions(dir, Some(my_pane), &canonical),
        );
        if let Some(session) = picked {
            // The pane's own registry row is proof enough; a registry-derived
            // pick needs the claim marker.
            let exact = live
                .iter()
                .any(|held| held.session_id == session && held.tmux_pane.as_deref() == Some(my_pane));
            if exact || boop::mail::claim_open_session(dir, &session) {
                return Some(session);
            }
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn session_for_pid(live: &[boop::live::LiveSession], pid: u32) -> Option<&boop::live::LiveSession> {
    let mut matches = live.iter().filter(|session| session.pid == Some(pid));
    let session = matches.next()?;
    matches.next().is_none().then_some(session)
}

fn newest_opened_session(
    live: Vec<boop::live::LiveSession>,
    canonical_cwd: &Path,
    opened_ms: u64,
    prior_observations: &HashMap<String, u64>,
    my_pane: Option<&str>,
    claimed: &BTreeSet<String>,
) -> Option<String> {
    let candidates = live
        .into_iter()
        .filter(|session| {
            session
                .cwd
                .as_ref()
                .map(|dir| std::fs::canonicalize(dir).unwrap_or_else(|_| dir.clone()))
                .as_deref()
                == Some(canonical_cwd)
                && session.scope != boop::live::LiveSessionScope::Child
                && !claimed.contains(&session.session_id)
        })
        .collect::<Vec<_>>();
    // The pane's own registry row outranks the newest-session heuristic:
    // a pane names exactly one session.
    if let Some(pane) = my_pane {
        if let Some(session) = candidates
            .iter()
            .find(|session| session.tmux_pane.as_deref() == Some(pane))
        {
            return Some(session.session_id.clone());
        }
    }
    if let Some(session) = candidates
        .iter()
        .filter(|session| {
            session.started_ms.map_or_else(
                || {
                    session.observed_ms >= opened_ms
                        && !prior_observations.contains_key(&session.session_id)
                },
                |started| started >= opened_ms,
            )
        })
        .max_by_key(|session| session.observed_ms)
    {
        return Some(session.session_id.clone());
    }
    let mut resumed = candidates.into_iter().filter(|session| {
        session.observed_ms >= opened_ms
            && prior_observations
                .get(&session.session_id)
                .is_some_and(|prior| session.observed_ms > *prior)
    });
    let session = resumed.next()?;
    resumed.next().is_none().then_some(session.session_id)
}

fn session_observations(adapter: &dyn Harness) -> HashMap<String, u64> {
    adapter
        .live()
        .live_sessions()
        .unwrap_or_default()
        .into_iter()
        .map(|session| (session.session_id, session.observed_ms))
        .collect()
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

/// One binding path for adapter-observed sessions and native control events.
fn bind_native_session(
    store: &boop::Store,
    route: &mut Route,
    trace: &mut Option<String>,
    session: &str,
    pid: u32,
) -> anyhow::Result<()> {
    let ts = boop::live::now_ms();
    if let Some(previous) = route.session_id.as_deref().filter(|previous| *previous != session) {
        store.record_status(previous, ts, "detached", None, None)?;
    }
    *trace = Some(store.trace_of(session)?.or_else(|| trace.clone())
        .unwrap_or_else(|| format!("trace-{session}")));
    store.attach_trace(session, trace.as_deref().unwrap(), "native-tui-session", ts)?;
    route.session_id = Some(session.to_owned());
    store.record_status(session, ts, "live", Some(i64::from(pid)), route.tmux.as_deref())
}

/// Release only the transport and process observation still owned by this
/// wrapper. A concurrent resume may already have rebound the same route.
fn release_native_route(store: &boop::Store, dir: &Path, name: &str, route: &Route, pid: u32) -> Result<()> {
    if let Some(session) = route.session_id.as_deref() {
        store.detach_process(session, pid, boop::live::now_ms())?;
    }
    boop::bus::cas_update_json(&dir.join("registry.json"), |routes| {
        if let Some(current) = routes.get_mut(name).and_then(serde_json::Value::as_object_mut) {
            if current.get("appServerSocket").and_then(serde_json::Value::as_str) == route.app_server_socket.as_deref()
                && current.get("sessionId").and_then(serde_json::Value::as_str) == route.session_id.as_deref()
                && current.get("registeredAt").and_then(serde_json::Value::as_str) == route.registered_at.as_deref() {
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
        NativeTuiEvent::Session { session_id, model, effort } => {
            bind_native_session(store, route, trace, &session_id, pid)?;
            (session_id, model, effort)
        }
        NativeTuiEvent::Settings { session_id, model, effort } if route.session_id.as_deref() == Some(&session_id) => (session_id, model, effort),
        NativeTuiEvent::Closed { session_id } if route.session_id.as_deref() == Some(&session_id) => {
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
        None => { store.clear_session_attr(&session_id, "effort")?; }
    }
    Ok(())
}

/// Ask the selected harness adapter to prepare its native process, register
/// this pane as its coordinator, then run the ordinary interactive TUI.
pub(crate) fn native_route_name(harness: &str) -> String {
    match std::env::var("TMUX_PANE").ok().filter(|pane| !pane.is_empty()) {
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
    tui_args: &[String],
) -> Result<()> {
    let executable = executable.unwrap_or(adapter.id().as_str());
    if !adapter.uses_native_tui(tui_args) {
        use std::os::unix::process::CommandExt;
        return Err(Command::new(executable).args(tui_args).current_dir(cwd).exec())
            .with_context(|| format!("execute {executable}"));
    }
    let mut signals = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGHUP, signal_hook::consts::SIGTERM,
    ])?;
    let pane = std::env::var("TMUX_PANE")
        .ok()
        .filter(|pane| !pane.is_empty());
    let default_name = native_route_name(adapter.id().as_str());
    let name = name.unwrap_or(&default_name);
    let dir = mail_dir(mail_dir_arg)?;
    let _ownership = boop::bus::try_route_lock(&boop::bus::db_path(&dir)?, name, "native-tui")?
        .with_context(|| format!("route {name} already has a native TUI wrapper"))?;
    let store = boop::bus::open_store(&dir)?;
    let existing = boop::bus::read_routes(&dir)?.remove(name);
    if let Some(existing) = &existing {
        anyhow::ensure!(existing.kind != "lane", "route {name} belongs to a lane supervisor");
        if let Some(session) = existing.session_id.as_deref() {
            let live_pid = store.live_row(session)?.and_then(|row| row.pid)
                .and_then(|pid| u32::try_from(pid).ok()).filter(|pid| boop::live::pid_alive(*pid));
            anyhow::ensure!(live_pid.is_none(), "route {name} still owns live process {live_pid:?}");
        }
    }
    let parent = existing.and_then(|route| route.parent)
        .or_else(|| boop::identity::from_env().and_then(|identity| identity.session).filter(|caller| caller != name));
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
            ("BOOP_DB".into(), boop::bus::db_path(&dir)?.display().to_string()),
        ],
    };
    let mut plan = adapter.door().tui_launch(&spec)?;
    let mut known = adapter.capabilities().native_tui_projector
        .then(|| store.known_sessions()).transpose()?;
    let prior_observations = session_observations(adapter);
    let opened_ms = boop::live::now_ms();
    let _alternate_screen =
        AlternateScreen::enter(adapter.capabilities().wrapper_owns_alternate_screen);
    // The stamp every `boop` call inside this TUI reads as its identity,
    // inherited by the harness's own shell and native subagents.
    plan.frontend = Some(Command::new(&plan.program)
        .args(&plan.args)
        .envs(spec.env.iter().cloned())
        .current_dir(cwd)
        .spawn()
        .with_context(|| format!("start native {} TUI", adapter.id()))?);
    let mut frontend_pid = plan.frontend.as_ref().unwrap().id();
    let mut respawns: u32 = 0;
    let mut spawned_at = std::time::Instant::now();
    if plan.session_id.is_none() && plan.observer.is_none() {
        plan.session_id = opened_session(
            adapter,
            &dir,
            cwd,
            opened_ms,
            &prior_observations,
            SESSION_WAIT,
            pane.as_deref().unwrap_or(""),
            frontend_pid,
        );
        if let Some(session) = plan.session_id.as_deref() {
            plan.source_path = Some(format!("native-session={session}"));
            info!(%session, harness = %adapter.id(), "native session route resolved");
        }
    }
    let mut route = Route {
        kind: "coordinator".into(),
        harness: Some(adapter.id()),
        tmux: pane.clone(),
        cwd: Some(cwd.display().to_string()),
        model: None,
        mode: Some(plan.mode.clone()),
        session_id: plan.session_id.clone(),
        source_path: plan.source_path.clone(),
        parent,
        goal: None,
        registered_at: Some(boop::bus::now_iso()),
        base_sha: None,
        worktree_dir: None,
        app_server_socket: plan.app_server_socket.clone(),
    };
    let mut trace = None;
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
    let outcome = (|| -> Result<()> { loop {
        if let Some(signal) = signals.pending().next() {
            anyhow::bail!("native TUI stopped by signal {signal}");
        }
        let mut observation_failure = None;
        if let Some(observer) = plan.observer.as_ref() {
            for event in observer.events.try_iter() {
                if let NativeTuiEvent::Failed(error) = event {
                    observation_failure = Some(error);
                } else {
                    apply_native_event(&store, &mut route, &mut trace, event, frontend_pid)?;
                    write_route(&dir, name, route.clone())?;
                }
            }
        }
        let frontend_exit = plan.frontend.as_mut().unwrap().try_wait().context("observe native TUI exit")?;
        let backend_exit = plan.backend.as_mut().map(|child| child.try_wait()).transpose()
            .context("observe native backend exit")?.flatten();
        if frontend_exit.is_some() || backend_exit.is_some() || observation_failure.is_some() {
            if frontend_exit.is_some_and(|status| status.success()) {
                return Ok(());
            }
            let next = match route.session_id.as_deref() {
                Some(session) if respawn_wanted(frontend_exit, respawns, spawned_at.elapsed()) => {
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
            next.frontend = Some(Command::new(&next.program)
                .args(&next.args)
                .envs(spec.env.iter().cloned())
                .current_dir(cwd)
                .spawn()
                .with_context(|| format!("respawn native {} TUI", adapter.id()))?);
            frontend_pid = next.frontend.as_ref().unwrap().id();
            spawned_at = std::time::Instant::now();
            route.app_server_socket = next.app_server_socket.clone();
            route.source_path = next.source_path.clone();
            route.session_id = next.session_id.clone();
            plan = next;
            write_route(&dir, name, route.clone())?;
            continue;
        }
        // A fresh TUI opens its session at its first prompt, after the route
        // was written; the route learns the id the first tick it exists.
        if route.session_id.is_none() && plan.observer.is_none() {
            match opened_session(
                adapter,
                &dir,
                cwd,
                opened_ms,
                &prior_observations,
                Duration::ZERO,
                pane.as_deref().unwrap_or(""),
                frontend_pid,
            ) {
                Some(session) => {
                    route.source_path = Some(format!("native-session={session}"));
                    bind_native_session(&store, &mut route, &mut trace, &session, frontend_pid)?;
                    write_route(&dir, name, route.clone())?;
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
            if let Some(session) = session_for_pid(&observed, frontend_pid) {
                if route.session_id.as_deref() != Some(session.session_id.as_str()) {
                    bind_native_session(&store, &mut route, &mut trace, &session.session_id, frontend_pid)?;
                    route.model = None;
                    route.source_path = Some(format!("native-session={}", session.session_id));
                    write_route(&dir, name, route.clone())?;
                }
            }
            if let Some(session) = route.session_id.as_deref()
                .and_then(|id| adapter.session_by_id(id, route.cwd.as_deref())) {
                if let Some(NativeTuiEvent::Settings { session_id, model, effort }) = adapter.native_settings(&session) {
                    if route.model != model || store.session_attr(&session_id, "effort")? != effort {
                        apply_native_event(&store, &mut route, &mut trace,
                            NativeTuiEvent::Settings { session_id, model, effort }, frontend_pid)?;
                        write_route(&dir, name, route.clone())?;
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
                    |message| crate::cli::mail::deliver_hail(registry, &dir, message, None),
                ) {
                    warn!(%error, route = name, "native discovery projector pass failed");
                }
            }
            if last_parent_project.elapsed() >= parent_project_every {
                if let Err(error) =
                    crate::cli::db::sync_native_parent_route_once(&store, known, adapter, name, &dir)
                {
                    warn!(%error, route = name, "native parent projector pass failed");
                }
            }
        }
        if last_parent_project.elapsed() >= parent_project_every {
            last_parent_project = std::time::Instant::now();
        }
        std::thread::sleep(EXIT_POLL);
    } })();
    drop(plan);
    let cleanup = release_native_route(&store, &dir, name, &route, frontend_pid);
    if let Err(error) = &cleanup {
        warn!(%error, route = name, "native route cleanup failed");
    }
    outcome.and(cleanup)
}

#[cfg(test)]
mod tests {
    use super::{newest_opened_session, respawn_wanted, RESPAWN_MIN_UPTIME};
    use std::collections::{BTreeSet, HashMap};
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;
    use std::time::Duration;

    #[test]
    fn a_named_route_has_one_wrapper_owner_and_can_be_reacquired() {
        let root = std::env::temp_dir().join(format!("boop-native-owner-{}", std::process::id()));
        let db = root.join("fixture.db");
        let first = boop::bus::try_route_lock(&db, "named", "native-tui").unwrap().unwrap();
        assert!(boop::bus::try_route_lock(&db, "named", "native-tui").unwrap().is_none());
        let other = boop::bus::try_route_lock(&db, "other", "native-tui").unwrap().unwrap();
        drop(first);
        let resumed = boop::bus::try_route_lock(&db, "named", "native-tui").unwrap().unwrap();
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
        store.record_status("release-session", 1, "live", Some(123), Some("%1")).unwrap();
        super::release_native_route(&store, &dir, "release-route", &route, 123).unwrap();
        let released = boop::bus::read_routes(&dir).unwrap().remove("release-route").unwrap();
        assert_eq!((released.session_id.as_deref(), released.parent.as_deref(), released.app_server_socket),
            (Some("release-session"), Some("parent"), None));
        let row = store.live_row("release-session").unwrap().unwrap();
        assert_eq!((row.status.as_deref(), row.pid, row.tmux_pane), (Some("detached"), None, None));
        let mut resumed = route.clone();
        resumed.app_server_socket = Some("/fixture/new.sock".into());
        boop::bus::write_route(&dir, "release-route", &resumed).unwrap();
        store.record_status("release-session", boop::live::now_ms(), "live", Some(456), Some("%2")).unwrap();
        super::release_native_route(&store, &dir, "release-route", &route, 123).unwrap();
        assert_eq!(boop::bus::read_routes(&dir).unwrap()["release-route"].app_server_socket, resumed.app_server_socket);
        assert_eq!(store.live_row("release-session").unwrap().unwrap().pid, Some(456));
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn observed_transitions_preserve_parent_and_ignore_late_thread_events() {
        use boop::harness::NativeTuiEvent::{Session, Settings, Closed};
        let store = boop::Store::open(":memory:".into()).unwrap();
        store.attach_trace("independent", "separate-trace", "fixture", 1).unwrap();
        let mut route = crate::cli::testkit::route_with(Some("parent"));
        let mut trace = None;
        let mut timeline = Vec::new();
        for event in [
            Session {session_id:"first".into(), model:Some("model-a".into()), effort:Some("low".into())},
            Settings {session_id:"first".into(), model:Some("model-b".into()), effort:Some("high".into())},
            Closed {session_id:"first".into()},
            Session {session_id:"new".into(), model:Some("model-b".into()), effort:Some("high".into())},
            Settings {session_id:"first".into(), model:Some("stale".into()), effort:None},
            Closed {session_id:"first".into()},
            Session {session_id:"independent".into(), model:Some("model-c".into()), effort:None},
        ] {
            super::apply_native_event(&store, &mut route, &mut trace, event, 123).unwrap();
            timeline.push((route.session_id.clone(), route.model.clone(), trace.clone(), route.parent.clone()));
        }
        let expected = [
            (Some("first"), Some("model-a"), "trace-first"),
            (Some("first"), Some("model-b"), "trace-first"),
            (None, None, "trace-first"),
            (Some("new"), Some("model-b"), "trace-first"),
            (Some("new"), Some("model-b"), "trace-first"),
            (Some("new"), Some("model-b"), "trace-first"),
            (Some("independent"), Some("model-c"), "separate-trace"),
        ].map(|(session, model, trace)| (session.map(str::to_owned), model.map(str::to_owned), Some(trace.to_owned()), Some("parent".to_owned())));
        assert_eq!(timeline, expected);
        assert_eq!(store.trace_of("new").unwrap().as_deref(), Some("trace-first"));
        assert_eq!(store.session_attr("new", "effort").unwrap().as_deref(), Some("high"));
        assert_eq!(store.session_attr("independent", "effort").unwrap(), None);
        assert_eq!(store.live_row("new").unwrap().unwrap().status.as_deref(), Some("detached"));
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

    fn session(
        id: &str,
        started_ms: u64,
        scope: boop::live::LiveSessionScope,
    ) -> boop::live::LiveSession {
        boop::live::LiveSession {
            harness: boop::harness::HarnessId::Codex,
            session_id: id.to_owned(),
            pid: None,
            cwd: Some("/tmp/turn-attribution-parent".into()),
            tmux_pane: None,
            status: boop::live::LiveStatus::Unknown,
            door: boop::live::DoorAddress::None,
            observed_ms: started_ms,
            started_ms: Some(started_ms),
            scope,
            parent_session: None,
        }
    }

    #[test]
    fn a_guardian_started_111ms_later_does_not_take_the_parent_pane() {
        let root = session(
            "01a053e4-d12c-parent",
            1_788_113_899_820,
            boop::live::LiveSessionScope::Root,
        );
        let guardian = session(
            "01a053e4-d19b-guardian",
            1_788_113_899_931,
            boop::live::LiveSessionScope::Child,
        );
        assert_eq!(
            newest_opened_session(
                vec![root, guardian],
                std::path::Path::new("/tmp/turn-attribution-parent"),
                1_788_113_899_000,
                &HashMap::new(),
                None,
                &BTreeSet::new(),
            )
            .as_deref(),
            Some("01a053e4-d12c-parent")
        );
    }

    #[test]
    fn one_old_root_updated_after_launch_is_recovered() {
        let mut root = session(
            "01a0550d-old-root",
            1_788_100_000_000,
            boop::live::LiveSessionScope::Root,
        );
        root.observed_ms = 1_788_200_001_000;
        let prior = HashMap::from([(root.session_id.clone(), 1_788_199_999_000)]);

        assert_eq!(
            newest_opened_session(
                vec![root],
                std::path::Path::new("/tmp/turn-attribution-parent"),
                1_788_200_000_000,
                &prior,
                None,
                &BTreeSet::new(),
            )
            .as_deref(),
            Some("01a0550d-old-root")
        );
    }

    #[test]
    fn unchanged_or_ambiguous_old_roots_are_not_recovered() {
        let mut unchanged = session(
            "unchanged",
            1_788_100_000_000,
            boop::live::LiveSessionScope::Root,
        );
        unchanged.observed_ms = 1_788_199_999_000;
        let mut first = unchanged.clone();
        first.session_id = "first".into();
        first.observed_ms = 1_788_200_001_000;
        let mut second = first.clone();
        second.session_id = "second".into();
        let prior = HashMap::from([
            ("unchanged".into(), 1_788_199_999_000),
            ("first".into(), 1_788_199_999_000),
            ("second".into(), 1_788_199_999_000),
        ]);

        assert_eq!(
            newest_opened_session(
                vec![unchanged],
                std::path::Path::new("/tmp/turn-attribution-parent"),
                1_788_200_000_000,
                &prior,
                None,
                &BTreeSet::new(),
            ),
            None
        );
        assert_eq!(
            newest_opened_session(
                vec![first, second],
                std::path::Path::new("/tmp/turn-attribution-parent"),
                1_788_200_000_000,
                &prior,
                None,
                &BTreeSet::new(),
            ),
            None
        );
    }

    /// RECEIPT. Two same-cwd panes of one harness bound the newest session
    /// twice; a pane's own registry row outranks the newest-session heuristic.
    #[test]
    fn a_pane_exact_session_wins_over_a_newer_unpaned_one() {
        let mut own = session("own-pane", 1_000, boop::live::LiveSessionScope::Root);
        own.tmux_pane = Some("%415".into());
        let newer = session("newer", 2_000, boop::live::LiveSessionScope::Root);
        assert_eq!(
            newest_opened_session(
                vec![newer, own],
                std::path::Path::new("/tmp/turn-attribution-parent"),
                900,
                &HashMap::new(),
                Some("%415"),
                &BTreeSet::new(),
            )
            .as_deref(),
            Some("own-pane")
        );
    }

    /// RECEIPT. codex's registry records no pane; its binding is registry-derived.
    #[test]
    fn a_session_another_pane_claimed_is_never_taken_again() {
        let first = session("first", 1_000, boop::live::LiveSessionScope::Root);
        let second = session("second", 2_000, boop::live::LiveSessionScope::Root);
        let claimed = BTreeSet::from(["second".to_string()]);
        assert_eq!(
            newest_opened_session(
                vec![first, second],
                std::path::Path::new("/tmp/turn-attribution-parent"),
                900,
                &HashMap::new(),
                None,
                &claimed,
            )
            .as_deref(),
            Some("first")
        );
    }

    #[test]
    fn two_registry_derived_binds_take_two_sessions_one_each() {
        let dir = std::env::temp_dir().join(format!("boop-claim-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        assert!(boop::mail::claim_open_session(&dir, "aa"));
        assert!(!boop::mail::claim_open_session(&dir, "aa"));
        assert!(boop::mail::claim_open_session(&dir, "bb"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
