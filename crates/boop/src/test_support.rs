//! Shared deterministic fixtures for Boop unit tests.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::tmux::{LiveSessions, Multiplexer};

/// A single-observation tmux fixture. It records each `live_sessions` request
/// so bounded projection receipts can assert one acquisition for all lanes.
pub(crate) struct FakeMux {
    sessions: Option<BTreeSet<String>>,
    pub(crate) observations: AtomicUsize,
}

impl FakeMux {
    pub(crate) fn available(names: &[&str]) -> Self {
        Self {
            sessions: Some(names.iter().map(|name| (*name).to_owned()).collect()),
            observations: AtomicUsize::new(0),
        }
    }

    pub(crate) fn inaccessible() -> Self {
        Self {
            sessions: None,
            observations: AtomicUsize::new(0),
        }
    }
}

impl Multiplexer for FakeMux {
    fn session_of_pane(&self, _: Option<&str>, _: &str) -> Option<String> {
        None
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

    fn target_alive(&self, _: Option<&str>, _: &str) -> bool {
        false
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
