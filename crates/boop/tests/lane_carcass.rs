//! FAIL-PRE-FIX: a dead-on-arrival spawn left its worktree and branch standing
//! while its epilogue dropped the route, so `lane create` bailed on the path
//! and `lane delete` bailed on the missing route. Both asserts below fail on
//! the pre-fix tree; only `git worktree remove --force` unblocked the name.

use boop_store::testing::BoopCommandExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// A throwaway repo, mailbox, PATH and tmux socket. The harness the spawn
/// names is absent from that PATH, which is what makes every spawn DOA.
struct Doa {
    root: PathBuf,
    repo: PathBuf,
    brief: PathBuf,
    mail: PathBuf,
    bin: PathBuf,
    socket: String,
}

fn executable(name: &str) -> PathBuf {
    let output = Command::new("sh")
        .args(["-c", &format!("command -v {name}")])
        .output()
        .unwrap();
    assert!(output.status.success(), "{name} is required by this test");
    PathBuf::from(String::from_utf8(output.stdout).unwrap().trim())
}

impl Doa {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("boop-carcass-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let mail = root.join("mail");
        let bin = root.join("bin");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&mail).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        let brief = repo.join("brief.md");
        std::fs::write(&brief, "do the work\n").unwrap();
        let git = executable("git");
        let at = repo.display().to_string();
        Command::new(&git)
            .args(["-C", &at, "init", "-q", "-b", "main"])
            .status()
            .unwrap();
        Command::new(&git)
            .args(["-C", &at, "add", "-A"])
            .status()
            .unwrap();
        Command::new(&git)
            .args([
                "-C",
                &at,
                "-c",
                "user.name=Boop Test",
                "-c",
                "user.email=boop@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ])
            .status()
            .unwrap();
        symlink(git, bin.join("git")).unwrap();
        boop_store::testing::write_tmux_fixture(&bin.join("tmux"), &executable("tmux"));
        symlink(env!("CARGO_BIN_EXE_boop"), bin.join("boop")).unwrap();
        Doa {
            root,
            repo,
            brief,
            mail,
            bin,
            socket: format!("boop-carcass-{}-{tag}", std::process::id()),
        }
    }

    fn boop(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_boop"));
        command
            .boop_test_root(&self.root)
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("BOOP_CONFIG", self.root.join("config/boop/config.json"))
            .env("PATH", &self.bin)
            .current_dir(&self.repo);
        command
    }

    fn create(&self, branch: &str, reclaim: bool) -> std::process::Output {
        let mut command = self.boop();
        command
            .args(["beep", "lane", "create", "--branch", branch])
            .arg("--cwd")
            .arg(&self.repo)
            .arg("--brief")
            .arg(&self.brief)
            .args(["--harness", "codex", "--model", "gpt-test"])
            .arg("--socket")
            .arg(&self.socket)
            .args(["--parent", "sprefa-coordinator"])
            .arg("--mail-dir")
            .arg(&self.mail)
            .arg("--no-start");
        if reclaim {
            command.arg("--reclaim");
        }
        command.output().unwrap()
    }

    fn worktree_of(&self, branch: &str) -> PathBuf {
        boop::lane::worktree_dir(&self.repo, branch)
    }

    fn branches(&self) -> String {
        let output = Command::new(executable("git"))
            .args([
                "-C",
                &self.repo.display().to_string(),
                "branch",
                "--format=%(refname:short)",
            ])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Spawn `branch` and block until the pane is gone and the epilogue has
    /// dropped the route, which is the carcass state a driver walks up to.
    fn spawn_a_carcass(&self, branch: &str, lane: &str) {
        let created = self.create(branch, false);
        assert!(
            created.status.success(),
            "the first spawn must reach tmux: {}",
            String::from_utf8_lossy(&created.stderr)
        );
        let worktree = self.worktree_of(branch);
        assert!(worktree.exists(), "the spawn makes the worktree");
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            let routed = boop::bus::read_routes(&self.mail)
                .unwrap_or_default()
                .contains_key(lane);
            if !routed && !self.session_alive(lane) {
                return;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        let log = std::fs::read_to_string(self.root.join("lanes").join(lane).join("supervise.log"))
            .unwrap_or_default();
        let pane = Command::new(executable("tmux"))
            .args(["-L", &self.socket, "capture-pane", "-p", "-t", lane])
            .output()
            .unwrap();
        panic!(
            "lane {lane} never died; supervise log: {log}; pane: {}",
            String::from_utf8_lossy(&pane.stdout)
        );
    }

    fn session_alive(&self, session: &str) -> bool {
        Command::new(executable("tmux"))
            .args(["-L", &self.socket, "has-session", "-t", session])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }
}

impl Drop for Doa {
    fn drop(&mut self) {
        let _ = Command::new(executable("tmux"))
            .args(["-L", &self.socket, "kill-server"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn is_worktree(path: &Path) -> bool {
    path.join(".git").exists()
}

/// FAIL-PRE-FIX (dead-lane-self-reset). A plain respawn bailed on `a branch
/// named '...' already exists`; it now resets the dead name and says so.
#[test]
fn a_plain_respawn_resets_the_name_a_dead_lane_left_behind() {
    let doa = Doa::new("reclaim");
    let branch = "feature/carcass-reclaim";
    let lane = "feature-carcass-reclaim";
    doa.spawn_a_carcass(branch, lane);

    let respawned = doa.create(branch, false);
    let out = text(&respawned.stdout);
    assert!(
        respawned.status.success(),
        "a dead name respawns with no flag: {}",
        text(&respawned.stderr)
    );
    assert!(
        out.contains(&format!("reclaim: {lane} was dead; removed ")),
        "{out}"
    );
    assert!(
        out.contains(".boop-worktrees/feature/carcass-reclaim"),
        "{out}"
    );
    assert!(out.contains(&format!("branch {branch}")), "{out}");
    assert!(
        is_worktree(&doa.worktree_of(branch)),
        "the respawn built the worktree again"
    );
}

/// RECEIPT. `lane delete` answers on a lane the epilogue already unrouted, and
/// says which worktree and branch it destroyed.
#[test]
fn lane_delete_clears_a_carcass_and_names_what_it_removed() {
    let doa = Doa::new("delete");
    let branch = "feature/carcass-delete";
    let lane = "feature-carcass-delete";
    doa.spawn_a_carcass(branch, lane);

    let deleted = doa
        .boop()
        .args(["beep", "lane", "delete", lane])
        .arg("--mail-dir")
        .arg(&doa.mail)
        .output()
        .unwrap();
    let out = text(&deleted.stdout);
    assert!(
        deleted.status.success(),
        "delete works with no route: {}",
        text(&deleted.stderr)
    );
    assert!(
        out.contains(&format!("deleted {lane}: removed worktree "))
            && out.contains(".boop-worktrees/feature/carcass-delete"),
        "{out}"
    );
    assert!(
        out.contains(&format!("deleted {lane}: removed branch {branch}")),
        "{out}"
    );
    assert!(!doa.worktree_of(branch).exists(), "the worktree is gone");
    assert!(!doa.branches().contains(branch), "the branch is gone");

    let again = doa
        .boop()
        .args(["beep", "lane", "delete", lane])
        .arg("--mail-dir")
        .arg(&doa.mail)
        .output()
        .unwrap();
    assert!(
        !again.status.success(),
        "a name with nothing behind it is still an error"
    );
    assert!(
        text(&again.stderr).contains("no registry route for lane"),
        "{}",
        text(&again.stderr)
    );
}

/// FAIL-PRE-FIX (gap 2). The reset ran only when a worktree or branch stood,
/// so a name cleaned by hand spawned onto the dead lane's conversation.
#[test]
fn a_create_clears_a_stale_pin_with_nothing_else_left_to_remove() {
    let doa = Doa::new("pin");
    let branch = "feature/carcass-pin";
    let lane = "feature-carcass-pin";
    let pin_dir = doa.root.join("lanes").join(lane);
    std::fs::create_dir_all(&pin_dir).unwrap();
    let pin = pin_dir.join("conversation");
    std::fs::write(
        &pin,
        r#"{"conversation":"ses_old","cwd":"/removed-by-hand","pinned_ts":1}"#,
    )
    .unwrap();
    assert!(!doa.worktree_of(branch).exists(), "nothing else is left");

    let created = doa.create(branch, false);
    let out = text(&created.stdout);
    assert!(
        created.status.success(),
        "the name spawns: {}",
        text(&created.stderr)
    );
    assert!(
        out.contains(&format!("reclaim: {lane} conversation pin cleared")),
        "{out}"
    );
    assert!(!pin.exists(), "the stale pin is gone");
}

/// The lane's `expect.json` as the trail holds it, `None` when absent.
fn trail_expect(doa: &Doa, lane: &str) -> Option<serde_json::Value> {
    let path = doa.root.join("lanes").join(lane).join("expect.json");
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// FAIL-PRE-FIX (expect-inherited-across-spawns). `write_expect` ran only when
/// a flag named one, so a respawn was failed by the first spawn's subject.
#[test]
fn a_create_with_no_expect_flag_clears_the_names_old_expectation() {
    let doa = Doa::new("expect");
    let branch = "feature/carcass-expect";
    let lane = "feature-carcass-expect";
    let trail = doa.root.join("lanes").join(lane);
    std::fs::create_dir_all(&trail).unwrap();
    std::fs::write(
        trail.join("expect.json"),
        r#"{"paths":[],"commit_subjects":["boop: a subject this brief never asked for"],"commits_at_least":null}"#,
    )
    .unwrap();

    let created = doa.create(branch, false);
    let out = text(&created.stdout);
    assert!(
        created.status.success(),
        "the name spawns: {}",
        text(&created.stderr)
    );
    assert!(
        out.contains(&format!("reclaim: {lane} expectation cleared")),
        "{out}"
    );
    assert_eq!(
        trail_expect(&doa, lane),
        Some(serde_json::json!({
            "paths": [],
            "commit_subjects": [],
            "commits_at_least": null,
        })),
        "the create owns the expectation, and it names nothing"
    );
}

/// RECEIPT. An expectation the create DID name still reaches the trail, and
/// it is the only one there.
#[test]
fn a_create_with_an_expect_flag_still_writes_it() {
    let doa = Doa::new("expect-flag");
    let branch = "feature/carcass-expect-flag";
    let lane = "feature-carcass-expect-flag";

    let mut command = doa.boop();
    let created = command
        .args(["beep", "lane", "create", "--branch", branch])
        .arg("--cwd")
        .arg(&doa.repo)
        .arg("--brief")
        .arg(&doa.brief)
        .args(["--harness", "codex", "--model", "gpt-test"])
        .arg("--socket")
        .arg(&doa.socket)
        .args(["--parent", "sprefa-coordinator"])
        .arg("--mail-dir")
        .arg(&doa.mail)
        .args(["--no-start", "--expect-path", "x"])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "the name spawns: {}",
        text(&created.stderr)
    );
    assert_eq!(
        trail_expect(&doa, lane),
        Some(serde_json::json!({
            "paths": ["x"],
            "commit_subjects": [],
            "commits_at_least": null,
        }))
    );
}

/// RECEIPT. A worktree holding uncommitted work is not a carcass; reclaim
/// refuses it and leaves both the tree and the branch standing.
#[test]
fn a_reclaim_refuses_a_worktree_that_still_holds_work() {
    let doa = Doa::new("dirty");
    let branch = "feature/carcass-dirty";
    let lane = "feature-carcass-dirty";
    doa.spawn_a_carcass(branch, lane);
    let worktree = doa.worktree_of(branch);
    std::fs::write(worktree.join("brief.md"), "edited by the lane\n").unwrap();

    let blocked = doa.create(branch, true);
    let complaint = text(&blocked.stderr);
    assert!(!blocked.status.success(), "dirty work blocks a reclaim");
    assert!(complaint.contains("uncommitted work"), "{complaint}");
    assert!(
        is_worktree(&worktree),
        "the worktree survives the refused reclaim"
    );
    assert!(doa.branches().contains(branch), "the branch survives too");
}
