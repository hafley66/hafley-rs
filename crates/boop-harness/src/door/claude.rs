//! The claude door: the registry files under `~/.claude/sessions` say what is
//! running, and each one names the unix socket that session reads messages on.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::door::{Delivered, Door, IdleNotice};
use crate::harness::{HarnessId, NativeTuiPlan, NativeTuiSpec};
use crate::live::{
    now_ms, pane_of_target, pid_alive, DoorAddress, LiveSession, LiveSessions, LiveStatus,
};

/// Overrides the directory the registry files are read from.
pub const SESSIONS_DIR_ENV: &str = "BOOP_CLAUDE_SESSIONS_DIR";

/// How often the idle poll re-reads a registry file.
const POLL: Duration = Duration::from_millis(500);

/// Reads the registry directory and writes to the socket a file names.
pub struct ClaudeDoor {
    dir: Option<PathBuf>,
}

impl ClaudeDoor {
    /// The registry of the claude installed for this user.
    pub const fn machine() -> Self {
        ClaudeDoor { dir: None }
    }

    /// A registry directory named outright, which is what a test hands in.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        ClaudeDoor {
            dir: Some(dir.into()),
        }
    }

    fn sessions_dir(&self) -> Result<PathBuf> {
        if let Some(dir) = &self.dir {
            return Ok(dir.clone());
        }
        if let Some(dir) = std::env::var_os(SESSIONS_DIR_ENV).filter(|value| !value.is_empty()) {
            return Ok(PathBuf::from(dir));
        }
        let home = dirs::home_dir().context("resolve home directory")?;
        Ok(home.join(".claude").join("sessions"))
    }

    /// The registry file for a session id, or `None` once it is gone.
    fn file_for(&self, session_id: &str) -> Result<Option<RegistryFile>> {
        Ok(self
            .files()?
            .into_iter()
            .find(|file| file.session_id == session_id))
    }

    fn files(&self) -> Result<Vec<RegistryFile>> {
        let dir = self.sessions_dir()?;
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            // A machine with no claude registry has nothing running, which is
            // an empty list rather than a failure.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error).context(format!("read {}", dir.display())),
        };
        let mut files = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(file) = serde_json::from_str::<RegistryFile>(&text) else {
                continue;
            };
            files.push(file);
        }
        files.sort_by(|left, right| left.session_id.cmp(&right.session_id));
        Ok(files)
    }

    /// The peer token beside a registry file, `<pid>.<digest>.key`.
    fn token_for(&self, pid: u32) -> Option<String> {
        let dir = self.sessions_dir().ok()?;
        let prefix = format!("{pid}.");
        let entry = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .find(|path| {
                path.extension().is_some_and(|extension| extension == "key")
                    && path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with(&prefix))
            })?;
        let text = std::fs::read_to_string(entry).ok()?;
        serde_json::from_str::<KeyFile>(&text)
            .ok()
            .map(|key| key.peer_token)
    }
}

/// One `~/.claude/sessions/<pid>.json`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryFile {
    pid: u32,
    session_id: String,
    #[serde(default)]
    cwd: Option<String>,
    /// `projects-2:@3418.%3418`.
    #[serde(default)]
    tmux: Option<String>,
    /// `busy` or `idle`.
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    messaging_socket_path: Option<String>,
    #[serde(default)]
    peer_features: Vec<String>,
    #[serde(default)]
    updated_at: Option<u64>,
    #[serde(default)]
    started_at: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyFile {
    peer_token: String,
}

impl RegistryFile {
    fn status(&self) -> LiveStatus {
        match self.status.as_deref() {
            Some("busy") => LiveStatus::Busy,
            Some("idle") => LiveStatus::Idle,
            _ => LiveStatus::Unknown,
        }
    }

    fn into_live(self, token: Option<String>) -> LiveSession {
        let door = match self.messaging_socket_path.as_deref() {
            Some(path) if !path.is_empty() => DoorAddress::UnixSocket {
                path: PathBuf::from(path),
                token,
            },
            _ => DoorAddress::None,
        };
        LiveSession {
            harness: HarnessId::Claude,
            session_id: self.session_id.clone(),
            pid: Some(self.pid),
            cwd: self.cwd.as_deref().map(PathBuf::from),
            tmux_pane: self.tmux.as_deref().and_then(pane_of_target),
            status: self.status(),
            door,
            observed_ms: self.updated_at.unwrap_or_else(now_ms),
            started_ms: self.started_at,
        }
    }
}

impl LiveSessions for ClaudeDoor {
    fn live_sessions(&self) -> Result<Vec<LiveSession>> {
        let mut live = Vec::new();
        for file in self.files()? {
            // A registry file outlives the process that wrote it.
            if !pid_alive(file.pid) {
                continue;
            }
            let token = self.token_for(file.pid);
            live.push(file.into_live(token));
        }
        Ok(live)
    }
}

