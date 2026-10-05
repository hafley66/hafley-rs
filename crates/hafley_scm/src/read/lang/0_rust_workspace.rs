//! Cargo ownership shared by syntax context and rust-analyzer hosts.

use std::path::{Path, PathBuf};

pub struct RustWorkspace {
    pub manifest: PathBuf,
    pub metadata: cargo_metadata::Metadata,
}

/// Locate the nearest manifest from a source (including an unborn source),
/// then let Cargo determine its owning workspace and targets.
pub fn nearest_manifest(source: &Path) -> Result<PathBuf, String> {
    let source = crate::read::io_path(source);
    let directory = if source.is_dir() {
        source.as_path()
    } else {
        source
            .parent()
            .ok_or_else(|| format!("{} has no parent", source.display()))?
    };
    directory
        .ancestors()
        .map(|directory| directory.join("Cargo.toml"))
        .find(|manifest| manifest.is_file())
        .ok_or_else(|| format!("Cargo.toml searched from {}", directory.display()))?
        .canonicalize()
        .map_err(|error| error.to_string())
}

#[derive(Debug)]
pub struct DiscoverError {
    pub manifest: Option<PathBuf>,
    pub reason: String,
}

pub fn discover(source: &Path) -> Result<RustWorkspace, DiscoverError> {
    let manifest = nearest_manifest(source).map_err(|reason| DiscoverError { manifest: None, reason })?;
    let metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(&manifest)
        .no_deps()
        .other_options(vec!["--offline".to_string()])
        .exec()
        .map_err(|error| DiscoverError { manifest: Some(manifest.clone()), reason: error.to_string() })?;
    Ok(RustWorkspace { manifest, metadata })
}

/// Content identity of the loaded workspace's manifests and lockfiles.
/// Paths are canonical and sorted; creation and deletion change the key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ManifestKey(pub Vec<(PathBuf, crate::read::types::ContentId)>);

impl RustWorkspace {
    pub fn manifest_key(&self) -> Result<ManifestKey, String> {
        let mut paths = std::collections::BTreeSet::new();
        paths.insert(
            self.metadata
                .workspace_root
                .as_std_path()
                .join("Cargo.toml"),
        );
        for package in self.metadata.workspace_packages() {
            paths.insert(package.manifest_path.as_std_path().to_path_buf());
        }
        let locks: Vec<_> = paths
            .iter()
            .filter_map(|path| path.parent().map(|parent| parent.join("Cargo.lock")))
            .collect();
        paths.extend(locks.into_iter().filter(|path| path.is_file()));
        let mut entries = Vec::new();
        for path in paths {
            let path = path.canonicalize().map_err(|error| error.to_string())?;
            let bytes =
                std::fs::read(crate::read::io_path(&path)).map_err(|error| error.to_string())?;
            entries.push((path, crate::read::shape::content_id_of(&bytes)));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(ManifestKey(entries))
    }
}
