use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ra_ap_ide::{AnalysisHost, RootDatabase};
use ra_ap_ide_db::ChangeWithProcMacros;
use ra_ap_load_cargo::{LoadCargoConfig, ProcMacroServerChoice, ProjectFolders, SourceRootConfig};
use ra_ap_project_model::{
    CargoConfig, CargoFeatures, ProjectJson, ProjectWorkspace, ProjectWorkspaceKind, RustLibSource,
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
    pub(super) names: super::rust_checker_ra::names_cache::NamesCache,
    /// Supplied files the load left out (another Cargo workspace, no crate):
    /// their absence is known, so it does not force a reload.
    outside: HashSet<PathBuf>,
    used: Instant,
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

/// Hosts kept warm per load mode, least recently used evicted first. A Types
/// host holds a whole crate graph plus the sysroot, about a gigabyte. A Names
/// host is light (35 of them, sprefa-extract's fixture crates plus two cargo
/// workspaces, measured about 100 MB), and one request over a tree with fixture
/// crates opens one per nearest manifest. `RYI_WARM_WORKSPACES` overrides both.
const WARM_TYPES: usize = 1;
const WARM_NAMES: usize = 64;

/// A workspace no request touched for this long is dropped by the reaper.
/// `RYI_WARM_IDLE_SECS` overrides it.
const WARM_IDLE: Duration = Duration::from_secs(60);

fn warm_workspaces(tier: LoadMode) -> usize {
    std::env::var("RYI_WARM_WORKSPACES")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(match tier {
            LoadMode::Types => WARM_TYPES,
            LoadMode::Names => WARM_NAMES,
        })
}

fn warm_idle() -> Duration {
    std::env::var("RYI_WARM_IDLE_SECS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .map_or(WARM_IDLE, Duration::from_secs)
}
const REAP_EVERY: Duration = Duration::from_secs(15);

/// Loads still running on their own thread, including ones past their budget:
/// the global type interner may only be collected while this is zero.
static LOADS_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
static REAPER_RUNNING: AtomicBool = AtomicBool::new(false);

/// Drop every cached workspace no other request holds and that `keep` refuses.
/// Once the map is empty, nothing is loading, and every dropped host had no
/// other holder, no rust-analyzer query can be running, so the global type
/// interner is collected too.
fn evict(
    all: &mut HashMap<SessionKey, Arc<Mutex<CheckerWorkspace>>>,
    keep: impl Fn(&SessionKey, &CheckerWorkspace) -> bool,
) {
    let before = all.len();
    all.retain(|key, workspace| {
        Arc::strong_count(workspace) > 1
            || workspace
                .try_lock()
                .map_or(true, |workspace| keep(key, &workspace))
    });
    if all.len() < before && all.is_empty() && LOADS_IN_FLIGHT.load(Ordering::Acquire) == 0 {
        // SAFETY: no cached host remains, every evicted host had no other
        // holder, and no load thread is running, so no query is live.
        unsafe { ra_ap_hir::collect_ty_garbage() };
        tracing::info!(evicted = before, "rust checker workspaces dropped, type interner collected");
    }
}

/// Wakes every `REAP_EVERY` and drops workspaces idle past `warm_idle()`; the
/// thread ends once the map is empty, so an idle process holds no host.
fn start_reaper() {
    if REAPER_RUNNING.swap(true, Ordering::AcqRel) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("rust-checker-reaper".into())
        .spawn(|| loop {
            std::thread::sleep(REAP_EVERY);
            let Some(workspaces) = CHECKER_WORKSPACES.get() else {
                REAPER_RUNNING.store(false, Ordering::Release);
                return;
            };
            let mut all = workspaces.lock().unwrap();
            evict(&mut all, |_, workspace| workspace.used.elapsed() < warm_idle());
            if all.is_empty() {
                REAPER_RUNNING.store(false, Ordering::Release);
                return;
            }
        });
    if spawned.is_err() {
        REAPER_RUNNING.store(false, Ordering::Release);
    }
}

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
        // Least recently used first, one at a time, until the new load fits;
        // a host another request holds is never a candidate.
        let cap = warm_workspaces(tier);
        while all.keys().filter(|(_, mode, _)| *mode == tier).count() >= cap {
            let Some(oldest) = all
                .iter()
                .filter(|((_, mode, _), workspace)| *mode == tier && Arc::strong_count(workspace) == 1)
                .filter_map(|(key, workspace)| {
                    workspace.try_lock().ok().map(|workspace| (key.clone(), workspace.used))
                })
                .min_by_key(|(_, used)| *used)
                .map(|(key, _)| key)
            else {
                break;
            };
            evict(&mut all, |key, _| key != &oldest);
        }
        let (db, vfs, roots, elapsed) =
            load_checker_workspace(&root, tier, budget, &discovered.metadata, files)?;
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
                names: Default::default(),
                used: Instant::now(),
            })),
        );
        start_reaper();
    }
    let handle = all.get(&key).unwrap().clone();
    drop(all);
    let mut workspace = handle.lock().unwrap();
    workspace.used = Instant::now();
    if tier == LoadMode::Types
        && !fresh
        && !unloaded(&workspace.vfs, files).is_subset(&workspace.outside)
    {
        let (db, vfs, roots, elapsed) =
            load_checker_workspace(&root, tier, budget, &discovered.metadata, files)?;
        workspace.host = AnalysisHost::with_database(db);
        workspace.outside = unloaded(&vfs, files);
        workspace.vfs = vfs;
        workspace.roots = roots;
        workspace.names = Default::default();
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
        self.names = Default::default();
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

#[cfg(test)]
mod warm_bound_tests {
    use super::*;

    fn scratch_root(scratch: &tempfile::TempDir) -> (PathBuf, Vec<(String, PathBuf)>) {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sprefa-extract/tests/fixtures/cleave_ratchet/private_fields");
        let root = scratch.path().to_path_buf();
        std::fs::create_dir_all(root.join("src")).unwrap();
        for path in ["Cargo.toml", "src/lib.rs", "src/source.rs", "src/dest.rs"] {
            std::fs::copy(fixture.join(path), root.join(path)).unwrap();
        }
        let root = std::fs::canonicalize(root).unwrap();
        let files = ["src/lib.rs", "src/source.rs", "src/dest.rs"]
            .into_iter()
            .map(|path| (path.to_string(), root.join(path)))
            .collect();
        (root, files)
    }

    fn cached_under(roots: &[&PathBuf]) -> usize {
        CHECKER_WORKSPACES.get().map_or(0, |workspaces| {
            workspaces
                .lock()
                .unwrap()
                .keys()
                .filter(|(root, _, _)| roots.contains(&root))
                .count()
        })
    }

    #[test]
    fn a_second_root_evicts_the_first_and_idle_eviction_empties_the_map() {
        let (first_dir, second_dir) = (tempfile::TempDir::new().unwrap(), tempfile::TempDir::new().unwrap());
        let (first, first_files) = scratch_root(&first_dir);
        let (second, second_files) = scratch_root(&second_dir);
        let budget = Duration::from_secs(120);
        checker_workspace(&first, LoadMode::Names, &first_files, budget).unwrap();
        assert_eq!(cached_under(&[&first, &second]), 1);
        checker_workspace(&second, LoadMode::Names, &second_files, budget).unwrap();
        assert_eq!(cached_under(&[&first]), 0, "the first root's host stayed loaded");
        assert_eq!(cached_under(&[&second]), 1);
        let mut all = CHECKER_WORKSPACES.get().unwrap().lock().unwrap();
        evict(&mut all, |(root, _, _), _| root != &second);
        drop(all);
        assert_eq!(cached_under(&[&first, &second]), 0);
    }
}

/// The toolchain a directory resolves to: the nearest `rust-toolchain` file
/// and its text, and `RUSTUP_TOOLCHAIN`.
type ToolchainKey = (Option<(PathBuf, String)>, Option<std::ffi::OsString>);

/// One Names workspace per toolchain, its `kind` swapped per load. A Names
/// project carries no sysroot (`names_project`), so `load_inline` differs per
/// workspace only in `kind`; the rest is four `rustc` spawns (cfg, target data,
/// target tuple, version) that every fixture crate of a tree repeated.
static NAMES_TEMPLATES: OnceLock<Mutex<HashMap<ToolchainKey, ProjectWorkspace>>> = OnceLock::new();

fn toolchain_key(root: &Path) -> ToolchainKey {
    let file = root.ancestors().find_map(|directory| {
        ["rust-toolchain.toml", "rust-toolchain"].iter().find_map(|name| {
            let path = directory.join(name);
            std::fs::read_to_string(&path).ok().map(|text| (path, text))
        })
    });
    (file, std::env::var_os("RUSTUP_TOOLCHAIN"))
}

fn names_workspace(project: ProjectJson, root: &Path) -> ProjectWorkspace {
    let key = toolchain_key(root);
    let templates = NAMES_TEMPLATES.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(template) = templates.lock().unwrap().get(&key) {
        let mut workspace = template.clone();
        workspace.kind = ProjectWorkspaceKind::Json(project);
        return workspace;
    }
    let workspace = ProjectWorkspace::load_inline(project, &CargoConfig::default(), &|_| {});
    templates.lock().unwrap().insert(key, workspace.clone());
    workspace
}

fn load_checker_workspace(
    root: &Path,
    tier: LoadMode,
    budget: Duration,
    metadata: &cargo_metadata::Metadata,
    files: &[(String, PathBuf)],
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
            names_workspace(project, root)
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
            let project = super::rust_checker_project::types_project(root, files)?;
            ProjectWorkspace::load_inline(project, &cargo_config, &|_| {})
        }
    };
    let roots = ProjectFolders::new(std::slice::from_ref(&workspace), &[], None).source_root_config;
    // The load runs on its own thread so the budget stops the caller mid-load; a load past the
    // budget finishes in the background and is dropped.
    let (sender, receiver) = std::sync::mpsc::channel();
    LOADS_IN_FLIGHT.fetch_add(1, Ordering::AcqRel);
    std::thread::Builder::new()
        .name("rust-analyzer-load".into())
        .spawn(move || {
            let loaded = ra_ap_load_cargo::load_workspace(workspace, &Default::default(), &load_config);
            let _ = sender.send(loaded);
            LOADS_IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
        })
        .map_err(|error| {
            LOADS_IN_FLIGHT.fetch_sub(1, Ordering::AcqRel);
            no_workspace(error)
        })?;
    let remaining = budget.saturating_sub(started.elapsed());
    let (db, vfs, _proc_macro) = match receiver.recv_timeout(remaining) {
        Ok(loaded) => loaded.map_err(no_workspace)?,
        Err(_) => return Err(CheckerError::Budget(budget)),
    };
    drop(_load_span);
    let load = started.elapsed();
    Ok((db, vfs, roots, load))
}
