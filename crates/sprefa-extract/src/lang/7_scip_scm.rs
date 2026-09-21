//! The `scip_scm` family: pass 1 of the SCIP shape, from a per-language `.scm`
//! query lowered through L1 and executed natively, plus the file's scope tree.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ast_grep_language::{LanguageExt, SupportLang};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

use super::ast_rule::{query_ast_rule, AstRule, AstRuleRequest};
use super::extract_lang::RyiLang;
use super::scm_lower::{lower_scm, ScmLowerError};
use crate::types::FlatFact;

const KOTLIN_SCIP_SCM: &str = include_str!("../../queries/kotlin/scip.scm");

/// The outer captures L1 selects. Everything else is read off the native
/// match that carries one of them.
const SPAN_LABELS: [&str; 5] = [
    "local.scope",
    "local.def.span",
    "local.site.span",
    "local.import",
    "local.export.package",
];

const ROOT: usize = 0;

#[derive(Debug)]
pub enum ScipScmError {
    /// A supplied path could not be read.
    Io { path: String, detail: String },
    /// A language pass 1 does not cover. Kotlin and TypeScript are the roster.
    OutOfScope { path: String, lang: String },
    /// The bundled query did not lower through L1.
    Lower { path: String, error: ScmLowerError },
    /// L1 or the native engine refused the bundled query.
    Query { path: String, detail: String },
    /// tree-sitter stopped enumerating matches, so the captures are partial.
    MatchLimit { path: String },
}

impl std::fmt::Display for ScipScmError {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, detail } => write!(out, "{path}: {detail}"),
            Self::OutOfScope { path, lang } => write!(
                out,
                "{path}: --family scip_scm covers kotlin and typescript, not {lang}. \
                 Pass 1 has no compiler leg, no indexer leg, no cross-repo symbol \
                 and no persistent index"
            ),
            Self::Lower { path, error } => write!(out, "{path}: ScmLowerError: {error}"),
            Self::Query { path, detail } => write!(out, "{path}: {detail}"),
            Self::MatchLimit { path } => write!(
                out,
                "{path}: the scip_scm query exceeded the tree-sitter match limit"
            ),
        }
    }
}

impl std::error::Error for ScipScmError {}

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
}

/// Every pass-1 row for the supplied files, in emission order. One file's rows
/// are a function of that file alone.
pub fn scip_scm_facts(paths: &[PathBuf]) -> Result<Vec<FlatFact>, ScipScmError> {
    let mut facts = Vec::new();
    for path in expand(paths)? {
        facts.extend(file_facts(&path)?);
    }
    Ok(facts)
}

