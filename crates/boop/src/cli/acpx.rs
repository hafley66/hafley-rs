//! Persistent coordinator sessions through the `acpx` ACP client.

use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::{Command, Output};

use anyhow::{Context, Result};

use boop::bus::Route;
use boop::config;
use boop::harness::HarnessId;

use crate::cli::{mail_dir, write_route};

const DEFAULT_ROUTE: &str = "coordinator";
const ACPX_NPM: &str = "acpx@0.13.1";

fn acpx_command() -> Command {
    if let Some(bin) = std::env::var_os("BOOP_ACPX_BIN") {
        return Command::new(bin);
    }
    Command::new("acpx")
}

fn invoke(args: &[String], cwd: &Path) -> Result<Output> {
    match acpx_command().args(args).current_dir(cwd).output() {
        Ok(output) => Ok(output),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Command::new("npx")
            .args(["--yes", ACPX_NPM])
            .args(args)
            .current_dir(cwd)
            .output()
            .context("run acpx through npx"),
        Err(error) => Err(error).context("run acpx"),
    }
}

// acpx defaults to approve-reads, so an unflagged write exits 5. Every queued
// prompt carries its own mode to the queue owner, so ensure alone is not enough.
fn base_args(model: Option<&str>) -> Result<Vec<String>> {
    let mut args: Vec<String> = vec![
        "--format".into(),
        "text".into(),
        "--ttl".into(),
        "0".into(),
        "--approve-all".into(),
        "--non-interactive-permissions".into(),
        "deny".into(),
    ];
    if let Some(model) = model {
        args.extend(["--model".into(), acpx_model(model)?]);
    }
    Ok(args)
}

fn output_detail(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let stdout = stdout.trim();
    let stderr = stderr.trim();
    match (stdout.is_empty(), stderr.is_empty()) {
        (false, false) => format!("stdout: {stdout}; stderr: {stderr}"),
        (false, true) => format!("stdout: {stdout}"),
        (true, false) => format!("stderr: {stderr}"),
        (true, true) => "no output".into(),
    }
}

fn checked(args: &[String], cwd: &Path) -> Result<Output> {
    let output = invoke(args, cwd)?;
    anyhow::ensure!(
        output.status.success(),
        "acpx {} failed ({}): {}",
        args.join(" "),
        output.status,
        output_detail(&output.stdout, &output.stderr)
    );
    Ok(output)
}

fn prompt_args(route: &Route, body: &str, no_wait: bool) -> Result<Vec<String>> {
    let agent = route
        .harness
        .map(HarnessId::as_str)
        .context("ACPX route has no agent")?;
    let session = route
        .session_id
        .as_deref()
        .context("ACPX route has no session")?;
    let mut args = base_args(route.model.as_deref())?;
    args.extend([agent.into(), "-s".into(), session.into()]);
    if no_wait {
        args.push("--no-wait".into());
    }
    args.push(body.into());
    Ok(args)
}

fn acpx_model(model: &str) -> Result<String> {
    let spec = model.parse::<boop::session::ModelSpec>()?;
    Ok(match spec.effort {
        Some(effort) => format!("{}[{}]", spec.name, effort.as_str()),
        None => spec.name,
    })
}

