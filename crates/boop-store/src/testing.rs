//! Shared deterministic fixtures for Boop unit tests.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::tmux::{LiveSessions, Multiplexer};

/// A throwaway git repo (one seed commit) plus a worktree path; harness
/// adapter tests spawn against it and tear both down on drop.
pub struct TempRepo {
    pub dir: PathBuf,
    pub sha: String,
    pub worktree: PathBuf,
}

/// `std::process::id()` is one PID for the whole test binary; adapter test
/// modules run in parallel threads, so a monotonic counter is the real
/// differentiator that keeps two callers' temp dirs from colliding.
static NEXT_TEMP_REPO: AtomicUsize = AtomicUsize::new(0);

impl TempRepo {
    #[allow(clippy::new_without_default)] // a fixture repo is minted, never defaulted
    pub fn new() -> TempRepo {
        let unique = NEXT_TEMP_REPO.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("boop-temprepo-{pid}-{unique}"));
        let _ = std::fs::remove_dir_all(&dir);
        let worktree = std::env::temp_dir().join(format!("boop-temprepo-wt-{pid}-{unique}"));
        let _ = std::fs::remove_dir_all(&worktree);
        Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(&dir)
            .status()
            .unwrap();
        let d = dir.display().to_string();
        Command::new("git")
            .args(["-C", &d, "config", "user.email", "t@t"])
            .status()
            .unwrap();
        Command::new("git")
            .args(["-C", &d, "config", "user.name", "t"])
            .status()
            .unwrap();
        std::fs::write(dir.join("seed.txt"), "s").unwrap();
        Command::new("git")
            .args(["-C", &d, "add", "-A"])
            .status()
            .unwrap();
        Command::new("git")
            .args(["-C", &d, "commit", "-qm", "seed"])
            .status()
            .unwrap();
        let sha = String::from_utf8_lossy(
            &Command::new("git")
                .args(["-C", &d, "rev-parse", "HEAD"])
                .output()
                .unwrap()
                .stdout,
        )
        .trim()
        .to_owned();
        TempRepo { dir, sha, worktree }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        let _ = std::fs::remove_dir_all(&self.worktree);
    }
}

/// A single-observation tmux fixture. It records each `live_sessions` request
/// so bounded projection receipts can assert one acquisition for all lanes.
pub struct FakeMux {
    sessions: Option<BTreeSet<String>>,
    panes: BTreeMap<String, String>,
    pane_pids: BTreeMap<String, u32>,
    pub observations: AtomicUsize,
}

impl FakeMux {
    pub fn available(names: &[&str]) -> Self {
        Self {
            sessions: Some(names.iter().map(|name| (*name).to_owned()).collect()),
            panes: BTreeMap::new(),
            pane_pids: BTreeMap::new(),
            observations: AtomicUsize::new(0),
        }
    }

    pub fn with_pane(mut self, pane: &str, session: &str) -> Self {
        self.panes.insert(pane.to_owned(), session.to_owned());
        self
    }

    pub fn with_pane_pid(mut self, target: &str, pid: u32) -> Self {
        self.pane_pids.insert(target.to_owned(), pid);
        self
    }

    pub fn inaccessible() -> Self {
        Self {
            sessions: None,
            panes: BTreeMap::new(),
            pane_pids: BTreeMap::new(),
            observations: AtomicUsize::new(0),
        }
    }
}

impl Multiplexer for FakeMux {
    fn current_pane(&self, _: Option<&str>) -> Option<String> {
        None
    }

    fn session_of_pane(&self, _: Option<&str>, pane: &str) -> Option<String> {
        self.panes.get(pane).cloned()
    }

    /// Emulates tmux target resolution: a pane id names itself, and a
    /// `session` or `session:window.pane` target names that session's pane.
    fn pane_id(&self, _: Option<&str>, target: &str) -> Option<String> {
        if self.panes.contains_key(target) {
            return Some(target.to_owned());
        }
        self.panes.iter().find_map(|(pane, session)| {
            (target == session || target.starts_with(&format!("{session}:"))).then(|| pane.clone())
        })
    }

    fn pane_pid(&self, _: Option<&str>, target: &str) -> Option<u32> {
        self.pane_pids.get(target).copied()
    }

    fn live_sessions(&self, _: Option<&str>) -> Option<LiveSessions> {
        self.observations.fetch_add(1, Ordering::SeqCst);
        self.sessions.as_ref().map(|names| LiveSessions {
            names: names.clone(),
        })
    }

    fn has_session(&self, _: Option<&str>, _: &str) -> anyhow::Result<bool> {
        Ok(false)
    }

    fn kill_session(&self, _: Option<&str>, _: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn new_bare_session(&self, _: Option<&str>, _: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn kill_window(&self, _: Option<&str>, _: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn target_alive(&self, _: Option<&str>, target: &str) -> bool {
        self.panes.contains_key(target)
    }

    fn capture_pane(&self, _: Option<&str>, _: &str, _: Option<u32>) -> anyhow::Result<String> {
        Ok(String::new())
    }

    fn new_detached_session(
        &self,
        _: Option<&str>,
        _: &str,
        _: &str,
        _: &str,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    fn new_window(
        &self,
        _: Option<&str>,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> anyhow::Result<String> {
        Ok(String::new())
    }

    fn swap_windows(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

/// What one ingest pass wrote into `agent_usage`, read back from a closed
/// store file: row count, then input, output and cache-read token sums. The
/// harness adapters' ingest tests assert against this rather than opening the
/// store's tables themselves.
pub struct UsageTotals {
    pub row_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
}

/// Read `agent_usage` totals from the store at `path`.
pub fn usage_totals_at(path: &Path) -> UsageTotals {
    let connection = rusqlite::Connection::open(path).expect("open the store under test");
    let (row_count, input_tokens, output_tokens, cache_read_tokens) = connection
        .query_row(
            "SELECT COUNT(*), SUM(input_tokens), SUM(output_tokens),
               SUM(cache_read_tokens) FROM agent_usage",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("agent_usage totals");
    UsageTotals {
        row_count,
        input_tokens,
        output_tokens,
        cache_read_tokens,
    }
}
