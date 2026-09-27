//! In-process `macro_rules!` expansion (`plans/extract-macro-lab-2026-08-29/PLAN.md`
//! Option 1): splice a local invocation's expansion into the file's own text.

use ra_ap_mbe::DeclarativeMacro;
use ra_ap_parser::Edition;
use ra_ap_span::{Span as RaSpan, SpanAnchor, SyntaxContext, ROOT_ERASED_FILE_AST_ID};
use ra_ap_syntax::{
    ast, ast::HasName, AstNode, Parse, SourceFile, SyntaxNode, TextRange, TextSize,
};
use ra_ap_syntax_bridge::{
    syntax_node_to_token_tree, token_tree_to_syntax_node, DocCommentDesugarMode, SpanMapper,
};
use ra_ap_tt::TopSubtree;
#[cfg(feature = "read")]
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

// A macro that keeps minting more of itself, never a budget to raise.
const MAX_PASSES: u32 = 8;
const MAX_GROWTH_FACTOR: usize = 4;

/// `Verbatim` bytes translate 1:1 to the original file. `Macro` bytes were
/// minted by one invocation; the whole run collapses to that invocation's span.
enum Chunk {
    Verbatim {
        start: u32,
        end: u32,
        orig_start: u32,
    },
    Macro {
        start: u32,
        end: u32,
        origin: Range<u32>,
        name: String,
    },
}

impl Chunk {
    fn start(&self) -> u32 {
        match self {
            Chunk::Verbatim { start, .. } | Chunk::Macro { start, .. } => *start,
        }
    }
    fn end(&self) -> u32 {
        match self {
            Chunk::Verbatim { end, .. } | Chunk::Macro { end, .. } => *end,
        }
    }
}

/// The spliced file plus enough of a map to report a gained def/site's span
/// as the original invocation's span (`map_span`).
pub struct Expanded {
    pub text: String,
    chunks: Vec<Chunk>,
    // A pass wanted to expand past MAX_PASSES or MAX_GROWTH_FACTORx bytes.
    pub budget_hit: bool,
}

