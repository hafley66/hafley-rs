//! `ryi fast`'s symbol / occurrence / local rows, from a per-language `.scm`
//! query run through the shared `hafley_scm` engine, plus the file's scope tree.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ast_grep_core::tree_sitter::LanguageExt;
use tree_sitter::Parser;

use super::extract_lang::RyiLang;
use super::scm_store::{NodeKind, Store};
use crate::types::FlatFact;

const ROOT: usize = 0;

#[derive(Debug)]
pub enum ScmError {
    /// A supplied path could not be read.
    Io { path: String, detail: String },
    /// The engine refused the bundled query.
    Query { path: String, detail: String },
    /// tree-sitter stopped enumerating matches, so the captures are partial.
    MatchLimit { path: String },
    /// The phase-5 scope graph refused a statement.
    Sql(String),
}

impl std::fmt::Display for ScmError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, detail } => write!(out, "{path}: {detail}"),
            Self::Query { path, detail } => write!(out, "{path}: {detail}"),
            Self::MatchLimit { path } => write!(
                out,
                "{path}: the fast scm query exceeded the tree-sitter match limit"
            ),
            Self::Sql(detail) => out.write_str(detail),
        }
    }
}

impl std::error::Error for ScmError {}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Capture {
    label: String,
    text: String,
    start: u32,
    end: u32,
}

struct Scope {
    start: u32,
    end: u32,
    parent: Option<usize>,
}

struct Definition {
    name: String,
    kind: String,
    start: u32,
    end: u32,
    owner: usize,
    /// The whole declaration the name heads, from the query's own span capture.
    decl: (u32, u32),
}

/// One call resolved across the supplied file set, named the way the lab's
/// judge keys an edge.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ScmEdge {
    pub caller_path: String,
    pub caller_name: String,
    pub callee_path: String,
    pub callee_name: String,
}

struct SpanNode {
    id: i64,
    start: u32,
    end: u32,
}

struct Reference {
    id: i64,
    path: String,
    owner: String,
}

