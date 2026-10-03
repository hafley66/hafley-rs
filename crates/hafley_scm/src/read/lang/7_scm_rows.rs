//! `ryi fast`'s symbol / occurrence / local rows, from a per-language `.scm`
//! query run through the shared `hafley_scm` engine, plus the file's scope tree.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use tree_sitter::Parser;

use super::extract_lang::RyiLang;
use super::scm_store::{NodeKind, Store};
use crate::read::types::FlatFact;

const ROOT: usize = 0;
static BUNDLED_QUERIES: OnceLock<
    Mutex<HashMap<(RyiLang, &'static str), Arc<hafley_scm::QueryExt>>>,
> = OnceLock::new();

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

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Capture<'a> {
    label: &'a str,
    text: &'a str,
    start: u32,
    end: u32,
}

/// Path-free query captures retained past the Kotlin parser arena lifetime.
/// The path is supplied when fast emits rows, since dispatch caches by blob.
#[derive(Default)]
pub struct ScmCaptures {
    source: Arc<[u8]>,
    labels: Vec<String>,
    label_ids: Vec<u16>,
    starts: Vec<u32>,
    ends: Vec<u32>,
    /// Preserve match pairing for nested callees such as `factory()()`.
    calls: Vec<(u32, u32, u32, u32)>,
}

impl ScmCaptures {
    pub fn from_arena(
        query: &hafley_scm::QueryExt,
        arena: &hafley_scm::MatchArena,
        source: &[u8],
    ) -> Self {
        Self::from_arena_with_source(query, arena, Arc::from(source))
    }

    fn from_arena_with_source(
        query: &hafley_scm::QueryExt,
        arena: &hafley_scm::MatchArena,
        source: Arc<[u8]>,
    ) -> Self {
        let mut calls = Vec::new();
        let site_label = query
            .names
            .iter()
            .position(|name| name.as_ref() == "syntax.call.span");
        let callee_label = query
            .names
            .iter()
            .position(|name| name.as_ref() == "syntax.call.callee");
        if let (Some(site_label), Some(callee_label)) = (site_label, callee_label) {
            for row in &arena.rows {
                let spans = &arena.spans[row.spans.start as usize..row.spans.end as usize];
                let site = spans.iter().find(|span| span.name as usize == site_label);
                let callee = spans.iter().find(|span| span.name as usize == callee_label);
                if let (Some(site), Some(callee)) = (site, callee) {
                    calls.push((
                        site.bytes.start,
                        site.bytes.end,
                        callee.bytes.start,
                        callee.bytes.end,
                    ));
                }
            }
            calls.sort_unstable();
            calls.dedup();
        }
        let mut captures = kept_captures(query, arena, &source);
        captures.sort_by(|left, right| {
            query.names[left.0 as usize]
                .cmp(&query.names[right.0 as usize])
                .then_with(|| {
                    capture_text(&source, left.1, left.2)
                        .cmp(capture_text(&source, right.1, right.2))
                })
                .then(left.1.cmp(&right.1))
                .then(left.2.cmp(&right.2))
        });
        captures.dedup();
        let mut label_ids = Vec::with_capacity(captures.len());
        let mut starts = Vec::with_capacity(captures.len());
        let mut ends = Vec::with_capacity(captures.len());
        for (label_id, start, end) in captures {
            label_ids.push(label_id);
            starts.push(start);
            ends.push(end);
        }
        Self {
            source,
            labels: query.names.iter().map(|name| name.to_string()).collect(),
            label_ids,
            starts,
            ends,
            calls,
        }
    }

    fn views(&self) -> Vec<Capture<'_>> {
        (0..self.label_ids.len())
            .map(|index| Capture {
                label: &self.labels[self.label_ids[index] as usize],
                text: capture_text(&self.source, self.starts[index], self.ends[index]),
                start: self.starts[index],
                end: self.ends[index],
            })
            .collect()
    }

    pub fn facts(&self, path: &str) -> Vec<FlatFact> {
        let captures = self.views();
        let mut facts = rows(path, self.source.len() as u32, &captures);
        facts.extend(syntax_rows(path, &self.source, &captures, &self.calls));
        facts
    }

    pub fn syntax_facts(&self, path: &str) -> Vec<FlatFact> {
        syntax_rows(path, &self.source, &self.views(), &self.calls)
    }
}

