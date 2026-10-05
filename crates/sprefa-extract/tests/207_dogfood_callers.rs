#![cfg(feature = "cli")]
use std::process::Command;

#[test]
fn bounded_rust_callers_keep_module_routes_or_emit_a_cargo_abstain() {
    let mut results = Vec::new();
    let cases: &[(&str, &str, &[&str], bool)] = &[
        (
            "dogfood_callers",
            "src/0_calls.rs#objects",
            &["src/0_calls.rs", "src/1_child.rs", "src/2_importer.rs"],
            false,
        ),
        (
            "dogfood_callers",
            "src/0_calls.rs#method",
            &["src/0_calls.rs", "src/1_child.rs", "src/2_importer.rs"],
            false,
        ),
        (
            "dogfood_callers",
            "src/1_child.rs#child",
            &["src/0_calls.rs", "src/1_child.rs", "src/2_importer.rs"],
            false,
        ),
        (
            "dogfood_inline",
            "tests/0_capture.rs#capture",
            &["tests/all.rs", "tests/0_capture.rs"],
            false,
        ),
        (
            "dogfood_invalid_manifest",
            "src/lib.rs#objects",
            &["src/lib.rs"],
            false,
        ),
        (
            "dogfood_callers",
            "src/0_calls.rs#objects",
            &["src/0_calls.rs", "src/1_child.rs", "src/2_importer.rs"],
            true,
        ),
        (
            "dogfood_callers",
            "src/0_calls.rs#method",
            &["src/0_calls.rs", "src/1_child.rs", "src/2_importer.rs"],
            true,
        ),
        (
            "dogfood_inline",
            "tests/0_capture.rs#capture",
            &["tests/all.rs", "tests/0_capture.rs"],
            true,
        ),
        ("dogfood_callers", "src/0_calls.rs#method", &["src/0_calls.rs", "src/3_unknown.rs"], false),
    ];
    for (case, anchor, files, slow) in cases {
        let root = format!("tests/fixtures/{case}");
        let mut command = Command::new(env!("CARGO_BIN_EXE_ryii"));
        command
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args([
                "graph",
                "--callers",
                &format!("{root}/{anchor}"),
                "--root",
                &root,
            ])
            .args(files.iter().map(|file| format!("{root}/{file}")))
            .env("KACHE_DISABLED", "1")
            .env("RUST_LOG", "off");
        if *slow {
            command.arg("--slow");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap().replace(
            &std::fs::canonicalize(env!("CARGO_MANIFEST_DIR"))
                .unwrap()
                .display()
                .to_string(),
            "$CRATE",
        );
        if files.contains(&"src/3_unknown.rs") { assert!(stdout.contains("\"reason\":\"inferred\""), "{stdout}"); }
        results.push(format!("{case}/{anchor} slow={slow}:\n{stdout}"));
    }
    insta::assert_snapshot!(results.join("\n"), @r#"
dogfood_callers/src/0_calls.rs#objects slow=false:
{"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","from_name":"local","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"objects","kind":"name_resolve","grade":"~","from_line":2,"to_line":1}
{"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/1_child.rs","from_name":"child","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"objects","kind":"import_resolve","grade":"+","from_line":2,"to_line":1}
{"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/2_importer.rs","from_name":"indirect","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"objects","kind":"import_resolve","grade":"+","from_line":2,"to_line":1}

dogfood_callers/src/0_calls.rs#method slow=false:
{"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","from_name":"run","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"method","kind":"name_resolve","grade":"~","from_line":6,"to_line":5}

dogfood_callers/src/1_child.rs#child slow=false:
{"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","from_name":"nested","to_path":"tests/fixtures/dogfood_callers/src/1_child.rs","to_name":"child","kind":"name_resolve","grade":"+","from_line":10,"to_line":2}

dogfood_inline/tests/0_capture.rs#capture slow=false:
{"record":"graph_edge","from_path":"tests/fixtures/dogfood_inline/tests/all.rs","from_name":"caller","to_path":"tests/fixtures/dogfood_inline/tests/0_capture.rs","to_name":"capture","kind":"name_resolve","grade":"+","from_line":6,"to_line":1}

dogfood_invalid_manifest/src/lib.rs#objects slow=false:
{"record":"file_unresolved","src_path":"tests/fixtures/dogfood_invalid_manifest/src/lib.rs","module":"$CRATE/tests/fixtures/dogfood_invalid_manifest/Cargo.toml","reason":"`cargo metadata` exited with an error: error: unclosed table, expected `]`\n --> tests/fixtures/dogfood_invalid_manifest/Cargo.toml:1:9\n  |\n1 | [package\n  |         ^\n"}

dogfood_callers/src/0_calls.rs#objects slow=true:
{"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","from_name":"local","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"objects","kind":"checker_resolve","grade":"+","from_line":2,"to_line":1}
{"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/1_child.rs","from_name":"child","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"objects","kind":"checker_resolve","grade":"+","from_line":2,"to_line":1}
{"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/2_importer.rs","from_name":"indirect","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"objects","kind":"checker_resolve","grade":"+","from_line":2,"to_line":1}

dogfood_callers/src/0_calls.rs#method slow=true:
{"record":"graph_edge","from_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","from_name":"run","to_path":"tests/fixtures/dogfood_callers/src/0_calls.rs","to_name":"method","kind":"checker_resolve","grade":"+","from_line":6,"to_line":5}

dogfood_inline/tests/0_capture.rs#capture slow=true:
{"record":"graph_edge","from_path":"tests/fixtures/dogfood_inline/tests/all.rs","from_name":"caller","to_path":"tests/fixtures/dogfood_inline/tests/0_capture.rs","to_name":"capture","kind":"checker_resolve","grade":"+","from_line":6,"to_line":1}
"#);
}
