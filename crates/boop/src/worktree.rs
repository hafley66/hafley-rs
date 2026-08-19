//! Spawn directory preparation: the worktree gap. A spawn makes a worktree by
//! default (branch at the stated base sha) and refuses a non-fast-forward;
//! working in the main tree requires `main_tree: true`.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use wait_timeout::ChildExt;

use crate::harness::SpawnSpec;

/// Every spawn-path child (`just --dump`, `just boop-start`, the setup shell)
/// dies on this one deadline; a hang here must not hang the whole spawn.
const SPAWN_CHILD_TIMEOUT: Duration = Duration::from_secs(120);

/// A spawn-path child ran past `SPAWN_CHILD_TIMEOUT`; its whole process group
/// was SIGKILLed rather than left to hang the spawn.
#[derive(Debug)]
pub struct SpawnChildTimedOut {
    what: &'static str,
    seconds: u64,
}

impl std::fmt::Display for SpawnChildTimedOut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} exceeded the {}s spawn deadline and was killed",
            self.what, self.seconds
        )
    }
}

impl std::error::Error for SpawnChildTimedOut {}

/// Create the directory a spawn runs in and run its setup steps. Returns that
/// directory. In the main tree the repo is the working dir and must
/// fast-forward to the base sha; otherwise a worktree is created at
/// `worktree_dir` and its setup steps run there in order.
pub fn prepare_spawn_dir(spec: &SpawnSpec) -> Result<PathBuf> {
    if spec.main_tree {
        merge_ff_only(&spec.repo, &spec.base_sha)?;
        return Ok(spec.repo.clone());
    }
    let Some(worktree) = spec.worktree_dir.clone() else {
        anyhow::bail!("worktree spawn requires a worktree_dir");
    };
    if worktree.exists() {
        anyhow::bail!(
            "worktree path already exists: {}\n\
             a dead lane left it behind; respawn with --reclaim, or `boop beep lane delete {}`",
            worktree.display(),
            spec.lane
        );
    }
    if let Some(parent) = worktree.parent() {
        std::fs::create_dir_all(parent).context("create worktree parent")?;
    }
    run_git(
        &spec.repo,
        &[
            "worktree",
            "add",
            "-b",
            &spec.branch,
            &worktree.display().to_string(),
            &spec.base_sha,
        ],
    )?;
    merge_ff_only(&worktree, &spec.base_sha)?;
    if spec.warm_start {
        let outcome = warm_start(&worktree)?;
        println!("{}", outcome.status);
        crate::lane::record_start_status(&spec.mail_dir, &spec.lane, &outcome.status)?;
    }
    for command in &spec.setup {
        run_shell(&worktree, command)?;
    }
    Ok(worktree)
}

/// The recipe every repo boop spawns into is expected to declare.
pub const START_RECIPE: &str = "boop-start";

/// A tree's `boop-start` recipe and the justfile that declares it.
#[derive(Clone, Debug)]
pub struct StartRecipe {
    pub justfile: PathBuf,
}

/// What the warm-up did, in the one line the spawned agent reads about setup.
#[derive(Clone, Debug)]
pub struct StartOutcome {
    /// False when the tree declares no recipe; the status line says so.
    pub ran: bool,
    pub status: String,
}

