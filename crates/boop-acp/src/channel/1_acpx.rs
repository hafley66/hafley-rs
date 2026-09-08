//! Persistent named coordinator queues through the installed ACPX client.

use anyhow::{Context, Result};
use boop_store::{bus::Route, harness_id::HarnessId};
use std::path::Path;
use std::process::{Command, Output};

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
    let agent = route_agent(route).context("ACPX route has no agent")?;
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
    let spec = model.parse::<boop_store::session::ModelSpec>()?;
    Ok(match spec.effort {
        Some(effort) => format!("{}[{}]", spec.name, effort.as_str()),
        None => spec.name,
    })
}

pub fn prompt(route: &Route, body: &str, no_wait: bool) -> Result<String> {
    let cwd = route.cwd.as_deref().context("ACPX route has no cwd")?;
    let args = prompt_args(route, body, no_wait)?;
    let output = checked(&args, Path::new(cwd))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The CLI agent roster belongs to this transport, including agents with
/// no Boop transcript adapter.
pub fn recognizes_agent(name: &str) -> bool {
    matches!(name, "codex" | "claude" | "gemini" | "opencode" | "kimi")
}

/// Named ACP queues retain their raw agent selector. Older registered routes
/// carried only the typed harness field, which remains readable.
pub fn route_agent(route: &Route) -> Option<&str> {
    route
        .source_path
        .as_deref()
        .and_then(|source| source.strip_prefix("acpx-agent="))
        .or_else(|| route.harness.map(HarnessId::as_str))
}

pub fn ensure(agent: &str, session: &str, model: Option<&str>, cwd: &Path) -> Result<()> {
    let mut args = base_args(model)?;
    args.extend([
        agent.into(),
        "sessions".into(),
        "ensure".into(),
        "--name".into(),
        session.into(),
    ]);
    checked(&args, cwd)?;
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
    fn an_acpx_agent_without_a_transcript_adapter_retains_its_selector() {
        let mut gemini = route();
        gemini.harness = None;
        gemini.source_path = Some("acpx-agent=gemini".into());
        assert!(recognizes_agent("gemini"));
        assert_eq!(route_agent(&gemini), Some("gemini"));
        let args = prompt_args(&gemini, "probe", true).unwrap();
        assert_eq!(
            &args[args.len() - 5..],
            ["gemini", "-s", "main", "--no-wait", "probe"]
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
