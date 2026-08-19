//! OpenCode's Agent Client Protocol transport over its `opencode acp` stdio
//! server. The protocol carries both stream updates and the prompt result.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEvent};

const CALL_TIMEOUT: Duration = Duration::from_secs(120);

type Replies = Arc<(Mutex<HashMap<i64, Value>>, Condvar)>;

struct AcpRpc {
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    next_id: i64,
    replies: Replies,
    notifications: Receiver<Value>,
    closed: Arc<AtomicBool>,
    last_read_ms: Arc<AtomicU64>,
}

impl AcpRpc {
    fn attach(mut child: Child) -> Result<Self> {
        let stdin = Arc::new(Mutex::new(
            child.stdin.take().context("acp child has no stdin")?,
        ));
        let stdout = child.stdout.take().context("acp child has no stdout")?;
        let replies: Replies = Arc::new((Mutex::new(HashMap::new()), Condvar::new()));
        let (sender, notifications): (Sender<Value>, Receiver<Value>) = channel();
        let reader_replies = Arc::clone(&replies);
        let reader_stdin = Arc::clone(&stdin);
        let closed = Arc::new(AtomicBool::new(false));
        let reader_closed = Arc::clone(&closed);
        let last_read_ms = Arc::new(AtomicU64::new(0));
        let reader_clock = Arc::clone(&last_read_ms);
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                reader_clock.store(crate::channel::now_ms(), Ordering::Relaxed);
                let Ok(frame) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if frame.get("id").is_some()
                    && (frame.get("result").is_some() || frame.get("error").is_some())
                {
                    if let Some(id) = frame.get("id").and_then(Value::as_i64) {
                        let (lock, ready) = &*reader_replies;
                        if let Ok(mut replies) = lock.lock() {
                            replies.insert(id, frame);
                            ready.notify_all();
                        }
                    }
                } else if frame.get("id").is_some() {
                    let id = frame.get("id").cloned().unwrap_or(Value::Null);
                    if let Ok(mut pipe) = reader_stdin.lock() {
                        let _ =
                            writeln!(pipe, "{}", json!({"jsonrpc":"2.0", "id":id, "result":{}}));
                        let _ = pipe.flush();
                    }
                } else if sender.send(frame).is_err() {
                    break;
                }
            }
            reader_closed.store(true, Ordering::Relaxed);
            let (_, ready) = &*reader_replies;
            ready.notify_all();
        });
        Ok(Self {
            child,
            stdin,
            next_id: 0,
            replies,
            notifications,
            closed,
            last_read_ms,
        })
    }

    fn request(&mut self, method: &str, params: Value) -> Result<i64> {
        self.next_id += 1;
        let id = self.next_id;
        self.write(&json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}))?;
        Ok(id)
    }

    fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.write(&json!({"jsonrpc":"2.0", "method":method, "params":params}))
    }

    fn write(&self, frame: &Value) -> Result<()> {
        let mut pipe = self
            .stdin
            .lock()
            .map_err(|_| anyhow::anyhow!("acp stdin lock poisoned"))?;
        writeln!(pipe, "{frame}").context("write acp frame")?;
        pipe.flush().context("flush acp frame")?;
        Ok(())
    }

    fn call(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.request(method, params)?;
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(reply) = self.take_reply(id)? {
                return rpc_result(method, reply);
            }
            if self.closed.load(Ordering::Relaxed) {
                anyhow::bail!("acp {method} ended before a response");
            }
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                anyhow::bail!("acp {method} timed out after {timeout:?}");
            }
            let (lock, ready) = &*self.replies;
            let replies = lock
                .lock()
                .map_err(|_| anyhow::anyhow!("acp reply lock poisoned"))?;
            let _ = ready
                .wait_timeout(replies, left)
                .map_err(|_| anyhow::anyhow!("acp reply lock poisoned"))?;
        }
    }

    fn take_reply(&self, id: i64) -> Result<Option<Value>> {
        let (lock, _) = &*self.replies;
        Ok(lock
            .lock()
            .map_err(|_| anyhow::anyhow!("acp reply lock poisoned"))?
            .remove(&id))
    }

    fn next_notification(&self, timeout: Duration) -> Option<Value> {
        self.notifications.recv_timeout(timeout).ok()
    }

    fn last_read_ms(&self) -> Option<u64> {
        match self.last_read_ms.load(Ordering::Relaxed) {
            0 => None,
            value => Some(value),
        }
    }

    fn ended(&mut self) -> bool {
        self.closed.load(Ordering::Relaxed) || self.child.try_wait().ok().flatten().is_some()
    }

    fn close(&mut self) -> Result<()> {
        let _ = self.child.kill();
        self.child.wait().context("wait acp child")?;
        Ok(())
    }
}

