use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::process::Command;

/// One `--resolve` run per case file; the whole `(caller, callee)` pair table
/// per file, sorted and deduped. Each old per-test `has()` claim is one
/// `claims` row, asserted before the snapshot freezes the tables.
pub fn evaluate(case: &Value) -> Value {
    let mut tables: BTreeMap<String, Value> = BTreeMap::new();
    for file in case["files"].as_array().expect("case files") {
        let name = file.as_str().expect("file name");
        let path = format!("{}/tests/fixtures/py_call_grind/{name}", env!("CARGO_MANIFEST_DIR"));
        let out = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .arg("--resolve")
            .args([&path, &path])
            .output()
            .expect("extract binary runs");
        assert!(
            out.status.success(),
            "{name} resolve failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let mut pairs: Vec<(String, String)> = Vec::new();
        for line in String::from_utf8(out.stdout).expect("utf8 wire").lines() {
            let Ok(fact) = serde_json::from_str::<Value>(line) else { continue };
            if fact["record"] != "resolved_edge" {
                continue;
            }
            pairs.push((
                fact["caller_name"].as_str().unwrap_or("").to_string(),
                fact["callee_name"].as_str().unwrap_or("").to_string(),
            ));
        }
        pairs.sort();
        pairs.dedup();
        let rows: Vec<Value> = pairs
            .into_iter()
            .map(|(caller, callee)| json!({"caller": caller, "callee": callee}))
            .collect();
        let stem = name.trim_end_matches(".py").to_string();
        for claim in case["claims"].as_array().expect("claims rows") {
            if claim["file"].as_str() != Some(stem.as_str()) {
                continue;
            }
            let caller = claim["caller"].as_str().expect("claim caller");
            let callee = claim["callee"].as_str().expect("claim callee");
            let hit = rows
                .iter()
                .any(|row| row["caller"] == *caller && row["callee"] == *callee);
            assert_eq!(hit, claim["present"].as_bool().unwrap_or(true), "{claim}: {rows:?}");
        }
        tables.insert(stem, Value::Array(rows));
    }
    Value::Object(tables.into_iter().collect())
}
