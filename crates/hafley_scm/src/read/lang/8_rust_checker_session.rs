use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ra_ap_ide::{AnalysisHost, RootDatabase};
use ra_ap_ide_db::ChangeWithProcMacros;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, ProjectFolders, SourceRootConfig};
use ra_ap_project_model::{
    CargoConfig, CargoFeatures, ProjectManifest, ProjectWorkspace, RustLibSource,
};
use ra_ap_vfs::{Change as VfsChange, Vfs, VfsPath};

use super::rust_checker::{CheckerError, LoadMode};
use super::rust_workspace::{ManifestKey, RustWorkspace};

pub(super) struct CheckerWorkspace {
    pub(super) host: AnalysisHost,
    pub(super) vfs: Vfs,
    /// The load's file-set partition: a file the load never saw joins its
    /// source root through it.
    pub(super) roots: SourceRootConfig,
    /// Supplied files the load left out (another Cargo workspace, no crate):
    /// their absence is known, so it does not force a reload.
    outside: HashSet<PathBuf>,
}

fn unloaded(vfs: &Vfs, files: &[(String, PathBuf)]) -> HashSet<PathBuf> {
    files
        .iter()
        .map(|(_, file)| std::fs::canonicalize(file).unwrap_or_else(|_| file.clone()))
        .filter(|file| {
            vfs.file_id(&VfsPath::new_real_path(file.to_string_lossy().into_owned()))
                .is_none()
        })
        .collect()
}

type SessionKey = (PathBuf, LoadMode, ManifestKey);

static CHECKER_WORKSPACES: OnceLock<Mutex<HashMap<SessionKey, Arc<Mutex<CheckerWorkspace>>>>> =
    OnceLock::new();

pub fn warm_workspace_available(root: &Path, tier: LoadMode) -> bool {
    let Ok(discovered) = super::rust_workspace::discover(root) else {
        return false;
    };
    let Ok(manifests) = discovered.manifest_key() else {
        return false;
    };
    let root = discovered
        .metadata
        .workspace_root
        .as_std_path()
        .to_path_buf();
    CHECKER_WORKSPACES.get().is_some_and(|workspaces| {
        workspaces
            .lock()
            .unwrap()
            .contains_key(&(root, tier, manifests))
    })
}

pub(super) fn checker_workspace(
    root: &Path,
    tier: LoadMode,
    files: &[(String, PathBuf)],
    budget: Duration,
) -> Result<(Arc<Mutex<CheckerWorkspace>>, Duration), CheckerError> {
    let discovered = super::rust_workspace::discover(root).map_err(|error| CheckerError::NoWorkspace(error.to_string()))?;
    checker_workspace_loaded(&discovered, tier, files, budget)
}

