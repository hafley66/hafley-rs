//! `lane create` mints one `agent_lane` row id per spawn, and that id is the
//! lane's identity: the name is reused, the cwd is the same for every respawn
//! of a branch, and the harness conversation id moves on `/clear`, on
//! compaction and on `session/load`. The dry run says which id the name
//! carries today; the real create prints the id it minted and writes it onto
//! the trail record a revive replays.

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
            std::env::temp_dir().join(format!("boop-spawn-identity-{}-{name}", std::process::id()));
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

    /// `lane create --dry-run` for `lane`, against this fixture's own HOME.
    fn dry_run(&self, lane: &str) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_boop"))
            .env("HOME", &self.root)
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .args(["beep", "lane", "create", "--lane", lane])
            .arg("--cwd")
            .arg(&self.repo)
            .arg("--brief")
            .arg(&self.brief)
            .args(["--preset", "flash4"])
            .arg("--mail-dir")
            .arg(&self.mail)
            .args(["--no-start", "--dry-run"])
            .output()
            .expect("run the boop binary");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// The trail record `lane` would be revived from.
    fn write_spawn_record(&self, lane: &str, spawn_id: Option<i64>) {
        let dir = self.root.join(".agent").join("lanes").join(lane);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("spawn.json"),
            serde_json::json!({
                "tmux": lane,
                "socket": serde_json::Value::Null,
                "cwd": self.repo.display().to_string(),
                "command": "boop lane run",
                "route": serde_json::Value::Null,
                "spawn_id": spawn_id,
            })
            .to_string(),
        )
        .unwrap();
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

fn line<'a>(stdout: &'a str, key: &str) -> Option<&'a str> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
}

/// RECEIPT. The dry run names the spawn id the lane name carries now and says
/// a create replaces it, so a coordinator can quote the id it is looking at
/// before anything spawns.
#[test]
fn the_dry_run_names_the_spawn_id_the_name_carries() {
    let fixture = Fixture::new("dry");
    let fresh = fixture.dry_run("spawn-fresh");
    assert_eq!(
        line(&fresh, "spawn"),
        Some("none on the trail; create mints the first"),
        "{fresh}"
    );

    fixture.write_spawn_record("spawn-carried", Some(7));
    let carried = fixture.dry_run("spawn-carried");
    assert_eq!(
        line(&carried, "spawn"),
        Some("7 on the trail; create mints a new one"),
        "{carried}"
    );

    // A record written before the id existed reads as no id at all.
    fixture.write_spawn_record("spawn-legacy", None);
    let legacy = fixture.dry_run("spawn-legacy");
    assert_eq!(
        line(&legacy, "spawn"),
        Some("none on the trail; create mints the first"),
        "{legacy}"
    );
}
