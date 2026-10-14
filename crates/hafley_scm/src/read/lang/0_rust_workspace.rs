//! Cargo ownership shared by syntax context and rust-analyzer hosts.

use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct RustWorkspace {
    pub manifest: PathBuf,
    pub metadata: cargo_metadata::Metadata,
    local_workspace_manifests: Vec<PathBuf>,
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

impl std::fmt::Display for DiscoverError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(manifest) = &self.manifest {
            write!(formatter, "{}: {}", manifest.display(), self.reason)
        } else {
            formatter.write_str(&self.reason)
        }
    }
}

pub fn discover(source: &Path) -> Result<RustWorkspace, DiscoverError> {
    let manifest = nearest_manifest(source).map_err(|reason| DiscoverError {
        manifest: None,
        reason,
    })?;
    let cache = WORKSPACES.get_or_init(Default::default);
    let mut cached = cache.lock().unwrap();
    if let Some((key, workspace)) = cached.get(&manifest) {
        if workspace.manifest_key().as_ref() == Ok(key) {
            return Ok(workspace.clone());
        }
    }
    let mut metadata = cargo_metadata::MetadataCommand::new()
        .manifest_path(&manifest)
        .no_deps()
        .other_options(vec!["--offline".to_string()])
        .exec()
        .map_err(|error| DiscoverError {
            manifest: Some(manifest.clone()),
            reason: error.to_string(),
        })?;
    // Cargo's --no-deps preserves offline discovery without resolving registry
    // packages. Follow its explicit local paths so separate workspace roots can
    // still share a Names graph. Only the requested package joins the graph,
    // not the other members of its workspace.
    let mut local_workspace_manifests = Vec::new();
    let mut seen: std::collections::HashSet<PathBuf> = metadata
        .packages
        .iter()
        .map(|package| package.manifest_path.as_std_path().to_path_buf())
        .collect();
    let mut index = 0;
    while index < metadata.packages.len() {
        let paths: Vec<_> = metadata.packages[index]
            .dependencies
            .iter()
            .filter_map(|dependency| dependency.path.as_ref())
            .map(|path| path.join("Cargo.toml").into_std_path_buf())
            .collect();
        index += 1;
        for path in paths {
            let path = path.canonicalize().map_err(|error| DiscoverError {
                manifest: Some(path.clone()),
                reason: error.to_string(),
            })?;
            if !seen.insert(path.clone()) {
                continue;
            }
            let dependency = cargo_metadata::MetadataCommand::new()
                .manifest_path(&path)
                .no_deps()
                .other_options(vec!["--offline".to_string()])
                .exec()
                .map_err(|error| DiscoverError {
                    manifest: Some(path.clone()),
                    reason: error.to_string(),
                })?;
            local_workspace_manifests.push(
                dependency
                    .workspace_root
                    .join("Cargo.toml")
                    .into_std_path_buf(),
            );
            let package = dependency
                .packages
                .into_iter()
                .find(|package| {
                    package
                        .manifest_path
                        .as_std_path()
                        .canonicalize()
                        .ok()
                        .as_ref()
                        == Some(&path)
                })
                .ok_or_else(|| DiscoverError {
                    manifest: Some(path),
                    reason: "path dependency is absent from Cargo metadata".into(),
                })?;
            metadata.packages.push(package);
        }
    }
    let workspace = RustWorkspace {
        manifest: manifest.clone(),
        metadata,
        local_workspace_manifests,
    };
    let key = workspace.manifest_key().map_err(|reason| DiscoverError {
        manifest: Some(manifest.clone()),
        reason,
    })?;
    cached.insert(manifest, (key, workspace.clone()));
    Ok(workspace)
}

type WorkspaceCache = std::collections::HashMap<PathBuf, (ManifestKey, RustWorkspace)>;
static WORKSPACES: std::sync::OnceLock<std::sync::Mutex<WorkspaceCache>> =
    std::sync::OnceLock::new();

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
        paths.extend(self.local_workspace_manifests.iter().cloned());
        for package in &self.metadata.packages {
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
