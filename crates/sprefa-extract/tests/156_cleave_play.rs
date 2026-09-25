//! `demo/cleave/play.sh` replayed: four successive `ryi cleave --commit` moves
//! over the six-file app in `demo/cleave/src`, each committed, then a checker
//! run over the result.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The `play: N cleave steps, M commits, tree at PATH` line the script closes
/// with, split into its three fields.
fn receipt(stdout: &str) -> (usize, usize, PathBuf) {
    let line = stdout
        .lines()
        .find(|line| line.starts_with("play: "))
        .unwrap_or_else(|| panic!("no play receipt in:\n{stdout}"));
    let fields: Vec<&str> = line.split_whitespace().collect();
    let steps = fields[1].parse().expect("a step count");
    let commits = fields[4].parse().expect("a commit count");
    let tree = PathBuf::from(fields.last().expect("a tree path"));
    (steps, commits, tree)
}

#[test]
fn the_demo_plays_four_cleaves_and_ends_green() {
    let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("demo/cleave");
    let output = Command::new("bash")
        .arg(demo.join("play.sh"))
        .current_dir(&demo)
        .env("RYI", env!("CARGO_BIN_EXE_ryi"))
        .env("RUST_LOG", "sprefa_extract=debug,hafley_scm=debug")
        .output()
        .expect("play.sh runs");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "play.sh exited {:?}:\n{stdout}\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let (steps, commits, tree) = receipt(&stdout);
    assert_eq!(steps, 4, "four cleave steps");
    assert_eq!(commits, 5, "the fixture commit plus one per step");
    for step in 1..=steps {
        assert!(
            stdout.contains(&format!("== step {step}:")),
            "step {step} left no header:\n{stdout}"
        );
    }
    assert!(
        stdout.contains("check tsc --noEmit -> 0 errors")
            || stdout.contains("check ryi fast (tsc is not installed) ->"),
        "no checker receipt:\n{stdout}"
    );

    // The created file and the exported helper are the two shapes the last
    // step proves; the tree is left on disk so a human can read it.
    let banner = std::fs::read_to_string(tree.join("src/banner.ts")).expect("banner.ts was made");
    assert!(banner.contains("import { SEP } from \"./utils\";"));
    let utils = std::fs::read_to_string(tree.join("src/utils.ts")).expect("utils.ts survived");
    assert_eq!(utils, "export const SEP = \"-\";\n");

    let traces = tree.parent().expect("a play directory");
    let text = std::fs::read_to_string(traces.join("trace-1.json")).expect("step 1 trace");
    let events: Vec<serde_json::Value> = serde_json::from_str(&text).expect("trace parses");
    assert!(
        events.iter().any(|event| event["ph"].as_str() == Some("B")),
        "step 1 emitted no chrome spans"
    );
}
