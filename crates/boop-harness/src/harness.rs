//! The trait every harness adapter implements; the CLI never names a harness.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::Result;

use boop_store::ident::{Store, SyncStat};

pub use boop_store::harness_id::HarnessId;
pub use boop_store::session::{
    ControlCapabilities, Ingested, KnownSession, KnownSessions, OneShotSpec, ReadChunk,
    SendOutcome, SessionRef, SpawnSpec,
};

/// The declared behaviour every former harness-name comparison now reads. One
/// `static CAPABILITIES` per harness module is the whole table.
pub struct Capabilities {
    /// Refuse a model whose own harness runs on a flat-rate plan. Was
    /// `boop-proc/src/lane.rs:343,360`.
    pub bans_plan_family_models: bool,
    /// Whether a worker gets a tmux lane at all. Was `lane.rs:365`.
    pub lanes: LanePolicy,
    /// How reasoning effort is spelled at spawn. Was `boop/src/cli/job.rs:798`.
    pub variant: VariantSupport,
    /// Where a hail lands. Was `boop/src/cli/me.rs:121`.
    pub mail: MailPolicy,
    /// Whether the native TUI wrapper runs the store projector alongside it.
    /// Was `boop/src/cli/control.rs:44`.
    pub native_tui_projector: bool,
}

/// Whether workers of this harness run as tmux lanes.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum LanePolicy {
    Allowed,
    /// Workers are the coordinator's own subagents; a lane spawn needs an
    /// explicit `--harness` to get past the default.
    CoordinatorSubagentsOnly,
}

/// How a spawn spells reasoning effort for this harness.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum VariantSupport {
    Flag,
    ModelSuffixEffort,
    None,
}

/// Where one hail is delivered.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MailPolicy {
    Door,
    TurnBoundaryHook,
    Keystrokes,
}

pub mod claude;
pub mod codex;
pub mod kimi;
pub mod opencode;

/// User-facing interactive launch request. The CLI supplies the executable so
/// aliases such as `ccz` can use the Claude adapter without inventing another
/// harness identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeTuiSpec {
    pub executable: String,
    pub cwd: PathBuf,
    pub args: Vec<String>,
}

/// Prepared native process plus the evidence recorded in its coordinator
/// route before the process takes over the terminal.
pub struct NativeTuiPlan {
    pub program: String,
    pub args: Vec<OsString>,
    pub mode: String,
    pub session_id: Option<String>,
    pub source_path: Option<String>,
    pub app_server_socket: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSessionRef {
    pub session_id: String,
    pub cwd: Option<PathBuf>,
    pub app_server_socket: Option<PathBuf>,
}

/// A native collaboration child fact observed by one harness from its own
/// durable records. The parent and child are harness session identifiers; the
/// child never needs a Boop route name or a mailbox parent setting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeChildEvent {
    Spawned {
        parent_session: String,
        child_session: String,
        at_ms: u64,
    },
    Completed {
        parent_session: String,
        child_session: String,
        outcome: String,
        at_ms: u64,
    },
}

impl NativeTuiPlan {
    pub fn direct(spec: &NativeTuiSpec) -> Self {
        Self {
            program: spec.executable.clone(),
            args: spec.args.iter().map(OsString::from).collect(),
            mode: "interactive".into(),
            session_id: None,
            source_path: Some(format!("native-executable={}", spec.executable)),
            app_server_socket: None,
        }
    }
}

/// Project one transcript file forward from its stored cursor, writing session,
/// turn, touch, cmd, fetch, skill, pr facts. Returns the new offset. A second
/// run with nothing appended after the cursor writes nothing.
pub fn sync_session(
    store: &Store,
    adapter: &dyn Harness,
    session: &SessionRef,
) -> Result<SyncStat> {
    sync_session_with_pid(store, adapter, session, None)
}

/// The pid-observing variant. The observation path (a lane route's pane pid)
/// names this session's process, so agent_live.pid can link session to process.
pub fn sync_session_with_pid(
    store: &Store,
    adapter: &dyn Harness,
    session: &SessionRef,
    pid: Option<i64>,
) -> Result<SyncStat> {
    boop_store::ident::sync_session_with(store, session, pid, |store, session, from| {
        adapter.ingest(store, session, from)
    })
}

/// One agent harness that writes transcripts to this machine. Harnesses are
/// shareable so a caller can bound a synchronous pass on its own thread.
pub trait Harness: Send + Sync {
    /// Stable short id used in CLI output and as the `--harness` filter value.
    fn id(&self) -> HarnessId;

    /// What this harness declares about itself. Every branch that used to
    /// compare a harness name reads one field here.
    fn capabilities(&self) -> &'static Capabilities;

    /// This harness's own live-session registry: the file, database or server
    /// it writes when a TUI is running.
    fn live(&self) -> &dyn crate::live::LiveSessions {
        &crate::door::UNREACHABLE
    }

    /// The control plane one message is written to. The default declares the
    /// harness unreachable rather than guessing a transport.
    fn door(&self) -> &dyn crate::door::Door {
        &crate::door::UNREACHABLE
    }

    /// Resolve the caller identity from the stamp boop puts in a child.
    fn identity_env(&self) -> Option<crate::identity::Identity> {
        crate::identity::from_env_for(self.id())
    }

    /// Resolve a caller pane from routes registered for this harness.
    fn identity_pane(
        &self,
        routes: &std::collections::BTreeMap<String, boop_store::bus::Route>,
    ) -> Option<crate::identity::Identity> {
        crate::identity::from_pane_for(self.id(), routes)
    }

