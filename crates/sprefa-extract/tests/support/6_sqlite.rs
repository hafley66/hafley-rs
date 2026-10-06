//! SQLite export behavior over fixture command sequences: typed catalog
//! round-trips, publication atomicity, batch ceilings and the byte budget.
//! Old per-claim assertions stay active here; the whole-output snapshot pins
//! the observed values on top.

use rusqlite::{types::Value as SqlValue, Connection};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;

use crate::t_0_sqlite::sqlite;

pub fn evaluate(case: &Value) -> Value {
    let mut output = crate::fixture_runner::commands(case, api);
    let mut cells = Vec::new();
    for value in output.as_object_mut().unwrap().values_mut() {
        if let Some(rows) = value.get_mut("rows").filter(|r| r.as_array().is_some_and(|r| r.first().is_some_and(|r| r.get("values").is_some()))) {
            *rows = compact_rows(rows.as_array().unwrap().iter().map(|row| &row["values"]), &mut cells);
        }
        if let Some(tables) = value.get_mut("uncoordinated").and_then(Value::as_object_mut) {
            for rows in tables.values_mut() {
                *rows = compact_rows(rows.as_array().unwrap().iter(), &mut cells);
            }
        }
    }
    if !cells.is_empty() { output["cells"] = json!(cells.chunks(16).map(|chunk| serde_json::to_string(chunk).unwrap()).collect::<Vec<_>>()); }
    output
}

// Typed cell dictionary: strings are SQL text, integers SQL integers; null,
// real/blob/redaction tags retain their original variants. Row order is exact.
// Cell indices address 16-cell JSON chunks: chunk = index / 16, slot = index % 16.
fn compact_rows<'a>(rows: impl Iterator<Item = &'a Value>, cells: &mut Vec<Value>) -> Value {
    Value::Array(rows.map(|row| {
        let indices: Vec<_> = row.as_array().unwrap().iter().map(|value| {
            let cell = value.get("text").or_else(|| value.get("integer")).unwrap_or(value).clone();
            match cells.iter().position(|stored| stored == &cell) {
                Some(index) => index,
                None => { cells.push(cell); cells.len() - 1 }
            }
        }).collect();
        Value::String(serde_json::to_string(&indices).unwrap())
    }).collect())
}