fn rpc_result(method: &str, reply: Value) -> Result<Value> {
    if let Some(error) = reply.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or_else(|| error.as_str().unwrap_or("acp error"));
        anyhow::bail!("{message}");
    }
    reply
        .get("result")
        .cloned()
        .context(format!("acp {method} returned no result"))
}

/// ACP-backed OpenCode session. `session/update` records activity while the
/// outstanding `session/prompt` request supplies the terminal stop reason.
pub struct AcpChannel {
    rpc: AcpRpc,
    session: String,
    prompt: Option<i64>,
}

impl AcpChannel {
    pub fn open(spec: &ChannelSpec) -> Result<Self> {
        let child = Command::new("opencode")
            .arg("acp")
            .current_dir(&spec.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(crate::trail::child_stderr(spec.lane.as_deref()))
            .spawn()
            .context("spawn opencode acp")?;
        Self::from_child(spec, child)
    }

    fn from_child(spec: &ChannelSpec, child: Child) -> Result<Self> {
        let mut rpc = AcpRpc::attach(child)?;
        rpc.call("initialize", json!({
            "protocolVersion": 1,
            "clientCapabilities": {"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false},
            "clientInfo": {"name":"boop","version":env!("CARGO_PKG_VERSION")},
        }), CALL_TIMEOUT)?;
        let session_params = json!({"cwd":spec.cwd.display().to_string(),"mcpServers":[]});
        let reply = match &spec.resume {
            Some(session) => rpc.call(
                "session/load",
                json!({"sessionId":session,"cwd":spec.cwd.display().to_string(),"mcpServers":[]}),
                CALL_TIMEOUT,
            )?,
            None => rpc.call("session/new", session_params, CALL_TIMEOUT)?,
        };
        let session = reply
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| spec.resume.clone())
            .context("acp session setup returned no sessionId")?;
        if let Some(model) = spec.model.as_deref().filter(|model| !model.is_empty()) {
            rpc.call(
                "session/set_config_option",
                json!({"sessionId":session,"configId":"model","value":model}),
                CALL_TIMEOUT,
            )?;
        }
        Ok(Self {
            rpc,
            session,
            prompt: None,
        })
    }

    fn poll(&mut self, timeout: Duration) -> Result<Option<TurnEvent>> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(note) = self.rpc.next_notification(Duration::ZERO) {
                if note.get("method").and_then(Value::as_str) == Some("session/update")
                    && note.pointer("/params/sessionId").and_then(Value::as_str)
                        == Some(&self.session)
                {
                    let update = &note["params"]["update"];
                    if matches!(
                        update.get("sessionUpdate").and_then(Value::as_str),
                        Some("agent_message_chunk") | Some("tool_call") | Some("tool_call_update")
                    ) {
                        return Ok(Some(TurnEvent::Started));
                    }
                }
                continue;
            }
            if let Some(id) = self.prompt {
                if let Some(reply) = self.rpc.take_reply(id)? {
                    self.prompt = None;
                    return match rpc_result("session/prompt", reply) {
                        Ok(result) => Ok(Some(TurnEvent::ok(
                            result
                                .get("stopReason")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown"),
                        ))),
                        Err(error) => Ok(Some(TurnEvent::failed(error.to_string()))),
                    };
                }
            }
            if self.rpc.ended() {
                self.prompt = None;
                return Ok(Some(TurnEvent::failed(
                    "acp stream ended before prompt result",
                )));
            }
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            let Some(note) = self.rpc.next_notification(left) else {
                continue;
            };
            if note.get("method").and_then(Value::as_str) != Some("session/update") {
                continue;
            }
            if note.pointer("/params/sessionId").and_then(Value::as_str) != Some(&self.session) {
                continue;
            }
            let update = &note["params"]["update"];
            match update.get("sessionUpdate").and_then(Value::as_str) {
                Some("agent_message_chunk") | Some("tool_call") | Some("tool_call_update") => {
                    return Ok(Some(TurnEvent::Started))
                }
                _ => {}
            }
        }
    }
}

