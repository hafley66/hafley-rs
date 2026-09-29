use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use ra_ap_ide::{AnalysisHost, RootDatabase};
use ra_ap_ide_db::ChangeWithProcMacros;
use ra_ap_load_cargo::{load_workspace_at, LoadCargoConfig, ProcMacroServerChoice};
use ra_ap_project_model::{CargoConfig, CargoFeatures, RustLibSource};
use ra_ap_vfs::{Change as VfsChange, Vfs, VfsPath};

use super::rust_checker::CheckerError;

pub(super) struct CheckerWorkspace {
    pub(super) host: AnalysisHost,
    pub(super) vfs: Vfs,
}

static CHECKER_WORKSPACES: OnceLock<Mutex<HashMap<PathBuf, Arc<Mutex<CheckerWorkspace>>>>> =
    OnceLock::new();

pub fn warm_workspace_available(root: &Path) -> bool {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    CHECKER_WORKSPACES
        .get()
        .is_some_and(|workspaces| workspaces.lock().unwrap().contains_key(&root))
}

pub(super) fn checker_workspace(
    root: &Path,
    files: &[(String, PathBuf)],
    budget: Duration,
) -> Result<(Arc<Mutex<CheckerWorkspace>>, Duration), CheckerError> {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let workspaces = CHECKER_WORKSPACES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut all = workspaces.lock().unwrap();
    let mut load = Duration::ZERO;
    if !all.contains_key(&root) {
        let (db, vfs, elapsed) = load_checker_workspace(&root, budget)?;
        load = elapsed;
        all.insert(
            root.clone(),
            Arc::new(Mutex::new(CheckerWorkspace {
                host: AnalysisHost::with_database(db),
                vfs,
            })),
        );
    }
    let handle = all.get(&root).unwrap().clone();
    drop(all);
    let mut workspace = handle.lock().unwrap();
    if files.iter().any(|(_, file)| {
        let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.clone());
        workspace
            .vfs
            .file_id(&VfsPath::new_real_path(file.to_string_lossy().into_owned()))
            .is_none()
    }) {
        let (db, vfs, elapsed) = load_checker_workspace(&root, budget)?;
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
        let target = std::env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("target"));
        let root = target.join(format!("warm-checker-{}", std::process::id()));
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
            .remove(&root);
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
            .remove(&root);
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
            .remove(&root);
        std::fs::remove_dir_all(root).unwrap();
    }
}

fn load_checker_workspace(
    root: &Path,
    budget: Duration,
) -> Result<(RootDatabase, ra_ap_vfs::Vfs, Duration), CheckerError> {
    let load_config = LoadCargoConfig {
        load_out_dirs_from_check: false,
        with_proc_macro_server: ProcMacroServerChoice::None,
        prefill_caches: false,
        num_worker_threads: 4,
        proc_macro_processes: 0,
    };
    // A crate graph with no sysroot declines every method whose receiver type
    // flows through std; `set_test` puts `#[cfg(test)]` bodies in the tree.
    let cargo_config = CargoConfig {
        sysroot: Some(RustLibSource::Discover),
        set_test: true,
        // The default selects no feature, so a `cfg`-gated module stays out of
        // the crate graph and every file it declares owns no module there.
        features: CargoFeatures::All,
        ..CargoConfig::default()
    };
    let started = Instant::now();
    let _load_span = tracing::info_span!("rust_analyzer.load").entered();
    let (db, vfs, _proc_macro) = load_workspace_at(root, &cargo_config, &load_config, &|_| {})
        .map_err(|error| CheckerError::NoWorkspace(error.to_string()))?;
    drop(_load_span);
    let load = started.elapsed();
    if load > budget {
        return Err(CheckerError::Budget(budget));
    }
    Ok((db, vfs, load))
}
