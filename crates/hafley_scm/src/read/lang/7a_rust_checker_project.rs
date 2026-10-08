//! Names crate graph: Cargo workspace targets and dependency names, without a sysroot.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

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
        let exclude_dirs = unrelated_dirs(package);
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
                "source": { "include_dirs": [package_dir, source_dir], "exclude_dirs": exclude_dirs },
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

/// Types crate graph: the packages owning `files`, their dependency closure and the sysroot.
/// Loading the whole Cargo workspace instead puts every member's dependencies in the graph, and
/// method resolution on a foreign or primitive receiver scans inherent impls in every crate
/// (hafley-observe in hafley-rs: 128 crates needed, 551 loaded, 11.4 s of inherent impls).
pub(super) fn types_project(root: &Path, files: &[(String, PathBuf)]) -> Result<ProjectJson, CheckerError> {
    let failed = |error: String| CheckerError::NoWorkspace(error);
    let metadata = cargo_metadata::MetadataCommand::new()
        .current_dir(root)
        .features(cargo_metadata::CargoOpt::AllFeatures)
        .other_options(vec!["--offline".to_string()])
        .exec()
        .map_err(|error| failed(error.to_string()))?;
    let resolve = metadata.resolve.as_ref().ok_or_else(|| failed("cargo metadata has no resolve".into()))?;
    let nodes: HashMap<&PackageId, &cargo_metadata::Node> = resolve.nodes.iter().map(|node| (&node.id, node)).collect();
    let packages: HashMap<&PackageId, &cargo_metadata::Package> = metadata.packages.iter().map(|package| (&package.id, package)).collect();
    let members: HashSet<&PackageId> = metadata.workspace_members.iter().collect();
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let member_dirs: Vec<(&PackageId, PathBuf)> = metadata
        .workspace_packages()
        .into_iter()
        .filter_map(|package| Some((&package.id, canonical(package.manifest_path.parent()?.as_std_path()))))
        .collect();
    let mut owners: HashSet<&PackageId> = files
        .iter()
        .filter_map(|(_, file)| {
            let file = canonical(file);
            member_dirs
                .iter()
                .filter(|(_, dir)| file.starts_with(dir))
                .max_by_key(|(_, dir)| dir.as_os_str().len())
                .map(|(id, _)| *id)
        })
        .collect();
    if owners.is_empty() {
        owners = members.clone();
    }

    // Dev dependencies count for the owning packages only, as cargo builds their tests.
    let mut closure: HashSet<&PackageId> = HashSet::new();
    let mut todo: Vec<&PackageId> = owners.iter().copied().collect();
    while let Some(id) = todo.pop() {
        if !closure.insert(id) {
            continue;
        }
        let Some(node) = nodes.get(id) else { continue };
        for dep in &node.deps {
            let wanted = dep.dep_kinds.iter().any(|info| match info.kind {
                DependencyKind::Normal | DependencyKind::Build => true,
                DependencyKind::Development => owners.contains(id),
                _ => false,
            });
            if wanted && !closure.contains(&dep.pkg) {
                todo.push(&dep.pkg);
            }
        }
    }

    let mut ids: Vec<&PackageId> = closure.iter().copied().collect();
    ids.sort();
    let mut crates: Vec<serde_json::Value> = Vec::new();
    let mut library: HashMap<&PackageId, usize> = HashMap::new();
    let mut entries: Vec<(&PackageId, bool, usize)> = Vec::new();
    for id in ids {
        let Some(package) = packages.get(id) else { continue };
        let owner = owners.contains(id);
        let member = members.contains(id);
        let mut cfg: Vec<String> = nodes
            .get(id)
            .map(|node| node.features.iter().map(|feature| format!("feature=\"{feature}\"")).collect())
            .unwrap_or_default();
        cfg.push("debug_assertions".to_owned());
        if member {
            cfg.push("test".to_owned());
        }
        for target in &package.targets {
            if target.kind.iter().any(|kind| matches!(kind, TargetKind::CustomBuild)) {
                continue;
            }
            let is_lib = target.kind.iter().any(is_library);
            if !is_lib && !owner {
                continue;
            }
            let index = crates.len();
            let mut entry = serde_json::json!({
                "display_name": target.name,
                "root_module": target.src_path,
                "edition": target.edition.as_str(),
                "deps": [],
                "cfg": cfg,
                "is_workspace_member": member,
                "is_proc_macro": target.kind.iter().any(|kind| matches!(kind, TargetKind::ProcMacro)),
            });
            if member {
                let package_dir = package.manifest_path.parent().map(|dir| dir.to_string()).unwrap_or_default();
                let source_dir = target.src_path.parent().map(|dir| dir.to_string()).unwrap_or_default();
                entry["source"] = serde_json::json!({ "include_dirs": [package_dir, source_dir], "exclude_dirs": unrelated_dirs(package) });
            }
            crates.push(entry);
            if is_lib {
                library.insert(id, index);
            }
            entries.push((id, is_lib, index));
        }
    }
    for (id, is_lib, index) in &entries {
        let mut deps: Vec<serde_json::Value> = Vec::new();
        if !is_lib {
            if let (Some(lib), Some(package)) = (library.get(id), packages.get(id)) {
                let name = package.targets.iter().find(|target| target.kind.iter().any(is_library)).map(|target| target.name.replace('-', "_")).unwrap_or_default();
                deps.push(serde_json::json!({ "crate": lib, "name": name }));
            }
        }
        if let Some(node) = nodes.get(id) {
            for dep in &node.deps {
                if !closure.contains(&dep.pkg) || (*is_lib && &dep.pkg == *id) {
                    continue;
                }
                if let Some(lib) = library.get(&dep.pkg) {
                    deps.push(serde_json::json!({ "crate": lib, "name": dep.name }));
                }
            }
        }
        crates[*index]["deps"] = serde_json::Value::Array(deps);
    }

    let sysroot = std::process::Command::new("rustc")
        .current_dir(root)
        .args(["--print", "sysroot"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
    let mut project = serde_json::json!({ "crates": crates });
    if let Some(sysroot) = sysroot {
        let source = Path::new(&sysroot).join("lib/rustlib/src/rust/library");
        if source.is_dir() {
            project["sysroot_src"] = serde_json::Value::String(source.to_string_lossy().into_owned());
        }
        project["sysroot"] = serde_json::Value::String(sysroot);
    }
    tracing::info!(owners = owners.len(), crates = crates_len(&project), "rust checker types graph");
    let data: ProjectJsonData = serde_json::from_value(project).map_err(|error| failed(error.to_string()))?;
    let base = std::fs::canonicalize(root).map_err(|error| failed(error.to_string()))?;
    Ok(ProjectJson::new(None, &AbsPathBuf::assert_utf8(base), data))
}

fn crates_len(project: &serde_json::Value) -> usize {
    project["crates"].as_array().map_or(0, Vec::len)
}

/// Top-level directories of the package that hold none of its target roots. The package
/// directory of a root package is often a whole repository (vendored trees, symlinked sibling
/// checkouts, worktrees); rust-analyzer walks every included directory before it answers.
fn unrelated_dirs(package: &cargo_metadata::Package) -> Vec<String> {
    let Some(dir) = package.manifest_path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut excluded: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir() || kind.is_symlink()))
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            let dir = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            !package.targets.iter().any(|target| {
                let source = target.src_path.as_std_path();
                source.starts_with(path)
                    || std::fs::canonicalize(source).is_ok_and(|source| source.starts_with(&dir))
            })
        })
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    excluded.sort();
    excluded
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