/// Additive written-syntax rows. Captures and their source buffer are retained
/// by the same fast pass; no parser or resolution pass is added here.
fn syntax_rows(
    path: &str,
    source: &[u8],
    captures: &[Capture<'_>],
    calls: &[(u32, u32, u32, u32)],
) -> Vec<FlatFact> {
    let functions = labelled(captures, |label| label == "syntax.fn.span");
    let elements = labelled(captures, |label| label == "syntax.jsx.span");
    if functions.is_empty() && elements.is_empty() && calls.is_empty() {
        return Vec::new();
    }
    let function_nest = Nest::new(functions.iter().map(|c| (c.start, c.end)).collect());
    let element_nest = Nest::new(elements.iter().map(|c| (c.start, c.end)).collect());
    let mut function_names = vec!["<anonymous>"; functions.len()];
    for name in captures.iter().filter(|c| c.label == "syntax.fn.name") {
        if let Some(index) = function_nest.innermost(name.start, name.end, None) {
            function_names[index] = name.text;
        }
    }
    let bindings = labelled(captures, |label| label == "syntax.binding.span");
    let binding_nest = Nest::new(bindings.iter().map(|c| (c.start, c.end)).collect());
    let mut function_order: Vec<usize> = (0..functions.len()).collect();
    function_order.sort_by_key(|&index| {
        (
            functions[index].start,
            std::cmp::Reverse(functions[index].end),
        )
    });
    for name in captures.iter().filter(|c| c.label == "syntax.binding.name") {
        if let Some(binding) = binding_nest.innermost(name.start, name.end, None) {
            let span = bindings[binding];
            let first =
                function_order.partition_point(|&index| functions[index].start < span.start);
            if let Some(&index) = function_order
                .get(first)
                .filter(|&&index| functions[index].end <= span.end)
            {
                function_names[index] = name.text;
            }
        }
    }
    let owner = |capture: &Capture<'_>| {
        function_nest
            .innermost(capture.start, capture.end, None)
            .map_or("<root>", |index| function_names[index])
            .to_string()
    };
    let newlines: Vec<usize> = memchr::memchr_iter(b'\n', source).collect();
    let line =
        |start: u32| (newlines.partition_point(|&offset| offset < start as usize) + 1) as u32;
    let mut facts = Vec::new();
    for &(start, end, callee_start, callee_end) in calls {
        let call = Capture {
            label: "syntax.call.span",
            text: "",
            start,
            end,
        };
        facts.push(FlatFact::CallSiteRow {
            callee: capture_text(source, callee_start, callee_end).to_string(),
            path: path.to_string(),
            line: line(call.start),
            enclosing_fn: owner(&call),
            start: call.start,
            end: call.end,
        });
    }
    let mut element_names = vec!["<fragment>"; elements.len()];
    for name in captures.iter().filter(|c| c.label == "syntax.jsx.name") {
        if let Some(index) = element_nest.innermost(name.start, name.end, None) {
            element_names[index] = name.text;
        }
    }
    for (index, element) in elements.iter().enumerate() {
        facts.push(FlatFact::JsxElementRow {
            name: element_names[index].to_string(),
            path: path.to_string(),
            line: line(element.start),
            enclosing_fn: owner(element),
            start: element.start,
            end: element.end,
            parent_start: element_nest
                .innermost(element.start, element.end, Some(index))
                .map(|parent| elements[parent].start),
        });
    }
    let attributes = labelled(captures, |label| label == "syntax.jsx.attr.span");
    let attribute_nest = Nest::new(attributes.iter().map(|c| (c.start, c.end)).collect());
    let mut names = vec![None; attributes.len()];
    let mut values = vec![None; attributes.len()];
    for capture in captures {
        let target = match capture.label {
            "syntax.jsx.attr.name" => &mut names,
            "syntax.jsx.attr.value" => &mut values,
            _ => continue,
        };
        if let Some(index) = attribute_nest.innermost(capture.start, capture.end, None) {
            target[index] = Some(capture.text);
        }
    }
    for (index, attribute) in attributes.iter().enumerate() {
        let Some(element) = element_nest.innermost(attribute.start, attribute.end, None) else {
            continue;
        };
        let Some(name) = names[index] else { continue };
        facts.push(FlatFact::JsxAttributeRow {
            path: path.to_string(),
            element_start: elements[element].start,
            name: name.to_string(),
            value: values[index].map(str::to_string),
            start: attribute.start,
            end: attribute.end,
        });
    }
    let holders = Nest::new(
        labelled(captures, |label| label == "syntax.jsx.spread.holder")
            .iter()
            .map(|c| (c.start, c.end))
            .collect(),
    );
    for spread in captures
        .iter()
        .filter(|c| c.label == "syntax.jsx.spread.span")
    {
        if holders.innermost(spread.start, spread.end, None).is_none() {
            continue;
        }
        let Some(element) = element_nest.innermost(spread.start, spread.end, None) else {
            continue;
        };
        facts.push(FlatFact::JsxAttributeRow {
            path: path.to_string(),
            element_start: elements[element].start,
            name: "..".into(),
            value: Some(spread.text.to_string()),
            start: spread.start,
            end: spread.end,
        });
    }
    facts
}

