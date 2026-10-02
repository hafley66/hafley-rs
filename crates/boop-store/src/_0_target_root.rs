//! Canonical containment for owned lane target paths.
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Resolve existing ancestors too: a nonexistent child of an escaping symlink is outside.
pub fn under(root: &Path, path: &Path) -> bool {
    fn resolve(path: &Path, depth: usize) -> Option<PathBuf> {
        if depth > 256 {
            return None;
        }
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let link = fs::read_link(path).ok()?;
                let target = if link.is_absolute() {
                    link
                } else {
                    path.parent()?.join(link)
                };
                return resolve(&target, depth + 1);
            }
            Ok(_) => return fs::canonicalize(path).ok(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
        let parent = resolve(path.parent()?, depth + 1)?;
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
        absolute(root).and_then(|p| resolve(&p, 0)),
        absolute(path).and_then(|p| resolve(&p, 0)),
    ) {
        (Some(root), Some(path)) => path != root && path.starts_with(root),
        _ => false,
    }
}
