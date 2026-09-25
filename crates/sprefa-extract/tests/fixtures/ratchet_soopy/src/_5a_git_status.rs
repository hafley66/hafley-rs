//! Per-path Git status without an aggregate dirty bit.
//!
//! The implementation keeps the two adjacent comparisons separate:
//! `HEAD -> index` is staged state and `index -> worktree` is unstaged state.
//! A path where HEAD and worktree happen to match can therefore still be both
//! staged and unstaged.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use crate::{
    ContentId, EntryIdentity, EntryTransition, GitEntryKind, GitEntryMode, GitFileQuery,
    IndexStageEntry, ObjectId, RepoPath, Repository, TrackedFileObservation, TrackedFileState,
    TrackedFileUnsupported, TrackedHeadState, TrackedStateMetrics, TrackedStateResult,
    UntrackedFilePolicy,
};
use anyhow::{bail, Context, Result};

/// Repository-owned cache. Its key includes both immutable comparison sides
/// and current worktree metadata, so an index-only or HEAD-only change cannot
/// reuse a comparison made for a previous source state.
#[derive(Default)]
pub(crate) struct GitStatusCache {
    entries: BTreeMap<RepoPath, CachedWorktree>,
    hasher: Option<GitHashObjectBatch>,
    byte_hasher: Option<GitHashObjectBytes>,
}

struct CachedWorktree {
    head: Option<EntryIdentity>,
    index: Option<EntryIdentity>,
    metadata: Option<WorktreeMetadata>,
    entry: Option<EntryIdentity>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WorktreeMetadata {
    modified: Option<SystemTime>,
    size: u64,
    kind: GitEntryKind,
    mode: GitEntryMode,
}

#[derive(Default)]
struct PathRows {
    head: Option<EntryIdentity>,
    stages: Vec<IndexStageEntry>,
    skip_worktree: bool,
    intent_to_add: bool,
    untracked: bool,
}

/// A persistent, one-request-at-a-time `git hash-object --stdin-paths`
/// process. It bounds child-process count to one per open Git root and keeps
/// the response and request buffers reusable across every status observation.
struct GitHashObjectBatch {
    child: Child,
    input: ChildStdin,
    output: BufReader<std::process::ChildStdout>,
    line: String,
}

impl GitHashObjectBatch {
    fn open(root: &Path, metrics: &mut TrackedStateMetrics) -> Result<Self> {
        metrics.git_child_processes += 1;
        metrics.hash_worker_launches += 1;
        let mut child = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["hash-object", "--stdin-paths"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("start git hash-object --stdin-paths")?;
        Ok(Self {
            input: child.stdin.take().context("open hash-object stdin")?,
            output: BufReader::new(child.stdout.take().context("open hash-object stdout")?),
            child,
            line: String::new(),
        })
    }

    fn hash_path(&mut self, path: &str) -> Result<ObjectId> {
        writeln!(self.input, "{path}").context("write hash-object path")?;
        self.input.flush().context("flush hash-object path")?;
        self.line.clear();
        let bytes = self
            .output
            .read_line(&mut self.line)
            .context("read hash-object response")?;
        if bytes == 0 {
            bail!("git hash-object closed before answering {path:?}");
        }
        let oid = self.line.trim();
        if oid.is_empty() || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("invalid git hash-object response for {path:?}: {oid:?}");
        }
        Ok(ObjectId(Arc::from(oid)))
    }
}

impl Drop for GitHashObjectBatch {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Repository-owned byte worker for symlink text. `git hash-object --stdin`
/// consumes one byte stream and exits at EOF, so this worker retains the root
/// and reusable byte buffer while starting Git only for a cache miss. Git owns
/// the object-format algorithm for SHA-1 and SHA-256 repositories.
struct GitHashObjectBytes {
    root: std::path::PathBuf,
    bytes: Vec<u8>,
}

impl GitHashObjectBytes {
    fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            bytes: Vec::new(),
        }
    }

