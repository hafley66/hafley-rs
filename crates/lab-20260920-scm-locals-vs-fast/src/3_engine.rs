use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::{
    matches_only, Analysis, Capture, LabError, NamedEdge, NodeKind, ScmRow, Store, Unresolved,
};

#[derive(Clone)]
struct SpanNode {
    id: i64,
    start: u32,
    end: u32,
}

#[derive(Clone)]
struct Reference {
    id: i64,
    path: String,
    name: String,
    owner: String,
}

pub fn analyze(language: &str, query: &str, paths: &[PathBuf]) -> Result<Analysis, LabError> {
    let store = Store::memory()?;
    let mut roots = Vec::new();
    let mut references = Vec::new();
    let mut imports = Vec::new();
    let mut rows = Vec::new();

    for path in paths {
        let source = std::fs::read(path).map_err(|error| LabError::Io(error.to_string()))?;
        let path_text = path.to_string_lossy().to_string();
        let output = matches_only(language, query, &source)?;
        if output.did_exceed_match_limit {
            return Err(LabError::MatchLimitExceeded {
                language: language.into(),
                path: path_text,
            });
        }
        let root = store.node(NodeKind::Root, "", &path_text, 0, source.len() as u32)?;
        roots.push(root);
        ingest_file(
            &store,
            root,
            &path_text,
            source.len() as u32,
            output.captures,
            &mut references,
            &mut imports,
            &mut rows,
        )?;
    }

    for (import, own_root) in imports {
        for root in roots.iter().copied().filter(|root| *root != own_root) {
            store.edge(import, root)?;
        }
    }
    for own_root in roots.iter().copied() {
        for root in roots.iter().copied().filter(|root| *root != own_root) {
            store.edge(own_root, root)?;
        }
    }

    let mut analysis = Analysis::default();
    for reference in references {
        let targets = store.resolve(reference.id)?;
        if targets.is_empty() {
            analysis.unresolved.push(Unresolved {
                path: reference.path,
                name: reference.name,
                reason: "no_graph_path".into(),
            });
        } else {
            for (callee_name, callee_path, _, _) in targets {
                rows.push(ScmRow::ScipRef {
                    file: reference.path.clone(),
                    symbol: symbol(&callee_path, &callee_name),
                    def_file: callee_path.clone(),
                    repo: "scm".into(),
                });
                analysis.edges.push(NamedEdge {
                    caller_path: reference.path.clone(),
                    caller_name: reference.owner.clone(),
                    callee_path,
                    callee_name,
                });
            }
        }
    }
    analysis.edges.sort();
    analysis.edges.dedup();
    analysis.unresolved.sort();
    rows.sort();
    rows.dedup();
    analysis.rows = rows;
    Ok(analysis)
}

