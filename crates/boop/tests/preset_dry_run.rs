//! Every row of the preset table, spawned as a dry run against the real
//! binary (issue presets-only-model-spelling: `luna` and `solx` (gpt through opencode) were both
//! found only at spawn, one by an ACP model rejection and one by the bail).

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const TABLE: &str = include_str!("fixtures/preset_table.json");

/// The one row whose harness refuses its model: no gemini harness exists and
/// opencode would pay metered credit for the family.
const REFUSED: &str = "solx";

#[derive(serde::Deserialize)]
struct Row {
    harness: String,
    model: String,
    #[serde(default)]
    effort: Option<String>,
    #[serde(default)]
    variant: Option<String>,
    #[serde(default)]
    bin: Option<String>,
}

fn rows() -> BTreeMap<String, Row> {
    let table: serde_json::Value = serde_json::from_str(TABLE).expect("preset table parses");
    serde_json::from_value(table["model-presets"].clone()).expect("preset rows parse")
}

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    brief: PathBuf,
    mail: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let root =
            std::env::temp_dir().join(format!("boop-preset-dry-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let mail = root.join("mail");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&mail).unwrap();
        // `dirs::config_dir` reads HOME on macOS and XDG_CONFIG_HOME on linux;
        // the table is written where each one looks.
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

    fn dry_run(&self, preset: &str) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_boop"))
            .env("HOME", &self.root)
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .args(["beep", "lane", "create", "--lane"])
            .arg(format!("lane-{preset}"))
            .arg("--cwd")
            .arg(&self.repo)
            .arg("--brief")
            .arg(&self.brief)
            .args(["--preset", preset])
            .arg("--mail-dir")
            .arg(&self.mail)
            .args(["--no-start", "--dry-run"])
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

/// Whether the printed command is a command a shell can run. `sh -n` parses
/// without executing, which is exactly the question the dry run answers.
fn parses_as_shell(command: &str) -> bool {
    let script = std::env::temp_dir().join(format!(
        "boop-preset-cmd-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::write(&script, command).unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o644)).unwrap();
    let status = Command::new("sh")
        .arg("-n")
        .arg(&script)
        .status()
        .expect("sh is required by this test");
    let _ = std::fs::remove_file(&script);
    status.success()
}

fn field<'a>(stdout: &'a str, key: &str) -> Option<&'a str> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
}

/// RECEIPT. Every preset spawns: the command parses, the harness is the one
/// the row names, and no effort rides inside the model string.
#[test]
fn every_preset_dry_runs_into_a_command_its_own_harness_runs() {
    let fixture = Fixture::new("all");
    for (name, row) in rows() {
        if name == REFUSED {
            continue;
        }
        let output = fixture.dry_run(&name);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        assert!(
            output.status.success(),
            "preset {name} refused: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let command = field(&stdout, "cmd").unwrap_or_else(|| panic!("no cmd line for {name}"));
        assert!(parses_as_shell(command), "preset {name}: {command}");
        assert_eq!(
            field(&stdout, "harness"),
            Some(row.harness.as_str()),
            "preset {name}: {stdout}"
        );
        assert!(
            command.contains(&format!("--model '{}'", row.model)),
            "preset {name} must spawn its own model: {command}"
        );
        match &row.effort {
            Some(effort) => {
                assert!(
                    command.contains(&format!("--effort '{effort}'")),
                    "preset {name} must carry effort as a flag: {command}"
                );
                assert!(
                    !command.contains(&format!("{}@{effort}", row.model)),
                    "preset {name} must not spell effort inside the model: {command}"
                );
                assert_eq!(field(&stdout, "effort"), Some(effort.as_str()));
            }
            None => assert!(
                !command.contains("--effort"),
                "preset {name} names no effort: {command}"
            ),
        }
        match &row.variant {
            Some(variant) => assert!(
                command.contains(&format!("--variant '{variant}'")),
                "preset {name}: {command}"
            ),
            None => assert!(!command.contains("--variant"), "preset {name}: {command}"),
        }
        match &row.bin {
            Some(bin) => assert!(
                command.contains(&format!("--bin '{bin}'")),
                "preset {name}: {command}"
            ),
            None => assert!(!command.contains("--bin"), "preset {name}: {command}"),
        }
    }
}

/// RECEIPT. The one row no harness can run is refused at `lane create`, in the
/// bail's own words, rather than at the spawn 40 seconds later.
#[test]
fn the_banned_preset_is_refused_by_name() {
    let fixture = Fixture::new("banned");
    let output = fixture.dry_run(REFUSED);
    assert!(!output.status.success(), "{REFUSED} must not dry run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("BANNED from opencode"), "{stderr}");
    assert!(stderr.contains("codex"), "{stderr}");
}
