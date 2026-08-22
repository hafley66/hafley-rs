//! The codex lane channel: a `codex app-server` child spoken to over
//! newline JSON-RPC. `turn/steer` puts text into the turn already running.
//!
//! RETIRED as a lane transport: `Codex::open_channel` mints an `AcpChannel`
//! on `CODEX_ADAPTER`. Kept unwired this arc as the rollback door; nothing
//! outside its own tests constructs it. `codex app-server` is not ACP, so the
//! two doors share no frames.

use std::collections::HashMap;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};

use crate::channel::jsonrpc::RpcChild;
use crate::channel::{ChannelSpec, Delivery, LaneChannel, TurnEvent};
use boop_store::session::ModelSpec;

const CALL_TIMEOUT: Duration = Duration::from_secs(120);
const INTERACTIVE_START_TIMEOUT: Duration = Duration::from_secs(10);

/// A transparent WebSocket relay whose only interpretation is correlating
/// Codex JSON-RPC thread creation replies with their request ids.
pub struct InspectingProxy {
    socket: PathBuf,
    identity: Receiver<Result<String, String>>,
    release: mpsc::SyncSender<()>,
}

impl InspectingProxy {
    pub fn start(upstream: &Path) -> Result<Self> {
        let socket = std::env::temp_dir().join(format!(
            "boop-codex-proxy-{}-{}.sock",
            std::process::id(),
            crate::channel::now_ms()
        ));
        let listener = UnixListener::bind(&socket)
            .with_context(|| format!("bind Codex inspecting proxy {}", socket.display()))?;
        let upstream = upstream.to_path_buf();
        let cleanup = socket.clone();
        let (send, identity) = mpsc::sync_channel(1);
        let (release, released) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("boop-codex-inspecting-proxy".into())
            .spawn(move || {
                let result = tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .build()
                    .context("build Codex inspecting proxy runtime")
                    .and_then(|runtime| {
                        runtime.block_on(relay_inspecting(listener, &upstream, &send, &released))
                    });
                if let Err(error) = result {
                    let _ = send.try_send(Err(format!("{error:#}")));
                }
                let _ = std::fs::remove_file(cleanup);
            })
            .context("start Codex inspecting proxy")?;
        Ok(Self {
            socket,
            identity,
            release,
        })
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    pub fn resolve(&mut self, timeout: Duration) -> Result<String> {
        self.identity
            .recv_timeout(timeout)
            .with_context(|| format!("Codex TUI did not establish a thread within {timeout:?}"))?
            .map_err(anyhow::Error::msg)
    }

    pub fn route_registered(&mut self) -> Result<()> {
        self.release
            .send(())
            .context("release Codex TUI after route registration")
    }
}

impl Drop for InspectingProxy {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
    }
}

async fn relay_inspecting(
    listener: UnixListener,
    upstream: &Path,
    identity: &mpsc::SyncSender<Result<String, String>>,
    released: &Receiver<()>,
) -> Result<()> {
    listener.set_nonblocking(true)?;
    let listener = tokio::net::UnixListener::from_std(listener)?;
    let (client_stream, _) = listener
        .accept()
        .await
        .context("accept Codex TUI WebSocket")?;
    let mut client = tokio_tungstenite::accept_async(client_stream)
        .await
        .context("accept Codex TUI handshake")?;
    let upstream_stream = tokio::net::UnixStream::connect(upstream)
        .await
        .with_context(|| format!("connect upstream Codex socket {}", upstream.display()))?;
    let (mut server, _) = tokio_tungstenite::client_async("ws://localhost/", upstream_stream)
        .await
        .context("upgrade upstream Codex WebSocket")?;
    let mut pending = HashMap::<String, String>::new();
    let mut reported = false;
    loop {
        tokio::select! {
            message = client.next() => match message {
            Some(Ok(message)) => {
                inspect_request(&message, &mut pending);
                server.send(message).await.context("relay Codex TUI request")?;
            }
            Some(Err(error)) => return Err(error).context("read Codex TUI WebSocket"),
            None => return Ok(()),
        },
            message = server.next() => match message {
            Some(Ok(message)) => {
                if !reported {
                    if let Some(thread) = inspect_response(&message, &mut pending) {
                        let _ = identity.try_send(Ok(thread));
                        released.recv_timeout(INTERACTIVE_START_TIMEOUT).context(
                            "route was not registered before Codex thread reply release",
                        )?;
                        reported = true;
                    }
                }
                client
                    .send(message)
                    .await
                    .context("relay Codex server response")?;
            }
            Some(Err(error)) => return Err(error).context("read upstream Codex WebSocket"),
            None => return Ok(()),
        }
        }
    }
}

fn inspect_request(message: &tungstenite::Message, pending: &mut HashMap<String, String>) {
    let tungstenite::Message::Text(text) = message else {
        return;
    };
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let Some(method @ ("thread/start" | "thread/resume")) =
        value.get("method").and_then(Value::as_str)
    else {
        return;
    };
    let Some(id) = value.get("id") else { return };
    pending.insert(id.to_string(), method.to_owned());
}

