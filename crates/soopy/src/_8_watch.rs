use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

use crate::_0_types::{Repository, Revision, RevisionId, SourceDelta, SourceQuery, SourceSnapshot};
use crate::_7_source_tree::SourceTree;

const QUIET_WINDOW: Duration = Duration::from_millis(120);
const MAX_WINDOW: Duration = Duration::from_millis(600);

/// Debounced worktree watcher. It owns a second repository session so reads
/// from the caller's `SourceTree` and watch snapshots never contend for the
/// same persistent Git reader.
pub struct SourceWatcher {
    _watcher: RecommendedWatcher,
    events: Receiver<notify::Result<Event>>,
    tree: SourceTree,
    query: SourceQuery,
    snapshot: SourceSnapshot,
    root: PathBuf,
    git_dir: PathBuf,
}

impl SourceWatcher {
    pub(crate) fn open(repository: Repository, query: SourceQuery) -> Result<Self> {
        if query.revision != Revision::Worktree {
            bail!("watch requires Revision::Worktree");
        }
        let root = repository.root.clone();
        let git_dir = git_dir(&root)?;
        let mut tree = SourceTree::open(repository);
        let snapshot = tree.snapshot(&query)?;
        let (send, events) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = send.send(event);
        })
        .context("create filesystem watcher")?;
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .context("watch source root")?;
        // A worktree can place its Git directory outside its root. Narrow
        // watches observe ref movement without watching pack/object churn.
        if git_dir != root.join(".git") {
            let _ = watcher.watch(&git_dir, RecursiveMode::NonRecursive);
            let refs = git_dir.join("refs");
            if refs.is_dir() {
                let _ = watcher.watch(&refs, RecursiveMode::Recursive);
            }
        }
        Ok(Self {
            _watcher: watcher,
            events,
            tree,
            query,
            snapshot,
            root,
            git_dir,
        })
    }

    /// Receive one quiet-window batch and return logical source deltas. A
    /// notify overflow/error yields `RescanRequired` plus the next snapshot
    /// diff, because consumers must be able to re-establish a complete view.
    pub fn recv(&mut self) -> Result<Vec<SourceDelta>> {
        let first = self.events.recv().context("filesystem watcher closed")?;
        self.recv_batch(first)
    }

    /// Like `recv`, with a caller-provided bound for test harnesses and polling
    /// loops. A timeout means no watcher event arrived and does not imply a
    /// source rescan.
    pub fn recv_timeout(&mut self, timeout: Duration) -> Result<Option<Vec<SourceDelta>>> {
        match self.events.recv_timeout(timeout) {
            Ok(first) => self.recv_batch(first).map(Some),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("filesystem watcher closed"),
        }
    }

    fn recv_batch(&mut self, first: notify::Result<Event>) -> Result<Vec<SourceDelta>> {
        let started = Instant::now();
        let mut events = vec![first];
        loop {
            let remaining = MAX_WINDOW.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                break;
            }
            match self.events.recv_timeout(QUIET_WINDOW.min(remaining)) {
                Ok(event) => events.push(event),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        let mut rescan = false;
        let mut touches_git = false;
        let mut touches_source = false;
        for event in events {
            match event {
                Ok(event) => {
                    rescan |= event.need_rescan();
                    for path in event.paths {
                        touches_git |= self.is_git_ref(&path);
                        touches_source |= !self.is_git_path(&path);
                    }
                }
                Err(_) => rescan = true,
            }
        }
        if !rescan && !touches_git && !touches_source {
            return Ok(Vec::new());
        }
        let before = self.snapshot.clone();
        let after = self.tree.snapshot(&self.query)?;
        self.snapshot = after.clone();
        let mut deltas = if rescan {
            vec![SourceDelta::RescanRequired]
        } else {
            Vec::new()
        };
        if touches_git && revision_head(&before.revision) != revision_head(&after.revision) {
            deltas.push(SourceDelta::RevisionChanged {
                before: before.revision.clone(),
                after: after.revision.clone(),
            });
        }
        deltas.extend(diff(&before, &after));
        Ok(deltas)
    }

    fn is_git_ref(&self, path: &Path) -> bool {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        path == self.git_dir.join("HEAD")
            || path == self.git_dir.join("packed-refs")
            || path.starts_with(self.git_dir.join("refs"))
            || (path.starts_with(&self.root.join(".git"))
                && (path.ends_with("HEAD")
                    || path.ends_with("packed-refs")
                    || path.components().any(|component| component.as_os_str() == "refs")))
    }

    fn is_git_path(&self, path: &Path) -> bool {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        path.starts_with(&self.git_dir) || path.starts_with(self.root.join(".git"))
    }
}

fn git_dir(root: &Path) -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--absolute-git-dir"])
        .output()
        .context("find Git directory for watcher")?;
    if !output.status.success() {
        bail!("git rev-parse --absolute-git-dir failed for {}", root.display());
    }
    Ok(PathBuf::from(String::from_utf8(output.stdout)?.trim()))
}

fn revision_head(revision: &RevisionId) -> Option<&crate::_0_types::ObjectId> {
    match revision {
        RevisionId::Worktree { head, .. } => head.as_ref(),
        RevisionId::Commit(commit) => Some(commit),
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
    let mut paths = before.keys().chain(after.keys()).cloned().collect::<Vec<_>>();
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        ContentId, ObjectId, RepoPath, RepositoryId, RevisionId, SourceEntry, SourceRef,
        SourceSnapshot,
    };

    use super::{diff, SourceDelta};

    fn snapshot(path: &str, content: &str) -> SourceSnapshot {
        let revision = RevisionId::Commit(ObjectId(Arc::from("head")));
        SourceSnapshot {
            revision: revision.clone(),
            files: vec![SourceEntry {
                source: SourceRef {
                    repository: RepositoryId(Arc::from("repo")),
                    revision,
                    path: RepoPath(Arc::from(path)),
                },
                content: ContentId::GitBlob(ObjectId(Arc::from(content))),
                size: 1,
            }],
            directories: Vec::new(),
        }
    }

    #[test]
    fn diff_classifies_add_change_and_remove_by_content_identity() {
        assert!(matches!(diff(&SourceSnapshot {
            revision: RevisionId::Commit(ObjectId(Arc::from("head"))),
            files: Vec::new(),
            directories: Vec::new(),
        }, &snapshot("src/a.rs", "one"))[0], SourceDelta::Added(_)));
        assert!(matches!(diff(&snapshot("src/a.rs", "one"), &snapshot("src/a.rs", "two"))[0], SourceDelta::Changed { .. }));
        assert!(matches!(diff(&snapshot("src/a.rs", "one"), &SourceSnapshot {
            revision: RevisionId::Commit(ObjectId(Arc::from("head"))),
            files: Vec::new(),
            directories: Vec::new(),
        })[0], SourceDelta::Removed(_)));
    }
}
