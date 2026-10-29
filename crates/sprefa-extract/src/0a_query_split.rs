//! `query --query` over the scm++ sibling split: a pattern whose node has 3+ unanchored
//! sibling children runs as parent edges plus a `sib` order, and prints `--query` rows.
use serde_json::{Map, Value};

/// The query compiles under scm++ for `grammar` and scm++ splits at least one level.
pub(crate) fn splits(query: &str, grammar: &tree_sitter::Language) -> bool {
    matches!(hafley_scm::scmpp::first_scmpp_only(query), Ok(None))
        && hafley_scm::scmpp::compile(grammar, query).is_ok_and(|compiled| {
            compiled.warnings.iter().any(|warning| warning.contains("split into"))
        })
}

/// One line-start offset per line; `line` bisects it the way `--query` rows do.
pub(crate) fn line_starts(content: &[u8]) -> Vec<u32> {
    std::iter::once(0)
        .chain(content.iter().enumerate().filter(|(_, byte)| **byte == b'\n').map(|(at, _)| at as u32 + 1))
        .collect()
}

fn line(starts: &[u32], offset: u32) -> u32 {
    starts.partition_point(|&start| start <= offset) as u32
}

/// An scm++ row (`NAME__start`, `NAME__end`, `NAME__text`) as a `--query` row: each capture's
/// text, `line` of the earliest capture start, `end_line` of the latest capture end.
pub(crate) fn plain_row(row: &Map<String, Value>, starts: &[u32]) -> std::collections::BTreeMap<String, Value> {
    let mut out = std::collections::BTreeMap::new();
    out.insert("path".into(), row["path"].clone());
    let (mut first, mut last) = (u32::MAX, 0);
    for (key, value) in row {
        if let Some(label) = key.strip_suffix("__text") {
            out.insert(label.into(), value.clone());
        }
        if let (Some(_), Some(at)) = (key.strip_suffix("__start"), value.as_u64()) {
            first = first.min(at as u32);
        }
        if let (Some(_), Some(at)) = (key.strip_suffix("__end"), value.as_u64()) {
            last = last.max(at as u32);
        }
    }
    out.insert("line".into(), line(starts, first).into());
    out.insert("end_line".into(), line(starts, last).into());
    out
}