pub(super) fn checker_workspace_loaded(
    discovered: &RustWorkspace,
    tier: LoadMode,
    files: &[(String, PathBuf)],
    budget: Duration,
) -> Result<(Arc<Mutex<CheckerWorkspace>>, Duration), CheckerError> {
    let root = discovered
        .metadata
        .workspace_root
        .as_std_path()
        .to_path_buf();
    let mut key = (
        root.clone(),
        tier,
        discovered
            .manifest_key()
            .map_err(CheckerError::NoWorkspace)?,
    );
    let workspaces = CHECKER_WORKSPACES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut all = workspaces.lock().unwrap();
    // Evict a changed manifest identity before loading its replacement.
    all.retain(|(cached_root, _, cached_key), _| cached_root != &root || cached_key == &key.2);
    let mut load = Duration::ZERO;
    let fresh = !all.contains_key(&key);
    if fresh {
        let (db, vfs, roots, elapsed) =
            load_checker_workspace(&root, tier, budget, &discovered.metadata)?;
        load = elapsed;
        key.2 = discovered
            .manifest_key()
            .map_err(CheckerError::NoWorkspace)?;
        all.insert(
            key.clone(),
            Arc::new(Mutex::new(CheckerWorkspace {
                host: AnalysisHost::with_database(db),
                outside: unloaded(&vfs, files),
                vfs,
                roots,
            })),
        );
    }
    let handle = all.get(&key).unwrap().clone();
    drop(all);
    let mut workspace = handle.lock().unwrap();
    if tier == LoadMode::Types
        && !fresh
        && !unloaded(&workspace.vfs, files).is_subset(&workspace.outside)
    {
        let (db, vfs, roots, elapsed) =
            load_checker_workspace(&root, tier, budget, &discovered.metadata)?;
        workspace.host = AnalysisHost::with_database(db);
        workspace.outside = unloaded(&vfs, files);
        workspace.vfs = vfs;
        workspace.roots = roots;
        load = elapsed;
    }
    let refresh: HashSet<PathBuf> = files
        .iter()
        .map(|(_, file)| file.clone())
        .chain(workspace.vfs.iter().filter_map(|(_, path)| {
            let path = PathBuf::from(path.as_path()?.to_string());
            ((tier == LoadMode::Names || path.starts_with(&root))
                && path.extension().is_some_and(|extension| extension == "rs"))
            .then_some(path)
        }))
        .collect();
    for file in refresh {
        let file = std::fs::canonicalize(&file).unwrap_or(file);
        let path = VfsPath::new_real_path(file.to_string_lossy().into_owned());
        if tier == LoadMode::Types && workspace.vfs.file_id(&path).is_none() {
            continue;
        }
        let contents = match std::fs::read(crate::read::io_path(&file)) {
            Ok(contents) => Some(contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(CheckerError::NoWorkspace(error.to_string())),
        };
        workspace.vfs.set_file_contents(path, contents);
    }
    workspace.apply_vfs_changes()?;
    drop(workspace);
    Ok((handle, load))
}

impl CheckerWorkspace {
    /// The vfs's pending changes into the host, one salsa change. A created
    /// file re-partitions the source roots so its crate's def map sees it.
    pub(super) fn apply_vfs_changes(&mut self) -> Result<(), CheckerError> {
        let changed = self.vfs.take_changes();
        if changed.is_empty() {
            return Ok(());
        }
        let mut change = ChangeWithProcMacros::default();
        let mut created = false;
        for (file, changed) in changed {
            created |= matches!(changed.change, VfsChange::Create(..));
            let text = match changed.change {
                VfsChange::Create(contents, _) | VfsChange::Modify(contents, _) => Some(
                    String::from_utf8(contents)
                        .map_err(|error| CheckerError::NoWorkspace(error.to_string()))?,
                ),
                VfsChange::Delete => None,
            };
            change.change_file(ra_ap_ide::FileId::from_raw(file.index()), text);
        }
        if created {
            change.set_roots(self.roots.partition(&self.vfs));
        }
        self.host.apply_change(change);
        Ok(())
    }
}

#[cfg(test)]
mod warm_workspace_tests {
    use super::super::rust_checker_ra::{answer, field_reads, FieldProbe, FieldRead};
    use super::*;

    #[test]
    fn warm_field_reads_match_cold_after_file_change() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sprefa-extract/tests/fixtures/cleave_ratchet/private_fields");
        // Outside every Cargo workspace, or the fixture's manifest joins the enclosing one.
        let scratch = tempfile::TempDir::new().unwrap();
        let root = scratch.path().to_path_buf();
        std::fs::create_dir_all(root.join("src")).unwrap();
        for path in ["Cargo.toml", "src/lib.rs", "src/source.rs", "src/dest.rs"] {
            let destination = root.join(path);
            std::fs::copy(fixture.join(path), destination).unwrap();
        }
        let root = std::fs::canonicalize(root).unwrap();
        let source = root.join("src/source.rs");
        let files = ["src/lib.rs", "src/source.rs", "src/dest.rs"]
            .into_iter()
            .map(|path| (path.to_string(), root.join(path)))
            .collect::<Vec<_>>();
        let original = std::fs::read_to_string(&source).unwrap();
        let probes = [FieldProbe {
            struct_name_start: original.find("Packet").unwrap() as u32,
            field_start: original.find("value").unwrap() as u32,
            field_name: "value".into(),
        }];
        let budget = Duration::from_secs(120);
        let cold = field_reads(&root, &source, &files, &probes, budget).unwrap();
        let changed = format!(
            "{original}\npub(crate) fn read_again() -> u8 {{ Packet {{ value: 8 }}.value }}\n"
        );
        std::fs::write(&source, changed).unwrap();
        let warm = field_reads(&root, &source, &files, &probes, budget).unwrap();
        CHECKER_WORKSPACES.get().unwrap().lock().unwrap().retain(
            |(cached_root, cached_mode, _), _| {
                cached_root != &root || *cached_mode != LoadMode::Types
            },
        );
        let fresh = field_reads(&root, &source, &files, &probes, budget).unwrap();
        let sites = |reads: Vec<FieldRead>| {
            reads
                .into_iter()
                .map(|read| (read.path, read.field_start, read.access_start))
                .collect::<Vec<_>>()
        };
        assert!(warm.len() > cold.len());
        assert_eq!(sites(warm), sites(fresh));
        let warm_graph = answer(&root, &files, budget, true).unwrap();
        CHECKER_WORKSPACES.get().unwrap().lock().unwrap().retain(
            |(cached_root, cached_mode, _), _| {
                cached_root != &root || *cached_mode != LoadMode::Types
            },
        );
        let cold_graph = answer(&root, &files, budget, true).unwrap();
        assert_eq!(
            serde_json::to_value(warm_graph.tsi).unwrap(),
            serde_json::to_value(cold_graph.tsi).unwrap()
        );
        CHECKER_WORKSPACES.get().unwrap().lock().unwrap().retain(
            |(cached_root, cached_mode, _), _| {
                cached_root != &root || *cached_mode != LoadMode::Types
            },
        );
    }
}

