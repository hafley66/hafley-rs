//! Stat-gated git HEAD watcher. It coalesces a burst of commits into one
//! `HeadMove` and reads the burst's commit trailers.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime};

use boop_acp::channel::{ToolCallFact, TOOL_STATUS_COMPLETED};
use tracing::warn;

/// Deadline on one git child. A hung git never blocks the supervisor's tick.
const GIT_TIMEOUT: Duration = Duration::from_secs(5);

/// A worker's own word for where the commit leaves its task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitStatus {
    Wip,
    Done,
    Blocked,
}

impl CommitStatus {
    /// The word that rides in a commit body and a message detail.
    pub fn as_str(&self) -> &'static str {
        match self {
            CommitStatus::Wip => "wip",
            CommitStatus::Done => "done",
            CommitStatus::Blocked => "blocked",
        }
    }

    /// The trailer's own word, defaulting to `wip` when absent or unknown.
    fn from_word(word: &str) -> CommitStatus {
        if word.eq_ignore_ascii_case("done") {
            CommitStatus::Done
        } else if word.eq_ignore_ascii_case("blocked") {
            CommitStatus::Blocked
        } else {
            CommitStatus::Wip
        }
    }
}

/// Everything a commit push body names about one burst of commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitFacts {
    pub old: String,
    pub new: String,
    pub count: u32,
    pub subject: String,
    pub status: CommitStatus,
    pub ask: Option<String>,
    pub check: Option<String>,
    pub dirty: usize,
}

/// One event HEAD produced, either a burst of commits or a rewind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeadMove {
    Advanced(CommitFacts),
    Rewound { old: String, new: String },
}

/// An open run of commits not yet quiet long enough to report.
struct Burst {
    first_old: String,
    latest: String,
    last_change: Instant,
    count: u32,
}

/// Watches a worktree's HEAD. The reflog mtime gates the git fork; `nudge`
/// forces a read on a reported command.
pub struct HeadWatch {
    reflog: PathBuf,
    last_mtime: Option<SystemTime>,
    reported: Option<String>,
    burst: Option<Burst>,
    quiet: Duration,
    nudged: bool,
}

impl HeadWatch {
    /// Watch `cwd`, seeded with the last head already reported to the parent.
    pub fn new(cwd: &Path, reported: Option<String>, quiet: Duration) -> Self {
        let reflog = reflog_path(cwd).unwrap_or_else(|| cwd.join(".git/logs/HEAD"));
        let last_mtime = stat_mtime(&reflog);
        // With nothing reported yet, the HEAD at start is the baseline; else
        // the first commit's tick would only seed `reported` and report nothing.
        let reported = reported.or_else(|| git_line(cwd, &["rev-parse", "HEAD"]));
        HeadWatch {
            reflog,
            last_mtime,
            reported,
            burst: None,
            quiet,
            nudged: false,
        }
    }

    /// Arm the next tick to read HEAD even when the reflog mtime is unchanged.
    pub fn nudge(&mut self) {
        self.nudged = true;
    }

    /// Read HEAD when the reflog moved or a nudge armed it, then emit a move
    /// whose burst has been quiet for `quiet`.
    pub fn tick(&mut self, cwd: &Path, now: Instant) -> Option<HeadMove> {
        let changed = self.nudged || stat_mtime(&self.reflog) != self.last_mtime;
        if changed {
            self.nudged = false;
            if let Some(head) = git_line(cwd, &["rev-parse", "HEAD"]) {
                self.last_mtime = stat_mtime(&self.reflog);
                let unchanged = match &self.burst {
                    Some(burst) => burst.latest == head,
                    None => self.reported.as_deref() == Some(head.as_str()),
                };
                if !unchanged {
                    match self.reported.clone() {
                        Some(prev) if !descends_from(cwd, &prev, &head) => {
                            self.reported = Some(head.clone());
                            self.burst = None;
                            return Some(HeadMove::Rewound {
                                old: prev,
                                new: head,
                            });
                        }
                        Some(prev) => {
                            let first_old = self
                                .burst
                                .as_ref()
                                .map(|b| b.first_old.clone())
                                .unwrap_or(prev);
                            let count = rev_list_count(cwd, &first_old, &head).unwrap_or(0);
                            self.burst = Some(Burst {
                                first_old,
                                latest: head,
                                last_change: now,
                                count,
                            });
                        }
                        None => self.reported = Some(head),
                    }
                }
            }
        }
        if let Some(burst) = &self.burst {
            if now.duration_since(burst.last_change) >= self.quiet {
                let burst = self.burst.take().unwrap();
                let facts = read_commit_facts(cwd, &burst.first_old, &burst.latest, burst.count);
                self.reported = Some(burst.latest);
                return Some(HeadMove::Advanced(facts));
            }
        }
        None
    }
}

