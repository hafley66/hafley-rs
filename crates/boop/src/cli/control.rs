//! Delivery to a native Codex TUI through its managed app-server.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use boop::bus::Route;
use boop::harness::{Harness, NativeTuiSpec};
use boop::registry::Registry;
use tracing::warn;

use crate::cli::{mail_dir, write_route};

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
    let executable = executable.unwrap_or(adapter.id());
    let mut plan = adapter.prepare_native_tui(&NativeTuiSpec {
        executable: executable.into(),
        cwd: cwd.to_path_buf(),
        args: tui_args.to_vec(),
    })?;
    let dir = mail_dir(mail_dir_arg)?;
    let store = (adapter.id() == "codex")
        .then(|| boop::Store::open(boop::Store::default_path()?))
        .transpose()?;
    let mut child = Command::new(&plan.program)
        .args(&plan.args)
        .current_dir(cwd)
        .spawn()
        .with_context(|| format!("start native {} TUI", adapter.id()))?;
    let mut resolver = plan.session_resolver.take();
    if let Some(resolver) = resolver.as_mut() {
        let resolved = resolver.resolve(Duration::from_secs(10));
        if let Err(error) = resolved {
            let _ = child.kill();
            return Err(error.context("resolve native TUI session identity"));
        }
        plan.session_id = resolved.ok();
        if plan.source_path.is_none() {
            plan.source_path = plan
                .session_id
                .as_ref()
                .map(|session| format!("native-session={session}"));
        }
    }
    write_route(
        &dir,
        name,
        Route {
            kind: "coordinator".into(),
            harness: Some(adapter.id().into()),
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
    if let Some(resolver) = resolver.as_mut() {
        resolver.route_registered()?;
    }
    loop {
        if let Some(status) = child.try_wait().context("observe native TUI exit")? {
            anyhow::ensure!(
                status.success(),
                "native {} TUI exited with {status}",
                adapter.id()
            );
            return Ok(());
        }
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
        std::thread::sleep(Duration::from_millis(250));
    }
}
