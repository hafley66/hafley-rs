//! Recursive graph questions return the edge rows that witness each answer.
//! The committed-fixture arms (call, type, flow, sqlite digest door) are
//! steps in `tests/fixtures/graph_path_cases/`; the slice's statement-text
//! extraction has no DSL verb, so it stays as the one library-level api step
//! below.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("graph_path_cases", |case| {
        crate::fixture_runner::commands(case, |step| {
            assert_eq!(
                step["api"], "slice_statements",
                "unknown api step: {step}"
            );
            slice_statements(step)
        })
    });
}

/// The control slice is a closed statement set: it names the branch and the
/// sink, never code after them.
fn slice_statements(step: &serde_json::Value) -> serde_json::Value {
    use std::process::Command;
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(step["file"].as_str().unwrap()),
    )
    .unwrap();
    let byte = text.find("allow()").unwrap() + 1;
    let seed = format!("{}:{byte}", step["arg"].as_str().unwrap());
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", "--slice", &seed])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(!rows.is_empty());
    assert!(
        rows.iter()
            .all(|row| row["record"] == "node" && row["family"] == "cfg"),
        "{rows:?}"
    );
    let selected: Vec<String> = rows
        .iter()
        .filter_map(|row| {
            let start = row["span"]["start"].as_u64()? as usize;
            let end = row["span"]["end"].as_u64()? as usize;
            text.get(start..end).map(str::to_string)
        })
        .collect();
    assert!(selected.iter().any(|text| text.contains("if (flag)")));
    assert!(selected.iter().any(|text| text.contains("allow()")));
    assert!(selected.iter().all(|text| !text.contains("after()")));
    serde_json::json!(selected)
}