    fn hash_symlink(
        &mut self,
        target: &Path,
        metrics: &mut TrackedStateMetrics,
    ) -> Result<ObjectId> {
        self.bytes.clear();
        append_path_bytes(&mut self.bytes, target);
        metrics.git_child_processes += 1;
        metrics.byte_worker_launches += 1;
        metrics.bytes_hashed += self.bytes.len() as u64;
        let mut child = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["hash-object", "--stdin", "--no-filters"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .context("start git hash-object --stdin for symlink")?;
        child
            .stdin
            .as_mut()
            .context("open symlink hash stdin")?
            .write_all(&self.bytes)
            .context("write symlink target bytes")?;
        let output = child
            .wait_with_output()
            .context("wait for git hash-object --stdin")?;
        if !output.status.success() {
            bail!("git hash-object --stdin failed for symlink");
        }
        let oid = String::from_utf8(output.stdout)?;
        let oid = oid.trim();
        if oid.is_empty() || !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("invalid Git object identity from symlink hash: {oid:?}");
        }
        Ok(ObjectId(Arc::from(oid)))
    }
}

pub(crate) fn tracked_state_with_metrics(
    repository: &Repository,
    query: &GitFileQuery,
    cache: &mut GitStatusCache,
) -> Result<TrackedStateResult> {
    let started = Instant::now();
    let span = tracing::info_span!(
        "git.tracked_state",
        operation = "tracked_state",
        repo.path = %repository.root.display(),
        items.observed = tracing::field::Empty,
        process.spawned = tracing::field::Empty,
        cache.hits = tracing::field::Empty,
        cache.misses = tracing::field::Empty,
        duration_ms = tracing::field::Empty,
    );
    let _entered = span.enter();
    let mut metrics = TrackedStateMetrics::default();
    let head_state = head_state(repository, &mut metrics)?;
    let mut rows = BTreeMap::<String, PathRows>::new();
    if let TrackedHeadState::Present(head) = &head_state {
        for (path, entry) in head_entries(repository, head, &query.pathspecs, &mut metrics)? {
            rows.entry(path).or_default().head = Some(entry);
        }
    }
    for (path, stage) in index_entries(repository, &query.pathspecs, &mut metrics)? {
        rows.entry(path).or_default().stages.push(stage);
    }
    for path in skip_worktree_paths(repository, &query.pathspecs, &mut metrics)? {
        rows.entry(path).or_default().skip_worktree = true;
    }
    for path in intent_to_add_paths(repository, &query.pathspecs, &mut metrics)? {
        rows.entry(path).or_default().intent_to_add = true;
    }
    if matches!(query.untracked, UntrackedFilePolicy::Include) {
        for path in untracked_paths(repository, &query.pathspecs, &mut metrics)? {
            rows.entry(path).or_default().untracked = true;
        }
    }

    let gitlinks = rows
        .iter()
        .filter_map(|(path, row)| row.gitlink().then_some(path.as_str()))
        .collect::<Vec<_>>();
    let gitlink_worktrees = (!gitlinks.is_empty())
        .then(|| gitlink_worktrees(repository, &mut metrics))
        .transpose()?;

    let mut observed = Vec::with_capacity(rows.len());
    for (path, mut row) in rows {
        row.stages.sort_by_key(|stage| stage.stage);
        let path = repo_path(&path)?;
        let index = row.stage_zero();
        let intent_to_add = row.intent_to_add;
        let unmerged = !row.stages.is_empty() && index.is_none();
        let worktree = if row.skip_worktree {
            Ok(None)
        } else if row.gitlink() {
            gitlink_worktrees
                .as_ref()
                .and_then(|map| map.get(path.0.as_ref()))
                .cloned()
                .map(Some)
                .ok_or_else(|| anyhow::anyhow!("gitlink worktree unavailable"))
        } else {
            worktree_identity(
                repository,
                cache,
                &path,
                row.head.as_ref(),
                index.as_ref(),
                &mut metrics,
            )
        };

        let worktree = match worktree {
            Ok(entry) => entry,
            Err(error) if row.gitlink() => {
                observed.push(exception_observation(
                    repository,
                    path,
                    &head_state,
                    row,
                    None,
                    TrackedFileState::Unsupported(
                        TrackedFileUnsupported::GitlinkWorktreeUnavailable,
                    ),
                ));
                let _ = error;
                continue;
            }
            Err(error) => {
                observed.push(exception_observation(
                    repository,
                    path,
                    &head_state,
                    row,
                    None,
                    TrackedFileState::Unsupported(
                        TrackedFileUnsupported::WorktreeContentUnavailable,
                    ),
                ));
                let _ = error;
                continue;
            }
        };

        if unmerged {
            observed.push(exception_observation(
                repository,
                path,
                &head_state,
                row,
                worktree,
                TrackedFileState::Unmerged,
            ));
            continue;
        }
        if intent_to_add {
            observed.push(exception_observation(
                repository,
                path,
                &head_state,
                row,
                worktree,
                TrackedFileState::IntentToAdd,
            ));
            continue;
        }
        if row.skip_worktree {
            observed.push(exception_observation(
                repository,
                path,
                &head_state,
                row,
                worktree,
                TrackedFileState::Sparse,
            ));
            continue;
        }
        if row.untracked && row.head.is_none() && index.is_none() {
            observed.push(TrackedFileObservation {
                worktree: repository.worktree.clone(),
                path,
                state: TrackedFileState::Untracked,
                head_state: head_state.clone(),
                head: None,
                index: None,
                worktree_entry: worktree,
                index_stages: Vec::new(),
                staged_change: None,
                unstaged_change: None,
                head_to_index: None,
                index_to_worktree: None,
            });
            continue;
        }

        let head_to_index = transition(row.head.as_ref(), index.as_ref());
        let index_to_worktree = transition(index.as_ref(), worktree.as_ref());
        let staged_change = Some(!matches!(head_to_index, EntryTransition::Unchanged));
        let unstaged_change = Some(!matches!(index_to_worktree, EntryTransition::Unchanged));
        let state = match (staged_change, unstaged_change) {
            (Some(false), Some(false)) => TrackedFileState::Clean,
            (Some(false), Some(true)) => TrackedFileState::Unstaged,
            (Some(true), Some(false)) => TrackedFileState::Staged,
            (Some(true), Some(true)) => TrackedFileState::StagedAndUnstaged,
            _ => unreachable!("comparable tracked rows always have both transitions"),
        };
        observed.push(TrackedFileObservation {
            worktree: repository.worktree.clone(),
            path,
            state,
            head_state: head_state.clone(),
            head: row.head,
            index,
            worktree_entry: worktree,
            index_stages: row.stages,
            staged_change,
            unstaged_change,
            head_to_index: Some(head_to_index),
            index_to_worktree: Some(index_to_worktree),
        });
    }
    span.record("items.observed", observed.len() as u64);
    span.record("process.spawned", metrics.git_child_processes);
    span.record("cache.hits", metrics.worktree_cache_hits);
    span.record("cache.misses", metrics.worktree_cache_misses);
    span.record("duration_ms", started.elapsed().as_secs_f64() * 1_000.0);
    tracing::debug!(
        git.child_processes = metrics.git_child_processes,
        git.hash_worker_launches = metrics.hash_worker_launches,
        git.byte_worker_launches = metrics.byte_worker_launches,
        bytes.hashed = metrics.bytes_hashed,
        cache.hits = metrics.worktree_cache_hits,
        cache.misses = metrics.worktree_cache_misses,
        observations = observed.len(),
        "tracked state completed"
    );
    Ok(TrackedStateResult {
        observations: observed,
        metrics,
    })
}

