use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ra_ap_ide::{AnalysisHost, RootDatabase};
use ra_ap_ide_db::ChangeWithProcMacros;
use ra_ap_load_cargo::{load_workspace_at, LoadCargoConfig, ProcMacroServerChoice};
use ra_ap_project_model::{CargoConfig, CargoFeatures, RustLibSource};
use ra_ap_vfs::{Change as VfsChange, Vfs, VfsPath};

use super::rust_checker::{CheckerError, Tier};

pub(super) struct CheckerWorkspace {
    pub(super) host: AnalysisHost,
    pub(super) vfs: Vfs,
}

type SessionKey = (PathBuf, Tier);

static CHECKER_WORKSPACES: OnceLock<Mutex<HashMap<SessionKey, Arc<Mutex<CheckerWorkspace>>>>> =
    OnceLock::new();

pub fn warm_workspace_available(root: &Path, tier: Tier) -> bool {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    CHECKER_WORKSPACES
        .get()
        .is_some_and(|workspaces| workspaces.lock().unwrap().contains_key(&(root, tier)))
}

pub(super) fn checker_workspace(
    root: &Path,
    tier: Tier,
    files: &[(String, PathBuf)],
    budget: Duration,
) -> Result<(Arc<Mutex<CheckerWorkspace>>, Duration), CheckerError> {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let key = (root.clone(), tier);
    let workspaces = CHECKER_WORKSPACES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut all = workspaces.lock().unwrap();
    let mut load = Duration::ZERO;
    let fresh = !all.contains_key(&key);
    if fresh {
        let (db, vfs, elapsed) = load_checker_workspace(&root, tier, budget)?;
        load = elapsed;
        all.insert(
            key.clone(),
            Arc::new(Mutex::new(CheckerWorkspace {
                host: AnalysisHost::with_database(db),
                vfs,
            })),
        );
    }
    let handle = all.get(&key).unwrap().clone();
    drop(all);
    let mut workspace = handle.lock().unwrap();
    if !fresh && files.iter().any(|(_, file)| {
        let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.clone());
        workspace
            .vfs
            .file_id(&VfsPath::new_real_path(file.to_string_lossy().into_owned()))
            .is_none()
    }) {
        let (db, vfs, elapsed) = load_checker_workspace(&root, tier, budget)?;
        workspace.host = AnalysisHost::with_database(db);
        workspace.vfs = vfs;
        load = elapsed;
    }
    for (_, file) in files {
        let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.clone());
        let path = VfsPath::new_real_path(file.to_string_lossy().into_owned());
        if workspace.vfs.file_id(&path).is_none() {
            continue;
        }
        let contents = match std::fs::read(&file) {
            Ok(contents) => Some(contents),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(CheckerError::NoWorkspace(error.to_string())),
        };
        workspace.vfs.set_file_contents(path, contents);
    }
    let changed = workspace.vfs.take_changes();
    if !changed.is_empty() {
        let mut change = ChangeWithProcMacros::default();
        for (file, changed) in changed {
            let text = match changed.change {
                VfsChange::Create(contents, _) | VfsChange::Modify(contents, _) => Some(
                    String::from_utf8(contents)
                        .map_err(|error| CheckerError::NoWorkspace(error.to_string()))?,
                ),
                VfsChange::Delete => None,
            };
            change.change_file(ra_ap_ide::FileId::from_raw(file.index()), text);
        }
        workspace.host.apply_change(change);
    }
    drop(workspace);
    Ok((handle, load))
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
        CHECKER_WORKSPACES
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .remove(&(root.clone(), Tier::Slow));
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
        CHECKER_WORKSPACES
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .remove(&(root.clone(), Tier::Slow));
        let cold_graph = answer(&root, &files, budget, true).unwrap();
        assert_eq!(
            serde_json::to_value(warm_graph.tsi).unwrap(),
            serde_json::to_value(cold_graph.tsi).unwrap()
        );
        CHECKER_WORKSPACES
            .get()
            .unwrap()
            .lock()
            .unwrap()
            .remove(&(root.clone(), Tier::Slow));
    }
}

fn load_checker_workspace(
    root: &Path,
    tier: Tier,
    budget: Duration,
) -> Result<(RootDatabase, ra_ap_vfs::Vfs, Duration), CheckerError> {
    let load_config = LoadCargoConfig {
        load_out_dirs_from_check: false,
        with_proc_macro_server: ProcMacroServerChoice::None,
        prefill_caches: false,
        num_worker_threads: 4,
        proc_macro_processes: 0,
    };
    let started = Instant::now();
    let _load_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.load")).entered();
    let (db, vfs, _proc_macro) = match tier {
        Tier::Fast => {
            let project = super::rust_checker_project::fast_project(root)?;
            let workspace = ra_ap_project_model::ProjectWorkspace::load_inline(project, &CargoConfig::default(), &|_| {});
            ra_ap_load_cargo::load_workspace(workspace, &Default::default(), &load_config)
        }
        // `set_test` puts `#[cfg(test)]` bodies in the tree; every feature keeps
        // `cfg`-gated modules in the crate graph.
        Tier::Slow => {
            let cargo_config = CargoConfig {
                sysroot: Some(RustLibSource::Discover),
                set_test: true,
                features: CargoFeatures::All,
                ..CargoConfig::default()
            };
            load_workspace_at(root, &cargo_config, &load_config, &|_| {})
        }
    }
    .map_err(|error| CheckerError::NoWorkspace(error.to_string()))?;
    drop(_load_span);
    let load = started.elapsed();
    if load > budget {
        return Err(CheckerError::Budget(budget));
    }
    Ok((db, vfs, load))
}

#[cfg(test)]
mod fast_tier_tests {
    use super::*;
    use ra_ap_ide_db::base_db;

    #[test]
    fn fast_loads_workspace_crates_with_features_and_no_sysroot() {
        let root = std::fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
        let (db, vfs, _) = load_checker_workspace(&root, Tier::Fast, Duration::from_secs(120)).unwrap();
        let names: Vec<String> = base_db::all_crates(&db)
            .iter()
            .filter_map(|krate| krate.extra_data(&db).display_name.as_ref().map(|name| name.to_string()))
            .collect();
        assert!(names.iter().any(|name| name == "hafley_scm"));
        assert!(!names.iter().any(|name| ["core", "std", "alloc"].contains(&name.as_str())));
        assert!(!names.iter().any(|name| name == "serde_json"));
        let trace = VfsPath::new_real_path(root.join("crates/hafley_scm/src/read/trace.rs").to_string_lossy().into_owned());
        let trace = ra_ap_ide::FileId::from_raw(vfs.file_id(&trace).unwrap().0.index());
        assert!(!base_db::relevant_crates(&db, trace).is_empty());
    }
}
