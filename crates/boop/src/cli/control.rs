//! `boop tui`: launch a harness's own interactive TUI, register the pane as
//! that harness's coordinator route, and project while it runs.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use boop::bus::Route;
use boop::harness::{Harness, NativeTuiSpec};
use boop::registry::Registry;
use tracing::warn;

use crate::cli::{mail_dir, write_route};

/// How long a fresh TUI is given to appear in its harness's own live-session
/// registry. The route is written either way; an unresolved session leaves the
/// `sessionId` field empty rather than carrying a guess.
const SESSION_WAIT: Duration = Duration::from_secs(10);

/// Respawn ceiling for one wrapper process; past it a dying TUI is a defect
/// to look at, never a loop to ride.
const RESPAWN_MAX: u32 = 3;

/// Below this uptime a nonzero exit is a launch defect (bad flag, bad
/// config) and a respawn would loop on it.
const RESPAWN_MIN_UPTIME: Duration = Duration::from_secs(10);

/// Whether a dead TUI earns another process against the same session. A
/// signal death is a deliberate kill and both gates below stop loops.
fn respawn_wanted(status: std::process::ExitStatus, respawns: u32, uptime: Duration) -> bool {
    status.code().is_some_and(|code| code != 0)
        && respawns < RESPAWN_MAX
        && uptime >= RESPAWN_MIN_UPTIME
}

