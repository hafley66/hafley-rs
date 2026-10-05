#![cfg(feature = "cli")]

use std::process::Command;

#[test]
fn bounded_rust_callers_keep_same_file_path_modules_globs_and_self() {
    let root = "tests/fixtures/dogfood_callers";
    let files = ["src/0_calls.rs", "src/1_child.rs"];
    let mut outputs = Vec::new();
    for (file, name) in [(files[0], "objects"), (files[0], "method")] {
        let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["graph", "--callers", &format!("{root}/{file}#{name}"), "--root", root])
            .args(files.map(|file| format!("{root}/{file}")))
            .output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        outputs.push(String::from_utf8(output.stdout).unwrap());
    }
    insta::assert_snapshot!(outputs.join("\n"), @r#"
    {"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","from_name":"local","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"objects","kind":"name_resolve","grade":"~","from_line":2,"to_line":1}
    {"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/1_child.rs","from_name":"child","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"objects","kind":"import_resolve","grade":"+","from_line":2,"to_line":1}

    {"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","from_name":"run","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"method","kind":"name_resolve","grade":"~","from_line":6,"to_line":5}
    "#);
}
