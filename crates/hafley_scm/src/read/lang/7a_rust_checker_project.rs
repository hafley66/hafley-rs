//! Fast tier crate graph: workspace targets, the edges cargo resolves between
//! them (`[patch]` included), and rust-src's `core`, as a rust-project.json.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use cargo_metadata::{DependencyKind, MetadataCommand, PackageId, TargetKind};
use ra_ap_project_model::{ProjectJson, ProjectJsonData};
use ra_ap_vfs::AbsPathBuf;

use super::rust_checker::CheckerError;

fn is_library(kind: &TargetKind) -> bool {
    matches!(
        kind,
        TargetKind::Lib
            | TargetKind::RLib
            | TargetKind::DyLib
            | TargetKind::CDyLib
            | TargetKind::StaticLib
            | TargetKind::ProcMacro
    )
}

/// rust-analyzer's own minimal `core` (its test fixture), written once to the user cache.
fn minicore() -> Result<PathBuf, CheckerError> {
    let failed = |error: std::io::Error| CheckerError::NoWorkspace(error.to_string());
    let path = dirs::cache_dir()
        .ok_or_else(|| CheckerError::NoWorkspace("no user cache directory".to_owned()))?
        .join("hafley/ra-minicore-0.0.352/core/src/lib.rs");
    let source = ra_ap_test_utils::MiniCore::from_flags([
        "iterator", "iterators", "try", "future", "async_fn", "range", "option", "result", "fn",
        "deref", "index", "from", "sized", "copy", "clone", "eq", "ord", "default", "drop",
        "panic", "fmt", "derive", "slice", "str",
    ])
    .source_code(ra_ap_test_utils::MiniCore::RAW_SOURCE);
    if std::fs::read_to_string(&path).ok().as_deref() != Some(source.as_str()) {
        std::fs::create_dir_all(path.parent().unwrap()).map_err(failed)?;
        std::fs::write(&path, source).map_err(failed)?;
    }
    Ok(path)
}

pub(super) fn fast_project(root: &Path) -> Result<ProjectJson, CheckerError> {
    let failed = |error: String| CheckerError::NoWorkspace(error);
    let rustc = std::process::Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|error| failed(error.to_string()))?;
    let host = String::from_utf8_lossy(&rustc.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_owned))
        .ok_or_else(|| failed("rustc -vV printed no host".to_owned()))?;
    // Unfiltered, cargo wants every platform's manifests (wasm's `js-sys`) on disk.
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .other_options(vec!["--offline".to_owned(), "--filter-platform".to_owned(), host])
        .exec()
        .map_err(|error| failed(error.to_string()))?;
    let members: HashSet<&PackageId> = metadata.workspace_members.iter().collect();
    let resolve = metadata
        .resolve
        .as_ref()
        .ok_or_else(|| failed("cargo metadata returned no resolve graph".to_owned()))?;
    let nodes: HashMap<&PackageId, &cargo_metadata::Node> =
        resolve.nodes.iter().map(|node| (&node.id, node)).collect();

    let mut crates: Vec<serde_json::Value> = Vec::new();
    let mut library: HashMap<&PackageId, (usize, String)> = HashMap::new();
    let mut targets: Vec<(&PackageId, bool, usize)> = Vec::new();
    for package in metadata.workspace_packages() {
        // Edges come from the default-feature resolve; every feature is on as a cfg.
        let cfg: Vec<String> = package
            .features
            .keys()
            .map(|feature| format!("feature=\"{feature}\""))
            .chain(["test".to_owned(), "debug_assertions".to_owned()])
            .collect();
        let package_dir = package.manifest_path.parent().map(|dir| dir.to_string()).unwrap_or_default();
        for target in &package.targets {
            if target.kind.iter().any(|kind| matches!(kind, TargetKind::CustomBuild)) {
                continue;
            }
            let is_lib = target.kind.iter().any(is_library);
            let index = crates.len();
            let source_dir = target.src_path.parent().map(|dir| dir.to_string()).unwrap_or_default();
            crates.push(serde_json::json!({
                "display_name": target.name,
                "root_module": target.src_path,
                "edition": target.edition.as_str(),
                "deps": [],
                "cfg": cfg,
                "is_workspace_member": true,
                "is_proc_macro": target.kind.iter().any(|kind| matches!(kind, TargetKind::ProcMacro)),
                "source": { "include_dirs": [package_dir, source_dir], "exclude_dirs": [] },
            }));
            if is_lib {
                library.insert(&package.id, (index, target.name.replace('-', "_")));
            }
            targets.push((&package.id, is_lib, index));
        }
    }

    for (package, is_lib, index) in &targets {
        let mut deps: Vec<serde_json::Value> = Vec::new();
        if !is_lib {
            if let Some((lib, name)) = library.get(package) {
                deps.push(serde_json::json!({ "crate": lib, "name": name }));
            }
        }
        let Some(node) = nodes.get(package) else { continue };
        for dependency in &node.deps {
            let wanted = dependency.dep_kinds.iter().any(|info| match info.kind {
                DependencyKind::Normal => true,
                DependencyKind::Development => !is_lib,
                _ => false,
            });
            if !wanted || !members.contains(&dependency.pkg) {
                continue;
            }
            let Some((lib, _)) = library.get(&dependency.pkg) else { continue };
            deps.push(serde_json::json!({ "crate": lib, "name": dependency.name }));
        }
        crates[*index]["deps"] = serde_json::Value::Array(deps);
    }

    // `for`, `?`, `.await` and ranges lower through core's lang items; without
    // them rust-analyzer drops the whole expression, calls inside included.
    let core = minicore()?;
    let library = core.ancestors().nth(3).map(Path::to_path_buf).unwrap_or_default();
    let data: ProjectJsonData = serde_json::from_value(serde_json::json!({
        "sysroot_src": library,
        "sysroot_project": { "crates": [{
            "display_name": "core",
            "root_module": core,
            "edition": "2024",
            "deps": [],
            "is_workspace_member": false,
        }] },
        "crates": crates,
    }))
        .map_err(|error| failed(error.to_string()))?;
    let base = std::fs::canonicalize(root).map_err(|error| failed(error.to_string()))?;
    Ok(ProjectJson::new(None, &AbsPathBuf::assert_utf8(base), data))
}