/// The session id a `claude` command line names outright.
///
/// The default `tui_launch` reports no session, so `boop tui claude --
/// --resume <id>` threw the id away and control.rs fell back to
/// `opened_session`, which only accepts a session that started AFTER the
/// wrapper did. A resumed session started hours earlier, so nothing bound the
/// pane to it and every route carried `session_id: null`.
///
/// `--resume`/`-r` with no id is claude's picker, and `--continue`/`-c` names
/// no id either. Both leave the answer to `opened_session`, unchanged.
pub(crate) fn explicit_resume(tui_args: &[String]) -> Option<String> {
    let mut args = tui_args.iter().peekable();
    while let Some(arg) = args.next() {
        if let Some(value) = arg.strip_prefix("--resume=") {
            if !value.is_empty() {
                return Some(value.to_string());
            }
            continue;
        }
        if arg == "--resume" || arg == "-r" {
            match args.peek() {
                Some(next) if !next.starts_with('-') => return Some((*next).clone()),
                _ => continue,
            }
        }
    }
    None
}

impl Door for ClaudeDoor {
    /// Claude's TUI takes the user's arguments as written; the only thing the
    /// wrapper adds is reading the resumed session id out of them.
    fn tui_launch(&self, spec: &NativeTuiSpec) -> Result<NativeTuiPlan> {
        let session_id = explicit_resume(&spec.args);
        Ok(NativeTuiPlan {
            source_path: Some(match &session_id {
                Some(session) => format!(
                    "native-executable={};requested-resume={session}",
                    spec.executable
                ),
                None => format!("native-executable={}", spec.executable),
            }),
            session_id,
            ..NativeTuiPlan::direct(spec)
        })
    }

    fn deliver(&self, session: &LiveSession, body: &str) -> Result<Delivered> {
        let DoorAddress::UnixSocket { path, token } = &session.door else {
            return Ok(Delivered::Unreachable(format!(
                "claude session `{}` names no messaging socket",
                session.session_id
            )));
        };
        match write_lines(path, token.as_deref(), body) {
            Ok(()) => Ok(Delivered::QueuedForTurnBoundary),
            Err(error) => Ok(Delivered::Unreachable(format!(
                "claude socket {}: {error}",
                path.display()
            ))),
        }
    }

    /// Phase 1 reads the `status` field the session keeps current. The
    /// `notify_idle` peer feature the file advertises is phase 2.
    fn notify_idle(&self, session: &LiveSession, timeout: Duration) -> Result<IdleNotice> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.file_for(&session.session_id)? {
                // The session is gone, which is as idle as it gets.
                None => return Ok(IdleNotice::now(Some("gone".into()))),
                Some(file) if file.status() == LiveStatus::Idle => {
                    return Ok(IdleNotice {
                        at_ms: file.updated_at.unwrap_or_else(now_ms),
                        status_line: file.status.clone(),
                    })
                }
                Some(_) => {}
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                anyhow::bail!(
                    "claude session `{}` stayed busy for {:?}",
                    session.session_id,
                    timeout
                );
            }
            std::thread::sleep(POLL.min(left));
        }
    }
}

/// The wire format the claude binary documents: an optional auth line, then
/// one user message, each its own JSON line.
fn write_lines(socket: &Path, token: Option<&str>, body: &str) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    if let Some(token) = token.filter(|value| !value.is_empty()) {
        let auth = serde_json::json!({ "type": "auth", "token": token });
        writeln!(stream, "{auth}")?;
    }
    let message = serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": body },
    });
    writeln!(stream, "{message}")?;
    stream.flush()
}

