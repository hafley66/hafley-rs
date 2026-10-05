//! rust-analyzer's module tree from the crate def maps alone: a file's crate,
//! module path and declaring `mod` item. No body is type checked.

use super::*;
use std::sync::{Arc, Mutex};
use ra_ap_vfs::VfsPath;

use super::super::rust_checker_session::{checker_workspace, CheckerWorkspace};

/// One module a file is. A file several targets include has one per target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModulePlace {
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
    workspace: Arc<Mutex<CheckerWorkspace>>,
    /// Absolute path -> the text the host holds in place of the disk's.
    staged: Mutex<HashMap<PathBuf, String>>,
}

/// Cargo selects the source's workspace from its directory; rust-analyzer
/// supplies module ownership from that workspace's def maps.
pub fn module_tree(source: &Path, budget: Duration) -> Result<RustModuleTree, CheckerError> {
    let discovered = super::super::rust_workspace::discover(source)
        .map_err(CheckerError::NoWorkspace)?;
    let manifest = discovered.manifest;
    let metadata = discovered.metadata;
    let root = metadata.workspace_root.as_std_path();
    let (workspace, _) = checker_workspace(root, super::super::rust_checker::Tier::Names, &[], budget)?;
    Ok(RustModuleTree {
        manifest,
        workspace,
        staged: Mutex::new(HashMap::new()),
    })
}

/// `path` canonical; an unborn file through its parent directory.
fn host_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| {
        match (path.parent().and_then(|dir| std::fs::canonicalize(dir).ok()), path.file_name()) {
            (Some(dir), Some(name)) => dir.join(name),
            _ => path.to_path_buf(),
        }
    })
}

fn vfs_path(path: &Path) -> VfsPath {
    VfsPath::new_real_path(path.to_string_lossy().into_owned())
}

impl RustModuleTree {
    /// The host holds exactly `texts` over the disk; an earlier staged path
    /// `texts` omits gets the disk's text back.
    pub fn sync(&self, texts: &[(PathBuf, String)]) -> Result<(), CheckerError> {
        let mut workspace = self.workspace.lock().unwrap();
        let mut staged = self.staged.lock().unwrap();
        let wanted: HashMap<PathBuf, &String> =
            texts.iter().map(|(path, text)| (host_path(path), text)).collect();
        for (path, text) in &wanted {
            if staged.get(path) != Some(*text) {
                workspace.vfs.set_file_contents(vfs_path(path), Some(text.as_bytes().to_vec()));
                staged.insert(path.clone(), (*text).clone());
            }
        }
        let released: Vec<PathBuf> =
            staged.keys().filter(|path| !wanted.contains_key(*path)).cloned().collect();
        for path in released {
            workspace.vfs.set_file_contents(vfs_path(&path), std::fs::read(crate::read::io_path(&path)).ok());
            staged.remove(&path);
        }
        workspace.apply_vfs_changes()
    }

    /// Whether the destination module path and item are visible to a source module.
    pub fn can_name(&self, from: &Path, to: &Path, item: &str) -> Option<bool> {
        use ra_ap_hir::HasVisibility;
        let workspace = self.workspace.lock().unwrap();
        let (from, _) = workspace.vfs.file_id(&vfs_path(&host_path(from)))?;
        let (to, _) = workspace.vfs.file_id(&vfs_path(&host_path(to)))?;
        let db = workspace.host.raw_database();
        attach_db(db, || {
            let sema = Semantics::new(db);
            let viewers: Vec<_> = sema.file_to_module_defs(ra_ap_ide::FileId::from_raw(from.index())).collect();
            let targets: Vec<_> = sema.file_to_module_defs(ra_ap_ide::FileId::from_raw(to.index())).collect();
            if viewers.is_empty() || targets.is_empty() { return None; }
            Some(viewers.iter().all(|viewer| targets.iter().any(|target| {
                target.path_to_root(db).into_iter().all(|module| module.is_visible_from(db, *viewer))
                    && target.scope(db, Some(*viewer)).iter().any(|(name, _)| name.as_str() == item)
            })))
        })
    }

    /// Every module the def maps place `file` at, in crate-root order.
    pub fn places(&self, file: &Path) -> Vec<ModulePlace> {
        let workspace = self.workspace.lock().unwrap();
        let Some((id, _)) = workspace.vfs.file_id(&vfs_path(&host_path(file))) else {
            return Vec::new();
        };
        let path_of = |file: ra_ap_ide::FileId| {
            workspace
                .vfs
                .file_path(ra_ap_vfs::FileId::from_raw(file.index()))
                .as_path()
                .map(|path| PathBuf::from(path.to_string()))
        };
        let db = workspace.host.raw_database();
        let mut places: Vec<ModulePlace> = attach_db(db, || {
            let sema = Semantics::new(db);
            sema.file_to_module_defs(ra_ap_ide::FileId::from_raw(id.index()))
                .filter_map(|module| {
                    let krate = module.krate(db);
                    let crate_root = path_of(krate.root_file(db))?;
                    let decl = module.declaration_source_range(db).and_then(|range| {
                        let file = range.file_id.file_id()?.file_id(db);
                        Some((
                            path_of(file)?,
                            u32::from(range.value.start()),
                            u32::from(range.value.end()),
                        ))
                    });
                    Some(ModulePlace {
                        crate_root,
                        crate_name: crate_name(db, krate),
                        path: module
                            .path_segments(db)
                            .map(|name| name.as_str().to_string())
                            .collect(),
                        decl,
                    })
                })
                .collect()
        });
        places.sort_by(|a, b| a.crate_root.cmp(&b.crate_root));
        places
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

#[cfg(test)]
mod names_measurement {
    use super::*;

    #[test]
    #[ignore = "one measurement run on the repository workspaces"]
    fn cold_and_warm_names_hosts() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap();
        println!("workspace\tstate\tload_seconds\tquery_seconds\tplaces");
        for (name, source) in [
            ("hafley-rs", repo.join("crates/hafley_scm/src/lib.rs")),
            ("sprefa-extract", repo.join("crates/sprefa-extract/src/lib.rs")),
        ] {
            for state in ["cold", "warm"] {
                let started = Instant::now();
                let tree = module_tree(&source, Duration::from_secs(120)).unwrap();
                let load = started.elapsed();
                let started = Instant::now();
                let places = tree.places(&source);
                println!("{name}\t{state}\t{:.6}\t{:.6}\t{}", load.as_secs_f64(), started.elapsed().as_secs_f64(), places.len());
                assert!(!places.is_empty());
            }
        }
    }
}
