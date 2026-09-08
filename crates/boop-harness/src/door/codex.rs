//! The codex door: `~/.codex/state_5.sqlite` says which threads exist, and the
//! remote-control daemon's socket is where a message for one is queued.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::os::unix::process::CommandExt;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::door::{Delivered, Door, IdleNotice};
use crate::harness::{HarnessId, NativeTuiEvent, NativeTuiObserver, NativeTuiPlan, NativeTuiSpec};
use crate::live::{now_ms, DoorAddress, LiveSession, LiveSessionScope, LiveSessions, LiveStatus};

/// Overrides the state database the thread list is read from.
pub const STATE_DB_ENV: &str = "BOOP_CODEX_STATE_DB";

/// Overrides the remote-control socket a delivery is queued through.
pub const SOCKET_ENV: &str = "BOOP_CODEX_APP_SERVER_SOCKET";

/// A thread whose last update is older than this is not a running TUI.
const RECENT_MS: u64 = 24 * 60 * 60 * 1000;

/// Reads the codex state database and queues through the app-server socket.
pub struct CodexDoor {
    db: Option<PathBuf>,
    socket: Option<PathBuf>,
}

impl CodexDoor {
    /// The state database and daemon socket of the codex installed here.
    pub const fn machine() -> Self {
        CodexDoor {
            db: None,
            socket: None,
        }
    }

    /// A state database and socket named outright, which is what a test hands in.
    pub fn at(db: impl Into<PathBuf>, socket: impl Into<PathBuf>) -> Self {
        CodexDoor {
            db: Some(db.into()),
            socket: Some(socket.into()),
        }
    }

    fn state_db(&self) -> Result<PathBuf> {
        if let Some(db) = &self.db {
            return Ok(db.clone());
        }
        if let Some(db) = std::env::var_os(STATE_DB_ENV).filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(db));
        }
        Ok(codex_home()?.join("state_5.sqlite"))
    }

    /// The daemon socket `codex remote-control start` maintains. It is a
    /// fixed path under the codex home, so a reader needs no handshake.
    fn socket(&self) -> Result<PathBuf> {
        if let Some(socket) = &self.socket {
            return Ok(socket.clone());
        }
        if let Some(socket) = std::env::var_os(SOCKET_ENV).filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(socket));
        }
        Ok(codex_home()?
            .join("app-server-control")
            .join("app-server-control.sock"))
    }
}

fn codex_home() -> Result<PathBuf> {
    let home = dirs::home_dir().context("resolve home directory")?;
    Ok(home.join(".codex"))
}

impl LiveSessions for CodexDoor {
    fn live_session_for_route(&self, route: &boop_store::bus::Route) -> Result<Option<LiveSession>> {
        let Some(id) = route.session_id.as_deref() else { return Ok(None); };
        if let Some(socket) = route.app_server_socket.as_deref() {
            // Owned TUI events identify the thread before legacy state_5
            // discovery can see it (including paginated-history threads).
            return Ok(Some(LiveSession {
                harness: HarnessId::Codex,
                session_id: id.to_owned(),
                pid: None,
                cwd: route.cwd.as_ref().map(PathBuf::from),
                tmux_pane: route.tmux.clone(),
                status: LiveStatus::Unknown,
                door: DoorAddress::AppServer { socket: PathBuf::from(socket), thread: id.to_owned() },
                observed_ms: now_ms(), started_ms: None,
                scope: LiveSessionScope::Root, parent_session: None,
            }));
        }
        Ok(self.live_sessions()?.into_iter().find(|session| session.session_id == id))
    }

