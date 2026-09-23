use tree_sitter::QueryMatch;

use crate::types::{CapturedSpan, EmittedCallSite, MatchArena, MatchRow, QueryExt};

/// One kept match: its spans in capture order, then the row that ranges over them.
pub fn append_match(q: &QueryExt, found: &QueryMatch, file: u16, arena: &mut MatchArena) {
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
    let first_emit = q.call_site_emits.partition_point(|emit| (emit.pattern as usize) < found.pattern_index);
    for emit in q.call_site_emits[first_emit..]
        .iter()
        .take_while(|emit| emit.pattern as usize == found.pattern_index)
    {
        let capture = |name| {
            found.captures.iter().find(|capture| capture.index as u16 == name)
                .map(|capture| capture.node.start_byte() as u32..capture.node.end_byte() as u32)
        };
        let (Some(group), Some(span)) = (capture(emit.group), capture(emit.span)) else {
            continue;
        };
        let callee_bytes = emit.callee_capture.and_then(capture);
        if emit.callee_capture.is_some() && callee_bytes.is_none() {
            continue;
        }
        arena.call_sites.push(EmittedCallSite {
            file,
            group,
            span,
            callee_bytes,
            callee_literal: emit.callee_literal,
        });
    }
}
