//! Canonical containment for owned lane target paths.
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Resolve existing ancestors too: a nonexistent child of an escaping symlink is outside.
pub fn under(root: &Path, path: &Path) -> bool {
    fn resolve(path: &Path) -> Option<PathBuf> {
        if path.exists() {
            return fs::canonicalize(path).ok();
        }
        let parent = resolve(path.parent()?)?;
        let name = path.file_name()?;
        if name == ".." || name == "." {
            return None;
        }
        Some(parent.join(name))
    }
    let absolute = |p: &Path| {
        if p.is_absolute() {
            Some(p.to_path_buf())
        } else {
            std::env::current_dir().ok().map(|cwd| cwd.join(p))
        }
    };
    match (
        absolute(root).and_then(|p| resolve(&p)),
        absolute(path).and_then(|p| resolve(&p)),
    ) {
        (Some(root), Some(path)) => path != root && path.starts_with(root),
        _ => false,
    }
}
