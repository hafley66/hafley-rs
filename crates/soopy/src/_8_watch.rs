use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::_0_types::{
    Head, IndexDelta, IndexId, IndexSnapshot, Repository, RepositoryDelta, RepositorySnapshot,
    Revision, RevisionId, SourceDelta, SourceQuery, SourceSnapshot, WatchCoalescing,
    WatchQuery, WorktreeDelta, WorktreeObservation, WorktreeSnapshot,
};
use crate::_11_refs::{diff_refs, Refs};
use crate::_2_repository::open;
use crate::_7_source_tree::SourceTree;

/// Repository watcher over selected worktree-source, ref, index, and linked
/// worktree surfaces. It owns independent source and ref readers so a caller's
/// `SourceTree` never shares mutable watcher state.
pub struct RepositoryWatcher {
    _watcher: RecommendedWatcher,
    events: Receiver<notify::Result<Event>>,
    repository: Repository,
    query: WatchQuery,
    snapshot: Option<RepositorySnapshot>,
    source_tree: Option<SourceTree>,
    refs: Option<Refs>,
    git_dir: PathBuf,
    common_dir: PathBuf,
    refs_watched: bool,
    worktrees_watched: bool,
}

impl RepositoryWatcher {
    pub(crate) fn open(repository: Repository, query: WatchQuery) -> Result<Self> {
        validate_query(&repository, &query)?;
        let root = repository.root.clone();
        let (git_dir, common_dir) = git_dirs(&root)?;
        let source_tree = query
            .source
            .as_ref()
            .map(|_| SourceTree::open(repository.clone()));
        let refs = query.refs.as_ref().map(|_| Refs::open(repository.clone()));
        let (send, events) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = send.send(event);
        })
        .context("create repository watcher")?;
        let registrations = register_watches(
            &mut watcher,
            &root,
            &git_dir,
            &common_dir,
            query.source.is_some() || query.refs.is_some(),
            query.linked_worktrees,
        )?;
        let mut watcher_state = Self {
            _watcher: watcher,
            events,
            repository,
            query,
            snapshot: None,
            source_tree,
            refs,
            git_dir,
            common_dir,
            refs_watched: registrations.refs,
            worktrees_watched: registrations.worktrees,
        };
        watcher_state.snapshot = Some(watcher_state.read_snapshot()?);
        Ok(watcher_state)
    }

    /// The most recent complete snapshot. It changes only after all selected
    /// surfaces have been read successfully for a receive batch.
    pub fn snapshot(&self) -> &RepositorySnapshot {
        self.snapshot
            .as_ref()
            .expect("repository watcher stores its opening snapshot")
    }

    /// Wait for one coalesced native-event batch and return logical repository
    /// deltas. Native overflow and callback errors emit `RescanRequired` before
    /// the deterministic old-to-new delta sequence.
    pub fn recv(&mut self) -> Result<Vec<RepositoryDelta>> {
        let first = self.events.recv().context("repository watcher closed")?;
        self.recv_batch(first)
    }

    /// Like `recv`, with a caller-provided timeout. A timeout means no native
    /// event arrived and leaves the retained snapshot unchanged.
    pub fn recv_timeout(&mut self, timeout: Duration) -> Result<Option<Vec<RepositoryDelta>>> {
        match self.events.recv_timeout(timeout) {
            Ok(first) => self.recv_batch(first).map(Some),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("repository watcher closed"),
        }
    }

    fn recv_batch(&mut self, first: notify::Result<Event>) -> Result<Vec<RepositoryDelta>> {
        let started = Instant::now();
        let quiet = Duration::from_millis(self.query.coalescing.quiet_ms.into());
        let maximum = Duration::from_millis(self.query.coalescing.max_ms.into());
        let mut events = vec![first];
        loop {
            let remaining = maximum.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                break;
            }
            match self.events.recv_timeout(quiet.min(remaining)) {
                Ok(event) => events.push(event),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        let event_state = classify_event_batch(events, |path| self.is_relevant(path));
        if !event_state.relevant && !event_state.rescan {
            return Ok(Vec::new());
        }

        let before = self
            .snapshot
            .clone()
            .context("repository watcher has no prior snapshot")?;
        let after = self.read_snapshot()?;
        self.refresh_optional_watches()?;
        let mut deltas = Vec::new();
        if event_state.rescan {
            deltas.push(RepositoryDelta::RescanRequired);
        }
        deltas.extend(diff_repository(&before, &after));
        self.snapshot = Some(after);
        Ok(deltas)
    }

    fn read_snapshot(&mut self) -> Result<RepositorySnapshot> {
        let source = match (&mut self.source_tree, &self.query.source) {
            (Some(tree), Some(query)) => Some(tree.snapshot(query)?),
            (None, None) => None,
            _ => bail!("repository watcher source state is inconsistent"),
        };
        let refs = match (&self.refs, &self.query.refs) {
            (Some(refs), Some(query)) => Some(refs.snapshot(query)?),
            (None, None) => None,
            _ => bail!("repository watcher ref state is inconsistent"),
        };
        let index = self.query.index.then(|| index_snapshot(&self.repository)).transpose()?;
        let worktrees = self
            .query
            .linked_worktrees
            .then(|| worktree_snapshot(&self.repository))
            .transpose()?;
        Ok(RepositorySnapshot {
            repository: self.repository.identity.clone(),
            source,
            refs,
            index,
            worktrees,
        })
    }

    fn is_relevant(&self, path: &Path) -> bool {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let relevant = [path, canonical.as_path()].into_iter().any(|path| {
            path_is_relevant(
                &self.repository,
                &self.query,
                &self.git_dir,
                &self.common_dir,
                path,
            )
        });
        relevant
    }

    fn refresh_optional_watches(&mut self) -> Result<()> {
        for target in pending_optional_watches(
            &self.common_dir,
            self.query.source.is_some() || self.query.refs.is_some(),
            self.query.linked_worktrees,
            self.refs_watched,
            self.worktrees_watched,
        ) {
            match target {
                OptionalWatch::Refs(path) => {
                    watch(
                        &mut self._watcher,
                        &path,
                        RecursiveMode::Recursive,
                        "watch newly created shared Git refs",
                    )?;
                    self.refs_watched = true;
                }
                OptionalWatch::Worktrees(path) => {
                    watch(
                        &mut self._watcher,
                        &path,
                        RecursiveMode::Recursive,
                        "watch newly created linked worktree metadata",
                    )?;
                    self.worktrees_watched = true;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct EventState {
    relevant: bool,
    rescan: bool,
}

#[cfg(test)]
fn should_snapshot(state: EventState) -> bool {
    state.relevant || state.rescan
}

fn classify_event_batch(
    events: impl IntoIterator<Item = notify::Result<Event>>,
    is_relevant: impl Fn(&Path) -> bool,
) -> EventState {
    let mut state = EventState::default();
    for event in events {
        match event {
            Ok(event) => {
                state.rescan |= event.need_rescan();
                state.relevant |= event.paths.iter().any(|path| is_relevant(path));
            }
            Err(_) => state.rescan = true,
        }
    }
    state
}

fn path_is_relevant(
    repository: &Repository,
    query: &WatchQuery,
    git_dir: &Path,
    common_dir: &Path,
    path: &Path,
) -> bool {
    let is_git_path = path.starts_with(git_dir) || path.starts_with(common_dir);
    let source_path = path.starts_with(&repository.root) && !is_git_path;
    let current_head = path == git_dir.join("HEAD");
    let common_head = path == common_dir.join("HEAD");
    let current_index = path == git_dir.join("index") || path == git_dir.join("index.lock");
    let named_ref = path == common_dir
        || common_head
        || path == common_dir.join("packed-refs")
        || path.starts_with(common_dir.join("refs"));
    let linked_worktree = path.starts_with(common_dir.join("worktrees"));
    query.source.is_some() && (source_path || current_head || named_ref)
        || query.refs.is_some() && (current_head || named_ref)
        || query.index && current_index
        || query.linked_worktrees && (current_head || common_head || linked_worktree)
}

/// Source-only compatibility wrapper used by `SourceTree::watch` and the
/// existing CLI. It delegates debounce, path registration, and source diffing
/// to `RepositoryWatcher`.
pub struct SourceWatcher {
    inner: RepositoryWatcher,
}

impl SourceWatcher {
    pub(crate) fn open(repository: Repository, query: SourceQuery) -> Result<Self> {
        let inner = RepositoryWatcher::open(
            repository,
            WatchQuery {
                source: Some(query),
                refs: None,
                // The source-only compatibility API historically reports an
                // index-only write as a source rescan requirement.
                index: true,
                linked_worktrees: false,
                coalescing: WatchCoalescing::default(),
            },
        )?;
        Ok(Self { inner })
    }

    pub fn recv(&mut self) -> Result<Vec<SourceDelta>> {
        self.inner.recv().map(source_deltas)
    }

    pub fn recv_timeout(&mut self, timeout: Duration) -> Result<Option<Vec<SourceDelta>>> {
        self.inner.recv_timeout(timeout).map(|deltas| deltas.map(source_deltas))
    }
}

fn source_deltas(deltas: Vec<RepositoryDelta>) -> Vec<SourceDelta> {
    let mut sources = Vec::new();
    if deltas.iter().any(|delta| {
        matches!(
            delta,
            RepositoryDelta::RescanRequired | RepositoryDelta::Index(_)
        )
    }) {
        sources.push(SourceDelta::RescanRequired);
    }
    for delta in deltas {
        match delta {
            RepositoryDelta::Source(delta) => sources.push(delta),
            RepositoryDelta::Ref(_)
            | RepositoryDelta::Index(_)
            | RepositoryDelta::Worktree(_)
            | RepositoryDelta::RescanRequired => {}
        }
    }
    sources
}

fn validate_query(repository: &Repository, query: &WatchQuery) -> Result<()> {
    query.coalescing.validate()?;
    if query.source.is_none() && query.refs.is_none() && !query.index && !query.linked_worktrees {
        bail!("repository watch requires at least one selected surface");
    }
    if query
        .source
        .as_ref()
        .is_some_and(|source| source.revision != Revision::Worktree)
    {
        bail!("repository watch source requires Revision::Worktree");
    }
    if query
        .refs
        .as_ref()
        .is_some_and(|refs| refs.repository != repository.identity)
    {
        bail!("repository watch ref query belongs to another repository");
    }
    Ok(())
}

impl WatchCoalescing {
    pub fn validate(&self) -> Result<()> {
        if self.quiet_ms == 0 || self.max_ms == 0 || self.quiet_ms > self.max_ms {
            bail!("repository watch requires 0 < quiet_ms <= max_ms");
        }
        Ok(())
    }
}

struct WatchRegistrations {
    refs: bool,
    worktrees: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum OptionalWatch {
    Refs(PathBuf),
    Worktrees(PathBuf),
}

fn pending_optional_watches(
    common_dir: &Path,
    watch_refs: bool,
    watch_worktrees: bool,
    refs_watched: bool,
    worktrees_watched: bool,
) -> Vec<OptionalWatch> {
    let refs = common_dir.join("refs");
    let worktrees = common_dir.join("worktrees");
    let mut pending = Vec::new();
    if watch_refs && !refs_watched && refs.exists() {
        pending.push(OptionalWatch::Refs(refs));
    }
    if watch_worktrees && !worktrees_watched && worktrees.exists() {
        pending.push(OptionalWatch::Worktrees(worktrees));
    }
    pending
}

fn register_watches(
    watcher: &mut RecommendedWatcher,
    root: &Path,
    git_dir: &Path,
    common_dir: &Path,
    watch_refs: bool,
    linked_worktrees: bool,
) -> Result<WatchRegistrations> {
    watch(watcher, root, RecursiveMode::Recursive, "watch source root")?;
    watch(watcher, git_dir, RecursiveMode::NonRecursive, "watch worktree Git directory")?;
    if common_dir != git_dir {
        watch(watcher, common_dir, RecursiveMode::NonRecursive, "watch common Git directory")?;
    }
    let refs = common_dir.join("refs");
    let refs_watched = watch_refs && refs.exists();
    if refs_watched {
        watch(watcher, &refs, RecursiveMode::Recursive, "watch shared Git refs")?;
    }
    let mut worktrees_watched = false;
    if linked_worktrees {
        let worktrees = common_dir.join("worktrees");
        if worktrees.exists() {
            watch(
                watcher,
                &worktrees,
                RecursiveMode::Recursive,
                "watch linked worktree metadata",
            )?;
            worktrees_watched = true;
        }
    }
    Ok(WatchRegistrations {
        refs: refs_watched,
        worktrees: worktrees_watched,
    })
}

fn watch(
    watcher: &mut RecommendedWatcher,
    path: &Path,
    mode: RecursiveMode,
    action: &str,
) -> Result<()> {
    watcher.watch(path, mode).with_context(|| format!("{action}: {}", path.display()))
}

fn git_dirs(root: &Path) -> Result<(PathBuf, PathBuf)> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--absolute-git-dir", "--git-common-dir"])
        .output()
        .context("find Git directories for watcher")?;
    if !output.status.success() {
        bail!(
            "git rev-parse --absolute-git-dir --git-common-dir failed for {}",
            root.display()
        );
    }
    let text = String::from_utf8(output.stdout).context("decode Git directory paths")?;
    let mut lines = text.lines();
    let git_dir = lines.next().context("Git directory response omitted git dir")?;
    let common = lines.next().context("Git directory response omitted common dir")?;
    if lines.next().is_some() {
        bail!("Git directory response has unexpected extra lines");
    }
    let common_dir = if Path::new(common).is_absolute() {
        PathBuf::from(common)
    } else {
        root.join(common)
    };
    Ok((
        std::fs::canonicalize(git_dir).context("canonicalize worktree Git directory")?,
        std::fs::canonicalize(common_dir).context("canonicalize common Git directory")?,
    ))
}

fn index_snapshot(repository: &Repository) -> Result<IndexSnapshot> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["ls-files", "--stage", "-z"])
        .output()
        .context("read Git index")?;
    if !output.status.success() {
        bail!("git ls-files --stage failed in {}", repository.root.display());
    }
    Ok(IndexSnapshot {
        repository: repository.identity.clone(),
        worktree: repository.worktree.clone(),
        index: IndexId(*blake3::hash(&output.stdout).as_bytes()),
    })
}

fn worktree_snapshot(repository: &Repository) -> Result<WorktreeSnapshot> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["worktree", "list", "--porcelain"])
        .output()
        .context("list linked worktrees")?;
    if !output.status.success() {
        bail!("git worktree list failed in {}", repository.root.display());
    }
    let text = String::from_utf8(output.stdout).context("decode Git worktree list")?;
    let mut worktrees = Vec::new();
    for record in text.split("\n\n").filter(|record| !record.is_empty()) {
        let mut root = None;
        let mut commit = None;
        let mut branch = None;
        let mut prunable = false;
        for line in record.lines() {
            if let Some(value) = line.strip_prefix("worktree ") {
                root = Some(PathBuf::from(value));
            } else if let Some(value) = line.strip_prefix("HEAD ") {
                commit = Some(Arc::from(value));
            } else if let Some(value) = line.strip_prefix("branch ") {
                branch = Some(Arc::from(value));
            } else if line == "prunable" || line.strip_prefix("prunable ").is_some() || line == "bare" {
                prunable = true;
            }
        }
        if prunable {
            continue;
        }
        let root = root.context("Git worktree record omitted root")?;
        if !root.exists() {
            continue;
        }
        let observed = open(&root).with_context(|| format!("open linked worktree {}", root.display()))?;
        if observed.identity != repository.identity {
            bail!("linked worktree {} belongs to another repository", root.display());
        }
        let head = match (branch, commit.clone()) {
            (Some(target), Some(_)) => Head::Symbolic { target },
            (Some(target), None) => Head::Unborn { target },
            (None, Some(commit)) => Head::Detached(crate::_0_types::ObjectId(commit)),
            (None, None) => bail!("Git worktree record omitted HEAD state for {}", root.display()),
        };
        worktrees.push(WorktreeObservation {
            repository: repository.identity.clone(),
            worktree: observed.worktree,
            root,
            commit: commit.map(crate::_0_types::ObjectId),
            head,
        });
    }
    worktrees.sort_by(|left, right| left.worktree.cmp(&right.worktree));
    Ok(WorktreeSnapshot {
        repository: repository.identity.clone(),
        worktrees,
    })
}

fn diff_repository(
    before: &RepositorySnapshot,
    after: &RepositorySnapshot,
) -> Vec<RepositoryDelta> {
    let mut deltas = Vec::new();
    if let (Some(before), Some(after)) = (&before.refs, &after.refs) {
        deltas.extend(diff_refs(before, after).into_iter().map(RepositoryDelta::Ref));
    }
    if let (Some(before), Some(after)) = (&before.source, &after.source) {
        if worktree_head(&before.revision) != worktree_head(&after.revision) {
            deltas.push(RepositoryDelta::Source(SourceDelta::RevisionChanged {
                before: before.revision.clone(),
                after: after.revision.clone(),
            }));
        }
        deltas.extend(diff(before, after).into_iter().map(RepositoryDelta::Source));
    }
    if let (Some(before), Some(after)) = (&before.index, &after.index) {
        if before != after {
            deltas.push(RepositoryDelta::Index(IndexDelta::Changed {
                before: before.clone(),
                after: after.clone(),
            }));
        }
    }
    if let (Some(before), Some(after)) = (&before.worktrees, &after.worktrees) {
        deltas.extend(diff_worktrees(before, after).into_iter().map(RepositoryDelta::Worktree));
    }
    deltas
}

fn worktree_head(revision: &RevisionId) -> Option<&crate::_0_types::ObjectId> {
    match revision {
        RevisionId::Worktree { head, .. } => head.as_ref(),
        RevisionId::Commit(_) => None,
    }
}

pub(crate) fn diff(before: &SourceSnapshot, after: &SourceSnapshot) -> Vec<SourceDelta> {
    let before: BTreeMap<_, _> = before
        .files
        .iter()
        .map(|entry| (entry.source.path.clone(), entry))
        .collect();
    let after: BTreeMap<_, _> = after
        .files
        .iter()
        .map(|entry| (entry.source.path.clone(), entry))
        .collect();
    let mut paths = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    let mut deltas = Vec::new();
    for path in paths {
        match (before.get(&path), after.get(&path)) {
            (None, Some(after)) => deltas.push(SourceDelta::Added((*after).clone())),
            (Some(before), None) => deltas.push(SourceDelta::Removed(before.source.clone())),
            (Some(before), Some(after)) if before.content != after.content => {
                deltas.push(SourceDelta::Changed {
                    before: (*before).clone(),
                    after: (*after).clone(),
                });
            }
            _ => {}
        }
    }
    deltas
}

fn diff_worktrees(before: &WorktreeSnapshot, after: &WorktreeSnapshot) -> Vec<WorktreeDelta> {
    let before: BTreeMap<_, _> = before
        .worktrees
        .iter()
        .map(|worktree| (worktree.worktree.clone(), worktree))
        .collect();
    let after: BTreeMap<_, _> = after
        .worktrees
        .iter()
        .map(|worktree| (worktree.worktree.clone(), worktree))
        .collect();
    let mut ids = before
        .keys()
        .chain(after.keys())
        .cloned()
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    let mut deltas = Vec::new();
    for id in ids {
        match (before.get(&id), after.get(&id)) {
            (None, Some(after)) => deltas.push(WorktreeDelta::Added((*after).clone())),
            (Some(before), None) => deltas.push(WorktreeDelta::Removed((*before).clone())),
            (Some(before), Some(after)) if before != after => {
                deltas.push(WorktreeDelta::Changed {
                    before: (*before).clone(),
                    after: (*after).clone(),
                });
            }
            _ => {}
        }
    }
    deltas
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::_0_types::{
        IndexDelta, IndexId, IndexSnapshot, ObjectId, RepositoryDelta, RepositoryId,
        RevisionId, SourceDelta, SourceSnapshot, WorktreeId,
    };

    use super::{
        classify_event_batch, diff_repository, path_is_relevant, pending_optional_watches,
        should_snapshot, source_deltas, EventState, OptionalWatch,
    };

    fn worktree_revision(head: &str, dirty: bool) -> RevisionId {
        RevisionId::Worktree {
            worktree: WorktreeId(Arc::from("worktree")),
            head: Some(ObjectId(Arc::from(head))),
            dirty,
        }
    }

    fn repository() -> crate::_0_types::Repository {
        crate::_0_types::Repository {
            root: std::path::PathBuf::from("/source"),
            identity: RepositoryId(Arc::from("repository")),
            worktree: WorktreeId(Arc::from("worktree")),
        }
    }

    #[test]
    fn overflow_without_paths_still_requires_a_snapshot() {
        let overflow = classify_event_batch(
            vec![Err(notify::Error::generic("overflow"))],
            |_| false,
        );
        assert_eq!(overflow, EventState { relevant: false, rescan: true });
        assert!(should_snapshot(overflow));
        assert!(!should_snapshot(EventState {
            relevant: false,
            rescan: false,
        }));
    }

    #[test]
    fn dirty_worktree_state_does_not_advance_source_revision() {
        let before = SourceSnapshot {
            revision: worktree_revision("one", false),
            files: Vec::new(),
            directories: Vec::new(),
        };
        let dirty = SourceSnapshot {
            revision: worktree_revision("one", true),
            files: Vec::new(),
            directories: Vec::new(),
        };
        let advanced = SourceSnapshot {
            revision: worktree_revision("two", true),
            files: Vec::new(),
            directories: Vec::new(),
        };
        let repository = repository();
        let snapshot = |source| crate::_0_types::RepositorySnapshot {
            repository: repository.identity.clone(),
            source: Some(source),
            refs: None,
            index: None,
            worktrees: None,
        };
        assert!(diff_repository(&snapshot(before.clone()), &snapshot(dirty)).is_empty());
        assert!(diff_repository(&snapshot(before), &snapshot(advanced))
            .iter()
            .any(|delta| matches!(delta, crate::_0_types::RepositoryDelta::Source(crate::_0_types::SourceDelta::RevisionChanged { .. }))));
    }

    #[test]
    fn linked_worktree_selection_observes_both_main_head_paths() {
        let repository = repository();
        let query = crate::_0_types::WatchQuery {
            source: None,
            refs: None,
            index: false,
            linked_worktrees: true,
            coalescing: Default::default(),
        };
        assert!(path_is_relevant(
            &repository,
            &query,
            std::path::Path::new("/source/.git"),
            std::path::Path::new("/shared/.git"),
            std::path::Path::new("/source/.git/HEAD"),
        ));
        assert!(path_is_relevant(
            &repository,
            &query,
            std::path::Path::new("/source/.git"),
            std::path::Path::new("/shared/.git"),
            std::path::Path::new("/shared/.git/HEAD"),
        ));
    }

    #[test]
    fn later_optional_directories_become_registration_targets() {
        let common = std::env::temp_dir().join(format!(
            "soopy_pending_watch_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&common).unwrap();
        assert!(pending_optional_watches(&common, true, true, false, false).is_empty());
        std::fs::create_dir_all(common.join("refs")).unwrap();
        std::fs::create_dir_all(common.join("worktrees")).unwrap();
        assert_eq!(
            pending_optional_watches(&common, true, true, false, false),
            vec![
                OptionalWatch::Refs(common.join("refs")),
                OptionalWatch::Worktrees(common.join("worktrees")),
            ]
        );
        std::fs::remove_dir_all(common).unwrap();
    }

    #[test]
    fn source_only_compatibility_maps_index_delta_to_one_rescan() {
        let repository = repository();
        let index = IndexSnapshot {
            repository: repository.identity.clone(),
            worktree: repository.worktree.clone(),
            index: IndexId([1; 32]),
        };
        assert_eq!(
            source_deltas(vec![RepositoryDelta::Index(IndexDelta::Changed {
                before: index.clone(),
                after: IndexSnapshot {
                    index: IndexId([2; 32]),
                    ..index
                },
            })]),
            vec![SourceDelta::RescanRequired]
        );
    }
}