impl PathRows {
    fn stage_zero(&self) -> Option<EntryIdentity> {
        self.stages
            .iter()
            .find(|stage| stage.stage == 0)
            .map(|stage| stage.identity.clone())
    }

    fn gitlink(&self) -> bool {
        self.head
            .as_ref()
            .is_some_and(|entry| entry.kind == GitEntryKind::Gitlink)
            || self
                .stages
                .iter()
                .any(|entry| entry.identity.kind == GitEntryKind::Gitlink)
    }
}

fn exception_observation(
    repository: &Repository,
    path: RepoPath,
    head_state: &TrackedHeadState,
    row: PathRows,
    worktree_entry: Option<EntryIdentity>,
    state: TrackedFileState,
) -> TrackedFileObservation {
    let index = row.stage_zero();
    TrackedFileObservation {
        worktree: repository.worktree.clone(),
        path,
        state,
        head_state: head_state.clone(),
        head: row.head,
        index,
        worktree_entry,
        index_stages: row.stages,
        staged_change: None,
        unstaged_change: None,
        head_to_index: None,
        index_to_worktree: None,
    }
}

fn head_state(
    repository: &Repository,
    metrics: &mut TrackedStateMetrics,
) -> Result<TrackedHeadState> {
    let output = git_output(
        repository,
        ["rev-parse", "--verify", "-q", "HEAD^{commit}"],
        &[],
        metrics,
    )?;
    if output.status.success() {
        return Ok(TrackedHeadState::Present(ObjectId(Arc::from(
            String::from_utf8(output.stdout)?.trim(),
        ))));
    }
    Ok(TrackedHeadState::Unborn)
}

