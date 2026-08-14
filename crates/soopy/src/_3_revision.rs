use std::process::Command;
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::_0_types::{ObjectId, Repository, Revision, RevisionId};

pub fn resolve(repository: &Repository, revision: Revision) -> Result<RevisionId> {
    match revision {
        Revision::Worktree => {
            let head = rev_parse(repository, "HEAD").ok().map(ObjectId);
            Ok(RevisionId::Worktree {
                worktree: repository.worktree.clone(),
                head,
                dirty: dirty(repository)?,
            })
        }
        Revision::Named(name) => Ok(RevisionId::Commit(ObjectId(rev_parse(repository, &name)?))),
        Revision::Commit(commit) => Ok(RevisionId::Commit(ObjectId(rev_parse(
            repository, &commit.0,
        )?))),
    }
}

fn rev_parse(repository: &Repository, name: &str) -> Result<Arc<str>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["rev-parse", "--verify", &format!("{name}^{{commit}}")])
        .output()
        .with_context(|| format!("resolve revision {name:?}"))?;
    if !output.status.success() {
        bail!(
            "revision {name:?} does not resolve in {}",
            repository.root.display()
        );
    }
    Ok(Arc::from(String::from_utf8(output.stdout)?.trim()))
}

fn dirty(repository: &Repository) -> Result<bool> {
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["status", "--porcelain", "-z", "--untracked-files=normal"])
        .output()
        .context("inspect worktree state")?;
    if !output.status.success() {
        bail!(
            "git status failed in {} (a clean worktree cannot be inferred from a failed command)",
            repository.root.display()
        );
    }
    Ok(!output.stdout.is_empty())
}

/// Resolve the blob a `commit:path` names. Used to verify a committed read
/// against its expected identity before returning content.
pub fn resolve_commit_path(
    repository: &Repository,
    commit: &ObjectId,
    path: &str,
) -> Result<ObjectId> {
    let spec = format!("{}:{}", commit.0, path);
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["rev-parse", "--verify", &spec])
        .output()
        .with_context(|| format!("resolve {spec}"))?;
    if !output.status.success() {
        bail!("{spec} does not resolve in {}", repository.root.display());
    }
    Ok(ObjectId(Arc::from(
        String::from_utf8(output.stdout)?.trim(),
    )))
}
