//! `boop lane create --env KEY=VAL`: the pairs ride the supervisor's spawn
//! env stamp (and the dry-run `cmd:` line), shell-quoted; malformed values and
//! keys that collide with a boop-owned stamp are refused.

use boop_store::testing::BoopCommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const TABLE: &str = include_str!("fixtures/preset_table.json");

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    brief: PathBuf,
    mail: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let root =
            std::env::temp_dir().join(format!("boop-lane-env-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let mail = root.join("mail");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&mail).unwrap();
        for config in [
            root.join("Library/Application Support/boop"),
            root.join("config/boop"),
        ] {
            std::fs::create_dir_all(&config).unwrap();
            std::fs::write(config.join("config.json"), TABLE).unwrap();
        }
        let brief = repo.join("brief.md");
        std::fs::write(&brief, "finish and report\n").unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["add", "."]);
        git(
            &repo,
            &[
                "-c",
                "user.name=Boop Test",
                "-c",
                "user.email=boop@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ],
        );
        Fixture {
            root,
            repo,
            brief,
            mail,
        }
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_boop"))
            .boop_test_root(&self.root)
            .env("BOOP_CONFIG", self.root.join("config/boop/config.json"))
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .args(["beep", "lane", "create", "--branch", "feature/x"])
            .arg("--cwd")
            .arg(&self.repo)
            .arg("--brief")
            .arg(&self.brief)
            .args(["--preset", "flash4"])
            .arg("--mail-dir")
            .arg(&self.mail)
            .args(["--no-start", "--dry-run"])
            .args(args)
            .output()
            .expect("run the boop binary")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .expect("git is required by this test");
    assert!(status.success(), "git {args:?}");
}

fn cmd_line(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("cmd: "))
        .expect("cmd line")
        .to_owned()
}

/// RECEIPT. Repeatable `--env` pairs land on the dry-run `cmd:` line as
/// shell-quoted `KEY='VAL'` stamps the supervisor child inherits.
#[test]
fn env_pairs_ride_the_dry_run_cmd_line() {
    let fixture = Fixture::new("pairs");
    let output = fixture.run(&["--env", "A=1", "--env", "B=two words"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cmd = cmd_line(&output);
    assert!(cmd.contains("A='1'"), "cmd: {cmd}");
    assert!(cmd.contains("B='two words'"), "cmd: {cmd}");
}

/// RECEIPT. A `--env` with no `=` fails the clap value_parser, naming the
/// offending value, before anything spawns.
#[test]
fn a_value_without_equals_is_refused_at_the_cli() {
    let fixture = Fixture::new("noequals");
    let output = fixture.run(&["--env", "NOEQUALS"]);
    assert!(!output.status.success(), "NOEQUALS must not dry run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("NOEQUALS"), "{stderr}");
    assert!(stderr.contains("--env"), "{stderr}");
}

/// RECEIPT. A key that collides with a boop-owned stamp is refused by name so
/// the lane can't lie about its own identity.
#[test]
fn a_key_colliding_with_a_boop_stamp_is_refused_by_name() {
    let fixture = Fixture::new("collide");
    for key in ["BOOP_SESSION", "BOOP_LANE", "BOOP_HARNESS", "BOOP_PARENT"] {
        let output = fixture.run(&["--env", &format!("{key}=x")]);
        assert!(!output.status.success(), "{key} must not dry run");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(key), "{key}: {stderr}");
    }
}