/// Run the repo's `boop-start` recipe in a fresh worktree. A tree without one
/// is not warmed and says so in one line, which is not an error.
///
/// A failure BLOCKS the spawn. The recipe exists because a worktree missing the
/// pre-commit hook's inputs makes every `git commit` abort, and a lane reads
/// that abort as success; warning instead would reproduce the loss it prevents.
pub fn warm_start(worktree: &Path) -> Result<StartOutcome> {
    let Some(recipe) = find_start_recipe(worktree)? else {
        let status = format!(
            "boop-start: no recipe in {}, nothing to warm",
            worktree.display()
        );
        return Ok(StartOutcome { ran: false, status });
    };
    let started = std::time::Instant::now();
    let mut cmd = Command::new("just");
    cmd.arg(START_RECIPE).current_dir(worktree);
    let output = run_captured_with_deadline(cmd, "just boop-start", SPAWN_CHILD_TIMEOUT)?;
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        println!("  {line}");
    }
    if !output.status.success() {
        anyhow::bail!(
            "just boop-start failed in {}: {}\n\
             the pre-commit hook needs it, so a lane spawned now could not commit; \
             fix it or respawn with --no-start",
            worktree.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let status = format!(
        "boop-start: ready in {:.1}s ({})",
        started.elapsed().as_secs_f64(),
        recipe_summary(&text, &recipe)
    );
    Ok(StartOutcome { ran: true, status })
}

/// The recipe's own summary, one line. A recipe that printed nothing is named
/// by its justfile instead, so the status line never reads as empty parentheses.
fn recipe_summary(stdout: &str, recipe: &StartRecipe) -> String {
    let summary = stdout
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("; ");
    if summary.is_empty() {
        format!("{} printed no summary", recipe.justfile.display())
    } else {
        summary
    }
}

/// The `boop-start` recipe this tree declares; `just --dump` names the recipes
/// and their justfile in one child. Absent `just` or dump reads as no recipe.
pub fn find_start_recipe(dir: &Path) -> Result<Option<StartRecipe>> {
    let mut cmd = Command::new("just");
    cmd.args(["--dump", "--dump-format", "json"])
        .current_dir(dir);
    let output = match run_captured_with_deadline(cmd, "just --dump", SPAWN_CHILD_TIMEOUT) {
        Ok(output) if output.status.success() => output,
        Ok(_) => return Ok(None),
        Err(err) if err.downcast_ref::<SpawnChildTimedOut>().is_some() => return Err(err),
        Err(_) => return Ok(None),
    };
    let Ok(dump) = serde_json::from_slice::<JustDump>(&output.stdout) else {
        return Ok(None);
    };
    if !dump.recipes.contains_key(START_RECIPE) {
        return Ok(None);
    }
    Ok(Some(StartRecipe {
        justfile: PathBuf::from(dump.source),
    }))
}

/// The two fields `just --dump --dump-format json` is read for; every recipe
/// body is discarded on the way past.
#[derive(serde::Deserialize)]
struct JustDump {
    source: String,
    recipes: std::collections::BTreeMap<String, serde::de::IgnoredAny>,
}

/// Put `cmd` in its own process group so a timeout kill takes any grandchildren
/// with it, then spawn it.
fn spawn_grouped(cmd: &mut Command) -> Result<std::process::Child> {
    cmd.process_group(0);
    cmd.spawn().context("spawn child")
}

/// `kill -9` the whole group `pid` leads, since `pid` was made its own group
/// leader by `spawn_grouped`.
fn kill_process_group(pid: i32) {
    let _ = Command::new("kill")
        .args(["-9", &format!("-{pid}")])
        .status();
}

fn read_all(pipe: Option<impl std::io::Read>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_end(&mut buf);
    }
    buf
}

/// Wait on `child` up to `deadline`; past it, kill the group and return
/// `SpawnChildTimedOut` instead of leaving the caller to hang.
fn wait_with_deadline(
    mut child: std::process::Child,
    what: &'static str,
    deadline: Duration,
) -> Result<std::process::ExitStatus> {
    let pid = child.id() as i32;
    match child
        .wait_timeout(deadline)
        .with_context(|| format!("wait on {what}"))?
    {
        Some(status) => Ok(status),
        None => {
            kill_process_group(pid);
            let _ = child.wait();
            Err(SpawnChildTimedOut {
                what,
                seconds: deadline.as_secs(),
            }
            .into())
        }
    }
}

/// Deadline-bounded equivalent of `Command::output`: stdout/stderr are read on
/// their own threads so a full pipe can never block the deadline wait, and
/// stdin is null as `Command::output` makes it, so no captured child can sit
/// on a prompt the spawn has no terminal to answer.
fn run_captured_with_deadline(
    mut cmd: Command,
    what: &'static str,
    deadline: Duration,
) -> Result<std::process::Output> {
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = spawn_grouped(&mut cmd)?;
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stdout_reader = std::thread::spawn(move || read_all(stdout_pipe));
    let stderr_reader = std::thread::spawn(move || read_all(stderr_pipe));
    let status = match wait_with_deadline(child, what, deadline) {
        Ok(status) => status,
        Err(err) => {
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(err);
        }
    };
    let stdout = stdout_reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

/// Deadline-bounded equivalent of `Command::status`: stdio stays inherited, so
/// the setup step's own output still streams live.
fn run_status_with_deadline(
    mut cmd: Command,
    what: &'static str,
    deadline: Duration,
) -> Result<std::process::ExitStatus> {
    let child = spawn_grouped(&mut cmd)?;
    wait_with_deadline(child, what, deadline)
}

/// `git -C repo merge --ff-only <sha>`; a non-fast-forward is an error.
fn merge_ff_only(repo: &PathBuf, sha: &str) -> Result<()> {
    run_git(repo, &["merge", "--ff-only", sha])
}

/// The spawn path's git children (`worktree add`, `merge --ff-only`) share the
/// one child deadline: a stale `index.lock` would otherwise hang the whole
/// spawn with nothing to kill it.
fn run_git(repo: &PathBuf, args: &[&str]) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(args);
    let output = run_captured_with_deadline(cmd, "git", SPAWN_CHILD_TIMEOUT)
        .with_context(|| format!("run git in {}", repo.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            repo.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Run one setup step (a shell command) inside `cwd`.
fn run_shell(cwd: &PathBuf, command: &str) -> Result<()> {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command).current_dir(cwd);
    let status = run_status_with_deadline(cmd, "sh -c <setup>", SPAWN_CHILD_TIMEOUT)?;
    if !status.success() {
        anyhow::bail!("setup step failed in {}: {command}", cwd.display());
    }
    Ok(())
}

/// What a reclaim removed, so the caller prints exactly what it destroyed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reclaimed {
    pub worktree: Option<PathBuf>,
    pub branch: Option<String>,
}

impl Reclaimed {
    /// True when the lane name was already free.
    pub fn nothing_removed(&self) -> bool {
        self.worktree.is_none() && self.branch.is_none()
    }

    /// One line per removed thing, in removal order.
    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(worktree) = &self.worktree {
            lines.push(format!("removed worktree {}", worktree.display()));
        }
        if let Some(branch) = &self.branch {
            lines.push(format!("removed branch {branch}"));
        }
        lines
    }
}