    /// Resolve a caller using a process tell exposed by this harness.
    fn identity_process(&self) -> Option<crate::identity::Identity> {
        None
    }

    /// Root sessions this harness recorded for `cwd`. A sidechain or subagent
    /// transcript carries a parent and never answers for a pane, so only roots
    /// come back.
    fn root_sessions_for_cwd(&self, cwd: &str) -> Result<Vec<SessionRef>> {
        Ok(self
            .sessions()?
            .into_iter()
            .filter(|session| session.cwd.as_deref() == Some(cwd) && session.parent.is_none())
            .collect())
    }

    /// Read this harness's native session identity from the live process tree
    /// rooted at an adopted tmux pane. `None` leaves the route anonymous.
    fn session_id_in_pane(
        &self,
        _multiplexer: &dyn boop_store::tmux::Multiplexer,
        _processes: &dyn boop_store::proc::ProcReader,
        _tmux_target: &str,
    ) -> Option<String> {
        None
    }

    /// Every session this harness has on disk, newest last. No cap.
    fn sessions(&self) -> anyhow::Result<Vec<SessionRef>>;

    /// Directories or stores whose mtimes cover discovery for this harness.
    /// A pass stats these before walking the session tree.
    fn session_roots(&self) -> anyhow::Result<Vec<PathBuf>> {
        Ok(Vec::new())
    }

    fn known_paths_can_move(&self) -> bool {
        true
    }

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
        store: &boop_store::ident::Store,
        session: &SessionRef,
        from: u64,
    ) -> anyhow::Result<Ingested> {
        boop_store::ident::project_transcript(store, session, from)
    }

    /// Observe collaboration child lifecycle facts in transcript bytes after
    /// `from`. The shared Boop projector owns edge persistence, mail, and
    /// parent-route control. A harness implementation owns only its source
    /// record decoding.
    fn observe_native_children(
        &self,
        _session: &SessionRef,
        _from: u64,
    ) -> anyhow::Result<Vec<NativeChildEvent>> {
        Ok(Vec::new())
    }

    /// Whether the parent transcript already contains this harness's native
    /// completion delivery for the exact child. False preserves Boop's queue
    /// fallback.
    fn native_child_completion_visible(
        &self,
        _parent_session: &str,
        _child_session: &str,
    ) -> anyhow::Result<bool> {
        Ok(false)
    }

    // facet 3: control. Defaults are the honest all-false / Unsupported shape,
    // so any adapter without control support is safe and explicit.

    /// What this harness can control. `true` only where a test confirms it.
    fn control_capabilities(&self) -> ControlCapabilities {
        ControlCapabilities::default()
    }

    /// Prepare the ordinary interactive TUI for this harness. Transcript and
    /// control adapters stay colocated in the same harness implementation.
    fn prepare_native_tui(&self, spec: &NativeTuiSpec) -> anyhow::Result<NativeTuiPlan> {
        Ok(NativeTuiPlan::direct(spec))
    }

    /// Deliver one message to an interactive session through the harness's
    /// native control plane. A coordinator must never fall back to PTY keys.
    fn send_native(&self, _session: &NativeSessionRef, _text: &str) -> anyhow::Result<SendOutcome> {
        Ok(SendOutcome::Unsupported)
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

    /// Stop a live session.
    fn stop(&self, _session: &SessionRef) -> anyhow::Result<()> {
        anyhow::bail!("harness `{}` has no stop support", self.id())
    }

    /// Open a lane conversation. The supervisor drives it; nothing else calls
    /// this, and no caller learns which harness answered.
    fn open_channel(
        &self,
        _spec: &boop_acp::channel::ChannelSpec,
    ) -> anyhow::Result<Box<dyn boop_acp::channel::LaneChannel>> {
        anyhow::bail!("harness `{}` has no lane channel", self.id())
    }
}

/// The one command a lane pane runs, whatever the harness: the boop
/// supervisor, which owns the harness child and drains the lane inbox.
pub fn supervisor_command(spec: &SpawnSpec) -> String {
    // Lanes and everything they spawn (cargo, node, browsers) inherit this
    // niceness, so agent builds yield to the interactive UI instead of
    // pinning every core (load 20.8 on 12 cores measured 2026-08-22).
    let mut command = format!(
        "nice -n 10 boop beep lane run --lane {} --harness {} --brief {} --mail-dir {}",
        quote(&spec.lane),
        quote(spec.harness.as_str()),
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

/// Drive one adapter's fixture sessions through a throwaway store and assert
/// the session graph they project. Every harness adapter's fixture test calls it.
#[cfg(test)]
pub(crate) fn assert_fixture_sessions_project(
    adapter: &dyn Harness,
    sessions: &[SessionRef],
    expected_edges: usize,
) {
    let path = std::env::temp_dir().join(format!(
        "boop-session-graph-fixture-{}-{}.db",
        std::process::id(),
        adapter.id()
    ));
    let _ = std::fs::remove_file(&path);
    let store = boop_store::ident::Store::open(path.clone()).unwrap();
    for session in sessions {
        sync_session(&store, adapter, session).unwrap();
    }
    let graph = boop_store::_0_session_graph::load_agent_session_graph(
        &store,
        boop_store::_0_session_graph::AgentSessionGraphQuery {
            cwd: None,
            include_history: true,
            ..boop_store::_0_session_graph::AgentSessionGraphQuery::default()
        },
    )
    .unwrap();
    assert_eq!(graph.sessions.len(), sessions.len());
    assert!(graph.edges.len() >= expected_edges);
    let _ = std::fs::remove_file(path);
}
