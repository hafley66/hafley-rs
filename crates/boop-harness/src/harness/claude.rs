//! The claude adapter: transcripts under `~/.claude/projects/<encoded-cwd>/`.
#![allow(dead_code)]

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use crate::harness::{
    jsonl_files, Capabilities, ControlCapabilities, Harness, HarnessId, KnownSessions, LanePolicy,
    MailPolicy, ReadChunk, SessionRef, SpawnSpec, VariantSupport,
};
use anyhow::Context;
use boop_store::event::{Access, AgentEvent, ToolPath};
use boop_store::tail;
use serde_json::Value;

/// The claude harness. Stateless; the trait methods read straight from disk.
pub struct Claude;

/// Claude workers are the coordinator's own Agent-tool subagents, and its
/// mail lands at a turn boundary rather than on the keyboard.
static CAPABILITIES: Capabilities = Capabilities {
    bans_plan_family_models: false,
    lanes: LanePolicy::CoordinatorSubagentsOnly,
    variant: VariantSupport::None,
    mail: MailPolicy::Door,
    native_tui_projector: true,
    wrapper_owns_alternate_screen: false,
};

/// The registry directory and messaging sockets of the claude on this
/// machine; both the live list and one delivery read it.
static DOOR: crate::door::claude::ClaudeDoor = crate::door::claude::ClaudeDoor::machine();

impl Harness for Claude {
    fn open_channel(
        &self,
        spec: &boop_acp::channel::ChannelSpec,
    ) -> anyhow::Result<Box<dyn boop_acp::channel::LaneChannel>> {
        // CLAUDE_ADAPTER is an npx row, so its program is npx and there is no
        // slot in it for an alternate claude binary. A lane that names one
        // takes the direct stream-json channel, which spawns that binary.
        if spec.executable.is_some() {
            return Ok(Box::new(boop_acp::channel::claude::ClaudeChannel::open(
                spec,
            )?));
        }
        Ok(Box::new(boop_acp::channel::acp::AcpChannel::open_adapter(
            spec,
            boop_acp::channel::acp::CLAUDE_ADAPTER,
        )?))
    }

    fn id(&self) -> HarnessId {
        HarnessId::Claude
    }

    fn capabilities(&self) -> &'static Capabilities {
        &CAPABILITIES
    }

    fn tui_composer(&self) -> crate::harness::TuiComposer {
        crate::harness::TuiComposer::Claude
    }

    fn live(&self) -> &dyn crate::live::LiveSessions {
        &DOOR
    }

    fn door(&self) -> &dyn crate::door::Door {
        &DOOR
    }

    fn sessions(&self) -> anyhow::Result<Vec<SessionRef>> {
        let base = claude_projects_dir()?;
        sessions_in(&base)
    }

    fn session_roots(&self) -> anyhow::Result<Vec<PathBuf>> {
        Ok(vec![claude_projects_dir()?])
    }

    fn sync_candidates(&self, known: &KnownSessions) -> anyhow::Result<Vec<SessionRef>> {
        let base = claude_projects_dir()?;
        sessions_in_with_known(&base, known)
    }

    fn read_from(&self, session: &SessionRef, offset: u64) -> anyhow::Result<ReadChunk> {
        let mut file = File::open(&session.path)
            .with_context(|| format!("open transcript {}", session.path.display()))?;
        let result = tail::read_complete_lines(&mut file, offset)?;

        let mut events = Vec::new();
        let mut skipped = 0usize;
        for line in &result.lines {
            match parse_line(session, line) {
                Some(event) => events.push(event),
                None => skipped += 1,
            }
        }

        Ok(ReadChunk {
            events,
            next_offset: result.next_offset,
            reset: result.reset,
            skipped,
        })
    }

    /// `send_midflight` is false since the lane channel became ACP:
    /// `session/prompt` is one request per turn and a second one before the
    /// first resolves is out of protocol.
    fn control_capabilities(&self) -> ControlCapabilities {
        ControlCapabilities {
            send_midflight: false,
            resume: true,
            spawn: true,
            subagent_visible: true,
        }
    }

    fn preview_command(&self, spec: &SpawnSpec) -> Option<String> {
        Some(crate::harness::supervisor_command(spec))
    }

    fn spawn(&self, spec: &SpawnSpec) -> anyhow::Result<SessionRef> {
        let session_id = format!("agent-{}", random_hex());
        let tmux_name = spec
            .tmux
            .clone()
            .unwrap_or_else(|| format!("boop-{session_id}"));
        let cwd = crate::worktree::prepare_spawn_dir(spec)?;
        let command = crate::harness::supervisor_command(spec);
        boop_store::tmux::mux().new_detached_session(
            spec.socket.as_deref(),
            &tmux_name,
            &cwd.display().to_string(),
            &command,
        )?;
        Ok(SessionRef {
            harness: HarnessId::Claude,
            session_id: session_id.clone(),
            nickname: session_id.clone(),
            path: cwd.join(session_id).with_extension("jsonl"),
            cwd: Some(cwd.display().to_string()),
            git_branch: Some(spec.branch.clone()),
            modified_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            size: 0,
            tmux: Some(tmux_name),
            tmux_socket: spec.socket.clone(),
            parent: None,
        })
    }

    fn stop(&self, session: &SessionRef) -> anyhow::Result<()> {
        if let Some(tmux) = &session.tmux {
            if boop_store::tmux::mux().has_session(session.tmux_socket.as_deref(), tmux)? {
                boop_store::tmux::mux().kill_session(session.tmux_socket.as_deref(), tmux)?;
            }
        }
        Ok(())
    }
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LivePeer {
    pid: u32,
    session_id: String,
    proc_start: String,
    messaging_socket_path: PathBuf,
    #[serde(default)]
    updated_at: u64,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PeerKey {
    peer_token: String,
    proc_start: String,
}

/// The claude command line a spawn runs. Resuming an existing session wins
/// over a fresh prompt.
fn launch_command(spec: &SpawnSpec) -> String {
    let mut command = match &spec.resume_session {
        Some(id) => format!("claude --resume {id}"),
        None => format!("claude {}", shell_quote(&spec.prompt)),
    };
    if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
        command.push_str(&format!(" --model {}", shell_quote(model)));
    }
    spec.with_on_exit(match &spec.env_stamp {
        Some(stamp) => format!("{stamp} {command}"),
        None => command,
    })
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

// The old per-byte time sample repeated one byte 8 times (measured live:
// "62626262626a6a6a"), so two close spawns could collide.
fn random_hex() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mixed = (nanos as u64) ^ ((std::process::id() as u64) << 48) ^ (nanos >> 64) as u64;
    format!("{mixed:016x}")
}

