//! One byte-span coordinate decoration for JSONL. End lines are inclusive.

use serde_json::Value;

/// 1-based (line, col) of a byte against newline offsets. Col counts BYTES
/// from the line start, not characters, matching the spans it decorates.
pub fn line_col(offsets: &[u32], start: u32) -> (u32, u32) {
    let line = offsets.partition_point(|offset| *offset < start);
    let line_start = line.checked_sub(1).map_or(0, |index| offsets[index] + 1);
    ((line + 1) as u32, start - line_start + 1)
}

/// Add `line`, `col`, and `line_end` beside every `start`/`end` pair at any depth, so a
/// decorated record is the undecorated bytes plus exactly those fields.
pub(super) fn decorate_lines(value: &mut Value, offsets: &[u32]) {
    match value {
        Value::Object(map) => {
            let start = map.get("start").and_then(Value::as_u64);
            let end = map.get("end").and_then(Value::as_u64);
            if let (Some(start), Some(end)) = (start, end) {
                let (line, col) = line_col(offsets, start as u32);
                map.insert("line".to_string(), Value::from(line));
                map.insert("col".to_string(), Value::from(col));
                let last = if end > start { end - 1 } else { start };
                map.insert(
                    "line_end".to_string(),
                    Value::from(line_col(offsets, last as u32).0),
                );
            }
            for child in map.values_mut() {
                decorate_lines(child, offsets);
            }
        }
        Value::Array(items) => {
            for item in items {
                decorate_lines(item, offsets);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_call_rows_include_end_lines_at_newline_boundaries() {
        let source = b"fn run() {\n    helper();\n}\n";
        let mut rows = serde_json::json!([
            {"record": "node", "family": "call", "kind": "function", "name": "run", "span": {"start": 3, "end": 26}},
            {"record": "site", "family": "call", "callee": "helper", "span": {"start": 15, "end": 23}},
            {"record": "node", "span": {"start": 0, "end": 11}},
            {"record": "node", "span": {"start": 11, "end": 11}}
        ]);
        decorate_lines(&mut rows, &sprefa_extract::newline_offsets(source));
        insta::with_settings!({
            snapshot_path => concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"),
            prepend_module_to_snapshot => false,
        }, {
            insta::assert_json_snapshot!("call_span_end_lines", rows);
        });
    }
}
