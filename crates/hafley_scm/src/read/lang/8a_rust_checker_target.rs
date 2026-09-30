//! Resolve only call sites named by a targeted graph question.

use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default)]
pub struct TargetCalls {
    pub calls: Vec<TargetCall>,
    pub loaded: BTreeSet<String>,
}

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
/// With `reaching`, only source crates that depend on one of those files' crates count:
/// a reference search never looks anywhere else.
pub(super) fn prime_crate_closure(
    db: &RootDatabase,
    ids: &HashMap<String, ra_ap_ide::FileId>,
    sources: &[&str],
    reaching: &[ra_ap_ide::FileId],
    threads: usize,
) {
    use ra_ap_ide_db::base_db;
    let _prime = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.prime_crate_closure")).entered();
    let targets: BTreeSet<base_db::Crate> = reaching
        .iter()
        .flat_map(|&file_id| base_db::relevant_crates(db, file_id).to_vec())
        .collect();
    let crates: BTreeSet<base_db::Crate> = sources
        .iter()
        .filter_map(|source| ids.get(*source))
        .flat_map(|&file_id| base_db::relevant_crates(db, file_id).to_vec())
        .collect();
    let mut scope: Vec<base_db::Crate> = crates
        .into_iter()
        .map(|krate| krate.transitive_deps(db))
        .filter(|closure| targets.is_empty() || closure.iter().any(|krate| targets.contains(krate)))
        .flatten()
        .collect();
    scope.sort();
    scope.dedup();
    ra_ap_ide_db::prime_caches::parallel_prime_caches(db, &scope, threads, &|_| {});
}


/// The name reference is the callee of a call: `f(..)`, `m::f(..)`, or `x.f(..)`.
fn is_call(name_ref: &ast::NameRef) -> bool {
    let node = name_ref.syntax();
    if let Some(call) = node.parent().and_then(ast::MethodCallExpr::cast) {
        return call.name_ref().as_ref() == Some(name_ref);
    }
    node.ancestors()
        .find_map(ast::PathExpr::cast)
        .and_then(|expr| {
            let path = expr.path()?;
            let last = path.segment()?.name_ref()?;
            let call = expr.syntax().parent().and_then(ast::CallExpr::cast)?;
            Some(&last == name_ref && call.expr()?.syntax() == expr.syntax())
        })
        .unwrap_or(false)
}

/// Every call of each seeded function, found by rust-analyzer's reference search,
/// and the supplied files rust-analyzer loaded (its answer covers only those).
///
/// Seeds are `(definition path, name)`. Aliased imports and calls inside macro
/// expansions are references of the definition, so they are found without a name match.
pub fn target_calls(
    root: &Path,
    files: &[(String, PathBuf)],
    seeds: &[(String, String)],
    budget: Duration,
) -> Result<TargetCalls, CheckerError> {
    if seeds.is_empty() {
        return Ok(TargetCalls::default());
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
    drop(_file_index_span);
    let db = workspace.host.raw_database();
    let _query_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.queries")).entered();
    let supplied: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
    let definitions: Vec<_> = seeds.iter().filter_map(|(path, _)| ids.get(path).copied()).collect();
    prime_crate_closure(
        db,
        &ids,
        &supplied,
        &definitions,
        crate::read::project::extract_pool().current_num_threads(),
    );
    attach_db(db, || {
        let sema = Semantics::new(db);
        let mut found = BTreeSet::new();
        let mut offsets: HashMap<ra_ap_ide::FileId, OffsetMap> = HashMap::new();
        for (definition_path, name) in seeds {
            let Some(&definition_file) = ids.get(definition_path) else {
                continue;
            };
            let syntax = sema.parse_guess_edition(definition_file);
            let functions: Vec<_> = syntax
                .syntax()
                .descendants()
                .filter_map(ast::Fn::cast)
                .filter(|item| item.name().is_some_and(|item_name| item_name.text() == name.as_str()))
                .filter_map(|item| sema.to_def(&item))
                .collect();
            for function in functions {
                let definition = Definition::Function(function);
                let Some(nav) = definition.try_to_nav(&sema).map(|nav| nav.call_site) else {
                    continue;
                };
                let Some(target_path) = paths.get(&nav.file_id) else {
                    continue;
                };
                let _usages_span = crate::read::trace::tracked(tracing::info_span!("rust_analyzer.usages")).entered();
                let direct = definition.usages(&sema).all();
                // `usages` does not follow `use x as y` (rust-analyzer issue #14079);
                // each renaming import is searched again under its alias.
                let renames: Vec<ast::Rename> = direct
                    .iter()
                    .flat_map(|(_, references)| references.iter())
                    .filter_map(|reference| reference.name.as_name_ref())
                    .filter_map(|name_ref| {
                        name_ref.syntax().ancestors().find_map(ast::UseTree::cast)?.rename()
                    })
                    .collect();
                let mut references = direct.references;
                for rename in &renames {
                    for (file, aliased) in definition.usages(&sema).with_rename(Some(rename)).all() {
                        references.entry(file).or_default().extend(aliased);
                    }
                }
                drop(_usages_span);
                for (file, references) in references {
                    let source_id = file.file_id(db);
                    let Some(source_path) = paths.get(&source_id) else {
                        continue;
                    };
                    for id in [source_id, nav.file_id] {
                        offsets.entry(id).or_insert_with(|| {
                            OffsetMap::new(&sema.parse_guess_edition(id).syntax().text().to_string())
                        });
                    }
                    let source_offsets = &offsets[&source_id];
                    let target_offsets = &offsets[&nav.file_id];
                    for reference in references {
                        let Some(name_ref) = reference.name.as_name_ref() else {
                            continue;
                        };
                        if !is_call(name_ref) {
                            continue;
                        }
                        // A reference inside a macro expansion lives in the macro's tree;
                        // its caller is read at the original range in the source file.
                        let site = match sema
                            .parse_guess_edition(source_id)
                            .syntax()
                            .covering_element(reference.range)
                        {
                            ra_ap_syntax::NodeOrToken::Node(node) => node,
                            ra_ap_syntax::NodeOrToken::Token(token) => token.parent().unwrap(),
                        };
                        found.insert(TargetCall {
                            source_path: source_path.clone(),
                            caller_name: target_owner_name(&site, source_offsets),
                            enclosing_name: enclosing_name(&site),
                            site_start: source_offsets.to_span_offset(u32::from(reference.range.start())),
                            site_end: source_offsets.to_span_offset(u32::from(reference.range.end())),
                            target_path: target_path.clone(),
                            target_start: target_offsets.to_span_offset(u32::from(
                                nav.focus_range.unwrap_or(nav.full_range).start(),
                            )),
                            target_end: target_offsets.to_span_offset(u32::from(nav.full_range.end())),
                            target_name: nav.name.as_str().to_string(),
                        });
                    }
                }
            }
        }
        Ok(TargetCalls {
            calls: found.into_iter().collect(),
            loaded: paths.into_values().collect(),
        })
    })
}
