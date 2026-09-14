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

/// A socket write that has not drained by now has no reader behind it.
const DOOR_DEADLINE: Duration = Duration::from_secs(5);

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
        let home = crate::harness::reader_home()?;
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

    /// Registry files whose process is still alive. A file outlives its process.
    fn live_files(&self) -> Result<Vec<RegistryFile>> {
        Ok(self
            .files()?
            .into_iter()
            .filter(|file| pid_alive(file.pid))
            .collect())
    }

    /// The session one host record presents. An interactive host parked on a
    /// background job presents that job's conversation, so the job row wins
    /// when its `jobId` equals the host's `parkedJobId` explicitly and exactly
    /// one live `bg` record carries it. A missing, dead, or duplicate job
    /// leaves the host row in place: the native record is kept rather than
    /// guessed at.
    fn presented_from_host(&self, host: &RegistryFile, live: &[RegistryFile]) -> LiveSession {
        let host_session = || host.clone().into_live(self.token_for(host.pid));
        let Some(parked) = host.parked_job_id.as_deref().filter(|id| !id.is_empty()) else {
            return host_session();
        };
        // Only an interactive host presents a parked job; a `bg` record that
        // happens to carry a parked id is not evidence it shows another
        // conversation.
        if host.kind.as_deref() != Some("interactive") {
            return host_session();
        }
        let mut jobs = live.iter().filter(|file| {
            file.kind.as_deref() == Some("bg") && file.job_id.as_deref() == Some(parked)
        });
        match (jobs.next(), jobs.next()) {
            (Some(job), None) => job.clone().into_live(self.token_for(job.pid)),
            _ => host_session(),
        }
    }

    /// The conversation a tmux pane presents. The pane-matching host is
    /// selected as before, then redirected to the background job it parks.
    fn presented_session_in_pane(&self, pane: &str) -> Result<Option<LiveSession>> {
        let wanted = pane.trim().trim_start_matches('%');
        if wanted.is_empty() {
            return Ok(None);
        }
        let live = self.live_files()?;
        let Some(host) = live.iter().find(|file| {
            file.tmux
                .as_deref()
                .and_then(pane_of_target)
                .is_some_and(|held| held.trim_start_matches('%') == wanted)
        }) else {
            return Ok(None);
        };
        Ok(Some(self.presented_from_host(host, &live)))
    }

    /// The conversation an explicitly bound session id presents. A live host
    /// row parked on a job redirects the same way a pane does.
    fn presented_session(&self, session_id: &str) -> Result<Option<LiveSession>> {
        let live = self.live_files()?;
        let Some(host) = live.iter().find(|file| file.session_id == session_id) else {
            return Ok(None);
        };
        Ok(Some(self.presented_from_host(host, &live)))
    }
}

