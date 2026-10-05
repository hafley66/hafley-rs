//! `ryi graph --slow` verifies target sites with the language checker,
//! and a checker-answered edge grades `+`.
#![cfg(feature = "cli")]

use std::process::Command;

use serde_json::Value;

#[test]
fn slow_callers_answer_from_the_index_and_grade_plus() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "graph",
            "--slow",
            "--callers",
            "fetch_ref",
            "--root",
            "tests/fixtures/ratchet_soopy",
            "--scip-index",
            "tests/fixtures/ratchet_soopy/index.scip",
            "tests/fixtures/ratchet_soopy/src",
        ])
        .env("RUST_LOG", "off")
        .output()
        .expect("graph binary runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows,
        serde_json::from_str::<Vec<Value>>(
            r#"[{"record":"graph_edge","from_path":"tests/fixtures/ratchet_soopy/src/_13_fetch.rs","from_name":"execute_one","to_path":"tests/fixtures/ratchet_soopy/src/_13_fetch.rs","to_name":"fetch_ref","kind":"call","grade":"+","from_line":69,"to_line":76}]"#,
        )
        .unwrap()
    );
}

#[test]
fn a_zero_second_timeout_is_refused() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "graph",
            "--timeout",
            "0",
            "--callers",
            "x",
            "tests/fixtures/graph_ts",
        ])
        .output()
        .expect("graph binary runs");
    assert_eq!(output.status.code(), Some(2));
}

#[cfg(not(feature = "ts-checker"))]
#[test]
fn slow_typescript_requires_the_checker_feature_before_index_discovery() {
    for query in [
        vec!["--from", "chainC"],
        vec!["--call-path", "chainC"],
        vec!["--callers", "chainD"],
        vec!["--uses", "Widget"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("RUST_LOG", "off")
            .args(["graph", "--slow"])
            .args(query)
            .arg("tests/fixtures/graph_ts")
            .output()
            .expect("graph binary runs");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("ts-checker"));
        assert!(output.stdout.is_empty());
    }
}

/// A checker that declines (no loadable TypeScript) leaves the slow walk no
/// oracle: the run fails naming the tier, it does not answer zero edges.
#[cfg(feature = "ts-checker")]
#[test]
fn slow_walk_fails_when_the_typescript_checker_declines() {
    let scratch = tempfile::tempdir().unwrap();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/graph_ts");
    for file in ["chain_c.ts", "chain_d.ts"] {
        std::fs::copy(fixture.join(file), scratch.path().join(file)).unwrap();
    }
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(scratch.path())
        .env("RUST_LOG", "off")
        .env(
            "SPREFA_TSGO",
            scratch.path().join("absent/tsgo"),
        )
        .args(["graph", "--slow", "--from", "chainC", "--root", ".", "."])
        .output()
        .expect("graph binary runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("ryi slow: tier.tsgo declined"), "{stderr}");
    assert!(output.stdout.is_empty());
}

/// A Rust-only tree whose `include_str!` data file owns no module: the rust
/// tier answers the walk, the file is one per-file decline line on stderr,
/// and the absent ts tier is never asked, so it never declines.
#[cfg(feature = "rust-checker")]
#[test]
fn slow_walk_answers_a_rust_tree_and_reports_the_unowned_file() {
    let scratch = tempfile::tempdir().unwrap();
    let src = scratch.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(
        scratch.path().join("Cargo.toml"),
        "[package]\nname = \"shimmed\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(
        src.join("lib.rs"),
        "mod helper;\n\npub const SHIM: &str = include_str!(\"shim.rs\");\n\npub fn build() -> usize {\n    helper::assemble()\n}\n",
    )
    .unwrap();
    std::fs::write(
        src.join("helper.rs"),
        "pub fn assemble() -> usize {\n    1\n}\n",
    )
    .unwrap();
    std::fs::write(
        src.join("shim.rs"),
        "pub fn shimmed() -> usize {\n    2\n}\n",
    )
    .unwrap();
    // The demand walk reads only the seed files, so only an unowned seed declines.
    let graph = |anchor: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .current_dir(scratch.path())
            .env("RUST_LOG", "off")
            .args(["graph", "--slow", "--from", anchor, "--root", ".", "src"])
            .output()
            .expect("graph binary runs");
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(output.status.success(), "{stderr}");
        let rows: Vec<Value> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        (rows, stderr)
    };
    let (rows, stderr) = graph("lib.rs#build");
    assert_eq!(
        rows,
        serde_json::from_str::<Vec<Value>>(
            r#"[{"record":"graph_node","path":"src/helper.rs","name":"assemble","depth":1,"grade":"+","line":1}]"#,
        )
        .unwrap()
    );
    assert_eq!(
        stderr,
        "demand walk: 2 bodies inferred, 1 call, 0 method, 0 passed, 0 trait_impl, 0 extern edges\n1 edges: 1 +, 0 ~, 0 -\n"
    );
    let (rows, stderr) = graph("shim.rs#shimmed");
    assert_eq!(rows, Vec::<Value>::new());
    assert_eq!(
        stderr,
        "ryi slow: tier.rust-analyzer declined src/shim.rs: owns no module in the loaded crate graph (cfg-gated, or outside every crate root)\n\
         demand walk: 0 bodies inferred, 0 call, 0 method, 0 passed, 0 trait_impl, 0 extern edges\n0 edges: 0 +, 0 ~, 0 -\n"
    );
}

