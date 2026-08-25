//! Durable application of a sealed stage to one source root.
//!
//! The commit boundary owns the target filesystem. It takes a root-scoped
//! lock, validates every operation while the lock is held, writes a journal
//! outside the target root, and only then performs atomic file operations.
//! Recovery re-reads the journal and classifies each operation from its before
//! and after bytes, so replay is idempotent after a process interruption.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result};
use atomic_write_file::AtomicWriteFile;
use fs4::fs_std::FileExt;
use serde::{Deserialize, Serialize};

use crate::_7e_stage_store::{elapsed_millis, SyncMeter};
use crate::{
    ContentId, FileModeObservation, PlannedFileKind, SourcePath, SourceRoot, SourceRootId, StageId,
    StagedContentId, StagedFile, StagedSourceTransaction,
};

const JOURNAL_VERSION: u32 = 2;

/// A deterministic interruption point used by integration tests and hosts
/// that need to exercise restart recovery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitFailpoint {
    AfterJournal,
    BeforeOperation(usize),
    AfterOperation(usize),
}

/// Stable correlation data for filesystem watchers. A watcher can use the
/// stage ID and exact paths to coalesce the events caused by one commit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitWatchCorrelation {
    pub stage_id: StageId,
    pub paths: Vec<SourcePath>,
    pub post: Vec<CommitWatchPost>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitWatchPost {
    pub path: SourcePath,
    pub present: bool,
    pub content: Option<ContentId>,
}

/// The durable result of a successful commit or a completed recovery.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitReceipt {
    pub stage_id: StageId,
    pub root: SourceRootId,
    pub applied_files: usize,
    pub operations: Vec<CommitOperationReceipt>,
    pub journal_bytes: u64,
    pub checkpoint_bytes: u64,
    pub watch: CommitWatchCorrelation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitOperationReceipt {
    pub kind: PlannedFileKind,
    pub path_before: Option<SourcePath>,
    pub path_after: Option<SourcePath>,
    pub before: Option<ContentId>,
    pub after: Option<ContentId>,
}

/// Typed refusal from the commit boundary. Filesystem failures are retained
/// as strings so the public API does not expose a platform-specific error
/// enum.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitRefusal {
    RootMismatch {
        expected: SourceRootId,
        actual: SourceRootId,
    },
    UnsafePath {
        path: SourcePath,
        reason: String,
    },
    Preflight {
        path: SourcePath,
        reason: String,
    },
    RecoveryRequired {
        stage_id: StageId,
        journal: PathBuf,
    },
    ReceiptDiverged {
        stage_id: StageId,
        path: SourcePath,
        reason: String,
    },
    Locked {
        lock: PathBuf,
    },
    Io {
        operation: String,
        detail: String,
    },
    Failpoint {
        point: CommitFailpoint,
    },
}

impl fmt::Display for CommitRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RootMismatch { expected, actual } => {
                write!(
                    f,
                    "commit root mismatch: expected {expected:?}, got {actual:?}"
                )
            }
            Self::UnsafePath { path, reason } => write!(f, "unsafe commit path {path:?}: {reason}"),
            Self::Preflight { path, reason } => {
                write!(f, "commit preflight failed for {path:?}: {reason}")
            }
            Self::RecoveryRequired { stage_id, journal } => {
                write!(
                    f,
                    "commit recovery required for {stage_id} at {}",
                    journal.display()
                )
            }
            Self::ReceiptDiverged {
                stage_id,
                path,
                reason,
            } => write!(f, "receipt {stage_id} diverged at {path:?}: {reason}"),
            Self::Locked { lock } => write!(f, "commit root is locked: {}", lock.display()),
            Self::Io { operation, detail } => write!(f, "commit {operation}: {detail}"),
            Self::Failpoint { point } => write!(f, "commit failpoint: {point:?}"),
        }
    }
}

impl std::error::Error for CommitRefusal {}

/// Whether commit state has to survive a machine crash.
///
/// `Rehearsal` targets a throwaway mirror: every journal, payload, receipt and
/// target write still lands and is still read back, and no write is flushed to
/// the device. A rehearsal engine must never be pointed at a root a human
/// keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Durability {
    #[default]
    Durable,
    Rehearsal,
}

impl Durability {
    fn rehearsal(self) -> bool {
        self == Self::Rehearsal
    }
}

/// A commit engine for one canonical target root and an external state root.
/// `state_root` must be outside `target_root`; journals and lock files never
/// become source-tree entries.
#[derive(Clone, Debug)]
pub struct CommitEngine {
    target_root: PathBuf,
    state_root: PathBuf,
    durability: Durability,
}

impl CommitEngine {
    pub fn open(target_root: impl AsRef<Path>, state_root: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_durability(target_root, state_root, Durability::Durable)
    }

    /// Open an engine for a throwaway mirror. Identical control flow to
    /// [`CommitEngine::open`] with every device flush dropped.
    pub fn open_rehearsal(
        target_root: impl AsRef<Path>,
        state_root: impl AsRef<Path>,
    ) -> Result<Self> {
        Self::open_with_durability(target_root, state_root, Durability::Rehearsal)
    }

    pub fn durability(&self) -> Durability {
        self.durability
    }

