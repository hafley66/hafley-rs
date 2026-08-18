//! The trait every harness adapter implements; the CLI never names a harness.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::event::AgentEvent;

pub mod claude;
pub mod codex;
pub mod kimi;
pub mod opencode;

/// One agent harness that writes transcripts to this machine. Harnesses are
/// shareable so a caller can bound a synchronous pass on its own thread.
pub trait Harness: Send + Sync {
    /// Stable short id used in CLI output and as the `--harness` filter value.
    fn id(&self) -> &'static str;

    /// Every session this harness has on disk, newest last. No cap.
    fn sessions(&self) -> anyhow::Result<Vec<SessionRef>>;

    /// Candidates for incremental sync. The store supplies metadata for paths
    /// it has already projected, so a file-backed harness can stat those
    /// paths without reopening and parsing their first record. New paths keep
    /// the harness's full discovery path.
    fn sync_candidates(&self, _known: &KnownSessions) -> anyhow::Result<Vec<SessionRef>> {
        self.sessions()
    }

    /// Read forward from `offset` bytes. Returns the events decoded and the
    /// new offset to resume from. A partial trailing line is NOT consumed and
    /// NOT counted in the returned offset.
    fn read_from(&self, session: &SessionRef, offset: u64) -> anyhow::Result<ReadChunk>;

    /// Project this session's new records into the store, resuming from
    /// `from`. The cursor is whatever the harness can resume on: a byte offset
    /// for a transcript, a rowid for a SQL store.
    fn ingest(
        &self,
        store: &crate::ident::Store,
        session: &SessionRef,
        from: u64,
    ) -> anyhow::Result<Ingested> {
        crate::ident::project_transcript(store, session, from)
    }

    // facet 3: control. Defaults are the honest all-false / Unsupported shape,
    // so any adapter without control support is safe and explicit.

    /// What this harness can control. `true` only where a test confirms it.
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    /// The literal command a spawn would run for `spec`, with nothing
    /// actually spawned. `None` means no accurate preview for this adapter.
    fn preview_command(&self, _spec: &SpawnSpec) -> Option<String> {
        None
    }

    /// Run one prompt to completion and return the reply text.
    fn one_shot(&self, _spec: &OneShotSpec) -> anyhow::Result<String> {
        anyhow::bail!("harness `{}` has no one-shot support", self.id())
    }

    /// Spawn a session per `spec`, returning a handle to it.
    fn spawn(&self, _spec: &SpawnSpec) -> anyhow::Result<SessionRef> {
        anyhow::bail!("harness `{}` has no spawn support", self.id())
    }

    /// Send `text` to a live session.
    fn send(&self, _session: &SessionRef, _text: &str) -> anyhow::Result<SendOutcome> {
        Ok(SendOutcome::Unsupported)
    }

    /// Stop a live session.
    fn stop(&self, _session: &SessionRef) -> anyhow::Result<()> {
        anyhow::bail!("harness `{}` has no stop support", self.id())
    }

    /// Open a lane conversation. The supervisor drives it; nothing else calls
    /// this, and no caller learns which harness answered.
    fn open_channel(
        &self,
        _spec: &crate::channel::ChannelSpec,
    ) -> anyhow::Result<Box<dyn crate::channel::LaneChannel>> {
        anyhow::bail!("harness `{}` has no lane channel", self.id())
    }
}

/// One prompt run to completion, reply text returned. The harness owns the
/// command spelling; no caller learns which binary ran.
pub struct OneShotSpec {
    /// The model in the harness's own flag spelling; `None` lets the harness
    /// default, and a harness with no default refuses.
    pub model: Option<String>,
    pub prompt: String,
}

/// The one command a lane pane runs, whatever the harness: the boop
/// supervisor, which owns the harness child and drains the lane inbox.
pub fn supervisor_command(spec: &SpawnSpec) -> String {
    let mut command = format!(
        "boop beep lane run --lane {} --harness {} --brief {} --mail-dir {}",
        quote(&spec.lane),
        quote(&spec.harness),
        quote(&spec.prompt),
        quote(&spec.mail_dir.display().to_string()),
    );
    if let Some(model) = spec.model.as_deref().filter(|value| !value.is_empty()) {
        command.push_str(&format!(" --model {}", quote(model)));
    }
    if let Some(variant) = spec.variant.as_deref().filter(|value| !value.is_empty()) {
        command.push_str(&format!(" --variant {}", quote(variant)));
    }
    if let Some(session) = &spec.resume_session {
        command.push_str(&format!(" --resume {}", quote(session)));
    }
    spec.with_on_exit(match &spec.env_stamp {
        Some(stamp) => format!("{stamp} {command}"),
        None => command,
    })
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// What one ingest pass wrote, plus where the next pass resumes.
pub struct Ingested {
    pub stat: crate::ident::SyncStat,
    pub next_cursor: u64,
}

/// One transcript on disk that belongs to a harness.
#[derive(Clone, Debug)]
pub struct SessionRef {
    pub harness: &'static str,
    /// Unique across every transcript the harness can see; a file stem is not
    /// (52 of 1318 claude stems name two different subagents).
    pub session_id: String,
    /// The short name a human types; unique only by luck.
    pub nickname: String,
    pub path: PathBuf,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    /// Last modified time in milliseconds since the epoch.
    pub modified_ms: u64,
    /// Size of the file in bytes.
    pub size: u64,
    /// The tmux session that runs this harness (a transport handle the
    /// control facet targets); `None` when there is no live pane.
    pub tmux: Option<String>,
    /// The tmux socket the session lives on (throwaway sockets in tests).
    pub tmux_socket: Option<String>,
    /// The session id that spawned this one, when the harness records it.
    pub parent: Option<String>,
}

/// Session metadata retained by the store for a transcript path it has
/// already projected. Adapters update file size and mtime from the filesystem.
#[derive(Clone, Debug)]
pub struct KnownSession {
    pub session_id: String,
    pub nickname: String,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub parent: Option<String>,
    pub cursor: u64,
}

/// Persisted transcript metadata grouped by source path. File-backed
/// harnesses normally have one session per path; database-backed harnesses can
/// retain many session cursors in one file without collapsing them.
#[derive(Default)]
pub struct KnownSessions(HashMap<PathBuf, Vec<KnownSession>>);

impl KnownSessions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: PathBuf, session: KnownSession) {
        self.0.entry(path).or_default().push(session);
    }

    pub fn get(&self, path: &Path) -> Option<&KnownSession> {
        let sessions = self.0.get(path)?;
        (sessions.len() == 1).then(|| &sessions[0])
    }

    pub fn get_session(&self, path: &Path, session_id: &str) -> Option<&KnownSession> {
        self.0
            .get(path)?
            .iter()
            .find(|session| session.session_id == session_id)
    }
}

