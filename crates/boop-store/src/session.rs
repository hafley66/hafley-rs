//! The transcript session types the store and every harness adapter share:
//! what one session on disk is, what the store already knows about it, and
//! what one ingest pass wrote.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::event::AgentEvent;

/// One prompt run to completion, reply text returned. The harness owns the
/// command spelling; no caller learns which binary ran.
pub struct OneShotSpec {
    /// The model in the harness's own flag spelling; `None` lets the harness
    /// default, and a harness with no default refuses.
    pub model: Option<String>,
    pub prompt: String,
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
    pub harness: String,
    pub session_id: String,
    pub nickname: String,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub parent: Option<String>,
    pub cursor: u64,
    pub modified_ms: u64,
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

    /// How many transcript paths this store already knows; a caller measuring
    /// a sync pass reports it beside the pass's own wall.
    pub fn len(&self) -> usize {
        self.0.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
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

/// Milliseconds since the epoch from an RFC 3339 transcript timestamp. Every
/// harness stamps its records this way; a record with no parseable stamp
/// projects at 0 rather than failing the pass.
pub fn parse_iso_ms(text: &str) -> Option<u64> {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;
    let parsed = OffsetDateTime::parse(text, &Rfc3339).ok()?;
    let seconds = parsed.unix_timestamp();
    u64::try_from(seconds)
        .ok()?
        .checked_mul(1000)?
        .checked_add(parsed.millisecond() as u64)
}

/// The codex reasoning efforts; the only spellings an `@` suffix takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effort {
    Low,
    Medium,
    High,
}

impl Effort {
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
        }
    }
}

impl std::str::FromStr for Effort {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Effort> {
        match value {
            "low" => Ok(Effort::Low),
            "medium" => Ok(Effort::Medium),
            "high" => Ok(Effort::High),
            other => {
                anyhow::bail!("effort `{other}` is not one of low, medium, high")
            }
        }
    }
}

/// A model spelling, split on the last `@`. `name@effort` names a reasoning
/// effort; a bare name carries none. An `@` present with no recognized effort
/// after it is a parse error, never a silently-kept model name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelSpec {
    pub name: String,
    pub effort: Option<Effort>,
}

impl std::str::FromStr for ModelSpec {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<ModelSpec> {
        match value.rsplit_once('@') {
            Some((name, suffix)) => {
                let effort = suffix.parse::<Effort>().with_context(|| {
                    format!(
                        "model `{value}` has an `@` suffix that names no reasoning effort \
                         (only low, medium, high are recognized)"
                    )
                })?;
                Ok(ModelSpec {
                    name: name.to_owned(),
                    effort: Some(effort),
                })
            }
            None => Ok(ModelSpec {
                name: value.to_owned(),
                effort: None,
            }),
        }
    }
}

/// What a lane does when its registered parent stops being addressable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum ParentDeathPolicy {
    /// End the lane the way a stall kill does, reporting `parent-died`.
    Kill,
    /// Rewrite the parent edge onto the one registered coordinator, keep going.
    Reparent,
    /// Keep running with the dead edge, which is what every spawn did before
    /// the policy existed.
    #[default]
    Orphan,
}

impl ParentDeathPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            ParentDeathPolicy::Kill => "kill",
            ParentDeathPolicy::Reparent => "reparent",
            ParentDeathPolicy::Orphan => "orphan",
        }
    }
}

impl std::str::FromStr for ParentDeathPolicy {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<ParentDeathPolicy> {
        match value {
            "kill" => Ok(ParentDeathPolicy::Kill),
            "reparent" => Ok(ParentDeathPolicy::Reparent),
            "orphan" => Ok(ParentDeathPolicy::Orphan),
            other => anyhow::bail!("on-parent-death must be kill, reparent or orphan: `{other}`"),
        }
    }
}