/// The session this launch opened, read from the harness's own live registry:
/// the newest one under `cwd` that the registry first saw after `opened_ms`.
fn opened_session(
    adapter: &dyn Harness,
    cwd: &Path,
    opened_ms: u64,
    wait: Duration,
) -> Option<String> {
    let canonical = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let deadline = std::time::Instant::now() + wait;
    loop {
        let live = adapter.live().live_sessions().unwrap_or_default();
        let newest = live
            .into_iter()
            .filter(|session| {
                // A harness that records a start time decides by it: a thread
                // another TUI keeps updating in the same cwd is not this one.
                session
                    .started_ms
                    .map_or(session.observed_ms >= opened_ms, |started| {
                        started >= opened_ms
                    })
                    && session
                        .cwd
                        .as_ref()
                        .map(|dir| std::fs::canonicalize(dir).unwrap_or_else(|_| dir.clone()))
                        .as_deref()
                        == Some(canonical.as_path())
            })
            .max_by_key(|session| session.observed_ms);
        if let Some(session) = newest {
            return Some(session.session_id);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Ask the selected harness adapter to prepare its native process, register
/// this pane as its coordinator, then run the ordinary interactive TUI.
pub(crate) fn run_native_tui(
    registry: &Registry,
    adapter: &dyn Harness,
    name: Option<&str>,
    cwd: &Path,
    mail_dir_arg: Option<&Path>,
    executable: Option<&str>,
    tui_args: &[String],
) -> Result<()> {
    let pane = std::env::var("TMUX_PANE")
        .ok()
        .filter(|pane| !pane.is_empty())
        .context("`boop tui` requires TMUX_PANE so its route matches harness identity")?;
    let default_name = format!("{}-{}", adapter.id(), pane.trim_start_matches('%'));
    let name = name.unwrap_or(&default_name);
    anyhow::ensure!(
        name == default_name,
        "native {} route name must be {default_name} for its TMUX_PANE",
        adapter.id()
    );
    let executable = executable.unwrap_or(adapter.id().as_str());
    let mut plan = adapter.door().tui_launch(&NativeTuiSpec {
        executable: executable.into(),
        cwd: cwd.to_path_buf(),
        args: tui_args.to_vec(),
    })?;
    let dir = mail_dir(mail_dir_arg)?;
    let store = adapter
        .capabilities()
        .native_tui_projector
        .then(|| boop::Store::open(boop::Store::default_path()?))
        .transpose()?;
    let mut known = store
        .as_ref()
        .map(boop::Store::known_sessions)
        .transpose()?;
    let opened_ms = boop::live::now_ms();
    // The stamp every `boop` call inside this TUI reads as its identity,
    // inherited by the harness's own shell and native subagents.
    let mut child = Command::new(&plan.program)
        .args(&plan.args)
        .env("BOOP_SESSION", name)
        .env("BOOP_LANE", name)
        .current_dir(cwd)
        .spawn()
        .with_context(|| format!("start native {} TUI", adapter.id()))?;
    let mut respawns: u32 = 0;
    let mut spawned_at = std::time::Instant::now();
    if plan.session_id.is_none() {
        plan.session_id = opened_session(adapter, cwd, opened_ms, SESSION_WAIT);
        if let Some(session) = plan.session_id.as_deref() {
            plan.source_path = Some(format!("native-session={session}"));
        }
    }
    let mut route = Route {
        kind: "coordinator".into(),
        harness: Some(adapter.id()),
        tmux: Some(pane),
        cwd: Some(cwd.display().to_string()),
        model: None,
        mode: Some(plan.mode.clone()),
        session_id: plan.session_id.clone(),
        source_path: plan.source_path.clone(),
        parent: None,
        goal: None,
        registered_at: Some(boop::bus::now_iso()),
        base_sha: None,
        worktree_dir: None,
        app_server_socket: plan.app_server_socket.clone(),
    };
    write_route(&dir, name, route.clone())?;
    // Child exit observation stays responsive while transcript projection is
    // independently bounded. The global known-session join ran once above;
    // every pass below reuses and incrementally updates that resident cache.
    const EXIT_POLL: Duration = Duration::from_millis(250);
    let project_every = std::env::var("BOOP_NATIVE_PROJECT_EVERY_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .map(Duration::from_millis)
        .unwrap_or(Duration::from_secs(30));
    let mut last_project = std::time::Instant::now() - project_every;
    loop {
        if let Some(status) = child.try_wait().context("observe native TUI exit")? {
            if status.success() {
                return Ok(());
            }
            let next = match route.session_id.as_deref() {
                Some(session) if respawn_wanted(status, respawns, spawned_at.elapsed()) => {
                    adapter.door().tui_relaunch(
                        &NativeTuiSpec {
                            executable: executable.into(),
                            cwd: cwd.to_path_buf(),
                            args: Vec::new(),
                        },
                        session,
                    )?
                }
                _ => None,
            };
            let Some(next) = next else {
                anyhow::bail!("native {} TUI exited with {status}", adapter.id());
            };
            respawns += 1;
            warn!(
                route = name,
                status = %status,
                attempt = respawns,
                "native TUI died; respawning against its session"
            );
            child = Command::new(&next.program)
                .args(&next.args)
                .env("BOOP_SESSION", name)
                .env("BOOP_LANE", name)
                .current_dir(cwd)
                .spawn()
                .with_context(|| format!("respawn native {} TUI", adapter.id()))?;
            spawned_at = std::time::Instant::now();
            route.app_server_socket = next.app_server_socket.clone();
            route.source_path = next.source_path.clone();
            write_route(&dir, name, route.clone())?;
            continue;
        }
        if last_project.elapsed() < project_every {
            std::thread::sleep(EXIT_POLL);
            continue;
        }
        last_project = std::time::Instant::now();
        // A fresh TUI opens its session at its first prompt, after the route
        // was written; the route learns the id the first tick it exists.
        if route.session_id.is_none() {
            match opened_session(adapter, cwd, opened_ms, Duration::ZERO) {
                Some(session) => {
                    route.source_path = Some(format!("native-session={session}"));
                    route.session_id = Some(session);
                    write_route(&dir, name, route.clone())?;
                }
                None => {
                    std::thread::sleep(EXIT_POLL);
                    continue;
                }
            }
        }
        if let Some(store) = store.as_ref() {
            if let Err(error) = crate::cli::db::sync_native_child_route_once(
                store,
                known.as_mut().expect("projector store has a session cache"),
                adapter,
                name,
                &dir,
                |message| crate::cli::mail::deliver_hail(registry, &dir, message, None),
            ) {
                warn!(%error, route = name, "native child projector pass failed");
            }
        }
        std::thread::sleep(EXIT_POLL);
    }
}

#[cfg(test)]
mod tests {
    use super::{respawn_wanted, RESPAWN_MIN_UPTIME};
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;
    use std::time::Duration;

    #[test]
    fn nonzero_exit_after_min_uptime_respawns() {
        let status = ExitStatus::from_raw(256);
        assert!(respawn_wanted(status, 0, RESPAWN_MIN_UPTIME));
        assert!(respawn_wanted(status, 2, Duration::from_secs(3600)));
    }

    #[test]
    fn signal_death_fast_death_and_exhaustion_end_the_wrapper() {
        let killed = ExitStatus::from_raw(9);
        assert!(!respawn_wanted(killed, 0, Duration::from_secs(3600)));
        let failed = ExitStatus::from_raw(256);
        assert!(!respawn_wanted(failed, 0, Duration::from_secs(1)));
        assert!(!respawn_wanted(failed, 3, Duration::from_secs(3600)));
    }
}