fn prompt(route: &Route, body: &str, no_wait: bool) -> Result<String> {
    let cwd = route.cwd.as_deref().context("ACPX route has no cwd")?;
    let args = prompt_args(route, body, no_wait)?;
    let output = checked(&args, Path::new(cwd))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(crate) fn deliver(route: &Route, body: &str) -> Result<String> {
    prompt(route, body, true)
}

fn resolve_agent_and_model(preset: &str) -> Result<(String, Option<String>)> {
    if matches!(preset, "codex" | "claude" | "gemini" | "opencode" | "kimi") {
        return Ok((preset.to_owned(), None));
    }
    let path = config::default_path()?;
    let row = config::resolve_preset(preset, &path)?;
    let agent = boop::lane::harness_for_preset(&row)?
        .context("model preset does not select an ACP agent")?;
    // acpx spells effort in brackets; the model string itself stays bare.
    let model = match row.effort.as_deref() {
        Some(effort) => format!("{}[{effort}]", row.model),
        None => row.model,
    };
    Ok((agent.as_str().to_owned(), Some(model)))
}

pub(crate) fn run_foreground(
    preset: &str,
    name: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let cwd = std::env::current_dir().context("read coordinator cwd")?;
    let (agent, model) = resolve_agent_and_model(preset)?;
    let name = name.unwrap_or(DEFAULT_ROUTE);
    let dir = mail_dir(mail_dir_arg)?;
    let routes = boop::bus::read_routes(&dir)?;
    let reusable = routes.get(name).is_some_and(|route| {
        route.kind == "coordinator"
            && route.mode.as_deref() == Some("acpx")
            && route.harness.map(HarnessId::as_str) == Some(agent.as_str())
            && route.cwd.as_deref() == Some(cwd.to_string_lossy().as_ref())
    });
    if !reusable {
        let mut args = base_args(model.as_deref())?;
        args.extend([
            agent.clone(),
            "sessions".into(),
            "ensure".into(),
            "--name".into(),
            name.into(),
        ]);
        checked(&args, &cwd)?;
    }
    write_route(
        &dir,
        name,
        Route {
            kind: "coordinator".into(),
            harness: HarnessId::parse(&agent),
            tmux: std::env::var("TMUX_PANE").ok(),
            cwd: Some(cwd.display().to_string()),
            model,
            mode: Some("acpx".into()),
            session_id: Some(name.into()),
            source_path: None,
            parent: None,
            goal: Some("foreground ACP coordinator".into()),
            registered_at: Some(boop::bus::now_iso()),
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        },
    )?;

    println!("registered {name} -> {agent} ACPX session");
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let route = boop::bus::read_routes(&dir)?
            .remove(name)
            .context("coordinator route disappeared")?;
        let response = prompt(&route, &line, false)?;
        stdout.write_all(response.as_bytes())?;
        if !response.ends_with('\n') {
            stdout.write_all(b"\n")?;
        }
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route() -> Route {
        Route {
            kind: "coordinator".into(),
            harness: Some(HarnessId::Codex),
            tmux: None,
            cwd: Some("/tmp/project".into()),
            model: Some("gpt-5.6-terra@medium".into()),
            mode: Some("acpx".into()),
            session_id: Some("main".into()),
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        }
    }

    #[test]
    fn coordinator_mail_uses_the_persistent_queue() {
        assert_eq!(
            prompt_args(&route(), "worker finished", true).unwrap(),
            vec![
                "--format",
                "text",
                "--ttl",
                "0",
                "--approve-all",
                "--non-interactive-permissions",
                "deny",
                "--model",
                "gpt-5.6-terra[medium]",
                "codex",
                "-s",
                "main",
                "--no-wait",
                "worker finished",
            ]
        );
    }

    #[test]
    fn foreground_prompt_waits_for_the_same_session() {
        let args = prompt_args(&route(), "continue", false).unwrap();
        assert_eq!(
            args,
            vec![
                "--format",
                "text",
                "--ttl",
                "0",
                "--approve-all",
                "--non-interactive-permissions",
                "deny",
                "--model",
                "gpt-5.6-terra[medium]",
                "codex",
                "-s",
                "main",
                "continue",
            ]
        );
    }

    #[test]
    fn session_ensure_and_prompts_share_the_permission_policy() {
        let ensure = base_args(Some("gpt-5.6-terra@medium")).unwrap();
        let prompt = prompt_args(&route(), "continue", false).unwrap();
        assert_eq!(prompt[..ensure.len()], ensure[..]);
        assert!(ensure.contains(&"--approve-all".to_string()));
    }

    #[test]
    fn failure_detail_keeps_both_output_streams() {
        assert_eq!(
            output_detail(b"permission denied\n", b"agent starting\n"),
            "stdout: permission denied; stderr: agent starting"
        );
        assert_eq!(
            output_detail(b"", b"agent starting\n"),
            "stderr: agent starting"
        );
        assert_eq!(output_detail(b"done\n", b""), "stdout: done");
        assert_eq!(output_detail(b"", b""), "no output");
    }
}
