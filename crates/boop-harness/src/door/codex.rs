//! The codex door: `~/.codex/state_5.sqlite` says which threads exist, and the
//! remote-control daemon's socket is where a message for one is queued.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::door::{Delivered, Door, IdleNotice};
use crate::harness::HarnessId;
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
            "SELECT id, cwd, COALESCE(updated_at_ms, updated_at * 1000) \
             FROM threads WHERE archived = 0 ORDER BY updated_at DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<i64>>(2)?.unwrap_or(0) as u64,
            ))
        })?;
        let floor = now_ms().saturating_sub(RECENT_MS);
        let mut live = Vec::new();
        for row in rows {
            let (id, cwd, updated_ms) = row?;
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
