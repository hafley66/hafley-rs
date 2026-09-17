use anyhow::Result;
use boop::bus::{self, Route};
use boop::harness::{shell_quote, SpawnSpec};

/// The visible native process reads the same fork brief as a supervised lane.
/// No supervisor startup acknowledgment or completion/idle epilogue runs here.
pub(super) fn command(spec: &SpawnSpec) -> String {
    let mut command = format!(
        "boop tui {} --name {} --mail-dir {}",
        spec.harness,
        shell_quote(&spec.lane),
        shell_quote(&spec.mail_dir.display().to_string()),
    );
    if let Some(bin) = &spec.bin {
        command.push_str(&format!(" --bin {}", shell_quote(bin)));
    }
    let prompt = format!(
        "Read the fork context and answer the request in this brief: {}",
        spec.prompt
    );
    if spec.harness == boop::harness::HarnessId::Kimi {
        command.push_str(&format!(" --initial-prompt {}", shell_quote(&prompt)));
    }
    if spec.harness == boop::harness::HarnessId::Opencode {
        if let Some(effort) = spec.variant.as_ref().or(spec.effort.as_ref()) {
            command.push_str(&format!(" --initial-effort {}", shell_quote(effort)));
        }
    }
    command.push_str(" --");
    if let Some(model) = &spec.model {
        command.push_str(&format!(" --model {}", shell_quote(model)));
    }
    if let Some(effort) = &spec.effort {
        match spec.harness {
            boop::harness::HarnessId::Claude => {
                command.push_str(&format!(" --effort {}", shell_quote(effort)))
            }
            boop::harness::HarnessId::Codex => command.push_str(&format!(
                " -c {}",
                shell_quote(&format!("model_reasoning_effort={effort}"))
            )),
            _ => {}
        }
    }
    match spec.harness {
        boop::harness::HarnessId::Kimi => {}
        boop::harness::HarnessId::Opencode => {
            command.push_str(&format!(" --prompt {}", shell_quote(&prompt)))
        }
        _ => command.push_str(&format!(" {}", shell_quote(&prompt))),
    }
    match &spec.env_stamp {
        Some(stamp) => format!("{stamp} {command}"),
        None => command,
    }
}

pub(super) fn dispatch(
    spec: &SpawnSpec,
    message: &bus::Message,
    parent: Option<String>,
    goal: Option<String>,
) -> Result<()> {
    let cwd = if spec.worktree_dir.is_some() {
        boop::worktree::prepare_spawn_dir(spec)?
    } else {
        spec.repo.clone()
    };
    let tmux = spec.tmux.as_deref().unwrap_or(&spec.lane);
    // Register before starting the native wrapper, which owns subsequent
    // process/session observations. A lane route would reject a TUI owner.
    let route = Route {
        kind: "coordinator".into(),
        harness: Some(spec.harness),
        tmux: Some(tmux.to_owned()),
        cwd: Some(cwd.display().to_string()),
        model: spec.model.clone(),
        mode: None,
        session_id: None,
        source_path: None,
        parent,
        goal,
        registered_at: Some(bus::now_iso()),
        base_sha: super::git_head(&cwd.display().to_string())?,
        worktree_dir: spec
            .worktree_dir
            .as_ref()
            .map(|path| path.display().to_string()),
        app_server_socket: None,
    };
    bus::write_route(&spec.mail_dir, &spec.lane, &route)?;
    boop::tmux::mux().new_detached_session(
        spec.socket.as_deref(),
        tmux,
        &cwd.display().to_string(),
        &command(spec),
    )?;
    super::append_message(&spec.mail_dir, message)?;
    println!(
        "interactive {} -> {} (tmux {tmux})",
        spec.harness, spec.lane
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boop::harness::HarnessId;

    #[test]
    fn claude_fork_command_preserves_settings_and_quotes_the_brief() {
        let spec = SpawnSpec {
            harness: HarnessId::Claude,
            branch: "fork/comment-1".into(),
            base_sha: "main".into(),
            main_tree: false,
            setup: vec![],
            prompt: "/tmp/Chris's brief.md".into(),
            resume_session: None,
            socket: None,
            worktree_dir: None,
            repo: "/tmp".into(),
            env_stamp: Some("BOOP_SESSION='fork-comment-1'".into()),
            model: Some("claude-fable-5-1".into()),
            effort: Some("high".into()),
            variant: None,
            bin: Some("ccz".into()),
            on_exit: Some("unwanted-epilogue".into()),
            tmux: Some("fork-comment-1".into()),
            lane: "fork-comment-1".into(),
            mail_dir: "/tmp/mail root".into(),
            warm_start: false,
        };
        assert_eq!(command(&spec), "BOOP_SESSION='fork-comment-1' boop tui claude --name 'fork-comment-1' --mail-dir '/tmp/mail root' --bin 'ccz' -- --model 'claude-fable-5-1' --effort 'high' 'Read the fork context and answer the request in this brief: /tmp/Chris'\\''s brief.md'");
    }
}