/// Remove a dead lane's worktree and branch so the name spawns again. Liveness
/// is the caller's; this refuses only on work git cannot get back.
pub fn reclaim_carcass(repo: &Path, branch: &str, worktree: &Path) -> Result<Reclaimed> {
    let mut removed = Reclaimed::default();
    if worktree.exists() {
        if let Some(dirt) = worktree_dirt(worktree) {
            anyhow::bail!(
                "worktree {} has uncommitted work, refusing to remove it:\n{dirt}",
                worktree.display()
            );
        }
    }
    let branch_present = branch_exists(repo, branch);
    if branch_present {
        if let Some(commits) = unique_commits(repo, branch) {
            anyhow::bail!(
                "branch {branch} carries commits no other ref has, refusing to delete it:\n{commits}"
            );
        }
    }
    if worktree.exists() {
        // --force, since a worktree carrying only ignored build output is
        // clean by `status` and still refused by a plain `worktree remove`.
        run_git(
            &repo.to_path_buf(),
            &[
                "worktree",
                "remove",
                "--force",
                &worktree.display().to_string(),
            ],
        )?;
        removed.worktree = Some(worktree.to_path_buf());
    }
    // A worktree directory deleted by hand leaves an admin entry that keeps
    // `worktree add` refusing the same path.
    run_git(&repo.to_path_buf(), &["worktree", "prune"])?;
    if branch_present {
        run_git(&repo.to_path_buf(), &["branch", "-D", branch])?;
        removed.branch = Some(branch.to_owned());
    }
    Ok(removed)
}

/// `git status --porcelain` output when the worktree holds changes, `None`
/// when it is clean or is not a worktree at all.
fn worktree_dirt(worktree: &Path) -> Option<String> {
    let status = git_stdout(worktree, &["status", "--porcelain"])?;
    (!status.is_empty()).then_some(status)
}

/// True when `refs/heads/<branch>` resolves in `repo`.
pub fn branch_exists(repo: &Path, branch: &str) -> bool {
    git_stdout(
        repo,
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_some()
}

/// The commits on `branch` that no other ref contains, `None` when every
/// commit on it is reachable from somewhere else.
fn unique_commits(repo: &Path, branch: &str) -> Option<String> {
    let reference = format!("refs/heads/{branch}");
    // Without --single-worktree, --all reads every linked worktree's HEAD,
    // including the one being reclaimed, so nothing ever looks unique.
    let log = git_stdout(
        repo,
        &[
            "log",
            "--oneline",
            "--max-count=20",
            "--single-worktree",
            &reference,
            "--not",
            "--exclude",
            &reference,
            "--all",
        ],
    )?;
    (!log.is_empty()).then_some(log)
}

/// When one lane held the shared main tree, and on which branch. Attribution
/// compares a commit against every concurrent lane's window, so one lane's
/// commits never land on another lane's row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaneWindow {
    pub lane: String,
    /// The branch checked out in the lane's worktree; `None` when git could not
    /// answer, which makes reachability say nothing about this lane.
    pub branch: Option<String>,
    /// Author time (unix seconds) at the lane's spawn.
    pub start_secs: i64,
    /// Author time the lane reported its result; `None` while it still runs.
    pub end_secs: Option<i64>,
}

impl LaneWindow {
    /// Author time inside the lane's run. The upper bound is open while the
    /// lane runs, and closed the moment it reported.
    fn holds(&self, authored_secs: i64) -> bool {
        authored_secs >= self.start_secs && self.end_secs.is_none_or(|end| authored_secs <= end)
    }
}

/// One main-tree commit no single lane's window claims alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmbiguousCommit {
    pub sha: String,
    /// Every lane whose window holds it, this one included.
    pub lanes: Vec<String>,
}

