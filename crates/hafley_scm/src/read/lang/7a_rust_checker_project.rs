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

/// rust-analyzer's own minimal `core` (its test fixture) and ryi's `std` shim over it,
/// written under the repo's ryi state dir; returns the directory holding `core/` and `std/`.
fn sysroot_library(root: &Path) -> Result<PathBuf, CheckerError> {
    let failed = |error: std::io::Error| CheckerError::NoWorkspace(error.to_string());
    let library = root.join(".dl/.state/ra-sysroot-0.0.352");
    let core = ra_ap_test_utils::MiniCore::from_flags([
        "iterator", "iterators", "try", "future", "async_fn", "range", "option", "result", "fn",
        "deref", "deref_mut", "index", "from", "sized", "copy", "clone", "eq", "ord", "default",
        "drop", "panic", "fmt", "derive", "slice", "str", "pin", "cell", "hash", "borrow",
        "assert", "write", "todo", "unimplemented", "matches", "concat", "env", "include",
    ])
    .source_code(ra_ap_test_utils::MiniCore::RAW_SOURCE)
    .replacen(
        "fn next(&mut self) -> Option<Self::Item>;\n",
        concat!("fn next(&mut self) -> Option<Self::Item>;\n", include_str!("7c_rust_core_iterator_methods.rs")),
        1,
    ) + include_str!("7c_rust_core_shim.rs");
    for (path, source) in [
        (library.join("core/src/lib.rs"), core.as_str()),
        (library.join("std/src/lib.rs"), include_str!("7b_rust_std_shim.rs")),
    ] {
        if std::fs::read_to_string(&path).ok().as_deref() != Some(source) {
            std::fs::create_dir_all(path.parent().unwrap()).map_err(failed)?;
            std::fs::write(&path, source).map_err(failed)?;
        }
    }
    Ok(library)
}

/// `sysroot`: rust-src's `core` and the `std` shim join the graph, written under `root`.
pub(super) fn fast_project(root: &Path, sysroot: bool) -> Result<ProjectJson, CheckerError> {
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
    // Without the sysroot nothing is type checked: no resolve, so no Cargo.lock written.
    let mut options = vec!["--offline".to_owned(), "--filter-platform".to_owned(), host];
    if !sysroot {
        options.push("--no-deps".to_owned());
    }
    let metadata = MetadataCommand::new()
        .manifest_path(root.join("Cargo.toml"))
        .other_options(options)
        .exec()
        .map_err(|error| failed(error.to_string()))?;
    let members: HashSet<&PackageId> = metadata.workspace_members.iter().collect();
    let edges = dependency_edges(&metadata);

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
        let Some(edges) = edges.get(package) else { continue };
        for (dependency, name, kinds) in edges {
            let wanted = kinds.iter().any(|kind| match kind {
                DependencyKind::Normal => true,
                DependencyKind::Development => !is_lib,
                _ => false,
            });
            if !wanted || !members.contains(dependency) {
                continue;
            }
            let Some((lib, _)) = library.get(dependency) else { continue };
            deps.push(serde_json::json!({ "crate": lib, "name": name }));
        }
        crates[*index]["deps"] = serde_json::Value::Array(deps);
    }

    // `for`, `?`, `.await` and ranges lower through core's lang items; without
    // them rust-analyzer drops the whole expression, calls inside included.
    let mut project = serde_json::json!({ "crates": crates });
    if sysroot {
        let library = sysroot_library(root)?;
        project["sysroot_src"] = serde_json::json!(library);
        project["sysroot_project"] = serde_json::json!({ "crates": [
            {
                "display_name": "core",
                "root_module": library.join("core/src/lib.rs"),
                "edition": "2024",
                "deps": [],
                "is_workspace_member": false,
            },
            {
                "display_name": "std",
                "root_module": library.join("std/src/lib.rs"),
                "edition": "2024",
                "deps": [{ "crate": 0, "name": "core" }],
                "is_workspace_member": false,
            },
        ] });
    }
    let data: ProjectJsonData = serde_json::from_value(project)
        .map_err(|error| failed(error.to_string()))?;
    let base = std::fs::canonicalize(root).map_err(|error| failed(error.to_string()))?;
    Ok(ProjectJson::new(None, &AbsPathBuf::assert_utf8(base), data))
}

type Edge<'a> = (&'a PackageId, String, Vec<DependencyKind>);

/// Each package's dependencies as (package, extern name, kinds): cargo's
/// resolve, or under `--no-deps` the path dependencies between members.
fn dependency_edges(metadata: &cargo_metadata::Metadata) -> HashMap<&PackageId, Vec<Edge<'_>>> {
    if let Some(resolve) = &metadata.resolve {
        return resolve
            .nodes
            .iter()
            .map(|node| {
                let edges = node
                    .deps
                    .iter()
                    .map(|dep| (&dep.pkg, dep.name.clone(), dep.dep_kinds.iter().map(|info| info.kind).collect()))
                    .collect();
                (&node.id, edges)
            })
            .collect();
    }
    let by_dir: HashMap<_, &PackageId> = metadata
        .packages
        .iter()
        .filter_map(|package| Some((package.manifest_path.parent()?.to_path_buf(), &package.id)))
        .collect();
    metadata
        .packages
        .iter()
        .map(|package| {
            let edges = package
                .dependencies
                .iter()
                .filter_map(|dep| {
                    let id = by_dir.get(dep.path.as_ref()?)?;
                    let name = dep.rename.as_ref().unwrap_or(&dep.name).replace('-', "_");
                    Some((*id, name, vec![dep.kind]))
                })
                .collect();
            (&package.id, edges)
        })
        .collect()
}