fn api(step: &Value) -> Value {
    match step["api"].as_str().unwrap() {
        "stream_order" => stream_order(step),
        "catalog_covers" => catalog_covers(step),
        "roundtrip" => roundtrip(step),
        "jsonl_matches" => jsonl_matches(step),
        "reject_rows" => reject_rows(step),
        "create_rejected" => create_rejected(step),
        "finish_race" => finish_race(step),
        "batch_boundaries" => batch_boundaries(step),
        "byte_budget" => byte_budget(step),
        other => panic!("unknown sqlite api: {other}"),
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|fields| {
            fields
                .iter()
                .map(|field| field.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn typed(value: SqlValue) -> Value {
    match value {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(value) => json!({"integer":value}),
        SqlValue::Real(value) => json!({"real":value}),
        SqlValue::Text(value) => json!({"text":value}),
        SqlValue::Blob(value) => json!({"blob":value.len()}),
    }
}

/// SQLite storage types are part of the claim; fields the old test left
/// unpinned (runtime timestamps) redact to a stable marker.
fn typed_row(values: &[SqlValue], columns: &[&Column], redact: &[String]) -> Vec<Value> {
    values
        .iter()
        .zip(columns)
        .map(|(value, column)| {
            if redact.iter().any(|field| field == &column.name) {
                json!({"redacted":true})
            } else {
                typed(value.clone())
            }
        })
        .collect()
}

const CATALOG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/schema/generated/5_facts.json"
));

#[derive(Deserialize)]
struct Column {
    name: String,
    path: Vec<String>,
    kind: String,
    optional: bool,
    nullable: bool,
    literal: Option<String>,
    values: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct Table {
    record: String,
    table: String,
    columns: Vec<Column>,
}

fn tables() -> Vec<Table> {
    serde_json::from_str(CATALOG).unwrap()
}
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

fn sql_values(value: &Value, columns: &[&Column]) -> Vec<SqlValue> {
    columns
        .iter()
        .map(|c| {
            let value = c.path.iter().try_fold(value, |v, k| v.get(k));
            match value {
                None => SqlValue::Null,
                Some(v) if c.kind == "json" => SqlValue::Text(v.to_string()),
                Some(Value::Null) => SqlValue::Null,
                Some(Value::String(s)) => SqlValue::Text(s.clone()),
                Some(Value::Bool(b)) => SqlValue::Integer(i64::from(*b)),
                Some(Value::Number(n)) => n
                    .as_i64()
                    .map_or_else(|| SqlValue::Text(n.to_string()), SqlValue::Integer),
                other => panic!("Unhandled {other:?}"),
            }
        })
        .collect()
}

fn count(connection: &Connection, table: &str) -> i64 {
    connection
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap()
}

fn total_rows(connection: &Connection) -> i64 {
    tables()
        .iter()
        .map(|table| count(connection, &quote(&table.table)))
        .sum()
}

/// The old `assert_rows_at`: every expected row read back through the catalog
/// at its exact `_row`, typed values compared, then total count and integrity.
fn rows_at(
    connection: &Connection,
    expected: &[Value],
    first_row: i64,
    exact: bool,
    redact: &[String],
) -> Value {
    let catalog = tables();
    let mut stored = Vec::new();
    for (i, value) in expected.iter().enumerate() {
        let table = catalog
            .iter()
            .find(|t| value["record"] == t.record)
            .unwrap();
        let columns: Vec<_> = table
            .columns
            .iter()
            .filter(|c| !c.name.starts_with('_'))
            .collect();
        let sql = format!(
            "SELECT {} FROM {} WHERE _row=?",
            columns
                .iter()
                .map(|c| quote(&c.name))
                .collect::<Vec<_>>()
                .join(","),
            quote(&table.table)
        );
        let got: Vec<SqlValue> = connection
            .query_row(&sql, [first_row + i as i64], |r| {
                (0..columns.len())
                    .map(|col| r.get::<_, SqlValue>(col))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap_or_else(|e| panic!("{sql}: {e}"));
        let want = sql_values(value, &columns);
        assert_eq!(got, want, "row {}: {}", first_row + i as i64, value);
        stored.push(json!({"record":value["record"],"values":typed_row(&got, &columns, redact)}));
    }
    let total = total_rows(connection);
    if exact {
        assert_eq!(total, expected.len() as i64);
    }
    assert_eq!(
        connection
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    json!({"rows":stored,"total":total,"integrity":"ok"})
}

/// The old `assert_uncoordinated_rows`: uncoordinated (raw) rows per table,
/// compared as a sorted set against the expected tail rows.
fn uncoordinated(connection: &Connection, expected: &[Value], redact: &[String]) -> Value {
    let mut per_table = serde_json::Map::new();
    for table in tables() {
        let columns: Vec<_> = table
            .columns
            .iter()
            .filter(|column| !column.name.starts_with('_'))
            .collect();
        let sql = format!(
            "SELECT {} FROM {} WHERE _input_path IS NULL AND _content_id IS NULL ORDER BY _row",
            columns
                .iter()
                .map(|column| quote(&column.name))
                .collect::<Vec<_>>()
                .join(","),
            quote(&table.table),
        );
        let mut got: Vec<Vec<SqlValue>> = connection
            .prepare(&sql)
            .unwrap()
            .query_map([], |row| {
                (0..columns.len())
                    .map(|column| row.get::<_, SqlValue>(column))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let mut want: Vec<_> = expected
            .iter()
            .filter(|value| value["record"] == table.record)
            .map(|value| sql_values(value, &columns))
            .collect();
        got.sort_by_key(|row| format!("{row:?}"));
        want.sort_by_key(|row| format!("{row:?}"));
        assert_eq!(got, want, "{}", table.table);
        per_table.insert(
            table.table.clone(),
            Value::Array(
                got.iter()
                    .map(|row| Value::Array(typed_row(row, &columns, redact)))
                    .collect(),
            ),
        );
    }
    Value::Object(per_table)
}

/// Streaming must hand back exactly the collected project rows, in order.
fn stream_order(step: &Value) -> Value {
    let scratch = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for index in 0..step["count"].as_u64().unwrap() {
        let key = index.to_string();
        let extension = step["extensions"][key.as_str()]
            .as_str()
            .unwrap_or(step["default_extension"].as_str().unwrap());
        let text = step["contents"][key.as_str()]
            .as_str()
            .unwrap_or(step["default_content"].as_str().unwrap());
        let path = scratch.path().join(format!("{index:04}.{extension}"));
        std::fs::write(&path, text).unwrap();
        paths.push(path);
    }
    let expected: Vec<Value> = sprefa_extract::diet_scip(&paths)
        .unwrap()
        .iter()
        .map(|fact| serde_json::to_value(fact).unwrap())
        .collect();
    let mut actual = Vec::new();
    sprefa_extract::diet_scip_streamed(&paths, &mut |row| {
        if let sprefa_extract::DietRow::Resolved(fact) = row {
            actual.push(serde_json::to_value(fact).unwrap());
        }
        Ok::<(), std::io::Error>(())
    })
    .unwrap();
    assert_eq!(actual, expected);
    let stream_root = scratch.path().to_str().unwrap();
    json!({
        "files":paths.len(),
        "rows":expected.len(),
        "equal":true,
        "resolved": actual
            .iter()
            .map(|row| crate::fixture_runner::replace_strings(row, &[(stream_root, "$stream"), ("/private$stream", "$stream")]))
            .collect::<Vec<_>>(),
    })
}

/// Every FlatFact variant and its fields carry a TypeSpec table, and the
/// pinned enums match their Rust sources.
fn catalog_covers(step: &Value) -> Value {
    let source =
        syn::parse_file(include_str!("../../../hafley_scm/src/read/0c_flat_fact.rs")).unwrap();
    let flat = source
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Enum(e) if e.ident == "FlatFact" => Some(e),
            _ => None,
        })
        .unwrap();
    let catalog = tables();
    fn renamed(attrs: &[syn::Attribute]) -> Option<String> {
        let mut result = None;
        for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("rename") {
                    result = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                } else if meta.input.peek(syn::Token![=]) {
                    let _: syn::Expr = meta.value()?.parse()?;
                }
                Ok(())
            })
            .unwrap();
        }
        result
    }
    let mut covered = BTreeSet::new();
    let mut variants = Vec::new();
    let mut field_sets = serde_json::Map::new();
    for variant in &flat.variants {
        let tag =
            renamed(&variant.attrs).unwrap_or_else(|| variant.ident.to_string().to_lowercase());
        let table = catalog
            .iter()
            .find(|t| t.record == tag)
            .unwrap_or_else(|| panic!("Missing TypeSpec table: {tag}"));
        covered.insert(tag.clone());
        variants.push(tag.clone());
        if let syn::Fields::Named(fields) = &variant.fields {
            let rust: BTreeSet<_> = fields
                .named
                .iter()
                .map(|f| renamed(&f.attrs).unwrap_or_else(|| f.ident.as_ref().unwrap().to_string()))
                .collect();
            let tsp: BTreeSet<_> = table
                .columns
                .iter()
                .map(|c| c.path[0].clone())
                .filter(|p| p != "record" && !p.starts_with('_'))
                .collect();
            assert_eq!(tsp, rust, "Field drift: {tag}");
            field_sets.insert(
                tag.clone(),
                Value::Array(tsp.iter().cloned().map(Value::String).collect()),
            );
        }
    }
    let extras: BTreeSet<_> = catalog
        .iter()
        .map(|t| t.record.clone())
        .filter(|t| !covered.contains(t))
        .collect();
    assert_eq!(extras, BTreeSet::from(["capture".to_owned()]));
    let tsi = syn::parse_file(include_str!("../../../hafley_scm/src/read/tsi/types.rs")).unwrap();
    let atoms = syn::parse_file(include_str!("../../../hafley_scm/src/atoms.rs")).unwrap();
    let mut enums = serde_json::Map::new();
    for spec in step["enums"].as_array().unwrap() {
        let spec = spec.as_array().unwrap();
        let (file, enum_name, table_name, column_name, snake_case) = (
            spec[0].as_str().unwrap(),
            spec[1].as_str().unwrap(),
            spec[2].as_str().unwrap(),
            spec[3].as_str().unwrap(),
            spec[4].as_bool().unwrap(),
        );
        let file = match file {
            "atoms" => &atoms,
            "tsi" => &tsi,
            other => panic!("unknown enum source: {other}"),
        };
        let e = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Enum(e) if e.ident == enum_name => Some(e),
                _ => None,
            })
            .unwrap();
        let names: Vec<String> = e
            .variants
            .iter()
            .map(|v| {
                let name = v.ident.to_string();
                let mut result = String::new();
                for (i, ch) in name.chars().enumerate() {
                    if snake_case && i > 0 && ch.is_ascii_uppercase() {
                        result.push('_');
                    }
                    result.push(ch.to_ascii_lowercase());
                }
                result
            })
            .collect();
        let column = catalog
            .iter()
            .find(|t| t.record == table_name)
            .unwrap()
            .columns
            .iter()
            .find(|c| c.name == column_name)
            .unwrap();
        assert_eq!(
            column.values.as_ref().unwrap(),
            &names,
            "Enum drift: {enum_name}"
        );
        enums.insert(
            enum_name.to_string(),
            Value::Array(names.iter().cloned().map(Value::String).collect()),
        );
    }
    json!({
        "variants": variants,
        "fields": Value::Object(field_sets),
        "tables": catalog.iter().map(|t| t.record.clone()).collect::<Vec<_>>(),
        "enums": Value::Object(enums),
    })
}

/// Fixture rows must deserialize as FlatFact (when the case carries that
/// guard), insert, and read back typed through the same catalog.
fn roundtrip(step: &Value) -> Value {
    let path = Path::new(step["database"].as_str().unwrap());
    let rows: Vec<Value> = step["rows"].as_array().unwrap().clone();
    let flatfact = step["flatfact"].as_bool().unwrap_or(false);
    let mut db = sqlite::Database::create(path).unwrap();
    for row in &rows {
        if flatfact && row["record"] != "capture" {
            serde_json::from_value::<sprefa_extract::FlatFact>(row.clone())
                .unwrap_or_else(|e| panic!("{}: {e}: {row}", row["record"]));
        }
        db.insert(row.clone())
            .unwrap_or_else(|e| panic!("{}: {e}: {row}", row["record"]));
    }
    db.finish().unwrap();
    let connection = Connection::open(path).unwrap();
    rows_at(&connection, &rows, 1, true, &strings(&step["redact"]))
}

/// JSONL a run streamed must land in SQLite column for column, typed.
fn jsonl_matches(step: &Value) -> Value {
    let connection = Connection::open(step["database"].as_str().unwrap()).unwrap();
    let mut expected: Vec<Value> = std::fs::read_to_string(step["jsonl"].as_str().unwrap())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    if let Some(source) = step["witness"].as_str() {
        expected.insert(
            0,
            serde_json::to_value(sprefa_extract::file_fact(
                source,
                &std::fs::read(source).unwrap(),
            ))
            .unwrap(),
        );
    }
    assert!(!expected.is_empty(), "Empty test input: {step}");
    let redact = strings(&step["redact"]);
    match step["mode"].as_str().unwrap() {
        "exact" => {
            let mut result = rows_at(&connection, &expected, 1, true, &redact);
            result["mode"] = json!("exact");
            result
        }
        "retains_raw" => {
            let total = total_rows(&connection);
            assert!(
                total > expected.len() as i64,
                "{total} <= {}",
                expected.len()
            );
            let first_resolved = total - expected.len() as i64 + 1;
            let uncoordinated_rows = uncoordinated(&connection, &expected, &redact);
            let sourced: i64 = tables()
                .iter()
                .map(|table| {
                    connection
                        .query_row(
                            &format!(
                                "SELECT count(*) FROM {} WHERE _row < ? AND _input_path IS NOT NULL AND _content_id IS NOT NULL",
                                quote(&table.table)
                            ),
                            [first_resolved],
                            |row| row.get::<_, i64>(0),
                        )
                        .unwrap()
                })
                .sum();
            assert_eq!(sourced, first_resolved - 1);
            let inherited: i64 = tables()
                .iter()
                .map(|table| {
                    connection
                        .query_row(
                            &format!(
                                "SELECT count(*) FROM {} WHERE _row >= ? AND (_input_path IS NOT NULL OR _content_id IS NOT NULL)",
                                quote(&table.table)
                            ),
                            [first_resolved],
                            |row| row.get::<_, i64>(0),
                        )
                        .unwrap()
                })
                .sum();
            assert_eq!(inherited, 0);
            json!({
                "mode":"retains_raw",
                "total":total,
                "first_resolved":first_resolved,
                "sourced":sourced,
                "inherited":inherited,
                "uncoordinated":uncoordinated_rows,
            })
        }
        other => panic!("unknown jsonl match mode: {other}"),
    }
}

/// Malformed rows must fail the insert and leave nothing on disk.
fn reject_rows(step: &Value) -> Value {
    let path = Path::new(step["database"].as_str().unwrap());
    let parent = path.parent().unwrap();
    let rows = step["rows"].as_array().unwrap();
    let mut rejected = Vec::new();
    for value in rows {
        let mut db = sqlite::Database::create(path).unwrap();
        assert!(db.insert(value.clone()).is_err());
        drop(db);
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(parent).unwrap().count(), 0);
        rejected.push(json!({
            "input": value,
            "insert_rejected": true,
            "destination_absent": !path.exists(),
            "staging_absent": !std::fs::read_dir(parent)
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".extract-sqlite-")),
        }));
    }
    json!({"rejected":rejected})
}

