//! Names crate graph: Cargo workspace targets and dependency names, without a sysroot.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use cargo_metadata::{DependencyKind, PackageId, TargetKind};
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

/// Render Cargo's workspace targets into the Names graph. No dependency
/// sources or sysroot are loaded, and no file is written.
pub(super) fn names_project(
    root: &Path,
    metadata: &cargo_metadata::Metadata,
) -> Result<ProjectJson, CheckerError> {
    let failed = |error: String| CheckerError::NoWorkspace(error);
    let edges = dependency_edges(metadata);
    let members: HashSet<&PackageId> = metadata.workspace_members.iter().collect();

    let mut crates: Vec<serde_json::Value> = Vec::new();
    let mut library: HashMap<&PackageId, (usize, String)> = HashMap::new();
    let mut targets: Vec<(&PackageId, bool, bool, bool, usize)> = Vec::new();
    for package in metadata.workspace_packages() {
        // Edges come from the default-feature resolve; every feature is on as a cfg.
        let cfg: Vec<String> = package
            .features
            .keys()
            .map(|feature| format!("feature=\"{feature}\""))
            .chain(["test".to_owned(), "debug_assertions".to_owned()])
            .collect();
        let package_dir = package
            .manifest_path
            .parent()
            .map(|dir| dir.to_string())
            .unwrap_or_default();
        for target in &package.targets {
            let is_build = target
                .kind
                .iter()
                .any(|kind| matches!(kind, TargetKind::CustomBuild));
            let is_lib = target.kind.iter().any(is_library);
            let is_dev = target.kind.iter().any(|kind| {
                matches!(
                    kind,
                    TargetKind::Test | TargetKind::Example | TargetKind::Bench
                )
            });
            let index = crates.len();
            let source_dir = target
                .src_path
                .parent()
                .map(|dir| dir.to_string())
                .unwrap_or_default();
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
            targets.push((&package.id, is_lib, is_build, is_dev, index));
        }
    }

    for (package, is_lib, is_build, is_dev, index) in &targets {
        let mut deps: Vec<serde_json::Value> = Vec::new();
        if !is_lib && !is_build {
            if let Some((lib, name)) = library.get(package) {
                deps.push(serde_json::json!({ "crate": lib, "name": name }));
            }
        }
        let Some(edges) = edges.get(package) else {
            continue;
        };
        for (dependency, name, kinds) in edges {
            let wanted = kinds.iter().any(|kind| match kind {
                DependencyKind::Normal => !is_build,
                DependencyKind::Development => *is_dev,
                DependencyKind::Build => *is_build,
                _ => false,
            });
            if !wanted || !members.contains(dependency) {
                continue;
            }
            let Some((lib, _)) = library.get(dependency) else {
                continue;
            };
            deps.push(serde_json::json!({ "crate": lib, "name": name }));
        }
        crates[*index]["deps"] = serde_json::Value::Array(deps);
    }

    let project = serde_json::json!({ "crates": crates });
    let data: ProjectJsonData =
        serde_json::from_value(project).map_err(|error| failed(error.to_string()))?;
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
                    .map(|dep| {
                        (
                            &dep.pkg,
                            dep.name.clone(),
                            dep.dep_kinds.iter().map(|info| info.kind).collect(),
                        )
                    })
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