    fn open_with_durability(
        target_root: impl AsRef<Path>,
        state_root: impl AsRef<Path>,
        durability: Durability,
    ) -> Result<Self> {
        let target_root = target_root.as_ref().canonicalize().with_context(|| {
            format!(
                "canonicalize commit target root {}",
                target_root.as_ref().display()
            )
        })?;
        if !target_root.is_dir() {
            anyhow::bail!(
                "commit target root is not a directory: {}",
                target_root.display()
            );
        }
        let state_root = state_root.as_ref().to_path_buf();
        let state_parent = state_root.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(state_parent)?;
        fs::create_dir_all(&state_root)?;
        let state_root = state_root.canonicalize()?;
        if state_root.starts_with(&target_root) {
            anyhow::bail!("commit state root must be outside target root");
        }
        fs::create_dir_all(state_root.join("locks"))?;
        fs::create_dir_all(state_root.join("journals"))?;
        fs::create_dir_all(state_root.join("blobs"))?;
        fs::create_dir_all(state_root.join("receipts"))?;
        Ok(Self {
            target_root,
            state_root,
            durability,
        })
    }

    pub fn commit(
        &self,
        stage: &StagedSourceTransaction,
    ) -> std::result::Result<CommitReceipt, CommitRefusal> {
        self.commit_with_failpoint(stage, None)
    }

    pub fn commit_with_failpoint(
        &self,
        stage: &StagedSourceTransaction,
        failpoint: Option<CommitFailpoint>,
    ) -> std::result::Result<CommitReceipt, CommitRefusal> {
        let commit_started = Instant::now();
        let commit_span = tracing::debug_span!(
            "commit.transaction",
            operation = "commit",
            files = stage.files.len(),
            durability = ?self.durability,
            duration_ms = tracing::field::Empty,
        );
        let _commit_entered = commit_span.enter();

        let lock_started = Instant::now();
        let _lock = tracing::debug_span!("commit.lock", operation = "lock_root")
            .in_scope(|| self.lock())?;
        tracing::debug!(
            duration_ms = elapsed_millis(lock_started),
            "commit root locked"
        );

        let identity_started = Instant::now();
        let identity_span =
            tracing::debug_span!("commit.verify_identity", operation = "actual_root_id");
        let actual = identity_span
            .in_scope(|| self.actual_root_id(&stage.root))
            .map_err(io_refusal)?;
        tracing::debug!(
            duration_ms = elapsed_millis(identity_started),
            "commit root identity verified"
        );
        if actual != stage.root {
            return Err(CommitRefusal::RootMismatch {
                expected: stage.root.clone(),
                actual,
            });
        }
        if let Some(receipt) = self.read_receipt(stage.id)? {
            let expected_operations = stage_operation_receipts(&stage.files);
            let expected_watch = watch_projection(stage.id, &expected_operations);
            if receipt.operations != expected_operations
                || receipt.applied_files != expected_operations.len()
                || receipt.watch != expected_watch
            {
                return Err(CommitRefusal::ReceiptDiverged {
                    stage_id: stage.id,
                    path: expected_operations
                        .first()
                        .and_then(|operation| {
                            operation
                                .path_after
                                .clone()
                                .or(operation.path_before.clone())
                        })
                        .unwrap_or(SourcePath::Directory {
                            path: crate::RootPath(std::sync::Arc::from(".")),
                        }),
                    reason: "receipt does not match the sealed stage".into(),
                });
            }
            validate_receipt(&self.target_root, &receipt, stage.id, &stage.root)?;
            return Ok(receipt);
        }
        let journal_path = self.journal_path(stage.id);
        if journal_path.exists() {
            return Err(CommitRefusal::RecoveryRequired {
                stage_id: stage.id,
                journal: journal_path,
            });
        }
        let preflight_started = Instant::now();
        let preflight_span = tracing::debug_span!(
            "commit.preflight",
            operation = "preflight",
            files = stage.files.len(),
        );
        let journal = preflight_span.in_scope(|| self.preflight(stage))?;
        validate_journal(&journal)?;
        tracing::debug!(
            files = journal.operations.len(),
            duration_ms = elapsed_millis(preflight_started),
            "commit preflight cleared"
        );

        self.materialize_payloads(stage, &journal)?;

        let journal_started = Instant::now();
        let journal_span = tracing::debug_span!(
            "commit.write_journal",
            operation = "write_journal",
            files = journal.operations.len(),
            fsync.count = tracing::field::Empty,
            fsync_ms = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
        );
        let journal_entered = journal_span.enter();
        let mut journal_sync = SyncMeter::default();
        write_journal(&journal_path, &journal, self.durability, &mut journal_sync)?;
        journal_span.record("fsync.count", journal_sync.count());
        journal_span.record("fsync_ms", journal_sync.millis());
        journal_span.record("duration_ms", elapsed_millis(journal_started));
        tracing::debug!(
            fsync.count = journal_sync.count(),
            fsync_ms = journal_sync.millis(),
            duration_ms = elapsed_millis(journal_started),
            "commit journal written"
        );
        drop(journal_entered);

        if failpoint == Some(CommitFailpoint::AfterJournal) {
            return Err(CommitRefusal::Failpoint {
                point: CommitFailpoint::AfterJournal,
            });
        }
        self.apply_journal(&journal, failpoint, &BTreeSet::new())?;

        let receipt_started = Instant::now();
        let receipt_span = tracing::debug_span!(
            "commit.write_receipt",
            operation = "write_receipt",
            files = journal.operations.len(),
            fsync.count = tracing::field::Empty,
            fsync_ms = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
        );
        let receipt_entered = receipt_span.enter();
        let receipt = receipt(
            stage.id,
            &stage.root,
            &journal.operations,
            fs::metadata(&journal_path).map(|m| m.len()).unwrap_or(0),
            0,
        );
        let mut receipt_sync = SyncMeter::default();
        write_receipt(
            &self.receipt_path(stage.id),
            &receipt,
            self.durability,
            &mut receipt_sync,
        )?;
        remove_journal(&journal_path, self.durability, &mut receipt_sync)?;
        receipt_span.record("fsync.count", receipt_sync.count());
        receipt_span.record("fsync_ms", receipt_sync.millis());
        receipt_span.record("duration_ms", elapsed_millis(receipt_started));
        tracing::debug!(
            fsync.count = receipt_sync.count(),
            fsync_ms = receipt_sync.millis(),
            duration_ms = elapsed_millis(receipt_started),
            "commit receipt written"
        );
        drop(receipt_entered);

        commit_span.record("duration_ms", elapsed_millis(commit_started));
        tracing::debug!(
            files = receipt.applied_files,
            duration_ms = elapsed_millis(commit_started),
            "commit applied"
        );
        Ok(receipt)
    }

