//! The claude lane channel: one long-lived `claude -p` child in stream-json
//! mode. Extra user lines written to its stdin land inside the running turn.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEvent};

pub struct ClaudeChannel {
    child: Child,
    stdin: ChildStdin,
    events: Receiver<Value>,
    conversation: String,
    /// Epoch millis of the newest line the claude child wrote. The stall
    /// watchdog reads this so a healthy long turn is not killed as silent.
    last_event_ms: Arc<AtomicU64>,
}

impl ClaudeChannel {
    pub fn open(spec: &ChannelSpec) -> Result<ClaudeChannel> {
        Self::open_with_binary(spec, "claude")
    }

    fn open_with_binary(spec: &ChannelSpec, binary: &str) -> Result<ClaudeChannel> {
        let conversation = spec.resume.clone().unwrap_or_else(new_uuid);
        let last_event_ms = Arc::new(AtomicU64::new(0));
        let activity = Arc::clone(&last_event_ms);
        let mut command = Command::new(binary);
        command
            .arg("-p")
            .args(["--input-format", "stream-json"])
            .args(["--output-format", "stream-json"])
            .arg("--verbose")
            .arg("--dangerously-skip-permissions");
        match &spec.resume {
            Some(id) => {
                command.args(["--resume", id]);
            }
            None => {
                command.args(["--session-id", &conversation]);
            }
        }
        if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
            command.args(["--model", model]);
        }
        let mut child = command
            .current_dir(&spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("spawn claude stream-json child")?;
        let stdin = child.stdin.take().context("claude child has no stdin")?;
        let stdout = child.stdout.take().context("claude child has no stdout")?;
        let (sender, events) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                activity.store(crate::channel::now_ms(), Ordering::Relaxed);
                let Ok(value) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        Ok(ClaudeChannel {
            child,
            stdin,
            events,
            conversation,
            last_event_ms,
        })
    }

    fn write_user(&mut self, text: &str) -> Result<()> {
        let frame = json!({
            "type": "user",
            "message": {"role": "user", "content": [{"type": "text", "text": text}]}
        });
        writeln!(self.stdin, "{frame}").context("write claude user line")?;
        self.stdin.flush().context("flush claude stdin")?;
        Ok(())
    }
}

impl LaneChannel for ClaudeChannel {
    fn conversation_id(&self) -> Option<String> {
        Some(self.conversation.clone())
    }

    fn start_turn(&mut self, text: &str) -> Result<()> {
        self.write_user(text)
    }

    fn steer(&mut self, text: &str) -> Result<Delivery> {
        self.write_user(text)?;
        Ok(Delivery::MidTurn)
    }

    fn next_event(&mut self, timeout: std::time::Duration) -> Result<Option<TurnEvent>> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            let event = match self.events.recv_timeout(left) {
                Ok(event) => event,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => return Ok(None),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Ok(Some(TurnEvent::failed(
                        "claude stream closed before a result event",
                    )))
                }
            };
            if let Some(id) = event.get("session_id").and_then(Value::as_str) {
                self.conversation = id.to_owned();
            }
            if event.get("type").and_then(Value::as_str) != Some("result") {
                continue;
            }
            let subtype = event
                .get("subtype")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let errored = event
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            return Ok(Some(match errored {
                false => TurnEvent::ok(subtype),
                true => TurnEvent::failed(subtype),
            }));
        }
    }

    fn last_activity_ms(&self) -> Option<u64> {
        let ms = self.last_event_ms.load(Ordering::Relaxed);
        (ms > 0).then_some(ms)
    }

    fn close(&mut self) -> Result<()> {
        drop(std::mem::replace(&mut self.stdin, blackhole()?));
        self.child.wait().context("wait claude child")?;
        Ok(())
    }
}

/// A stdin handle to drop into so the real one can be closed without an
/// `Option` field the rest of the impl would have to unwrap.
fn blackhole() -> Result<ChildStdin> {
    let mut child = Command::new("true")
        .stdin(Stdio::piped())
        .spawn()
        .context("spawn placeholder for a closed stdin")?;
    child.stdin.take().context("placeholder has no stdin")
}

