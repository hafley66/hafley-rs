//! rust-analyzer's module tree from the crate def maps alone: a file's crate,
//! module path and declaring `mod` item. No body is type checked.

use super::*;
use ra_ap_hir::HasVisibility;
use ra_ap_syntax::ast::HasAttrs;
use ra_ap_vfs::VfsPath;
use std::sync::{Arc, Mutex};

use super::super::rust_checker_session::{checker_workspace_loaded, CheckerWorkspace};

/// One module a file is. A file several targets include has one per target.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ModulePlace {
    /// Dense workspace target identity, ordered by canonical root file.
    pub target: u32,
    pub file: PathBuf,
    /// Directory used when planning a child module declaration.
    pub directory: PathBuf,
    /// The crate's root file, absolute: the crate's identity.
    pub crate_root: PathBuf,
    /// The crate's own name, as an extern path spells it.
    pub crate_name: String,
    /// Module names from the crate root down; empty for the root itself.
    pub path: Vec<String>,
    /// The `mod` item declaring this module: its file and byte range.
    pub decl: Option<(PathBuf, u32, u32)>,
}

/// The fast tier's warm host, plus the texts this tree laid over the disk.
pub struct RustModuleTree {
    /// Cargo manifest selected for the source, retained for ownership abstains.
    pub manifest: PathBuf,
    pub(super) workspace: Arc<Mutex<CheckerWorkspace>>,
    pub key: super::super::rust_workspace::ManifestKey,
    pub load: Duration,
    pub(super) crate_roots: std::collections::BTreeMap<PathBuf, u32>,
    /// Absolute path -> the text the host holds in place of the disk's.
    staged: Mutex<HashMap<PathBuf, String>>,
    pub(super) external_names: HashMap<PathBuf, std::collections::BTreeSet<String>>,
}

/// Cargo selects the source's workspace from its directory; rust-analyzer
/// supplies module ownership from that workspace's def maps.
pub fn module_tree(source: &Path, budget: Duration) -> Result<RustModuleTree, CheckerError> {
    let discovered = super::super::rust_workspace::discover(source)
        .map_err(|error| CheckerError::NoWorkspace(error.to_string()))?;
    module_tree_for_workspace(source, &discovered, budget)
}

/// Open the provider from workspace discovery already performed by the caller.
pub fn module_tree_for_workspace(
    source: &Path,
    discovered: &super::super::rust_workspace::RustWorkspace,
    budget: Duration,
) -> Result<RustModuleTree, CheckerError> {
    module_tree_for_workspace_files(&[host_path(source)], discovered, budget)
}

/// Refresh every requested workspace file, including newly created modules.
pub fn module_tree_for_workspace_files(
    sources: &[PathBuf],
    discovered: &super::super::rust_workspace::RustWorkspace,
    budget: Duration,
) -> Result<RustModuleTree, CheckerError> {
    let key = discovered
        .manifest_key()
        .map_err(CheckerError::NoWorkspace)?;
    let files: Vec<_> = sources
        .iter()
        .map(|source| (source.to_string_lossy().into_owned(), host_path(source)))
        .collect();
    let (workspace, load) = checker_workspace_loaded(
        discovered,
        super::super::rust_checker::LoadMode::Names,
        &files,
        budget,
    )?;
    let roots: std::collections::BTreeSet<_> = discovered
        .metadata
        .workspace_packages()
        .into_iter()
        .flat_map(|package| &package.targets)
        .map(|target| host_path(target.src_path.as_std_path()))
        .collect();
    let crate_roots = roots
        .into_iter()
        .enumerate()
        .map(|(index, root)| (root, index as u32))
        .collect();
    let members: std::collections::BTreeSet<_> = discovered
        .metadata
        .workspace_packages()
        .into_iter()
        .filter_map(|package| {
            package
                .manifest_path
                .parent()
                .map(|path| path.to_path_buf())
        })
        .collect();
    let mut external_names = HashMap::new();
    for package in discovered.metadata.workspace_packages() {
        let names: std::collections::BTreeSet<_> = package
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency
                    .path
                    .as_ref()
                    .is_none_or(|path| !members.contains(path))
            })
            .map(|dependency| {
                dependency
                    .rename
                    .as_ref()
                    .unwrap_or(&dependency.name)
                    .replace('-', "_")
            })
            .chain(
                ["std", "core", "alloc", "proc_macro", "test"]
                    .into_iter()
                    .map(str::to_string),
            )
            .collect();
        for target in &package.targets {
            external_names.insert(host_path(target.src_path.as_std_path()), names.clone());
        }
    }
    Ok(RustModuleTree {
        manifest: discovered.manifest.clone(),
        workspace,
        key,
        load,
        crate_roots,
        staged: Mutex::new(HashMap::new()),
        external_names,
    })
}