    /// Reconcile and replay an interrupted journal. Every operation is checked
    /// against its before/after state before any pending operation runs.
    pub fn recover(&self, stage_id: StageId) -> std::result::Result<CommitReceipt, CommitRefusal> {
        let _lock = self.lock()?;
        let journal_path = self.journal_path(stage_id);
        let journal = read_journal(&journal_path)?;
        if journal.stage_id != stage_id {
            return Err(io_refusal_context(
                "validate commit journal",
                "journal stage ID mismatch",
            ));
        }
        validate_journal(&journal)?;
        let actual = self.actual_root_id(&journal.root).map_err(io_refusal)?;
        if actual != journal.root {
            return Err(CommitRefusal::RootMismatch {
                expected: journal.root.clone(),
                actual,
            });
        }
        for operation in &journal.operations {
            for path in operation.paths() {
                validate_relative_path(&path)?;
                check_parent_components(&self.target_root, &path)?;
            }
        }
        let states = journal
            .operations
            .iter()
            .map(|operation| {
                let bytes = self.operation_bytes(operation)?;
                let state = classify_operation(&self.target_root, operation, bytes.as_deref())?;
                Ok(state)
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if states.contains(&OperationState::Diverged) {
            return Err(CommitRefusal::RecoveryRequired {
                stage_id,
                journal: journal_path,
            });
        }
        let completed = states
            .iter()
            .enumerate()
            .filter_map(|(index, state)| (*state == OperationState::After).then_some(index))
            .collect();
        self.apply_journal(&journal, None, &completed)?;
        let receipt = receipt(
            stage_id,
            &journal.root,
            &journal.operations,
            fs::metadata(&journal_path).map(|m| m.len()).unwrap_or(0),
            0,
        );
        let mut recovery_sync = SyncMeter::default();
        write_receipt(
            &self.receipt_path(stage_id),
            &receipt,
            self.durability,
            &mut recovery_sync,
        )?;
        remove_journal(&journal_path, self.durability, &mut recovery_sync)?;
        Ok(receipt)
    }

    pub fn journal_path_for(&self, stage_id: StageId) -> PathBuf {
        self.journal_path(stage_id)
    }

    fn receipt_path(&self, id: StageId) -> PathBuf {
        self.state_root.join("receipts").join(format!("{id}.json"))
    }

    fn read_receipt(
        &self,
        id: StageId,
    ) -> std::result::Result<Option<CommitReceipt>, CommitRefusal> {
        let path = self.receipt_path(id);
        match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|error| io_refusal_context("decode commit receipt", error)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(io_refusal_context("read commit receipt", error)),
        }
    }

    fn lock(&self) -> std::result::Result<RootLock, CommitRefusal> {
        let key = blake3::hash(self.target_root.as_os_str().to_string_lossy().as_bytes()).to_hex();
        let path = self.state_root.join("locks").join(format!("{key}.lock"));
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| io_refusal_context("open root lock", error))?;
        file.try_lock_exclusive()
            .map_err(|_| CommitRefusal::Locked { lock: path.clone() })?;
        Ok(RootLock { file })
    }

    fn journal_path(&self, id: StageId) -> PathBuf {
        self.state_root.join("journals").join(format!("{id}.json"))
    }

    fn blob_path(&self, id: StagedContentId) -> PathBuf {
        self.state_root.join("blobs").join(id.to_hex())
    }

    fn materialize_payloads(
        &self,
        stage: &StagedSourceTransaction,
        journal: &CommitJournal,
    ) -> std::result::Result<(), CommitRefusal> {
        let started = Instant::now();
        let span = tracing::debug_span!(
            "commit.materialize_payloads",
            operation = "materialize_payloads",
            files = stage.files.len(),
            fsync.count = tracing::field::Empty,
            fsync_ms = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
        );
        let _entered = span.enter();
        let mut meter = SyncMeter::default();
        for (file, operation) in stage.files.iter().zip(&journal.operations) {
            let Some(blob) = operation.after_blob else {
                continue;
            };
            let Some(bytes) = file.bytes_after.as_deref() else {
                return Err(io_refusal_context(
                    "materialize commit payload",
                    "stage has no output bytes",
                ));
            };
            if blake3::hash(bytes) != blake3::Hash::from_bytes(blob.0) {
                return Err(io_refusal_context(
                    "materialize commit payload",
                    "stage blob digest mismatch",
                ));
            }
            let path = self.blob_path(blob);
            if path.exists() {
                let existing = fs::read(&path)
                    .map_err(|error| io_refusal_context("read commit payload", error))?;
                if blake3::hash(&existing) != blake3::Hash::from_bytes(blob.0) {
                    return Err(io_refusal_context(
                        "verify commit payload",
                        "payload digest mismatch",
                    ));
                }
                continue;
            }
            if self.durability.rehearsal() {
                write_unsynced(&path, bytes, None, "write commit payload")?;
            } else {
                let mut file = AtomicWriteFile::open(&path)
                    .map_err(|error| io_refusal_context("open commit payload", error))?;
                file.write_all(bytes)
                    .map_err(|error| io_refusal_context("write commit payload", error))?;
                meter
                    .measure(|| file.commit())
                    .map_err(|error| io_refusal_context("publish commit payload", error))?;
            }
        }
        sync_dir_when_durable(&self.state_root.join("blobs"), self.durability, &mut meter)?;
        span.record("fsync.count", meter.count());
        span.record("fsync_ms", meter.millis());
        span.record("duration_ms", elapsed_millis(started));
        tracing::debug!(
            files = stage.files.len(),
            fsync.count = meter.count(),
            fsync_ms = meter.millis(),
            duration_ms = elapsed_millis(started),
            "commit payloads materialized"
        );
        Ok(())
    }