    /// `threads` is the codex thread registry: `id`, `cwd`, `updated_at_ms`,
    /// `archived`. It records no pid and no pane, so a route supplies those.
    fn live_sessions(&self) -> Result<Vec<LiveSession>> {
        let db = self.state_db()?;
        if !db.exists() {
            return Ok(Vec::new());
        }
        let socket = self.socket()?;
        let connection = rusqlite::Connection::open_with_flags(
            &db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_URI,
        )
        .with_context(|| format!("open {}", db.display()))?;
        let mut statement = connection.prepare(
            "SELECT id, cwd, COALESCE(updated_at_ms, updated_at * 1000), \
             COALESCE(created_at_ms, created_at * 1000), source \
             FROM threads WHERE archived = 0 ORDER BY updated_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<i64>>(2)?.unwrap_or(0) as u64,
                row.get::<_, Option<i64>>(3)?.map(|ms| ms as u64),
                row.get::<_, String>(4)?,
            ))
        })?;
        let floor = now_ms().saturating_sub(RECENT_MS);
        let mut live = Vec::new();
        for row in rows {
            let (id, cwd, updated_ms, created_ms, source) = row?;
            if updated_ms < floor {
                continue;
            }
            let source = serde_json::from_str::<serde_json::Value>(&source).ok();
            let subagent = source.as_ref().and_then(|value| value.get("subagent"));
            let parent_session = subagent
                .and_then(|value| value.get("thread_spawn"))
                .and_then(|value| value.get("parent_thread_id"))
                .and_then(|value| value.as_str())
                .map(str::to_owned);
            live.push(LiveSession {
                harness: HarnessId::Codex,
                session_id: id.clone(),
                pid: None,
                cwd: cwd.map(PathBuf::from),
                tmux_pane: None,
                // The thread table records no turn state; the app-server
                // notification stream is what answers busy or idle.
                status: LiveStatus::Unknown,
                door: DoorAddress::AppServer {
                    socket: socket.clone(),
                    thread: id,
                },
                observed_ms: updated_ms,
                started_ms: created_ms,
                scope: if subagent.is_some() {
                    LiveSessionScope::Child
                } else {
                    LiveSessionScope::Root
                },
                parent_session,
            });
        }
        // Guardian sources carry no parent id. Codex creates the guardian
        // immediately after its interactive thread, so recover the closest
        // preceding root in the same cwd. Explicit thread_spawn parents above
        // remain authoritative.
        let roots = live
            .iter()
            .filter(|session| session.scope == LiveSessionScope::Root)
            .map(|session| {
                (
                    session.session_id.clone(),
                    session.cwd.clone(),
                    session.started_ms,
                )
            })
            .collect::<Vec<_>>();
        for session in live.iter_mut().filter(|session| {
            session.scope == LiveSessionScope::Child && session.parent_session.is_none()
        }) {
            session.parent_session = roots
                .iter()
                .filter(|(_, cwd, started)| {
                    cwd == &session.cwd
                        && match (started, session.started_ms) {
                            (Some(root), Some(child)) => root <= &child,
                            _ => false,
                        }
                })
                .max_by_key(|(_, _, started)| *started)
                .map(|(id, _, _)| id.clone());
        }
        Ok(live)
    }
}

impl Door for CodexDoor {
    fn deliver(&self, session: &LiveSession, body: &str) -> Result<Delivered> {
        let DoorAddress::AppServer { socket, thread } = &session.door else {
            return Ok(Delivered::Unreachable(format!(
                "codex thread `{}` names no app-server socket",
                session.session_id
            )));
        };
        match queue_message(socket, thread, body) {
            Ok(()) => Ok(Delivered::Injected),
            Err(error) => Ok(Delivered::Unreachable(format!("{error}"))),
        }
    }

    /// `thread/status/changed` on the daemon's notification stream: the
    /// notice is the first `idle` for this thread after the subscription.
    fn notify_idle(&self, session: &LiveSession, timeout: Duration) -> Result<IdleNotice> {
        let DoorAddress::AppServer { socket, thread } = &session.door else {
            anyhow::bail!(
                "codex thread `{}` names no app-server socket",
                session.session_id
            );
        };
        wait_for_idle(Path::new(socket), thread, timeout)
    }

