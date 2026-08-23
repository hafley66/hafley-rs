//! The codex door: `~/.codex/state_5.sqlite` says which threads exist, and the
//! remote-control daemon's socket is where a message for one is queued.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::door::{Delivered, Door, IdleNotice};
use crate::harness::{HarnessId, NativeTuiPlan, NativeTuiSpec};
use crate::live::{now_ms, DoorAddress, LiveSession, LiveSessions, LiveStatus};

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
             COALESCE(created_at_ms, created_at * 1000) \
             FROM threads WHERE archived = 0 ORDER BY updated_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<i64>>(2)?.unwrap_or(0) as u64,
                row.get::<_, Option<i64>>(3)?.map(|ms| ms as u64),
            ))
        })?;
        let floor = now_ms().saturating_sub(RECENT_MS);
        let mut live = Vec::new();
        for row in rows {
            let (id, cwd, updated_ms, created_ms) = row?;
            if updated_ms < floor {
                continue;
            }
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
            });
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

    /// The app-server reports turn end over its notification stream; reading
    /// that stream is phase 2 and this refuses rather than polling a guess.
    fn notify_idle(&self, session: &LiveSession, _timeout: Duration) -> Result<IdleNotice> {
        anyhow::bail!(
            "codex thread `{}` reports idle over the app-server stream, which boop does not read yet",
            session.session_id
        )
    }

    /// The TUI attaches to the daemon socket this door queues through, with
    /// nothing between the two.
    fn tui_launch(&self, spec: &NativeTuiSpec) -> Result<NativeTuiPlan> {
        let socket = self.start_daemon(&spec.executable)?;
        let (requested_thread, forwarded) = explicit_resume(&spec.args)?;
        // The TUI opens its thread at its first prompt; `thread/start` on the
        // daemon makes a thread `codex resume` refuses (no rollout until a
        // turn), so the wrapper adopts the TUI's thread once it exists.
        Ok(NativeTuiPlan {
            program: spec.executable.clone(),
            args: native_tui_args(requested_thread.as_deref(), &socket, &spec.cwd, forwarded),
            mode: "native-remote".into(),
            session_id: requested_thread.clone(),
            source_path: Some(match &requested_thread {
                Some(thread) => format!("managed-app-server={socket};requested-resume={thread}"),
                None => format!("managed-app-server={socket}"),
            }),
            app_server_socket: Some(socket),
        })
    }
}


impl CodexDoor {
    /// `codex remote-control start` is idempotent: it reports the socket of
    /// the daemon already running, or starts one and reports that.
    fn start_daemon(&self, executable: &str) -> Result<String> {
        let output = Command::new(executable)
            .args(["remote-control", "start", "--json"])
            .output()
            .context("start managed Codex remote-control daemon")?;
        anyhow::ensure!(
            output.status.success(),
            "Codex remote-control start failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        daemon_socket_from_start(&String::from_utf8_lossy(&output.stdout))
            .context("Codex remote-control start did not report an app-server socket")
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
    let thread = tui_args
        .get(1)
        .filter(|value| !value.starts_with('-'))
        .context("`boop tui codex -- resume` requires an explicit thread id")?;
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
        assert_eq!(
            session.door,
            DoorAddress::AppServer {
                socket: fixture.dir.join("daemon.sock"),
                thread: "01a02a8b-live".into(),
            }
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
    fn remote_control_start_socket_is_read_without_assuming_its_json_key() {
        let output =
            r#"{"daemon":{"socketPath":"/tmp/codex.sock","otherSocket":"/tmp/wrong.sock"}}"#;
        assert_eq!(
            daemon_socket_from_start(output).as_deref(),
            Some("/tmp/codex.sock")
        );
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