/// Whether a completed tool call ran a git command that writes HEAD.
pub fn is_git_write(tool: &ToolCallFact) -> bool {
    if tool.status != TOOL_STATUS_COMPLETED {
        return false;
    }
    const VERBS: [&str; 7] = [
        "commit",
        "merge",
        "rebase",
        "cherry-pick",
        "reset",
        "am",
        "revert",
    ];
    let mut tokens = tool.title.split_whitespace();
    while let Some(token) = tokens.next() {
        if token == "git" {
            if let Some(verb) = tokens.next() {
                let verb = verb.trim_matches(|c: char| !c.is_alphanumeric() && c != '-');
                if VERBS.contains(&verb) {
                    return true;
                }
            }
        }
    }
    false
}

/// Read the latest commit's subject and Boop trailers plus the worktree's
/// dirty count over `old..new`.
pub fn read_commit_facts(cwd: &Path, old: &str, new: &str, count: u32) -> CommitFacts {
    let format = "%s%x00%(trailers:key=Boop-Status,valueonly)%x00%(trailers:key=Boop-Ask,valueonly)%x00%(trailers:key=Boop-Check,valueonly)%x00%B";
    let (subject, status_word, ask, check) =
        match git_output(cwd, &["log", "-1", &format!("--format={format}"), new]) {
            Some(out) if out.status.success() => {
                let text = String::from_utf8_lossy(&out.stdout).into_owned();
                let mut parts = text.split('\0');
                let subject = parts.next().unwrap_or("").trim().to_owned();
                let status_trailer = parts.next().unwrap_or("").trim().to_owned();
                let ask_trailer = parts.next().unwrap_or("").trim().to_owned();
                let check_trailer = parts.next().unwrap_or("").trim().to_owned();
                let body = parts.next().unwrap_or("");
                let pick = |trailer: String, key: &str| -> Option<String> {
                    let trailer = trailer.trim();
                    if trailer.is_empty() {
                        body_trailer(body, key)
                    } else {
                        Some(trailer.to_owned())
                    }
                };
                let status_word = pick(status_trailer, "Boop-Status").unwrap_or_default();
                let ask = pick(ask_trailer, "Boop-Ask");
                let check = pick(check_trailer, "Boop-Check");
                (subject, status_word, ask, check)
            }
            _ => (String::new(), String::new(), None, None),
        };
    let status = if burst_has_blocked(cwd, old, new) || status_word.eq_ignore_ascii_case("blocked")
    {
        CommitStatus::Blocked
    } else {
        CommitStatus::from_word(&status_word)
    };
    CommitFacts {
        old: old.to_owned(),
        new: new.to_owned(),
        count,
        subject,
        status,
        ask,
        check,
        dirty: dirty_count(cwd),
    }
}