/// The flags a `lane wait`/`lane list` prints when a lane's commits landed
/// outside its registered worktree. Detection only: the coordinator reads these
/// and decides, so nothing here resets or hard-fails.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EscapeFlags {
    /// The registered worktree has zero new commits: its HEAD still equals the
    /// base sha the spawn branched from.
    pub worktree_untouched: bool,
    /// Shas on the repo's local `main`, not on `origin/main`, that only this
    /// lane's window and branch reachability account for.
    pub main_commits: Vec<String>,
    /// Shas a concurrent lane's window holds too. Naming one lane here would be
    /// a guess, and a guess is what sent lane extract-module-plane-go the
    /// commits lane dl6-bytes-target-lowering-2 made.
    pub ambiguous_main_commits: Vec<AmbiguousCommit>,
}

impl EscapeFlags {
    pub fn is_empty(&self) -> bool {
        !self.worktree_untouched
            && self.main_commits.is_empty()
            && self.ambiguous_main_commits.is_empty()
    }
}

/// Compare a lane's worktree HEAD against the base sha the spawn recorded, and
/// scan the repo's local `main` for commits the lane may have left there. A git
/// failure is never an error: the rail only detects, it never blocks.
///
/// `siblings` is every other lane registered against the same repo; without it
/// the scan cannot tell two lanes sharing one main tree apart.
pub fn detect_escape(
    worktree: &Path,
    repo: &Path,
    base_sha: &str,
    run: &LaneWindow,
    siblings: &[LaneWindow],
) -> EscapeFlags {
    let worktree_untouched = match (git_stdout(worktree, &["rev-parse", "HEAD"]), base_sha) {
        (Some(head), base) if !base.is_empty() => head == base,
        _ => false,
    };
    let mut flags = EscapeFlags {
        worktree_untouched,
        ..EscapeFlags::default()
    };
    for (sha, authored_secs) in local_main_commits(repo) {
        match attribute(repo, &sha, authored_secs, run, siblings) {
            Attribution::Own => flags.main_commits.push(sha),
            Attribution::Shared(lanes) => flags
                .ambiguous_main_commits
                .push(AmbiguousCommit { sha, lanes }),
            Attribution::Elsewhere => {}
        }
    }
    flags
}

/// Which lane a main-tree commit belongs to.
enum Attribution {
    /// This lane's branch reaches it, or this lane's window alone holds it.
    Own,
    /// More than one concurrent lane's window holds it; the named lanes.
    Shared(Vec<String>),
    /// Another lane's branch reaches it, or no lane's window holds it.
    Elsewhere,
}

/// Attribute one sha. Reachability decides first: a branch that contains the
/// commit witnessed it, and the author-time window is only a guess. Only where
/// no branch but `main` reaches the commit does the window speak, and a window
/// two lanes share names both instead of picking one.
///
/// Reachability reads git, not the lane roster, so a pruned sibling route still
/// clears this lane.
fn attribute(
    repo: &Path,
    sha: &str,
    authored_secs: i64,
    run: &LaneWindow,
    siblings: &[LaneWindow],
) -> Attribution {
    let containing = branches_containing(repo, sha);
    let reaches = |window: &LaneWindow| {
        window
            .branch
            .as_deref()
            .is_some_and(|branch| containing.iter().any(|name| name == branch))
    };
    if reaches(run) {
        return Attribution::Own;
    }
    if !containing.is_empty() {
        return Attribution::Elsewhere;
    }
    if !run.holds(authored_secs) {
        return Attribution::Elsewhere;
    }
    let mut lanes: Vec<String> = siblings
        .iter()
        .filter(|window| window.lane != run.lane && window.holds(authored_secs))
        .map(|window| window.lane.clone())
        .collect();
    if lanes.is_empty() {
        return Attribution::Own;
    }
    lanes.push(run.lane.clone());
    lanes.sort();
    Attribution::Shared(lanes)
}

/// Every `(sha, author time)` on the repo's local `main` that `origin/main`
/// does not carry. Author time, not commit time: a rebase or a cherry-pick
/// rewrites the commit time and would move the commit out of its lane's run.
fn local_main_commits(repo: &Path) -> Vec<(String, i64)> {
    let Some(output) = git_stdout(repo, &["log", "origin/main..main", "--format=%H %at"]) else {
        return Vec::new();
    };
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let sha = parts.next()?.to_owned();
            let authored = parts.next()?.parse::<i64>().ok()?;
            Some((sha, authored))
        })
        .collect()
}

/// The local branches whose tip reaches `sha`. `main` itself is dropped: every
/// main-tree commit is on main by construction, so it separates no two lanes.
fn branches_containing(repo: &Path, sha: &str) -> Vec<String> {
    let Some(output) = git_stdout(
        repo,
        &["branch", "--contains", sha, "--format=%(refname:short)"],
    ) else {
        return Vec::new();
    };
    output
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty() && !name.starts_with('('))
        .filter(|name| !matches!(*name, "main" | "master"))
        .map(str::to_owned)
        .collect()
}

/// The branch checked out in a lane's worktree, for `LaneWindow::branch`.
pub fn current_branch(worktree: &Path) -> Option<String> {
    git_stdout(worktree, &["rev-parse", "--abbrev-ref", "HEAD"])
        .filter(|name| !name.is_empty() && name != "HEAD")
}

