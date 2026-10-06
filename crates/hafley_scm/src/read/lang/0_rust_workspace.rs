//! Cargo ownership shared by syntax context and rust-analyzer hosts.

use std::path::{Path, PathBuf};

pub struct RustWorkspace {
    pub manifest: PathBuf,
    pub metadata: cargo_metadata::Metadata,
}

#[derive(Debug)]
/// Why discovery failed; `manifest` is the Cargo.toml Cargo was asked about, if one was found.
pub struct DiscoverError {
    pub manifest: Option<PathBuf>,
    pub reason: String,
}

/// Locate the nearest manifest from a source (including an unborn source),
/// then let Cargo determine its owning workspace and targets.
pub fn discover(source: &Path) -> Result<RustWorkspace, DiscoverError> {
    let fail = |manifest: Option<&Path>, reason: String| DiscoverError { manifest: manifest.map(Path::to_path_buf), reason };
    let source = crate::read::io_path(source);
    let directory = if source.is_dir() { source.as_path() } else {
        source.parent().ok_or_else(|| fail(None, format!("{} has no parent", source.display())))?
    };
    let manifest = directory.ancestors()
        .map(|directory| directory.join("Cargo.toml"))
        .find(|manifest| manifest.is_file())
        .ok_or_else(|| fail(None, format!("Cargo.toml searched from {}", directory.display())))?
        .canonicalize().map_err(|error| fail(None, error.to_string()))?;
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(&manifest)
        .no_deps()
        .other_options(vec!["--offline".to_string()])
        .exec().map_err(|error| fail(Some(&manifest), error.to_string()))?;
    Ok(RustWorkspace { manifest, metadata })
}