/// `path` canonical; an unborn file through its parent directory.
pub(super) fn host_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        match (
            path.parent()
                .and_then(|dir| std::fs::canonicalize(dir).ok()),
            path.file_name(),
        ) {
            (Some(dir), Some(name)) => dir.join(name),
            _ => path.to_path_buf(),
        }
    })
}

pub(super) fn vfs_path(path: &Path) -> VfsPath {
    VfsPath::new_real_path(path.to_string_lossy().into_owned())
}

impl RustModuleTree {
    /// The host holds exactly `texts` over the disk; an earlier staged path
    /// `texts` omits gets the disk's text back.
    pub fn sync(&self, texts: &[(PathBuf, String)]) -> Result<(), CheckerError> {
        let mut workspace = self.workspace.lock().unwrap();
        let mut staged = self.staged.lock().unwrap();
        let wanted: HashMap<PathBuf, &String> = texts
            .iter()
            .map(|(path, text)| (host_path(path), text))
            .collect();
        for (path, text) in &wanted {
            workspace
                .vfs
                .set_file_contents(vfs_path(path), Some(text.as_bytes().to_vec()));
            staged.insert(path.clone(), (*text).clone());
        }
        let released: Vec<PathBuf> = staged
            .keys()
            .filter(|path| !wanted.contains_key(*path))
            .cloned()
            .collect();
        for path in released {
            workspace.vfs.set_file_contents(
                vfs_path(&path),
                std::fs::read(crate::read::io_path(&path)).ok(),
            );
            staged.remove(&path);
        }
        workspace.apply_vfs_changes()
    }

    /// Whether the destination module path and item are visible to a source module.
    pub fn can_name(&self, from: &Path, to: &Path, item: &str) -> Option<bool> {
        let workspace = self.workspace.lock().unwrap();
        let (from, _) = workspace.vfs.file_id(&vfs_path(&host_path(from)))?;
        let (to, _) = workspace.vfs.file_id(&vfs_path(&host_path(to)))?;
        let db = workspace.host.raw_database();
        attach_db(db, || {
            let sema = Semantics::new(db);
            let viewers: Vec<_> = sema
                .file_to_module_defs(ra_ap_ide::FileId::from_raw(from.index()))
                .collect();
            let targets: Vec<_> = sema
                .file_to_module_defs(ra_ap_ide::FileId::from_raw(to.index()))
                .collect();
            if viewers.is_empty() || targets.is_empty() {
                return None;
            }
            let definitions: Vec<_> = targets
                .iter()
                .flat_map(|target| target.scope(db, None))
                .filter_map(|(name, definition)| match definition {
                    ra_ap_hir::ScopeDef::ModuleDef(definition) if name.as_str() == item => {
                        Some(definition)
                    }
                    _ => None,
                })
                .collect();
            let parsed = sema.parse_guess_edition(ra_ap_ide::FileId::from_raw(from.index()));
            let mut readings = Vec::new();
            for path in parsed.syntax().descendants().filter_map(ast::Path::cast) {
                let Some(scope) = sema.scope(path.syntax()) else {
                    continue;
                };
                let Some(PathResolution::Def(definition)) = scope.speculative_resolve(&path) else {
                    continue;
                };
                if !definitions.contains(&definition) {
                    continue;
                }
                let viewer = scope.module();
                let mut visible = definition.is_visible_from(db, viewer);
                let mut prefix = path.qualifier();
                while let Some(path) = prefix {
                    if let Some(PathResolution::Def(definition)) = scope.speculative_resolve(&path)
                    {
                        visible &= definition.is_visible_from(db, viewer);
                    }
                    prefix = path.qualifier();
                }
                readings.push(visible);
            }
            Some(if readings.is_empty() {
                viewers.iter().all(|viewer| {
                    targets.iter().any(|target| {
                        target
                            .path_to_root(db)
                            .into_iter()
                            .all(|module| module.is_visible_from(db, *viewer))
                            && target
                                .scope(db, Some(*viewer))
                                .iter()
                                .any(|(name, _)| name.as_str() == item)
                    })
                })
            } else {
                readings.into_iter().all(|visible| visible)
            })
        })
    }