    fn operation_bytes(
        &self,
        operation: &JournalOperation,
    ) -> std::result::Result<Option<Vec<u8>>, CommitRefusal> {
        let Some(blob) = operation.after_blob else {
            return Ok(None);
        };
        let bytes = fs::read(self.blob_path(blob))
            .map_err(|error| io_refusal_context("read commit payload", error))?;
        if blake3::hash(&bytes) != blake3::Hash::from_bytes(blob.0) {
            return Err(io_refusal_context(
                "verify commit payload",
                "payload digest mismatch",
            ));
        }
        Ok(Some(bytes))
    }

    fn actual_root_id(&self, expected: &SourceRootId) -> Result<SourceRootId> {
        let root = match expected {
            SourceRootId::Directory { .. } => SourceRoot::open_directory(&self.target_root)?,
            SourceRootId::GitWorktree { .. } => SourceRoot::discover_git(&self.target_root)?,
        };
        Ok(match root {
            SourceRoot::Directory(directory) => SourceRootId::Directory {
                directory: directory.identity,
            },
            SourceRoot::GitWorktree(git) => SourceRootId::GitWorktree {
                repository: git.repository.identity,
                worktree: git.repository.worktree,
            },
        })
    }

    fn preflight(
        &self,
        stage: &StagedSourceTransaction,
    ) -> std::result::Result<CommitJournal, CommitRefusal> {
        let mut operations = Vec::with_capacity(stage.files.len());
        let mut paths = BTreeSet::new();
        for file in &stage.files {
            let operation = operation_from_file(file)?;
            for path in operation.paths() {
                validate_relative_path(&path)?;
                check_parent_components(&self.target_root, &path)?;
                paths.insert(path.clone());
            }
            preflight_operation(&self.target_root, &operation)?;
            operations.push(operation);
        }
        Ok(CommitJournal {
            version: JOURNAL_VERSION,
            stage_id: stage.id,
            root: stage.root.clone(),
            operations,
        })
    }

    fn apply_journal(
        &self,
        journal: &CommitJournal,
        failpoint: Option<CommitFailpoint>,
        completed: &BTreeSet<usize>,
    ) -> std::result::Result<(), CommitRefusal> {
        let started = Instant::now();
        let span = tracing::debug_span!(
            "commit.apply",
            operation = "apply_journal",
            files = journal.operations.len(),
            fsync.count = tracing::field::Empty,
            fsync_ms = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
        );
        let _entered = span.enter();
        let mut meter = SyncMeter::default();
        for index in 0..journal.operations.len() {
            if completed.contains(&index) {
                continue;
            }
            if failpoint == Some(CommitFailpoint::BeforeOperation(index)) {
                return Err(CommitRefusal::Failpoint {
                    point: CommitFailpoint::BeforeOperation(index),
                });
            }
            let bytes = self.operation_bytes(&journal.operations[index])?;
            apply_operation(
                &self.target_root,
                &journal.operations[index],
                bytes.as_deref().unwrap_or_default(),
                self.durability,
                &mut meter,
            )?;
            if failpoint == Some(CommitFailpoint::AfterOperation(index)) {
                return Err(CommitRefusal::Failpoint {
                    point: CommitFailpoint::AfterOperation(index),
                });
            }
        }
        span.record("fsync.count", meter.count());
        span.record("fsync_ms", meter.millis());
        span.record("duration_ms", elapsed_millis(started));
        tracing::debug!(
            files = journal.operations.len(),
            fsync.count = meter.count(),
            fsync_ms = meter.millis(),
            duration_ms = elapsed_millis(started),
            "commit journal applied"
        );
        Ok(())
    }
}

#[derive(Debug)]
struct RootLock {
    file: File,
}

