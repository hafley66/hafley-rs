use boop_store::testing::BoopCommandExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Fixture {
    root: PathBuf,
    mail: PathBuf,
    bin: PathBuf,
    tmux_calls: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("boop-lane-delete-dry-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mail = root.join("mail");
        let bin = root.join("bin");
        let repo = root.join("repo");
        std::fs::create_dir_all(&mail).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .output()
                .unwrap()
        };
        assert!(git(&["init", "-q", "-b", "main"]).status.success());
        assert!(git(&["config", "user.name", "Boop Test"]).status.success());
        assert!(git(&["config", "user.email", "boop@example.invalid"])
            .status
            .success());
        std::fs::write(repo.join("seed.txt"), "seed\n").unwrap();
        assert!(git(&["add", "-A"]).status.success());
        assert!(git(&["commit", "-qm", "seed"]).status.success());

        let tmux_calls = root.join("tmux-calls");
        let tmux = bin.join("tmux");
        std::fs::write(
            &tmux,
            format!(
                "#!/bin/sh\nif [ \"$1\" = kill-session ]; then printf '%s\\n' \"$*\" >> '{}'; fi\nexit 0\n",
                tmux_calls.display()
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&tmux).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&tmux, permissions).unwrap();

        let worktree = root.join("lane-worktree");
        let added = git(&[
            "worktree",
            "add",
            "-b",
            "chore/delete-dry-run-probe",
            worktree.to_str().unwrap(),
        ]);
        assert!(
            added.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&added.stderr)
        );
        std::fs::write(
            mail.join("registry.json"),
            serde_json::json!({
                "probe-lane": {
                    "kind": "lane",
                    "tmux": "boop-delete-dry-run-probe",
                    "worktreeDir": worktree,
                }
            })
            .to_string(),
        )
        .unwrap();

        Self {
            root,
            mail,
            bin,
            tmux_calls,
        }
    }

    fn boop(&self, args: &[&str]) -> Output {
        let path = format!(
            "{}:{}",
            self.bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new(env!("CARGO_BIN_EXE_boop"))
            .boop_test_root(self.root.join("home"))
            .env("BOOP_DB", self.mail.join("boop.db"))
            .env("BOOP_NO_SYNC", "1")
            .env("PATH", path)
            .args(args)
            .arg("--mail-dir")
            .arg(&self.mail)
            .output()
            .unwrap()
    }

    fn drop(self) {
        let _ = std::fs::remove_dir_all(self.root);
    }
}

fn text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn single_lane_dry_run_preserves_a_live_route_and_its_pane() {
    let fixture = Fixture::new();
    let live = Command::new(fixture.bin.join("tmux"))
        .args(["has-session", "-t", "boop-delete-dry-run-probe"])
        .status()
        .unwrap();
    assert!(live.success(), "the fixture marks the lane pane live");

    let output = fixture.boop(&["beep", "lane", "delete", "probe-lane", "--dry-run"]);
    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let stdout = text(&output);
    assert!(
        stdout.contains("lane probe-lane: route (lane) would be removed"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("lane probe-lane: tmux session boop-delete-dry-run-probe would be killed"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("lane-worktree"), "stdout: {stdout}");
    assert!(
        boop::bus::read_routes(&fixture.mail)
            .unwrap()
            .contains_key("probe-lane"),
        "dry-run must keep the registered lane"
    );
    assert!(
        Path::new(&fixture.root.join("lane-worktree")).exists(),
        "dry-run must keep the worktree"
    );
    let branches = Command::new("git")
        .arg("-C")
        .arg(fixture.root.join("repo"))
        .args(["branch", "--list", "chore/delete-dry-run-probe"])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&branches.stdout).contains("chore/delete-dry-run-probe"),
        "dry-run must keep the branch"
    );
    assert!(
        !fixture.tmux_calls.exists(),
        "dry-run must not kill the pane"
    );

    let help = fixture.boop(&["beep", "lane", "delete", "--help"]);
    assert!(help.status.success(), "stderr: {}", stderr(&help));
    assert!(
        text(&help).contains("single-lane or bulk delete"),
        "help must state the single-lane dry-run contract: {}",
        text(&help)
    );
    fixture.drop();
}
