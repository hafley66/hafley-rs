use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::process::Command;

/// One `--resolve --arms call` run over the module-plane corpus (the original
/// nine crate_a files plus crate_b's lib.rs, in the original order). The whole
/// resolved_edge / resolved_import / unresolved tables, projected to the same
/// tuples the old helpers compared, sorted. Each original `contains`/absence/
/// count claim is one row in the case JSON, asserted before the snapshot.
pub fn evaluate(case: &Value) -> Value {
    let mut args: Vec<String> = vec!["--resolve".into(), "--arms".into(), "call".into()];
    for name in case["args"].as_array().expect("arg files") {
        args.push(format!(
            "{}/tests/fixtures/{}",
            env!("CARGO_MANIFEST_DIR"),
            name.as_str().expect("arg string")
        ));
    }
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(&args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut tables: BTreeMap<&str, Vec<Value>> = BTreeMap::from([
        ("edges", Vec::new()),
        ("imports", Vec::new()),
        ("unresolved", Vec::new()),
    ]);
    for line in String::from_utf8(output.stdout).expect("stdout is UTF-8").lines() {
        let row: Value = serde_json::from_str(line).expect("a flat fact is JSON");
        let stem = |path: &str| {
            path.rsplit('/').next().unwrap_or(path).trim_end_matches(".rs").to_string()
        };
        let projection = match row["record"].as_str().expect("record") {
            "resolved_edge" => json!([
                row["caller_name"], stem(row["callee_path"].as_str().unwrap_or("")),
                row["callee_name"], row["kind"],
            ]),
            "resolved_import" => json!([
                stem(row["src_path"].as_str().unwrap_or("")), row["local"], row["name"],
                stem(row["target_path"].as_str().unwrap_or("")), row["target_name"],
                row["kind"], row["hops"].as_u64().unwrap_or(0),
            ]),
            "unresolved" => json!([
                stem(row["path"].as_str().unwrap_or("")), row["reason"], row["detail"],
            ]),
            _ => continue,
        };
        match row["record"].as_str().expect("record") {
            "resolved_edge" => tables.get_mut("edges").unwrap().push(projection),
            "resolved_import" => tables.get_mut("imports").unwrap().push(projection),
            _ => tables.get_mut("unresolved").unwrap().push(projection),
        }
    }
    for table in tables.values_mut() {
        table.sort_by_key(|row| serde_json::to_string(row).expect("row serializes"));
    }
    for claim in case["claims"].as_array().expect("claims rows") {
        let table = &tables[claim["table"].as_str().expect("claim table")];
        if let Some(row) = claim.get("present") {
            assert!(table.contains(row), "missing {row} in {claim}: {table:?}");
        }
        if let Some(caller) = claim["absent_caller"].as_str() {
            assert!(
                table.iter().all(|row| row[0].as_str() != Some(caller)),
                "{claim}: {table:?}"
            );
        }
        if let Some(local) = claim["absent_local"].as_str() {
            assert!(
                table.iter().all(|row| row[1].as_str() != Some(local)),
                "{claim}: {table:?}"
            );
        }
        if let Some(expected) = claim.get("counts") {
            let modules = table.iter().filter(|row| row[5] == "module").count();
            assert_eq!(
                (table.len() - modules, modules),
                (
                    expected["non_module"].as_u64().expect("non_module") as usize,
                    expected["module"].as_u64().expect("module") as usize
                ),
                "{claim}: {table:?}"
            );
        }
    }
    Value::Object(tables.into_iter().map(|(name, rows)| (name.to_string(), Value::Array(rows))).collect())
}
