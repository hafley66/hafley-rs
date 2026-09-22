use tree_sitter::QueryMatch;

use crate::types::{CapturedSpan, MatchArena, MatchRow};

/// One kept match: its spans in capture order, then the row that ranges over them.
pub fn append_match(found: &QueryMatch, file: u16, arena: &mut MatchArena) {
    let start = arena.spans.len() as u32;
    for capture in found.captures {
        arena.spans.push(CapturedSpan {
            name: capture.index as u16,
            bytes: capture.node.start_byte() as u32..capture.node.end_byte() as u32,
        });
    }
    arena.rows.push(MatchRow {
        file,
        pattern: found.pattern_index as u16,
        spans: start..arena.spans.len() as u32,
    });
}
