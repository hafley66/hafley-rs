//! Targeted slow graph evidence over the same resolved-edge rows as fast.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sprefa_extract::{resolve_project_target_with_raw, FamilyTag, FlatFact, ResolveRequest};

pub(super) fn facts(
    request: &ResolveRequest<'_>,
    root: &Path,
    name: &str,
) -> Result<Vec<FlatFact>, Box<dyn std::error::Error>> {
    sprefa_extract::slow::require_ts_checker(request.paths)?;
    let mut definitions: BTreeMap<(String, String, bool), Vec<(u32, u32)>> = BTreeMap::new();
    let mut sites = Vec::new();
    let mut checker_defs = sprefa_extract::edit::checker_edges::CheckerDefs::default();
    let _extract_span = tracing::info_span!("fast.extract_resolve").entered();
    let mut facts = resolve_project_target_with_raw(request, &mut |raw| {
        if let FlatFact::Site { family: FamilyTag::Call, span, callee, .. } = &raw.fact {
            if callee == name {
                sites.push((raw.path.to_string(), span.start, span.end));
            }
        }
        checker_defs.capture(&raw);
        if let FlatFact::Node {
            family: family @ (FamilyTag::Call | FamilyTag::Type),
            span,
            name: Some(name),
            ..
        } = &raw.fact
        {
            definitions
                .entry((
                    raw.path.to_string(),
                    name.clone(),
                    *family == FamilyTag::Call,
                ))
                .or_default()
                .push((span.start, span.end));
        }
        Ok::<(), std::convert::Infallible>(())
    })?;
    drop(_extract_span);
    let files: Vec<(String, PathBuf)> = request
        .paths
        .iter()
        .map(|path| {
            let supplied = path.to_string_lossy().into_owned();
            (supplied, sprefa_extract::io_path(path))
        })
        .collect();
    let ts_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    // Call sites: the checker answers a definition at each site written `name`.
    checker_defs.seal();
    let supplied: Vec<&str> = files.iter().map(|(path, _)| path.as_str()).collect();
    let candidates: Vec<&str> = sites
        .iter()
        .map(|(path, ..)| path.as_str())
        .filter(|path| path.ends_with(".ts") || path.ends_with(".tsx"))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let ts_edges = sprefa_extract::edit::ts7_callers::callers(
        &ts_root, name, &candidates, &supplied, &checker_defs,
    )
    .map_err(|error| format!("TypeScript LSP target {name}: {error}"))?;
    // Type uses: the checker's references to each declaration named `name`.
    let ts_seeds: Vec<(String, String)> = if request.arms.types {
        let mut seeds: BTreeSet<(String, String)> = facts
            .iter()
            .filter_map(|fact| {
                let FlatFact::ResolvedTypeEdge { target_path, target_name, resolution_origin, .. } = fact else {
                    return None;
                };
                (target_name.as_deref() == Some(name)
                    && resolution_origin != "scip"
                    && (target_path.ends_with(".ts") || target_path.ends_with(".tsx")))
                .then(|| (target_path.clone(), name.to_string()))
            })
            .collect();
        seeds.extend(definitions.keys().filter_map(|(path, definition, _)| {
            (definition == name && (path.ends_with(".ts") || path.ends_with(".tsx")))
                .then(|| (path.clone(), definition.clone()))
        }));
        seeds.into_iter().collect()
    } else {
        Vec::new()
    };
    // Open unresolved source files too. A checker search must not inherit
    // the fast tier's set of already-bound users as its project boundary.
    let ts_sources: BTreeSet<_> = files.iter().map(|(path, _)| path.clone()).collect();
    let ts_references = sprefa_extract::edit::ts7_graph_target::references(
        &ts_root,
        &files,
        &ts_seeds,
        &ts_sources,
    )
    .map_err(|error| format!("TypeScript LSP target {name}: {error}"))?;
    #[cfg(feature = "rust-checker")]
    {
        let seeds: Vec<(String, String)> = definitions
            .keys()
            .filter(|(path, definition_name, is_call)| {
                *is_call && definition_name == name && path.ends_with(".rs")
            })
            .map(|(path, definition_name, _)| (path.clone(), definition_name.clone()))
            .collect();
        let calls = sprefa_extract::lang::rust_checker::target_calls(
            root,
            &files,
            &seeds,
            sprefa_extract::lang::rust_checker::LoadMode::Types,
            Duration::from_secs(30),
        )
        .map_err(|error| format!("rust-analyzer target {name}: {error}"))?;
        let loaded = calls.loaded;
        let calls = calls.calls;
        let type_seeds: Vec<(String, String)> = facts
            .iter()
            .filter_map(|fact| {
                let FlatFact::ResolvedTypeEdge {
                    target_path,
                    target_name,
                    resolution_origin,
                    ..
                } = fact
                else {
                    return None;
                };
                (request.arms.types
                    && target_name.as_deref() == Some(name)
                    && target_path.ends_with(".rs")
                    && resolution_origin != "scip")
                    .then(|| (target_path.clone(), name.to_string()))
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let types = sprefa_extract::lang::rust_checker::target_types(
            root,
            &files,
            &type_seeds,
            Duration::from_secs(30),
        )
        .map_err(|error| format!("rust-analyzer type target {name}: {error}"))?;
        let verified: BTreeSet<_> = calls
            .into_iter()
            .filter_map(|mut call| {
                let definition =
                    [FamilyTag::Call, FamilyTag::Type]
                        .into_iter()
                        .find_map(|family| {
                            let spans = definitions.get(&(
                                call.target_path.clone(),
                                call.target_name.clone(),
                                family == FamilyTag::Call,
                            ))?;
                            spans
                                .iter()
                                .find(|(start, end)| {
                                    *start <= call.target_start && call.target_start < *end
                                })
                                .copied()
                                .or_else(|| (spans.len() == 1).then_some(spans[0]))
                        })?;
                call.target_start = definition.0;
                call.target_end = definition.1;
                Some(call)
            })
            .collect();
        for fact in &mut facts {
            let FlatFact::ResolvedEdge {
                caller_path,
                caller_name,
                callee_path,
                callee_name,
                caller_site_start,
                caller_site_end,
                callee_start,
                callee_end,
                kind,
                resolution_origin,
                ..
            } = fact
            else {
                continue;
            };
            if callee_name.as_deref() != Some(name) {
                continue;
            }
            if let Some(call) = verified.iter().find(|call| {
                call.source_path == *caller_path
                    && *caller_site_start <= call.site_start
                    && call.site_end <= *caller_site_end
            }) {
                if *callee_path != call.target_path {
                    *callee_path = call.target_path.clone();
                    *callee_start = call.target_start;
                    *callee_end = call.target_end;
                }
                *kind = if *caller_name == call.caller_name {
                    "checker_resolve"
                } else {
                    "name_resolve"
                }
                .to_string();
                *resolution_origin = "checker".to_string();
            }
        }
        for call in verified {
            let present = facts.iter().any(|fact| {
                matches!(fact, FlatFact::ResolvedEdge {
                    caller_path, caller_name, callee_name, caller_site_start, caller_site_end, ..
                } if *caller_path == call.source_path
                    && *caller_name == call.caller_name
                    && callee_name.as_deref() == Some(call.target_name.as_str())
                    && *caller_site_start <= call.site_start
                    && call.site_end <= *caller_site_end)
            });
            if present {
                // A closure's named enclosing function owns a mirror edge.
            } else {
                facts.push(FlatFact::ResolvedEdge {
                    fact: None,
                    caller_path: call.source_path.clone(),
                    caller_name: call.caller_name.clone(),
                    callee_path: call.target_path.clone(),
                    callee_name: Some(call.target_name.clone()),
                    caller_site_start: call.site_start,
                    caller_site_end: call.site_end,
                    callee_start: call.target_start,
                    callee_end: call.target_end,
                    kind: "checker_resolve".to_string(),
                    resolution_origin: "checker".to_string(),
                });
            }
            if call
                .caller_name
                .as_deref()
                .is_some_and(|name| name.starts_with("closure@"))
            {
                if let Some(enclosing) = call.enclosing_name {
                    if !facts.iter().any(|fact| matches!(fact, FlatFact::ResolvedEdge {
                        caller_path, caller_name, caller_site_start, caller_site_end,
                        callee_path, kind, resolution_origin, ..
                    } if *caller_path == call.source_path && caller_name.as_deref() == Some(enclosing.as_str())
                        && *caller_site_start <= call.site_start && call.site_end <= *caller_site_end
                        && *callee_path == call.target_path && kind == "name_resolve"
                        && resolution_origin == "checker")) {
                        facts.push(FlatFact::ResolvedEdge {
                            fact: None,
                            caller_path: call.source_path,
                            caller_name: Some(enclosing),
                            callee_path: call.target_path,
                            callee_name: Some(call.target_name),
                            caller_site_start: call.site_start,
                            caller_site_end: call.site_end,
                            callee_start: call.target_start,
                            callee_end: call.target_end,
                            kind: "name_resolve".to_string(),
                            resolution_origin: "checker".to_string(),
                        });
                    }
                }
            }
        }
        for fact in &mut facts {
            let FlatFact::ResolvedTypeEdge {
                owner_path,
                owner_name,
                target_path,
                target_name,
                resolution_origin,
                ..
            } = fact
            else {
                continue;
            };
            if target_name.as_deref() != Some(name) {
                continue;
            }
            if types.iter().any(|reference| {
                reference.source_path == *owner_path
                    && reference.target_path == *target_path
                    && reference.owner_name == *owner_name
            }) {
                *resolution_origin = "checker".to_string();
            }
        }
        // rust-analyzer answered every call of `name` in the files it loaded; a
        // name-matched edge it did not confirm there calls some other `name`.
        facts.retain(|fact| {
            !matches!(fact, FlatFact::ResolvedEdge {
                caller_path, callee_name, resolution_origin, ..
            } if callee_name.as_deref() == Some(name)
                && caller_path.ends_with(".rs")
                && loaded.contains(caller_path)
                && resolution_origin != "checker")
        });
    }
    #[cfg(not(feature = "rust-checker"))]
    let _ = (root, name, files, Duration::from_secs(30));
    for fact in &mut facts {
        let FlatFact::ResolvedTypeEdge {
            owner_path,
            target_path,
            target_name,
            resolution_origin,
            ..
        } = fact
        else {
            continue;
        };
        if target_name.as_deref() != Some(name) {
            continue;
        }
        if ts_references.iter().any(|reference| {
            reference.source_path == *owner_path && reference.target_path == *target_path
        }) {
            *resolution_origin = "checker".to_string();
        }
    }
    checker_defs.write(&mut facts, ts_edges);
    Ok(facts)
}
