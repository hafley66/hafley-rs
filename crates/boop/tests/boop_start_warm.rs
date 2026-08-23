//! Pins `boop-start` warm-up: a shared build target crosses two sibling
//! spawns exactly once, and the setup sentence rides the preamble verbatim.

use std::path::{Path, PathBuf};
use std::process::Command;

use boop::harness::SpawnSpec;

fn executable(name: &str) -> PathBuf {
    let output = Command::new("sh")
        .args(["-c", &format!("command -v {name}")])
        .output()
        .expect("run `command -v`");
    assert!(output.status.success(), "{name} is required by this test");
    PathBuf::from(
        String::from_utf8(output.stdout)
            .expect("command -v prints utf8")
            .trim(),
    )
}

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} in {}: {}",
        repo.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn head_sha(repo: &Path) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("git rev-parse HEAD");
    assert!(output.status.success(), "git rev-parse HEAD failed");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A committed repo at `path`, on branch `main`.
fn init_repo(path: &Path) -> String {
    std::fs::create_dir_all(path).expect("make repo dir");
    let status = Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .arg(path)
        .status()
        .expect("git init");
    assert!(status.success(), "git init failed for {}", path.display());
    std::fs::write(path.join("seed.txt"), "seed").expect("write seed file");
    git(path, &["add", "-A"]);
    git(
        path,
        &[
            "-c",
            "user.name=Boop Test",
            "-c",
            "user.email=boop@example.invalid",
            "commit",
            "-qm",
            "seed",
        ],
    );
    head_sha(path)
}

/// Declares a `boop-start` recipe whose shared target is built once: a
/// second run against the same shared dir finds it already warm.
fn commit_shared_target_justfile(repo: &Path, shared: &Path, count_file: &Path) {
    let body = format!(
        "boop-start:\n\
         \t#!/usr/bin/env bash\n\
         \tset -euo pipefail\n\
         \tshared=\"{shared}\"\n\
         \tmkdir -p \"$shared\"\n\
         \tif [ -f \"$shared/built\" ]; then\n\
         \t  echo \"boop-start: shared target already warm\"\n\
         \telse\n\
         \t  echo cargo >> \"{count}\"\n\
         \t  touch \"$shared/built\"\n\
         \t  echo \"boop-start: shared target built\"\n\
         \tfi\n",
        shared = shared.display(),
        count = count_file.display(),
    );
    std::fs::write(repo.join("justfile"), body).expect("write justfile");
    git(repo, &["add", "-A"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Boop Test",
            "-c",
            "user.email=boop@example.invalid",
            "commit",
            "-qm",
            "add boop-start",
        ],
    );
}

fn spawn_spec(
    repo: &Path,
    branch: &str,
    base_sha: &str,
    worktree: &Path,
    lane: &str,
    mail_dir: &Path,
) -> SpawnSpec {
    SpawnSpec {
        harness: boop::harness::HarnessId::Claude,
        branch: branch.to_owned(),
        base_sha: base_sha.to_owned(),
        main_tree: false,
        setup: Vec::new(),
        prompt: "do the lane".to_owned(),
        resume_session: None,
        socket: None,
        worktree_dir: Some(worktree.to_path_buf()),
        repo: repo.to_path_buf(),
        env_stamp: None,
        model: None,
        variant: None,
        bin: None,
        on_exit: None,
        tmux: None,
        lane: lane.to_owned(),
        mail_dir: mail_dir.to_path_buf(),
        warm_start: true,
    }
}

