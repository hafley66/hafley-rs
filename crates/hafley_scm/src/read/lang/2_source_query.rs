//! One library entrypoint for source queries backed by tree-sitter.
//!
//! The query compiles and runs through the shared `hafley_scm` engine: native
//! tree-sitter text predicates (`eq?`, `not-eq?`, `match?`, `not-match?`,
//! `any-of?`, and their `any-`/`not-any-` forms) evaluate on the cursor, host
//! predicates (`has-ancestor?`, `has-parent?`, `has?`, `contains?`, and the
//! generic `not-` forms) evaluate in the arena fill, and unknown predicates
//! are build errors. Canonical source occurrence, match, and capture facts
//! belong to the later normalization boundary and are intentionally absent
//! here.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use tree_sitter::Parser as TreeParser;


/// A tree-sitter query keeps the native S-expression and explicit grammar name.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TreeSitterQuery {
    pub language: String,
    pub query: String,
}

/// The source query algebras currently hosted by sprefa-extract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "engine", content = "specification", rename_all = "snake_case")]
pub enum SourceQuery {
    TreeSitter(TreeSitterQuery),
}

/// The existing tree-sitter CLI row: capture names map to captured text, with
/// one-based `line` and `end_line` fields in the same top-level object.
pub type TreeSitterQueryMatch = BTreeMap<String, Value>;

/// One ordered tree-sitter capture with its exact half-open byte range.
/// The legacy query CLI projects these rows back into its name-to-text map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeSitterSpannedCapture {
    pub label: String,
    pub text: String,
    pub start: u32,
    pub end: u32,
}

/// One tree-sitter match before the legacy map projection discards byte spans
/// and repeated capture names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeSitterSpannedMatch {
    pub pattern: u32,
    pub start: u32,
    pub end: u32,
    pub line: u32,
    pub end_line: u32,
    pub captures: Vec<TreeSitterSpannedCapture>,
}

/// Results remain engine-shaped until the common match-fact schema is reviewed.
#[derive(Clone, Debug, PartialEq)]
pub enum SourceQueryOutput {
    TreeSitter(Vec<TreeSitterQueryMatch>),
}
#[derive(Debug)]
pub enum SourceQueryError {
    TreeSitter(String),
    Projection(String),
}