fn claude_projects_dir() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().context("resolve home directory")?;
    Ok(home.join(".claude").join("projects"))
}

/// Discover sessions under `base` (typically `~/.claude/projects`). A
/// transcript under a `subagents/` directory inherits its parent session id
/// from the containing folder's name, which is how claude writes the spawn
/// edge.
fn sessions_in(base: &std::path::Path) -> anyhow::Result<Vec<SessionRef>> {
    sessions_in_with_known(base, &KnownSessions::new())
}

fn sessions_in_with_known(
    base: &std::path::Path,
    known: &KnownSessions,
) -> anyhow::Result<Vec<SessionRef>> {
    let mut sessions = Vec::new();
    for file in jsonl_files(base)? {
        let path = &file.path;
        let nickname = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_string();
        let parent = parent_for(path);
        let session_id = match &parent {
            Some(parent) => format!("{parent}/{nickname}"),
            None => nickname.clone(),
        };
        let modified_ms = file.modified_ms;
        let size = file.size;

        if let Some(known) = known.get(path) {
            sessions.push(SessionRef {
                harness: HarnessId::Claude,
                session_id: known.session_id.clone(),
                nickname: known.nickname.clone(),
                path: path.to_path_buf(),
                cwd: known.cwd.clone(),
                git_branch: known.git_branch.clone(),
                modified_ms,
                size,
                tmux: None,
                tmux_socket: None,
                parent: known.parent.clone(),
            });
            continue;
        }

        let (cwd, git_branch) = first_record_context(path);
        sessions.push(SessionRef {
            harness: HarnessId::Claude,
            session_id,
            nickname,
            path: path.to_path_buf(),
            cwd,
            git_branch,
            modified_ms,
            size,
            tmux: None,
            tmux_socket: None,
            parent,
        });
    }
    sessions.sort_by_key(|session| session.modified_ms);
    Ok(sessions)
}

/// The session id that spawned the transcript at `path`. A file whose parent
/// directory is `subagents/` is a sidechain; its parent session is the folder
/// that contains `subagents/`.
fn parent_for(path: &std::path::Path) -> Option<String> {
    let parent_dir = path.parent()?.file_name()?.to_str()?;
    if parent_dir != "subagents" {
        return None;
    }
    path.parent()?
        .parent()?
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
}

