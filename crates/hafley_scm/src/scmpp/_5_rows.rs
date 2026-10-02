//! The rows the lowered SQL reads, per file: `capture` rows of every flat pattern and CST `edge` rows.
use tree_sitter::{QueryCursor, StreamingIterator, Tree};

use super::_0_types::{Compiled, ScmppError};
use super::_2_compile::ROOT;

pub struct CaptureRow<'q> {
    pub pattern: u16,
    /// Per-file ordinal across every flat pattern.
    pub r#match: u32,
    pub capture: &'q str,
    /// Empty for the `@__root` capture.
    pub text: String,
    pub start: u32,
    pub end: u32,
    pub match_start: u32,
    pub match_end: u32,
    pub kind: &'q str,
}

pub fn capture_rows<'q, E: From<ScmppError>>(
    compiled: &'q Compiled,
    tree: &'q Tree,
    src: &[u8],
    mut row: impl FnMut(CaptureRow<'q>) -> Result<(), E>,
) -> Result<(), E> {
    let mut ordinal = 0u32;
    for pattern in &compiled.patterns {
        let names = pattern.query.capture_names();
        let root = names
            .iter()
            .position(|name| *name == ROOT)
            .expect("flat pattern root") as u32;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&pattern.query, tree.root_node(), src);
        while let Some(found) = matches.next() {
            let Some(anchor) = found
                .captures()
                .iter()
                .find(|capture| capture.index == root)
            else {
                continue;
            };
            for capture in found.captures() {
                let node = capture.node;
                let text = if capture.index == root {
                    String::new()
                } else {
                    String::from_utf8_lossy(&src[node.byte_range()]).into_owned()
                };
                row(CaptureRow {
                    pattern: pattern.id,
                    r#match: ordinal,
                    capture: names[capture.index as usize],
                    text,
                    start: node.start_byte() as u32,
                    end: node.end_byte() as u32,
                    match_start: anchor.node.start_byte() as u32,
                    match_end: anchor.node.end_byte() as u32,
                    kind: node.kind(),
                })?;
            }
            ordinal += 1;
        }
        drop(matches);
        if cursor.did_exceed_match_limit() {
            return Err(ScmppError::MatchLimit {
                pattern: pattern.id,
            }
            .into());
        }
    }
    Ok(())
}

pub struct CstRow<'a> {
    pub start: u32,
    pub end: u32,
    pub kind: &'a str,
    pub named: bool,
    /// `(start, end, kind)` of the parent; `None` at the root, which has no edge.
    pub parent: Option<(u32, u32, &'a str)>,
    pub field: Option<&'a str>,
    pub index: u32,
    pub named_index: Option<u32>,
}

/// Every node, named and anonymous, in preorder; the first error stops the writes.
pub fn cst_rows<E>(tree: &Tree, mut row: impl FnMut(CstRow<'_>) -> Result<(), E>) -> Result<(), E> {
    let language = tree.language();
    let fields: Vec<Option<&str>> = (0..=language.field_count() as u16)
        .map(|id| language.field_name_for_id(id))
        .collect();
    let mut failed = None;
    crate::cst::walk_streaming(tree, |node, parent: Option<(u32, u32, &str)>, slot| {
        let here = (
            node.start_byte() as u32,
            node.end_byte() as u32,
            node.kind(),
        );
        if failed.is_none() {
            failed = row(CstRow {
                start: here.0,
                end: here.1,
                kind: here.2,
                named: node.is_named(),
                parent,
                field: slot.field.and_then(|id| fields[id as usize]),
                index: slot.index,
                named_index: slot.named_index,
            })
            .err();
        }
        here
    });
    failed.map_or(Ok(()), Err)
}