    /// Each wrapper owns its backend, environment and observed thread events.
    fn tui_launch(&self, spec: &NativeTuiSpec) -> Result<NativeTuiPlan> {
        // Codex rejects symlinked socket parents; macOS /tmp is a symlink.
        let backend_root = tempfile::Builder::new().prefix("boop-codex-")
            .tempdir_in(std::fs::canonicalize("/tmp")?)?;
        let socket = backend_root.path().join("control.sock").display().to_string();
        let frontend = backend_root.path().join("tui.sock").display().to_string();
        let (requested_thread, forwarded) = explicit_resume(&spec.args)?;
        let mut command = Command::new(&spec.executable);
        command.args(["app-server", "--listen", &format!("unix://{socket}")]);
        command.args(server_config_args(&spec.args)?);
        let backend = command.envs(spec.env.iter().cloned())
            .current_dir(&spec.cwd).process_group(0)
            .stdin(Stdio::null()).stdout(Stdio::null())
            .stderr(boop_store::trail::child_stderr(spec.env.iter().find(|(key, _)| key == "BOOP_SESSION").map(|(_, value)| value.as_str())))
            .spawn().context("start owned Codex app-server")?;
        let mut plan = NativeTuiPlan {
            program: spec.executable.clone(),
            args: native_tui_args(requested_thread.as_deref(), &frontend, &spec.cwd, forwarded),
            mode: "native-owned".into(),
            session_id: None,
            source_path: Some(format!("owned-app-server={socket}")),
            app_server_socket: Some(socket.clone()),
            observer: None,
            backend: Some(backend),
            backend_root: Some(backend_root),
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !Path::new(&socket).exists() {
            anyhow::ensure!(plan.backend.as_mut().unwrap().try_wait()?.is_none(), "owned Codex app-server exited before opening its socket");
            anyhow::ensure!(std::time::Instant::now() < deadline, "owned Codex app-server socket timed out");
            std::thread::sleep(Duration::from_millis(25));
        }
        plan.observer = Some(observe_tui(&frontend, &socket)?);
        Ok(plan)
    }

    /// Resume under a new owned backend after an abnormal process exit.
    fn tui_relaunch(&self, spec: &NativeTuiSpec, session: &str) -> Result<Option<NativeTuiPlan>> {
        let mut resume = spec.clone();
        resume.args = vec!["resume".into(), session.into()];
        resume.args.extend(server_config_args(&spec.args)?);
        self.tui_launch(&resume).map(Some)
    }
}

/// Process config reaches the backend that executes tools and reads trust.
fn server_config_args(args: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" | "--config" | "--enable" | "--disable" => {
                out.push(arg.clone());
                out.push(args.next().with_context(|| format!("{arg} needs a value"))?.clone());
            }
            "--strict-config" => out.push(arg.clone()),
            _ if arg.starts_with("--config=") || arg.starts_with("--enable=") || arg.starts_with("--disable=") => out.push(arg.clone()),
            _ => {}
        }
    }
    Ok(out)
}

/// A selected TUI response identifies the conversation. Global started events
/// also describe background work and cannot select a route.
fn tui_event(value: &serde_json::Value, selected: bool) -> Option<NativeTuiEvent> {
    let string = |v: &serde_json::Value| v.as_str().map(str::to_owned);
    if selected {
        let result = &value["result"];
        let thread = &result["thread"];
        if thread["ephemeral"].as_bool() == Some(true)
            || thread["parentThreadId"].as_str().is_some()
            || thread["source"].get("subAgent").is_some() {
            return None;
        }
        return Some(NativeTuiEvent::Session {
            session_id: string(&thread["id"])?,
            model: string(&result["model"]).or_else(|| string(&thread["model"])),
            effort: string(&result["reasoningEffort"]).or_else(|| string(&thread["reasoningEffort"])),
        });
    }
    let params = &value["params"];
    match value["method"].as_str()? {
        "thread/settings/updated" => Some(NativeTuiEvent::Settings {
            session_id: string(&params["threadId"])?,
            model: string(&params["threadSettings"]["model"]),
            effort: string(&params["threadSettings"]["effort"]),
        }),
        "thread/closed" => Some(NativeTuiEvent::Closed { session_id: string(&params["threadId"])? }),
        _ => None,
    }
}

