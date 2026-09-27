//! `mod live;` must make a Rust file edge to the declared module file.

use serde_json::Value;
use std::process::Command;

const FIXTURE: &str = "tests/fixtures/rust_module_reachability";

#[test]
fn file_edges_include_declared_rust_modules() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--deps", "--root", FIXTURE, FIXTURE])
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("each fact is JSON"))
        .collect();
    assert!(
        rows.iter().any(|row| row["record"] == "file_edge"
            && row["src_path"] == "src/lib.rs"
            && row["dst_path"] == "src/live.rs"
            && row["kind"] == "module"),
        "expected src/lib.rs -> src/live.rs, got {rows:?}"
    );
}