/// The trimmed stdout of `git -C repo <args>`, or `None` when git fails.
fn git_stdout(repo: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    /// A repo with no justfile needs no warmup, so the spawn path stays quiet
    /// instead of failing on a recipe nobody declared.
    #[test]
    fn warm_start_is_a_no_op_where_no_recipe_is_declared() {
        let dir = std::env::temp_dir().join(format!("boop-nostart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(super::find_start_recipe(&dir).unwrap().is_none());
        let outcome = super::warm_start(&dir).expect("no recipe means no work");
        assert!(!outcome.ran);
        assert!(
            outcome.status.contains("nothing to warm"),
            "{}",
            outcome.status
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A declared recipe is found and run, and its failure blocks the spawn.
    #[test]
    fn a_failing_boop_start_blocks_the_spawn() {
        let dir = std::env::temp_dir().join(format!("boop-badstart-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("justfile"), "boop-start:\n    @exit 3\n").unwrap();
        assert!(super::find_start_recipe(&dir).unwrap().is_some());
        let error = super::warm_start(&dir).unwrap_err().to_string();
        assert!(error.contains("could not commit"), "{error}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // RECEIPT: before this deadline, `sh -c 'sleep 999'` under `.status()`
    // blocked forever; nothing in the spawn path ever returned.
    #[test]
    fn a_hung_setup_step_fails_within_its_deadline_instead_of_hanging() {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg("sleep 999");
        let started = std::time::Instant::now();
        let result = super::run_status_with_deadline(
            cmd,
            "sh -c <setup>",
            std::time::Duration::from_secs(2),
        );
        let elapsed = started.elapsed();
        let error = result.unwrap_err();
        assert!(
            error.downcast_ref::<super::SpawnChildTimedOut>().is_some(),
            "{error}"
        );
        assert!(elapsed < std::time::Duration::from_secs(10), "{elapsed:?}");
    }

    // RECEIPT: run from a terminal against an inherited stdin, `sh -c cat`
    // read the pane instead of seeing EOF and the call sat there until the
    // deadline killed it.
    #[test]
    fn a_captured_child_reads_eof_instead_of_a_prompt() {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg("cat");
        let output = super::run_captured_with_deadline(
            cmd,
            "sh -c <setup>",
            std::time::Duration::from_secs(5),
        )
        .expect("a closed stdin ends `cat` at once");
        assert!(output.status.success(), "{:?}", output.status);
        assert!(output.stdout.is_empty(), "{:?}", output.stdout);
    }

    /// The killed child's whole process group dies too, not just the `sh` pid:
    /// a grandchild started after the kill signal proves nothing survived.
    #[test]
    fn the_killed_child_leaves_no_orphan() {
        let marker = std::env::temp_dir().join(format!(
            "boop-orphan-check-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::remove_file(&marker);
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(format!(
            "sleep 3 && touch {} && sleep 999",
            marker.display()
        ));
        let result = super::run_status_with_deadline(
            cmd,
            "sh -c <setup>",
            std::time::Duration::from_secs(1),
        );
        assert!(result.is_err());
        std::thread::sleep(std::time::Duration::from_secs(4));
        assert!(
            !marker.exists(),
            "the grandchild ran after the kill, so the group survived it"
        );
    }

    use std::process::Command;

    use crate::harness::SpawnSpec;

    use super::prepare_spawn_dir;

    /// The escape rail names `origin/main..main`, so the fixture pins the
    /// branch instead of taking `init.defaultBranch`. A runner that defaults to
    /// `master` made every `EscapeRepo` test read an empty commit list.
    fn init_repo(path: &std::path::Path) -> String {
        Command::new("git")
            .args(["init", "-q", "-b", "main"])
            .arg(path)
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                path.to_string_lossy().as_ref(),
                "config",
                "user.email",
                "t@t",
            ])
            .status()
            .unwrap();
        Command::new("git")
            .args([
                "-C",
                path.to_string_lossy().as_ref(),
                "config",
                "user.name",
                "t",
            ])
            .status()
            .unwrap();
        std::fs::write(path.join("seed.txt"), "seed").unwrap();
        let out = Command::new("git")
            .args(["-C", path.to_string_lossy().as_ref(), "add", "-A"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let out = Command::new("git")
            .args([
                "-C",
                path.to_string_lossy().as_ref(),
                "commit",
                "-qm",
                "seed",
            ])
            .output()
            .unwrap();
        assert!(out.status.success());
        let out = Command::new("git")
            .args(["-C", path.to_string_lossy().as_ref(), "rev-parse", "HEAD"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn spec(
        repo: &std::path::Path,
        base: &str,
        worktree: &std::path::Path,
        main_tree: bool,
    ) -> SpawnSpec {
        SpawnSpec {
            harness: "claude".to_owned(),
            branch: "lane-wt".to_owned(),
            base_sha: base.to_owned(),
            main_tree,
            setup: Vec::new(),
            prompt: "do the lane".to_owned(),
            resume_session: None,
            socket: None,
            worktree_dir: Some(worktree.to_path_buf()),
            repo: repo.to_path_buf(),
            env_stamp: None,
            model: None,
            variant: None,
            on_exit: None,
            tmux: None,
            lane: "lane-test".to_owned(),
            mail_dir: std::env::temp_dir(),
            warm_start: false,
        }
    }

    #[test]
    fn worktree_spawn_creates_a_branch_at_the_base() {
        let base = std::env::temp_dir().join(format!("boop-wt-{}-repo", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let sha = init_repo(&base);
        let wt = base.with_file_name(format!("boop-wt-{}-work", std::process::id()));
        let _ = std::fs::remove_dir_all(&wt);
        let dir = prepare_spawn_dir(&spec(&base, &sha, &wt, false)).unwrap();
        assert!(
            dir.join("seed.txt").exists(),
            "worktree must carry the seed commit"
        );
        let branch = String::from_utf8_lossy(
            &Command::new("git")
                .args([
                    "-C",
                    &wt.display().to_string(),
                    "rev-parse",
                    "--abbrev-ref",
                    "HEAD",
                ])
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_owned();
        assert_eq!(branch, "lane-wt");
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&wt);
    }

    #[test]
    fn setup_steps_run_in_order_in_the_worktree() {
        let base = std::env::temp_dir().join(format!("boop-st-{}-repo", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let sha = init_repo(&base);
        let wt = base.with_file_name(format!("boop-st-{}-work", std::process::id()));
        let _ = std::fs::remove_dir_all(&wt);
        let mut s = spec(&base, &sha, &wt, false);
        s.setup = vec![
            "echo first > marker.txt".to_owned(),
            "echo second >> marker.txt".to_owned(),
        ];
        let dir = prepare_spawn_dir(&s).unwrap();
        let marker = std::fs::read_to_string(dir.join("marker.txt")).unwrap();
        assert_eq!(
            marker, "first\nsecond\n",
            "setup steps run in order in the worktree"
        );
        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&wt);
    }

    #[test]
    fn main_tree_spawn_refuses_a_non_fast_forward() {
        let base = std::env::temp_dir().join(format!("boop-ff-{}-repo", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let sha0 = init_repo(&base);
        let path = base.to_string_lossy().to_string();
        // Move HEAD forward on the default branch: commit A on top of sha0.
        std::fs::write(base.join("a.txt"), "a").unwrap();
        Command::new("git")
            .args(["-C", &path, "add", "-A"])
            .status()
            .unwrap();
        Command::new("git")
            .args(["-C", &path, "commit", "-qm", "a"])
            .status()
            .unwrap();
        let main_branch = String::from_utf8_lossy(
            &Command::new("git")
                .args(["-C", &path, "rev-parse", "--abbrev-ref", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_owned();
        // Diverge on a side branch: commit B on top of sha0, then return HEAD.
        Command::new("git")
            .args(["-C", &path, "branch", "other", &sha0])
            .status()
            .unwrap();
        Command::new("git")
            .args(["-C", &path, "checkout", "-q", "other"])
            .status()
            .unwrap();
        std::fs::write(base.join("b.txt"), "b").unwrap();
        Command::new("git")
            .args(["-C", &path, "add", "-A"])
            .status()
            .unwrap();
        Command::new("git")
            .args(["-C", &path, "commit", "-qm", "b"])
            .status()
            .unwrap();
        let divergent = String::from_utf8_lossy(
            &Command::new("git")
                .args(["-C", &path, "rev-parse", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_owned();
        Command::new("git")
            .args(["-C", &path, "checkout", "-q", &main_branch])
            .status()
            .unwrap();
        // Pin a main-tree spawn to the diverged commit: it cannot fast-forward.
        let result = prepare_spawn_dir(&spec(&base, &divergent, &base.join("nope"), true));
        assert!(
            result.is_err(),
            "non-fast-forward main-tree spawn must be refused"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    fn git(repo: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A repo on `main` with a seed commit exposed as `origin/main`, plus a
    /// linked worktree checked out at the seed (the base) on `feature/x`.
    struct EscapeRepo {
        repo: std::path::PathBuf,
        worktree: std::path::PathBuf,
        base: String,
    }

    impl EscapeRepo {
        fn new(tag: &str) -> EscapeRepo {
            let root =
                std::env::temp_dir().join(format!("boop-escape-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let repo = root.join("repo");
            let worktree = root.join("wt");
            std::fs::create_dir_all(&repo).unwrap();
            let base = init_repo(&repo);
            // Expose the seed as origin/main, the default base a spawn reads.
            git(&repo, &["update-ref", "refs/remotes/origin/main", &base]);
            git(
                &repo,
                &[
                    "worktree",
                    "add",
                    "-q",
                    "-b",
                    "feature/x",
                    &worktree.display().to_string(),
                    &base,
                ],
            );
            EscapeRepo {
                repo,
                worktree,
                base,
            }
        }
    }

    impl Drop for EscapeRepo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.repo.parent().unwrap());
        }
    }

    /// The lane the `EscapeRepo` worktree belongs to, running since the epoch
    /// and still open.
    fn window(repo: &EscapeRepo, lane: &str) -> super::LaneWindow {
        super::LaneWindow {
            lane: lane.to_owned(),
            branch: super::current_branch(&repo.worktree),
            start_secs: 0,
            end_secs: None,
        }
    }

    /// RECEIPT. A lane that made no commit in its worktree leaves the worktree
    /// HEAD on the base sha; the rail flags it untouched.
    #[test]
    fn an_untouched_worktree_is_flagged() {
        let repo = EscapeRepo::new("untouched");
        let flags = super::detect_escape(
            &repo.worktree,
            &repo.repo,
            &repo.base,
            &window(&repo, "mine"),
            &[],
        );
        assert!(flags.worktree_untouched, "flags: {flags:?}");
        assert!(flags.main_commits.is_empty(), "flags: {flags:?}");
    }

    /// RECEIPT. A commit on the repo's local main (not on origin/main) during
    /// the lane's run is listed as a main-tree suspect; the untouched worktree
    /// is the pairing the escape issue reported.
    #[test]
    fn a_local_main_commit_is_flagged_suspect() {
        let repo = EscapeRepo::new("main-commit");
        std::fs::write(repo.repo.join("escape.txt"), "escaped").unwrap();
        git(&repo.repo, &["add", "-A"]);
        git(&repo.repo, &["commit", "-qm", "escaped to main"]);
        let flags = super::detect_escape(
            &repo.worktree,
            &repo.repo,
            &repo.base,
            &window(&repo, "mine"),
            &[],
        );
        assert_eq!(flags.main_commits.len(), 1, "flags: {flags:?}");
        assert!(flags.worktree_untouched, "flags: {flags:?}");
    }

    /// RECEIPT. A lane that committed inside its worktree and left main alone
    /// prints neither flag.
    #[test]
    fn a_proper_worktree_commit_prints_no_flag() {
        let repo = EscapeRepo::new("proper");
        std::fs::write(repo.worktree.join("done.txt"), "done").unwrap();
        git(&repo.worktree, &["add", "-A"]);
        git(&repo.worktree, &["commit", "-qm", "work in the worktree"]);
        let flags = super::detect_escape(
            &repo.worktree,
            &repo.repo,
            &repo.base,
            &window(&repo, "mine"),
            &[],
        );
        assert!(!flags.worktree_untouched, "flags: {flags:?}");
        assert!(flags.main_commits.is_empty(), "flags: {flags:?}");
    }

    // FAIL-PRE-FIX: attribution was a one-sided commit-time window with no lane
    // binding at all, so lane extract-module-plane-go (window opened 03:48:44Z)
    // was printed as the suspect for cd71912cd and 36f56f008 (authored 03:57:58Z
    // and 03:49:19Z), which lane dl6-bytes-target-lowering-2 made in the shared
    // sprefa main tree; `git branch --contains cd71912cd` names that lane's
    // branch and not the go lane's. The shape below is that incident: a wide
    // open window over a commit another lane's branch reaches.
    // SABOTAGE RECEIPT: drop the `!containing.is_empty()` arm from `attribute`
    // and the first assertion fails with the sibling's commit on this row.
    #[test]
    fn a_sibling_lane_s_main_commit_is_not_this_lane_s() {
        let repo = EscapeRepo::new("sibling");
        std::fs::write(repo.repo.join("escape.txt"), "escaped").unwrap();
        git(&repo.repo, &["add", "-A"]);
        git(&repo.repo, &["commit", "-qm", "escaped to main"]);
        // The sibling lane's branch witnesses the commit, the same way
        // feature/dl6-bytes-target-lowering-2 does in sprefa.
        git(&repo.repo, &["branch", "feature/sibling", "main"]);
        let mine = super::LaneWindow {
            lane: "mine".to_owned(),
            branch: super::current_branch(&repo.worktree),
            start_secs: 0,
            end_secs: None,
        };
        let sibling = super::LaneWindow {
            lane: "sibling".to_owned(),
            branch: Some("feature/sibling".to_owned()),
            start_secs: 0,
            end_secs: None,
        };
        let flags = super::detect_escape(
            &repo.worktree,
            &repo.repo,
            &repo.base,
            &mine,
            std::slice::from_ref(&sibling),
        );
        assert!(
            flags.main_commits.is_empty(),
            "a commit another lane's branch reaches is not this lane's: {flags:?}"
        );
        assert!(
            flags.ambiguous_main_commits.is_empty(),
            "reachability settles it, so nothing is ambiguous: {flags:?}"
        );
        // The lane whose branch does reach it still gets it, roster or no roster.
        let flags = super::detect_escape(&repo.worktree, &repo.repo, &repo.base, &sibling, &[]);
        assert_eq!(flags.main_commits.len(), 1, "flags: {flags:?}");
    }

    // FAIL-PRE-FIX: two lanes alive in one main tree both got the same sha
    // printed as their own suspect, with nothing saying the call was a guess.
    // SABOTAGE RECEIPT: return `Attribution::Own` in place of
    // `Attribution::Shared(lanes)` and the ambiguous set stays empty here.
    #[test]
    fn two_overlapping_lanes_make_a_commit_ambiguous_instead_of_blaming_one() {
        let repo = EscapeRepo::new("overlap");
        std::fs::write(repo.repo.join("escape.txt"), "escaped").unwrap();
        git(&repo.repo, &["add", "-A"]);
        git(&repo.repo, &["commit", "-qm", "escaped to main"]);
        let mine = window(&repo, "mine");
        let sibling = super::LaneWindow {
            lane: "sibling".to_owned(),
            branch: Some("feature/sibling".to_owned()),
            start_secs: 0,
            end_secs: None,
        };
        let flags = super::detect_escape(
            &repo.worktree,
            &repo.repo,
            &repo.base,
            &mine,
            std::slice::from_ref(&sibling),
        );
        assert!(flags.main_commits.is_empty(), "flags: {flags:?}");
        assert_eq!(flags.ambiguous_main_commits.len(), 1, "flags: {flags:?}");
        assert_eq!(
            flags.ambiguous_main_commits[0].lanes,
            vec!["mine".to_owned(), "sibling".to_owned()]
        );
    }

    /// A commit the lane's own branch reaches is the lane's whatever the clock
    /// says; reachability is a witness and the window is a guess.
    /// SABOTAGE RECEIPT: drop the `if reaches(run)` arm from `attribute` and
    /// the out-of-window commit stops being attributed at all.
    #[test]
    fn branch_reachability_beats_the_time_window() {
        let repo = EscapeRepo::new("reach");
        std::fs::write(repo.worktree.join("done.txt"), "done").unwrap();
        git(&repo.worktree, &["add", "-A"]);
        git(&repo.worktree, &["commit", "-qm", "work in the worktree"]);
        let head = String::from_utf8_lossy(
            &Command::new("git")
                .args([
                    "-C",
                    &repo.worktree.display().to_string(),
                    "rev-parse",
                    "HEAD",
                ])
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_owned();
        // Fast-forward main onto the lane's branch: the commit is now on main
        // and out of every window, and still reachable from feature/x.
        git(&repo.repo, &["merge", "--ff-only", &head]);
        let mut mine = window(&repo, "mine");
        mine.start_secs = i64::MAX;
        let flags = super::detect_escape(&repo.worktree, &repo.repo, &repo.base, &mine, &[]);
        assert_eq!(flags.main_commits, vec![head], "flags: {flags:?}");
    }

    /// RECEIPT. A carcass sitting at its base sha costs one call to clear, and
    /// the return says exactly what was destroyed.
    #[test]
    fn a_reclaim_clears_a_clean_worktree_and_its_branch() {
        let repo = EscapeRepo::new("reclaim-clean");
        let removed = super::reclaim_carcass(&repo.repo, "feature/x", &repo.worktree).unwrap();
        assert_eq!(removed.worktree.as_deref(), Some(repo.worktree.as_path()));
        assert_eq!(removed.branch.as_deref(), Some("feature/x"));
        assert_eq!(
            removed.lines().len(),
            2,
            "one line per removed thing: {:?}",
            removed.lines()
        );
        assert!(!repo.worktree.exists());
        assert!(!super::branch_exists(&repo.repo, "feature/x"));
    }

    /// RECEIPT. A lane that committed before it died is not a carcass; the
    /// commit no other ref carries stops the reclaim before any removal.
    #[test]
    fn a_reclaim_refuses_a_branch_no_other_ref_carries() {
        let repo = EscapeRepo::new("reclaim-unique");
        std::fs::write(repo.worktree.join("lane.txt"), "work").unwrap();
        git(&repo.worktree, &["add", "-A"]);
        git(&repo.worktree, &["commit", "-qm", "lane work"]);
        let error = super::reclaim_carcass(&repo.repo, "feature/x", &repo.worktree)
            .unwrap_err()
            .to_string();
        assert!(error.contains("lane work"), "{error}");
        assert!(error.contains("no other ref has"), "{error}");
        assert!(
            repo.worktree.exists(),
            "the worktree survives the refused reclaim"
        );
        assert!(super::branch_exists(&repo.repo, "feature/x"));
    }
}