impl Drop for RootLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CommitJournal {
    version: u32,
    stage_id: StageId,
    root: SourceRootId,
    operations: Vec<JournalOperation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct JournalOperation {
    kind: PlannedFileKind,
    before_path: Option<SourcePath>,
    after_path: Option<SourcePath>,
    expected: Option<ContentId>,
    after_blob: Option<StagedContentId>,
    after_content: Option<ContentId>,
    mode: Option<FileModeObservation>,
}

impl JournalOperation {
    fn paths(&self) -> Vec<SourcePath> {
        self.before_path
            .iter()
            .chain(self.after_path.iter())
            .cloned()
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OperationState {
    Before,
    After,
    Diverged,
}

fn operation_from_file(file: &StagedFile) -> std::result::Result<JournalOperation, CommitRefusal> {
    if file.kind == PlannedFileKind::Create && file.path_before.is_some() {
        return Err(CommitRefusal::Preflight {
            path: file.path_before.clone().unwrap(),
            reason: "create has a source path".into(),
        });
    }
    if file.kind == PlannedFileKind::Delete && file.path_after.is_some() {
        return Err(CommitRefusal::Preflight {
            path: file.path_after.clone().unwrap(),
            reason: "delete has a destination path".into(),
        });
    }
    Ok(JournalOperation {
        kind: file.kind,
        before_path: file.path_before.clone(),
        after_path: file.path_after.clone(),
        expected: file.content_before.clone(),
        after_blob: file.staged_bytes.or_else(|| {
            file.bytes_after
                .as_deref()
                .map(|bytes| StagedContentId(*blake3::hash(bytes).as_bytes()))
        }),
        after_content: file.content_after.clone(),
        mode: file.mode_before.clone(),
    })
}

fn preflight_operation(
    root: &Path,
    operation: &JournalOperation,
) -> std::result::Result<(), CommitRefusal> {
    match operation.kind {
        PlannedFileKind::Create => {
            let destination = operation
                .after_path
                .as_ref()
                .ok_or_else(|| missing_path(operation))?;
            let path = target_path(root, destination);
            if fs::symlink_metadata(&path).is_ok() {
                return Err(preflight(destination, "create destination already exists"));
            }
            if operation.after_blob.is_none() {
                return Err(preflight(destination, "create has no output bytes"));
            }
        }
        PlannedFileKind::Replace => {
            let source = operation
                .before_path
                .as_ref()
                .ok_or_else(|| missing_path(operation))?;
            verify_existing(
                root,
                source,
                operation.expected.as_ref(),
                operation.mode.as_ref(),
            )?;
            if operation.after_blob.is_none() {
                return Err(preflight(source, "replace has no output bytes"));
            }
        }
        PlannedFileKind::Move => {
            let source = operation
                .before_path
                .as_ref()
                .ok_or_else(|| missing_path(operation))?;
            let destination = operation
                .after_path
                .as_ref()
                .ok_or_else(|| missing_path(operation))?;
            verify_existing(
                root,
                source,
                operation.expected.as_ref(),
                operation.mode.as_ref(),
            )?;
            let destination_path = target_path(root, destination);
            if fs::symlink_metadata(&destination_path).is_ok() {
                return Err(preflight(destination, "move destination already exists"));
            }
            if operation.after_blob.is_none() {
                return Err(preflight(destination, "move has no output bytes"));
            }
        }
        PlannedFileKind::Delete => {
            let source = operation
                .before_path
                .as_ref()
                .ok_or_else(|| missing_path(operation))?;
            verify_existing(
                root,
                source,
                operation.expected.as_ref(),
                operation.mode.as_ref(),
            )?;
        }
    }
    Ok(())
}

fn apply_operation(
    root: &Path,
    operation: &JournalOperation,
    bytes: &[u8],
    durability: Durability,
    meter: &mut SyncMeter,
) -> std::result::Result<(), CommitRefusal> {
    match operation.kind {
        PlannedFileKind::Create | PlannedFileKind::Replace => {
            let path = target_path(
                root,
                operation
                    .after_path
                    .as_ref()
                    .ok_or_else(|| missing_path(operation))?,
            );
            ensure_parent_dirs(root, &path)?;
            atomic_write(&path, bytes, operation.mode.as_ref(), durability, meter)?;
            sync_dir_when_durable(
                path.parent().unwrap_or_else(|| Path::new(".")),
                durability,
                meter,
            )?;
        }
        PlannedFileKind::Move => {
            let source = target_path(
                root,
                operation
                    .before_path
                    .as_ref()
                    .ok_or_else(|| missing_path(operation))?,
            );
            let destination = target_path(
                root,
                operation
                    .after_path
                    .as_ref()
                    .ok_or_else(|| missing_path(operation))?,
            );
            ensure_parent_dirs(root, &destination)?;
            fs::rename(&source, &destination)
                .map_err(|error| io_refusal_context("rename staged move", error))?;
            sync_dir_when_durable(
                source.parent().unwrap_or_else(|| Path::new(".")),
                durability,
                meter,
            )?;
            if destination.parent() != source.parent() {
                sync_dir_when_durable(
                    destination.parent().unwrap_or_else(|| Path::new(".")),
                    durability,
                    meter,
                )?;
            }
        }
        PlannedFileKind::Delete => {
            let source = target_path(
                root,
                operation
                    .before_path
                    .as_ref()
                    .ok_or_else(|| missing_path(operation))?,
            );
            fs::remove_file(&source)
                .map_err(|error| io_refusal_context("delete staged file", error))?;
            sync_dir_when_durable(
                source.parent().unwrap_or_else(|| Path::new(".")),
                durability,
                meter,
            )?;
        }
    }
    Ok(())
}

fn classify_operation(
    root: &Path,
    operation: &JournalOperation,
    after: Option<&[u8]>,
) -> std::result::Result<OperationState, CommitRefusal> {
    match operation.kind {
        PlannedFileKind::Create | PlannedFileKind::Replace => {
            let path = target_path(
                root,
                operation
                    .after_path
                    .as_ref()
                    .ok_or_else(|| missing_path(operation))?,
            );
            match fs::symlink_metadata(&path) {
                Ok(metadata) if !metadata.file_type().is_file() => Err(preflight(
                    operation.after_path.as_ref().unwrap(),
                    "recovery target is not a regular file",
                )),
                Ok(_) if Some(read_bytes(&path)?.as_slice()) == after => Ok(OperationState::After),
                Ok(_)
                    if operation.kind == PlannedFileKind::Replace
                        && content_matches(
                            root,
                            operation.after_path.as_ref().unwrap(),
                            operation.expected.as_ref(),
                        )? =>
                {
                    Ok(OperationState::Before)
                }
                Err(_)
                    if operation.kind == PlannedFileKind::Create
                        && operation.expected.is_none() =>
                {
                    Ok(OperationState::Before)
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::NotFound
                        && operation.kind == PlannedFileKind::Replace =>
                {
                    Err(preflight(
                        operation.after_path.as_ref().unwrap(),
                        "recovery replacement disappeared",
                    ))
                }
                _ => Ok(OperationState::Diverged),
            }
        }
        PlannedFileKind::Delete => {
            let path = target_path(
                root,
                operation
                    .before_path
                    .as_ref()
                    .ok_or_else(|| missing_path(operation))?,
            );
            match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Ok(OperationState::After)
                }
                Ok(_)
                    if content_matches(
                        root,
                        operation.before_path.as_ref().unwrap(),
                        operation.expected.as_ref(),
                    )? =>
                {
                    Ok(OperationState::Before)
                }
                Ok(_) => Ok(OperationState::Diverged),
                Err(error) => Err(io_refusal_context("inspect delete recovery", error)),
            }
        }
        PlannedFileKind::Move => {
            let source = target_path(
                root,
                operation
                    .before_path
                    .as_ref()
                    .ok_or_else(|| missing_path(operation))?,
            );
            let destination = target_path(
                root,
                operation
                    .after_path
                    .as_ref()
                    .ok_or_else(|| missing_path(operation))?,
            );
            let source_state = fs::symlink_metadata(&source)
                .ok()
                .map(|_| read_bytes(&source))
                .transpose()?;
            let destination_state = fs::symlink_metadata(&destination)
                .ok()
                .map(|_| read_bytes(&destination))
                .transpose()?;
            if source_state.is_some()
                && destination_state.is_none()
                && content_matches(
                    root,
                    operation.before_path.as_ref().unwrap(),
                    operation.expected.as_ref(),
                )?
            {
                Ok(OperationState::Before)
            } else if source_state.is_none() && destination_state.as_deref() == after {
                Ok(OperationState::After)
            } else {
                Ok(OperationState::Diverged)
            }
        }
    }
}

fn verify_existing(
    root: &Path,
    source: &SourcePath,
    expected: Option<&ContentId>,
    mode: Option<&FileModeObservation>,
) -> std::result::Result<(), CommitRefusal> {
    let path = target_path(root, source);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| preflight(source, format!("source unavailable: {error}")))?;
    if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
        return Err(preflight(source, "source is not a regular file"));
    }
    if !content_matches(root, source, expected)? {
        return Err(preflight(source, "source content changed after staging"));
    }
    let Some(expected_mode) = mode else {
        return Err(preflight(source, "source mode was not captured"));
    };
    if !mode_matches(&metadata, expected_mode) {
        return Err(preflight(source, "source mode changed after staging"));
    }
    Ok(())
}