/// THE CROSS-FILE PASS, off the family path: one scope graph over every
/// supplied file, resolved by the recursive walk in `8_scm_store.rs`.
pub fn scm_edges(paths: &[PathBuf]) -> Result<Vec<ScmEdge>, ScmError> {
    let store = Store::memory()?;
    let mut roots = Vec::new();
    let mut references = Vec::new();
    let mut imports = Vec::new();
    for path in expand(paths)? {
        let Some((name, end, captured)) = file_captures(&path)? else {
            continue;
        };
        let root = store.node(NodeKind::Root, "", &name, 0, end)?;
        roots.push(root);
        ingest(
            &store,
            root,
            &name,
            end,
            &captured.into_iter().collect::<Vec<_>>(),
            &mut references,
            &mut imports,
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
    let mut edges = Vec::new();
    for reference in references {
        for (callee_name, callee_path) in store.resolve(reference.id)? {
            edges.push(ScmEdge {
                caller_path: reference.path.clone(),
                caller_name: reference.owner.clone(),
                callee_path,
                callee_name,
            });
        }
    }
    edges.sort();
    edges.dedup();
    Ok(edges)
}

/// One file's captures as graph nodes: scopes under their parents, definitions
/// behind a pop, calls behind a push, imports and package exports at the root.
fn ingest(
    store: &Store,
    root: i64,
    path: &str,
    file_end: u32,
    captures: &[Capture],
    references: &mut Vec<Reference>,
    imports: &mut Vec<(i64, i64)>,
) -> Result<(), ScmError> {
    let mut nodes = vec![SpanNode {
        id: root,
        start: 0,
        end: file_end,
    }];
    let scope_spans = labelled(captures, |label| label == "local.scope");
    for span in &scope_spans {
        nodes.push(SpanNode {
            id: store.node(NodeKind::Scope, "", path, span.start, span.end)?,
            start: span.start,
            end: span.end,
        });
    }
    for index in 1..nodes.len() {
        let parent = holder(&nodes, nodes[index].start, nodes[index].end, Some(index));
        store.edge(nodes[index].id, nodes[parent].id)?;
    }

    let definitions = labelled(captures, |label| label.starts_with("local.definition"));
    let owners = span_names(&scope_spans, &definitions);
    let mut definition_spans = BTreeSet::new();
    for definition in &definitions {
        definition_spans.insert((definition.start, definition.end));
        let direct = holder(&nodes, definition.start, definition.end, None);
        let owner = match definition.label.as_str() {
            "local.definition.function" | "local.definition.type" => {
                holder(&nodes, nodes[direct].start, nodes[direct].end, Some(direct))
            }
            _ => direct,
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
        store.edge(nodes[owner].id, pop)?;
        store.edge(pop, def)?;
        if nodes[owner].id == root {
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

    for call in labelled(captures, |label| label == "local.call") {
        if definition_spans.contains(&(call.start, call.end)) {
            continue;
        }
        let owner = holder(&nodes, call.start, call.end, None);
        let reference = store.node(NodeKind::Ref, &call.text, path, call.start, call.end)?;
        let push = store.node(NodeKind::Push, &call.text, path, call.start, call.end)?;
        store.edge(reference, push)?;
        store.edge(push, nodes[owner].id)?;
        references.push(Reference {
            id: reference,
            path: path.to_string(),
            owner: containing_span(&scope_spans, call.start, call.end)
                .and_then(|span| owners.get(&(span.start, span.end)).cloned())
                .unwrap_or_else(|| "<root>".into()),
        });
    }

    for capture in labelled(captures, |label| label == "local.import") {
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
    for capture in labelled(captures, |label| label == "local.export.package") {
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

fn holder(nodes: &[SpanNode], start: u32, end: u32, skip: Option<usize>) -> usize {
    nodes
        .iter()
        .enumerate()
        .filter(|(index, node)| skip != Some(*index) && node.start <= start && end <= node.end)
        .min_by_key(|(_, node)| node.end - node.start)
        .map(|(index, _)| index)
        .unwrap_or(ROOT)
}

fn containing_span<'a>(spans: &'a [Capture], start: u32, end: u32) -> Option<&'a Capture> {
    spans
        .iter()
        .filter(|span| span.start <= start && end <= span.end)
        .min_by_key(|span| span.end - span.start)
}

fn span_names(spans: &[Capture], definitions: &[Capture]) -> BTreeMap<(u32, u32), String> {
    spans
        .iter()
        .filter_map(|span| {
            definitions
                .iter()
                .filter(|def| span.start <= def.start && def.end <= span.end)
                .min_by_key(|def| def.start)
                .map(|def| ((span.start, span.end), def.text.clone()))
        })
        .collect()
}

/// Every pass-1 row for the supplied files, in emission order. One file's rows
/// are a function of that file alone.
pub fn scm_facts(paths: &[PathBuf]) -> Result<Vec<FlatFact>, ScmError> {
    let mut facts = Vec::new();
    for path in expand(paths)? {
        facts.extend(file_facts(&path)?);
    }
    Ok(facts)
}

/// Every supplied path, filtered to the files a bundled query covers: a
/// language with no `.scm` yet contributes no rows to fast.
fn expand(paths: &[PathBuf]) -> Result<Vec<PathBuf>, ScmError> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut covered = Vec::new();
            walk(path, &mut covered)?;
            covered.sort();
            files.extend(covered);
        } else if query_for(&path.to_string_lossy()).is_some() {
            files.push(path.clone());
        }
    }
    Ok(files)
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), ScmError> {
    let entries = std::fs::read_dir(dir).map_err(|error| ScmError::Io {
        path: dir.to_string_lossy().to_string(),
        detail: error.to_string(),
    })?;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, files)?;
        } else if query_for(&path.to_string_lossy()).is_some() {
            files.push(path);
        }
    }
    Ok(())
}

/// The bundled query for a path's language, with the grammar it executes on.
/// Both come off the `Source` roster, so no language is named here.
fn query_for(path: &str) -> Option<(RyiLang, &'static str)> {
    let source = super::source_for(path)?;
    Some((source.extract_lang(path)?, source.scm_query(path)?))
}

fn file_facts(path: &Path) -> Result<Vec<FlatFact>, ScmError> {
    let Some((name, end, captured)) = file_captures(path)? else {
        return Ok(Vec::new());
    };
    Ok(rows(&name, end, captured))
}

/// One file's pass: the engine builds the bundled query once and runs it
/// natively; the arena's captures are every later projection's input.
fn file_captures(path: &Path) -> Result<Option<(String, u32, BTreeSet<Capture>)>, ScmError> {
    let name = path.to_string_lossy().to_string();
    let Some((lang, query_text)) = query_for(&name) else {
        return Ok(None);
    };
    let source = std::fs::read(path).map_err(|error| ScmError::Io {
        path: name.clone(),
        detail: error.to_string(),
    })?;
    let captured = arena_captures(&name, lang, query_text, &source)?;
    Ok(Some((name, source.len() as u32, captured)))
}

/// One build and one native run: the engine applies the query's own
/// predicates and appends every kept match's captures to the arena.
fn arena_captures(
    path: &str,
    lang: RyiLang,
    query_text: &str,
    source: &[u8],
) -> Result<BTreeSet<Capture>, ScmError> {
    let language = lang.get_ts_language();
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|error| ScmError::Query {
            path: path.to_string(),
            detail: format!("set language: {error}"),
        })?;
    let tree = parser.parse(source, None).ok_or_else(|| ScmError::Query {
        path: path.to_string(),
        detail: "parse returned no tree".into(),
    })?;
    let query = hafley_scm::build(&language, query_text).map_err(|error| scm_error(path, error))?;
    let mut arena = hafley_scm::MatchArena::default();
    // The fresh-cursor default the direct run always had; the engine's limit
    // check cannot fire at u32::MAX.
    hafley_scm::run(&query, path, source, &tree, u32::MAX, &mut arena)
        .map_err(|error| scm_error(path, error))?;
    Ok(kept_captures(&query, &arena, source))
}

