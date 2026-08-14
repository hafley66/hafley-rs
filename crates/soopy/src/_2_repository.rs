use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::_0_types::{Repository, RepositoryId};

pub fn discover(start: impl AsRef<Path>) -> Result<Repository> {
    let start = start.as_ref();
    let cwd = if start.is_dir() { start } else { start.parent().unwrap_or(start) };
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
    let identity = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "--git-dir"])
        .output()
        .context("run git rev-parse --git-dir")?;
    if !identity.status.success() {
        bail!("{} is not a Git repository", root.display());
    }
    let git_dir = String::from_utf8(identity.stdout)?;
    let key = blake3::hash(format!("{}\0{}", root.display(), git_dir.trim()).as_bytes());
    Ok(Repository {
        root,
        identity: RepositoryId(Arc::from(key.to_hex().as_str())),
    })
}