fn create_rejected(step: &Value) -> Value {
    assert!(sqlite::Database::create(Path::new(step["database"].as_str().unwrap())).is_err());
    json!({"rejected":true})
}

/// A concurrent writer on the destination path must lose the publication,
/// never the file it raced with.
fn finish_race(step: &Value) -> Value {
    let path = Path::new(step["database"].as_str().unwrap());
    let contents = step["contents"].as_str().unwrap();
    let db = sqlite::Database::create(path).unwrap();
    std::fs::write(path, contents).unwrap();
    assert!(db.finish().is_err());
    assert_eq!(std::fs::read(path).unwrap(), contents.as_bytes());
    json!({"preserved":contents})
}

/// Batch flushes walk the runtime row ceiling at source boundaries and no
/// table carries a secondary index. Ceiling-relative probe positions keep the
/// runtime-dependent row number out of the snapshot.
fn batch_boundaries(step: &Value) -> Value {
    let ceiling = sqlite::writers::max_batch_rows(&Connection::open_in_memory().unwrap()).unwrap();
    let path = Path::new(step["database"].as_str().unwrap());
    let mut db = sqlite::Database::create(path).unwrap();
    for group in step["sources"].as_array().unwrap() {
        let source = group["source"].as_array().unwrap();
        db.source(
            source[0].as_str().unwrap(),
            source[1].as_str().unwrap().to_owned(),
        )
        .unwrap();
        let rows = match &group["rows"] {
            Value::Number(rows) => rows.as_u64().unwrap() as usize,
            Value::Object(_) => ceiling + group["rows"]["ceiling_plus"].as_u64().unwrap() as usize,
            other => panic!("unknown rows spec: {other}"),
        };
        for _ in 0..rows {
            db.insert(json!({"record":"protocol","version":1})).unwrap();
        }
    }
    db.finish().unwrap();
    let connection = Connection::open(path).unwrap();
    let mut probes = serde_json::Map::new();
    for probe in step["probes"].as_array().unwrap() {
        let row = match probe {
            Value::String(first) if first == "first" => 1,
            Value::Number(past) => ceiling as i64 + past.as_i64().unwrap(),
            other => panic!("unknown probe: {other}"),
        };
        let got: (i64, String, String) = connection
            .query_row(
                "SELECT _row, _input_path, _content_id FROM protocol WHERE _row = ?",
                [row],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(got.0, row);
        probes.insert(
            match probe {
                Value::String(first) => first.clone(),
                Value::Number(past) => past.to_string(),
                _ => unreachable!(),
            },
            json!({"input_path":got.1,"content_id":got.2}),
        );
    }
    let mut evidence = serde_json::Map::new();
    for table in tables() {
        let columns: Vec<(String, String, i64)> = connection
            .prepare(&format!("PRAGMA table_info({})", quote(&table.table)))
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        let primary_keys: Vec<_> = columns
            .iter()
            .filter(|(_, _, primary_key)| *primary_key != 0)
            .cloned()
            .collect();
        assert_eq!(
            primary_keys,
            vec![("_row".to_owned(), "INTEGER".to_owned(), 1)]
        );
        let secondary_indexes: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_index_list(?)",
                [&table.table],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(secondary_indexes, 0, "{}", table.table);
        evidence.insert(
            table.table.clone(),
            json!({"primary_key":primary_keys,"secondary_indexes":secondary_indexes}),
        );
    }
    let oversized_spec = &step["oversized"];
    let oversized_path = Path::new(oversized_spec["database"].as_str().unwrap());
    let mut oversized = sqlite::Database::create(oversized_path).unwrap();
    for row in oversized_spec["rows"].as_array().unwrap() {
        oversized.insert(expand_repeat(row)).unwrap();
    }
    oversized.finish().unwrap();
    let oversized = Connection::open(oversized_path).unwrap();
    let doc_length: i64 = oversized
        .query_row(
            "SELECT length(json_extract(doc, '$')) FROM data_doc",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    let expected_length = oversized_spec["rows"][0]["doc"]["repeat"]["count"]
        .as_u64()
        .unwrap() as i64;
    assert_eq!(doc_length, expected_length);
    let protocol_rows: i64 = count(&oversized, "protocol");
    assert_eq!(protocol_rows, 1);
    json!({
        "probes": Value::Object(probes),
        "tables": Value::Object(evidence),
        "oversized": {"doc_length":doc_length,"protocol_rows":protocol_rows},
    })
}

/// {"repeat":{"text":..,"count":..}} composes oversized literal values.
fn expand_repeat(value: &Value) -> Value {
    if let Some(repeat) = value.get("repeat") {
        let unit = repeat["text"].as_str().unwrap();
        let count = repeat["count"].as_u64().unwrap();
        return Value::String(unit.repeat(count as usize));
    }
    match value {
        Value::Array(items) => Value::Array(items.iter().map(expand_repeat).collect()),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, item)| (key.clone(), expand_repeat(item)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Declared encoded sizes exercise the batch accounting; facts stay small.
fn byte_budget(step: &Value) -> Value {
    let mut database =
        sqlite::Database::create(Path::new(step["database"].as_str().unwrap())).unwrap();
    let batch_bytes = sqlite::tests::BATCH_BYTES;
    let mut observed = Vec::new();
    for action in step["steps"].as_array().unwrap() {
        if action.get("flush").is_some() {
            sqlite::tests::flush_pending(&mut database).unwrap();
        } else {
            let fact: sqlite::writers::Fact =
                serde_json::from_value(action["fact"].clone()).unwrap();
            let bytes = match &action["bytes"] {
                Value::Number(bytes) => bytes.as_u64().unwrap() as usize,
                Value::Object(spec) => {
                    if let Some(plus) = spec.get("batch_bytes_plus") {
                        batch_bytes + plus.as_u64().unwrap() as usize
                    } else {
                        batch_bytes / 2
                            + spec.get("half_batch_bytes_plus").unwrap().as_u64().unwrap() as usize
                    }
                }
                other => panic!("unknown byte spec: {other}"),
            };
            database.insert_fact(fact, bytes).unwrap();
        }
        let (pending, pending_bytes) = sqlite::tests::accounting(&database);
        observed.push(json!({
            "pending":pending,
            "pending_bytes":pending_bytes,
            "stored":sqlite::tests::stored_rows(&database),
        }));
    }
    json!({"steps":observed})
}
