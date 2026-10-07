#![cfg(feature = "cli")]

use std::process::Command;

#[test]
fn generated_clap_help_matches_captured_main() {
    use clap::CommandFactory as _;
    let mut cases = vec![("top".to_owned(), vec!["--help".to_owned()])];
    cases.extend(ryi_proto::cli_auto::Ryi::command().get_subcommands().map(|sub| {
        (sub.get_name().to_owned(), vec![sub.get_name().to_owned(), "--help".to_owned()])
    }));
    cases.push(("bare".to_owned(), vec![]));
    let mut snapshot = String::new();
    for (name, args) in cases {
        let result = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
            .args(args).current_dir("tests/fixtures/rust")
            .env("RUST_LOG", "off").env_remove("RYI_STALE_CHECK")
            .output().unwrap();
        let normalize = |bytes: &[u8]| String::from_utf8_lossy(bytes)
            .replace(env!("CARGO_BIN_EXE_ryii"), "ryii")
            .split_inclusive('\n').map(|line| if line.starts_with("Build: git hash: ") { "Build: <stamp>\n" } else { line }).collect::<String>();
        snapshot.push_str(&format!("=== {name} (exit {}) ===\nstdout:\n{}stderr:\n{}", result.status.code().unwrap_or(-1), normalize(&result.stdout), normalize(&result.stderr)));
    }
    insta::assert_snapshot!("help", snapshot);
}

#[test]
fn generated_format_accepts_root_and_global_positions() {
    let file = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/type_ladder/src/_1_none.rs"
    );
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
            .args(args)
            .env("DL_TRAIL", "0")
            .output()
            .expect("ryi jsonl");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("UTF-8 JSONL")
    };
    let before = run(&["--format", "jsonl", "fast", file]);
    let after = run(&["fast", "--format", "jsonl", file]);
    let root = run(&["--format", "jsonl", file]);
    let rows = [
        ("before", before == after, before.lines().last()),
        ("root", root.lines().last().is_some(), root.lines().last()),
    ];
    assert_eq!(
        rows.map(|(name, condition, last)| format!("{name} {condition} {}", last.unwrap_or("")))
            .join("\n"),
        "before true {\"complete\":true,\"rows\":33}\nroot true {\"complete\":true,\"rows\":35}",
    );
}