fn capture_text(source: &[u8], start: u32, end: u32) -> &str {
    source
        .get(start as usize..end as usize)
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .unwrap_or("")
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
        let Some((name, captured)) = file_captures(&path, None)? else {
            continue;
        };
        let end = captured.source.len() as u32;
        let root = store.node(NodeKind::Root, "", &name, 0, end)?;
        roots.push(root);
        let captures = captured.views();
        ingest(
            &store,
            root,
            &name,
            end,
            &captures,
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
    captures: &[Capture<'_>],
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
        let owner = match definition.label {
            "local.definition.function" | "local.definition.type" => {
                holder(&nodes, nodes[direct].start, nodes[direct].end, Some(direct))
            }
            _ => direct,
        };
        let pop = store.node(
            NodeKind::Pop,
            definition.text,
            path,
            definition.start,
            definition.end,
        )?;
        let def = store.node(
            NodeKind::Def,
            definition.text,
            path,
            definition.start,
            definition.end,
        )?;
        store.edge(nodes[owner].id, pop)?;
        store.edge(pop, def)?;
        if nodes[owner].id == root {
            let export = store.node(
                NodeKind::Export,
                definition.text,
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
        let reference = store.node(NodeKind::Ref, call.text, path, call.start, call.end)?;
        let push = store.node(NodeKind::Push, call.text, path, call.start, call.end)?;
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
            capture.text,
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
            capture.text,
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

fn containing_span<'a, 'source>(
    spans: &'a [Capture<'source>],
    start: u32,
    end: u32,
) -> Option<&'a Capture<'source>> {
    spans
        .iter()
        .filter(|span| span.start <= start && end <= span.end)
        .min_by_key(|span| span.end - span.start)
}

fn span_names(spans: &[Capture<'_>], definitions: &[Capture<'_>]) -> BTreeMap<(u32, u32), String> {
    spans
        .iter()
        .filter_map(|span| {
            definitions
                .iter()
                .filter(|def| span.start <= def.start && def.end <= span.end)
                .min_by_key(|def| def.start)
                .map(|def| ((span.start, span.end), def.text.to_string()))
        })
        .collect()
}

/// Every pass-1 row for the supplied files, in emission order. One file's rows
/// are a function of that file alone.
pub fn scm_facts(paths: &[PathBuf]) -> Result<Vec<FlatFact>, ScmError> {
    scm_facts_at(paths, None)
}

pub(crate) fn scm_facts_at(
    paths: &[PathBuf],
    root: Option<&Path>,
) -> Result<Vec<FlatFact>, ScmError> {
    let mut facts = Vec::new();
    for path in expand(paths)? {
        facts.extend(file_facts(&path, root)?);
    }
    Ok(facts)
}

/// The supplied files a bundled query covers: a language with no `.scm` yet
/// contributes no rows to fast. Directories are the caller's to expand.
fn expand(paths: &[PathBuf]) -> Result<Vec<PathBuf>, ScmError> {
    Ok(paths
        .iter()
        .filter(|path| query_for(&path.to_string_lossy()).is_some())
        .cloned()
        .collect())
}

/// The bundled query for a path's language, with the grammar it executes on.
/// Both come off the `Source` roster, so no language is named here.
fn query_for(path: &str) -> Option<(RyiLang, &'static str)> {
    let source = super::source_for(path)?;
    Some((source.extract_lang(path)?, source.scm_query(path)?))
}

fn file_facts(path: &Path, root: Option<&Path>) -> Result<Vec<FlatFact>, ScmError> {
    let Some((name, captured)) = file_captures(path, root)? else {
        return Ok(Vec::new());
    };
    Ok(captured.facts(&name))
}

/// One file's pass: the engine runs the cached bundled query; the arena's
/// captures are every later projection's input.
fn file_captures(
    path: &Path,
    root: Option<&Path>,
) -> Result<Option<(String, ScmCaptures)>, ScmError> {
    let name = path.to_string_lossy().to_string();
    let Some((lang, query_text)) = query_for(&name) else {
        return Ok(None);
    };
    let io_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.map_or_else(|| path.to_path_buf(), |root| root.join(path))
    };
    let source: Arc<[u8]> = Arc::from(std::fs::read(crate::read::io_path(&io_path)).map_err(
        |error| ScmError::Io {
            path: name.clone(),
            detail: error.to_string(),
        },
    )?);
    let captured = arena_captures(&name, lang, query_text, source)?;
    Ok(Some((name, captured)))
}

/// One cached build per grammar/query and one native run per file. The engine
/// applies predicates and appends kept captures to the arena.
fn arena_captures(
    path: &str,
    lang: RyiLang,
    query_text: &'static str,
    source: Arc<[u8]>,
) -> Result<ScmCaptures, ScmError> {
    let language = lang.tree_sitter_language();
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|error| ScmError::Query {
            path: path.to_string(),
            detail: format!("set language: {error}"),
        })?;
    let tree = parser.parse(&source, None).ok_or_else(|| ScmError::Query {
        path: path.to_string(),
        detail: "parse returned no tree".into(),
    })?;
    let mut queries = BUNDLED_QUERIES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("bundled query cache is not poisoned");
    let query = if let Some(query) = queries.get(&(lang, query_text)) {
        Arc::clone(query)
    } else {
        let query = Arc::new(
            hafley_scm::build(&language, query_text).map_err(|error| scm_error(path, error))?,
        );
        queries.insert((lang, query_text), Arc::clone(&query));
        query
    };
    drop(queries);
    let mut arena = hafley_scm::MatchArena::default();
    // The fresh-cursor default the direct run always had; the engine's limit
    // check cannot fire at u32::MAX.
    hafley_scm::run(&query, path, &source, &tree, u32::MAX, &mut arena)
        .map_err(|error| scm_error(path, error))?;
    Ok(ScmCaptures::from_arena_with_source(&query, &arena, source))
}

/// MatchArena rows -> a columnar set of label ids and spans; source text stays
/// in the per-file backing buffer until a row needs it.
fn kept_captures(
    query: &hafley_scm::QueryExt,
    arena: &hafley_scm::MatchArena,
    source: &[u8],
) -> Vec<(u16, u32, u32)> {
    let mut kept = Vec::new();
    for row in &arena.rows {
        for span in &arena.spans[row.spans.start as usize..row.spans.end as usize] {
            let label_id = span.name;
            let start = span.bytes.start;
            let end = span.bytes.end;
            let _ = query.names[label_id as usize];
            let _ = capture_text(source, start, end);
            kept.push((label_id, start, end));
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
        hafley_scm::QueryExtError::Scmpp(error) => ScmError::Query {
            path: path.to_string(),
            detail: error.to_string(),
        },
        hafley_scm::QueryExtError::RelationPredicate { pattern, op } => ScmError::Query {
            path: path.to_string(),
            detail: format!(
                "pattern {pattern}: #{op} is a relation predicate; use ryii query --scmpp"
            ),
        },
        hafley_scm::QueryExtError::UnknownOperator(operator) => ScmError::Query {
            path: path.to_string(),
            detail: format!("predicate #{operator} is not allowed"),
        },
        hafley_scm::QueryExtError::Arity { operator, got } => ScmError::Query {
            path: path.to_string(),
            detail: format!("predicate #{operator} got {got} arguments"),
        },
        hafley_scm::QueryExtError::DuplicateField(key) => ScmError::Query {
            path: path.to_string(),
            detail: format!("emission repeats field {key}"),
        },
        hafley_scm::QueryExtError::MatchLimit { .. } => ScmError::MatchLimit {
            path: path.to_string(),
        },
    }
}

/// Spans indexed for the innermost-holder question: which span is the smallest
/// one holding `[start, end)`, ties to the lowest index. tree-sitter nodes nest,
/// so the answer walks up from the last span starting at or before `start`;
/// a set that does not nest falls back to the linear scan.
struct Nest {
    spans: Vec<(u32, u32)>,
    /// Span indices by (start asc, end desc, index desc): among identical spans
    /// the lowest index sits deepest, so it is reached first.
    order: Vec<usize>,
    starts: Vec<u32>,
    up: Vec<Option<usize>>,
    nested: bool,
}

impl Nest {
    fn new(spans: Vec<(u32, u32)>) -> Self {
        let mut order: Vec<usize> = (0..spans.len()).collect();
        order.sort_by(|&a, &b| {
            spans[a]
                .0
                .cmp(&spans[b].0)
                .then(spans[b].1.cmp(&spans[a].1))
                .then(b.cmp(&a))
        });
        let mut up = vec![None; spans.len()];
        let mut stack: Vec<usize> = Vec::new();
        let mut nested = true;
        for &index in &order {
            let (start, end) = spans[index];
            while let Some(&top) = stack.last() {
                if end <= spans[top].1 {
                    break;
                }
                if start < spans[top].1 {
                    nested = false;
                }
                stack.pop();
            }
            up[index] = stack.last().copied();
            stack.push(index);
        }
        let starts = order.iter().map(|&index| spans[index].0).collect();
        Self {
            spans,
            order,
            starts,
            up,
            nested,
        }
    }

    fn holds(&self, index: usize, start: u32, end: u32) -> bool {
        let (from, to) = self.spans[index];
        from <= start && end <= to
    }

    fn innermost(&self, start: u32, end: u32, skip: Option<usize>) -> Option<usize> {
        if !self.nested {
            return (0..self.spans.len())
                .filter(|&index| skip != Some(index) && self.holds(index, start, end))
                .min_by_key(|&index| self.spans[index].1 - self.spans[index].0);
        }
        let last = self.starts.partition_point(|&from| from <= start);
        let mut at = last.checked_sub(1).map(|position| self.order[position]);
        while let Some(index) = at {
            if skip != Some(index) && self.holds(index, start, end) {
                return Some(index);
            }
            at = self.up[index];
        }
        None
    }
}

/// The file's scopes (index 0 is the file itself) and the nest that answers
/// which scope holds a span.
struct Scopes {
    scopes: Vec<Scope>,
    nest: Nest,
}

impl Scopes {
    fn containing(&self, start: u32, end: u32, skip: Option<usize>) -> usize {
        self.nest.innermost(start, end, skip).unwrap_or(ROOT)
    }
}

/// The first definition each (scope, name) pair owns, in definition order:
/// the lexical walk's per-scope question.
type Owned<'a> = HashMap<(usize, &'a str), usize>;

fn owned(definitions: &[Definition]) -> Owned<'_> {
    let mut owned = HashMap::new();
    for (index, def) in definitions.iter().enumerate() {
        owned.entry((def.owner, def.name.as_str())).or_insert(index);
    }
    owned
}

/// The scope tree, the definitions it owns, and the references it resolves,
/// projected onto the three pass-1 rows.
fn rows(path: &str, file_end: u32, captures: &[Capture<'_>]) -> Vec<FlatFact> {
    let scope_spans = labelled(captures, |label| label == "local.scope");
    let scopes = scope_tree(file_end, &scope_spans);
    let definitions = definitions(captures, &scopes);
    let owned = owned(&definitions);
    let by_start = starts_order(&definitions);
    let names = scope_names(&scopes.scopes, &definitions, &by_start);
    let exports = labelled(captures, |label| label == "local.export.package");
    let declaring = exports
        .iter()
        .any(|export| first_inside(&definitions, &by_start, export.start, export.end).is_none());
    let export_nest = Nest::new(
        exports
            .iter()
            .map(|export| (export.start, export.end))
            .collect(),
    );

    let mut facts = Vec::new();
    for def in &definitions {
        let symbol = symbol(path, &def.name);
        let exported = def.owner == ROOT
            && (declaring || export_nest.innermost(def.start, def.end, None).is_some());
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
                    owner => names
                        .get(&owner)
                        .cloned()
                        .unwrap_or_else(|| "<root>".into()),
                },
                name: def.name.clone(),
                path: path.to_string(),
                start: def.start,
                end: def.end,
            });
        }
    }

    let bound: HashSet<(u32, u32)> = definitions.iter().map(|def| (def.start, def.end)).collect();
    for call in captures
        .iter()
        .filter(|capture| capture.label == "local.call")
    {
        if bound.contains(&(call.start, call.end)) {
            continue;
        }
        let Some(target) = resolve(call, &scopes, &definitions, &owned) else {
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
    facts.extend(free_names(
        path,
        file_end,
        captures,
        &scopes,
        &definitions,
        &owned,
        &bound,
    ));
    facts
}

/// Every occurrence a top-level item needs from outside itself: an identifier
/// the file's scope tree answers at file level, or not at all.
fn free_names(
    path: &str,
    file_end: u32,
    captures: &[Capture<'_>],
    scopes: &Scopes,
    definitions: &[Definition],
    owned: &Owned<'_>,
    bound: &HashSet<(u32, u32)>,
) -> Vec<FlatFact> {
    let imports = Nest::new(
        captures
            .iter()
            .filter(|capture| capture.label == "local.import")
            .map(|capture| (capture.start, capture.end))
            .collect(),
    );
    let path_heads: HashSet<(u32, u32)> = captures
        .iter()
        .filter(|capture| capture.label == "local.reference.path")
        .map(|capture| (capture.start, capture.end))
        .collect();
    let mut facts = Vec::new();
    for name in captures
        .iter()
        .filter(|capture| capture.label == "local.reference")
    {
        if bound.contains(&(name.start, name.end)) {
            continue;
        }
        if imports.innermost(name.start, name.end, None).is_some() {
            continue;
        }
        let path_head = path_heads.contains(&(name.start, name.end));
        if !path_head
            && resolve(name, scopes, definitions, owned).is_some_and(|def| def.owner != ROOT)
        {
            continue;
        }
        let owner = top_level(
            &scopes.scopes,
            scopes.containing(name.start, name.end, None),
        );
        facts.push(FlatFact::FreeNameRow {
            path: path.to_string(),
            owner_start: match owner {
                ROOT => 0,
                index => scopes.scopes[index].start,
            },
            owner_end: match owner {
                ROOT => file_end,
                index => scopes.scopes[index].end,
            },
            name: name.text.to_string(),
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
    call: &Capture<'_>,
    scopes: &Scopes,
    definitions: &'a [Definition],
    owned: &Owned<'_>,
) -> Option<&'a Definition> {
    let mut scope = Some(scopes.containing(call.start, call.end, None));
    while let Some(index) = scope {
        if let Some(&found) = owned.get(&(index, call.text)) {
            return Some(&definitions[found]);
        }
        scope = scopes.scopes[index].parent;
    }
    None
}

fn symbol(path: &str, name: &str) -> String {
    format!("scm . . `{path}`/{name}().")
}

fn labelled<'a>(captures: &[Capture<'a>], accepts: impl Fn(&str) -> bool) -> Vec<Capture<'a>> {
    captures
        .iter()
        .filter(|capture| accepts(capture.label))
        .copied()
        .collect()
}

/// Index 0 is the file itself, so every span has an owner.
fn scope_tree(file_end: u32, spans: &[Capture<'_>]) -> Scopes {
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
    let nest = Nest::new(
        scopes
            .iter()
            .map(|scope| (scope.start, scope.end))
            .collect(),
    );
    for index in 1..scopes.len() {
        let (start, end) = (scopes[index].start, scopes[index].end);
        scopes[index].parent = Some(nest.innermost(start, end, Some(index)).unwrap_or(ROOT));
    }
    Scopes { scopes, nest }
}

/// A function or type belongs to the scope AROUND the one it opens: its own
/// body must not be where its name resolves.
fn definitions(captures: &[Capture<'_>], scopes: &Scopes) -> Vec<Definition> {
    let spans = Nest::new(
        captures
            .iter()
            .filter(|capture| capture.label == "local.def.span")
            .map(|capture| (capture.start, capture.end))
            .collect(),
    );
    let mut definitions = Vec::new();
    for capture in captures {
        let Some(kind) = capture.label.strip_prefix("local.definition.") else {
            continue;
        };
        let direct = scopes.containing(capture.start, capture.end, None);
        let owner = if kind == "function" || kind == "type" {
            let (start, end) = (scopes.scopes[direct].start, scopes.scopes[direct].end);
            scopes.containing(start, end, Some(direct))
        } else {
            direct
        };
        definitions.push(Definition {
            name: capture.text.to_string(),
            kind: kind.to_string(),
            start: capture.start,
            end: capture.end,
            owner,
            decl: spans
                .innermost(capture.start, capture.end, None)
                .map_or((capture.start, capture.end), |index| spans.spans[index]),
        });
    }
    definitions
}

/// Definition indices by (start, index): the order `min_by_key(start)` breaks
/// ties in.
fn starts_order(definitions: &[Definition]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..definitions.len()).collect();
    order.sort_by_key(|&index| (definitions[index].start, index));
    order
}

/// The earliest-starting definition inside `[start, end)`, ties to the lowest
/// index.
fn first_inside<'a>(
    definitions: &'a [Definition],
    by_start: &[usize],
    start: u32,
    end: u32,
) -> Option<&'a Definition> {
    let from = by_start.partition_point(|&index| definitions[index].start < start);
    by_start[from..]
        .iter()
        .map(|&index| &definitions[index])
        .take_while(|def| def.start <= end)
        .find(|def| def.end <= end)
}

/// A scope's name is the first definition inside it, which for a function or a
/// class scope is its own declared name.
fn scope_names(
    scopes: &[Scope],
    definitions: &[Definition],
    by_start: &[usize],
) -> BTreeMap<usize, String> {
    let mut names = BTreeMap::new();
    for (index, scope) in scopes.iter().enumerate() {
        if let Some(first) = first_inside(definitions, by_start, scope.start, scope.end) {
            names.insert(index, first.name.clone());
        }
    }
    names
}