#[cfg(feature = "rust-checker")]
#[test]
fn targeted_checker_rows_match_the_whole_checker_at_unresolved_sites() {
    let root = "tests/fixtures/rust_checker_wiring";
    let binary = env!("CARGO_BIN_EXE_ryii");
    let whole = Command::new(binary)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("RUST_LOG", "off")
        .args([
            "--resolve",
            "--arms",
            "call",
            "--root",
            root,
            "--rust-checker",
            "tests/fixtures/rust_checker_wiring/src",
        ])
        .output()
        .expect("whole checker runs");
    assert!(
        whole.status.success(),
        "{}",
        String::from_utf8_lossy(&whole.stderr)
    );
    let mut expected: Vec<Value> = String::from_utf8(whole.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| row["record"] == "resolved_edge" && row["callee_name"] == "render")
        .collect();
    expected.sort_by_key(Value::to_string);

    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("graph.db");
    let targeted = Command::new(binary)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("RUST_LOG", "off")
        .args([
            "graph",
            "--slow",
            "--callers",
            "render",
            "--root",
            root,
            "--sqlite",
        ])
        .arg(&database)
        .arg("tests/fixtures/rust_checker_wiring/src")
        .output()
        .expect("targeted graph runs");
    assert!(
        targeted.status.success(),
        "{}",
        String::from_utf8_lossy(&targeted.stderr)
    );
    let connection = rusqlite::Connection::open(database).unwrap();
    let mut statement = connection
        .prepare(
            "SELECT caller_path, caller_name, callee_path, callee_name,
         caller_site_start, caller_site_end, callee_start, callee_end,
         kind, resolution_origin FROM resolved_edge WHERE callee_name = 'render'",
        )
        .unwrap();
    let mut actual: Vec<Value> = statement
        .query_map([], |row| {
            Ok(serde_json::json!({
                "record": "resolved_edge",
                "caller_path": row.get::<_, String>(0)?,
                "caller_name": row.get::<_, Option<String>>(1)?,
                "callee_path": row.get::<_, String>(2)?,
                "callee_name": row.get::<_, Option<String>>(3)?,
                "caller_site_start": row.get::<_, u32>(4)?,
                "caller_site_end": row.get::<_, u32>(5)?,
                "callee_start": row.get::<_, u32>(6)?,
                "callee_end": row.get::<_, u32>(7)?,
                "kind": row.get::<_, String>(8)?,
                "resolution_origin": row.get::<_, String>(9)?,
            }))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    actual.sort_by_key(Value::to_string);
    assert_eq!(actual, expected);
}

#[test]
#[cfg(feature = "ts-checker")]
fn typescript_target_references_include_importing_files() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("RUST_LOG", "off")
        .args([
            "graph",
            "--slow",
            "--callers",
            "chainD",
            "--root",
            "tests/fixtures/graph_ts",
            "tests/fixtures/graph_ts",
        ])
        .output()
        .expect("TypeScript graph runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows,
        [serde_json::json!({
            "record": "graph_edge",
            "from_path": "tests/fixtures/graph_ts/chain_c.ts",
            "from_name": "chainC",
            "to_path": "tests/fixtures/graph_ts/chain_d.ts",
            "to_name": "chainD",
            "kind": "checker_resolve",
            "grade": "+",
            "from_line": 4,
            "to_line": 2
        })]
    );
}

#[cfg(feature = "rust-checker")]
#[test]
fn targeted_callers_include_aliased_and_macro_calls_and_skip_decoys() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("RUST_LOG", "off")
        .args([
            "graph",
            "--slow",
            "--callers",
            "render",
            "--root",
            "tests/fixtures/rust_checker_usages",
            "tests/fixtures/rust_checker_usages/src",
        ])
        .output()
        .expect("targeted graph runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut edges: Vec<String> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|row| row["record"] == "graph_edge")
        .map(|row| {
            format!(
                "{} -> {}:{}",
                row["from_name"].as_str().unwrap(),
                row["to_path"].as_str().unwrap().rsplit('/').next().unwrap(),
                row["to_name"].as_str().unwrap()
            )
        })
        .collect();
    edges.sort();
    edges.dedup();
    // `expanded` is absent: its call is written in the `macro_rules!` body, which
    // rust-analyzer's reference search does not map (a rust-analyzer gap, not ryi's).
    assert_eq!(
        edges,
        [
            "aliased -> widget.rs:render",
            "decoyed -> decoy.rs:render",
            "plain -> widget.rs:render",
        ]
    );
}

#[test]
#[cfg(feature = "ts-checker")]
fn calls_answered_by_overload_signatures_land_on_the_implementation() {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("RUST_LOG", "off")
        .args([
            "graph",
            "--slow",
            "--callers",
            "pick",
            "--root",
            "tests/fixtures/graph_ts_overload",
            "tests/fixtures/graph_ts_overload",
        ])
        .output()
        .expect("TypeScript graph runs");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows,
        [
            serde_json::json!({
                "record": "graph_edge",
                "from_path": "tests/fixtures/graph_ts_overload/consumer/use_dist.ts",
                "from_name": "useDist",
                "to_path": "tests/fixtures/graph_ts_overload/pick.ts",
                "to_name": "pick",
                "kind": "checker_resolve",
                "grade": "+",
                "from_line": 4,
                "to_line": 4
            }),
            serde_json::json!({
                "record": "graph_edge",
                "from_path": "tests/fixtures/graph_ts_overload/pick.ts",
                "from_name": "pickText",
                "to_path": "tests/fixtures/graph_ts_overload/pick.ts",
                "to_name": "pick",
                "kind": "checker_resolve",
                "grade": "+",
                "from_line": 9,
                "to_line": 4
            }),
            serde_json::json!({
                "record": "graph_edge",
                "from_path": "tests/fixtures/graph_ts_overload/use_pick.ts",
                "from_name": "usePick",
                "to_path": "tests/fixtures/graph_ts_overload/pick.ts",
                "to_name": "pick",
                "kind": "checker_resolve",
                "grade": "+",
                "from_line": 4,
                "to_line": 4
            }),
        ]
    );
}