async fn emit_tui_event(send: &std::sync::mpsc::SyncSender<NativeTuiEvent>, stop: &std::sync::atomic::AtomicBool, mut event: NativeTuiEvent) {
    while !stop.load(std::sync::atomic::Ordering::Acquire) {
        match send.try_send(event) {
            Ok(()) | Err(std::sync::mpsc::TrySendError::Disconnected(_)) => return,
            Err(std::sync::mpsc::TrySendError::Full(pending)) => event = pending,
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

/// Forward the actual TUI connection and observe its responses. Mail targets
/// the backend directly. Observation adds no thread selection or subscription.
fn observe_tui(frontend: &str, backend: &str) -> Result<NativeTuiObserver> {
    use std::os::unix::net::UnixListener;
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
    let listener = UnixListener::bind(frontend).context("bind owned Codex TUI socket")?;
    listener.set_nonblocking(true)?;
    let backend = backend.to_owned();
    let (send, events) = std::sync::mpsc::sync_channel(64);
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
    let worker = std::thread::spawn(move || runtime.block_on(async move {
        let listener = match tokio::net::UnixListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                emit_tui_event(&send, &stopping, NativeTuiEvent::Failed(error.to_string())).await;
                return;
            }
        };
        let mut connections = tokio::task::JoinSet::new();
        let mut tick = tokio::time::interval(Duration::from_millis(20));
        loop {
            tokio::select! {
                _ = tick.tick() => if stopping.load(Ordering::Acquire) { break; },
                result = listener.accept() => match result {
                    Ok((client, _)) => {
                        let send = send.clone();
                        let stop = stopping.clone();
                        let backend = backend.clone();
                        connections.spawn(async move {
                            if let Err(error) = forward_tui(client, &backend, &send, &stop).await {
                                emit_tui_event(&send, &stop, NativeTuiEvent::Failed(error.to_string())).await;
                            }
                        });
                    }
                    Err(error) => {
                        emit_tui_event(&send, &stopping, NativeTuiEvent::Failed(error.to_string())).await;
                        break;
                    }
                },
                _ = connections.join_next(), if !connections.is_empty() => {}
            }
        }
        connections.shutdown().await;
    }));
    Ok(NativeTuiObserver { events, stop, worker: Some(worker) })
}

async fn forward_tui(client: tokio::net::UnixStream, backend: &str, send: &std::sync::mpsc::SyncSender<NativeTuiEvent>, stop: &std::sync::atomic::AtomicBool) -> Result<()> {
    use futures_util::{SinkExt, StreamExt};
    use tungstenite::Message;
    let (client, server) = tokio::time::timeout(Duration::from_secs(5), async {
        let client = tokio_tungstenite::accept_async(client).await.context("accept TUI websocket")?;
        let server = tokio::net::UnixStream::connect(backend).await.context("connect owned backend")?;
        let (server, _) = tokio_tungstenite::client_async("ws://localhost/", server).await.context("connect backend websocket")?;
        anyhow::Ok((client, server))
    }).await.context("TUI connection handshake timed out")??;
    let (mut client_send, mut client_read) = client.split();
    let (mut server_send, mut server_read) = server.split();
    let selections = std::sync::Mutex::new(std::collections::BTreeSet::new());
    let outgoing = async {
        while let Some(frame) = client_read.next().await {
            let frame = frame.context("read TUI frame")?;
            if let Message::Text(text) = &frame {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                    if matches!(value["method"].as_str(), Some("thread/start" | "thread/resume" | "thread/fork")) && !value["id"].is_null() {
                        selections.lock().unwrap().insert(value["id"].to_string());
                    }
                }
            }
            let closing = frame.is_close();
            server_send.send(frame).await.context("forward TUI frame")?;
            if closing { break; }
        }
        anyhow::Ok(())
    };
    let incoming = async {
        while let Some(frame) = server_read.next().await {
            let frame = frame.context("read backend frame")?;
            if let Message::Text(text) = &frame {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
                    let selected = selections.lock().unwrap().remove(&value["id"].to_string());
                    if let Some(event) = tui_event(&value, selected) {
                        emit_tui_event(send, stop, event).await;
                    }
                }
            }
            let closing = frame.is_close();
            client_send.send(frame).await.context("forward backend frame")?;
            if closing { break; }
        }
        anyhow::Ok(())
    };
    tokio::select! { result = outgoing => result, result = incoming => result }
}