#[test]
fn one_shared_target_is_built_once_across_two_sibling_spawns() {
    let root = std::env::temp_dir().join(format!("boop-start-warm-{}-count", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let repo = root.join("repo");
    let shared = root.join("shared-target");
    let count_file = root.join("cargo-count.txt");
    let mail = root.join("mail");
    std::fs::create_dir_all(&mail).expect("make mail dir");

    init_repo(&repo);
    commit_shared_target_justfile(&repo, &shared, &count_file);

    let worktree_one = boop::lane::worktree_dir(&repo, "feature/one");
    let worktree_two = boop::lane::worktree_dir(&repo, "feature/two");
    let sha = head_sha(&repo);

    let spec_one = spawn_spec(&repo, "feature/one", &sha, &worktree_one, "lane-one", &mail);
    boop::worktree::prepare_spawn_dir(&spec_one).expect("first spawn prepares its worktree");

    let spec_two = spawn_spec(&repo, "feature/two", &sha, &worktree_two, "lane-two", &mail);
    boop::worktree::prepare_spawn_dir(&spec_two).expect("second spawn prepares its worktree");

    let count_text = std::fs::read_to_string(&count_file).unwrap_or_default();
    let lines: Vec<&str> = count_text.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "cargo ran {} times across two spawns: {count_text:?}",
        lines.len()
    );

    let status_two = std::fs::read_to_string(boop::lane::start_status_path(&mail, "lane-two"))
        .expect("second lane's start status was recorded");
    assert!(status_two.contains("already warm"), "{status_two}");
    assert!(
        status_two.starts_with("boop-start: ready in"),
        "{status_two}"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_tree_without_the_recipe_warms_nothing_and_does_not_fail() {
    let dir = std::env::temp_dir().join(format!("boop-start-warm-{}-norecipe", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("make target dir");

    let outcome = boop::worktree::warm_start(&dir).expect("a tree with no recipe still returns Ok");
    assert!(!outcome.ran, "status: {}", outcome.status);
    assert_eq!(
        outcome.status,
        format!(
            "boop-start: no recipe in {}, nothing to warm",
            dir.display()
        )
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_preamble_leads_the_brief_with_the_setup_sentence_verbatim() {
    let mail =
        std::env::temp_dir().join(format!("boop-start-warm-{}-preamble", std::process::id()));
    let _ = std::fs::remove_dir_all(&mail);
    std::fs::create_dir_all(&mail).expect("make mail dir");
    let brief = mail.join("brief.md");
    std::fs::write(&brief, "do the work\n").expect("write brief");

    boop::lane::record_start_status(&mail, "fix-thing", "boop-start: ready in 1.2s (built)")
        .expect("record the start status");
    let composed = boop::lane::brief_with_preamble(&mail, "fix-thing", &brief);
    assert_ne!(
        composed, brief,
        "a recorded status must compose a new brief, not hand back the original path"
    );
    let text = std::fs::read_to_string(&composed).expect("read the composed brief");
    let mut lines = text.lines();
    assert_eq!(
        lines.next(),
        Some("boop-start: ready in 1.2s (built)"),
        "{text}"
    );
    assert_eq!(
        lines.next(),
        Some("setup is done; do not run installs or builds to get started; build only what you change."),
        "{text}"
    );
    assert!(text.contains("do the work"), "{text}");

    let unset = boop::lane::brief_with_preamble(&mail, "no-status-for-this-lane", &brief);
    assert_eq!(
        unset, brief,
        "with no status file recorded, the original brief path comes back unchanged"
    );

    let _ = std::fs::remove_dir_all(&mail);
}

/// A throwaway repo, mailbox, and PATH holding only `git`, `just`, `bash` and
/// `boop`; the CLI resolves `boop-start` for real through this PATH.
struct DryRunFixture {
    root: PathBuf,
    repo: PathBuf,
    brief: PathBuf,
    mail: PathBuf,
    bin: PathBuf,
    base_sha: String,
}

impl DryRunFixture {
    fn new(tag: &str, with_justfile: bool) -> Self {
        let root =
            std::env::temp_dir().join(format!("boop-start-warm-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repo = root.join("repo");
        let mail = root.join("mail");
        let bin = root.join("bin");
        std::fs::create_dir_all(&mail).expect("make mail dir");
        std::fs::create_dir_all(&bin).expect("make bin dir");
        let base_sha = init_repo(&repo);
        let base_sha = if with_justfile {
            commit_shared_target_justfile(
                &repo,
                &root.join("shared-target"),
                &root.join("cargo-count.txt"),
            );
            head_sha(&repo)
        } else {
            base_sha
        };
        let brief = root.join("brief.md");
        std::fs::write(&brief, "do the work\n").expect("write brief");
        std::os::unix::fs::symlink(executable("git"), bin.join("git")).expect("link git");
        std::os::unix::fs::symlink(executable("just"), bin.join("just")).expect("link just");
        std::os::unix::fs::symlink(executable("bash"), bin.join("bash")).expect("link bash");
        DryRunFixture {
            root,
            repo,
            brief,
            mail,
            bin,
            base_sha,
        }
    }

    fn run(&self, branch: &str, extra: &[&str]) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_boop"))
            .env_clear()
            .env("HOME", &self.root)
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("XDG_CONFIG_HOME", self.root.join("config"))
            .env("PATH", &self.bin)
            .args(["beep", "lane", "create"])
            .args(["--branch", branch])
            .arg("--cwd")
            .arg(&self.repo)
            .arg("--brief")
            .arg(&self.brief)
            .args(["--harness", "codex", "--model", "gpt-test"])
            .arg("--base-sha")
            .arg(&self.base_sha)
            .arg("--mail-dir")
            .arg(&self.mail)
            .args(["--dry-run"])
            .args(extra)
            .output()
            .expect("run boop")
    }
}

impl Drop for DryRunFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn combined(output: &std::process::Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn lane_create_dry_run_names_the_justfile_boop_start_runs_from() {
    let fixture = DryRunFixture::new("dryrun-with", true);
    let output = fixture.run("feature/dry", &[]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(output.status.success(), "{}", combined(&output));
    let canonical_justfile = std::fs::canonicalize(fixture.repo.join("justfile"))
        .expect("canonicalize the fixture justfile");
    let expected = format!("boop-start: will run from {}", canonical_justfile.display());
    assert!(stdout.contains(&expected), "{}", combined(&output));

    let no_start_output = fixture.run("feature/dry-no-start", &["--no-start"]);
    assert!(
        no_start_output.status.success(),
        "{}",
        combined(&no_start_output)
    );
    let no_start_stdout = String::from_utf8_lossy(&no_start_output.stdout).into_owned();
    assert!(
        no_start_stdout.contains("boop-start: skipped (--no-start)"),
        "{}",
        combined(&no_start_output)
    );

    let bare = DryRunFixture::new("dryrun-without", false);
    let bare_output = bare.run("feature/dry-bare", &[]);
    let bare_stdout = String::from_utf8_lossy(&bare_output.stdout).into_owned();
    assert!(bare_output.status.success(), "{}", combined(&bare_output));
    assert!(
        bare_stdout.contains("boop-start: no recipe in "),
        "{}",
        combined(&bare_output)
    );
}

#[test]
fn agent_register_with_a_worktree_prints_the_preamble_to_its_own_stdout() {
    let fixture = DryRunFixture::new("register", true);
    let output = Command::new(env!("CARGO_BIN_EXE_boop"))
        .env_clear()
        .env("HOME", &fixture.root)
        .env("BOOP_DB", fixture.root.join("boop.db"))
        .env("XDG_CONFIG_HOME", fixture.root.join("config"))
        // The recipe's own body needs the system utilities, not just `just`.
        .env("PATH", format!("{}:/usr/bin:/bin", fixture.bin.display()))
        .args([
            "beep",
            "agent",
            "register",
            "native-warm",
            "--kind",
            "native",
        ])
        .arg("--worktree")
        .arg(&fixture.repo)
        .arg("--mail-dir")
        .arg(&fixture.mail)
        .output()
        .expect("run boop");
    assert!(output.status.success(), "{}", combined(&output));
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        stdout.contains("boop-start: ready in"),
        "{}",
        combined(&output)
    );
    assert!(
        stdout.contains(
            "setup is done; do not run installs or builds to get started; \
             build only what you change."
        ),
        "{}",
        combined(&output)
    );
}