fn mode_matches(metadata: &std::fs::Metadata, expected: &FileModeObservation) -> bool {
    if metadata.permissions().readonly() != expected.readonly {
        return false;
    }
    #[cfg(unix)]
    if let Some(mode) = expected.unix_mode {
        use std::os::unix::fs::MetadataExt;
        return metadata.mode() == mode;
    }
    true
}

fn content_matches(
    root: &Path,
    source: &SourcePath,
    expected: Option<&ContentId>,
) -> std::result::Result<bool, CommitRefusal> {
    let Some(expected) = expected else {
        return Ok(false);
    };
    let bytes = read_bytes(&target_path(root, source))?;
    let observed = match expected {
        ContentId::Blake3(_) => ContentId::Blake3(*blake3::hash(&bytes).as_bytes()),
        ContentId::GitBlob(_) => {
            let repository = crate::_2_repository::discover(root).map_err(io_refusal)?;
            ContentId::GitBlob(
                crate::_9_git_files::hash_object(&repository, &bytes).map_err(io_refusal)?,
            )
        }
    };
    Ok(&observed == expected)
}

fn atomic_write(
    path: &Path,
    bytes: &[u8],
    mode: Option<&FileModeObservation>,
    durability: Durability,
    meter: &mut SyncMeter,
) -> std::result::Result<(), CommitRefusal> {
    if durability.rehearsal() {
        return write_unsynced(path, bytes, mode, "write atomic target");
    }
    let mut file = AtomicWriteFile::open(path)
        .map_err(|error| io_refusal_context("open atomic target", error))?;
    file.write_all(bytes)
        .map_err(|error| io_refusal_context("write atomic target", error))?;
    if let Some(mode) = mode {
        set_mode(&file, mode).map_err(|error| io_refusal_context("set target mode", error))?;
    }
    meter
        .measure(|| file.commit())
        .map_err(|error| io_refusal_context("publish atomic target", error))
}

fn set_mode(file: &File, mode: &FileModeObservation) -> std::io::Result<()> {
    let mut permissions = file.metadata()?.permissions();
    permissions.set_readonly(mode.readonly);
    #[cfg(unix)]
    if let Some(unix_mode) = mode.unix_mode {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(unix_mode);
    }
    file.set_permissions(permissions)
}

fn validate_relative_path(path: &SourcePath) -> std::result::Result<(), CommitRefusal> {
    let raw = path_text(path);
    if raw.is_empty() || raw == "." {
        return Err(unsafe_path(path, "empty path"));
    }
    if Path::new(raw).is_absolute() {
        return Err(unsafe_path(path, "absolute path"));
    }
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(part) if part == ".git" => {
                return Err(unsafe_path(path, ".git path is confined"))
            }
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(unsafe_path(path, "path escapes root"))
            }
            _ => {}
        }
    }
    Ok(())
}

fn check_parent_components(
    root: &Path,
    path: &SourcePath,
) -> std::result::Result<(), CommitRefusal> {
    let raw = path_text(path);
    let mut current = root.to_path_buf();
    let components: Vec<_> = Path::new(raw).components().collect();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        if let Component::Normal(name) = component {
            current.push(name);
            match fs::symlink_metadata(&current) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    return Err(unsafe_path(path, "parent component is a symlink"))
                }
                Ok(metadata) if !metadata.is_dir() => {
                    return Err(preflight(path, "parent component is not a directory"))
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_refusal_context("inspect parent component", error)),
            }
        }
    }
    Ok(())
}