/// MatchArena rows -> the `Capture` set every later projection reads: one
/// entry per kept capture, deduped and ordered by label, text, then span.
fn kept_captures(
    query: &hafley_scm::QueryExt,
    arena: &hafley_scm::MatchArena,
    source: &[u8],
) -> BTreeSet<Capture> {
    let mut kept = BTreeSet::new();
    for row in &arena.rows {
        for span in &arena.spans[row.spans.start as usize..row.spans.end as usize] {
            let text = source
                .get(span.bytes.start as usize..span.bytes.end as usize)
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .unwrap_or("")
                .to_string();
            kept.insert(Capture {
                label: query.names[span.name as usize].to_string(),
                text,
                start: span.bytes.start,
                end: span.bytes.end,
            });
        }
    }
    kept
}

/// The engine's error shape onto this module's path-qualified one.
fn scm_error(path: &str, error: hafley_scm::QueryExtError) -> ScmError {
    match error {
        hafley_scm::QueryExtError::Parse(error) => ScmError::Query {
            path: path.to_string(),
            detail: format!("query row {}: {error}", error.row + 1),
        },
        hafley_scm::QueryExtError::UnknownOperator(operator) => ScmError::Query {
            path: path.to_string(),
            detail: format!("predicate #{operator} is not allowed"),
        },
        hafley_scm::QueryExtError::Arity { operator, got } => ScmError::Query {
            path: path.to_string(),
            detail: format!("predicate #{operator} got {got} arguments"),
        },
        hafley_scm::QueryExtError::MatchLimit { .. } => ScmError::MatchLimit {
            path: path.to_string(),
        },
    }
}

/// The scope tree, the definitions it owns, and the references it resolves,
/// projected onto the three pass-1 rows.
fn rows(path: &str, file_end: u32, captured: BTreeSet<Capture>) -> Vec<FlatFact> {
    let captures: Vec<Capture> = captured.into_iter().collect();
    let scope_spans = labelled(&captures, |label| label == "local.scope");
    let scopes = scope_tree(file_end, &scope_spans);
    let definitions = definitions(&captures, &scopes);
    let names = scope_names(&scopes, &definitions);
    let exports = labelled(&captures, |label| label == "local.export.package");
    let declaring = exports.iter().any(|export| {
        !definitions
            .iter()
            .any(|def| export.start <= def.start && def.end <= export.end)
    });

    let mut facts = Vec::new();
    for def in &definitions {
        let symbol = symbol(path, &def.name);
        let exported = def.owner == ROOT
            && (declaring
                || exports
                    .iter()
                    .any(|export| export.start <= def.start && def.end <= export.end));
        facts.push(FlatFact::SymbolRow {
            symbol: symbol.clone(),
            path: path.to_string(),
            kind: def.kind.clone(),
        });
        facts.push(FlatFact::OccurrenceRow {
            symbol,
            path: path.to_string(),
            start: def.start,
            end: def.end,
            role: "def".into(),
            exported,
            decl_start: def.decl.0,
            decl_end: def.decl.1,
        });
        if !exported {
            facts.push(FlatFact::LocalRow {
                enclosing_fn: match def.owner {
                    ROOT => "<root>".into(),
                    owner => names.get(&owner).cloned().unwrap_or_else(|| "<root>".into()),
                },
                name: def.name.clone(),
                path: path.to_string(),
                start: def.start,
                end: def.end,
            });
        }
    }

    for call in labelled(&captures, |label| label == "local.call") {
        if definitions
            .iter()
            .any(|def| def.start == call.start && def.end == call.end)
        {
            continue;
        }
        let Some(target) = resolve(&call, &scopes, &definitions) else {
            continue;
        };
        facts.push(FlatFact::OccurrenceRow {
            symbol: symbol(path, &target.name),
            path: path.to_string(),
            start: call.start,
            end: call.end,
            role: "ref".into(),
            exported: false,
            decl_start: call.start,
            decl_end: call.end,
        });
    }
    facts.extend(free_names(path, file_end, &captures, &scopes, &definitions));
    facts
}

