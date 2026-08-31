//! Which sessions of one harness are running right now, read from that
//! harness's own registry: a file it writes per process, its state database,
//! or its server. No tmux scraping, no transcript mtime.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::harness::HarnessId;

/// What a live session is doing at the moment it was observed.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum LiveStatus {
    Busy,
    Idle,
    Unknown,
}

/// Whether a harness-native session is the interactive root or one of its
/// internal collaboration/approval children. `Unknown` preserves launch
/// support for registries that do not expose this distinction.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum LiveSessionScope {
    Root,
    Child,
    Unknown,
}

/// Where a message is written to reach a running session.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum DoorAddress {
    /// A newline-delimited JSON socket the harness process listens on.
    UnixSocket {
        path: PathBuf,
        token: Option<String>,
    },
    /// A remote-control daemon addressed by socket plus the thread inside it.
    AppServer { socket: PathBuf, thread: String },
    /// An HTTP server whose sessions the TUI shares.
    Http { base: url::Url, session: String },
    /// The harness publishes no door for a running TUI.
    None,
}

/// One running session of one harness, as its own registry reports it.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct LiveSession {
    pub harness: HarnessId,
    pub session_id: String,
    pub pid: Option<u32>,
    pub cwd: Option<PathBuf>,
    /// The pane id alone, `%3418`, never the window or session prefix.
    pub tmux_pane: Option<String>,
    pub status: LiveStatus,
    pub door: DoorAddress,
    pub observed_ms: u64,
    /// When the session began, from the harness's own record; `None` where
    /// the registry keeps no start time.
    pub started_ms: Option<u64>,
    pub scope: LiveSessionScope,
    /// Interactive root owning this child when the harness exposes or permits
    /// recovery of the relation. Root and unrelated sessions carry `None`.
    pub parent_session: Option<String>,
}

/// The live-session registry of one harness.
pub trait LiveSessions: Send + Sync {
    /// Harness-native registry only. An empty vector means nothing of this
    /// harness is running, never that the lookup failed.
    fn live_sessions(&self) -> Result<Vec<LiveSession>>;

    /// The session occupying a tmux pane. `pane` is matched as written and
    /// with a leading `%` added, so both `3418` and `%3418` resolve.
    fn live_session_in_pane(&self, pane: &str) -> Result<Option<LiveSession>> {
        let wanted = pane.trim().trim_start_matches('%');
        if wanted.is_empty() {
            return Ok(None);
        }
        Ok(self.live_sessions()?.into_iter().find(|session| {
            session
                .tmux_pane
                .as_deref()
                .is_some_and(|held| held.trim_start_matches('%') == wanted)
        }))
    }
}

/// Milliseconds since the epoch, the stamp every observation carries.
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_millis() as u64)
        .unwrap_or(0)
}

/// Whether a pid names a process this user can still signal. Registry files
/// outlive their process, so every reader filters on this.
pub fn pid_alive(pid: u32) -> bool {
    // Signal 0 runs the permission and existence checks and delivers nothing.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// The pane id inside a tmux target such as `projects-2:@3418.%3418`.
pub fn pane_of_target(target: &str) -> Option<String> {
    let pane = target.rsplit('.').next()?.trim();
    pane.starts_with('%').then(|| pane.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Two;

    impl LiveSessions for Two {
        fn live_sessions(&self) -> Result<Vec<LiveSession>> {
            Ok(vec![session("a", Some("%1")), session("b", Some("%3418"))])
        }
    }

    fn session(id: &str, pane: Option<&str>) -> LiveSession {
        LiveSession {
            harness: HarnessId::Claude,
            session_id: id.into(),
            pid: None,
            cwd: None,
            tmux_pane: pane.map(str::to_string),
            status: LiveStatus::Unknown,
            door: DoorAddress::None,
            observed_ms: 0,
            started_ms: None,
            scope: LiveSessionScope::Unknown,
            parent_session: None,
        }
    }

    /// RECEIPT. A route holds a pane either spelling; both find the session.
    #[test]
    fn a_pane_lookup_ignores_the_percent_prefix() {
        assert_eq!(
            Two.live_session_in_pane("%3418")
                .unwrap()
                .map(|s| s.session_id),
            Some("b".to_string())
        );
        assert_eq!(
            Two.live_session_in_pane("3418")
                .unwrap()
                .map(|s| s.session_id),
            Some("b".to_string())
        );
        assert!(Two.live_session_in_pane("%9").unwrap().is_none());
        assert!(Two.live_session_in_pane("  ").unwrap().is_none());
    }

    /// RECEIPT. The registry writes a full tmux target; the pane is the tail.
    #[test]
    fn a_tmux_target_yields_its_pane() {
        assert_eq!(
            pane_of_target("projects-2:@3418.%3418"),
            Some("%3418".to_string())
        );
        assert_eq!(pane_of_target("projects-2:@3418"), None);
        assert_eq!(pane_of_target(""), None);
    }

    /// RECEIPT. This process is alive; pid 0 is not addressable as a process.
    #[test]
    fn a_live_pid_answers_the_signal_probe() {
        assert!(pid_alive(std::process::id()));
        assert!(!pid_alive(4_000_000));
    }
}
