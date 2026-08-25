//! `boop debug <lane>`: the five sections a parent reads to answer "what
//! happened", each of which prints `none` rather than nothing.

use std::path::PathBuf;
use std::process::Command;

fn fixture(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!("boop-lane-debug-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("mail")).unwrap();
    home
}

fn boop(home: &PathBuf, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_boop"))
        .env_clear()
        .env("HOME", home)
        .env("BOOP_DB", home.join("mail").join("boop.db"))
        .env("PATH", "/usr/bin:/bin")
        .args(args)
        .arg("--mail-dir")
        .arg(home.join("mail"))
        .output()
        .unwrap()
}

fn sections(stdout: &str) -> Vec<&str> {
    stdout
        .lines()
        .filter(|line| line.starts_with("== "))
        .collect()
}

// FAIL-PRE-FIX: `boop debug` answered only with a WARN/ERROR window, so a
// parent asking what a lane did got nothing about its route, its mail, its
// commits, or its transcript.
#[test]
fn a_lane_with_nothing_recorded_prints_five_sections_of_none() {
    let home = fixture("empty");
    let output = boop(&home, &["debug", "feature-nothing"]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(
        sections(&stdout),
        [
            "== 1 route feature-nothing ==",
            "== 2 mail feature-nothing ==",
            "== 3 worktree feature-nothing ==",
            "== 4 transcript feature-nothing ==",
            "== 5 alerts feature-nothing ==",
        ],
        "five sections, in the order a reader asks them: {stdout}"
    );
    assert_eq!(
        stdout.matches("\nnone\n").count(),
        4,
        "every empty section says none: {stdout}"
    );
    let _ = std::fs::remove_dir_all(home);
}

/// RECEIPT (debug). A registered lane with one worktree commit and one mail
/// row fills the route, mail, and worktree sections, and the mail row names
/// the rung its delivery landed on.
#[test]
fn a_registered_lane_fills_the_route_mail_and_worktree_sections() {
    let home = fixture("filled");
    let tree = home.join("tree");
    std::fs::create_dir_all(&tree).unwrap();
    let git = |args: &[&str]| {
        Command::new("git")
            .args(["-C", tree.to_str().unwrap()])
            .args(args)
            .output()
            .unwrap()
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.email", "lane@boop"]);
    git(&["config", "user.name", "lane"]);
    std::fs::write(tree.join("notes.md"), "one\n").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "first note"]);
    std::fs::write(tree.join("dirty.md"), "uncommitted\n").unwrap();

    std::fs::write(
        home.join("mail").join("registry.json"),
        serde_json::json!({
            "feature-notes": {
                "kind": "lane",
                "harness": "opencode",
                "model": "a-model",
                "cwd": tree.to_str().unwrap(),
                "parent": "coordinator",
            }
        })
        .to_string(),
    )
    .unwrap();
    let sent = boop(
        &home,
        &[
            "beep",
            "feature-notes",
            "work this",
            "--as",
            "coordinator",
            "--no-wait",
        ],
    );
    assert!(sent.status.success(), "stderr: {:?}", sent.stderr);

    let output = boop(&home, &["debug", "feature-notes"]);
    assert!(output.status.success(), "stderr: {:?}", output.stderr);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("harness   opencode"), "{stdout}");
    assert!(stdout.contains("parent    coordinator"), "{stdout}");
    assert!(
        stdout.contains("coordinator -> feature-notes [request] held-for-turn-boundary"),
        "the mail section names the rung the row landed on: {stdout}"
    );
    assert!(stdout.contains("first note"), "{stdout}");
    assert!(stdout.contains("dirty 1"), "{stdout}");
    let _ = std::fs::remove_dir_all(home);
}
