use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::_0_types::{
    ContentId, DirectoryEntry, GitFilesQuery, ReadRequest, Repository, Revision, RevisionId,
    SourceBytes, SourceEntry, SourceQuery, SourceSnapshot,
};
use crate::_1_pattern::Pattern;
use crate::_4_worktree::WorktreeCache;
use crate::_6_git_batch::GitBatch;

pub struct SourceTree {
    repository: Repository,
    git: Option<GitBatch>,
    worktree: WorktreeCache,
}

impl SourceTree {
    pub fn open(repository: Repository) -> Self {
        Self {
            repository,
            git: None,
            worktree: WorktreeCache::default(),
        }
    }

    pub fn repository(&self) -> &Repository {
        &self.repository
    }

    pub fn resolve_revision(&self, revision: Revision) -> Result<RevisionId> {
        crate::_3_revision::resolve(&self.repository, revision)
    }

    pub fn enumerate(
        &mut self,
        revision: &RevisionId,
        patterns: &[Pattern],
    ) -> Result<Vec<SourceEntry>> {
        match revision {
            RevisionId::Worktree { .. } => crate::_4_worktree::enumerate(
                &self.repository,
                revision,
                patterns,
                &mut self.worktree,
            ),
            RevisionId::Commit(_) => {
                crate::_5_git_tree::enumerate(&self.repository, revision, patterns)
            }
        }
    }

    /// Resolve and enumerate a repository-local query into files and their
    /// parent directories. Directories are derived from files so they obey the
    /// same Git tree, ignore, nested-repository, and glob rules.
    pub fn snapshot(&mut self, query: &SourceQuery) -> Result<SourceSnapshot> {
        let revision = self.resolve_revision(query.revision.clone())?;
        let files = self.enumerate(&revision, &query.patterns)?;
        let mut paths = BTreeSet::new();
        for file in &files {
            let mut path = file.source.path.0.as_ref();
            while let Some((parent, _)) = path.rsplit_once('/') {
                paths.insert(parent.to_string());
                path = parent;
            }
        }
        let directories = paths
            .into_iter()
            .map(|path| DirectoryEntry {
                repository: self.repository.identity.clone(),
                revision: revision.clone(),
                path: crate::_0_types::RepoPath(Arc::from(path)),
            })
            .collect();
        Ok(SourceSnapshot {
            revision,
            files,
            directories,
        })
    }

    /// Enumerate tracked paths using Git's `ls-files` pathspec semantics. This
    /// is the source-language host surface, distinct from the filesystem query
    /// above, which deliberately includes ordinary non-ignored worktree files.
    pub fn git_files(&mut self, query: &GitFilesQuery) -> Result<Vec<SourceEntry>> {
        crate::_9_git_files::enumerate(&self.repository, query)
    }

    /// The same tracked-file query from a caller's working directory. This is
    /// for Git CLI compatible host surfaces whose pathspec is cwd-relative.
    pub fn git_files_from(
        &mut self,
        query: &GitFilesQuery,
        cwd: &std::path::Path,
    ) -> Result<Vec<SourceEntry>> {
        crate::_9_git_files::enumerate_from(&self.repository, query, cwd)
    }

    /// Start a debounced watcher for a worktree query. Immutable Git revisions
    /// have no changing filesystem state and are rejected by `SourceWatcher`.
    pub fn watch(&self, query: SourceQuery) -> Result<crate::_8_watch::SourceWatcher> {
        crate::_8_watch::SourceWatcher::open(self.repository.clone(), query)
    }

    pub fn read_many(&mut self, requests: &[ReadRequest]) -> Result<Vec<SourceBytes>> {
        let mut answers = Vec::with_capacity(requests.len());
        for request in requests {
            if request.source.repository != self.repository.identity {
                bail!("read request belongs to another repository");
            }
            crate::_10_path::ensure_repository_relative(&request.source.path.0)?;
            let (bytes, content) = match &request.source.revision {
                RevisionId::Worktree { worktree, .. } => {
                    if *worktree != self.repository.worktree {
                        bail!("worktree read request belongs to another worktree");
                    }
                    let path = self.repository.root.join(request.source.path.0.as_ref());
                    let bytes =
                        std::fs::read(&path).with_context(|| format!("read {}", path.display()))?;
                    // Worktree enumeration can carry either identity: the
                    // filesystem snapshot uses BLAKE3, while the tracked-file
                    // surface uses the Git blob OID. Honor the caller's
                    // expected variant so both round-trip through `read_many`.
                    let digest = match request.expected.as_ref() {
                        Some(ContentId::GitBlob(_)) => ContentId::GitBlob(
                            crate::_9_git_files::hash_object(&self.repository, &bytes)?,
                        ),
                        _ => ContentId::Blake3(*blake3::hash(&bytes).as_bytes()),
                    };
                    (Arc::from(bytes), digest)
                }
                RevisionId::Commit(commit) => {
                    // Resolve the requested `commit:path` and read that blob,
                    // rather than trusting an arbitrary caller-supplied OID.
                    let oid = crate::_3_revision::resolve_commit_path(
                        &self.repository,
                        commit,
                        request.source.path.0.as_ref(),
                    )?;
                    if let Some(expected) = request.expected.as_ref() {
                        match expected {
                            ContentId::GitBlob(expected_oid) if expected_oid.0 == oid.0 => {}
                            ContentId::GitBlob(expected_oid) => {
                                bail!(
                                    "commit:path {}:{} resolved to {} but {} was expected",
                                    commit.0,
                                    request.source.path.0,
                                    oid.0,
                                    expected_oid.0
                                )
                            }
                            ContentId::Blake3(_) => {
                                bail!("commit read requires a Git blob identity")
                            }
                        }
                    }
                    if self.git.is_none() {
                        self.git = Some(GitBatch::open(&self.repository.root)?);
                    }
                    let bytes = self
                        .git
                        .as_mut()
                        .context("Git batch reader was not initialized")?
                        .read(&oid)?;
                    (bytes, ContentId::GitBlob(oid))
                }
            };
            if request
                .expected
                .as_ref()
                .is_some_and(|expected| expected != &content)
            {
                bail!("content changed for {}", request.source.path.0);
            }
            answers.push(SourceBytes {
                source: request.source.clone(),
                content,
                bytes,
            });
        }
        Ok(answers)
    }
}
