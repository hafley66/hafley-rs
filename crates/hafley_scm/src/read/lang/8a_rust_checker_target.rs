//! Resolve only call sites named by a targeted graph question.

use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TargetCall {
    pub source_path: String,
    pub caller_name: Option<String>,
    pub enclosing_name: Option<String>,
    pub site_start: u32,
    pub site_end: u32,
    pub target_path: String,
    pub target_start: u32,
    pub target_end: u32,
    pub target_name: String,
}

enum Candidate {
    Method(ast::MethodCallExpr),
    Path(ast::Path),
}

fn target_owner_name(
    node: &ra_ap_syntax::SyntaxNode,
    offsets: &OffsetMap,
) -> Option<String> {
    for ancestor in node.ancestors() {
        if let Some(closure) = ast::ClosureExpr::cast(ancestor.clone()) {
            let start = offsets.to_span_offset(u32::from(closure.syntax().text_range().start()));
            return Some(format!("closure@{start}"));
        }
        if let Some(function) = ast::Fn::cast(ancestor) {
            return function.name().map(|name| name.text().to_string());
        }
    }
    None
}

fn enclosing_name(node: &ra_ap_syntax::SyntaxNode) -> Option<String> {
    node.ancestors()
        .find_map(|ancestor| ast::Fn::cast(ancestor).and_then(|function| function.name()))
        .map(|name| name.text().to_string())
}

/// Build the def maps of the source files' crates and their dependency closure
/// in parallel, crate by crate in dependency order, before any file is touched.
///
/// The first `attach_first_edition` otherwise builds that closure on one
/// thread (7.0 s for `hafley_scm`'s 436 crates; parallel priming takes 3.9 s on 8 threads).
pub(super) fn prime_crate_closure(
    db: &RootDatabase,
    ids: &HashMap<String, ra_ap_ide::FileId>,
    sources: &[&str],
    threads: usize,
) {
    use ra_ap_ide_db::base_db;
    let _prime = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.prime_crate_closure")).entered();
    let mut scope: Vec<base_db::Crate> = sources
        .iter()
        .filter_map(|source| ids.get(*source))
        .flat_map(|&file_id| base_db::relevant_crates(db, file_id).to_vec())
        .flat_map(|krate| krate.transitive_deps(db))
        .collect();
    scope.sort();
    scope.dedup();
    ra_ap_ide_db::prime_caches::parallel_prime_caches(db, &scope, threads, &|_| {});
}