impl LaneChannel for AcpChannel {
    fn conversation_id(&self) -> Option<String> {
        Some(self.session.clone())
    }

    fn conversation_id_kind(&self) -> &'static str {
        "opencode_session"
    }

    fn start_turn(&mut self, text: &str) -> Result<()> {
        if self.prompt.is_some() {
            anyhow::bail!("an acp prompt is already running");
        }
        self.prompt = Some(self.rpc.request(
            "session/prompt",
            json!({"sessionId":self.session,"prompt":[{"type":"text","text":text}]}),
        )?);
        Ok(())
    }

    fn steer(&mut self, _text: &str) -> Result<Delivery> {
        Ok(Delivery::NextTurn)
    }

    fn next_event(&mut self, timeout: Duration) -> Result<Option<TurnEvent>> {
        self.poll(timeout)
    }

    fn interrupt(&mut self) -> Result<()> {
        self.rpc
            .notify("session/cancel", json!({"sessionId":self.session}))
    }

    fn last_activity_ms(&self) -> Option<u64> {
        self.rpc.last_read_ms()
    }

    fn close(&mut self) -> Result<()> {
        self.rpc.close()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ChannelSpec {
        ChannelSpec {
            model: Some("openrouter/deepseek/deepseek-v4-flash-0731".into()),
            cwd: std::env::temp_dir(),
            resume: None,
            lane: None,
        }
    }

    fn fake(script: &str) -> AcpChannel {
        let child = Command::new("sh")
            .args(["-c", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        AcpChannel::from_child(&spec(), child).unwrap()
    }

    #[test]
    fn prompt_updates_and_stop_reason_arrive_from_acp() {
        let mut channel = fake(
            r#"
read line; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}'
read line; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"ses_fake"}}'
read line; printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{}}'
read line; printf '%s\n' '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"ses_fake","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"hello"}}}}'; printf '%s\n' '{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"ses_fake","update":{"sessionUpdate":"tool_call","toolCallId":"tool-1","title":"rg","kind":"search","status":"pending"}}}}'; printf '%s\n' '{"jsonrpc":"2.0","id":4,"result":{"stopReason":"end_turn"}}'
"#,
        );
        channel.start_turn("hello").unwrap();
        assert!(matches!(
            channel.next_event(Duration::from_secs(1)).unwrap(),
            Some(TurnEvent::Started)
        ));
        assert_eq!(
            channel
                .next_event(Duration::from_secs(1))
                .unwrap()
                .unwrap()
                .detail(),
            "end_turn"
        );
    }

    #[test]
    fn an_early_provider_error_is_the_turn_end_reason_verbatim() {
        let mut channel = fake(
            r#"
read line; printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}'
read line; printf '%s\n' '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"ses_fake"}}'
read line; printf '%s\n' '{"jsonrpc":"2.0","id":3,"result":{}}'
read line; printf '%s\n' '{"jsonrpc":"2.0","id":4,"error":{"code":-32000,"message":"provider stream ended early"}}'
"#,
        );
        channel.start_turn("hello").unwrap();
        let event = channel.next_event(Duration::from_secs(1)).unwrap().unwrap();
        assert_eq!(event.detail(), "provider stream ended early");
    }
}
