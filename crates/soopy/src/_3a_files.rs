use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use ignore::WalkBuilder;

use crate::_0_types::{
    ContentId, FileBytesRef, FileEntry, FileQuery, FileReadRequest, FileRef, FileSnapshot, RootPath,
};
use crate::_1_pattern::compile;
use crate::_2a_directory::{CachedFile, DirectoryRoot};

impl DirectoryRoot {
    pub fn snapshot(&mut self, query: &FileQuery) -> Result<FileSnapshot> {
        snapshot(self, query)
    }

    /// Read requests one at a time through one reusable bounded buffer.
    pub fn read_each<F>(&mut self, requests: &[FileReadRequest], mut visit: F) -> Result<()>
    where
        F: FnMut(FileBytesRef<'_>) -> Result<()>,
    {
        read_each(self, requests, &mut visit)
    }
}

pub(crate) fn snapshot(root: &mut DirectoryRoot, query: &FileQuery) -> Result<FileSnapshot> {
    let matcher = (!query.patterns.is_empty())
        .then(|| compile(&query.patterns))
        .transpose()?;
    let mut files = Vec::new();
    let mut directories = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let walk_ref_secs = now_secs();
    let mut walk = WalkBuilder::new(&root.root);
    walk.hidden(false)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .filter_entry(|entry| entry.file_name() != ".git");
    for entry in walk.build() {
        let entry = entry.context("walk directory")?;
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(&root.root)
            .context("make directory-relative path")?;
        if matcher
            .as_ref()
            .is_some_and(|matcher| !matcher.is_match(relative))
        {
            continue;
        }
        let path = root_path(relative, entry.path())?;
        let metadata = entry.metadata().context("read directory file metadata")?;
        let modified_secs = modified_secs(&metadata);
        let size = metadata.len();
        let content = match root.cache.get(&path) {
            Some(cached)
                if cached.modified_secs == modified_secs
                    && cached.size == size
                    && cached.modified_secs < walk_ref_secs =>
            {
                cached.content.clone()
            }
            _ => {
                let bytes = std::fs::read(entry.path())
                    .with_context(|| format!("read {}", entry.path().display()))?;
                ContentId::Blake3(*blake3::hash(&bytes).as_bytes())
            }
        };
        seen.insert(path.clone());
        root.cache.insert(
            path.clone(),
            CachedFile {
                modified_secs,
                size,
                content: content.clone(),
            },
        );
        let mut parent = path.0.as_ref();
        while let Some((next, _)) = parent.rsplit_once('/') {
            directories.insert(RootPath(Arc::from(next)));
            parent = next;
        }
        files.push(FileEntry {
            file: FileRef {
                directory: root.identity.clone(),
                path,
            },
            content,
            size,
        });
    }
    root.cache.retain(|path, _| seen.contains(path));
    files.sort_by(|left, right| left.file.path.cmp(&right.file.path));
    Ok(FileSnapshot {
        files,
        directories: directories.into_iter().collect(),
    })
}

pub(crate) fn read_each<F>(
    root: &DirectoryRoot,
    requests: &[FileReadRequest],
    visit: &mut F,
) -> Result<()>
where
    F: FnMut(FileBytesRef<'_>) -> Result<()>,
{
    let mut buffer = Vec::new();
    for request in requests {
        if request.file.directory != root.identity {
            bail!("file read request belongs to another directory");
        }
        let path = path_for(root, &request.file.path)?;
        buffer.clear();
        std::fs::File::open(&path)
            .with_context(|| format!("open {}", path.display()))?
            .read_to_end(&mut buffer)
            .with_context(|| format!("read {}", path.display()))?;
        let content = ContentId::Blake3(*blake3::hash(&buffer).as_bytes());
        if request
            .expected
            .as_ref()
            .is_some_and(|expected| expected != &content)
        {
            bail!("content changed for {}", request.file.path.0);
        }
        visit(FileBytesRef {
            file: &request.file,
            content: &content,
            bytes: &buffer,
        })?;
    }
    Ok(())
}

pub(crate) fn path_for(root: &DirectoryRoot, path: &RootPath) -> Result<std::path::PathBuf> {
    let relative = Path::new(path.0.as_ref());
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("file path is not directory-relative: {}", path.0);
    }
    Ok(root.root.join(relative))
}

fn root_path(relative: &Path, original: &Path) -> Result<RootPath> {
    let relative = relative
        .to_str()
        .with_context(|| format!("non-UTF-8 path is not supported: {}", original.display()))?;
    Ok(RootPath(Arc::from(relative.replace('\\', "/"))))
}

fn modified_secs(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::{DirectoryRoot, FileQuery};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn latest_complete_walk_evicts_deleted_files_from_the_cache() {
        let root = std::env::temp_dir().join(format!(
            "soopy_directory_cache_{}_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("first.txt"), b"first").unwrap();
        std::fs::write(root.join("second.txt"), b"second").unwrap();
        let mut directory = DirectoryRoot::open(&root).unwrap();
        directory.snapshot(&FileQuery::default()).unwrap();
        assert_eq!(directory.cache.len(), 2);
        std::fs::remove_file(root.join("second.txt")).unwrap();
        directory.snapshot(&FileQuery::default()).unwrap();
        assert_eq!(directory.cache.len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