impl Expanded {
    /// Translate a byte range of `self.text` to the original file: exact for a
    /// `Verbatim` range, the whole invocation span for a `Macro` range.
    pub fn map_span(&self, spliced: Range<u32>) -> Option<Range<u32>> {
        let idx = self
            .chunks
            .binary_search_by(|c| {
                if spliced.start < c.start() {
                    std::cmp::Ordering::Greater
                } else if spliced.start >= c.end() {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()?;
        let chunk = &self.chunks[idx];
        if spliced.end > chunk.end() {
            return None;
        }
        match chunk {
            Chunk::Verbatim {
                start, orig_start, ..
            } => Some(orig_start + (spliced.start - start)..orig_start + (spliced.end - start)),
            Chunk::Macro { origin, .. } => Some(origin.clone()),
        }
    }

    /// `true` for a spliced range born inside a macro expansion.
    pub fn is_macro_span(&self, spliced: Range<u32>) -> bool {
        self.chunks.iter().any(|c| {
            matches!(c, Chunk::Macro { .. }) && c.start() <= spliced.start && spliced.end <= c.end()
        })
    }

    /// One row per distinct invocation, deduped across nested chunks that
    /// share one origin (f3: `outer!`/`inner!` collapse to one row).
    pub fn macro_sites(&self) -> Vec<(Range<u32>, &str)> {
        let mut seen = Vec::new();
        for c in &self.chunks {
            let Chunk::Macro { origin, name, .. } = c else {
                continue;
            };
            if !seen.iter().any(|(s, _): &(Range<u32>, &str)| *s == *origin) {
                seen.push((origin.clone(), name.as_str()));
            }
        }
        seen
    }
}

struct FileSpanMap;

impl SpanMapper for FileSpanMap {
    fn span_for(&self, range: TextRange) -> RaSpan {
        ra_span_at(range)
    }
}

fn ra_span_at(range: TextRange) -> RaSpan {
    RaSpan {
        range,
        anchor: SpanAnchor {
            file_id: ra_ap_span::EditionedFileId::new(
                ra_ap_span::FileId::from_raw(0),
                Edition::CURRENT,
            ),
            ast_id: ROOT_ERASED_FILE_AST_ID,
        },
        ctx: SyntaxContext::root(Edition::CURRENT),
    }
}

#[salsa::db]
#[derive(Default)]
struct Db {
    storage: salsa::Storage<Self>,
}

#[salsa::db]
impl salsa::Database for Db {}

fn edition(_ctx: SyntaxContext) -> Edition {
    Edition::CURRENT
}

fn collect_rules(node: &SyntaxNode, defs: &mut HashMap<String, (TopSubtree, String)>) {
    for ev in node.preorder() {
        let ra_ap_syntax::WalkEvent::Enter(n) = ev else {
            continue;
        };
        let Some(mr) = ast::MacroRules::cast(n) else {
            continue;
        };
        let (Some(name), Some(tt)) = (mr.name().map(|n| n.text().to_string()), mr.token_tree())
        else {
            continue;
        };
        let top = syntax_node_to_token_tree(
            tt.syntax(),
            FileSpanMap,
            ra_span_at(tt.syntax().text_range()),
            DocCommentDesugarMode::Mbe,
        );
        defs.insert(name, (top, mr.syntax().text().to_string()));
    }
}

/// One `name!(...)` invocation as found in the current pass's text.
struct Invocation {
    name: String,
    call_tt: TopSubtree,
    range: TextRange,
}

enum ExpansionAttempt {
    Failed { memoize_shape: bool },
    Expanded(String),
}

fn expand_invocation(definition: &DeclarativeMacro, invocation: &Invocation) -> ExpansionAttempt {
    let db = Db::default();
    if definition.err().is_some() {
        return ExpansionAttempt::Failed {
            memoize_shape: false,
        };
    }
    let result = definition.expand(
        &db,
        &invocation.call_tt,
        |_span| {},
        ra_ap_mbe::MacroCallStyle::FnLike,
        ra_span_at(invocation.range),
    );
    if result.err.is_some() {
        return ExpansionAttempt::Failed {
            memoize_shape: result.err.as_ref().is_some_and(|error| {
                !matches!(error.inner.1, ra_ap_mbe::ExpandErrorKind::LimitExceeded)
            }),
        };
    }
    let (top, _) = result.value;
    let (parsed, _) =
        token_tree_to_syntax_node(&top, ra_ap_parser::TopEntryPoint::MacroItems, &mut edition);
    ExpansionAttempt::Expanded(spaced_text(parsed.syntax_node()))
}

/// Normalize capture values while preserving delimiters, punctuation, keywords,
/// and every literal identifier or literal token from the macro definition.
/// This lets a structural matcher failure skip equivalent generated calls.
fn invocation_shape(definition: &str, invocation: &str) -> String {
    use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};

    fn matcher_literals(tokens: TokenStream, literals: &mut HashSet<String>) {
        let mut previous_dollar = false;
        let mut previous_colon = false;
        for token in tokens {
            match &token {
                TokenTree::Ident(ident) => {
                    let value = ident.to_string();
                    let fragment = matches!(
                        value.as_str(),
                        "block"
                            | "expr"
                            | "ident"
                            | "item"
                            | "lifetime"
                            | "literal"
                            | "meta"
                            | "pat"
                            | "path"
                            | "stmt"
                            | "tt"
                            | "ty"
                            | "vis"
                    );
                    if !previous_dollar && !(previous_colon && fragment) {
                        literals.insert(value);
                    }
                    previous_dollar = false;
                    previous_colon = false;
                }
                TokenTree::Literal(literal) => {
                    literals.insert(literal.to_string());
                    previous_dollar = false;
                    previous_colon = false;
                }
                TokenTree::Group(group) => {
                    matcher_literals(group.stream(), literals);
                    previous_dollar = false;
                    previous_colon = false;
                }
                TokenTree::Punct(punct) => {
                    previous_dollar = punct.as_char() == '$';
                    previous_colon = punct.as_char() == ':';
                }
            }
        }
    }

    fn append_shape(tokens: TokenStream, literals: &HashSet<String>, out: &mut String) {
        for token in tokens {
            match token {
                TokenTree::Group(group) => {
                    out.push(match group.delimiter() {
                        Delimiter::Parenthesis => '(',
                        Delimiter::Brace => '{',
                        Delimiter::Bracket => '[',
                        Delimiter::None => '<',
                    });
                    append_shape(group.stream(), literals, out);
                    out.push(match group.delimiter() {
                        Delimiter::Parenthesis => ')',
                        Delimiter::Brace => '}',
                        Delimiter::Bracket => ']',
                        Delimiter::None => '>',
                    });
                }
                TokenTree::Ident(ident) => {
                    let value = ident.to_string();
                    if literals.contains(&value)
                        || matches!(
                            value.as_str(),
                            "Self"
                                | "as"
                                | "async"
                                | "await"
                                | "break"
                                | "const"
                                | "continue"
                                | "crate"
                                | "dyn"
                                | "else"
                                | "enum"
                                | "extern"
                                | "false"
                                | "fn"
                                | "for"
                                | "if"
                                | "impl"
                                | "in"
                                | "let"
                                | "loop"
                                | "match"
                                | "mod"
                                | "move"
                                | "mut"
                                | "pub"
                                | "ref"
                                | "return"
                                | "self"
                                | "static"
                                | "struct"
                                | "trait"
                                | "true"
                                | "type"
                                | "unsafe"
                                | "use"
                                | "where"
                                | "while"
                                | "yield"
                        )
                    {
                        out.push_str(&value);
                    } else {
                        out.push_str("<ident>");
                    }
                    out.push(';');
                }
                TokenTree::Literal(literal) => {
                    let value = literal.to_string();
                    if literals.contains(&value) {
                        out.push_str(&value);
                    } else {
                        out.push_str("<literal>");
                    }
                    out.push(';');
                }
                TokenTree::Punct(punct) => {
                    out.push(punct.as_char());
                    out.push(if punct.spacing() == Spacing::Joint {
                        '+'
                    } else {
                        ' '
                    });
                }
            }
        }
    }

    let Ok(tokens) = definition.parse::<TokenStream>() else {
        return invocation.to_owned();
    };
    let body = tokens.into_iter().find_map(|token| match token {
        TokenTree::Group(group) => Some(group.stream()),
        _ => None,
    });
    let Some(body) = body else {
        return invocation.to_owned();
    };
    let body = body.into_iter().collect::<Vec<_>>();
    let mut literals = HashSet::new();
    for rule in body.windows(3) {
        let [TokenTree::Group(matcher), TokenTree::Punct(eq), TokenTree::Punct(gt)] = rule else {
            continue;
        };
        if eq.as_char() == '=' && gt.as_char() == '>' {
            matcher_literals(matcher.stream(), &mut literals);
        }
    }
    let Ok(tokens) = invocation.parse::<TokenStream>() else {
        return invocation.to_owned();
    };
    let mut out = String::new();
    append_shape(tokens, &literals, &mut out);
    out
}

#[derive(Clone)]
struct PendingExpansion<'a> {
    key: (String, String),
    invocation: &'a Invocation,
    definition: &'a DeclarativeMacro,
}

fn collect_calls(
    node: &SyntaxNode,
    defs: &HashMap<String, (TopSubtree, String)>,
    scope: Option<&[Range<u32>]>,
    out: &mut Vec<Invocation>,
) {
    let node_range = node.text_range();
    if scope.is_some_and(|ranges| {
        !ranges.iter().any(|range| {
            range.start < u32::from(node_range.end()) && u32::from(node_range.start()) < range.end
        })
    }) {
        return;
    }
    if let Some(mc) = ast::MacroCall::cast(node.clone()) {
        let name = mc
            .path()
            .and_then(|p| p.segment())
            .and_then(|s| s.name_ref())
            .map(|n| n.text().to_string());
        if let Some(name) = name.filter(|name| defs.contains_key(name)) {
            if let Some(tt) = mc.token_tree() {
                let call_range = mc.syntax().text_range();
                let call_tt = syntax_node_to_token_tree(
                    tt.syntax(),
                    FileSpanMap,
                    ra_span_at(call_range),
                    DocCommentDesugarMode::Mbe,
                );
                out.push(Invocation {
                    name,
                    call_tt,
                    range: call_range,
                });
            }
        }
    }
    for child in node.children() {
        collect_calls(&child, defs, scope, out);
    }
}

fn spaced_text(node: SyntaxNode) -> String {
    node.preorder_with_tokens()
        .filter_map(|ev| match ev {
            ra_ap_syntax::WalkEvent::Enter(e) => e.into_token(),
            _ => None,
        })
        .map(|t| t.text().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One pass over `text`: expand every LOCAL `macro_rules!` invocation found
/// there. A name with no local def (cross-file, builtin, derive) is untouched.
/// `failed` holds the (definition, call) texts whose expansion already erred:
/// an unexpanded call survives into every later pass, and matching it again
/// is deterministic and can cost hundreds of milliseconds.
fn expand_pass(
    parsed: &Parse<SourceFile>,
    text: &str,
    scope: Option<&[Range<u32>]>,
    failed: &mut HashSet<(String, String)>,
    failed_shapes: &mut HashSet<(String, String)>,
    expanded: &mut HashMap<(String, String), String>,
    parsed_defs: &mut HashMap<String, DeclarativeMacro>,
) -> Vec<(Range<u32>, String, String)> {
    let root = parsed.syntax_node();
    let mut defs = HashMap::new();
    collect_rules(&root, &mut defs);
    for (definition, definition_text) in defs.values() {
        parsed_defs
            .entry(definition_text.clone())
            .or_insert_with(|| DeclarativeMacro::parse_macro_rules(definition, edition));
    }
    let mut calls = Vec::new();
    collect_calls(&root, &defs, scope, &mut calls);

    let mut edits = Vec::new();
    let mut groups: Vec<((String, String), Vec<PendingExpansion<'_>>)> = Vec::new();
    let mut group_indexes = HashMap::new();
    for inv in &calls {
        let Some((_, def_text)) = defs.get(&inv.name) else {
            continue;
        };
        let Some(parsed_def) = parsed_defs.get(def_text) else {
            continue;
        };
        let call_text =
            &text[u32::from(inv.range.start()) as usize..u32::from(inv.range.end()) as usize];
        let key = (def_text.clone(), call_text.to_string());
        if let Some(replacement) = expanded.get(&key) {
            edits.push((
                Range {
                    start: u32::from(inv.range.start()),
                    end: u32::from(inv.range.end()),
                },
                replacement.clone(),
                inv.name.clone(),
            ));
            continue;
        }
        if failed.contains(&key) {
            continue;
        }
        let shape = invocation_shape(def_text, call_text);
        let shape_key = (def_text.clone(), shape);
        if failed_shapes.contains(&shape_key) {
            failed.insert(key);
            continue;
        }
        let group_index = *group_indexes.entry(shape_key.clone()).or_insert_with(|| {
            groups.push((shape_key, Vec::new()));
            groups.len() - 1
        });
        groups[group_index].1.push(PendingExpansion {
            key,
            invocation: inv,
            definition: parsed_def,
        });
    }
    #[cfg(feature = "read")]
    let representatives = groups
        .par_iter()
        .map(|(_, group)| expand_invocation(group[0].definition, group[0].invocation))
        .collect::<Vec<_>>();
    #[cfg(not(feature = "read"))]
    let representatives = groups
        .iter()
        .map(|(_, group)| expand_invocation(group[0].definition, group[0].invocation))
        .collect::<Vec<_>>();
    let mut remaining = Vec::new();
    for ((shape_key, group), attempt) in groups.into_iter().zip(representatives) {
        let mut members = group.into_iter();
        let first = members
            .next()
            .expect("each expansion group has a representative");
        match attempt {
            ExpansionAttempt::Failed {
                memoize_shape: true,
            } => {
                failed_shapes.insert(shape_key);
                failed.insert(first.key);
                failed.extend(members.map(|member| member.key));
            }
            attempt => {
                record_attempt(first, attempt, failed, expanded, &mut edits);
                remaining.extend(members);
            }
        }
    }
    #[cfg(feature = "read")]
    let attempts = remaining
        .par_iter()
        .map(|member| {
            (
                member.clone(),
                expand_invocation(member.definition, member.invocation),
            )
        })
        .collect::<Vec<_>>();
    #[cfg(not(feature = "read"))]
    let attempts = remaining
        .iter()
        .map(|member| {
            (
                member.clone(),
                expand_invocation(member.definition, member.invocation),
            )
        })
        .collect::<Vec<_>>();
    for (pending, attempt) in attempts {
        record_attempt(pending, attempt, failed, expanded, &mut edits);
    }
    edits
}

fn record_attempt(
    pending: PendingExpansion<'_>,
    attempt: ExpansionAttempt,
    failed: &mut HashSet<(String, String)>,
    expanded: &mut HashMap<(String, String), String>,
    edits: &mut Vec<(Range<u32>, String, String)>,
) {
    match attempt {
        ExpansionAttempt::Failed { .. } => {
            failed.insert(pending.key);
        }
        ExpansionAttempt::Expanded(replacement) => {
            expanded.insert(pending.key, replacement.clone());
            edits.push((
                Range {
                    start: u32::from(pending.invocation.range.start()),
                    end: u32::from(pending.invocation.range.end()),
                },
                replacement,
                pending.invocation.name.clone(),
            ));
        }
    }
}

/// Reparse the combined changed range once per pass. rust-analyzer reuses
/// unaffected syntax subtrees and reparses only the smallest enclosing region.
fn reparse_pass(parsed: &Parse<SourceFile>, old_text: &str, new_text: &str) -> Parse<SourceFile> {
    let old = old_text.as_bytes();
    let new = new_text.as_bytes();
    let mut prefix = old
        .iter()
        .zip(new)
        .take_while(|(left, right)| left == right)
        .count();
    while !old_text.is_char_boundary(prefix) || !new_text.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let mut suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();
    while !old_text.is_char_boundary(old.len() - suffix)
        || !new_text.is_char_boundary(new.len() - suffix)
    {
        suffix -= 1;
    }
    parsed.clone().reparse(
        TextRange::new(
            TextSize::from(prefix as u32),
            TextSize::from((old.len() - suffix) as u32),
        ),
        &new_text[prefix..new.len() - suffix],
        Edition::CURRENT,
    )
}

/// Applies one pass's edits (byte ranges into `old_text`/`old_chunks`) to
/// build the next text and chunk generation.
fn apply_pass(
    old_text: &str,
    old_chunks: &[Chunk],
    mut edits: Vec<(Range<u32>, String, String)>,
) -> (String, Vec<Chunk>, Vec<Range<u32>>) {
    edits.sort_by_key(|(r, _, _)| r.start);
    let mut new_text = String::with_capacity(old_text.len());
    let mut new_chunks = Vec::new();
    let mut changed_ranges = Vec::new();
    let mut cursor: u32 = 0;

    let push_verbatim_range =
        |from: u32, to: u32, new_text: &mut String, new_chunks: &mut Vec<Chunk>| {
            if from >= to {
                return;
            }
            new_text.push_str(&old_text[from as usize..to as usize]);
            for c in old_chunks {
                let lo = c.start().max(from);
                let hi = c.end().min(to);
                if lo >= hi {
                    continue;
                }
                let shift = new_text.len() as u32 - (to - from);
                let new_start = shift + (lo - from);
                let new_end = shift + (hi - from);
                match c {
                    Chunk::Verbatim {
                        orig_start, start, ..
                    } => new_chunks.push(Chunk::Verbatim {
                        start: new_start,
                        end: new_end,
                        orig_start: orig_start + (lo - start),
                    }),
                    Chunk::Macro { origin, name, .. } => new_chunks.push(Chunk::Macro {
                        start: new_start,
                        end: new_end,
                        origin: origin.clone(),
                        name: name.clone(),
                    }),
                }
            }
        };

    for (range, replacement, name) in &edits {
        push_verbatim_range(cursor, range.start, &mut new_text, &mut new_chunks);
        let (origin, name) = invocation_origin(old_chunks, range.clone(), name);
        let start = new_text.len() as u32;
        new_text.push_str(replacement);
        let end = new_text.len() as u32;
        new_chunks.push(Chunk::Macro {
            start,
            end,
            origin,
            name,
        });
        if start < end {
            changed_ranges.push(start..end);
        }
        cursor = range.end;
    }
    push_verbatim_range(
        cursor,
        old_text.len() as u32,
        &mut new_text,
        &mut new_chunks,
    );
    (new_text, new_chunks, changed_ranges)
}

/// The (span, name) an invocation at `range` reports: its own verbatim
/// position and its own name, or whatever the enclosing macro chunk carries.
fn invocation_origin(
    old_chunks: &[Chunk],
    range: Range<u32>,
    own_name: &str,
) -> (Range<u32>, String) {
    for c in old_chunks {
        if c.start() <= range.start && range.end <= c.end() {
            return match c {
                Chunk::Verbatim {
                    start, orig_start, ..
                } => (
                    orig_start + (range.start - start)..orig_start + (range.end - start),
                    own_name.to_string(),
                ),
                Chunk::Macro { origin, name, .. } => (origin.clone(), name.clone()),
            };
        }
    }
    (range, own_name.to_string())
}

/// Expand every LOCAL `macro_rules!` invocation in `content` to a fixpoint.
/// `None` when there is nothing local to expand; `budget_hit` marks a cap stop.
pub fn expand_file(content: &str) -> Option<Expanded> {
    // A file with no `macro_rules!` text has nothing this module can splice;
    // skip the ra_ap_syntax parse rather than pay its RSS on every call file.
    if !content.contains("macro_rules!") {
        return None;
    }
    let parsed = SourceFile::parse(content, Edition::CURRENT);
    let mut failed = HashSet::new();
    let mut failed_shapes = HashSet::new();
    let mut expanded = HashMap::new();
    let mut parsed_defs = HashMap::new();
    let first_pass = expand_pass(
        &parsed,
        content,
        None,
        &mut failed,
        &mut failed_shapes,
        &mut expanded,
        &mut parsed_defs,
    );
    if first_pass.is_empty() {
        return None;
    }

    let mut chunks = vec![Chunk::Verbatim {
        start: 0,
        end: content.len() as u32,
        orig_start: 0,
    }];
    let (mut text, new_chunks, mut changed_ranges) = apply_pass(content, &chunks, first_pass);
    let mut parsed = reparse_pass(&parsed, content, &text);
    chunks = new_chunks;
    let mut budget_hit = false;

    for _pass in 1..MAX_PASSES {
        if text.len() > content.len() * MAX_GROWTH_FACTOR {
            budget_hit = true;
            break;
        }
        let edits = expand_pass(
            &parsed,
            &text,
            Some(&changed_ranges),
            &mut failed,
            &mut failed_shapes,
            &mut expanded,
            &mut parsed_defs,
        );
        if edits.is_empty() {
            break;
        }
        let (next_text, next_chunks, next_changed_ranges) = apply_pass(&text, &chunks, edits);
        if next_text.len() > content.len() * MAX_GROWTH_FACTOR {
            budget_hit = true;
            break;
        }
        let next_parsed = reparse_pass(&parsed, &text, &next_text);
        text = next_text;
        chunks = next_chunks;
        changed_ranges = next_changed_ranges;
        parsed = next_parsed;
    }
    if !budget_hit
        && !expand_pass(
            &parsed,
            &text,
            Some(&changed_ranges),
            &mut failed,
            &mut failed_shapes,
            &mut expanded,
            &mut parsed_defs,
        )
        .is_empty()
    {
        budget_hit = true;
    }

    Some(Expanded {
        text,
        chunks,
        budget_hit,
    })
}
