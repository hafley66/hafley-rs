//! Delivery to a native Codex TUI through its managed app-server.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use boop::bus::Route;
use boop::channel::codex::CodexChannel;
use boop::channel::{ChannelSpec, Delivery, LaneChannel};

use crate::cli::{mail_dir, write_route};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeliveryReceipt {
    Steered,
    Started,
}

fn receipt_for_delivery(delivery: Delivery) -> DeliveryReceipt {
    match delivery {
        Delivery::MidTurn => DeliveryReceipt::Steered,
        Delivery::NextTurn => DeliveryReceipt::Started,
    }
}

/// Use only the daemon socket recorded by `boop codex`. The proxy subprocess
/// is a stdio bridge, not an app-server instance.
pub(crate) fn deliver(route: &Route, body: &str) -> Result<DeliveryReceipt> {
    let socket = route.app_server_socket.as_deref().context(
        "native Codex route has no managed app-server socket; start it with `boop codex`",
    )?;
    let thread = route
        .session_id
        .as_deref()
        .context("native Codex route has no verified thread id")?;
    let cwd = route
        .cwd
        .as_deref()
        .context("native Codex route has no cwd")?;
    let spec = ChannelSpec {
        model: route.model.clone(),
        cwd: cwd.into(),
        resume: Some(thread.into()),
        lane: None,
    };
    let mut channel = CodexChannel::open_proxy(&spec, Path::new(socket))?;
    anyhow::ensure!(
        channel.conversation_id().as_deref() == Some(thread),
        "Codex proxy resumed another thread"
    );
    let delivery = channel.steer(body)?;
    if delivery == Delivery::NextTurn {
        channel.start_turn(body)?;
    }
    let receipt = receipt_for_delivery(delivery);
    channel.close()?;
    Ok(receipt)
}

/// Start Codex's managed remote-control daemon, register its socket, then run
/// the ordinary TUI against it. No Codex executable or symlink is modified.
pub(crate) fn run_native_tui(
    name: Option<&str>,
    cwd: &Path,
    mail_dir_arg: Option<&Path>,
    tui_args: &[String],
) -> Result<()> {
    let pane = std::env::var("TMUX_PANE")
        .ok()
        .filter(|pane| !pane.is_empty())
        .context("`boop codex` requires TMUX_PANE so its route matches Codex identity")?;
    let default_name = format!("codex-{}", pane.trim_start_matches('%'));
    let name = name.unwrap_or(&default_name);
    anyhow::ensure!(
        name == default_name,
        "native Codex route name must be {default_name} for its TMUX_PANE"
    );
    let output = Command::new("codex")
        .args(["remote-control", "start", "--json"])
        .output()
        .context("start managed Codex remote-control daemon")?;
    anyhow::ensure!(
        output.status.success(),
        "Codex remote-control start failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let socket = daemon_socket_from_start(&String::from_utf8_lossy(&output.stdout))
        .context("Codex remote-control start did not report an app-server socket")?;
    let (requested_thread, forwarded_args) = explicit_resume(tui_args)?;
    let spec = ChannelSpec {
        model: None,
        cwd: cwd.to_path_buf(),
        resume: requested_thread,
        lane: None,
    };
    let mut channel = CodexChannel::open_proxy(&spec, Path::new(&socket))?;
    let thread = channel
        .conversation_id()
        .context("Codex app-server did not return the native TUI thread id")?;
    channel.close()?;
    let dir = mail_dir(mail_dir_arg)?;
    write_route(
        &dir,
        name,
        Route {
            kind: "coordinator".into(),
            harness: Some("codex".into()),
            tmux: Some(pane),
            cwd: Some(cwd.display().to_string()),
            model: None,
            mode: Some("native-remote".into()),
            session_id: Some(thread.clone()),
            source_path: Some(format!("managed-app-server={socket}")),
            parent: None,
            goal: None,
            registered_at: Some(boop::bus::now_iso()),
            base_sha: None,
            worktree_dir: None,
            app_server_socket: Some(socket.clone()),
        },
    )?;
    let status = Command::new("codex")
        .arg("resume")
        .arg(&thread)
        .arg("--remote")
        .arg(format!("unix://{socket}"))
        .arg("--cd")
        .arg(cwd)
        .args(forwarded_args)
        .status()
        .context("start native Codex TUI through managed daemon")?;
    anyhow::ensure!(status.success(), "native Codex TUI exited with {status}");
    Ok(())
}

fn explicit_resume(tui_args: &[String]) -> Result<(Option<String>, &[String])> {
    if tui_args.first().map(String::as_str) != Some("resume") {
        return Ok((None, tui_args));
    }
    let thread = tui_args
        .get(1)
        .filter(|value| !value.starts_with('-'))
        .context("`boop codex -- resume` requires an explicit thread id")?;
    Ok((Some(thread.clone()), &tui_args[2..]))
}

fn daemon_socket_from_start(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    value
        .get("daemon")
        .and_then(|daemon| daemon.get("socketPath"))
        .and_then(serde_json::Value::as_str)
        .filter(|socket| socket.ends_with(".sock"))
        .map(str::to_owned)
        .or_else(|| find_socket(&value))
}

fn find_socket(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) if value.ends_with(".sock") => Some(value.clone()),
        serde_json::Value::Array(values) => values.iter().find_map(find_socket),
        serde_json::Value::Object(values) => values.values().find_map(find_socket),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_native_route_requires_socket_thread_and_cwd_before_proxy_spawn() {
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
    fn remote_control_start_socket_is_read_without_assuming_its_json_key() {
        let output =
            r#"{"daemon":{"socketPath":"/tmp/codex.sock","otherSocket":"/tmp/wrong.sock"}}"#;
        assert_eq!(
            daemon_socket_from_start(output).as_deref(),
            Some("/tmp/codex.sock")
        );
    }

    #[test]
    fn idle_and_active_turns_choose_start_and_steer() {
        assert_eq!(
            receipt_for_delivery(Delivery::NextTurn),
            DeliveryReceipt::Started
        );
        assert_eq!(
            receipt_for_delivery(Delivery::MidTurn),
            DeliveryReceipt::Steered
        );
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

    #[test]
    fn explicit_resume_is_registered_before_the_tui_starts() {
        let args = vec![
            "resume".to_string(),
            "019ffb9b-51cb-7e92-be44-4eb469f46d95".to_string(),
            "--no-alt-screen".to_string(),
        ];
        let (thread, forwarded) = explicit_resume(&args).expect("explicit resume");
        assert_eq!(
            thread.as_deref(),
            Some("019ffb9b-51cb-7e92-be44-4eb469f46d95")
        );
        assert_eq!(forwarded, ["--no-alt-screen"]);
    }

    #[test]
    fn a_fresh_launch_reserves_a_thread_before_the_tui_starts() {
        let args = vec!["--no-alt-screen".to_string()];
        let (thread, forwarded) = explicit_resume(&args).expect("fresh launch");
        assert_eq!(thread, None);
        assert_eq!(forwarded, args);
    }
}
