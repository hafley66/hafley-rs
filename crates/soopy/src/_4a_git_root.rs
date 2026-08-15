use std::path::Path;

use anyhow::Result;

use crate::_2_repository;
use crate::_2a_directory::DirectoryRoot;
use crate::_5a_git_status::{self, GitStatusCache};
use crate::_7_source_tree::SourceTree;
use crate::{GitFileQuery, TrackedFileObservation, TrackedStateResult};

/// A directory plus caller-selected Git capability. Construction is explicit:
/// filesystem operations never use this type unless the caller asks for Git
/// discovery.
pub struct GitWorktreeRoot {
    pub directory: DirectoryRoot,
    pub repository: crate::Repository,
    source_tree: SourceTree,
    status: GitStatusCache,
}

impl GitWorktreeRoot {
    fn open(directory: DirectoryRoot, repository: crate::Repository) -> Self {
        Self {
            directory,
            source_tree: SourceTree::open(repository.clone()),
            repository,
            status: GitStatusCache::default(),
        }
    }

    /// Compatibility access to the existing Git source, revision, object, and
    /// watcher API.
    pub fn source_tree(&mut self) -> &mut SourceTree {
        &mut self.source_tree
    }

    /// Observe tracked paths through the separate HEAD -> index and index ->
    /// worktree transitions. The root retains a bounded Git process and a
    /// metadata-keyed worktree identity cache between calls.
    pub fn tracked_state(&mut self, query: &GitFileQuery) -> Result<Vec<TrackedFileObservation>> {
        Ok(self.tracked_state_with_metrics(query)?.observations)
    }

    /// Observe tracked state and retain the exact child-process, hash, and
    /// cache work performed by this call.
    pub fn tracked_state_with_metrics(
        &mut self,
        query: &GitFileQuery,
    ) -> Result<TrackedStateResult> {
        _5a_git_status::tracked_state_with_metrics(&self.repository, query, &mut self.status)
    }
}

/// A filesystem-first root. `Directory` is valid for every ordinary
/// directory. `GitWorktree` adds existing Git mechanics only after explicit
/// discovery.
pub enum SourceRoot {
    Directory(DirectoryRoot),
    GitWorktree(Box<GitWorktreeRoot>),
}

impl SourceRoot {
    /// Open a plain directory without spawning Git or checking for `.git`.
    pub fn open_directory(root: impl AsRef<Path>) -> Result<Self> {
        Ok(Self::Directory(DirectoryRoot::open(root)?))
    }

    /// Explicitly discover the containing Git worktree and attach existing Git
    /// mechanics to its canonical checkout root.
    pub fn discover_git(root: impl AsRef<Path>) -> Result<Self> {
        let repository = _2_repository::discover(root)?;
        let directory = DirectoryRoot::open(&repository.root)?;
        Ok(Self::GitWorktree(Box::new(GitWorktreeRoot::open(
            directory, repository,
        ))))
    }

    pub fn directory(&self) -> &DirectoryRoot {
        match self {
            Self::Directory(directory) => directory,
            Self::GitWorktree(git) => &git.directory,
        }
    }

    pub fn directory_mut(&mut self) -> &mut DirectoryRoot {
        match self {
            Self::Directory(directory) => directory,
            Self::GitWorktree(git) => &mut git.directory,
        }
    }

    pub fn git(&self) -> Option<&GitWorktreeRoot> {
        match self {
            Self::Directory(_) => None,
            Self::GitWorktree(git) => Some(git),
        }
    }

    pub fn git_mut(&mut self) -> Option<&mut GitWorktreeRoot> {
        match self {
            Self::Directory(_) => None,
            Self::GitWorktree(git) => Some(git),
        }
    }
}