/// Every occurrence a top-level item needs from outside itself: an identifier
/// the file's scope tree answers at file level, or not at all.
fn free_names(
    path: &str,
    file_end: u32,
    captures: &[Capture],
    scopes: &[Scope],
    definitions: &[Definition],
) -> Vec<FlatFact> {
    let bound: BTreeSet<(u32, u32)> = definitions.iter().map(|def| (def.start, def.end)).collect();
    let imports = labelled(captures, |label| label == "local.import");
    let mut facts = Vec::new();
    for name in labelled(captures, |label| label == "local.reference") {
        if bound.contains(&(name.start, name.end)) {
            continue;
        }
        if containing_span(&imports, name.start, name.end).is_some() {
            continue;
        }
        if resolve(&name, scopes, definitions).is_some_and(|def| def.owner != ROOT) {
            continue;
        }
        let owner = top_level(scopes, containing(scopes, name.start, name.end, None));
        facts.push(FlatFact::FreeNameRow {
            path: path.to_string(),
            owner_start: match owner {
                ROOT => 0,
                index => scopes[index].start,
            },
            owner_end: match owner {
                ROOT => file_end,
                index => scopes[index].end,
            },
            name: name.text.clone(),
            start: name.start,
            end: name.end,
        });
    }
    facts
}

/// The outermost scope under the file that holds `scope`, which is the item a
/// free name travels with. ROOT when the occurrence sits at file level.
fn top_level(scopes: &[Scope], scope: usize) -> usize {
    let mut at = scope;
    while let Some(parent) = scopes[at].parent {
        if parent == ROOT {
            return at;
        }
        at = parent;
    }
    ROOT
}

/// The lexical answer: the innermost scope holding the reference, then its
/// ancestors, and the first definition of the name any of them owns.
fn resolve<'a>(
    call: &Capture,
    scopes: &[Scope],
    definitions: &'a [Definition],
) -> Option<&'a Definition> {
    let mut scope = Some(containing(scopes, call.start, call.end, None));
    while let Some(index) = scope {
        if let Some(found) = definitions
            .iter()
            .find(|def| def.owner == index && def.name == call.text)
        {
            return Some(found);
        }
        scope = scopes[index].parent;
    }
    None
}

fn symbol(path: &str, name: &str) -> String {
    format!("scm . . `{path}`/{name}().")
}

fn labelled(captures: &[Capture], accepts: impl Fn(&str) -> bool) -> Vec<Capture> {
    captures
        .iter()
        .filter(|capture| accepts(&capture.label))
        .cloned()
        .collect()
}

/// Index 0 is the file itself, so every span has an owner.
fn scope_tree(file_end: u32, spans: &[Capture]) -> Vec<Scope> {
    let mut scopes = vec![Scope {
        start: 0,
        end: file_end,
        parent: None,
    }];
    for span in spans {
        scopes.push(Scope {
            start: span.start,
            end: span.end,
            parent: None,
        });
    }
    for index in 1..scopes.len() {
        let (start, end) = (scopes[index].start, scopes[index].end);
        scopes[index].parent = Some(containing(&scopes, start, end, Some(index)));
    }
    scopes
}

/// A function or type belongs to the scope AROUND the one it opens: its own
/// body must not be where its name resolves.
fn definitions(captures: &[Capture], scopes: &[Scope]) -> Vec<Definition> {
    let spans = labelled(captures, |label| label == "local.def.span");
    let mut definitions = Vec::new();
    for capture in captures {
        let Some(kind) = capture.label.strip_prefix("local.definition.") else {
            continue;
        };
        let direct = containing(scopes, capture.start, capture.end, None);
        let owner = if kind == "function" || kind == "type" {
            let (start, end) = (scopes[direct].start, scopes[direct].end);
            containing(scopes, start, end, Some(direct))
        } else {
            direct
        };
        definitions.push(Definition {
            name: capture.text.clone(),
            kind: kind.to_string(),
            start: capture.start,
            end: capture.end,
            owner,
            decl: containing_span(&spans, capture.start, capture.end)
                .map_or((capture.start, capture.end), |span| (span.start, span.end)),
        });
    }
    definitions
}

/// A scope's name is the first definition inside it, which for a function or a
/// class scope is its own declared name.
fn scope_names(scopes: &[Scope], definitions: &[Definition]) -> BTreeMap<usize, String> {
    let mut names = BTreeMap::new();
    for (index, scope) in scopes.iter().enumerate() {
        if let Some(first) = definitions
            .iter()
            .filter(|def| scope.start <= def.start && def.end <= scope.end)
            .min_by_key(|def| def.start)
        {
            names.insert(index, first.name.clone());
        }
    }
    names
}

fn containing(scopes: &[Scope], start: u32, end: u32, skip: Option<usize>) -> usize {
    scopes
        .iter()
        .enumerate()
        .filter(|(index, scope)| {
            skip != Some(*index) && scope.start <= start && end <= scope.end
        })
        .min_by_key(|(_, scope)| scope.end - scope.start)
        .map(|(index, _)| index)
        .unwrap_or(ROOT)
}