fn head_entries(
    repository: &Repository,
    head: &ObjectId,
    pathspecs: &[String],
    metrics: &mut TrackedStateMetrics,
) -> Result<Vec<(String, EntryIdentity)>> {
    let output = git_output(
        repository,
        ["ls-tree", "-r", "-z", head.0.as_ref()],
        pathspecs,
        metrics,
    )?;
    if !output.status.success() {
        bail!("git ls-tree failed for {}", head.0);
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .map(|record| {
            let tab = record
                .iter()
                .position(|byte| *byte == b'\t')
                .context("parse ls-tree path")?;
            let fields = std::str::from_utf8(&record[..tab])
                .context("parse ls-tree header")?
                .split_whitespace()
                .collect::<Vec<_>>();
            if fields.len() != 3 {
                bail!("unexpected git ls-tree header")
            }
            Ok((
                checked_path(&record[tab + 1..])?,
                identity(fields[0], fields[1], fields[2])?,
            ))
        })
        .collect()
}

fn index_entries(
    repository: &Repository,
    pathspecs: &[String],
    metrics: &mut TrackedStateMetrics,
) -> Result<Vec<(String, IndexStageEntry)>> {
    let output = git_output(
        repository,
        ["ls-files", "--stage", "-z"],
        pathspecs,
        metrics,
    )?;
    if !output.status.success() {
        bail!(
            "git ls-files --stage failed in {}",
            repository.root.display()
        );
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .map(|record| {
            let tab = record
                .iter()
                .position(|byte| *byte == b'\t')
                .context("parse index path")?;
            let fields = std::str::from_utf8(&record[..tab])
                .context("parse index header")?
                .split_whitespace()
                .collect::<Vec<_>>();
            if fields.len() != 3 {
                bail!("unexpected git ls-files --stage header")
            }
            Ok((
                checked_path(&record[tab + 1..])?,
                IndexStageEntry {
                    stage: fields[2].parse().context("parse index stage")?,
                    identity: identity(fields[0], mode_kind(fields[0]), fields[1])?,
                },
            ))
        })
        .collect()
}

fn skip_worktree_paths(
    repository: &Repository,
    pathspecs: &[String],
    metrics: &mut TrackedStateMetrics,
) -> Result<BTreeSet<String>> {
    let output = git_output(repository, ["ls-files", "-v", "-z"], pathspecs, metrics)?;
    if !output.status.success() {
        bail!("git ls-files -v failed in {}", repository.root.display());
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter_map(|record| record.strip_prefix(b"S "))
        .map(checked_path)
        .collect()
}

fn untracked_paths(
    repository: &Repository,
    pathspecs: &[String],
    metrics: &mut TrackedStateMetrics,
) -> Result<Vec<String>> {
    let output = git_output(
        repository,
        ["ls-files", "--others", "--exclude-standard", "-z"],
        pathspecs,
        metrics,
    )?;
    if !output.status.success() {
        bail!(
            "git ls-files --others failed in {}",
            repository.root.display()
        );
    }
    output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(checked_path)
        .collect()
}

fn intent_to_add_paths(
    repository: &Repository,
    pathspecs: &[String],
    metrics: &mut TrackedStateMetrics,
) -> Result<BTreeSet<String>> {
    let output = git_output(
        repository,
        ["ls-files", "--stage", "--debug"],
        pathspecs,
        metrics,
    )?;
    if !output.status.success() {
        bail!(
            "git ls-files --debug failed in {}",
            repository.root.display()
        );
    }
    let mut current = None;
    let mut paths = BTreeSet::new();
    for line in String::from_utf8(output.stdout)?.lines() {
        if line.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            if let Some((_, path)) = line.split_once('\t') {
                current = Some(path.to_string());
                continue;
            }
        }
        let Some((_, flags)) = line.trim().rsplit_once("flags: ") else {
            continue;
        };
        let flags = u32::from_str_radix(flags, 16).context("parse index debug flags")?;
        if flags & 0x2000_0000 != 0 {
            if let Some(path) = current.as_ref() {
                paths.insert(path.clone());
            }
        }
    }
    Ok(paths)
}

fn gitlink_worktrees(
    repository: &Repository,
    metrics: &mut TrackedStateMetrics,
) -> Result<BTreeMap<String, EntryIdentity>> {
    let output = git_output(
        repository,
        ["submodule", "status", "--recursive"],
        &[],
        metrics,
    )?;
    if !output.status.success() {
        return Ok(BTreeMap::new());
    }
    let mut entries = BTreeMap::new();
    for line in String::from_utf8(output.stdout)?.lines() {
        let Some((oid, rest)) = line.get(1..).and_then(|line| line.split_once(' ')) else {
            continue;
        };
        let path = rest.split(" (").next().unwrap_or(rest);
        if oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            entries.insert(path.to_string(), identity("160000", "commit", oid)?);
        }
    }
    Ok(entries)
}

fn worktree_identity(
    repository: &Repository,
    cache: &mut GitStatusCache,
    path: &RepoPath,
    head: Option<&EntryIdentity>,
    index: Option<&EntryIdentity>,
    metrics: &mut TrackedStateMetrics,
) -> Result<Option<EntryIdentity>> {
    let full_path = repository.root.join(path.0.as_ref());
    let metadata = match std::fs::symlink_metadata(&full_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("stat {}", full_path.display())),
    };
    let metadata = metadata_identity(&metadata)?;
    if let Some(cached) = cache.entries.get(path) {
        if cached.head.as_ref() == head
            && cached.index.as_ref() == index
            && cached.metadata.as_ref() == Some(&metadata)
        {
            metrics.worktree_cache_hits += 1;
            return Ok(cached.entry.clone());
        }
    }
    metrics.worktree_cache_misses += 1;
    let content = match metadata.kind {
        GitEntryKind::File => {
            if cache.hasher.is_none() {
                cache.hasher = Some(GitHashObjectBatch::open(&repository.root, metrics)?);
            }
            cache
                .hasher
                .as_mut()
                .expect("hash worker is initialized")
                .hash_path(path.0.as_ref())?
        }
        // Git's path worker follows symlinks. The repository-owned byte worker
        // hashes exact link text through `git hash-object --stdin`, retaining
        // Git's repository object format without a manual SHA implementation.
        GitEntryKind::Symlink => {
            let target = std::fs::read_link(&full_path)
                .with_context(|| format!("read symlink {}", full_path.display()))?;
            cache
                .byte_hasher
                .get_or_insert_with(|| GitHashObjectBytes::new(&repository.root))
                .hash_symlink(&target, metrics)?
        }
        _ => bail!("worktree type has no normalized Git content"),
    };
    if metadata.kind == GitEntryKind::File {
        metrics.bytes_hashed += metadata.size;
    }
    let entry = EntryIdentity {
        kind: metadata.kind,
        mode: metadata.mode,
        content: ContentId::GitBlob(content),
    };
    cache.entries.insert(
        path.clone(),
        CachedWorktree {
            head: head.cloned(),
            index: index.cloned(),
            metadata: Some(metadata),
            entry: Some(entry.clone()),
        },
    );
    Ok(Some(entry))
}

