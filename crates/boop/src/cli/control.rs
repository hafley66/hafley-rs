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
                session.observed_ms >= opened_ms
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
    let opened_ms = boop::live::now_ms();
    let mut child = Command::new(&plan.program)
        .args(&plan.args)
        .current_dir(cwd)
        .spawn()
        .with_context(|| format!("start native {} TUI", adapter.id()))?;
    if plan.session_id.is_none() {
        plan.session_id = opened_session(adapter, cwd, opened_ms, SESSION_WAIT);
        if let Some(session) = plan.session_id.as_deref() {
            plan.source_path = Some(format!("native-session={session}"));
        }
    }
    write_route(
        &dir,
        name,
        Route {
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
        },
    )?;
    // The projector pass joins every sync_cursor row (~190ms on a 4k-session
    // store); running it each 250ms tick burned a core per wrapper.
    const EXIT_POLL: Duration = Duration::from_millis(250);
    const PROJECT_EVERY: Duration = Duration::from_secs(2);
    let mut last_project = std::time::Instant::now() - PROJECT_EVERY;
    loop {
        if let Some(status) = child.try_wait().context("observe native TUI exit")? {
            anyhow::ensure!(
                status.success(),
                "native {} TUI exited with {status}",
                adapter.id()
            );
            return Ok(());
        }
        if last_project.elapsed() < PROJECT_EVERY {
            std::thread::sleep(EXIT_POLL);
            continue;
        }
        last_project = std::time::Instant::now();
        if let Some(store) = store.as_ref() {
            if let Err(error) = crate::cli::db::sync_native_child_route_once(
                store,
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