/// One websocket on the remote-control socket, `initialize`, then read
/// notifications until `thread/status/changed` says `idle` for `thread`.
fn wait_for_idle(socket: &Path, thread: &str, timeout: Duration) -> Result<IdleNotice> {
    use std::os::unix::net::UnixStream;
    use tungstenite::{client::IntoClientRequest, Message};
    let deadline = std::time::Instant::now() + timeout;
    let stream = UnixStream::connect(socket)
        .with_context(|| format!("connect codex remote-control socket {}", socket.display()))?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    let request = "ws://localhost/".into_client_request()?;
    let (mut ws, _) = tungstenite::client(request, stream)
        .map_err(|error| anyhow::anyhow!("websocket handshake with codex app-server: {error}"))?;
    let hello = serde_json::json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"clientInfo": {"name": "boop", "title": "boop", "version": env!("CARGO_PKG_VERSION")}}
    });
    ws.send(Message::Text(hello.to_string().into()))?;
    loop {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("codex thread `{thread}` stayed active for {timeout:?}");
        }
        let message = match ws.read() {
            Ok(message) => message,
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue
            }
            Err(error) => return Err(error.into()),
        };
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if value.get("method").and_then(serde_json::Value::as_str) != Some("thread/status/changed")
        {
            continue;
        }
        let params = &value["params"];
        if params.get("threadId").and_then(serde_json::Value::as_str) != Some(thread) {
            continue;
        }
        let status = params
            .pointer("/status/type")
            .and_then(serde_json::Value::as_str);
        if status == Some("idle") {
            return Ok(IdleNotice::now(Some("idle".into())));
        }
    }
}

fn native_tui_args(
    thread: Option<&str>,
    socket: &str,
    cwd: &Path,
    forwarded: &[String],
) -> Vec<std::ffi::OsString> {
    let mut args = Vec::new();
    if let Some(thread) = thread {
        args.push("resume".into());
        args.push(thread.into());
    }
    args.extend([
        "--remote".into(),
        format!("unix://{socket}").into(),
        "--cd".into(),
        cwd.as_os_str().to_owned(),
    ]);
    args.extend(forwarded.iter().map(Into::into));
    args
}

fn explicit_resume(tui_args: &[String]) -> anyhow::Result<(Option<String>, &[String])> {
    if tui_args.first().map(String::as_str) != Some("resume") {
        return Ok((None, tui_args));
    }
    // `resume`, `resume --last`, `resume --all`: the TUI picks the thread
    // itself; the whole argument list forwards and the wrapper adopts the
    // thread once the TUI reports it.
    let Some(thread) = tui_args.get(1).filter(|value| !value.starts_with('-')) else {
        return Ok((None, tui_args));
    };
    Ok((Some(thread.clone()), &tui_args[2..]))
}

