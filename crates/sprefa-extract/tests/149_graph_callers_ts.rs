use std::path::PathBuf;
use std::process::Command;

fn trace_path() -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "sprefa_graph_callers_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    directory.join("graph-callers.json")
}

#[test]
fn callers_rows_are_graded_and_sorted() {
    let fixture = PathBuf::from("tests/fixtures/ts5_findings/module_plane");
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["graph", "--callers", "deep"])
        .arg(fixture)
        .env("HAFLEY_TRACE", trace_path())
        .env("RUST_LOG", "sprefa_extract=debug")
        .output()
        .expect("graph binary runs");
    assert!(
        output.status.success(),
        "graph failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let rows: Vec<serde_json::Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 1);
    assert!(rows.iter().all(|row| {
        row.get("grade")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|grade| !grade.is_empty())
    }));
    assert_eq!(
        rows,
        serde_json::from_str::<Vec<serde_json::Value>>(
            r#"[{"from_line":4,"from_name":"reach","from_path":"tests/fixtures/ts5_findings/module_plane/two_hop_consumer.ts","grade":"+","kind":"import_resolve","record":"graph_edge","to_line":2,"to_name":"deep","to_path":"tests/fixtures/ts5_findings/module_plane/two_hop_inner.ts"}]"#,
        )
        .unwrap()
    );
}
