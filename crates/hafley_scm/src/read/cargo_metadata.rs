//! Cargo's package and target view for Rust resolution.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use cargo_metadata::{Metadata, MetadataCommand};
use quick_cache::sync::Cache;

use crate::read::project::ProjectError;
use crate::read::shape::ContentId;

static ROOT_KEYS: LazyLock<Cache<PathBuf, ContentId>> = LazyLock::new(|| Cache::new(128));
static RESULTS: LazyLock<Cache<ContentId, Arc<Metadata>>> = LazyLock::new(|| Cache::new(128));

/// A Cargo target's owning manifest, package edition, and target edition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RustTarget {
    pub manifest: PathBuf,
    pub package_edition: String,
    pub edition: String,
}

/// Load Cargo metadata for one manifest root. The cache key covers the lockfile
/// and every workspace member manifest, so edits to inherited dependencies or
/// target paths invalidate the result.
pub fn load(root: &Path) -> Result<Arc<Metadata>, ProjectError> {
    let root = std::fs::canonicalize(crate::read::io_path(root))
        .map_err(|error| ProjectError::Read(root.to_path_buf(), error))?;
    let root_manifest = root.join("Cargo.toml");
    let old_key = ROOT_KEYS.get(&root);
    if let Some(key) = old_key {
        if let Some(metadata) = RESULTS.get(&key) {
            if fingerprint(&root, &metadata).is_some_and(|current| current == key) {
                return Ok(metadata);
            }
        }
    }

    let metadata = MetadataCommand::new()
        .manifest_path(&root_manifest)
        .no_deps()
        .other_options(vec!["--offline".to_owned()])
        .exec()
        .map_err(|error| ProjectError::CargoMetadataFailed(root.clone(), error.to_string()))?;
    let metadata = Arc::new(metadata);
    let key = fingerprint(&root, &metadata).ok_or_else(|| {
        ProjectError::CargoMetadataFailed(
            root.clone(),
            "could not read Cargo.lock or a workspace member manifest".to_owned(),
        )
    })?;
    RESULTS.insert(key.clone(), metadata.clone());
    ROOT_KEYS.insert(root, key);
    Ok(metadata)
}

/// Rust source path to owning Cargo target, including manifest and edition.
pub fn targets(metadata: &Metadata) -> BTreeMap<PathBuf, RustTarget> {
    metadata
        .workspace_packages()
        .into_iter()
        .flat_map(|package| {
            package.targets.iter().map(move |target| {
                let source_path = Path::new(target.src_path.as_str());
                let source = std::fs::canonicalize(crate::read::io_path(source_path))
                    .unwrap_or_else(|_| source_path.to_path_buf());
                (
                    source,
                    RustTarget {
                        manifest: PathBuf::from(package.manifest_path.as_str()),
                        package_edition: package.edition.to_string(),
                        edition: target.edition.to_string(),
                    },
                )
            })
        })
        .collect()
}

/// Workspace member manifest path to package, with dependency path targets.
pub fn packages(metadata: &Metadata) -> BTreeMap<PathBuf, &cargo_metadata::Package> {
    metadata
        .workspace_packages()
        .into_iter()
        .map(|package| (PathBuf::from(package.manifest_path.as_str()), package))
        .collect()
}

/// Find each Cargo workspace represented by the supplied paths. The nearest
/// ancestor manifest with a `[workspace]` table owns a member; a standalone
/// package uses its own manifest directory.
pub fn workspace_roots(fallback: &Path, paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = std::collections::BTreeSet::new();
    for path in paths {
        let mut directory = path.parent().unwrap_or(path);
        let mut nearest_package = None;
        let mut workspace = None;
        loop {
            let manifest = directory.join("Cargo.toml");
            if manifest.is_file() {
                nearest_package.get_or_insert_with(|| directory.to_path_buf());
                if std::fs::read_to_string(crate::read::io_path(&manifest))
                    .ok()
                    .and_then(|text| basic_toml::from_str::<serde_json::Value>(&text).ok())
                    .is_some_and(|value| value.get("workspace").is_some())
                {
                    workspace = Some(directory.to_path_buf());
                    break;
                }
            }
            let Some(parent) = directory.parent() else {
                break;
            };
            directory = parent;
        }
        roots.insert(
            workspace
                .or(nearest_package)
                .unwrap_or_else(|| fallback.to_path_buf()),
        );
    }
    roots.into_iter().collect()
}

fn fingerprint(root: &Path, metadata: &Metadata) -> Option<ContentId> {
    let mut paths = vec![root.join("Cargo.lock"), root.join("Cargo.toml")];
    paths.extend(
        metadata
            .workspace_packages()
            .into_iter()
            .map(|package| PathBuf::from(package.manifest_path.as_str())),
    );
    paths.sort();
    paths.dedup();
    let mut bytes = Vec::new();
    for path in paths {
        let content = std::fs::read(crate::read::io_path(&path)).unwrap_or_default();
        bytes.extend_from_slice(path.to_string_lossy().as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&(content.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&content);
    }
    Some(ContentId::blake3(&bytes))
}
