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