fn inspect_response(
    message: &tungstenite::Message,
    pending: &mut HashMap<String, String>,
) -> Option<String> {
    let tungstenite::Message::Text(text) = message else {
        return None;
    };
    let value = serde_json::from_str::<Value>(text).ok()?;
    let id = value.get("id")?.to_string();
    pending.remove(&id)?;
    value.get("result").and_then(thread_id)
}

/// The subset of `thread/start` settings that the interactive Codex CLI
/// exposes directly. `None` leaves the managed app-server's configured
/// default intact.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InteractiveThreadStart {
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub sandbox: Option<String>,
    pub approval_policy: Option<String>,
    pub approvals_reviewer: Option<String>,
}

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

    /// Start the thread an interactive native TUI will resume through the
    /// managed app-server. This has no lane defaults: omitted settings retain
    /// the same configured defaults a normal interactive TUI receives.
    pub fn start_interactive_proxy(
        start: &InteractiveThreadStart,
        socket: &Path,
    ) -> Result<String> {
        start_interactive_websocket(start, socket, INTERACTIVE_START_TIMEOUT)
    }
}

fn start_interactive_websocket(
    start: &InteractiveThreadStart,
    socket: &Path,
    timeout: Duration,
) -> Result<String> {
    let stream = UnixStream::connect(socket).with_context(|| {
        format!(
            "connect to Codex remote-control socket {}",
            socket.display()
        )
    })?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    let (mut websocket, _) = tungstenite::client("ws://localhost/", stream)
        .context("upgrade Codex remote-control UDS to WebSocket")?;
    websocket_call(
        &mut websocket,
        1,
        "initialize",
        json!({"clientInfo": {"name": "boop", "version": env!("CARGO_PKG_VERSION")}}),
        timeout,
    )?;
    websocket.send(tungstenite::Message::Text(
        json!({"jsonrpc": "2.0", "method": "initialized", "params": {}})
            .to_string()
            .into(),
    ))?;
    let reply = websocket_call(
        &mut websocket,
        2,
        "thread/start",
        interactive_thread_params(start),
        timeout,
    )?;
    let _ = websocket.close(None);
    thread_id(&reply).context("Codex interactive thread/start returned no thread id")
}

impl CodexChannel {
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

fn websocket_call(
    websocket: &mut tungstenite::WebSocket<UnixStream>,
    id: i64,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value> {
    websocket.send(tungstenite::Message::Text(
        json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params})
            .to_string()
            .into(),
    ))?;
    loop {
        let message = websocket
            .read()
            .with_context(|| format!("Codex WebSocket {method} timed out after {timeout:?}"))?;
        let tungstenite::Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text)
            .with_context(|| format!("decode Codex WebSocket {method} reply"))?;
        if value.get("id").and_then(Value::as_i64) != Some(id) {
            continue;
        }
        if let Some(error) = value.get("error") {
            anyhow::bail!("Codex WebSocket {method} failed: {error}");
        }
        return Ok(value.get("result").cloned().unwrap_or(Value::Null));
    }
}

fn proxy_command(socket: &Path) -> Command {
    let mut command = Command::new("codex");
    command.args(["app-server", "proxy", "--sock"]).arg(socket);
    command
}