fn ensure_parent_dirs(_root: &Path, path: &Path) -> std::result::Result<(), CommitRefusal> {
    let parent = path.parent().ok_or_else(|| CommitRefusal::Io {
        operation: "find target parent".into(),
        detail: path.display().to_string(),
    })?;
    fs::create_dir_all(parent)
        .map_err(|error| io_refusal_context("create target parent", error))?;
    Ok(())
}

fn target_path(root: &Path, path: &SourcePath) -> PathBuf {
    root.join(path_text(path))
}
fn path_text(path: &SourcePath) -> &str {
    match path {
        SourcePath::Directory { path } => &path.0,
        SourcePath::Git { path } => &path.0,
    }
}
fn read_bytes(path: &Path) -> std::result::Result<Vec<u8>, CommitRefusal> {
    fs::read(path).map_err(|error| io_refusal_context("read commit target", error))
}
fn missing_path(operation: &JournalOperation) -> CommitRefusal {
    CommitRefusal::Io {
        operation: "construct journal operation".into(),
        detail: format!("missing path for {:?}", operation.kind),
    }
}
fn preflight(path: &SourcePath, reason: impl Into<String>) -> CommitRefusal {
    CommitRefusal::Preflight {
        path: path.clone(),
        reason: reason.into(),
    }
}
fn unsafe_path(path: &SourcePath, reason: impl Into<String>) -> CommitRefusal {
    CommitRefusal::UnsafePath {
        path: path.clone(),
        reason: reason.into(),
    }
}
fn io_refusal(error: anyhow::Error) -> CommitRefusal {
    CommitRefusal::Io {
        operation: "commit".into(),
        detail: error.to_string(),
    }
}
fn io_refusal_context(operation: &str, error: impl fmt::Display) -> CommitRefusal {
    CommitRefusal::Io {
        operation: operation.into(),
        detail: error.to_string(),
    }
}

fn write_journal(
    path: &Path,
    journal: &CommitJournal,
    durability: Durability,
    meter: &mut SyncMeter,
) -> std::result::Result<(), CommitRefusal> {
    let bytes = serde_json::to_vec(journal)
        .map_err(|error| io_refusal_context("encode commit journal", error))?;
    if durability.rehearsal() {
        write_unsynced(path, &bytes, None, "write commit journal")?;
    } else {
        let mut file = AtomicWriteFile::open(path)
            .map_err(|error| io_refusal_context("open commit journal", error))?;
        file.write_all(&bytes)
            .map_err(|error| io_refusal_context("write commit journal", error))?;
        meter
            .measure(|| file.commit())
            .map_err(|error| io_refusal_context("publish commit journal", error))?;
    }
    sync_dir_when_durable(
        path.parent().unwrap_or_else(|| Path::new(".")),
        durability,
        meter,
    )
}
fn read_journal(path: &Path) -> std::result::Result<CommitJournal, CommitRefusal> {
    let bytes = fs::read(path).map_err(|error| io_refusal_context("read commit journal", error))?;
    let journal: CommitJournal = serde_json::from_slice(&bytes)
        .map_err(|error| io_refusal_context("decode commit journal", error))?;
    if journal.version != JOURNAL_VERSION {
        return Err(io_refusal_context(
            "validate commit journal",
            "unsupported journal version",
        ));
    }
    Ok(journal)
}