/// Whether a session advertises the idle-notification peer feature. The phase
/// 2 subscription reads this before opening the peer protocol.
pub fn advertises_idle_notice(file_text: &str) -> bool {
    serde_json::from_str::<RegistryFile>(file_text)
        .map(|file| file.peer_features.iter().any(|name| name == "notify_idle"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;

    struct Fixture {
        dir: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Fixture {
            let dir = std::env::temp_dir().join(format!(
                "boop-claude-door-{}-{}-{name}",
                std::process::id(),
                now_ms()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Fixture { dir }
        }

        fn write(&self, pid: u32, session: &str, socket: &str, status: &str, tmux: &str) {
            let file = serde_json::json!({
                "pid": pid,
                "sessionId": session,
                "cwd": "/Users/someone/projects",
                "startedAt": 1787425778695u64,
                "peerProtocol": 1,
                "peerFeatures": ["notify_idle"],
                "kind": "interactive",
                "tmux": tmux,
                "messagingSocketPath": socket,
                "name": "projects-e3",
                "status": status,
                "updatedAt": 1787434679415u64,
            });
            std::fs::write(
                self.dir.join(format!("{pid}.json")),
                serde_json::to_vec(&file).unwrap(),
            )
            .unwrap();
            std::fs::write(
                self.dir.join(format!("{pid}.abc123.key")),
                br#"{"peerToken":"f7849b"}"#,
            )
            .unwrap();
        }

        fn door(&self) -> ClaudeDoor {
            ClaudeDoor::at(&self.dir)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// A listener that answers one connection and hands back what it read.
    fn listener(path: &Path) -> mpsc::Receiver<Vec<String>> {
        let listener = UnixListener::bind(path).unwrap();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let Ok((stream, _)) = listener.accept() else {
                return;
            };
            let lines = BufReader::new(stream)
                .lines()
                .map_while(Result::ok)
                .collect::<Vec<_>>();
            let _ = sender.send(lines);
        });
        receiver
    }

    /// RECEIPT. A registry file is read into a `LiveSession`: the pane loses
    /// its window prefix, the socket becomes the door, the key file the token.
    #[test]
    fn a_registry_file_becomes_a_live_session() {
        let fixture = Fixture::new("lists");
        let pid = std::process::id();
        fixture.write(
            pid,
            "5c7c1a83-2d6f",
            "/tmp/cc-socks/x.sock",
            "busy",
            "projects-2:@3418.%3418",
        );
        // A dead pid's file is left behind by design; it must not be listed.
        fixture.write(
            4_000_000,
            "dead-one",
            "/tmp/cc-socks/dead.sock",
            "idle",
            "a:@1.%1",
        );

        let live = fixture.door().live_sessions().unwrap();
        assert_eq!(live.len(), 1);
        let session = &live[0];
        assert_eq!(session.harness, HarnessId::Claude);
        assert_eq!(session.session_id, "5c7c1a83-2d6f");
        assert_eq!(session.pid, Some(pid));
        assert_eq!(session.tmux_pane.as_deref(), Some("%3418"));
        assert_eq!(session.status, LiveStatus::Busy);
        assert_eq!(
            session.door,
            DoorAddress::UnixSocket {
                path: PathBuf::from("/tmp/cc-socks/x.sock"),
                token: Some("f7849b".into()),
            }
        );
        assert_eq!(
            fixture
                .door()
                .live_session_in_pane("%3418")
                .unwrap()
                .map(|found| found.session_id),
            Some("5c7c1a83-2d6f".to_string())
        );
    }

    /// RECEIPT. What lands on the socket is the two documented JSON lines,
    /// auth first, then one user message carrying the body verbatim.
    #[test]
    fn a_delivery_writes_the_documented_json_lines() {
        let fixture = Fixture::new("socket");
        // A unix socket path is capped at SUN_LEN, well under what the
        // per-test temp directory name costs, so it lives beside /tmp.
        let socket = PathBuf::from(format!("/tmp/boop-cd-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket);
        let received = listener(&socket);
        fixture.write(
            std::process::id(),
            "session-a",
            &socket.display().to_string(),
            "busy",
            "a:@1.%1",
        );

        let door = fixture.door();
        let session = door.live_sessions().unwrap().remove(0);
        assert_eq!(
            door.deliver(&session, "ping from boop").unwrap(),
            Delivered::QueuedForTurnBoundary
        );

        let lines = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(lines.len(), 2);
        let auth: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(auth["type"], "auth");
        assert_eq!(auth["token"], "f7849b");
        let message: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(message["type"], "user");
        assert_eq!(message["message"]["role"], "user");
        assert_eq!(message["message"]["content"], "ping from boop");
        let _ = std::fs::remove_file(&socket);
    }

    /// RECEIPT. A socket nothing listens on reports Unreachable rather than
    /// raising, so one delivery outcome covers every failure.
    #[test]
    fn a_dead_socket_is_unreachable_not_an_error() {
        let fixture = Fixture::new("dead-socket");
        fixture.write(
            std::process::id(),
            "session-b",
            &fixture.dir.join("absent.sock").display().to_string(),
            "busy",
            "a:@1.%1",
        );
        let door = fixture.door();
        let session = door.live_sessions().unwrap().remove(0);
        assert!(matches!(
            door.deliver(&session, "ping").unwrap(),
            Delivered::Unreachable(_)
        ));
    }

    /// RECEIPT. The idle poll answers off the registry status, and a session
    /// that stays busy fails on its own deadline instead of hanging.
    #[test]
    fn the_idle_poll_reads_the_registry_status() {
        let fixture = Fixture::new("idle");
        let pid = std::process::id();
        fixture.write(pid, "session-c", "/tmp/cc-socks/c.sock", "idle", "a:@1.%1");
        let door = fixture.door();
        let session = door.live_sessions().unwrap().remove(0);
        let notice = door
            .notify_idle(&session, Duration::from_millis(50))
            .unwrap();
        assert_eq!(notice.status_line.as_deref(), Some("idle"));
        assert_eq!(notice.at_ms, 1787434679415);

        fixture.write(pid, "session-c", "/tmp/cc-socks/c.sock", "busy", "a:@1.%1");
        assert!(door
            .notify_idle(&session, Duration::from_millis(50))
            .is_err());
    }

    /// RECEIPT. The peer feature list in a real registry file parses.
    #[test]
    fn the_peer_feature_list_is_read() {
        assert!(advertises_idle_notice(
            r#"{"pid":1,"sessionId":"a","peerFeatures":["notify_idle"]}"#
        ));
        assert!(!advertises_idle_notice(r#"{"pid":1,"sessionId":"a"}"#));
    }

    /// RECEIPT, live machine. Prints what this machine is running right now.
    /// Ignored by default: it reads the real registry directory.
    #[test]
    #[ignore]
    fn live_claude_sessions_lists_this_machine() {
        let door = ClaudeDoor::machine();
        let live = door.live_sessions().unwrap();
        for session in &live {
            println!(
                "{} pid={:?} pane={:?} status={:?} door={:?} cwd={:?}",
                session.session_id,
                session.pid,
                session.tmux_pane,
                session.status,
                match &session.door {
                    DoorAddress::UnixSocket { path, token } =>
                        format!("unix {} token={}", path.display(), token.is_some()),
                    other => format!("{other:?}"),
                },
                session.cwd,
            );
        }
        println!("{} live claude sessions", live.len());
        assert!(
            !live.is_empty(),
            "this test runs from a live claude session"
        );
    }
}

#[cfg(test)]
mod tui_launch_tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn reads_the_session_id_after_a_long_resume_flag() {
        assert_eq!(
            explicit_resume(&args(&["--resume", "f3deaaac-d198-47d5-975d-8e84a038046f"])),
            Some("f3deaaac-d198-47d5-975d-8e84a038046f".to_string())
        );
    }

    #[test]
    fn reads_it_after_the_short_flag_and_from_an_equals_form() {
        assert_eq!(explicit_resume(&args(&["-r", "abc"])), Some("abc".to_string()));
        assert_eq!(explicit_resume(&args(&["--resume=abc"])), Some("abc".to_string()));
    }

    #[test]
    fn reads_it_past_earlier_flags() {
        assert_eq!(
            explicit_resume(&args(&["--model", "opus", "--resume", "abc"])),
            Some("abc".to_string())
        );
    }

    // Claude's picker: `--resume` alone opens a chooser and names no session,
    // so the answer stays with opened_session rather than becoming a flag name.
    #[test]
    fn reports_nothing_when_resume_names_no_session() {
        assert_eq!(explicit_resume(&args(&["--resume"])), None);
        assert_eq!(explicit_resume(&args(&["--resume", "--verbose"])), None);
        assert_eq!(explicit_resume(&args(&["--resume="])), None);
    }

    #[test]
    fn reports_nothing_for_continue_or_a_bare_launch() {
        assert_eq!(explicit_resume(&args(&["--continue"])), None);
        assert_eq!(explicit_resume(&args(&["-c"])), None);
        assert_eq!(explicit_resume(&args(&[])), None);
    }

    // The defect this exists for: control.rs only reached opened_session, which
    // rejects a session that started before the wrapper did.
    #[test]
    fn tui_launch_carries_the_resumed_session_into_the_plan() {
        let spec = NativeTuiSpec {
            executable: "claude".into(),
            cwd: std::path::PathBuf::from("/tmp"),
            args: args(&["--resume", "f3deaaac-d198-47d5-975d-8e84a038046f"]),
        };
        let plan = ClaudeDoor::machine().tui_launch(&spec).unwrap();
        assert_eq!(
            plan.session_id.as_deref(),
            Some("f3deaaac-d198-47d5-975d-8e84a038046f")
        );
        assert_eq!(plan.args, spec.args.iter().map(std::ffi::OsString::from).collect::<Vec<_>>());
    }

    #[test]
    fn a_bare_launch_still_leaves_the_session_to_opened_session() {
        let spec = NativeTuiSpec {
            executable: "claude".into(),
            cwd: std::path::PathBuf::from("/tmp"),
            args: Vec::new(),
        };
        assert_eq!(ClaudeDoor::machine().tui_launch(&spec).unwrap().session_id, None);
    }
}