/// The one-line commit push body plus its review command.
pub fn commit_body(lane: &str, worktree: &Path, facts: &CommitFacts) -> String {
    let ask = match (facts.status, facts.ask.as_deref()) {
        (CommitStatus::Blocked, Some(q)) => format!(" ask={q:?}"),
        _ => String::new(),
    };
    let check = match facts.check.as_deref() {
        Some(c) => format!(" check={c:?}"),
        None => String::new(),
    };
    format!(
        "commit {lane} {}..{} n={} status={} subject={:?} dirty={}{}{}\n review: git -C {} log -p {}..{}",
        facts.old,
        facts.new,
        facts.count,
        facts.status.as_str(),
        facts.subject,
        facts.dirty,
        ask,
        check,
        worktree.display(),
        facts.old,
        facts.new,
    )
}

/// Whether any commit in `old..new` carries a `Boop-Status: blocked` trailer.
fn burst_has_blocked(cwd: &Path, old: &str, new: &str) -> bool {
    match git_output(cwd, &["log", "--format=%B", &format!("{old}..{new}")]) {
        Some(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).lines().any(|line| {
                line.trim()
                    .strip_prefix("Boop-Status:")
                    .is_some_and(|value| value.trim().eq_ignore_ascii_case("blocked"))
            })
        }
        _ => false,
    }
}

/// The last `key:` line's value in a commit body. Git parses trailers only from
/// the final paragraph, so a worker's separate `-m` paragraphs need this scan.
fn body_trailer(body: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    let mut found = None;
    for line in body.lines() {
        if let Some(value) = line.trim().strip_prefix(&prefix) {
            let value = value.trim();
            if !value.is_empty() {
                found = Some(value.to_owned());
            }
        }
    }
    found
}

/// Whether `candidate` descends from `ancestor`. A git that cannot answer
/// reports `true`, so an unreadable repo raises no rewind.
fn descends_from(cwd: &Path, ancestor: &str, candidate: &str) -> bool {
    match git_output(cwd, &["merge-base", "--is-ancestor", ancestor, candidate]) {
        Some(out) => out.status.success(),
        None => true,
    }
}

/// How many commits `new` adds on top of `old`.
fn rev_list_count(cwd: &Path, old: &str, new: &str) -> Option<u32> {
    let out = git_output(cwd, &["rev-list", "--count", &format!("{old}..{new}")])?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// The resolved path of the worktree's HEAD reflog.
fn reflog_path(cwd: &Path) -> Option<PathBuf> {
    let out = git_output(cwd, &["rev-parse", "--git-path", "logs/HEAD"])?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if line.is_empty() {
        return None;
    }
    let path = PathBuf::from(line);
    Some(if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    })
}

/// One git line with `-C cwd`, or `None` when git fails or times out.
fn git_line(cwd: &Path, args: &[&str]) -> Option<String> {
    let out = git_output(cwd, args)?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (!line.is_empty()).then_some(line)
}

/// How many paths `git status --porcelain` lists. A git that cannot answer
/// counts as zero.
fn dirty_count(cwd: &Path) -> usize {
    match git_output(cwd, &["status", "--porcelain"]) {
        Some(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count(),
        _ => 0,
    }
}

/// The reflog mtime, or `None` when the path has no readable mtime.
fn stat_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok().and_then(|m| m.modified().ok())
}

