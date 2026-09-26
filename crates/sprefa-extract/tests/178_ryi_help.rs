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
        let actual = String::from_utf8(output.stdout).expect("UTF-8 help");
        let mut expected = std::fs::read_to_string(format!("{fixtures}/{verb}.txt")).expect("captured main help");
        if verb == "root" {
            let old = expected.lines().find(|line| line.starts_with("Build: ")).expect("main build line");
            let current = concat!(
                "Build: git hash: ", env!("SPREFA_BUILD_GIT_HASH"),
                ", datetime: ", env!("SPREFA_BUILD_DATETIME")
            );
            expected = expected.replace(old, current);
        }
        assert_eq!(actual.as_bytes(), expected.as_bytes(), "{verb} help bytes");
    }
}
