//! `--lines`: the sink-side line decoration. The extractor law stays
//! untouched (src/lang/ts.rs:2832): a node is a byte Span, and line/col are
//! added in the CLI funnel or not at all. Off, stdout is byte-identical; on,
//! every start/end pair carries 1-based line and col, and col counts BYTES
//! from the line start.
#![cfg(feature = "cli")]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

/// `const α = 1;` newlines at 13, `function hop...` ends at 56, one call on
/// line 3. The α characters are where byte cols and character cols disagree.
const TS_FIXTURE: &str = "const \u{3b1} = 1;\nfunction hop(n: number) { return n + \u{3b1}; }\nhop(\u{3b1});\n";
/// The TS fixture's newline offsets: end of each of the first two lines and
/// the trailing newline.
const TS_OFFSETS: [u32; 3] = [13, 56, 65];

fn ryi(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(args)
        .env("RUST_LOG", "off")
        .output()
        .expect("run ryi")
}

fn stdout_lines(output: &std::process::Output) -> Vec<String> {
    assert!(output.status.success());
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn write_fixture(dir: &Path, name: &str, content: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, content).expect("write fixture");
    path
}

/// The (line, col) of `start` against `offsets`, 1-based, byte columns: the
/// same arithmetic the span_lines view applies in SQL.
fn expected_line_col(offsets: &[u32], start: u32) -> (u32, u32) {
    let line = offsets.partition_point(|offset| *offset < start);
    let line_start = line.checked_sub(1).map_or(0, |index| offsets[index] + 1);
    (line as u32 + 1, start - line_start + 1)
}

fn assert_decorated(value: &Value, offsets: &[u32]) {
    match value {
        Value::Object(map) => {
            let start = map.get("start").and_then(Value::as_u64);
            let end = map.get("end").and_then(Value::as_u64);
            if let (Some(start), Some(end)) = (start, end) {
                let (line, col) = expected_line_col(offsets, start as u32);
                assert_eq!(map.get("line"), Some(&Value::from(line)), "line at {value}");
                assert_eq!(map.get("col"), Some(&Value::from(col)), "col at {value}");
                assert!(end >= start);
            } else {
                assert!(!map.contains_key("line") && !map.contains_key("col"),
                    "decoration escaped a non-span object: {value}");
            }
            for child in map.values() {
                assert_decorated(child, offsets);
            }
        }
        Value::Array(items) => {
            for item in items {
                assert_decorated(item, offsets);
            }
        }
        _ => {}
    }
}