/// A named file is taken as given; a directory contributes every file under it
/// whose language pass 1 covers.
fn expand(paths: &[PathBuf]) -> Result<Vec<PathBuf>, ScipScmError> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut covered = Vec::new();
            walk(path, &mut covered)?;
            covered.sort();
            files.extend(covered);
        } else {
            files.push(path.clone());
        }
    }
    Ok(files)
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), ScipScmError> {
    let entries = std::fs::read_dir(dir).map_err(|error| ScipScmError::Io {
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
fn query_for(path: &str) -> Option<(SupportLang, &'static str)> {
    match RyiLang::from_path(path) {
        Some(RyiLang::Sg(SupportLang::Kotlin)) => Some((SupportLang::Kotlin, KOTLIN_SCIP_SCM)),
        _ => None,
    }
}

fn file_facts(path: &PathBuf) -> Result<Vec<FlatFact>, ScipScmError> {
    let name = path.to_string_lossy().to_string();
    let Some((lang, query_text)) = query_for(&name) else {
        return Err(ScipScmError::OutOfScope {
            lang: RyiLang::from_path(&name)
                .map(|lang| lang.name().to_string())
                .unwrap_or_else(|| "an unrostered language".into()),
            path: name,
        });
    };
    let source = std::fs::read(path).map_err(|error| ScipScmError::Io {
        path: name.clone(),
        detail: error.to_string(),
    })?;
    let selected = lowered_spans(&name, &source, query_text)?;
    let captured = native_captures(&name, lang, query_text, &source, &selected)?;
    Ok(rows(&name, source.len() as u32, captured))
}

/// L1 supplies the candidate spans. Native execution retains the capture
/// grouping the AstRule representation does not store.
fn lowered_spans(
    path: &str,
    source: &[u8],
    query_text: &str,
) -> Result<BTreeSet<(u32, u32)>, ScipScmError> {
    let program = lower_scm(query_text).map_err(|error| ScipScmError::Lower {
        path: path.to_string(),
        error,
    })?;
    let rule = AstRule::Any(
        SPAN_LABELS
            .into_iter()
            .map(|name| AstRule::Matches(name.to_string()))
            .collect(),
    );
    let request = AstRuleRequest {
        id: "scip-scm".into(),
        rule,
        utils: program.utils,
        constraints: program.constraints,
        fix: None,
    };
    let matches = query_ast_rule(path, source, &request).map_err(|error| ScipScmError::Query {
        path: path.to_string(),
        detail: error.to_string(),
    })?;
    Ok(matches
        .into_iter()
        .map(|row| (row.span.start, row.span.end()))
        .collect())
}

/// One native run, grouped: a capture is kept when the match's own span
/// capture is one L1 selected.
fn native_captures(
    path: &str,
    lang: SupportLang,
    query_text: &str,
    source: &[u8],
    selected: &BTreeSet<(u32, u32)>,
) -> Result<BTreeSet<Capture>, ScipScmError> {
    let language = lang.get_ts_language();
    let mut parser = Parser::new();
    parser
        .set_language(&language)
        .map_err(|error| ScipScmError::Query {
            path: path.to_string(),
            detail: format!("set language: {error}"),
        })?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ScipScmError::Query {
            path: path.to_string(),
            detail: "parse returned no tree".into(),
        })?;
    let query = Query::new(&language, query_text).map_err(|error| ScipScmError::Query {
        path: path.to_string(),
        detail: format!("query row {}: {error}", error.row + 1),
    })?;
    let names = query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source);
    let mut kept = BTreeSet::new();
    while let Some(found) = matches.next() {
        let mut captures = Vec::new();
        let mut span = None;
        for capture in found.captures {
            let node = capture.node;
            let label = names[capture.index as usize];
            let taken = Capture {
                label: label.to_string(),
                text: node.utf8_text(source).unwrap_or("").to_string(),
                start: node.start_byte() as u32,
                end: node.end_byte() as u32,
            };
            if SPAN_LABELS.contains(&label) {
                span = Some((taken.start, taken.end));
            }
            captures.push(taken);
        }
        if span.is_some_and(|span| selected.contains(&span)) {
            kept.extend(captures);
        }
    }
    drop(matches);
    if cursor.did_exceed_match_limit() {
        return Err(ScipScmError::MatchLimit {
            path: path.to_string(),
        });
    }
    Ok(kept)
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
        facts.push(FlatFact::ScipScmSymbolRow {
            symbol: symbol.clone(),
            path: path.to_string(),
            kind: def.kind.clone(),
        });
        facts.push(FlatFact::ScipScmOccurrenceRow {
            symbol,
            path: path.to_string(),
            start: def.start,
            end: def.end,
            role: "def".into(),
        });
        let exported = def.owner == ROOT
            && (declaring
                || exports
                    .iter()
                    .any(|export| export.start <= def.start && def.end <= export.end));
        if !exported {
            facts.push(FlatFact::ScipScmLocalRow {
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
        facts.push(FlatFact::ScipScmOccurrenceRow {
            symbol: symbol(path, &target.name),
            path: path.to_string(),
            start: call.start,
            end: call.end,
            role: "ref".into(),
        });
    }
    facts
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