fn metadata_identity(metadata: &std::fs::Metadata) -> Result<WorktreeMetadata> {
    let file_type = metadata.file_type();
    let (kind, mode) = if file_type.is_symlink() {
        (GitEntryKind::Symlink, GitEntryMode(0o120000))
    } else if file_type.is_file() {
        (
            GitEntryKind::File,
            GitEntryMode(worktree_file_mode(metadata)),
        )
    } else {
        bail!("worktree entry is neither a regular file nor a symlink")
    };
    Ok(WorktreeMetadata {
        modified: metadata.modified().ok(),
        size: metadata.len(),
        kind,
        mode,
    })
}

#[cfg(unix)]
fn append_path_bytes(buffer: &mut Vec<u8>, path: &Path) {
    use std::os::unix::ffi::OsStrExt;
    buffer.extend_from_slice(path.as_os_str().as_bytes());
}

#[cfg(not(unix))]
fn append_path_bytes(buffer: &mut Vec<u8>, path: &Path) {
    buffer.extend_from_slice(path.as_os_str().to_string_lossy().as_bytes());
}

#[cfg(unix)]
fn worktree_file_mode(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o111 != 0 {
        0o100755
    } else {
        0o100644
    }
}

#[cfg(not(unix))]
fn worktree_file_mode(_: &std::fs::Metadata) -> u32 {
    0o100644
}