#[test]
fn undecorated_stdout_is_byte_identical() {
    let dir = scratch("ryi-lines-off");
    let fixture = write_fixture(&dir, "fixture.ts", TS_FIXTURE);
    let path = fixture.to_string_lossy().into_owned();
    let digest = "blake3:5aaf58237f593b49a71d78bc09924050d18f011bafe1b4c396f00dc3322da450";
    let output = ryi(&["--family", "call", "--file-fact", &path]);
    assert_eq!(
        stdout_lines(&output),
        vec![
            format!(r#"{{"record":"file","path":"{path}","digest":"{digest}","bytes":66,"lines":3}}"#),
            r#"{"record":"node","family":"call","span":{"start":14,"end":56},"kind":"function","name":"hop"}"#.to_string(),
            r#"{"record":"node","family":"call","span":{"start":0,"end":66},"kind":"module","name":"<module>"}"#.to_string(),
            r#"{"record":"site","family":"call","span":{"start":57,"end":60},"callee":"hop","callee_path":null}"#.to_string(),
        ],
        "undecorated stdout moved; the no-flag contract is byte-identity"
    );
}

#[test]
fn lines_decorates_every_span_and_only_spans() {
    let dir = scratch("ryi-lines-on");
    let fixture = write_fixture(&dir, "fixture.ts", TS_FIXTURE);
    let output = ryi(&["--family", "call", "--lines", &fixture.to_string_lossy()]);
    let lines = stdout_lines(&output);
    for line in &lines {
        let value: Value = serde_json::from_str(line).expect("each stdout row is JSON");
        assert_decorated(&value, &TS_OFFSETS);
    }
    // Inline snapshots of the decorated rows: today's bytes plus line/col and
    // nothing else.
    assert_eq!(
        lines,
        vec![
            r#"{"record":"node","family":"call","span":{"start":14,"end":56,"line":2,"col":1},"kind":"function","name":"hop"}"#.to_string(),
            r#"{"record":"node","family":"call","span":{"start":0,"end":66,"line":1,"col":1},"kind":"module","name":"<module>"}"#.to_string(),
            r#"{"record":"site","family":"call","span":{"start":57,"end":60,"line":3,"col":1},"callee":"hop","callee_path":null}"#.to_string(),
        ],
    );
}

#[test]
fn col_counts_bytes_not_characters() {
    // `{"first": "α", "second": "β"}`: the `second` value starts at byte 27,
    // two bytes past the α, so its byte col is 28 where a character col
    // would say 27.
    let dir = scratch("ryi-lines-bytes");
    let fixture = write_fixture(&dir, "fixture.json", "{\"first\": \"\u{3b1}\", \"second\": \"\u{3b2}\"}\n");
    let output = ryi(&["--family", "data", "--lines", &fixture.to_string_lossy()]);
    assert_eq!(
        stdout_lines(&output),
        vec![
            r#"{"record":"data_doc","family":"data","ordinal":0,"span":{"start":0,"end":31,"line":1,"col":1},"format":"json","doc":{"first":"α","second":"β"}}"#.to_string(),
            r#"{"record":"data_value","family":"data","ordinal":0,"path":"","kind":"object","text":null,"span":{"start":0,"end":31,"line":1,"col":1}}"#.to_string(),
            r#"{"record":"data_value","family":"data","ordinal":0,"path":"first","kind":"string","text":"α","span":{"start":11,"end":13,"line":1,"col":12}}"#.to_string(),
            r#"{"record":"data_value","family":"data","ordinal":0,"path":"second","kind":"string","text":"β","span":{"start":27,"end":29,"line":1,"col":28}}"#.to_string(),
        ],
        "col must count bytes from the line start, not characters"
    );
}

#[test]
fn sqlite_arm_writes_line_start_and_the_view_joins_it() {
    let dir = scratch("ryi-lines-db");
    let fixture = write_fixture(&dir, "fixture.ts", TS_FIXTURE);
    let database = dir.join("facts.db");
    let output = ryi(&[
        "--sqlite",
        &database.to_string_lossy(),
        "--lines",
        &fixture.to_string_lossy(),
    ]);
    assert!(output.status.success());
    let connection = rusqlite::Connection::open(&database).expect("open export");
    let (path, digest, offsets): (String, String, String) = connection
        .query_row(
            "SELECT path, digest, offsets FROM line_start",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("one line_start row");
    assert_eq!(path, fixture.to_string_lossy());
    assert_eq!(digest, "blake3:5aaf58237f593b49a71d78bc09924050d18f011bafe1b4c396f00dc3322da450");
    assert_eq!(offsets, "[13,56,65]");
    let line: u32 = connection
        .query_row(
            "SELECT line FROM span_lines WHERE _table = 'site' AND start = 57",
            [],
            |row| row.get(0),
        )
        .expect("the view resolves the site span");
    assert_eq!(line, 3, "span_lines joins spans to line_start");
}

#[test]
fn sqlite_arm_without_the_flag_leaves_line_start_empty() {
    // The committed gate: line_start rows ride --lines only, and the
    // span_lines summary says so ("needs --lines for non-empty line_start").
    let dir = scratch("ryi-lines-gate");
    let fixture = write_fixture(&dir, "fixture.ts", TS_FIXTURE);
    let database = dir.join("facts.db");
    let output = ryi(&["--sqlite", &database.to_string_lossy(), &fixture.to_string_lossy()]);
    assert!(output.status.success());
    let connection = rusqlite::Connection::open(&database).expect("open export");
    let rows: i64 = connection
        .query_row("SELECT count(*) FROM line_start", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn stdout_never_carries_a_line_start_record() {
    // line_start is the sqlite-side join key; the stdout contract only adds
    // line/col decoration.
    let dir = scratch("ryi-lines-stdout");
    let fixture = write_fixture(&dir, "fixture.ts", TS_FIXTURE);
    let output = ryi(&["--lines", &fixture.to_string_lossy()]);
    for line in stdout_lines(&output) {
        assert!(!line.contains(r#""record":"line_start""#), "{line}");
    }
}

#[test]
fn resolve_decorates_rows_by_their_own_path() {
    // A multi-file stream keys each row against the file the row itself
    // names: the unresolved site sits in b.ts and decorates against b.ts's
    // table, while rows with no span and no single file stay raw.
    let dir = scratch("ryi-lines-resolve");
    let b_ts = "export function go() {\n  return Math.pow(2, 3);\n}\n";
    let a_ts = "import { helper } from \"./b\";\nimport { gone } from \"./nowhere\";\nexport function use() {\n  return gone + helper;\n}\n";
    let a_path = write_fixture(&dir, "a.ts", a_ts);
    let b_path = write_fixture(&dir, "b.ts", b_ts);
    let offsets_b: Vec<u32> = b_ts
        .bytes()
        .enumerate()
        .filter(|(_, byte)| *byte == b'\n')
        .map(|(index, _)| index as u32)
        .collect();
    let output = ryi(&[
        "--resolve",
        "--lines",
        &a_path.to_string_lossy(),
        &b_path.to_string_lossy(),
    ]);
    let mut saw_unresolved = false;
    for line in stdout_lines(&output) {
        let value: Value = serde_json::from_str(&line).expect("every line is one record");
        match value.get("record").and_then(Value::as_str) {
            Some("unresolved") => {
                saw_unresolved = true;
                assert_eq!(value["path"], Value::from(b_path.to_string_lossy().as_ref()));
                assert_decorated(&value, &offsets_b);
            }
            Some("resolved_import") => {
                assert!(value.get("line").is_none(), "{line}");
                assert!(value.get("col").is_none(), "{line}");
            }
            _ => {}
        }
    }
    assert!(saw_unresolved, "the fixture must emit an unresolved row");
}

#[test]
fn resolve_without_the_flag_stays_byte_offset() {
    let dir = scratch("ryi-lines-resolve-off");
    let b_ts = "export function go() {\n  return Math.pow(2, 3);\n}\n";
    let a_ts = "import { helper } from \"./b\";\nimport { gone } from \"./nowhere\";\nexport function use() {\n  return gone + helper;\n}\n";
    let a_path = write_fixture(&dir, "a.ts", a_ts);
    let b_path = write_fixture(&dir, "b.ts", b_ts);
    let output = ryi(&[
        "--resolve",
        &a_path.to_string_lossy(),
        &b_path.to_string_lossy(),
    ]);
    for line in stdout_lines(&output) {
        assert!(!line.contains("\"line\":"), "{line}");
        assert!(!line.contains("_line\":"), "{line}");
    }
}

#[test]
fn resolve_decorates_edge_spans_by_their_owning_path() {
    // resolved_edge carries two files' spans: the caller site decorates
    // against caller_path's table and the callee against callee_path's;
    // resolved_type_edge does the same for owner and target. One
    // decoration per owning path, so a cross-file row mixes two tables.
    let dir = scratch("ryi-lines-edges");
    let a_ts = "import { Animal, feed } from \"./b\";\nexport function pet(a: Animal): string {\n  return a.sound();\n}\nexport const run = feed(pet);\n";
    let b_ts = "export class Animal {\n  sound(): string {\n    return \"\";\n  }\n}\nexport function feed(fn: (a: Animal) => string): string {\n  return fn(new Animal());\n}\n";
    let a_path = write_fixture(&dir, "a.ts", a_ts);
    let b_path = write_fixture(&dir, "b.ts", b_ts);
    let offsets_of = |content: &str| -> Vec<u32> {
        content
            .bytes()
            .enumerate()
            .filter(|(_, byte)| *byte == b'\n')
            .map(|(index, _)| index as u32)
            .collect()
    };
    let a_string = a_path.to_string_lossy().into_owned();
    let b_string = b_path.to_string_lossy().into_owned();
    let mut saw_edge = false;
    let mut saw_cross_file_edge = false;
    let mut saw_type_edge = false;
    let mut saw_owned_span = false;
    let outputs = [
        ryi(&["--resolve", "--lines", &a_string, &b_string]),
        ryi(&[
            "--resolve",
            "--family",
            "type",
            "--lines",
            &a_string,
            &b_string,
        ]),
    ];
    for output in &outputs {
        for line in stdout_lines(output) {
            let value: Value = serde_json::from_str(&line).expect("one record per line");
            let record = value.get("record").and_then(Value::as_str).unwrap_or("");
            let pairs: &[(&str, &str)] = match record {
                "resolved_edge" => {
                    saw_edge = true;
                    if value["caller_path"] != value["callee_path"] {
                        saw_cross_file_edge = true;
                    }
                    &[
                        ("caller_path", "caller_site_start"),
                        ("callee_path", "callee_start"),
                    ]
                }
                "resolved_type_edge" => {
                    saw_type_edge = true;
                    &[("owner_path", "owner_start"), ("target_path", "target_start")]
                }
                _ => &[],
            };
            for (path_field, start_field) in pairs {
                let Some(start) = value.get(*start_field).and_then(Value::as_u64) else {
                    continue;
                };
                let path = value
                    .get(*path_field)
                    .and_then(Value::as_str)
                    .expect("every owned span names its file");
                let content = if path == a_string { a_ts } else { b_ts };
                let (line_no, col) = expected_line_col(&offsets_of(content), start as u32);
                let line_field = start_field.replace("start", "line");
                let col_field = start_field.replace("start", "col");
                assert_eq!(
                    value.get(line_field.as_str()),
                    Some(&Value::from(line_no)),
                    "{line_field} at {line}"
                );
                assert_eq!(
                    value.get(col_field.as_str()),
                    Some(&Value::from(col)),
                    "{col_field} at {line}"
                );
                saw_owned_span = true;
            }
        }
    }
    assert!(saw_edge, "the fixture must emit resolved_edge rows");
    assert!(saw_cross_file_edge, "at least one edge must cross files");
    assert!(saw_type_edge, "the typed run must emit resolved_type_edge");
    assert!(saw_owned_span, "every owned span must carry line and col");
}

#[test]
fn diet_scip_decorates_edge_spans_by_their_owning_path() {
    // The fast family emits the same resolved_edge rows through the same
    // funnel, so its two-file spans decorate against their owning files too.
    let dir = scratch("ryi-lines-diet-edges");
    let a_ts = "import { feed } from \"./b\";\nexport const run = feed();\n";
    let b_ts = "export function feed(): number {\n  return 1;\n}\n";
    let a_path = write_fixture(&dir, "a.ts", a_ts);
    let b_path = write_fixture(&dir, "b.ts", b_ts);
    let output = ryi(&[
        "--family",
        "diet_scip",
        "--lines",
        &a_path.to_string_lossy(),
        &b_path.to_string_lossy(),
    ]);
    let mut saw_decorated_edge = false;
    for line in stdout_lines(&output) {
        let value: Value = serde_json::from_str(&line).expect("one record per line");
        if value.get("record").and_then(Value::as_str) == Some("resolved_edge") {
            assert!(
                value.get("caller_site_line").is_some(),
                "caller site decorates: {line}"
            );
            assert!(
                value.get("callee_line").is_some(),
                "callee decorates: {line}"
            );
            saw_decorated_edge = true;
        }
    }
    assert!(saw_decorated_edge, "the fixture must emit resolved_edge rows");
}
