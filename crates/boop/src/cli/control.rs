//! Delivery to a native Codex TUI through its managed app-server.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use boop::bus::Route;
use boop::harness::{Harness, NativeTuiSpec};

use crate::cli::{mail_dir, write_route};

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