fn transition(left: Option<&EntryIdentity>, right: Option<&EntryIdentity>) -> EntryTransition {
    match (left, right) {
        (None, None) => EntryTransition::Unchanged,
        (None, Some(_)) => EntryTransition::Added,
        (Some(_), None) => EntryTransition::Deleted,
        (Some(left), Some(right)) if left == right => EntryTransition::Unchanged,
        (Some(left), Some(right)) => {
            let kind = left.kind != right.kind;
            let mode = left.mode != right.mode;
            let content = left.content != right.content;
            match (kind, mode, content) {
                (false, false, true) => EntryTransition::Modified,
                (false, true, false) => EntryTransition::ModeChanged,
                (true, false, false) => EntryTransition::TypeChanged,
                (false, true, true) => EntryTransition::ModeAndContentChanged,
                (true, false, true) => EntryTransition::TypeAndContentChanged,
                (true, true, false) => EntryTransition::TypeAndModeChanged,
                (true, true, true) => EntryTransition::TypeModeAndContentChanged,
                (false, false, false) => unreachable!(),
            }
        }
    }
}

fn identity(mode: &str, kind: &str, oid: &str) -> Result<EntryIdentity> {
    let mode = u32::from_str_radix(mode, 8).with_context(|| format!("parse Git mode {mode:?}"))?;
    if !oid.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("invalid Git object identity {oid:?}");
    }
    Ok(EntryIdentity {
        kind: match kind {
            "blob" => {
                if mode == 0o120000 {
                    GitEntryKind::Symlink
                } else {
                    GitEntryKind::File
                }
            }
            "commit" => GitEntryKind::Gitlink,
            "tree" => GitEntryKind::Tree,
            _ => GitEntryKind::Other,
        },
        mode: GitEntryMode(mode),
        content: ContentId::GitBlob(ObjectId(Arc::from(oid))),
    })
}

fn mode_kind(mode: &str) -> &str {
    match mode {
        "160000" => "commit",
        "040000" => "tree",
        _ => "blob",
    }
}

fn checked_path(bytes: &[u8]) -> Result<String> {
    let path = std::str::from_utf8(bytes).context("non-UTF-8 Git path is unsupported")?;
    crate::_1a_path::ensure_line_safe(path)?;
    Ok(path.to_string())
}

fn repo_path(path: &str) -> Result<RepoPath> {
    crate::_1a_path::ensure_line_safe(path)?;
    Ok(RepoPath(Arc::from(path)))
}

fn git_output<const N: usize>(
    repository: &Repository,
    args: [&str; N],
    pathspecs: &[String],
    metrics: &mut TrackedStateMetrics,
) -> Result<std::process::Output> {
    metrics.git_child_processes += 1;
    let mut command = Command::new("git");
    command.arg("-C").arg(&repository.root).args(args);
    if !pathspecs.is_empty() {
        command.arg("--").args(pathspecs);
    }
    command.output().context("run Git tracked-state query")
}