fn ingest_file(
    store: &Store,
    root: i64,
    path: &str,
    file_end: u32,
    captures: Vec<Capture>,
    references: &mut Vec<Reference>,
    imports: &mut Vec<(i64, i64)>,
    rows: &mut Vec<ScmRow>,
) -> Result<(), LabError> {
    let mut scopes = vec![SpanNode {
        id: root,
        start: 0,
        end: file_end,
    }];
    let scope_spans = unique(&captures, |label| label == "local.scope");
    for capture in &scope_spans {
        scopes.push(SpanNode {
            id: store.node(NodeKind::Scope, "", path, capture.start, capture.end)?,
            start: capture.start,
            end: capture.end,
        });
    }
    for scope in scopes.iter().skip(1) {
        let parent = containing(&scopes, scope.start, scope.end, Some(scope.id)).unwrap();
        store.edge(scope.id, parent.id)?;
    }

    let definitions = unique(&captures, |label| label.starts_with("local.definition"));
    let owner_names = scope_names(&scope_spans, &definitions);
    let mut definition_spans = BTreeSet::new();
    for definition in &definitions {
        definition_spans.insert((definition.start, definition.end));
        let direct_owner = containing(&scopes, definition.start, definition.end, None).unwrap();
        let owner = if definition.label == "local.definition.function"
            || definition.label == "local.definition.type"
        {
            containing(
                &scopes,
                direct_owner.start,
                direct_owner.end,
                Some(direct_owner.id),
            )
            .unwrap_or(direct_owner)
        } else {
            direct_owner
        };
        let pop = store.node(
            NodeKind::Pop,
            &definition.text,
            path,
            definition.start,
            definition.end,
        )?;
        let def = store.node(
            NodeKind::Def,
            &definition.text,
            path,
            definition.start,
            definition.end,
        )?;
        store.edge(owner.id, pop)?;
        store.edge(pop, def)?;
        rows.push(ScmRow::ScipDef {
            symbol: symbol(path, &definition.text),
            file: path.into(),
            repo: "scm".into(),
        });
        if definition.label.contains("variable") {
            let enclosing_fn = containing_capture(&scope_spans, definition.start, definition.end)
                .and_then(|span| owner_names.get(&(span.start, span.end)).cloned())
                .unwrap_or_else(|| "<root>".into());
            rows.push(ScmRow::ScipLocal {
                enclosing_fn,
                name: definition.text.clone(),
            });
        }
        if owner.id == root {
            let export = store.node(
                NodeKind::Export,
                &definition.text,
                path,
                definition.start,
                definition.end,
            )?;
            store.edge(export, def)?;
        }
    }

    let calls = unique(&captures, |label| label == "local.call");
    for call in calls {
        if definition_spans.contains(&(call.start, call.end)) {
            continue;
        }
        let owner_scope = containing_capture(&scope_spans, call.start, call.end);
        let owner = containing(&scopes, call.start, call.end, None).unwrap();
        let reference = store.node(NodeKind::Ref, &call.text, path, call.start, call.end)?;
        let push = store.node(NodeKind::Push, &call.text, path, call.start, call.end)?;
        store.edge(reference, push)?;
        store.edge(push, owner.id)?;
        references.push(Reference {
            id: reference,
            path: path.into(),
            name: call.text,
            owner: owner_scope
                .and_then(|span| owner_names.get(&(span.start, span.end)).cloned())
                .unwrap_or_else(|| "<root>".into()),
        });
    }

    for capture in unique(&captures, |label| label == "local.import") {
        let import = store.node(
            NodeKind::Import,
            &capture.text,
            path,
            capture.start,
            capture.end,
        )?;
        store.edge(root, import)?;
        imports.push((import, root));
    }
    for capture in unique(&captures, |label| label == "local.export.package") {
        let export = store.node(
            NodeKind::Export,
            &capture.text,
            path,
            capture.start,
            capture.end,
        )?;
        store.edge(export, root)?;
    }
    Ok(())
}

fn symbol(path: &str, name: &str) -> String {
    format!("scm . . `{path}`/{name}().")
}

fn unique(captures: &[Capture], accepts: impl Fn(&str) -> bool) -> Vec<Capture> {
    captures
        .iter()
        .filter(|capture| accepts(&capture.label))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn containing<'a>(
    scopes: &'a [SpanNode],
    start: u32,
    end: u32,
    skip: Option<i64>,
) -> Option<&'a SpanNode> {
    scopes
        .iter()
        .filter(|scope| skip != Some(scope.id) && scope.start <= start && end <= scope.end)
        .min_by_key(|scope| scope.end - scope.start)
}

fn containing_capture(scopes: &[Capture], start: u32, end: u32) -> Option<&Capture> {
    scopes
        .iter()
        .filter(|scope| scope.start <= start && end <= scope.end)
        .min_by_key(|scope| scope.end - scope.start)
}

fn scope_names(scopes: &[Capture], definitions: &[Capture]) -> BTreeMap<(u32, u32), String> {
    scopes
        .iter()
        .filter_map(|scope| {
            definitions
                .iter()
                .filter(|def| scope.start <= def.start && def.end <= scope.end)
                .min_by_key(|def| def.start)
                .map(|def| ((scope.start, scope.end), def.text.clone()))
        })
        .collect()
}

pub fn paths_under(root: &Path, extension: &str) -> Result<Vec<PathBuf>, LabError> {
    let mut paths = std::fs::read_dir(root)
        .map_err(|error| LabError::Io(error.to_string()))?
        .filter_map(Result::ok)
        .flat_map(|entry| {
            let path = entry.path();
            if path.is_dir() {
                paths_under(&path, extension).unwrap_or_default()
            } else if path.extension().and_then(|part| part.to_str()) == Some(extension) {
                vec![path]
            } else {
                Vec::new()
            }
        })
        .collect::<Vec<_>>();
    paths.sort();
    Ok(paths)
}