fn validate_journal(journal: &CommitJournal) -> std::result::Result<(), CommitRefusal> {
    for operation in &journal.operations {
        for path in operation.paths() {
            validate_relative_path(&path)?;
        }
        match journal.root {
            SourceRootId::Directory { .. }
                if operation
                    .paths()
                    .iter()
                    .any(|path| !matches!(path, SourcePath::Directory { .. })) =>
            {
                return Err(io_refusal_context(
                    "validate commit journal",
                    "directory journal has Git path",
                ));
            }
            SourceRootId::GitWorktree { .. }
                if operation
                    .paths()
                    .iter()
                    .any(|path| !matches!(path, SourcePath::Git { .. })) =>
            {
                return Err(io_refusal_context(
                    "validate commit journal",
                    "Git journal has directory path",
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_receipt(
    root: &Path,
    receipt: &CommitReceipt,
    stage_id: StageId,
    expected_root: &SourceRootId,
) -> std::result::Result<(), CommitRefusal> {
    if receipt.stage_id != stage_id || &receipt.root != expected_root {
        return Err(CommitRefusal::ReceiptDiverged {
            stage_id,
            path: SourcePath::Directory {
                path: crate::RootPath(std::sync::Arc::from(".")),
            },
            reason: "receipt identity does not match requested stage".into(),
        });
    }
    for operation in &receipt.operations {
        for candidate in operation
            .path_before
            .iter()
            .chain(operation.path_after.iter())
        {
            let compatible = match expected_root {
                SourceRootId::Directory { .. } => matches!(candidate, SourcePath::Directory { .. }),
                SourceRootId::GitWorktree { .. } => matches!(candidate, SourcePath::Git { .. }),
            };
            if !compatible {
                return Err(CommitRefusal::ReceiptDiverged {
                    stage_id,
                    path: candidate.clone(),
                    reason: "receipt path variant does not match root".into(),
                });
            }
        }
        let path = match operation.kind {
            PlannedFileKind::Delete => operation.path_before.as_ref(),
            _ => operation.path_after.as_ref(),
        }
        .ok_or_else(|| CommitRefusal::ReceiptDiverged {
            stage_id,
            path: operation
                .path_before
                .clone()
                .or_else(|| operation.path_after.clone())
                .unwrap_or(SourcePath::Directory {
                    path: crate::RootPath(std::sync::Arc::from(".")),
                }),
            reason: "receipt operation has no path".into(),
        })?;
        validate_relative_path(path)?;
        check_parent_components(root, path)?;
        let target = target_path(root, path);
        match operation.kind {
            PlannedFileKind::Delete => {
                if fs::symlink_metadata(&target).is_ok() {
                    return Err(CommitRefusal::ReceiptDiverged {
                        stage_id,
                        path: path.clone(),
                        reason: "deleted path is present".into(),
                    });
                }
            }
            _ => {
                let metadata = fs::symlink_metadata(&target).map_err(|error| {
                    CommitRefusal::ReceiptDiverged {
                        stage_id,
                        path: path.clone(),
                        reason: error.to_string(),
                    }
                })?;
                if !metadata.file_type().is_file()
                    || !content_matches(root, path, operation.after.as_ref())?
                {
                    return Err(CommitRefusal::ReceiptDiverged {
                        stage_id,
                        path: path.clone(),
                        reason: "post-commit identity does not match receipt".into(),
                    });
                }
                if operation.kind == PlannedFileKind::Move {
                    if let Some(source) = &operation.path_before {
                        if fs::symlink_metadata(target_path(root, source)).is_ok() {
                            return Err(CommitRefusal::ReceiptDiverged {
                                stage_id,
                                path: source.clone(),
                                reason: "moved source path is still present".into(),
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn write_receipt(
    path: &Path,
    receipt: &CommitReceipt,
    durability: Durability,
    meter: &mut SyncMeter,
) -> std::result::Result<(), CommitRefusal> {
    let bytes = serde_json::to_vec(receipt)
        .map_err(|error| io_refusal_context("encode commit receipt", error))?;
    if durability.rehearsal() {
        write_unsynced(path, &bytes, None, "write commit receipt")?;
    } else {
        let mut file = AtomicWriteFile::open(path)
            .map_err(|error| io_refusal_context("open commit receipt", error))?;
        file.write_all(&bytes)
            .map_err(|error| io_refusal_context("write commit receipt", error))?;
        meter
            .measure(|| file.commit())
            .map_err(|error| io_refusal_context("publish commit receipt", error))?;
    }
    sync_dir_when_durable(
        path.parent().unwrap_or_else(|| Path::new(".")),
        durability,
        meter,
    )
}

fn sync_dir(path: &Path) -> std::result::Result<(), CommitRefusal> {
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|error| io_refusal_context("sync commit directory", error))
}

/// A rehearsal mirror is discarded whole, so a partly written file can never
/// be read by anything but the same process. The temporary-and-rename dance
/// buys nothing there and costs one device flush per file.
fn write_unsynced(
    path: &Path,
    bytes: &[u8],
    mode: Option<&FileModeObservation>,
    operation: &'static str,
) -> std::result::Result<(), CommitRefusal> {
    fs::write(path, bytes).map_err(|error| io_refusal_context(operation, error))?;
    let Some(mode) = mode else {
        return Ok(());
    };
    let file = File::open(path).map_err(|error| io_refusal_context(operation, error))?;
    set_mode(&file, mode).map_err(|error| io_refusal_context("set target mode", error))
}

fn sync_dir_when_durable(
    path: &Path,
    durability: Durability,
    meter: &mut SyncMeter,
) -> std::result::Result<(), CommitRefusal> {
    if durability.rehearsal() {
        return Ok(());
    }
    meter.measure(|| sync_dir(path))
}

fn remove_journal(
    path: &Path,
    durability: Durability,
    meter: &mut SyncMeter,
) -> std::result::Result<(), CommitRefusal> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_refusal_context("remove commit journal", error)),
    }?;
    sync_dir_when_durable(
        path.parent().unwrap_or_else(|| Path::new(".")),
        durability,
        meter,
    )
}
fn receipt(
    stage_id: StageId,
    root: &SourceRootId,
    operations: &[JournalOperation],
    journal_bytes: u64,
    checkpoint_bytes: u64,
) -> CommitReceipt {
    let receipts = operations
        .iter()
        .map(|operation| CommitOperationReceipt {
            kind: operation.kind,
            path_before: operation.before_path.clone(),
            path_after: operation.after_path.clone(),
            before: operation.expected.clone(),
            after: operation.after_content.clone(),
        })
        .collect::<Vec<_>>();
    let watch = watch_projection(stage_id, &receipts);
    CommitReceipt {
        stage_id,
        root: root.clone(),
        applied_files: operations.len(),
        operations: receipts,
        journal_bytes,
        checkpoint_bytes,
        watch,
    }
}

fn stage_operation_receipts(files: &[StagedFile]) -> Vec<CommitOperationReceipt> {
    files
        .iter()
        .map(|file| CommitOperationReceipt {
            kind: file.kind,
            path_before: file.path_before.clone(),
            path_after: file.path_after.clone(),
            before: file.content_before.clone(),
            after: file.content_after.clone(),
        })
        .collect()
}

fn watch_projection(
    stage_id: StageId,
    operations: &[CommitOperationReceipt],
) -> CommitWatchCorrelation {
    let paths = operations
        .iter()
        .flat_map(|operation| {
            operation
                .path_before
                .iter()
                .chain(operation.path_after.iter())
                .cloned()
        })
        .collect();
    let post = operations
        .iter()
        .flat_map(|operation| match operation.kind {
            PlannedFileKind::Delete => operation
                .path_before
                .iter()
                .map(|path| CommitWatchPost {
                    path: path.clone(),
                    present: false,
                    content: None,
                })
                .collect::<Vec<_>>(),
            _ => operation
                .path_after
                .iter()
                .map(|path| CommitWatchPost {
                    path: path.clone(),
                    present: true,
                    content: operation.after.clone(),
                })
                .collect::<Vec<_>>(),
        })
        .collect();
    CommitWatchCorrelation {
        stage_id,
        paths,
        post,
    }
}
