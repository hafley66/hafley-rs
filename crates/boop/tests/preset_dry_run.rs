//! Every row of the preset table, spawned as a dry run against the real
//! binary (issue presets-only-model-spelling: `luna` and `solx` (gpt through opencode) were both
//! found only at spawn, one by an ACP model rejection and one by the bail).

use boop_store::testing::BoopCommandExt;
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
            .boop_test_root(&self.root)
            .env("BOOP_CONFIG", self.root.join("config/boop/config.json"))
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

#[test]
fn instant_forks_select_the_native_claude_tui_and_keep_other_presets() {
    let fixture = Fixture::new("interactive-fork");
    git(
        &fixture.repo,
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    let store = boop::bus::open_store(&fixture.mail).unwrap();
    let id = store
        .turn_comment_upsert(&boop::ident::TurnCommentUpsert {
            client_id: "interactive-fork",
            kind: "note",
            quote: "unification",
            note: Some("Explain the quote like a textbook"),
            enabled: true,
            tab_name: Some("source"),
            targets: &[],
            ts: 1,
        })
        .unwrap();
    for cwd in [&fixture.repo, &fixture.root] {
        for (preset, row) in rows().into_iter().filter(|(name, _)| name != REFUSED) {
            let output = Command::new(env!("CARGO_BIN_EXE_boop"))
                .boop_test_root(&fixture.root)
                .env("BOOP_CONFIG", fixture.root.join("config/boop/config.json"))
                .env("BOOP_DB", fixture.root.join("boop.db"))
                .env("BOOP_NO_SYNC", "1")
                .args([
                    "beep",
                    "fork",
                    &id.to_string(),
                    "--interactive",
                    "--dry-run",
                    "--preset",
                    &preset,
                ])
                .arg("--cwd")
                .arg(cwd)
                .arg("--mail-dir")
                .arg(&fixture.mail)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{preset}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            let command = stdout
                .lines()
                .find_map(|line| line.strip_prefix("cmd: "))
                .unwrap();
            assert!(
                command.contains(&format!("boop tui {}", row.harness)),
                "{preset}: {command}"
            );
            assert!(!command.contains("beep lane run"), "{preset}: {command}");
            if row.harness == "codex" {
                if let Some(effort) = &row.effort {
                    assert!(
                        command.contains(&format!("model_reasoning_effort={effort}")),
                        "{command}"
                    );
                }
            }
            assert!(
                command.contains(&format!("--model '{}'", row.model)),
                "{command}"
            );
            if row.harness == "opencode" {
                if let Some(variant) = row.variant.as_ref().or(row.effort.as_ref()) {
                    assert!(
                        command.contains(&format!("--initial-effort '{variant}'")),
                        "{command}"
                    );
                }
            }
            assert!(parses_as_shell(command), "{command}");
            let brief = std::fs::read_to_string(
                fixture.mail.join("forks").join(format!("comment-{id}.md")),
            )
            .unwrap();
            assert!(brief.contains("Explain the quote like a textbook"));
            assert!(brief.contains("unification"));
        }
    }
}

#[test]
fn interactive_fork_accepts_keyboard_input_in_its_tmux_pane() {
    use std::time::{Duration, Instant};
    let tmux = Command::new("which").arg("tmux").output().unwrap();
    assert!(
        tmux.status.success(),
        "tmux is required for this regression"
    );
    let tmux = String::from_utf8(tmux.stdout).unwrap().trim().to_owned();
    struct Server {
        bin: String,
        name: String,
    }
    impl Drop for Server {
        fn drop(&mut self) {
            let _ = Command::new(&self.bin)
                .args(["-L", &self.name, "kill-server"])
                .output();
        }
    }
    for has_repo in [true, false] {
        let fixture = Fixture::new("interactive-tmux");
        let server = Server {
            bin: tmux.clone(),
            name: format!("boop-fork-input-{}", std::process::id()),
        };
        let bin = fixture.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let fake = bin.join("fixture-claude");
        std::fs::write(&fake, "#!/usr/bin/env python3\nimport sys\nprint('FORK-INPUT-READY', flush=True)\nprint(repr(sys.argv[1:]), flush=True)\nfor line in sys.stdin:\n print('FORK-INPUT-ECHO:' + line.strip(), flush=True)\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mux = bin.join("tmux");
        std::fs::write(
            &mux,
            format!(
                "#!/bin/sh\nexec '{}' -L '{}' -f /dev/null \"$@\"\n",
                server.bin, server.name
            ),
        )
        .unwrap();
        std::fs::set_permissions(&mux, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_boop"), bin.join("boop")).unwrap();
        let mut config: serde_json::Value = serde_json::from_str(TABLE).unwrap();
        config["model-presets"]["fable"]["bin"] = fake.display().to_string().into();
        std::fs::write(
            fixture.root.join("config/boop/config.json"),
            config.to_string(),
        )
        .unwrap();
        git(
            &fixture.repo,
            &["update-ref", "refs/remotes/origin/main", "HEAD"],
        );
        let store = boop::bus::open_store(&fixture.mail).unwrap();
        let id = store
            .turn_comment_upsert(&boop::ident::TurnCommentUpsert {
                client_id: "tmux-fork",
                kind: "note",
                quote: "quoted context",
                note: Some("Explain this"),
                enabled: true,
                tab_name: Some("source"),
                targets: &[],
                ts: 1,
            })
            .unwrap();
        let lane = format!("fork-comment-{id}");
        let output = Command::new(env!("CARGO_BIN_EXE_boop"))
            .boop_test_root(&fixture.root)
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("BOOP_CONFIG", fixture.root.join("config/boop/config.json"))
            .env("BOOP_DB", fixture.mail.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .env("BOOP_DISK_FLOOR_GB", "0")
            .args([
                "beep",
                "fork",
                &id.to_string(),
                "--interactive",
                "--preset",
                "fable",
            ])
            .arg("--cwd")
            .arg(if has_repo {
                &fixture.repo
            } else {
                &fixture.root
            })
            .arg("--mail-dir")
            .arg(&fixture.mail)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let screen = Command::new(&server.bin)
                .args(["-L", &server.name, "capture-pane", "-p", "-t", &lane])
                .output()
                .unwrap();
            if String::from_utf8_lossy(&screen.stdout).contains("FORK-INPUT-READY") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "native input did not open: {}",
                String::from_utf8_lossy(&screen.stdout)
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(Command::new(&server.bin)
            .args([
                "-L",
                &server.name,
                "send-keys",
                "-t",
                &lane,
                "next-question",
                "Enter"
            ])
            .status()
            .unwrap()
            .success());
        loop {
            let screen = Command::new(&server.bin)
                .args(["-L", &server.name, "capture-pane", "-p", "-t", &lane])
                .output()
                .unwrap();
            let screen = String::from_utf8_lossy(&screen.stdout);
            let routes = boop::bus::read_routes(&fixture.mail).unwrap();
            let route = &routes[&lane];
            if screen.contains("FORK-INPUT-ECHO:next-question") && route.source_path.is_some() {
                assert_eq!(route.kind.as_str(), "coordinator");
                assert_eq!(route.goal.as_deref(), Some("Explain this"));
                assert_eq!(route.base_sha.is_some(), has_repo);
                if has_repo {
                    assert!(route
                        .worktree_dir
                        .as_ref()
                        .unwrap()
                        .ends_with(&format!("fork/comment-{id}")));
                } else {
                    assert_eq!(route.worktree_dir, None);
                    assert_eq!(
                        std::fs::canonicalize(route.cwd.as_ref().unwrap()).unwrap(),
                        std::fs::canonicalize(&fixture.root).unwrap()
                    );
                    assert_eq!(store.turn_comment_forks(id).unwrap()[0].branch, "");
                    assert!(!fixture.root.join(".git").exists());
                }
                assert!(screen.contains("Read the fork context"));
                assert_eq!(store.turn_comment_forks(id).unwrap()[0].lane, lane);
                break;
            }
            assert!(Instant::now() < deadline, "input/route not ready: {screen}");
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
