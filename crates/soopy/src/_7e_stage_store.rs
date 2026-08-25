//! Sealed stage storage and deterministic previews.
//!
//! A stage is a planner result plus content-addressed output blobs. Saving a
//! stage never opens the target root. Durable publication writes and syncs all
//! blobs before atomically publishing the manifest that references them.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use diffy::PatchFormatter;
use serde::{Deserialize, Serialize};

use crate::_0a_durable_write::{elapsed_millis, publish_file, record_sync, SyncLevel, SyncMeter};
use crate::{
    ActionSource, ContentId, FileModeObservation, MutationPlan, NormalizedEdit, PlannedFileKind,
    SourcePath, SourceRootId,
};

pub const STAGE_STORE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StageId(pub [u8; 32]);

impl StageId {
    pub fn to_hex(self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }
}

impl fmt::Display for StageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for StageId {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(
            *blake3::Hash::from_hex(value)
                .map_err(|error| error.to_string())?
                .as_bytes(),
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StagedContentId(pub [u8; 32]);

impl StagedContentId {
    pub fn to_hex(self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }
}

impl fmt::Display for StagedContentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedSourceTransaction {
    pub id: StageId,
    pub root: SourceRootId,
    pub files: Vec<StagedFile>,
    pub previews: Vec<FilePreview>,
}

impl StagedSourceTransaction {
    /// Canonical identity input. Presentation fields such as previews are not
    /// present, so changing display formatting cannot change this byte vector.
    pub fn canonical_manifest_bytes(&self) -> Result<Vec<u8>> {
        identity_bytes(&self.root, &self.files)
    }

    pub fn recompute_id(&self) -> Result<StageId> {
        Ok(StageId(
            *blake3::hash(&self.canonical_manifest_bytes()?).as_bytes(),
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedFile {
    pub kind: PlannedFileKind,
    pub source: Option<ActionSource>,
    pub path_before: Option<SourcePath>,
    pub path_after: Option<SourcePath>,
    pub content_before: Option<ContentId>,
    pub content_after: Option<ContentId>,
    pub mode_before: Option<FileModeObservation>,
    pub staged_bytes: Option<StagedContentId>,
    pub edits: Vec<NormalizedEdit>,
    /// Loaded by a store for callers that need the exact result bytes.
    #[serde(skip)]
    pub bytes_after: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilePreview {
    pub kind: PlannedFileKind,
    pub path_before: Option<SourcePath>,
    pub path_after: Option<SourcePath>,
    pub summary: String,
    pub unified: Option<String>,
    pub binary: bool,
    pub before_bytes: u64,
    pub after_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct StageIdentity {
    schema_version: u32,
    root: SourceRootId,
    files: Vec<IdentityFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct IdentityFile {
    kind: PlannedFileKind,
    source: Option<ActionSource>,
    path_before: Option<SourcePath>,
    path_after: Option<SourcePath>,
    content_before: Option<ContentId>,
    content_after: Option<ContentId>,
    mode_before: Option<FileModeObservation>,
    staged_bytes: Option<StagedContentId>,
    edits: Vec<NormalizedEdit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct StoredManifest {
    schema_version: u32,
    id: StageId,
    identity: Vec<u8>,
    root: SourceRootId,
    files: Vec<StagedFile>,
    previews: Vec<FilePreview>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CleanupPolicy {
    /// Manifests listed here are retained. An empty list retains no manifests
    /// only when the caller explicitly sets `remove_manifests`.
    pub retain: BTreeSet<StageId>,
    pub remove_manifests: bool,
    /// Blob deletion is opt-in. The default leaves unreferenced blobs intact.
    pub remove_unreferenced_blobs: bool,
}

pub trait StageStore {
    fn save(&mut self, plan: MutationPlan) -> Result<StagedSourceTransaction>;
    fn load(&self, id: StageId) -> Result<Option<StagedSourceTransaction>>;
    fn discard(&mut self, id: StageId) -> Result<bool>;
    fn cleanup(&mut self, policy: &CleanupPolicy) -> Result<usize>;
}

pub fn stage_mutations<S: StageStore>(
    root: &mut crate::SourceRoot,
    request: &crate::StageRequest,
    store: &mut S,
) -> Result<StagedSourceTransaction, crate::StageRefusal> {
    let span = tracing::debug_span!(
        "stage.mutations",
        operation = "stage_mutations",
        actions = request.actions.len(),
        files = tracing::field::Empty,
        plan_ms = tracing::field::Empty,
        save_ms = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
    );
    let _entered = span.enter();
    let started = Instant::now();
    let plan = crate::plan_mutations(root, request)?;
    let plan_ms = elapsed_millis(started);
    let files = plan.files.len();
    let save_started = Instant::now();
    let staged = store
        .save(plan)
        .map_err(|error| crate::StageRefusal::Store {
            detail: error.to_string(),
        })?;
    let save_ms = elapsed_millis(save_started);
    let duration_ms = elapsed_millis(started);
    span.record("files", files);
    span.record("plan_ms", plan_ms);
    span.record("save_ms", save_ms);
    span.record("duration_ms", duration_ms);
    tracing::debug!(
        files,
        plan_ms,
        save_ms,
        duration_ms,
        "stage mutations sealed"
    );
    Ok(staged)
}

pub fn show_stage<S: StageStore>(
    store: &S,
    id: StageId,
) -> Result<Option<StagedSourceTransaction>> {
    store.load(id)
}

pub fn discard_stage<S: StageStore>(store: &mut S, id: StageId) -> Result<bool> {
    store.discard(id)
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryStageStore {
    stages: BTreeMap<StageId, StagedSourceTransaction>,
    blobs: BTreeMap<StagedContentId, Vec<u8>>,
}

impl InMemoryStageStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }
    pub fn blob_count(&self) -> usize {
        self.blobs.len()
    }
    pub fn blob_bytes(&self, id: StagedContentId) -> Option<&[u8]> {
        self.blobs.get(&id).map(Vec::as_slice)
    }
}

impl StageStore for InMemoryStageStore {
    fn save(&mut self, plan: MutationPlan) -> Result<StagedSourceTransaction> {
        let (transaction, blobs) = seal_plan(plan)?;
        for (id, bytes) in blobs {
            self.blobs.entry(id).or_insert(bytes);
        }
        self.stages.insert(transaction.id, transaction.clone());
        Ok(transaction)
    }

    fn load(&self, id: StageId) -> Result<Option<StagedSourceTransaction>> {
        let Some(mut stage) = self.stages.get(&id).cloned() else {
            return Ok(None);
        };
        for file in &mut stage.files {
            if let Some(blob) = file.staged_bytes {
                file.bytes_after = Some(
                    self.blobs
                        .get(&blob)
                        .context("missing staged blob")?
                        .clone(),
                );
            }
        }
        Ok(Some(stage))
    }

    fn discard(&mut self, id: StageId) -> Result<bool> {
        Ok(self.stages.remove(&id).is_some())
    }

    fn cleanup(&mut self, policy: &CleanupPolicy) -> Result<usize> {
        let mut removed = 0;
        if policy.remove_manifests {
            let ids: Vec<_> = self
                .stages
                .keys()
                .copied()
                .filter(|id| !policy.retain.contains(id))
                .collect();
            for id in ids {
                self.stages.remove(&id);
                removed += 1;
            }
        }
        if policy.remove_unreferenced_blobs {
            let referenced: BTreeSet<_> = self
                .stages
                .values()
                .flat_map(|stage| stage.files.iter().filter_map(|file| file.staged_bytes))
                .collect();
            let ids: Vec<_> = self
                .blobs
                .keys()
                .copied()
                .filter(|id| !referenced.contains(id))
                .collect();
            for id in ids {
                self.blobs.remove(&id);
                removed += 1;
            }
        }
        Ok(removed)
    }
}

#[derive(Clone, Debug)]
pub struct DurableStageStore {
    root: PathBuf,
}

impl DurableStageStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self> {
        Self::open(root)
    }
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("blobs"))?;
        fs::create_dir_all(root.join("manifests"))?;
        Ok(Self { root })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn blob_count(&self) -> Result<usize> {
        Ok(fs::read_dir(self.root.join("blobs"))?
            .filter_map(std::result::Result::ok)
            .filter(|entry| {
                entry
                    .file_type()
                    .map(|kind| kind.is_file())
                    .unwrap_or(false)
            })
            .count())
    }
    pub fn blob_bytes(&self) -> Result<usize> {
        Ok(fs::read_dir(self.root.join("blobs"))?
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| {
                entry
                    .file_type()
                    .ok()
                    .filter(|kind| kind.is_file())
                    .map(|_| entry)
            })
            .filter_map(|entry| entry.metadata().ok())
            .map(|metadata| usize::try_from(metadata.len()).unwrap_or(usize::MAX))
            .sum())
    }
    fn blob_path(&self, id: StagedContentId) -> PathBuf {
        self.root.join("blobs").join(id.to_hex())
    }
    fn manifest_path(&self, id: StageId) -> PathBuf {
        self.root.join("manifests").join(format!("{}.json", id))
    }
    fn blobs_dir(&self) -> PathBuf {
        self.root.join("blobs")
    }
    fn manifests_dir(&self) -> PathBuf {
        self.root.join("manifests")
    }
}

impl StageStore for DurableStageStore {
    fn save(&mut self, plan: MutationPlan) -> Result<StagedSourceTransaction> {
        let seal_started = Instant::now();
        let seal_span = tracing::debug_span!(
            "stage.seal",
            operation = "seal_plan",
            files = plan.files.len(),
        );
        let (transaction, blobs) = seal_span.in_scope(|| seal_plan(plan))?;
        tracing::debug!(
            files = transaction.files.len(),
            duration_ms = elapsed_millis(seal_started),
            "stage plan sealed"
        );

        let blobs_started = Instant::now();
        let blob_count = blobs.len();
        let blob_span = tracing::debug_span!(
            "stage.write_blobs",
            operation = "write_staged_blobs",
            files = blob_count,
            sync.data = tracing::field::Empty,
            sync.fences = tracing::field::Empty,
            sync.flushes = tracing::field::Empty,
            sync_ms = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
        );
        let blob_entered = blob_span.enter();
        let mut blob_sync = SyncMeter::default();
        let mut published = false;
        for (id, bytes) in blobs {
            let target = self.blob_path(id);
            if target.exists() && blob_matches(&target, id)? {
                continue;
            }
            publish_file(&target, &bytes, SyncLevel::Data, &mut blob_sync, |_| Ok(()))
                .with_context(|| format!("create staged blob {}", target.display()))?;
            published = true;
        }
        // The fence is what stops a manifest from reaching the device ahead of
        // the blobs it names.
        if published {
            blob_sync.directory(&self.blobs_dir(), SyncLevel::Fence)?;
        }
        record_sync(&blob_span, blob_sync, blobs_started);
        tracing::debug!(
            files = blob_count,
            sync.data = blob_sync.data(),
            sync.fences = blob_sync.fences(),
            sync.flushes = blob_sync.flushes(),
            sync_ms = blob_sync.millis(),
            duration_ms = elapsed_millis(blobs_started),
            "staged blobs published"
        );
        drop(blob_entered);

        let manifest_started = Instant::now();
        let manifest_span = tracing::debug_span!(
            "stage.write_manifest",
            operation = "write_stage_manifest",
            files = transaction.files.len(),
            sync.data = tracing::field::Empty,
            sync.fences = tracing::field::Empty,
            sync.flushes = tracing::field::Empty,
            sync_ms = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
        );
        let manifest_entered = manifest_span.enter();
        let identity = identity_bytes(&transaction.root, &transaction.files)?;
        let manifest = StoredManifest {
            schema_version: STAGE_STORE_SCHEMA_VERSION,
            id: transaction.id,
            identity,
            root: transaction.root.clone(),
            files: transaction.files.clone(),
            previews: transaction.previews.clone(),
        };
        let bytes = serde_json::to_vec(&manifest)?;
        let target = self.manifest_path(transaction.id);
        let mut manifest_sync = SyncMeter::default();
        publish_file(&target, &bytes, SyncLevel::Data, &mut manifest_sync, |_| Ok(()))?;
        // The one flush a save pays: it settles every blob fenced before it.
        manifest_sync.directory(&self.manifests_dir(), SyncLevel::Flush)?;
        record_sync(&manifest_span, manifest_sync, manifest_started);
        tracing::debug!(
            manifest.bytes = bytes.len(),
            sync.data = manifest_sync.data(),
            sync.fences = manifest_sync.fences(),
            sync.flushes = manifest_sync.flushes(),
            sync_ms = manifest_sync.millis(),
            duration_ms = elapsed_millis(manifest_started),
            "stage manifest published"
        );
        drop(manifest_entered);
        Ok(transaction)
    }

    fn load(&self, id: StageId) -> Result<Option<StagedSourceTransaction>> {
        let started = Instant::now();
        let span = tracing::debug_span!(
            "stage.load",
            operation = "load_stage",
            files = tracing::field::Empty,
            duration_ms = tracing::field::Empty,
        );
        let _entered = span.enter();
        let path = self.manifest_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path)?;
        let manifest: StoredManifest =
            serde_json::from_slice(&bytes).context("decode stage manifest")?;
        if manifest.schema_version != STAGE_STORE_SCHEMA_VERSION || manifest.id != id {
            bail!("invalid stage manifest identity")
        }
        if blake3::hash(&manifest.identity) != blake3::Hash::from_bytes(id.0) {
            bail!("stage manifest digest mismatch")
        }
        let expected = identity_bytes(&manifest.root, &manifest.files)?;
        if expected != manifest.identity {
            bail!("stage manifest identity bytes mismatch")
        }
        let mut stage = StagedSourceTransaction {
            id,
            root: manifest.root,
            files: manifest.files,
            previews: manifest.previews,
        };
        for file in &mut stage.files {
            if let Some(blob) = file.staged_bytes {
                let bytes = fs::read(self.blob_path(blob))
                    .with_context(|| format!("read staged blob {blob}"))?;
                if blake3::hash(&bytes) != blake3::Hash::from_bytes(blob.0) {
                    bail!("staged blob digest mismatch")
                }
                file.bytes_after = Some(bytes);
            }
        }
        span.record("files", stage.files.len());
        span.record("duration_ms", elapsed_millis(started));
        tracing::debug!(
            files = stage.files.len(),
            duration_ms = elapsed_millis(started),
            "stage rehydrated"
        );
        Ok(Some(stage))
    }

    fn discard(&mut self, id: StageId) -> Result<bool> {
        let path = self.manifest_path(id);
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(path)?;
        SyncMeter::default().directory(&self.manifests_dir(), SyncLevel::Flush)?;
        Ok(true)
    }

    fn cleanup(&mut self, policy: &CleanupPolicy) -> Result<usize> {
        let mut removed = 0;
        if policy.remove_manifests {
            for entry in fs::read_dir(self.root.join("manifests"))? {
                let entry = entry?;
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                let Some(hex_id) = name.strip_suffix(".json") else {
                    continue;
                };
                let Ok(id) = StageId::from_str(hex_id) else {
                    continue;
                };
                if !policy.retain.contains(&id) {
                    fs::remove_file(entry.path())?;
                    removed += 1;
                }
            }
            SyncMeter::default().directory(&self.manifests_dir(), SyncLevel::Flush)?;
        }
        if policy.remove_unreferenced_blobs {
            let mut referenced = BTreeSet::new();
            for entry in fs::read_dir(self.root.join("manifests"))? {
                let entry = entry?;
                let manifest: StoredManifest = serde_json::from_slice(&fs::read(entry.path())?)?;
                referenced.extend(manifest.files.iter().filter_map(|file| file.staged_bytes));
            }
            for entry in fs::read_dir(self.root.join("blobs"))? {
                let entry = entry?;
                let file_name = entry.file_name();
                let Some(name) = file_name.to_str() else {
                    continue;
                };
                let Ok(id) = StagedContentId::from_str(name) else {
                    continue;
                };
                if !referenced.contains(&id) {
                    fs::remove_file(entry.path())?;
                    removed += 1;
                }
            }
            SyncMeter::default().directory(&self.blobs_dir(), SyncLevel::Flush)?;
        }
        Ok(removed)
    }
}

type SealedPlan = (StagedSourceTransaction, Vec<(StagedContentId, Vec<u8>)>);

fn seal_plan(plan: MutationPlan) -> Result<SealedPlan> {
    let mut files = Vec::with_capacity(plan.files.len());
    let mut blobs = Vec::new();
    let mut previews = Vec::with_capacity(plan.files.len());
    for mut file in plan.files {
        let before = file.bytes_before.as_deref();
        let after = file.bytes_after.as_deref();
        previews.push(preview(
            file.kind,
            file.path_before.clone(),
            file.path_after.clone(),
            before,
            after,
        )?);
        let staged_bytes = file.bytes_after.take().map(|bytes| {
            let id = StagedContentId(*blake3::hash(&bytes).as_bytes());
            blobs.push((id, bytes));
            id
        });
        files.push(StagedFile {
            kind: file.kind,
            source: file.source.clone(),
            path_before: file.path_before.clone(),
            path_after: file.path_after.clone(),
            content_before: file.content_before.clone(),
            content_after: file.content_after.clone(),
            mode_before: file.mode_before.clone(),
            staged_bytes,
            edits: file.edits.clone(),
            bytes_after: None,
        });
    }
    let mut pairs: Vec<_> = files.into_iter().zip(previews).collect();
    pairs.sort_by_key(|(file, _)| (file.path_after.clone(), file.path_before.clone(), file.kind));
    let (files, previews): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
    let identity = identity_bytes(&plan.root, &files)?;
    let id = StageId(*blake3::hash(&identity).as_bytes());
    Ok((
        StagedSourceTransaction {
            id,
            root: plan.root.clone(),
            files,
            previews,
        },
        blobs,
    ))
}

fn blob_matches(path: &Path, id: StagedContentId) -> Result<bool> {
    let bytes =
        fs::read(path).with_context(|| format!("read existing staged blob {}", path.display()))?;
    Ok(blake3::hash(&bytes) == blake3::Hash::from_bytes(id.0))
}

fn identity_bytes(root: &SourceRootId, files: &[StagedFile]) -> Result<Vec<u8>> {
    let mut identity_files = files
        .iter()
        .map(|file| IdentityFile {
            kind: file.kind,
            source: file.source.clone(),
            path_before: file.path_before.clone(),
            path_after: file.path_after.clone(),
            content_before: file.content_before.clone(),
            content_after: file.content_after.clone(),
            mode_before: file.mode_before.clone(),
            staged_bytes: file.staged_bytes,
            edits: file.edits.clone(),
        })
        .collect::<Vec<_>>();
    identity_files
        .sort_by_key(|file| (file.path_after.clone(), file.path_before.clone(), file.kind));
    Ok(serde_json::to_vec(&StageIdentity {
        schema_version: STAGE_STORE_SCHEMA_VERSION,
        root: root.clone(),
        files: identity_files,
    })?)
}

fn preview(
    kind: PlannedFileKind,
    path_before: Option<SourcePath>,
    path_after: Option<SourcePath>,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
) -> Result<FilePreview> {
    let before = before.unwrap_or_default();
    let after = after.unwrap_or_default();
    let binary = std::str::from_utf8(before).is_err() || std::str::from_utf8(after).is_err();
    let unified = if binary {
        None
    } else {
        let patch = diffy::create_patch_bytes(before, after);
        let mut unified_bytes = Vec::new();
        PatchFormatter::new().write_patch_into(&patch, &mut unified_bytes)?;
        Some(String::from_utf8_lossy(&unified_bytes).into_owned())
    };
    let after_bytes = u64::try_from(after.len()).unwrap_or(u64::MAX);
    let summary = match (&path_before, &path_after, kind) {
        (_, Some(path), PlannedFileKind::Create) => {
            format!("create {} ({} bytes)", path_text(path), after.len())
        }
        (Some(path), _, PlannedFileKind::Delete) => format!("delete {}", path_text(path)),
        (Some(from), Some(to), PlannedFileKind::Move) => format!(
            "move {} -> {} ({} bytes)",
            path_text(from),
            path_text(to),
            after.len()
        ),
        (_, Some(path), _) => format!("update {} ({} bytes)", path_text(path), after.len()),
        _ => "empty staged operation".to_owned(),
    };
    Ok(FilePreview {
        kind,
        path_before,
        path_after,
        summary,
        unified,
        binary,
        before_bytes: before.len() as u64,
        after_bytes,
    })
}

fn path_text(path: &SourcePath) -> &str {
    match path {
        SourcePath::Directory { path } => path.0.as_ref(),
        SourcePath::Git { path } => path.0.as_ref(),
    }
}

impl FromStr for StagedContentId {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(
            *blake3::Hash::from_hex(value)
                .map_err(|error| error.to_string())?
                .as_bytes(),
        ))
    }
}
