//! The install rail: which trees may be installed over ~/.cargo/bin/boop, and
//! whether the installed binary can be named afterwards.
//!
//! FAIL-PRE-FIX: three installs inside ten minutes on 2026-08-16, one of them
//! from a tree carrying no fix, and a lane that died at 42s on the third could
//! not be tied to any of them. `boop --version` printed `boop 0.0.2` for all
//! three, and nothing refused an install from a tree that was never on main.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// A clock reading is not an identifier: two parallel test threads read one
/// value and race into one fixture path (sprefa failure ledger 54).
static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn guard_script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("install-guard.sh")
}

/// The guard's verdict on one repo: exit code, stdout, stderr.
fn guard(repo: &Path) -> (i32, String, String) {
    let out = Command::new("bash")
        .arg(guard_script())
        .arg(repo)
        .output()
        .expect("run install-guard.sh");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn git(repo: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn git_line(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A repo on `main` with one commit, exposed as `origin/main`: the shape the
/// guard is meant to allow.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Fixture {
        let root = std::env::temp_dir().join(format!(
            "boop-install-rail-{}-{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "user.email", "t@t"]);
        git(&root, &["config", "user.name", "t"]);
        std::fs::write(root.join("seed.txt"), "seed\n").unwrap();
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-qm", "seed"]);
        let head = git_line(&root, &["rev-parse", "HEAD"]);
        git(&root, &["update-ref", "refs/remotes/origin/main", &head]);
        Fixture { root }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

// RECEIPT. The allowed shape: HEAD on origin/main, no tracked file changed.
#[test]
fn a_clean_tree_on_origin_main_is_allowed() {
    let repo = Fixture::new();
    let (code, out, err) = guard(&repo.root);
    assert_eq!(code, 0, "stdout: {out}\nstderr: {err}");
    assert!(out.contains("is on origin/main"), "{out}");
}

// FAIL-PRE-FIX: nothing stopped an install from a tree carrying uncommitted
// work, so the binary in ~/.cargo/bin matched no commit anyone could name.
// Sabotage receipt: deleting the guard's tracked-changes block made this PASS
// the install (code 0) with a modified file in the tree.
#[test]
fn a_tracked_change_refuses_the_install() {
    let repo = Fixture::new();
    std::fs::write(repo.root.join("seed.txt"), "edited\n").unwrap();
    let (code, out, err) = guard(&repo.root);
    assert_eq!(code, 1, "stdout: {out}\nstderr: {err}");
    assert!(err.contains("tracked files differ from HEAD"), "{err}");
    assert!(err.contains("seed.txt"), "{err}");
}

// An untracked file is not a difference from the commit, so it is reported and
// allowed; refusing on it would make the rail something drivers route around.
#[test]
fn an_untracked_file_is_reported_and_allowed() {
    let repo = Fixture::new();
    std::fs::write(repo.root.join("scratch.md"), "notes\n").unwrap();
    let (code, out, err) = guard(&repo.root);
    assert_eq!(code, 0, "stdout: {out}\nstderr: {err}");
    assert!(out.contains("scratch.md"), "{out}");
}

// FAIL-PRE-FIX: the 23:43 and 23:49 installs on 2026-08-16 came from trees that
// were never on origin/main, and the second overwrote a merged fix.
// Sabotage receipt: deleting the guard's `merge-base --is-ancestor` block made
// this PASS the install (code 0) from a commit on no branch main had seen.
#[test]
fn a_commit_that_never_reached_origin_main_refuses_the_install() {
    let repo = Fixture::new();
    std::fs::write(repo.root.join("local.txt"), "local only\n").unwrap();
    git(&repo.root, &["add", "-A"]);
    git(&repo.root, &["commit", "-qm", "local only"]);
    let (code, out, err) = guard(&repo.root);
    assert_eq!(code, 1, "stdout: {out}\nstderr: {err}");
    assert!(err.contains("not an ancestor of origin/main"), "{err}");
}

// A repo with no origin/main at all cannot answer the question, so it refuses
// rather than installing on a check it never ran.
#[test]
fn a_repo_without_origin_main_refuses_the_install() {
    let repo = Fixture::new();
    git(
        &repo.root,
        &["update-ref", "-d", "refs/remotes/origin/main"],
    );
    let (code, _, err) = guard(&repo.root);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("no origin/main"), "{err}");
}

#[test]
fn a_path_that_is_not_a_repo_refuses_the_install() {
    let (code, _, err) = guard(Path::new("/"));
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("not a git repository"), "{err}");
}

// FAIL-PRE-FIX: `boop --version` printed `boop 0.0.2` whichever tree built it,
// so three installs in ten minutes were indistinguishable.
// Sabotage receipt: restoring the bare `version` clap attribute FAILED this on
// a version string with no parenthesised commit.
#[test]
fn the_version_string_carries_the_commit_it_was_built_from() {
    let out = Command::new(env!("CARGO_BIN_EXE_boop"))
        .arg("--version")
        .output()
        .expect("run boop --version");
    assert!(out.status.success());
    let printed = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let stamp = printed
        .rsplit_once('(')
        .and_then(|(_, tail)| tail.strip_suffix(')'))
        .unwrap_or_else(|| panic!("no build stamp in {printed:?}"))
        .to_owned();
    assert!(
        printed.starts_with(&format!("boop {}", env!("CARGO_PKG_VERSION"))),
        "{printed}"
    );
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let head = Command::new("git")
        .arg("-C")
        .arg(manifest)
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned());
    match head {
        // The dirty suffix is not asserted: an edit made after the build script
        // ran would leave the stamp behind without rerunning it.
        Some(head) => assert!(
            stamp == head || stamp == format!("{head}-dirty"),
            "stamp {stamp:?} names neither HEAD {head:?} nor its dirty form"
        ),
        None => assert_eq!(stamp, "unknown", "{printed}"),
    }
}

// FAIL-PRE-FIX: a lane spawned by an unknown binary died and no log line tied
// the death to any build. The stamp rides the log rather than stdout: stdout's
// first line is a contract two tests in `lane_wait_exit.rs` read.
// Sabotage receipt: dropping `boop_build` from `run_lane`'s first event FAILED
// this on a spawn naming no build.
#[test]
fn a_lane_spawn_names_the_binary_that_ran_it() {
    let repo = Fixture::new();
    let brief = repo.root.join("brief.md");
    std::fs::write(&brief, "do the work\n").unwrap();
    let mail = repo.root.join("mail");
    std::fs::create_dir_all(&mail).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_boop"))
        .args(["beep", "lane", "create"])
        .arg("--cwd")
        .arg(&repo.root)
        .arg("--brief")
        .arg(&brief)
        .arg("--mail-dir")
        .arg(&mail)
        .args(["--branch", "fix/install-rail-probe", "--dry-run"])
        .output()
        .expect("run boop beep lane create --dry-run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");
    // The first log line of the spawn, before anything is opened.
    let first = stderr.lines().next().unwrap_or_default();
    // The pane formatter wraps every field name in colour codes, so the name
    // and its value are asserted separately.
    assert!(first.contains("lane create resolved"), "stderr: {stderr}");
    assert!(first.contains("boop_build"), "first: {first}");
    assert!(
        first.contains(&format!("\"{}\"", boop::BUILD)),
        "first: {first}"
    );
}