/// A version-4 UUID, the only session id shape `--session-id` accepts.
fn new_uuid() -> String {
    let mut bytes = [0u8; 16];
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    let mixed = (nanos as u64) ^ ((std::process::id() as u64) << 32);
    bytes[..8].copy_from_slice(&mixed.to_be_bytes());
    bytes[8..].copy_from_slice(&(nanos as u64).rotate_left(17).to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minted_session_id_is_a_v4_uuid() {
        let id = new_uuid();
        assert_eq!(id.len(), 36, "{id}");
        let parts: Vec<&str> = id.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12],
            "{id}"
        );
        assert!(parts[2].starts_with('4'), "{id}");
        assert!(
            matches!(&parts[3][0..1], "8" | "9" | "a" | "b"),
            "{id} variant nibble"
        );
    }

    #[test]
    fn two_mints_differ() {
        assert_ne!(new_uuid(), new_uuid());
    }

    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    fn spec() -> ChannelSpec {
        ChannelSpec {
            model: None,
            cwd: std::env::temp_dir(),
            resume: None,
        }
    }

    /// Write a fake `claude` shell script that runs `body`, and return its path.
    /// The channel spawns it through the same `open` path as the real binary.
    fn fake_claude(name: &str, body: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("boop-claude-fake-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("claude");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).unwrap();
        path
    }

    /// A stream-json `result` event with `is_error: false`: the shape claude
    /// emits when a session completes cleanly.
    const COMPLETED: &str =
        r#"{"type":"result","subtype":"success","is_error":false,"result":"done"}"#;

    fn poll_after_open(binary: &Path, feed: bool) -> TurnEvent {
        let mut channel =
            ClaudeChannel::open_with_binary(&spec(), &binary.display().to_string()).unwrap();
        if feed {
            let _ = channel.start_turn("do the lane");
        }
        channel
            .next_event(Duration::from_secs(5))
            .unwrap()
            .expect("the fake claude transcript yields a turn end")
    }

    /// RECEIPT. A completed transcript wins over a nonzero exit: the claude CLI
    /// may exit 1 after a finished session, but the result event is the truth.
    #[test]
    fn a_completed_transcript_reports_ok_even_when_the_cli_exits_nonzero() {
        let binary = fake_claude("exit-one", &format!("printf '%s\\n' '{COMPLETED}'\nexit 1"));
        let end = poll_after_open(&binary, true);
        assert!(
            end.is_done(),
            "completed transcript must not fail: {}",
            end.detail()
        );
    }

    /// RECEIPT. The inverse case: a zero exit with a completed transcript is
    /// also a success.
    #[test]
    fn a_zero_exit_with_a_completed_transcript_reports_ok() {
        let binary = fake_claude(
            "exit-zero",
            &format!("printf '%s\\n' '{COMPLETED}'\nexit 0"),
        );
        let end = poll_after_open(&binary, true);
        assert!(
            end.is_done(),
            "clean completion must succeed: {}",
            end.detail()
        );
    }

    /// RECEIPT. A spawn that exits before emitting anything is a genuine
    /// failure: no result event, so the turn ends failed.
    #[test]
    fn a_spawn_that_emits_nothing_reports_failed() {
        let binary = fake_claude("no-output", "exit 7");
        let end = poll_after_open(&binary, false);
        assert!(!end.is_done(), "an empty transcript must report failure");
    }

    /// RECEIPT. Activity from the reader thread is visible to the stall
    /// watchdog, so a long turn is measured against the mid-turn bound, not
    /// the first-signal bound.
    #[test]
    fn streamed_activity_is_reported_to_the_stall_watchdog() {
        let binary = fake_claude(
            "activity",
            "printf '%s\\n' '{\"type\":\"assistant\"}'\nsleep 5\n",
        );
        let channel =
            ClaudeChannel::open_with_binary(&spec(), &binary.display().to_string()).unwrap();
        let mut seen = None;
        for _ in 0..20 {
            if let Some(ms) = channel.last_activity_ms() {
                seen = Some(ms);
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            seen.is_some(),
            "the reader thread must surface the child's activity"
        );
    }
}
