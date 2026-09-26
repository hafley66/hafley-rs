#![cfg(feature = "cli")]

use std::process::Command;

#[test]
fn generated_clap_help_matches_captured_main() {
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ryi_help");
    for verb in [
        "root", "fast", "slow", "scip", "graph", "cleave", "move", "rename",
        "query", "region", "watch", "diff", "ingest", "schema", "trail",
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ryi"));
        if verb != "root" {
            command.arg(verb);
        }
        let output = command.arg("--help").output().expect("ryi help");
        assert!(output.status.success(), "{verb}: {}", String::from_utf8_lossy(&output.stderr));
        // The build stamp comes from whichever build last wrote the binary; a
        // shared cargo target can swap it mid-run. Every other byte is pinned.
        let stamp = |text: &str| -> String {
            text.split_inclusive('\n')
                .map(|line| if line.starts_with("Build: git hash: ") { "Build: <stamp>\n" } else { line })
                .collect()
        };
        let actual = stamp(&String::from_utf8(output.stdout).expect("UTF-8 help"));
        let expected = stamp(&std::fs::read_to_string(format!("{fixtures}/{verb}.txt")).expect("captured main help"));
        if verb == "root" {
            assert!(actual.contains("Build: <stamp>\n"), "root help carries a build line");
        }
        assert_eq!(actual.as_bytes(), expected.as_bytes(), "{verb} help bytes");
    }
}

#[test]
fn generated_format_accepts_root_and_global_positions() {
    let file = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/type_ladder/src/_1_none.rs");
    let run = |args: &[&str]| {
        let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
            .args(args)
            .env("DL_TRAIL", "0")
            .output()
            .expect("ryi jsonl");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
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
        rows.map(|(name, condition, last)| format!("{name} {condition} {}", last.unwrap_or(""))).join("\n"),
        "before true {\"complete\":true,\"rows\":31}\nroot true {\"complete\":true,\"rows\":35}",
    );
}
