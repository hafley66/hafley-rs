//! Cargo target and module ancestry for a bounded Rust corpus.

use super::*;
use std::path::Path;

/// Read module facts along Cargo's target trees, retaining only ancestors of
/// supplied files. Definitions and call sites remain the supplied corpus's.
pub(super) fn load(
    files: &mut Vec<(String, RustModuleFacts)>,
    corpus: &mut Vec<(String, ContentId)>,
) -> (
    HashMap<String, HashSet<TargetScope>>,
    Vec<(String, String, String)>,
) {
    let nearest = nearest_crate_dirs(corpus);
    let wanted: HashSet<String> = corpus.iter().map(|(path, _)| path.clone()).collect();
    let mut facts: HashMap<String, RustModuleFacts> = files.iter().cloned().collect();
    let mut retained = HashSet::new();
    let mut failures = Vec::new();
    let mut scopes: HashMap<String, HashSet<TargetScope>> = HashMap::new();
    for package_root in nearest.values().collect::<BTreeSet<_>>() {
        let manifest = crate::read::io_path(&Path::new(package_root).join("Cargo.toml"));
        let Ok(manifest) = manifest.canonicalize() else {
            continue;
        };
        let metadata = match super::super::rust_workspace::discover(&manifest) {
            Ok(workspace) => workspace.metadata,
            Err(error) => {
                for (path, root) in &nearest {
                    if root == package_root {
                        failures.push((path.clone(), lexical(&manifest), error.clone()));
                    }
                }
                continue;
            }
        };
        let Some(package) = metadata
            .packages
            .iter()
            .find(|package| package.manifest_path.as_std_path() == manifest)
        else {
            continue;
        };
        for target in &package.targets {
            let Ok(relative) = target
                .src_path
                .as_std_path()
                .strip_prefix(manifest.parent().unwrap())
            else {
                continue;
            };
            let root = lexical(&Path::new(package_root).join(relative));
            let kind = if target
                .kind
                .contains(&cargo_metadata::TargetKind::CustomBuild)
            {
                TargetKind::Build
            } else if target.kind.iter().any(|kind| {
                matches!(
                    kind,
                    cargo_metadata::TargetKind::Test
                        | cargo_metadata::TargetKind::Bench
                        | cargo_metadata::TargetKind::Example
                )
            }) {
                TargetKind::Dev
            } else {
                TargetKind::Normal
            };
            retain_module_ancestry(
                &root,
                &TargetScope {
                    root: root.clone(),
                    kind,
                },
                &wanted,
                &mut facts,
                &mut retained,
                &mut scopes,
                &mut HashSet::new(),
            );
        }
    }
    for path in retained {
        if wanted.contains(&path) {
            continue;
        }
        let Ok(bytes) = std::fs::read(crate::read::io_path(Path::new(&path))) else {
            continue;
        };
        corpus.push((path.clone(), crate::read::shape::content_id_of(&bytes)));
        let Some(module) = facts.remove(&path) else {
            continue;
        };
        // Context contributes name routes, never off-corpus callable targets.
        files.push((
            path,
            RustModuleFacts {
                uses: module.uses,
                stars: module.stars,
                inline_mods: module.inline_mods,
                mod_decls: module.mod_decls,
                mod_scopes: module.mod_scopes,
                ..RustModuleFacts::default()
            },
        ));
    }
    (scopes, failures)
}

fn retain_module_ancestry(
    path: &str,
    scope: &TargetScope,
    wanted: &HashSet<String>,
    facts: &mut HashMap<String, RustModuleFacts>,
    retained: &mut HashSet<String>,
    scopes: &mut HashMap<String, HashSet<TargetScope>>,
    visiting: &mut HashSet<String>,
) -> bool {
    if !visiting.insert(path.to_string()) {
        return false;
    }
    if !facts.contains_key(path) {
        if let Ok(bytes) = std::fs::read(crate::read::io_path(Path::new(path))) {
            if let Some(module) = rust_module_facts(path, &bytes) {
                facts.insert(path.to_string(), module);
            }
        }
    }
    let mut needed = wanted.contains(path);
    let declarations = facts
        .get(path)
        .map(|facts| facts.mod_decls.clone())
        .unwrap_or_default();
    for (name, attribute) in declarations {
        let inline_scope = facts
            .get(path)
            .and_then(|facts| facts.mod_scopes.get(&name))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let candidates = module_candidates(path, &name, attribute.as_deref(), inline_scope);
        if let Some(child) = candidates.into_iter().find(|child| {
            facts.contains_key(child) || crate::read::io_path(Path::new(child)).is_file()
        }) {
            needed |=
                retain_module_ancestry(&child, scope, wanted, facts, retained, scopes, visiting);
        }
    }
    visiting.remove(path);
    if needed {
        retained.insert(path.to_string());
        scopes
            .entry(path.to_string())
            .or_default()
            .insert(scope.clone());
    }
    needed
}