/// Read the head of the transcript for the session cwd and git branch. The
/// first record is a `queue-operation`/`mode`/`ai-title` metadata line that
/// carries neither, so the scan runs until a record has them. A line that
/// fails to parse is a partial write and is skipped, never trusted.
const CONTEXT_SCAN_LINES: usize = 16;

fn first_record_context(path: &std::path::Path) -> (Option<String>, Option<String>) {
    let Ok(file) = File::open(path) else {
        return (None, None);
    };
    let mut reader = BufReader::new(file);
    for _ in 0..CONTEXT_SCAN_LINES {
        let Ok(Some(line)) = tail::read_first_complete_line(&mut reader) else {
            return (None, None);
        };
        let Ok(value) = serde_json::from_slice::<Value>(&line.bytes) else {
            continue;
        };
        let Some(cwd) = value.get("cwd").and_then(Value::as_str) else {
            continue;
        };
        return (
            Some(cwd.to_owned()),
            value
                .get("gitBranch")
                .and_then(Value::as_str)
                .map(str::to_owned),
        );
    }
    (None, None)
}

/// Decode one JSONL line into an `AgentEvent`. An unrecognized record shape is
/// still an event, never an error and never dropped. Returns `None` only for a
/// line that fails to parse as JSON.
fn parse_line(session: &SessionRef, line: &tail::CompleteLine) -> Option<AgentEvent> {
    let value: Value = serde_json::from_slice(&line.bytes).ok()?;

    let record_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let ts_ms = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_iso_ms)
        .unwrap_or(0);
    let uuid = value.get("uuid").and_then(Value::as_str).map(str::to_owned);
    let parent_uuid = value
        .get("parentUuid")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let record_cwd = value.get("cwd").and_then(Value::as_str).map(str::to_owned);
    let record_branch = value
        .get("gitBranch")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let session_id = value
        .get("sessionId")
        .or_else(|| value.get("session_id"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| session.session_id.clone());

    let mut tool_name = None;
    let mut paths = Vec::new();
    let mut urls = Vec::new();
    collect_tool_use(&value, &mut tool_name, &mut paths, &mut urls);

    Some(AgentEvent {
        harness: session.harness.as_str(),
        session_id,
        ts_ms,
        uuid,
        parent_uuid,
        cwd: record_cwd.or_else(|| session.cwd.clone()),
        git_branch: record_branch.or_else(|| session.git_branch.clone()),
        record_type,
        tool_name,
        paths,
        urls,
        raw_line_offset: line.start,
    })
}

/// Walk `message.content` for tool_use blocks and surface tool name, file paths,
/// and urls from them.
fn collect_tool_use(
    value: &Value,
    tool_name: &mut Option<String>,
    paths: &mut Vec<ToolPath>,
    urls: &mut Vec<String>,
) {
    let Some(message) = value.get("message") else {
        return;
    };
    let Some(content) = message.get("content").and_then(Value::as_array) else {
        return;
    };
    for block in content {
        let Some(block) = block.as_object() else {
            continue;
        };
        if block.get("type").and_then(Value::as_str) != Some("tool_use") {
            continue;
        }
        let Some(name) = block.get("name").and_then(Value::as_str) else {
            continue;
        };
        *tool_name = Some(name.to_owned());
        let input = block.get("input").and_then(Value::as_object);
        if let Some(file_path) = input
            .and_then(|input| input.get("file_path"))
            .and_then(Value::as_str)
        {
            let access = match name {
                "Read" => Access::Read,
                _ => Access::Write,
            };
            paths.push(ToolPath {
                path: file_path.to_owned(),
                access,
            });
        }
        if let Some(url) = input
            .and_then(|input| input.get("url"))
            .and_then(Value::as_str)
        {
            urls.push(url.to_owned());
        }
    }
}

/// Parse an ISO-8601 UTC timestamp into ms since the epoch.
pub use boop_store::session::parse_iso_ms;

#[cfg(test)]
mod tests {
    use crate::harness::HarnessId;
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::path::PathBuf;

    use crate::harness::Harness;
    use crate::harness::SessionRef;
    use boop_store::testing::TempRepo;