fn interactive_thread_params(start: &InteractiveThreadStart) -> Value {
    let mut params = json!({"cwd": start.cwd.display().to_string()});
    if let Some(model) = &start.model {
        params["model"] = Value::String(model.clone());
    }
    if let Some(sandbox) = &start.sandbox {
        params["sandbox"] = Value::String(sandbox.clone());
    }
    if let Some(approval_policy) = &start.approval_policy {
        params["approvalPolicy"] = Value::String(approval_policy.clone());
    }
    if let Some(approvals_reviewer) = &start.approvals_reviewer {
        params["approvalsReviewer"] = Value::String(approvals_reviewer.clone());
    }
    params
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
    use std::os::unix::net::UnixListener;

    #[test]
    fn inspecting_proxy_relays_frames_and_captures_matching_thread_start() {
        let upstream_socket = std::env::temp_dir().join(format!(
            "boop-codex-upstream-{}-{}.sock",
            std::process::id(),
            crate::channel::now_ms()
        ));
        let listener = UnixListener::bind(&upstream_socket).unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut websocket = tungstenite::accept(stream).unwrap();
            let request: Value =
                serde_json::from_str(websocket.read().unwrap().into_text().unwrap().as_str())
                    .unwrap();
            assert_eq!(request["method"], "thread/start");
            websocket
                .send(tungstenite::Message::Text(
                    json!({
                        "jsonrpc": "2.0",
                        "id": request["id"],
                        "result": {"thread": {"id": "actual-parent"}}
                    })
                    .to_string()
                    .into(),
                ))
                .unwrap();
        });
        let mut proxy = InspectingProxy::start(&upstream_socket).unwrap();
        let stream = UnixStream::connect(proxy.socket()).unwrap();
        let (mut client, _) = tungstenite::client("ws://localhost/", stream).unwrap();
        client
            .send(tungstenite::Message::Text(
                json!({"jsonrpc": "2.0", "id": 91, "method": "thread/start", "params": {}})
                    .to_string()
                    .into(),
            ))
            .unwrap();
        assert_eq!(
            proxy.resolve(Duration::from_secs(1)).unwrap(),
            "actual-parent"
        );
        proxy.route_registered().unwrap();
        let reply: Value =
            serde_json::from_str(client.read().unwrap().into_text().unwrap().as_str()).unwrap();
        assert_eq!(reply["result"]["thread"]["id"], "actual-parent");
        server.join().unwrap();
        let _ = std::fs::remove_file(upstream_socket);
    }

    #[test]
    fn interactive_start_speaks_websocket_json_rpc_over_uds() {
        let socket = std::env::temp_dir().join(format!(
            "boop-codex-websocket-{}-{}.sock",
            std::process::id(),
            crate::channel::now_ms()
        ));
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut websocket = tungstenite::accept(stream).unwrap();
            let initialize: Value =
                serde_json::from_str(websocket.read().unwrap().into_text().unwrap().as_str())
                    .unwrap();
            assert_eq!(initialize["method"], "initialize");
            websocket
                .send(tungstenite::Message::Text(
                    json!({"jsonrpc":"2.0","id":1,"result":{}})
                        .to_string()
                        .into(),
                ))
                .unwrap();
            let initialized: Value =
                serde_json::from_str(websocket.read().unwrap().into_text().unwrap().as_str())
                    .unwrap();
            assert_eq!(initialized["method"], "initialized");
            let start: Value =
                serde_json::from_str(websocket.read().unwrap().into_text().unwrap().as_str())
                    .unwrap();
            assert_eq!(start["method"], "thread/start");
            websocket
                .send(tungstenite::Message::Text(
                    json!({"jsonrpc":"2.0","id":2,"result":{"thread":{"id":"exact-parent"}}})
                        .to_string()
                        .into(),
                ))
                .unwrap();
        });
        let thread = start_interactive_websocket(
            &InteractiveThreadStart {
                cwd: PathBuf::from("/repo"),
                ..Default::default()
            },
            &socket,
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(thread, "exact-parent");
        server.join().unwrap();
        std::fs::remove_file(socket).unwrap();
    }

    #[test]
    fn interactive_start_times_out_when_the_websocket_peer_is_silent() {
        let socket = std::env::temp_dir().join(format!(
            "boop-codex-websocket-timeout-{}-{}.sock",
            std::process::id(),
            crate::channel::now_ms()
        ));
        let listener = UnixListener::bind(&socket).unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut websocket = tungstenite::accept(stream).unwrap();
            let _ = websocket.read().unwrap();
            std::thread::sleep(Duration::from_millis(300));
        });
        let started = std::time::Instant::now();
        let error = start_interactive_websocket(
            &InteractiveThreadStart {
                cwd: PathBuf::from("/repo"),
                ..Default::default()
            },
            &socket,
            Duration::from_millis(100),
        )
        .unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(
            error.to_string().contains("timed out after 100ms"),
            "{error:#}"
        );
        server.join().unwrap();
        std::fs::remove_file(socket).unwrap();
    }

    #[test]
    #[ignore = "requires a running Codex remote-control daemon and BOOP_CODEX_TEST_SOCKET"]
    fn live_remote_control_starts_an_exact_parent_thread() {
        let socket = std::env::var_os("BOOP_CODEX_TEST_SOCKET")
            .map(PathBuf::from)
            .expect("BOOP_CODEX_TEST_SOCKET");
        let thread = CodexChannel::start_interactive_proxy(
            &InteractiveThreadStart {
                cwd: std::env::current_dir().unwrap(),
                ..Default::default()
            },
            &socket,
        )
        .unwrap();
        assert!(!thread.is_empty());
        eprintln!("thread={thread}");
    }

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

    #[test]
    fn interactive_start_uses_server_defaults_when_the_tui_supplies_no_override() {
        let params = interactive_thread_params(&InteractiveThreadStart {
            cwd: PathBuf::from("/repo"),
            ..InteractiveThreadStart::default()
        });
        assert_eq!(params, json!({"cwd": "/repo"}));
    }

    #[test]
    fn interactive_start_sends_only_explicit_tui_settings() {
        let params = interactive_thread_params(&InteractiveThreadStart {
            cwd: PathBuf::from("/repo"),
            model: Some("gpt-5.6".into()),
            sandbox: Some("workspace-write".into()),
            approval_policy: Some("on-request".into()),
            approvals_reviewer: Some("auto_review".into()),
        });
        assert_eq!(
            params,
            json!({
                "cwd": "/repo",
                "model": "gpt-5.6",
                "sandbox": "workspace-write",
                "approvalPolicy": "on-request",
                "approvalsReviewer": "auto_review",
            })
        );
    }
}
