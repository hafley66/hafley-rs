use tree_sitter::QueryMatch;

use crate::types::{CapturedSpan, EmitSource, EmittedFact, EmittedField, EmittedValue, MatchArena, MatchRow, QueryExt};

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
    let first_emit = q.emits.partition_point(|emit| (emit.pattern as usize) < found.pattern_index);
    for emit in q.emits[first_emit..]
        .iter()
        .take_while(|emit| emit.pattern as usize == found.pattern_index)
    {
        let capture = |name| {
            found.captures.iter().find(|capture| capture.index as u16 == name)
                .map(|capture| capture.node.start_byte() as u32..capture.node.end_byte() as u32)
        };
        let mut fields = Vec::with_capacity(emit.fields.len());
        for field in &emit.fields {
            let value = match field.source {
                EmitSource::Capture(name) => capture(name).map(EmittedValue::Bytes),
                EmitSource::Literal(index) => Some(EmittedValue::Literal(index)),
            };
            if let Some(value) = value {
                fields.push(EmittedField { key: field.key, value });
            }
        }
        if fields.len() != emit.fields.len() {
            continue;
        }
        let start = arena.emitted_fields.len() as u32;
        arena.emitted_fields.extend(fields);
        arena.emitted.push(EmittedFact {
            file,
            relation: emit.relation,
            fields: start..arena.emitted_fields.len() as u32,
        });
    }
}
