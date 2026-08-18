//! Shared deterministic fixtures for Boop unit tests.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::tmux::{LiveSessions, Multiplexer};

/// A throwaway git repo (one seed commit) plus a worktree path; harness
/// adapter tests spawn against it and tear both down on drop.
pub(crate) struct TempRepo {
    pub(crate) dir: PathBuf,
    pub(crate) sha: String,
    pub(crate) worktree: PathBuf,
}

/// `std::process::id()` is one PID for the whole test binary; adapter test
/// modules run in parallel threads, so a monotonic counter is the real
/// differentiator that keeps two callers' temp dirs from colliding.
static NEXT_TEMP_REPO: AtomicUsize = AtomicUsize::new(0);

impl TempRepo {
    pub(crate) fn new() -> TempRepo {
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
pub(crate) struct FakeMux {
    sessions: Option<BTreeSet<String>>,
    panes: BTreeMap<String, String>,
    pub(crate) observations: AtomicUsize,
}

impl FakeMux {
    pub(crate) fn available(names: &[&str]) -> Self {
        Self {
            sessions: Some(names.iter().map(|name| (*name).to_owned()).collect()),
            panes: BTreeMap::new(),
            observations: AtomicUsize::new(0),
        }
    }

    pub(crate) fn with_pane(mut self, pane: &str, session: &str) -> Self {
        self.panes.insert(pane.to_owned(), session.to_owned());
        self
    }

    pub(crate) fn inaccessible() -> Self {
        Self {
            sessions: None,
            panes: BTreeMap::new(),
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

    fn pane_pid(&self, _: Option<&str>, _: &str) -> Option<u32> {
        None
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

    fn send_keys_literal(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn send_text(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
        Ok(())
    }

    fn send_key_named(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
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
