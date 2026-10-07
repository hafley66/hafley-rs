use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::process::Command;

/// One `--resolve` run over the residual fixture dir; the whole
/// `(caller -> [(callee, path, kind)])` table, paths relative to the fixture
/// root. Each old per-test `one_edge` claim is one `claims` row in the case
/// JSON, asserted before the snapshot freezes the table.
pub fn evaluate(case: &Value) -> Value {
    let root = format!("{}/tests/fixtures/go_residual", env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    let out = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("--resolve")
        .args(&files)
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut edges: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for line in String::from_utf8(out.stdout).expect("utf8 wire").lines() {
        let Ok(row) = serde_json::from_str::<Value>(line) else { continue };
        if row["record"] != "resolved_edge" {
            continue;
        }
        let path = row["callee_path"].as_str().unwrap_or("");
        let path = path.strip_prefix(&root).unwrap_or(path).trim_start_matches('/');
        edges
            .entry(row["caller_name"].as_str().unwrap_or("").to_string())
            .or_default()
            .push(json!({
                "callee": row["callee_name"].as_str().unwrap_or(""),
                "path": path,
                "kind": row["kind"].as_str().unwrap_or(""),
            }));
    }
    for claim in case["claims"].as_array().expect("claims rows") {
        let caller = claim["caller"].as_str().expect("claim caller");
        let callee = claim["callee"].as_str().expect("claim callee");
        let hits: Vec<&Value> = edges
            .get(caller)
            .map(|rows| rows.iter().filter(|hit| hit["callee"].as_str() == Some(callee)).collect())
            .unwrap_or_default();
        assert_eq!(
            hits.len(),
            claim["count"].as_u64().expect("claim count") as usize,
            "{claim}: {hits:?}"
        );
        if let Some(file) = claim["file"].as_str() {
            assert!(
                hits.iter().all(|hit| hit["path"].as_str().unwrap().ends_with(file)),
                "bound in {file}: {hits:?}"
            );
        }
    }
    Value::Object(edges.into_iter().map(|(caller, rows)| (caller, Value::Array(rows))).collect())
}

fn walk(dir: &str, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("fixture dir readable") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            walk(&path.to_string_lossy(), out);
        } else if path.extension().is_some_and(|ext| ext == "go") {
            out.push(path.to_string_lossy().into_owned());
        }
    }
}