/// Queue one message for a thread through the remote-control daemon. This is
/// the one place boop spells the `codex queue` command.
pub fn queue_message(socket: &Path, thread: &str, text: &str) -> Result<()> {
    // The executable name is the one the id declares, never a literal here.
    let program = HarnessId::Codex
        .process_names()
        .first()
        .copied()
        .context("codex declares no process name")?;
    let output = Command::new(program)
        .args(["queue", "--thread", thread, "--message", text, "--remote"])
        .arg(format!("unix://{}", socket.display()))
        .output()
        .context("queue message through Codex remote control")?;
    anyhow::ensure!(
        output.status.success(),
        "Codex remote queue failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The columns this reader names, in the shape codex 0.149 writes them.
    const SCHEMA: &str = "CREATE TABLE threads (
        id TEXT PRIMARY KEY,
        rollout_path TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        source TEXT NOT NULL,
        model_provider TEXT NOT NULL,
        cwd TEXT NOT NULL,
        title TEXT NOT NULL,
        sandbox_policy TEXT NOT NULL,
        approval_mode TEXT NOT NULL,
        tokens_used INTEGER NOT NULL DEFAULT 0,
        has_user_event INTEGER NOT NULL DEFAULT 0,
        archived INTEGER NOT NULL DEFAULT 0,
        archived_at INTEGER,
        created_at_ms INTEGER,
        updated_at_ms INTEGER)";

    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Fixture {
            let dir = std::env::temp_dir().join(format!(
                "boop-codex-door-{}-{}-{name}",
                std::process::id(),
                now_ms()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let connection = rusqlite::Connection::open(dir.join("state_5.sqlite")).unwrap();
            connection.execute_batch(SCHEMA).unwrap();
            let now = now_ms() as i64;
            let rows = [
                ("01a02a8b-live", "/Users/someone/projects", now, 0),
                ("01a02a8b-archived", "/Users/someone/projects", now, 1),
                (
                    "01a02a8b-stale",
                    "/Users/someone/old",
                    now - 3 * RECENT_MS as i64,
                    0,
                ),
            ];
            for (id, cwd, updated_ms, archived) in rows {
                connection
                    .execute(
                        "INSERT INTO threads (id, rollout_path, created_at, updated_at, source, \
                         model_provider, cwd, title, sandbox_policy, approval_mode, archived, \
                         created_at_ms, updated_at_ms) \
                         VALUES (?1, '', ?2, ?2, 'cli', 'openai', ?3, '', 'workspace', 'on-request', ?4, ?5, ?5)",
                        rusqlite::params![id, updated_ms / 1000, cwd, archived, updated_ms],
                    )
                    .unwrap();
            }
            Fixture { dir }
        }

        fn door(&self) -> CodexDoor {
            CodexDoor::at(
                self.dir.join("state_5.sqlite"),
                self.dir.join("daemon.sock"),
            )
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// RECEIPT. Live threads come back with the daemon socket as their door;
    /// archived and long-stale rows do not.
    #[test]
    fn the_thread_table_lists_live_threads_only() {
        let fixture = Fixture::new("threads");
        let live = fixture.door().live_sessions().unwrap();
        assert_eq!(
            live.iter()
                .map(|session| session.session_id.as_str())
                .collect::<Vec<_>>(),
            vec!["01a02a8b-live"]
        );
        let session = &live[0];
        assert_eq!(session.harness, HarnessId::Codex);
        assert_eq!(session.cwd, Some(PathBuf::from("/Users/someone/projects")));
        assert_eq!(session.status, LiveStatus::Unknown);
        assert_eq!(session.scope, LiveSessionScope::Root);
        assert_eq!(session.parent_session, None);
        assert_eq!(
            session.door,
            DoorAddress::AppServer {
                socket: fixture.dir.join("daemon.sock"),
                thread: "01a02a8b-live".into(),
            }
        );
    }

    #[test]
    fn the_thread_source_marks_guardian_and_spawned_threads_as_children() {
        let fixture = Fixture::new("thread-scope");
        let connection = rusqlite::Connection::open(fixture.dir.join("state_5.sqlite")).unwrap();
        let now = now_ms() as i64;
        for (id, source) in [
            ("guardian", r#"{"subagent":{"other":"guardian"}}"#),
            (
                "spawned",
                r#"{"subagent":{"thread_spawn":{"parent_thread_id":"parent","depth":1}}}"#,
            ),
        ] {
            connection
                .execute(
                    "INSERT INTO threads (id, rollout_path, created_at, updated_at, source, \
                 model_provider, cwd, title, sandbox_policy, approval_mode, archived, \
                 created_at_ms, updated_at_ms) \
                 VALUES (?1, '', ?2, ?2, ?3, 'openai', '/Users/someone/projects', '', \
                 'workspace', 'on-request', 0, ?4, ?4)",
                    rusqlite::params![id, now / 1000, source, now],
                )
                .unwrap();
        }
        let scopes = fixture
            .door()
            .live_sessions()
            .unwrap()
            .into_iter()
            .map(|session| (session.session_id, session.scope))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(scopes.get("guardian"), Some(&LiveSessionScope::Child));
        assert_eq!(scopes.get("spawned"), Some(&LiveSessionScope::Child));
        assert_eq!(scopes.get("01a02a8b-live"), Some(&LiveSessionScope::Root));

        let parents = fixture
            .door()
            .live_sessions()
            .unwrap()
            .into_iter()
            .map(|session| (session.session_id, session.parent_session))
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            parents.get("spawned").and_then(Option::as_deref),
            Some("parent")
        );
        assert_eq!(
            parents.get("guardian").and_then(Option::as_deref),
            Some("01a02a8b-live")
        );
    }

    /// RECEIPT. A machine with no state database has no codex running, which
    /// is an empty list rather than a raised error.
    #[test]
    fn a_missing_state_database_lists_nothing() {
        let door = CodexDoor::at("/nonexistent/state_5.sqlite", "/nonexistent/daemon.sock");
        assert!(door.live_sessions().unwrap().is_empty());
    }

    #[test]
    fn backend_configuration_is_forwarded_without_replaying_the_prompt() {
        let args = ["resume", "thread-1", "-m", "gpt-6-astra", "-c", "model_reasoning_effort=max", "--enable=x", "bounded prompt"]
            .map(str::to_owned);
        assert_eq!(
            server_config_args(&args).unwrap(),
            ["-c", "model_reasoning_effort=max", "--enable=x"]
        );
    }

    #[test]
    fn native_lifecycle_events_keep_roots_settings_and_closure_separate() {
        use serde_json::json;
        let events = [
            (json!({"id":1,"result":{"thread":{"id":"root","source":"vscode"},"model":"gpt-6-astra","reasoningEffort":"max"}}), true),
            (json!({"id":2,"result":{"thread":{"id":"child","parentThreadId":"root","source":{"subAgent":{}}}}}), true),
            (json!({"id":3,"result":{"thread":{"id":"guardian","ephemeral":true}}}), true),
            (json!({"method":"thread/started","params":{"thread":{"id":"another-root"}}}), false),
            (json!({"id":4,"result":{"thread":{"id":"read-only-query"}}}), false),
            (json!({"id":5,"error":{"message":"bad resume id"}}), true),
            (json!({"method":"thread/settings/updated","params":{"threadId":"root","threadSettings":{"model":"gpt-5.6-luna","effort":"low"}}}), false),
            (json!({"method":"thread/closed","params":{"threadId":"root"}}), false),
            (json!({"method":"thread/compacted","params":{"threadId":"root"}}), false),
        ];
        assert_eq!(events.iter().filter_map(|(value, selected)| tui_event(value, *selected)).collect::<Vec<_>>(), vec![
            NativeTuiEvent::Session { session_id:"root".into(), model:Some("gpt-6-astra".into()), effort:Some("max".into()) },
            NativeTuiEvent::Settings { session_id:"root".into(), model:Some("gpt-5.6-luna".into()), effort:Some("low".into()) },
            NativeTuiEvent::Closed { session_id:"root".into() },
        ]);
    }

    #[test]
    fn native_connection_forwards_large_frames_in_both_directions_and_observes_resume() {
        use std::os::unix::net::{UnixListener, UnixStream};
        use serde_json::json;
        use tungstenite::Message;
        let root = tempfile::tempdir_in(std::fs::canonicalize("/tmp").unwrap()).unwrap();
        let backend = root.path().join("backend.sock");
        let frontend = root.path().join("tui.sock");
        let listener = UnixListener::bind(&backend).unwrap();
        let observer = observe_tui(frontend.to_str().unwrap(), backend.to_str().unwrap()).unwrap();
        let fake = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            stream.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
            let mut server = tungstenite::accept(stream).unwrap();
            for (id, session, model, effort) in [(1, "first", "gpt-5.6-luna", "low"), (2, "resumed", "gpt-5.6-terra", "high")] {
                let request: serde_json::Value = serde_json::from_str(&server.read().unwrap().into_text().unwrap()).unwrap();
                assert_eq!(request["id"], id);
                let response = json!({"id":id,"result":{"thread":{"id":session,"ephemeral":false},"model":model,"reasoningEffort":effort},"padding":"x".repeat(512_000)});
                server.send(Message::Text(response.to_string().into())).unwrap();
            }
            let _ = server.close(None);
        });
        let stream = UnixStream::connect(&frontend).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        stream.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
        let (mut client, _) = tungstenite::client("ws://localhost/", stream).unwrap();
        for (id, method) in [(1, "thread/start"), (2, "thread/resume")] {
            client.send(Message::Text(json!({"id":id,"method":method,"params":{"padding":"y".repeat(512_000)}}).to_string().into())).unwrap();
        }
        for id in [1, 2] {
            let response: serde_json::Value = serde_json::from_str(&client.read().unwrap().into_text().unwrap()).unwrap();
            assert_eq!((response["id"].as_i64(), response["padding"].as_str().unwrap().len()), (Some(id), 512_000));
        }
        let events = (0..2).map(|_| observer.events.recv_timeout(Duration::from_secs(3)).unwrap()).collect::<Vec<_>>();
        assert_eq!(events, vec![
            NativeTuiEvent::Session {session_id:"first".into(), model:Some("gpt-5.6-luna".into()), effort:Some("low".into())},
            NativeTuiEvent::Session {session_id:"resumed".into(), model:Some("gpt-5.6-terra".into()), effort:Some("high".into())},
        ]);
        drop(client);
        drop(observer);
        fake.join().unwrap();
    }

    #[test]
    fn explicit_resume_is_separated_from_forwarded_tui_arguments() {
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
    fn a_resume_without_an_id_forwards_to_the_native_picker() {
        for args in [
            vec!["resume".to_string()],
            vec!["resume".to_string(), "--last".to_string()],
        ] {
            let (thread, forwarded) = explicit_resume(&args).expect("picker resume");
            assert_eq!(thread, None);
            assert_eq!(forwarded, args);
        }
    }

    #[test]
    fn a_fresh_launch_forwards_every_tui_argument() {
        let args = vec!["--no-alt-screen".to_string()];
        let (thread, forwarded) = explicit_resume(&args).expect("fresh launch");
        assert_eq!(thread, None);
        assert_eq!(forwarded, args);
    }

    #[test]
    fn native_launch_resumes_the_thread_that_was_already_created() {
        let cwd = PathBuf::from("/tmp/project");
        let explicit = native_tui_args(Some("thread-1"), "/tmp/codex.sock", &cwd, &[]);
        assert_eq!(
            explicit,
            [
                "resume",
                "thread-1",
                "--remote",
                "unix:///tmp/codex.sock",
                "--cd",
                "/tmp/project"
            ]
        );
        let fresh = native_tui_args(Some("thread-started"), "/tmp/codex.sock", &cwd, &[]);
        assert_eq!(
            fresh,
            [
                "resume",
                "thread-started",
                "--remote",
                "unix:///tmp/codex.sock",
                "--cd",
                "/tmp/project"
            ]
        );
    }

    /// RECEIPT. A session whose door is not an app-server reports Unreachable
    /// instead of shelling out.
    #[test]
    fn a_session_without_a_socket_is_unreachable() {
        let door = CodexDoor::machine();
        let mut session = crate::door::tests::probe(HarnessId::Codex);
        session.harness = HarnessId::Codex;
        assert!(matches!(
            door.deliver(&session, "ping").unwrap(),
            Delivered::Unreachable(_)
        ));
        assert!(door
            .notify_idle(&session, Duration::from_millis(1))
            .is_err());
    }
}
