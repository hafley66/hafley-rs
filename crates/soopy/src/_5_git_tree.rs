use std::process::Command;
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::_0_types::{
    ContentId, ObjectId, RepoPath, Repository, RevisionId, SourceEntry, SourceRef,
};
use crate::_1_pattern::{compile, Pattern};

pub fn enumerate(
    repository: &Repository,
    revision: &RevisionId,
    patterns: &[Pattern],
) -> Result<Vec<SourceEntry>> {
    let RevisionId::Commit(commit) = revision else {
        bail!("Git tree enumeration requires a commit revision");
    };
    let matcher = compile(patterns)?;
    let output = Command::new("git")
        .arg("-C")
        .arg(&repository.root)
        .args(["ls-tree", "-r", "-l", "-z", commit.0.as_ref()])
        .output()
        .context("enumerate Git tree")?;
    if !output.status.success() {
        bail!("git ls-tree failed for {}", commit.0);
    }
    let mut rows = Vec::new();
    for record in output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let Some(tab) = record.iter().position(|byte| *byte == b'\t') else {
            continue;
        };
        let meta = String::from_utf8_lossy(&record[..tab]);
        let path = std::str::from_utf8(&record[tab + 1..])
            .with_context(|| format!("non-UTF-8 path in tree {} is not supported", commit.0))?;
        let fields: Vec<&str> = meta.split_whitespace().collect();
        // `git ls-tree` reports symlinks as `blob` with mode `120000`. The
        // worktree walker does not follow symlinks, so a tracked symlink must
        // be dropped here too for the two snapshots to agree.
        if fields.get(1) != Some(&"blob")
            || fields.first() == Some(&"120000")
            || !matcher.is_match(path)
        {
            continue;
        }
        let Some(oid) = fields.get(2) else { continue };
        let size = match fields.get(3) {
            Some(value) => value
                .parse()
                .with_context(|| format!("parse `git ls-tree` size {value:?} for {path:?}"))?,
            None => 0,
        };
        rows.push(SourceEntry {
            source: SourceRef {
                repository: repository.identity.clone(),
                revision: revision.clone(),
                path: RepoPath(Arc::from(path)),
            },
            content: ContentId::GitBlob(ObjectId(Arc::from(*oid))),
            size,
        });
    }
    rows.sort_by(|left, right| left.source.path.cmp(&right.source.path));
    Ok(rows)
}
