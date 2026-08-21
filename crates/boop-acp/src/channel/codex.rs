//! The codex lane channel: a `codex app-server` child spoken to over
//! newline JSON-RPC. `turn/steer` puts text into the turn already running.
//!
//! RETIRED as a lane transport: `Codex::open_channel` mints an `AcpChannel`
//! on `CODEX_ADAPTER`. Kept unwired this arc as the rollback door; nothing
//! outside its own tests constructs it. `codex app-server` is not ACP, so the
//! two doors share no frames.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::channel::jsonrpc::RpcChild;
use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEvent};
use boop_store::session::ModelSpec;

const CALL_TIMEOUT: Duration = Duration::from_secs(120);

pub struct CodexChannel {
    rpc: RpcChild,
    thread: String,
    turn: Option<String>,
    effort: Option<String>,
}

impl CodexChannel {
    pub fn open(spec: &ChannelSpec) -> Result<CodexChannel> {
        let mut command = Command::new("codex");
        command.arg("app-server");
        Self::open_command(spec, command)
    }

    /// Connect to the daemon already owned by the native TUI. This never
    /// starts a second app-server.
    pub fn open_proxy(spec: &ChannelSpec, socket: &Path) -> Result<CodexChannel> {
        Self::open_command(spec, proxy_command(socket))
    }

    fn open_command(spec: &ChannelSpec, mut command: Command) -> Result<CodexChannel> {
        let child = command
            .current_dir(&spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(boop_store::trail::child_stderr(spec.lane.as_deref()))
            .spawn()
            .context("connect to Codex app-server")?;
        let mut rpc = RpcChild::attach(child)?;
        rpc.call(
            "initialize",
            json!({"clientInfo": {"name": "boop", "version": env!("CARGO_PKG_VERSION")}}),
            CALL_TIMEOUT,
        )?;
        rpc.notify("initialized", json!({}))?;
        let model_spec = spec
            .model
            .as_deref()
            .filter(|value| !value.is_empty())
            .map(str::parse::<ModelSpec>)
            .transpose()?;
        let mut params = json!({
            "cwd": spec.cwd.display().to_string(),
            "sandbox": "danger-full-access",
            "approvalPolicy": "never",
        });
        if let Some(spec) = &model_spec {
            params["model"] = Value::String(spec.name.clone());
        }
        let (thread, turn) = match &spec.resume {
            Some(id) => {
                params["threadId"] = Value::String(id.clone());
                let reply = rpc.call("thread/resume", params, CALL_TIMEOUT)?;
                let resumed =
                    thread_id(&reply).context("Codex thread/resume returned no thread id")?;
                verify_resumed_thread(id, &resumed)?;
                (resumed, active_turn_id(&reply))
            }
            None => {
                let reply = rpc.call("thread/start", params, CALL_TIMEOUT)?;
                (
                    thread_id(&reply).context("codex thread/start returned no thread id")?,
                    None,
                )
            }
        };
        Ok(CodexChannel {
            rpc,
            thread,
            turn,
            effort: model_spec
                .and_then(|spec| spec.effort)
                .map(|effort| effort.as_str().to_owned()),
        })
    }
}

fn proxy_command(socket: &Path) -> Command {
    let mut command = Command::new("codex");
    command.args(["app-server", "proxy", "--sock"]).arg(socket);
    command
}

fn verify_resumed_thread(expected: &str, actual: &str) -> Result<()> {
    anyhow::ensure!(
        actual == expected,
        "Codex thread/resume returned {actual}, expected {expected}"
    );
    Ok(())
}

fn active_turn_id(reply: &Value) -> Option<String> {
    reply
        .get("thread")?
        .get("turns")?
        .as_array()?
        .iter()
        .rev()
        .find_map(|turn| {
            matches!(
                turn.get("status").and_then(Value::as_str),
                Some("inProgress" | "in_progress")
            )
            .then(|| turn.get("id").and_then(Value::as_str).map(str::to_owned))
            .flatten()
        })
}

impl LaneChannel for CodexChannel {
    fn conversation_id(&self) -> Option<String> {
        Some(self.thread.clone())
    }

    fn start_turn(&mut self, text: &str) -> Result<()> {
        let mut params = json!({
            "threadId": self.thread,
            "input": [{"type": "text", "text": text}],
        });
        if let Some(effort) = &self.effort {
            params["effort"] = Value::String(effort.clone());
        }
        let reply = self.rpc.call("turn/start", params, CALL_TIMEOUT)?;
        self.turn = reply
            .get("turn")
            .and_then(|turn| turn.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        Ok(())
    }

    fn steer(&mut self, text: &str) -> Result<Delivery> {
        let Some(turn) = self.turn.clone() else {
            return Ok(Delivery::NextTurn);
        };
        let params = json!({
            "threadId": self.thread,
            "expectedTurnId": turn,
            "input": [{"type": "text", "text": text}],
        });
        match self.rpc.call("turn/steer", params, CALL_TIMEOUT) {
            Ok(_) => Ok(Delivery::MidTurn),
            Err(_) => Ok(Delivery::NextTurn),
        }
    }

    fn next_event(&mut self, timeout: Duration) -> Result<Option<TurnEvent>> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            let Some(note) = self.rpc.next_notification(left) else {
                return Ok(None);
            };
            let method = note.get("method").and_then(Value::as_str).unwrap_or("");
            let params = note.get("params").cloned().unwrap_or(Value::Null);
            match method {
                "turn/started" => {
                    self.turn = params
                        .get("turn")
                        .and_then(|turn| turn.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                }
                "turn/completed" => {
                    self.turn = None;
                    let status = params
                        .get("turn")
                        .and_then(|turn| turn.get("status"))
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    return Ok(Some(match status {
                        "failed" => TurnEvent::failed(status),
                        _ => TurnEvent::ok(status),
                    }));
                }
                _ => {}
            }
        }
    }

    fn last_activity_ms(&self) -> Option<u64> {
        self.rpc.last_read_ms()
    }

    fn close(&mut self) -> Result<()> {
        self.rpc.close()
    }
}

fn thread_id(reply: &Value) -> Option<String> {
    reply
        .get("thread")
        .and_then(|thread| thread.get("id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thread_id_reads_the_nested_field() {
        let reply = json!({"thread": {"id": "019f-abc"}});
        assert_eq!(thread_id(&reply).as_deref(), Some("019f-abc"));
        assert_eq!(thread_id(&json!({})), None);
    }

    #[test]
    fn resumed_active_turn_is_recovered_for_steering() {
        let reply = json!({"thread": {"turns": [
            {"id": "completed", "status": "completed"},
            {"id": "active", "status": "inProgress"}
        ]}});
        assert_eq!(active_turn_id(&reply).as_deref(), Some("active"));
        assert_eq!(active_turn_id(&json!({"thread": {"turns": []}})), None);
    }

    #[test]
    fn resume_rejects_a_thread_other_than_the_transcript_thread() {
        assert!(verify_resumed_thread("transcript-thread", "other-thread").is_err());
        assert!(verify_resumed_thread("transcript-thread", "transcript-thread").is_ok());
    }

    #[test]
    fn proxy_command_uses_the_managed_daemon_socket() {
        let command = proxy_command(Path::new("/run/boop/codex.sock"));
        let args: Vec<_> = command.get_args().collect();
        assert_eq!(
            args,
            ["app-server", "proxy", "--sock", "/run/boop/codex.sock"]
        );
    }
}
