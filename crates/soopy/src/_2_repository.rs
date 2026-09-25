use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::_0_types::{Repository, RepositoryId, WorktreeId};

pub fn discover(start: impl AsRef<Path>) -> Result<Repository> {
    let start = start.as_ref();
    let cwd = if start.is_dir() {
        start
    } else {
        start.parent().unwrap_or(start)
    };
    if let Some(repository) = discover_on_disk(cwd) {
        return Ok(repository);
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("run git rev-parse --show-toplevel")?;
    if !output.status.success() {
        bail!("{} is not inside a Git worktree", cwd.display());
    }
    let root = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    open(root)
}
/// `discover` read off `.git` / `gitdir:` / `commondir`, hashed as `open` hashes
/// git's answers; `None` (env override, unknown layout) hands the question to git.
fn discover_on_disk(cwd: &Path) -> Option<Repository> {
    if ["GIT_DIR", "GIT_WORK_TREE", "GIT_COMMON_DIR", "GIT_CEILING_DIRECTORIES"]
        .iter()
        .any(|name| std::env::var_os(name).is_some())
    {
        return None;
    }
    let cwd = std::fs::canonicalize(cwd).ok()?;
    let root = cwd.ancestors().find(|dir| dir.join(".git").exists())?.to_path_buf();
    let dot_git = root.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        let text = std::fs::read_to_string(&dot_git).ok()?;
        root.join(text.trim().strip_prefix("gitdir:")?.trim())
    };
    let git_dir = std::fs::canonicalize(git_dir).ok()?;
    if !git_dir.join("HEAD").is_file() {
        return None;
    }
    let common_dir = match std::fs::read_to_string(git_dir.join("commondir")) {
        Ok(text) => std::fs::canonicalize(git_dir.join(text.trim())).ok()?,
        Err(_) => git_dir.clone(),
    };
    let key = blake3::hash(common_dir.as_os_str().to_string_lossy().as_bytes());
    let worktree_key = blake3::hash(git_dir.as_os_str().to_string_lossy().as_bytes());
    Some(Repository {
        root,
        identity: RepositoryId(Arc::from(key.to_hex().as_str())),
        worktree: WorktreeId(Arc::from(worktree_key.to_hex().as_str())),
    })
}

pub fn open(root: impl Into<PathBuf>) -> Result<Repository> {
    let root = std::fs::canonicalize(root.into()).context("canonicalize repository root")?;
    // Repository identity comes from the common Git directory, not the
    // per-worktree `--git-dir`. A linked worktree reports
    // `<common>/.git/worktrees/<name>` for `--git-dir` but shares the common
    // directory with its siblings, so hashing the common directory keeps one
    // repository's identity stable across its worktrees.
    let identity = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .context("run git rev-parse --git-common-dir")?;
    if !identity.status.success() {
        bail!("{} is not a Git repository", root.display());
    }
    let common_dir = String::from_utf8(identity.stdout)?.trim().to_string();
    let common_path = std::fs::canonicalize(root.join(&common_dir))
        .with_context(|| format!("canonicalize common Git directory {common_dir:?}"))?;
    let key = blake3::hash(common_path.as_os_str().to_string_lossy().as_bytes());
    // Worktree identity comes from the per-checkout Git directory, not the
    // shared common directory. The main worktree's absolute Git directory is
    // the common `.git`; a linked worktree's is `worktrees/<name>`. Hashing
    // the absolute Git directory keeps each checkout distinct from its
    // siblings and stable across reopen.
    let git_dir = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "--absolute-git-dir"])
        .output()
        .context("run git rev-parse --absolute-git-dir")?;
    if !git_dir.status.success() {
        bail!("{} has no resolvable Git directory", root.display());
    }
    let git_dir_path = std::fs::canonicalize(String::from_utf8(git_dir.stdout)?.trim())
        .context("canonicalize Git directory")?;
    let worktree_key = blake3::hash(git_dir_path.as_os_str().to_string_lossy().as_bytes());
    Ok(Repository {
        root,
        identity: RepositoryId(Arc::from(key.to_hex().as_str())),
        worktree: WorktreeId(Arc::from(worktree_key.to_hex().as_str())),
    })
}

#[cfg(test)]
mod disk_discovery {
    use super::*;

    /// The on-disk answer and git's answer agree for the checkout the tests run in.
    #[test]
    fn on_disk_discovery_matches_git() {
        let here = std::env::current_dir().expect("current dir");
        let Some(fast) = discover_on_disk(&here) else {
            return;
        };
        let output = Command::new("git")
            .arg("-C")
            .arg(&here)
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .expect("git runs");
        let slow = open(PathBuf::from(String::from_utf8(output.stdout).expect("utf8").trim()))
            .expect("git opens the repository");
        assert_eq!(fast, slow);
    }
}