pub(crate) struct TranscriptFile {
    pub path: PathBuf,
    pub modified_ms: u64,
    pub size: u64,
}

pub(crate) fn jsonl_files(base: &Path) -> anyhow::Result<Vec<TranscriptFile>> {
    let files = std::sync::Mutex::new(Vec::new());
    ignore::WalkBuilder::new(base)
        .hidden(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .build_parallel()
        .run(|| {
            let files = &files;
            Box::new(move |entry| {
                let Ok(entry) = entry else {
                    return ignore::WalkState::Continue;
                };
                let path = entry.path();
                if !entry.file_type().is_some_and(|kind| kind.is_file())
                    || path
                        .extension()
                        .is_none_or(|extension| extension != "jsonl")
                {
                    return ignore::WalkState::Continue;
                }
                let Ok(metadata) = entry.metadata() else {
                    return ignore::WalkState::Continue;
                };
                let modified_ms = metadata
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_millis() as u64)
                    .unwrap_or(0);
                files
                    .lock()
                    .expect("transcript file collector")
                    .push(TranscriptFile {
                        path: path.to_path_buf(),
                        modified_ms,
                        size: metadata.len(),
                    });
                ignore::WalkState::Continue
            })
        });
    let mut files = files.into_inner().expect("transcript file collector");
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

/// The decoded events from one forward read, plus where to resume.
#[derive(Clone, Debug)]
pub struct ReadChunk {
    pub events: Vec<AgentEvent>,
    pub next_offset: u64,
    /// True when the file was shorter than the requested offset (truncated or
    /// rotated); the read restarted from byte 0.
    pub reset: bool,
    /// Lines skipped because they failed to parse as JSON.
    pub skipped: usize,
}

/// What a harness can do, for the control facet. A capability is `true` only
/// when a test exercises it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Capabilities {
    pub send_midflight: bool,
    pub resume: bool,
    pub spawn: bool,
    pub subagent_visible: bool,
}

/// The result of sending text to a live session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendOutcome {
    Injected,
    QueuedForNextSpawn,
    Unsupported,
}

/// What a spawn should create.
#[derive(Clone, Debug)]
pub struct SpawnSpec {
    pub harness: String,
    pub branch: String,
    pub base_sha: String,
    pub main_tree: bool,
    /// Worktree gap steps (install, build) run in order before the prompt.
    pub setup: Vec<String>,
    pub prompt: String,
    /// Resume an existing transcript under this session id.
    pub resume_session: Option<String>,
    /// The tmux socket to spawn on (`None` is the default server).
    pub socket: Option<String>,
    /// The directory to run the harness in (the worktree, once created).
    pub worktree_dir: Option<std::path::PathBuf>,
    /// The git checkout a worktree branches from (or the main-tree working
    /// dir when `main_tree` is true).
    pub repo: std::path::PathBuf,
    /// Env assignments prefixed to the launch command. Every value describes
    /// the CHILD; the spawner appears only as BOOP_PARENT.
    pub env_stamp: Option<String>,
    /// The model the lane runs, in the harness's own flag spelling. `None`
    /// lets the harness default; a harness with no default refuses.
    pub model: Option<String>,
    /// opencode reasoning-effort variant (`--variant low|medium|high`).
    /// `None` emits no flag, keeping opencode's own per-model default.
    pub variant: Option<String>,
    /// Shell appended after the harness command exits; it may read `$__rc`
    /// (the harness exit code), which the lane re-raises afterwards.
    pub on_exit: Option<String>,
    /// The tmux session name to spawn under; `None` mints `boop-agent-<hex>`.
    pub tmux: Option<String>,
    /// The lane id the supervisor drains messages for.
    pub lane: String,
    /// The mailbox directory the lane's inbox lives in.
    pub mail_dir: PathBuf,
    /// Run the repo's `boop-start` recipe in a new worktree before spawning.
    pub warm_start: bool,
}

impl SpawnSpec {
    /// Wrap a composed harness command with the on-exit epilogue, preserving
    /// the harness's own exit code.
    pub fn with_on_exit(&self, command: String) -> String {
        match &self.on_exit {
            Some(epilogue) => format!("{command}; __rc=$?; {epilogue}; exit $__rc"),
            None => command,
        }
    }
}
