//! Path contract enforcement. `RepoPath` is a public `Arc<str>`, so a caller
//! can hand back a path that escapes the repository root or one the Git batch
//! protocols cannot carry. Both are rejected here rather than turned into a
//! host-file read or a silent record split.

use std::path::{Component, Path};

use anyhow::{bail, Result};

/// Reject a repository path that escapes its root. `RepoPath` values are
/// repository-relative by contract; `..`, an absolute path, or a prefix breaks
/// that contract before any `root.join(path)`.
pub(crate) fn ensure_repository_relative(path: &str) -> Result<()> {
    for component in Path::new(path).components() {
        match component {
            Component::ParentDir => bail!("repository path escapes its root: {path:?}"),
            Component::RootDir | Component::Prefix(_) => {
                bail!("repository path must be relative, not absolute: {path:?}")
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }
    Ok(())
}

/// Reject a path the line-oriented Git batch protocols cannot carry. A newline
/// or carriage return inside a filename splits one `hash-object --stdin-paths`
/// or `cat-file --batch-check` record into two, which corrupts the OID pairing.
pub(crate) fn ensure_line_safe(path: &str) -> Result<()> {
    if path.contains('\n') || path.contains('\r') {
        bail!(
            "path contains a newline or carriage return, which the Git batch protocol cannot carry: {path:?}"
        );
    }
    Ok(())
}
