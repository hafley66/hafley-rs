//! FAIL-PRE-FIX: a dead-on-arrival spawn left its worktree and branch standing
//! while its epilogue dropped the route, so `lane create` bailed on the path
//! and `lane delete` bailed on the missing route. Both asserts below fail on
//! the pre-fix tree; only `git worktree remove --force` unblocked the name.

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
        symlink(executable("tmux"), bin.join("tmux")).unwrap();
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
            .env_clear()
            .env("HOME", &self.root)
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
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
        panic!("lane {lane} never died; its route or pane is still up");
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

/// RECEIPT. The carcass blocks a plain respawn, the bail names `--reclaim`,
/// and the flagged respawn rebuilds the same worktree in one command.
#[test]
fn a_reclaim_respawns_the_name_a_dead_lane_left_behind() {
    let doa = Doa::new("reclaim");
    let branch = "feature/carcass-reclaim";
    let lane = "feature-carcass-reclaim";
    doa.spawn_a_carcass(branch, lane);

    let blocked = doa.create(branch, false);
    let complaint = text(&blocked.stderr);
    assert!(!blocked.status.success(), "the carcass blocks a respawn");
    assert!(
        complaint.contains("worktree path already exists"),
        "{complaint}"
    );
    assert!(complaint.contains("--reclaim"), "{complaint}");
    assert!(
        complaint.contains("boop beep lane delete feature-carcass-reclaim"),
        "{complaint}"
    );

    let reclaimed = doa.create(branch, true);
    let out = text(&reclaimed.stdout);
    assert!(
        reclaimed.status.success(),
        "--reclaim respawns the name: {}",
        text(&reclaimed.stderr)
    );
    assert!(out.contains("reclaim: removed worktree"), "{out}");
    assert!(
        out.contains("reclaim: removed branch feature/carcass-reclaim"),
        "{out}"
    );
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