impl std::fmt::Display for SourceQueryError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TreeSitter(error) => formatter.write_str(error),
            Self::Projection(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for SourceQueryError {}

/// Dispatch one query against caller-owned source bytes.
pub fn query_source(
    content: &[u8],
    query: &SourceQuery,
) -> Result<SourceQueryOutput, SourceQueryError> {
    match query {
        SourceQuery::TreeSitter(query) => query_tree_sitter(content, query)
            .map(SourceQueryOutput::TreeSitter)
            .map_err(SourceQueryError::TreeSitter),
    }
}

pub fn query_tree_sitter(
    content: &[u8],
    request: &TreeSitterQuery,
) -> Result<Vec<TreeSitterQueryMatch>, String> {
    query_tree_sitter_spans(content, request).map(|matches| {
        matches
            .into_iter()
            .map(|found| {
                let mut row = BTreeMap::new();
                for capture in found.captures {
                    row.insert(capture.label, Value::String(capture.text));
                }
                row.insert("line".to_string(), Value::from(found.line));
                row.insert("end_line".to_string(), Value::from(found.end_line));
                row
            })
            .collect()
    })
}

/// Run a native tree-sitter query through the shared `hafley_scm` engine,
/// retaining the spans and ordering that the shared source-fact projection
/// needs. One `QueryExt` per request, one `MatchArena` per run.
pub fn query_tree_sitter_spans(
    content: &[u8],
    request: &TreeSitterQuery,
) -> Result<Vec<TreeSitterSpannedMatch>, String> {
    let language = query_language(&request.language)?;
    std::str::from_utf8(content)
        .map_err(|error| format!("query input is not valid UTF-8: {error}"))?;
    let mut parser = TreeParser::new();
    parser
        .set_language(&language)
        .map_err(|error| format!("invalid language '{}': {error:?}", request.language))?;
    let tree = parser
        .parse(content, None)
        .ok_or_else(|| "query parse failed: source tree was not produced".to_string())?;
    let query = hafley_scm::build(&language, &request.query)
        .map_err(|error| one_line_text(query_error_text(&error)))?;
    let mut arena = hafley_scm::MatchArena::default();
    // The fresh-cursor default this facade always ran with; the engine's
    // limit check cannot fire at u32::MAX.
    hafley_scm::run(&query, "", content, &tree, u32::MAX, &mut arena)
        .map_err(|error| one_line_text(query_error_text(&error)))?;
    project_match_arena(&query, &arena, content)
}

/// Every language the `Source` roster can parse, through the one name table
/// `RyiLang::parse_name` owns. A name this rejects reaches no grammar at all.
fn query_language(name: &str) -> Result<tree_sitter::Language, String> {
    crate::read::lang::extract_lang::RyiLang::parse_name(name)
        .map(|lang| lang.tree_sitter_language())
        .ok_or_else(|| format!("unknown lang '{name}'"))
}

/// Project one arena of native SCM matches into the legacy spanned rows.
/// The arena retains no tree nodes, only capture byte ranges and name
/// indices, so one-based lines come from one line-start table over the
/// source bytes instead of node positions.
fn project_match_arena(
    query: &hafley_scm::QueryExt,
    arena: &hafley_scm::MatchArena,
    content: &[u8],
) -> Result<Vec<TreeSitterSpannedMatch>, String> {
    let starts = line_starts(content);
    let line_of = |offset: u32| starts.partition_point(|&line_start| line_start <= offset) as u32;
    let mut rows = Vec::with_capacity(arena.rows.len());
    for row in &arena.rows {
        let spans = &arena.spans[row.spans.start as usize..row.spans.end as usize];
        if spans.is_empty() {
            // The legacy cursor loop dropped matches whose pattern captured
            // nothing; the arena keeps every engine match, so the projection
            // keeps dropping them.
            continue;
        }
        let mut captures = Vec::with_capacity(spans.len());
        let mut start = u32::MAX;
        let mut end = 0;
        for span in spans {
            let text = std::str::from_utf8(
                content
                    .get(span.bytes.start as usize..span.bytes.end as usize)
                    .ok_or("query capture range is out of bounds")?,
            )
            .map_err(|error| format!("query capture text: {error}"))?;
            captures.push(TreeSitterSpannedCapture {
                label: query.names[span.name as usize].to_string(),
                text: text.to_string(),
                start: span.bytes.start,
                end: span.bytes.end,
            });
            start = start.min(span.bytes.start);
            end = end.max(span.bytes.end);
        }
        rows.push(TreeSitterSpannedMatch {
            pattern: u32::from(row.pattern),
            start,
            end,
            line: line_of(start),
            end_line: line_of(end),
            captures,
        });
    }
    Ok(rows)
}

/// One line-start offset per line, built in one pass; `line_of` bisects it.
fn line_starts(content: &[u8]) -> Vec<u32> {
    let mut starts = vec![0u32];
    starts.extend(memchr::memchr_iter(b'\n', content).map(|newline| newline as u32 + 1));
    starts
}

/// One-line text for the engine's error shape, preserving the facade's
/// historical wording where one exists.
fn query_error_text(error: &hafley_scm::QueryExtError) -> String {
    match error {
        hafley_scm::QueryExtError::Parse(error) => {
            format!(
                "invalid query at row {}: {error}",
                error.row.saturating_add(1)
            )
        }
        hafley_scm::QueryExtError::UnknownOperator(operator) => {
            format!("invalid query: predicate #{operator} is not allowed")
        }
        hafley_scm::QueryExtError::Arity { operator, got } => {
            format!("invalid query: predicate #{operator} got {got} arguments")
        }
        hafley_scm::QueryExtError::DuplicateField(key) => {
            format!("invalid query: emission repeats field {key}")
        }
        hafley_scm::QueryExtError::MatchLimit { file } => {
            format!("query match limit exceeded on '{file}'")
        }
    }
}

fn one_line_text(text: String) -> String {
    text.lines()
        .next()
        .unwrap_or("invalid query command")
        .to_string()
}