    use super::{launch_command, parse_iso_ms, Claude};

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("boop_claude_{}_{}", std::process::id(), name))
    }

    fn write_lines(path: &PathBuf, lines: &[&str]) {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)
            .unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    fn session_for(path: &std::path::Path, size: u64) -> SessionRef {
        SessionRef {
            harness: HarnessId::Claude,
            session_id: "test-session".to_owned(),
            nickname: "test-session".to_owned(),
            path: path.to_path_buf(),
            cwd: None,
            git_branch: None,
            modified_ms: 0,
            size,
            tmux: None,
            tmux_socket: None,
            parent: None,
        }
    }

    /// FAIL-PRE-FIX. Current claude transcripts open with a metadata record
    /// (`queue-operation`, `mode`, `ai-title`), so reading only line 1 for the
    /// cwd returned `None` for every one of them, and a cwd lookup could
    /// answer nothing.
    #[test]
    fn a_transcript_whose_head_is_metadata_still_reports_its_cwd() {
        let base = temp_path("cwdscan");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("subagents")).unwrap();
        write_lines(
            &base.join("root-a.jsonl"),
            &[
                r#"{"type":"queue-operation"}"#,
                r#"{"type":"mode"}"#,
                r#"{"type":"user","sessionId":"root-a","cwd":"/repo","gitBranch":"main"}"#,
            ],
        );
        write_lines(
            &base.join("root-b.jsonl"),
            &[r#"{"type":"user","sessionId":"root-b","cwd":"/elsewhere"}"#],
        );
        write_lines(
            &base.join("subagents").join("agent-x.jsonl"),
            &[r#"{"type":"user","sessionId":"agent-x","cwd":"/repo"}"#],
        );

        let roots: Vec<SessionRef> = super::sessions_in(&base)
            .unwrap()
            .into_iter()
            .filter(|session| session.cwd.as_deref() == Some("/repo") && session.parent.is_none())
            .collect();
        let names: Vec<&str> = roots
            .iter()
            .map(|session| session.session_id.as_str())
            .collect();
        assert_eq!(names, vec!["root-a"], "roots: {names:?}");
        assert_eq!(roots[0].git_branch.as_deref(), Some("main"));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn parses_the_corpus_timestamp_shape() {
        assert_eq!(
            parse_iso_ms("2026-08-06T16:56:57.904Z"),
            Some(1_786_035_417_904)
        );
        assert_eq!(
            parse_iso_ms("2026-01-01T00:00:00.000Z"),
            Some(1_767_225_600_000)
        );
    }

    #[test]
    fn skips_an_invalid_json_line_but_keeps_the_rest() {
        let path = temp_path("jn1");
        write_lines(
            &path,
            &[
                r#"{"type":"user","sessionId":"s1","uuid":"u1"}"#,
                r#"this is not json at all"#,
                r#"{"type":"assistant","sessionId":"s1","uuid":"u2"}"#,
            ],
        );
        let metadata = path.metadata().unwrap();
        let claude = Claude;
        let chunk = claude
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 2);
        assert_eq!(chunk.skipped, 1);
        assert_eq!(chunk.next_offset, metadata.len());
        assert_eq!(chunk.events[0].record_type, "user");
        assert_eq!(chunk.events[1].record_type, "assistant");
    }

    #[test]
    fn extracts_file_paths_and_urls_from_tool_use() {
        let path = temp_path("jn2");
        let record = r#"{"type":"assistant","sessionId":"s1","uuid":"u1","message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/tmp/x.rs"}},{"type":"tool_use","name":"WebFetch","input":{"url":"https://example.com"}},{"type":"tool_use","name":"Read","input":{"file_path":"/tmp/y.rs"}}]}}"#;
        write_lines(&path, &[record]);
        let metadata = path.metadata().unwrap();
        let claude = Claude;
        let chunk = claude
            .read_from(&session_for(&path, metadata.len()), 0)
            .unwrap();
        assert_eq!(chunk.events.len(), 1);
        let event = &chunk.events[0];
        assert_eq!(event.tool_name.as_deref(), Some("Read"));
        assert_eq!(event.paths.len(), 2);
        assert_eq!(event.paths[0].path, "/tmp/x.rs");
        assert_eq!(event.paths[1].path, "/tmp/y.rs");
        assert_eq!(event.urls, vec!["https://example.com"]);
    }

    // ---- facet 3 ----

    /// A throwaway tmux server on its own socket; drop kills the whole server.
    struct TmuxGuard {
        socket: String,
    }

    static NEXT_SOCKET: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    impl TmuxGuard {
        /// One socket per guard: tests run in parallel and a shared name makes
        /// each new guard kill its neighbour's server.
        fn new() -> TmuxGuard {
            let socket = format!(
                "boop-test-{}-ctl{}",
                std::process::id(),
                NEXT_SOCKET.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            boop_store::tmux::kill_test_server(&socket);
            TmuxGuard { socket }
        }
    }

    impl Drop for TmuxGuard {
        fn drop(&mut self) {
            boop_store::tmux::kill_test_server(&self.socket);
        }
    }

    fn spec(guard: &TmuxGuard) -> crate::harness::SpawnSpec {
        crate::harness::SpawnSpec {
            effort: None,
            harness: HarnessId::Claude,
            branch: "lane-test".to_owned(),
            base_sha: "0000000000000000000000000000000000000000".to_owned(),
            main_tree: true,
            setup: Vec::new(),
            prompt: "do the lane".to_owned(),
            resume_session: None,
            socket: Some(guard.socket.clone()),
            worktree_dir: None,
            repo: std::env::temp_dir(),
            env_stamp: None,
            model: None,
            variant: None,
            bin: None,
            on_exit: None,
            tmux: None,
            lane: "lane-test".to_owned(),
            mail_dir: std::env::temp_dir(),
            warm_start: false,
        }
    }

    #[test]
    fn claude_capabilities_are_measured() {
        let caps = Claude.control_capabilities();
        assert!(!caps.send_midflight, "acp takes one prompt per turn");
        assert!(caps.resume);
        assert!(caps.spawn);
        assert!(caps.subagent_visible);
    }

    #[test]
    fn claude_spawn_returns_handle_and_stop_tears_down() {
        let guard = TmuxGuard::new();
        let repo = TempRepo::new();
        let worktree = repo.worktree.clone();
        let mut req = spec(&guard);
        req.main_tree = false;
        req.base_sha = repo.sha.clone();
        req.repo = repo.dir.clone();
        req.worktree_dir = Some(worktree.clone());
        let claude = Claude;
        let session = claude.spawn(&req).unwrap();
        assert_eq!(
            session
                .tmux
                .as_deref()
                .map(|t| t.starts_with("boop-agent-")),
            Some(true)
        );
        assert_eq!(session.tmux_socket.as_deref(), Some(guard.socket.as_str()));
        assert!(
            worktree.join("seed.txt").exists(),
            "worktree must be created by spawn"
        );
        claude.stop(&session).unwrap();
        // The spawned session (a shell claude runs under) must be gone after
        // stop; the guard also kills the whole server regardless.
        assert!(!has_session_on(&guard, session.tmux.as_deref().unwrap()));
    }

    #[test]
    fn claude_launch_resumes_with_session_id() {
        let mut req = spec(&TmuxGuard::new());
        req.resume_session = Some("abc123".to_owned());
        assert!(launch_command(&req).contains("--resume abc123"));
    }

    #[test]
    fn claude_reads_subagent_edge_from_real_fixture() {
        use super::sessions_in;
        let base = std::path::PathBuf::from("tests/fixtures/claude");
        let sessions = sessions_in(&base).unwrap();
        let sub = sessions
            .iter()
            .find(|s| s.nickname == "agent-a6cee372fea5c1c2f")
            .expect("subagent transcript present in fixture");
        assert_eq!(
            sub.parent.as_deref(),
            Some("2579238b-e154-40c0-b018-f6aa80f87a90")
        );
        assert_eq!(
            sub.session_id,
            "2579238b-e154-40c0-b018-f6aa80f87a90/agent-a6cee372fea5c1c2f"
        );
    }

    #[test]
    fn claude_fixture_projects_through_the_graph_query() {
        use super::sessions_in;
        let sessions = sessions_in(&std::path::PathBuf::from("tests/fixtures/claude")).unwrap();
        crate::harness::assert_fixture_sessions_project(&super::Claude, &sessions, 0);
    }

    /// FAIL-FIRST (D4). 52 of 1318 live transcript stems name two different
    /// subagent transcripts under two different parents.
    #[test]
    fn two_transcripts_sharing_a_stem_get_distinct_session_ids() {
        use super::sessions_in;
        let base = std::path::PathBuf::from("tests/fixtures/claude");
        let sessions = sessions_in(&base).unwrap();
        let sharing: Vec<&SessionRef> = sessions
            .iter()
            .filter(|session| {
                session
                    .path
                    .to_string_lossy()
                    .contains("agent-dupstem00000000")
            })
            .collect();
        assert_eq!(sharing.len(), 2, "the fixture pair must both be discovered");
        assert_ne!(
            sharing[0].session_id, sharing[1].session_id,
            "one id for two agents merges their turns and their spawn edges"
        );
        for session in &sharing {
            assert!(
                session
                    .session_id
                    .contains(session.parent.as_deref().unwrap()),
                "a subagent id carries its parent: {}",
                session.session_id
            );
        }
    }

    fn has_session_on(guard: &TmuxGuard, name: &str) -> bool {
        boop_store::tmux::mux()
            .has_session(Some(&guard.socket), name)
            .unwrap_or(false)
    }
}
