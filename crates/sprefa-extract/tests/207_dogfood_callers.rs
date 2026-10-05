#![cfg(feature = "cli")]

use std::process::Command;

#[test]
fn bounded_rust_callers_keep_same_file_path_modules_globs_and_self() {
    let root = "tests/fixtures/dogfood_callers";
    let files = ["src/0_calls.rs", "src/1_child.rs", "src/2_importer.rs"];
    let mut outputs = Vec::new();
    for (file, name) in [(files[0], "objects"), (files[0], "method"), (files[1], "child")] {
        let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["graph", "--callers", &format!("{root}/{file}#{name}"), "--root", root])
            .args(files.map(|file| format!("{root}/{file}")))
            .output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        outputs.push(String::from_utf8(output.stdout).unwrap());
    }
    let root = "tests/fixtures/dogfood_inline";
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", "--callers", &format!("{root}/tests/0_capture.rs#capture"), "--root", root])
        .args([format!("{root}/tests/all.rs"), format!("{root}/tests/0_capture.rs")])
        .env("KACHE_DISABLED", "1").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    outputs.push(String::from_utf8(output.stdout).unwrap());
    let root = "tests/fixtures/dogfood_invalid_manifest";
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["graph", "--callers", "objects", "--root", root])
        .arg(format!("{root}/src/lib.rs"))
        .env("KACHE_DISABLED", "1").output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let stdout = String::from_utf8(output.stdout).unwrap().replace(
        &std::fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap().display().to_string(),
        "$CRATE",
    );
    outputs.push(stdout);
    insta::assert_snapshot!(outputs.join("\n"), @r#"
    {"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","from_name":"local","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"objects","kind":"name_resolve","grade":"~","from_line":2,"to_line":1}
    {"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/1_child.rs","from_name":"child","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"objects","kind":"import_resolve","grade":"+","from_line":2,"to_line":1}
    {"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/2_importer.rs","from_name":"indirect","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"objects","kind":"import_resolve","grade":"+","from_line":2,"to_line":1}

    {"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","from_name":"run","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"method","kind":"name_resolve","grade":"~","from_line":6,"to_line":5}

    {"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","from_name":"nested","to_path":"tests/fixtures/dogfood_callers/src/1_child.rs","to_name":"child","kind":"name_resolve","grade":"+","from_line":10,"to_line":2}

    {"record":"graph_edge","from_path":"tests/fixtures/dogfood_inline/tests/all.rs","from_name":"caller","to_path":"tests/fixtures/dogfood_inline/tests/0_capture.rs","to_name":"capture","kind":"name_resolve","grade":"+","from_line":6,"to_line":1}

    {"record":"file_unresolved","src_path":"tests/fixtures/dogfood_invalid_manifest/src/lib.rs","module":"$CRATE/tests/fixtures/dogfood_invalid_manifest/Cargo.toml","reason":"`cargo metadata` exited with an error: error: unclosed table, expected `]`\n --> tests/fixtures/dogfood_invalid_manifest/Cargo.toml:1:9\n  |\n1 | [package\n  |         ^\n"}
    
    "#);
}
