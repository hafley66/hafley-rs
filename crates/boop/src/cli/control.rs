//! Delivery to a native Codex TUI through its managed app-server.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use boop::bus::Route;
use boop::harness::{Harness, NativeTuiSpec};

use crate::cli::{mail_dir, write_route};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeliveryReceipt {
    Queued,
}

/// Use Codex's supported remote queue command against the daemon socket
/// recorded by `boop tui codex`.
pub(crate) fn deliver(route: &Route, body: &str) -> Result<DeliveryReceipt> {
    let socket = route.app_server_socket.as_deref().context(
        "native Codex route has no managed app-server socket; start it with `boop codex`",
    )?;
    let thread = route
        .session_id
        .as_deref()
        .context("native Codex route has no verified thread id")?;
    let output = Command::new("codex")
        .args(["queue", "--thread", thread, "--message", body, "--remote"])
        .arg(format!("unix://{socket}"))
        .output()
        .context("queue message through Codex remote control")?;
    anyhow::ensure!(
        output.status.success(),
        "Codex remote queue failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(DeliveryReceipt::Queued)
}

/// Ask the selected harness adapter to prepare its native process, register
/// this pane as its coordinator, then run the ordinary interactive TUI.
pub(crate) fn run_native_tui(
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
    let plan = adapter.prepare_native_tui(&NativeTuiSpec {
        executable: executable.into(),
        cwd: cwd.to_path_buf(),
        args: tui_args.to_vec(),
    })?;
    let dir = mail_dir(mail_dir_arg)?;
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
    let status = Command::new(&plan.program)
        .args(&plan.args)
        .current_dir(cwd)
        .status()
        .with_context(|| format!("start native {} TUI", adapter.id()))?;
    anyhow::ensure!(
        status.success(),
        "native {} TUI exited with {status}",
        adapter.id()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_native_route_requires_a_socket_before_remote_queue() {
        let route = Route {
            kind: "coordinator".into(),
            harness: Some("codex".into()),
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
        assert!(deliver(&route, "mail")
            .unwrap_err()
            .to_string()
            .contains("managed app-server socket"));
    }

    #[test]
    fn concurrent_native_sessions_in_one_cwd_require_their_own_thread_evidence() {
        let route = Route {
            kind: "coordinator".into(),
            harness: Some("codex".into()),
            tmux: Some("%12".into()),
            cwd: Some("/shared".into()),
            model: None,
            mode: Some("native-remote".into()),
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
            app_server_socket: Some("/tmp/codex.sock".into()),
        };
        assert!(deliver(&route, "mail")
            .unwrap_err()
            .to_string()
            .contains("verified thread id"));
    }
}
