use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use ignore::WalkBuilder;

use crate::_0_types::{ContentId, RepoPath, Repository, RevisionId, SourceEntry, SourceRef};
use crate::_1_pattern::{compile, Pattern};

#[derive(Clone, Debug)]
struct CachedFile {
    modified_secs: i64,
    size: u64,
    content: ContentId,
}

/// Metadata retained by one repository session. The timestamp guard follows
/// Git's racy-index rule: a file observed in the prior walk's clock second is
/// re-hashed even when its whole-second mtime and size still match.
#[derive(Default)]
pub(crate) struct WorktreeCache {
    files: BTreeMap<RepoPath, CachedFile>,
    walk_ref_secs: i64,
}

pub fn enumerate(
    repository: &Repository,
    revision: &RevisionId,
    patterns: &[Pattern],
    cache: &mut WorktreeCache,
) -> Result<Vec<SourceEntry>> {
    let matcher = compile(patterns)?;
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();
    // Walks start at each pattern's literal directory prefix; the walker still
    // reads parent ignore files upward from there.
    let mut bases = walk_bases(&repository.root, patterns).into_iter();
    let Some(first) = bases.next() else {
        return Ok(Vec::new());
    };
    let mut walk = WalkBuilder::new(&first);
    for base in bases {
        walk.add(base);
    }
    walk.hidden(false).filter_entry(|entry| {
        if entry.file_name() == ".git" {
            return false;
        }
        !(entry.depth() >= 1
            && entry.file_type().is_some_and(|kind| kind.is_dir())
            && entry.path().join(".git").exists())
    });
    for entry in walk.build() {
        let entry = entry.context("walk worktree")?;
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(&repository.root)
            .context("make repository-relative path")?;
        if !matcher.is_match(relative) {
            continue;
        }
        let relative = relative
            .to_str()
            .with_context(|| format!("non-UTF-8 path is not supported: {:?}", entry.path()))?;
        let path = RepoPath(Arc::from(relative.replace('\\', "/")));
        seen.insert(path.clone());
        let metadata = entry.metadata().context("read worktree metadata")?;
        let modified_secs = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0);
        let size = metadata.len();
        let content = match cache.files.get(&path) {
            Some(cached)
                if cached.modified_secs == modified_secs
                    && cached.size == size
                    && cached.modified_secs < cache.walk_ref_secs =>
            {
                cached.content.clone()
            }
            _ => {
                let bytes = std::fs::read(entry.path())
                    .with_context(|| format!("read {}", entry.path().display()))?;
                ContentId::blake3(&bytes)
            }
        };
        cache.files.insert(
            path.clone(),
            CachedFile {
                modified_secs,
                size,
                content: content.clone(),
            },
        );
        rows.push(SourceEntry {
            source: SourceRef {
                repository: repository.identity.clone(),
                revision: revision.clone(),
                path,
            },
            content,
            size,
        });
    }
    cache.files.retain(|path, _| seen.contains(path));
    cache.walk_ref_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(i64::MAX);
    rows.sort_by(|left, right| left.source.path.cmp(&right.source.path));
    Ok(rows)
}

/// The directories the walk must start from: each pattern's components before
/// its first glob metacharacter, minus any base another base already covers.
fn walk_bases(root: &std::path::Path, patterns: &[Pattern]) -> Vec<std::path::PathBuf> {
    let mut bases: Vec<std::path::PathBuf> = patterns
        .iter()
        .map(|pattern| {
            let literal: Vec<&str> = pattern
                .0
                .split('/')
                .take_while(|part| !part.contains(['*', '?', '[', '{']))
                .collect();
            // The last literal component of a glob-free pattern names a file.
            let dirs = if literal.len() == pattern.0.split('/').count() {
                &literal[..literal.len().saturating_sub(1)]
            } else {
                &literal[..]
            };
            dirs.iter().fold(root.to_path_buf(), |path, part| path.join(part))
        })
        .collect();
    if bases.is_empty() {
        bases.push(root.to_path_buf());
    }
    bases.sort();
    bases.dedup();
    let mut kept: Vec<std::path::PathBuf> = Vec::new();
    for base in bases {
        if !kept.iter().any(|outer| base.starts_with(outer)) {
            kept.push(base);
        }
    }
    kept.retain(|base| base.is_dir());
    kept
}
