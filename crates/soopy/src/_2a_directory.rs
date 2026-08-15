use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use crate::_0_types::{ContentId, DirectoryId, RootPath};

#[derive(Clone, Debug)]
pub(crate) struct CachedFile {
    pub(crate) modified_secs: i64,
    pub(crate) size: u64,
    pub(crate) content: ContentId,
}

pub(crate) type DirectoryCache = BTreeMap<RootPath, CachedFile>;

/// A canonical filesystem directory with a bounded cache for its latest
/// complete filesystem walk. It has no Git dependency.
pub struct DirectoryRoot {
    pub root: PathBuf,
    pub identity: DirectoryId,
    pub(crate) cache: DirectoryCache,
}

impl DirectoryRoot {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = std::fs::canonicalize(root.as_ref())
            .with_context(|| format!("canonicalize directory {}", root.as_ref().display()))?;
        if !root.is_dir() {
            bail!("{} is not a directory", root.display());
        }
        let identity = blake3::hash(root.as_os_str().to_string_lossy().as_bytes());
        Ok(Self {
            root,
            identity: DirectoryId(Arc::from(identity.to_hex().as_str())),
            cache: BTreeMap::new(),
        })
    }
}
