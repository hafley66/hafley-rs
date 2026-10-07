use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::process::Command;

/// One ryii run per case command; the whole projected table per command
/// (`edge_triples` or the filtered flat-fact rows). Each old per-test assert
/// is one `claims` row in the case JSON, asserted before the snapshot freezes
/// the tables.
pub fn evaluate(case: &Value) -> Value {
    let mut tables: BTreeMap<String, Value> = BTreeMap::new();
    for command in case["commands"].as_array().expect("case commands") {
        let name = command["name"].as_str().expect("command name").to_string();
        let args: Vec<String> = command["args"]
            .as_array()
            .expect("command args")
            .iter()
            .map(|arg| arg.as_str().expect("arg string").to_string())
            .collect();
        let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(&args)
            .output()
            .expect("extract binary runs");
        assert!(
            output.status.success(),
            "{name} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let facts: Vec<Value> = String::from_utf8(output.stdout)
            .expect("stdout is UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("a flat fact is JSON"))
            .collect();
        let text = |row: &Value, key: &str| row[key].as_str().unwrap_or("").to_string();
        let table = if command["mode"] == "edge_triples" {
            let mut triples: Vec<(String, String, String)> = facts
                .iter()
                .filter(|row| row["record"] == "resolved_edge")
                .map(|row| (text(row, "caller_name"), text(row, "callee_name"), text(row, "kind")))
                .collect();
            triples.sort();
            json!(triples)
        } else {
            let record = command["record"].as_str().expect("record filter");
            let wanted = command["name_equals"].as_str();
            let rows: Vec<Value> = facts
                .iter()
                .filter(|row| row["record"] == record && wanted.is_none_or(|name| row["name"] == name))
                .map(|row| scrub(row))
                .collect();
            json!(rows)
        };
        for claim in case["claims"].as_array().expect("claims rows") {
            if claim["command"].as_str() != Some(name.as_str()) {
                continue;
            }
            check(&claim, &table);
        }
        tables.insert(name, table);
    }
    Value::Object(tables.into_iter().collect())
}

fn check(claim: &Value, table: &Value) {
    let kind = claim["kind"].as_str().expect("claim kind");
    match kind {
        "edges" => {
            let pairs: Vec<Vec<&str>> = table
                .as_array().unwrap()
                .iter()
                .map(|row| vec![row[0].as_str().unwrap(), row[1].as_str().unwrap()])
                .collect();
            let expect: Vec<Vec<&str>> = claim["expect"]
                .as_array().unwrap()
                .iter()
                .map(|pair| {
                    vec![pair[0].as_str().unwrap(), pair[1].as_str().unwrap()]
                })
                .collect();
            assert_eq!(pairs, expect, "{claim}");
        }
        "module_node" => {
            let rows = table.as_array().unwrap();
            assert_eq!(rows.len(), 1, "{claim}");
            assert_eq!(rows[0]["kind"], "module", "{claim}");
            assert_eq!(rows[0]["span"]["start"], 0, "{claim}");
            let bytes = std::fs::read(
                format!("{}/{}", env!("CARGO_MANIFEST_DIR"), claim["file"].as_str().unwrap()),
            )
            .expect("fixture readable")
            .len();
            assert_eq!(rows[0]["span"]["end"], bytes, "{claim}");
        }
        "no_module" => assert!(table.as_array().unwrap().is_empty(), "{claim}"),
        "site_paths" => {
            let paths: Vec<&str> = table
                .as_array().unwrap()
                .iter()
                .map(|row| row["callee_path"].as_str().unwrap())
                .collect();
            let expect: Vec<&str> = claim["expect"].as_array().unwrap()
                .iter().map(|value| value.as_str().unwrap()).collect();
            assert_eq!(paths, expect, "{claim}");
        }
        "refs" => {
            let refs: Vec<Vec<&str>> = table
                .as_array().unwrap()
                .iter()
                .map(|row| vec![row["functor"].as_str().unwrap(), row["position"].as_str().unwrap()])
                .collect();
            let expect: Vec<Vec<&str>> = claim["expect"].as_array().unwrap()
                .iter()
                .map(|pair| vec![pair[0].as_str().unwrap(), pair[1].as_str().unwrap()])
                .collect();
            assert_eq!(refs, expect, "{claim}");
        }
        "no_refs" => assert!(table.as_array().unwrap().is_empty(), "{claim}"),
        other => panic!("unknown claim kind: {other}"),
    }
}

/// Absolute manifest paths in flat facts become `$manifest/...`.
fn scrub(row: &Value) -> Value {
    match row {
        Value::String(text) => {
            Value::String(text.replace(env!("CARGO_MANIFEST_DIR"), "$manifest"))
        }
        Value::Array(items) => Value::Array(items.iter().map(scrub).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), scrub(value)))
                .collect(),
        ),
        other => other.clone(),
    }
}
