//! Materialize selected Git blobs for one-shot, revision-correct project work.
//! The scratch directory lasts only for the callback; paths inside it remain
//! repository-relative in the emitted facts.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct RevisionReader {
    tree: soopy::SourceTree,
    batch: soopy::GitBatch,
    blobs: BTreeMap<String, Arc<[u8]>>,
}

pub struct RevisionSnapshot {
    pub sha: String,
    pub files: BTreeMap<String, String>,
}

impl RevisionReader {
    pub fn open(root: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let root = std::fs::canonicalize(root)?;
        let repository = soopy::open(&root)?;
        Ok(Self {
            tree: soopy::SourceTree::open(repository.clone()),
            batch: soopy::GitBatch::open(&repository.root)?,
            blobs: BTreeMap::new(),
        })
    }

    pub fn with_revision<T>(
        &mut self,
        revision: &str,
        patterns: &[soopy::Pattern],
        selected: Option<&[PathBuf]>,
        run: impl FnOnce(&[PathBuf], &Path) -> Result<T, Box<dyn std::error::Error>>,
    ) -> Result<(RevisionSnapshot, T), Box<dyn std::error::Error>> {
        let resolved = self
            .tree
            .resolve_revision(soopy::Revision::Named(Arc::from(revision)))?;
        let soopy::RevisionId::Commit(commit) = resolved else {
            return Err(format!("{revision} is not a commit").into());
        };
        let snapshot = self.tree.snapshot(&soopy::SourceQuery {
            revision: soopy::Revision::Commit(commit.clone()),
            patterns: patterns.to_vec(),
        })?;
        let mut files = BTreeMap::new();
        for entry in &snapshot.files {
            let path: &str = &entry.source.path.0;
            if selected.is_some_and(|selected| {
                !selected.iter().any(|wanted| {
                    wanted.as_os_str().is_empty() || Path::new(path).starts_with(wanted)
                })
            }) {
                continue;
            }
            let soopy::ContentId::GitBlob(oid) = &entry.content else {
                return Err(format!("{path} at {revision} carries no Git blob").into());
            };
            files.insert(path.to_string(), oid.0.to_string());
        }

        let scratch = tempfile::Builder::new().prefix("ryi-revision-").tempdir()?;
        let mut paths = Vec::with_capacity(files.len());
        for (path, oid) in &files {
            let bytes = match self.blobs.get(oid) {
                Some(bytes) => bytes.clone(),
                None => {
                    let bytes = self.batch.read(&soopy::ObjectId(Arc::from(oid.as_str())))?;
                    self.blobs.insert(oid.clone(), bytes.clone());
                    bytes
                }
            };
            let destination = scratch.path().join(path);
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&destination, bytes.as_ref())?;
            paths.push(PathBuf::from(path));
        }

        let previous = std::env::current_dir()?;
        std::env::set_current_dir(scratch.path())?;
        let answer = run(&paths, scratch.path());
        std::env::set_current_dir(previous)?;
        Ok((
            RevisionSnapshot {
                sha: commit.0.to_string(),
                files,
            },
            answer?,
        ))
    }
}