fn load_checker_workspace(
    root: &Path,
    tier: LoadMode,
    budget: Duration,
    metadata: &cargo_metadata::Metadata,
) -> Result<(RootDatabase, ra_ap_vfs::Vfs, SourceRootConfig, Duration), CheckerError> {
    let load_config = LoadCargoConfig {
        load_out_dirs_from_check: false,
        with_proc_macro_server: ProcMacroServerChoice::None,
        prefill_caches: false,
        num_worker_threads: 4,
        proc_macro_processes: 0,
    };
    let started = Instant::now();
    let _load_span =
        crate::read::trace::tracked(tracing::debug_span!("rust_analyzer.load")).entered();
    fn no_workspace(error: impl std::fmt::Display) -> CheckerError {
        CheckerError::NoWorkspace(error.to_string())
    }
    let workspace = match tier {
        LoadMode::Names => {
            let project = super::rust_checker_project::names_project(root, metadata)?;
            ProjectWorkspace::load_inline(project, &CargoConfig::default(), &|_| {})
        }
        // `set_test` puts `#[cfg(test)]` bodies in the tree; every feature keeps
        // `cfg`-gated modules in the crate graph.
        LoadMode::Types => {
            let cargo_config = CargoConfig {
                sysroot: Some(RustLibSource::Discover),
                set_test: true,
                features: CargoFeatures::All,
                ..CargoConfig::default()
            };
            let root = ra_ap_vfs::AbsPathBuf::assert_utf8(root.to_path_buf());
            let manifest = ProjectManifest::discover_single(&root).map_err(no_workspace)?;
            ProjectWorkspace::load(manifest, &cargo_config, &|_| {}).map_err(no_workspace)?
        }
    };
    let roots = ProjectFolders::new(std::slice::from_ref(&workspace), &[], None).source_root_config;
    let (db, vfs, _proc_macro) =
        ra_ap_load_cargo::load_workspace(workspace, &Default::default(), &load_config)
            .map_err(no_workspace)?;
    drop(_load_span);
    let load = started.elapsed();
    if load > budget {
        return Err(CheckerError::Budget(budget));
    }
    Ok((db, vfs, roots, load))
}