    /// Every module the def maps place `file` at, in crate-root order.
    pub fn places(&self, file: &Path) -> Vec<ModulePlace> {
        self.places_for_files(&[file.to_path_buf()]).pop().unwrap()
    }

    pub(super) fn place_of(
        &self,
        workspace: &CheckerWorkspace,
        db: &RootDatabase,
        module: ra_ap_hir::Module,
    ) -> Option<ModulePlace> {
        let path_of = |file: ra_ap_ide::FileId| {
            workspace
                .vfs
                .file_path(ra_ap_vfs::FileId::from_raw(file.index()))
                .as_path()
                .map(|path| PathBuf::from(path.to_string()))
        };
        let krate = module.krate(db);
        let crate_root = path_of(krate.root_file(db))?;
        let decl = module.declaration_source_range(db).and_then(|range| {
            Some((
                path_of(range.file_id.file_id()?.file_id(db))?,
                range.value.start().into(),
                range.value.end().into(),
            ))
        });
        let file = path_of(module.definition_source_file_id(db).file_id()?.file_id(db))?;
        let directory = self.module_directory(workspace, db, module)?;
        Some(ModulePlace {
            file,
            directory,
            target: *self.crate_roots.get(&crate_root)?,
            crate_root,
            crate_name: crate_name(db, krate),
            path: module
                .path_segments(db)
                .map(|name| name.as_str().to_string())
                .collect(),
            decl,
        })
    }

    fn module_directory(
        &self,
        workspace: &CheckerWorkspace,
        db: &RootDatabase,
        module: ra_ap_hir::Module,
    ) -> Option<PathBuf> {
        let source = module.definition_source(db);
        match source.value {
            ra_ap_hir::ModuleSource::Module(_) => {
                let mut directory = self.module_directory(workspace, db, module.parent(db)?)?;
                directory.push(module.name(db)?.as_str());
                Some(directory)
            }
            ra_ap_hir::ModuleSource::SourceFile(_) => {
                let file = source.file_id.file_id()?.file_id(db);
                let path = workspace
                    .vfs
                    .file_path(ra_ap_vfs::FileId::from_raw(file.index()))
                    .as_path()?;
                let file = PathBuf::from(path.to_string());
                let attributed = module.declaration_source(db).is_some_and(|source| {
                    source.value.attrs().any(|attr| {
                        attr.path()
                            .is_some_and(|path| path.syntax().text().to_string() == "path")
                    })
                });
                if module.parent(db).is_none()
                    || file.file_name().is_some_and(|name| name == "mod.rs")
                    || attributed
                {
                    file.parent().map(Path::to_path_buf)
                } else {
                    Some(file.with_extension(""))
                }
            }
            _ => None,
        }
    }

    /// The name crate `from` reaches crate `to` by: its dependency entry's
    /// (rename included), else `to`'s own name.
    pub fn extern_name(&self, from: &ModulePlace, to: &ModulePlace) -> String {
        let workspace = self.workspace.lock().unwrap();
        let db = workspace.host.raw_database();
        let root_of = |krate: Crate| {
            workspace
                .vfs
                .file_path(ra_ap_vfs::FileId::from_raw(krate.root_file(db).index()))
                .as_path()
                .map(|path| PathBuf::from(path.to_string()))
        };
        attach_db(db, || {
            Crate::all(db)
                .into_iter()
                .find(|krate| root_of(*krate).as_deref() == Some(from.crate_root.as_path()))
                .and_then(|krate| {
                    krate
                        .dependencies(db)
                        .into_iter()
                        .find(|dep| root_of(dep.krate).as_deref() == Some(to.crate_root.as_path()))
                })
                .map(|dep| dep.name.as_str().to_string())
                .unwrap_or_else(|| to.crate_name.clone())
        })
    }
}

fn crate_name(db: &RootDatabase, krate: Crate) -> String {
    krate
        .display_name(db)
        .map(|name| name.crate_name().as_str().to_string())
        .unwrap_or_default()
}

impl Drop for RustModuleTree {
    fn drop(&mut self) {
        // Return staged paths to disk before releasing this request's overlay.
        // Drop cannot report an error; sync callers still receive UTF-8 errors.
        let _ = self.sync(&[]);
    }
}
