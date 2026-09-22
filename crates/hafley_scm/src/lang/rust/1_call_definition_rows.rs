//! The Rust CallF definition rows: the engine's grouped captures for the
//! crate-owned CallF query, projected onto plain rows a consumer can own.

use std::collections::BTreeMap;

use crate::{run, MatchArena, QueryExt};

/// What kind of callable a definition row claims.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallDefinitionKind {
    Free,
    Method,
    Lambda,
}

/// One callable definition: its byte range, its kind, and its name when the
/// query captured one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CallDefinitionRow {
    pub range: std::ops::Range<u32>,
    pub kind: CallDefinitionKind,
    pub name: Option<String>,
}

/// Runs the supplied query over the supplied tree and projects the rows.
pub fn call_definition_rows(
    query: &QueryExt,
    path: &str,
    src: &[u8],
    tree: &tree_sitter::Tree,
) -> Vec<CallDefinitionRow> {
    let mut arena = MatchArena::default();
    run(query, path, src, tree, u32::MAX, &mut arena).expect("rust call query runs");
    call_definition_rows_from_arena(query, &arena, src)
}

/// Projects an arena the caller already ran, deduped first claim by range.
pub fn call_definition_rows_from_arena(
    query: &QueryExt,
    arena: &MatchArena,
    src: &[u8],
) -> Vec<CallDefinitionRow> {
    let mut defs = BTreeMap::<(u32, u32), (CallDefinitionKind, Option<String>)>::new();
    for row in &arena.rows {
        let spans = &arena.spans[row.spans.start as usize..row.spans.end as usize];
        let capture = |label: &str| {
            spans
                .iter()
                .find(|span| query.names[span.name as usize].as_ref() == label)
                .map(|span| span.bytes.clone())
        };
        let name = capture("def.name").map(|range| {
            String::from_utf8_lossy(&src[range.start as usize..range.end as usize]).into_owned()
        });
        let Some((kind, range)) = (if let Some(lambda) = capture("def.lambda") {
            Some((CallDefinitionKind::Lambda, lambda))
        } else if let Some(variant) = capture("def.variant") {
            Some((CallDefinitionKind::Free, variant))
        } else if let (Some(name_range), Some(body)) = (capture("def.name"), capture("def.body")) {
            let kind = if capture("def.method").is_some() {
                CallDefinitionKind::Method
            } else {
                CallDefinitionKind::Free
            };
            Some((kind, name_range.start..body.end))
        } else if let (Some(name_range), Some(sig)) = (capture("def.name"), capture("def.sig")) {
            let mut end = sig.end.saturating_sub(1);
            while end > name_range.end && src[end as usize - 1].is_ascii_whitespace() {
                end -= 1;
            }
            Some((CallDefinitionKind::Method, name_range.start..end))
        } else {
            None
        }) else {
            continue;
        };
        defs.entry((range.start, range.end)).or_insert((kind, name));
    }
    defs.into_iter()
        .map(|((start, end), (kind, name))| CallDefinitionRow {
            range: start..end,
            kind,
            name,
        })
        .collect()
}