pub fn target_calls(
    root: &Path,
    files: &[(String, PathBuf)],
    sites: &[(String, u32, u32, String)],
    budget: Duration,
) -> Result<Vec<TargetCall>, CheckerError> {
    if sites.is_empty() {
        return Ok(Vec::new());
    }
    let (workspace, _) =
        super::super::rust_checker_session::checker_workspace(root, files, budget)?;
    let workspace = workspace.lock().unwrap();
    let _file_index_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.file_index")).entered();
    let wanted: HashMap<PathBuf, &str> = files
        .iter()
        .map(|(name, path)| {
            (
                std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()),
                name.as_str(),
            )
        })
        .collect();
    let mut ids = HashMap::new();
    let mut paths = HashMap::new();
    for (vfs_id, vfs_path) in workspace.vfs.iter() {
        let Some(absolute) = vfs_path.as_path() else {
            continue;
        };
        let path = PathBuf::from(absolute.to_string());
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        if let Some(name) = wanted.get(&path) {
            let id = ra_ap_ide::FileId::from_raw(vfs_id.index());
            ids.insert((*name).to_string(), id);
            paths.insert(id, (*name).to_string());
        }
    }
    let mut by_source: HashMap<&str, BTreeSet<(u32, u32, &str)>> = HashMap::new();
    for (path, start, end, name) in sites {
        by_source
            .entry(path)
            .or_default()
            .insert((*start, *end, name));
    }
    drop(_file_index_span);
    let db = workspace.host.raw_database();
    let _query_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.queries")).entered();
    let pool = crate::read::project::extract_pool();
    let mut sources: Vec<_> = by_source.into_iter().collect();
    sources.sort_by_key(|(path, _)| *path);
    let source_paths: Vec<&str> = sources.iter().map(|(path, _)| *path).collect();
    prime_crate_closure(&db, &ids, &source_paths, pool.current_num_threads());
    let chunk_size = sources.len().div_ceil(pool.current_num_threads()).max(1);
    let chunks: Vec<_> = sources
        .chunks(chunk_size)
        .map(|chunk| (db.clone(), chunk))
        .collect();
    let query_span = tracing::Span::current();
    let per_chunk: Vec<Vec<TargetCall>> = pool.install(|| {
        use rayon::prelude::*;
        chunks
            .into_par_iter()
            .map(|(db, chunk)| {
                let _query_entered = query_span.enter();
                attach_db(&db, || {
                    let _sema_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.sema_init")).entered();
                    let sema = Semantics::new(&db);
                    drop(_sema_span);
                    let mut found = BTreeSet::new();
                    let mut destination_offsets = HashMap::new();
                    let _source_files_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.source_files")).entered();
                    for (source_path, wanted_sites) in chunk {
                        let Some(&file_id) = ids.get(*source_path) else {
                            continue;
                        };
                        let _edition_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.source_edition")).entered();
                        let editioned = sema.attach_first_edition(file_id);
                        drop(_edition_span);
                        let _parse_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.source_parse")).entered();
                        let syntax = sema.parse(editioned);
                        drop(_parse_span);
                        let _offset_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.source_offsets")).entered();
                        let source_offsets = OffsetMap::new(&syntax.syntax().text().to_string());
                        drop(_offset_span);
                        let _scan_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.syntax_scan")).entered();
                        for node in syntax.syntax().descendants() {
                            let candidate =
                                if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
                                    call.name_ref()
                                        .map(|name_ref| (name_ref, Candidate::Method(call)))
                                } else if let Some(call) = ast::CallExpr::cast(node) {
                                    call.expr()
                                        .and_then(|expr| ast::PathExpr::cast(expr.syntax().clone()))
                                        .and_then(|expr| expr.path())
                                        .and_then(|path| {
                                            let name_ref = path.segment()?.name_ref()?;
                                            Some((name_ref, Candidate::Path(path)))
                                        })
                                } else {
                                    None
                                };
                            let Some((name_ref, candidate)) = candidate else {
                                continue;
                            };
                            let range = name_ref.syntax().text_range();
                            let start = source_offsets.to_span_offset(u32::from(range.start()));
                            let end = source_offsets.to_span_offset(u32::from(range.end()));
                            let name = name_ref.text();
                            if !wanted_sites.iter().any(|(site_start, site_end, callee)| {
                                *callee == name && *site_start <= start && end <= *site_end
                            }) {
                                continue;
                            }
                            let _definition_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.definition_lookup")).entered();
                            let definition = match candidate {
                                Candidate::Method(call) => {
                                    sema.resolve_method_call(&call).map(ModuleDef::Function)
                                }
                                Candidate::Path(path) => match sema.resolve_path(&path) {
                                    Some(PathResolution::Def(definition)) => Some(definition),
                                    _ => None,
                                },
                            };
                            drop(_definition_span);
                            let Some(definition) = definition else {
                                continue;
                            };
                            let _navigation_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.navigation")).entered();
                            let Some(nav) = definition.try_to_nav(&sema).map(|nav| nav.call_site)
                            else {
                                continue;
                            };
                            let Some(target_path) = paths.get(&nav.file_id) else {
                                continue;
                            };
                            let target_offsets =
                                destination_offsets.entry(nav.file_id).or_insert_with(|| {
                                    let text = sema
                                        .parse_guess_edition(nav.file_id)
                                        .syntax()
                                        .text()
                                        .to_string();
                                    OffsetMap::new(&text)
                                });
                            found.insert(TargetCall {
                                source_path: source_path.to_string(),
                                caller_name: target_owner_name(name_ref.syntax(), &source_offsets),
                                enclosing_name: enclosing_name(name_ref.syntax()),
                                site_start: start,
                                site_end: end,
                                target_path: target_path.clone(),
                                target_start: target_offsets.to_span_offset(u32::from(
                                    nav.focus_range.unwrap_or(nav.full_range).start(),
                                )),
                                target_end: target_offsets
                                    .to_span_offset(u32::from(nav.full_range.end())),
                                target_name: nav.name.as_str().to_string(),
                            });
                        }
                        drop(_scan_span);
                    }
                    drop(_source_files_span);
                    found.into_iter().collect()
                })
            })
            .collect()
    });
    Ok(per_chunk
        .into_iter()
        .flatten()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}