/// One `~/.claude/sessions/<pid>.json`.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegistryFile {
    pid: u32,
    session_id: String,
    #[serde(default)]
    cwd: Option<String>,
    /// `interactive` for a TUI host, `bg` for a backgrounded job record.
    #[serde(default)]
    kind: Option<String>,
    /// The background job a `bg` record runs, named by its owning host.
    #[serde(default)]
    job_id: Option<String>,
    /// On an `interactive` host, the background job currently parked in it.
    #[serde(default)]
    parked_job_id: Option<String>,
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
            scope: crate::live::LiveSessionScope::Unknown,
            parent_session: None,
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

    /// A pane shows the conversation hosted there, which is the parked job's
    /// when the interactive host has parked one.
    fn live_session_in_pane(&self, pane: &str) -> Result<Option<LiveSession>> {
        self.presented_session_in_pane(pane)
    }

    /// A route bound to a host session follows the conversation the host is
    /// presenting, so a route left naming the parked root resolves to the job.
    /// A route naming no session falls back to its pane.
    fn live_session_for_route(
        &self,
        route: &boop_store::bus::Route,
    ) -> Result<Option<LiveSession>> {
        if let Some(id) = route.session_id.as_deref() {
            return self.presented_session(id);
        }
        match route.tmux.as_deref() {
            Some(target) => {
                let pane = pane_of_target(target).unwrap_or_else(|| target.to_owned());
                self.presented_session_in_pane(&pane)
            }
            None => Ok(None),
        }
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
    fn inbox_hook_installed(&self, cwd: &Path, route: &str) -> bool {
        super::claude_hooks::installed_for(cwd, route)
    }

    /// Claude's TUI takes the user's arguments as written; the only thing the
    /// wrapper adds is reading the resumed session id out of them.
    fn tui_launch(&self, spec: &NativeTuiSpec) -> Result<NativeTuiPlan> {
        let session_id = explicit_resume(&spec.args);
        let mut plan = NativeTuiPlan::direct(spec);
        plan.source_path = Some(match &session_id {
            Some(session) => format!(
                "native-executable={};requested-resume={session}",
                spec.executable
            ),
            None => format!("native-executable={}", spec.executable),
        });
        plan.session_id = session_id;
        Ok(plan)
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
    stream.set_write_timeout(Some(DOOR_DEADLINE))?;
    stream.set_read_timeout(Some(DOOR_DEADLINE))?;
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

        fn record(&self, pid: u32, value: serde_json::Value) {
            std::fs::write(
                self.dir.join(format!("{pid}.json")),
                serde_json::to_vec(&value).unwrap(),
            )
            .unwrap();
        }

        /// An interactive host record. `parked` names the background job the
        /// host has parked in its pane, when it has one.
        fn write_host(
            &self,
            pid: u32,
            session: &str,
            tmux: &str,
            status: &str,
            parked: Option<&str>,
        ) {
            let mut file = serde_json::json!({
                "pid": pid,
                "sessionId": session,
                "cwd": "/Users/someone/projects",
                "kind": "interactive",
                "tmux": tmux,
                "messagingSocketPath": format!("/tmp/cc-socks/{session}.sock"),
                "status": status,
                "updatedAt": 1787434679415u64,
            });
            if let Some(parked) = parked {
                file["parkedJobId"] = serde_json::json!(parked);
            }
            self.record(pid, file);
        }

        /// A background job record. It carries no pane of its own; its owning
        /// host names the pane it is presented in.
        fn write_job(&self, pid: u32, session: &str, job_id: &str, socket: &str) {
            self.record(
                pid,
                serde_json::json!({
                    "pid": pid,
                    "sessionId": session,
                    "cwd": "/Users/someone/projects",
                    "kind": "bg",
                    "jobId": job_id,
                    "messagingSocketPath": socket,
                    "status": "busy",
                    "updatedAt": 1787434679415u64,
                }),
            );
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

    /// A real child process, alive while held, killed when dropped. Its pid
    /// stands in for a live background job the way a running Claude job would.
    struct Sleeper(std::process::Child);

    impl Sleeper {
        fn new() -> Sleeper {
            Sleeper(
                std::process::Command::new("sleep")
                    .arg("60")
                    .spawn()
                    .expect("sleep is a real process"),
            )
        }

        fn pid(&self) -> u32 {
            self.0.id()
        }
    }

    impl Drop for Sleeper {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
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

    /// The pane a fixture door resolves, through the public pane lookup.
    fn pane_id(fixture: &Fixture) -> String {
        fixture
            .door()
            .live_session_in_pane("%9")
            .unwrap()
            .unwrap()
            .session_id
    }

    /// RECEIPT. An interactive host parked on a background job presents the
    /// job's conversation in its pane, and the native host row still lists.
    #[test]
    fn a_parked_host_presents_its_live_job() {
        let fixture = Fixture::new("parked");
        let job = Sleeper::new();
        fixture.write_host(
            std::process::id(),
            "old-root",
            "compiler:@9.%9",
            "idle",
            Some("job-a"),
        );
        fixture.write_job(job.pid(), "current-root", "job-a", "/tmp/cc-socks/job.sock");

        let found = fixture.door().live_session_in_pane("%9").unwrap().unwrap();
        assert_eq!(found.session_id, "current-root");
        assert_eq!(found.pid, Some(job.pid()));
        assert_eq!(
            found.door,
            DoorAddress::UnixSocket {
                path: PathBuf::from("/tmp/cc-socks/job.sock"),
                token: None,
            }
        );
        // The host row survives: `live_sessions` reports native records.
        assert_eq!(fixture.door().live_sessions().unwrap().len(), 2);
    }

    /// RECEIPT. A host that parked nothing presents its own conversation.
    #[test]
    fn an_unparked_host_presents_itself() {
        let fixture = Fixture::new("unparked");
        fixture.write_host(std::process::id(), "host-root", "a:@1.%9", "idle", None);
        assert_eq!(pane_id(&fixture), "host-root");
    }

    /// RECEIPT. A missing job file and a job whose process is dead both leave
    /// the native host row in place rather than guessing.
    #[test]
    fn a_missing_or_dead_job_leaves_the_host_row() {
        let missing = Fixture::new("missing-job");
        missing.write_host(
            std::process::id(),
            "old-root",
            "a:@1.%9",
            "idle",
            Some("job-a"),
        );
        assert_eq!(pane_id(&missing), "old-root");

        let dead = Fixture::new("dead-job");
        dead.write_host(
            std::process::id(),
            "old-root",
            "a:@1.%9",
            "idle",
            Some("job-a"),
        );
        dead.write_job(
            4_000_000,
            "current-root",
            "job-a",
            "/tmp/cc-socks/dead.sock",
        );
        assert_eq!(pane_id(&dead), "old-root");
    }

    /// RECEIPT. Two live jobs carrying the same id make the match ambiguous,
    /// which leaves the host row rather than picking one.
    #[test]
    fn duplicate_live_jobs_leave_the_host_row() {
        let fixture = Fixture::new("dup-job");
        let one = Sleeper::new();
        let two = Sleeper::new();
        fixture.write_host(
            std::process::id(),
            "old-root",
            "a:@1.%9",
            "idle",
            Some("job-a"),
        );
        fixture.write_job(one.pid(), "first", "job-a", "/tmp/one.sock");
        fixture.write_job(two.pid(), "second", "job-a", "/tmp/two.sock");
        assert_eq!(pane_id(&fixture), "old-root");
    }

    /// RECEIPT. The parked id matches whole, so a longer id that merely extends
    /// it is not the job.
    #[test]
    fn only_the_exact_job_id_matches() {
        let fixture = Fixture::new("exact-job");
        let job = Sleeper::new();
        fixture.write_host(
            std::process::id(),
            "old-root",
            "a:@1.%9",
            "idle",
            Some("job-a"),
        );
        fixture.write_job(job.pid(), "almost", "job-ab", "/tmp/almost.sock");
        assert_eq!(pane_id(&fixture), "old-root");
    }

    /// RECEIPT. Only an `interactive` host redirects to a parked job; a record
    /// of another kind keeps its own conversation.
    #[test]
    fn only_an_interactive_host_redirects_to_a_parked_job() {
        let fixture = Fixture::new("kind-guard");
        let job = Sleeper::new();
        fixture.write_job(job.pid(), "current-root", "job-a", "/tmp/cc-socks/job.sock");
        fixture.record(
            std::process::id(),
            serde_json::json!({
                "pid": std::process::id(),
                "sessionId": "odd-root",
                "kind": "bg",
                "tmux": "a:@1.%9",
                "parkedJobId": "job-a",
                "status": "idle",
            }),
        );
        assert_eq!(pane_id(&fixture), "odd-root");
    }

    /// RECEIPT. When the host parks a different job, the pane presents the new
    /// job's conversation.
    #[test]
    fn a_changed_parked_job_presents_the_new_job() {
        let fixture = Fixture::new("changed-job");
        let a = Sleeper::new();
        let b = Sleeper::new();
        fixture.write_job(a.pid(), "session-a", "job-a", "/tmp/a.sock");
        fixture.write_job(b.pid(), "session-b", "job-b", "/tmp/b.sock");
        fixture.write_host(
            std::process::id(),
            "old-root",
            "a:@1.%9",
            "idle",
            Some("job-a"),
        );
        assert_eq!(pane_id(&fixture), "session-a");
        fixture.write_host(
            std::process::id(),
            "old-root",
            "a:@1.%9",
            "idle",
            Some("job-b"),
        );
        assert_eq!(pane_id(&fixture), "session-b");
    }

    /// RECEIPT. A route left naming the parked host resolves to the presented
    /// job, by session id or by pane.
    #[test]
    fn a_route_naming_a_parked_host_resolves_to_the_job() {
        let fixture = Fixture::new("parked-route");
        let job = Sleeper::new();
        fixture.write_host(
            std::process::id(),
            "old-root",
            "compiler:@9.%9",
            "idle",
            Some("job-a"),
        );
        fixture.write_job(job.pid(), "current-root", "job-a", "/tmp/cc-socks/job.sock");
        let door = fixture.door();
        let bound = boop_store::bus::route_from_value(&serde_json::json!({
            "kind": "coordinator", "harness": "claude",
            "tmux": "compiler:@9.%9", "session_id": "old-root"
        }));
        assert_eq!(
            door.live_session_for_route(&bound)
                .unwrap()
                .unwrap()
                .session_id,
            "current-root"
        );
        let by_pane = boop_store::bus::route_from_value(&serde_json::json!({
            "kind": "coordinator", "harness": "claude", "tmux": "compiler:@9.%9"
        }));
        assert_eq!(
            door.live_session_for_route(&by_pane)
                .unwrap()
                .unwrap()
                .session_id,
            "current-root"
        );
    }

    /// RECEIPT, public path. The shared pane resolver reads the real Claude
    /// registry through `Claude`'s static door and follows a parked host. The
    /// registry dir reaches the resolver through a process-global env var, so
    /// this case re-execs the test binary instead of mutating the environment
    /// behind parallel tests.
    #[test]
    fn the_public_pane_lookup_follows_a_parked_host() {
        const CHILD_ENV: &str = "BOOP_TEST_CLAUDE_PANE_CHILD";
        const JOB_PID_ENV: &str = "BOOP_TEST_CLAUDE_PANE_JOB_PID";
        const TEST_NAME: &str = "door::claude::tests::the_public_pane_lookup_follows_a_parked_host";

        if std::env::var_os(CHILD_ENV).is_none() {
            // Parent: a scratch registry dir and a real live job process, then
            // a child test binary that receives both and resolves the pane.
            let fixture = Fixture::new("public-pane");
            let job = Sleeper::new();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([TEST_NAME, "--exact", "--nocapture"])
                .env(CHILD_ENV, &fixture.dir)
                .env(JOB_PID_ENV, job.pid().to_string())
                .output()
                .unwrap();
            assert!(output.status.success(), "{output:#?}");
            return;
        }

        // Child: this process's own pid is the live host, the parent's sleeper
        // the live job. Write both records, then run the public resolver.
        let fixture = Fixture {
            dir: PathBuf::from(std::env::var_os(CHILD_ENV).unwrap()),
        };
        let job_pid: u32 = std::env::var(JOB_PID_ENV).unwrap().parse().unwrap();
        fixture.write_host(
            std::process::id(),
            "old-root",
            "compiler:@9.%9",
            "idle",
            Some("job-a"),
        );
        fixture.write_job(job_pid, "current-root", "job-a", "/tmp/cc-socks/job.sock");
        std::env::set_var(SESSIONS_DIR_ENV, &fixture.dir);
        let found =
            crate::live::session_in_pane(&crate::Registry::discover(), "%9", &fixture.dir).unwrap();
        assert_eq!(found.as_deref(), Some("current-root"));

        // Public baseline: the same pane with an unparked host resolves to the
        // host's own session.
        fixture.write_host(
            std::process::id(),
            "old-root",
            "compiler:@9.%9",
            "idle",
            None,
        );
        let plain =
            crate::live::session_in_pane(&crate::Registry::discover(), "%9", &fixture.dir).unwrap();
        assert_eq!(plain.as_deref(), Some("old-root"));
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
        assert_eq!(
            explicit_resume(&args(&["-r", "abc"])),
            Some("abc".to_string())
        );
        assert_eq!(
            explicit_resume(&args(&["--resume=abc"])),
            Some("abc".to_string())
        );
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
            env: Vec::new(),
        };
        let plan = ClaudeDoor::machine().tui_launch(&spec).unwrap();
        assert_eq!(
            plan.session_id.as_deref(),
            Some("f3deaaac-d198-47d5-975d-8e84a038046f")
        );
        assert_eq!(
            plan.args,
            spec.args
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_bare_launch_still_leaves_the_session_to_opened_session() {
        let spec = NativeTuiSpec {
            executable: "claude".into(),
            cwd: std::path::PathBuf::from("/tmp"),
            args: Vec::new(),
            env: Vec::new(),
        };
        assert_eq!(
            ClaudeDoor::machine().tui_launch(&spec).unwrap().session_id,
            None
        );
    }
}