/// Run one git command against `cwd`, killing it after `GIT_TIMEOUT`.
fn git_output(cwd: &Path, args: &[&str]) -> Option<Output> {
    use wait_timeout::ChildExt;
    let started = Instant::now();
    let mut child = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    match child.wait_timeout(GIT_TIMEOUT) {
        Ok(Some(_)) => {
            let mut stdout = Vec::new();
            if let Some(mut pipe) = child.stdout.take() {
                let _ = pipe.read_to_end(&mut stdout);
            }
            let status = child.wait().ok()?;
            Some(Output {
                status,
                stdout,
                stderr: Vec::new(),
            })
        }
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            warn!(
                args = ?args,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "git call timed out"
            );
            None
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tempdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("boop-headwatch-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn git(dir: &Path, args: &[&str]) -> Output {
        std::process::Command::new("git")
            .args(["-C", &dir.display().to_string()])
            .args(args)
            .output()
            .unwrap()
    }

    /// A worktree with one commit, so HEAD ancestry has something to answer.
    fn git_repo(dir: &Path) -> String {
        std::fs::create_dir_all(dir).unwrap();
        git(dir, &["init", "-q", "-b", "main"]);
        git(dir, &["config", "user.email", "lane@boop"]);
        git(dir, &["config", "user.name", "lane"]);
        std::fs::write(dir.join("one.txt"), "one\n").unwrap();
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-qm", "one"]);
        git_line(dir, &["rev-parse", "HEAD"]).unwrap()
    }

    fn commit_file(dir: &Path, name: &str, subject: &str) {
        std::fs::write(dir.join(name), format!("{name}\n")).unwrap();
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "-qm", subject]);
    }

    fn head(dir: &Path) -> String {
        git_line(dir, &["rev-parse", "HEAD"]).unwrap()
    }

    #[test]
    fn stat_gate_returns_none_for_an_unchanged_reflog() {
        let dir = tempdir("gate");
        let repo = dir.join("work");
        let first = git_repo(&repo);
        let mut watch = HeadWatch::new(&repo, Some(first), Duration::ZERO);
        assert_eq!(watch.tick(&repo, Instant::now()), None);
    }

    #[test]
    fn an_unseeded_watch_reports_the_first_commit() {
        let dir = tempdir("unseeded");
        let repo = dir.join("work");
        git_repo(&repo);
        let mut watch = HeadWatch::new(&repo, None, Duration::ZERO);
        commit_file(&repo, "two.txt", "two");
        watch.nudge();
        match watch.tick(&repo, Instant::now()) {
            Some(HeadMove::Advanced(facts)) => assert_eq!(facts.subject, "two"),
            other => panic!("expected the first commit reported, got {other:?}"),
        }
    }

    #[test]
    fn nudge_forces_a_head_read_over_an_unchanged_mtime() {
        let dir = tempdir("nudge");
        let repo = dir.join("work");
        let first = git_repo(&repo);
        let mut watch = HeadWatch::new(&repo, Some(first), Duration::ZERO);
        commit_file(&repo, "two.txt", "two");
        watch.last_mtime = stat_mtime(&watch.reflog);
        assert_eq!(watch.tick(&repo, Instant::now()), None);
        watch.nudge();
        match watch.tick(&repo, Instant::now()) {
            Some(HeadMove::Advanced(facts)) => {
                assert_eq!(facts.count, 1);
                assert_eq!(facts.subject, "two");
            }
            other => panic!("expected an advance, got {other:?}"),
        }
    }

    #[test]
    fn a_single_commit_reports_one_wip_row() {
        let dir = tempdir("single");
        let repo = dir.join("work");
        let first = git_repo(&repo);
        let mut watch = HeadWatch::new(&repo, Some(first.clone()), Duration::ZERO);
        commit_file(&repo, "two.txt", "two");
        let second = head(&repo);
        watch.nudge();
        match watch.tick(&repo, Instant::now()) {
            Some(HeadMove::Advanced(facts)) => {
                assert_eq!(facts.old, first);
                assert_eq!(facts.new, second);
                assert_eq!(facts.count, 1);
                assert_eq!(facts.subject, "two");
                assert_eq!(facts.status, CommitStatus::Wip);
                assert_eq!(facts.dirty, 0);
            }
            other => panic!("expected an advance, got {other:?}"),
        }
    }

    #[test]
    fn commits_inside_the_quiet_window_coalesce_into_one_row() {
        let dir = tempdir("burst");
        let repo = dir.join("work");
        let first = git_repo(&repo);
        let quiet = Duration::from_secs(3600);
        let mut watch = HeadWatch::new(&repo, Some(first.clone()), quiet);
        let base = Instant::now();

        commit_file(&repo, "two.txt", "two");
        watch.nudge();
        assert_eq!(watch.tick(&repo, base), None);

        commit_file(&repo, "three.txt", "three");
        watch.nudge();
        assert_eq!(watch.tick(&repo, base + Duration::from_secs(1)), None);

        commit_file(&repo, "four.txt", "four");
        watch.nudge();
        assert_eq!(watch.tick(&repo, base + Duration::from_secs(2)), None);

        let emit = base + Duration::from_secs(2) + quiet;
        watch.nudge();
        match watch.tick(&repo, emit) {
            Some(HeadMove::Advanced(facts)) => {
                assert_eq!(facts.old, first);
                assert_eq!(facts.count, 3);
                assert_eq!(facts.subject, "four");
            }
            other => panic!("expected an advance, got {other:?}"),
        }
    }

    #[test]
    fn a_blocked_trailer_carries_its_ask() {
        let dir = tempdir("blocked");
        let repo = dir.join("work");
        let first = git_repo(&repo);
        let mut watch = HeadWatch::new(&repo, Some(first), Duration::ZERO);
        git(
            &repo,
            &[
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "need input",
                "-m",
                "Boop-Status: blocked",
                "-m",
                "Boop-Ask: q?",
            ],
        );
        watch.nudge();
        match watch.tick(&repo, Instant::now()) {
            Some(HeadMove::Advanced(facts)) => {
                assert_eq!(facts.status, CommitStatus::Blocked);
                assert_eq!(facts.ask.as_deref(), Some("q?"));
            }
            other => panic!("expected an advance, got {other:?}"),
        }
    }

    #[test]
    fn a_reset_to_an_older_commit_reports_a_rewind() {
        let dir = tempdir("rewind");
        let repo = dir.join("work");
        let first = git_repo(&repo);
        let mut watch = HeadWatch::new(&repo, Some(first.clone()), Duration::ZERO);
        commit_file(&repo, "two.txt", "two");
        let second = head(&repo);
        watch.nudge();
        assert!(matches!(
            watch.tick(&repo, Instant::now()),
            Some(HeadMove::Advanced(_))
        ));

        git(&repo, &["reset", "-q", "--hard", "HEAD~1"]);
        watch.nudge();
        match watch.tick(&repo, Instant::now()) {
            Some(HeadMove::Rewound { old, new }) => {
                assert_eq!(old, second);
                assert_eq!(new, first);
            }
            other => panic!("expected a rewind, got {other:?}"),
        }
    }

    #[test]
    fn commit_body_is_exact() {
        let facts = CommitFacts {
            old: "aaa".into(),
            new: "bbb".into(),
            count: 2,
            subject: "x".into(),
            status: CommitStatus::Done,
            ask: None,
            check: Some("cargo test -> pass".into()),
            dirty: 3,
        };
        assert_eq!(
            commit_body("mine", Path::new("/w"), &facts),
            "commit mine aaa..bbb n=2 status=done subject=\"x\" dirty=3 check=\"cargo test -> pass\"\n review: git -C /w log -p aaa..bbb"
        );
    }

    #[test]
    fn a_blocked_body_names_the_ask() {
        let facts = CommitFacts {
            old: "aaa".into(),
            new: "bbb".into(),
            count: 1,
            subject: "x".into(),
            status: CommitStatus::Blocked,
            ask: Some("q?".into()),
            check: None,
            dirty: 0,
        };
        assert_eq!(
            commit_body("mine", Path::new("/w"), &facts),
            "commit mine aaa..bbb n=1 status=blocked subject=\"x\" dirty=0 ask=\"q?\"\n review: git -C /w log -p aaa..bbb"
        );
    }

    #[test]
    fn is_git_write_matches_completed_git_writes_only() {
        let call = |title: &str, status: &str| ToolCallFact {
            title: title.into(),
            kind: "execute".into(),
            status: status.into(),
            paths: Vec::new(),
        };
        assert!(is_git_write(&call("Bash git commit -m x", "completed")));
        assert!(is_git_write(&call("git reset --hard HEAD~1", "completed")));
        assert!(!is_git_write(&call("git status", "completed")));
        assert!(!is_git_write(&call("Bash git commit -m x", "in_progress")));
    }
}
