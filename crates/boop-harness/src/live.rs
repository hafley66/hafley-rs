//! Which sessions of one harness are running right now, read from that
//! harness's own registry: a file it writes per process, its state database,
//! or its server. No tmux scraping, no transcript mtime.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

use crate::harness::HarnessId;
use crate::Registry;

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

    /// Resolve an explicitly registered route through the harness's own
    /// control plane. A harness-specific endpoint belongs to its adapter.
    fn live_session_for_route(
        &self,
        route: &boop_store::bus::Route,
    ) -> Result<Option<LiveSession>> {
        // Pane numbers repeat across tmux servers and can be reused after an
        // exit. A bound conversation is authoritative, including when stale.
        if let Some(id) = route.session_id.as_deref() {
            return Ok(self
                .live_sessions()?
                .into_iter()
                .find(|session| session.session_id == id));
        }
        if let Some(target) = route.tmux.as_deref() {
            let pane = pane_of_target(target).unwrap_or_else(|| target.to_owned());
            if let Some(live) = self.live_session_in_pane(&pane)? {
                return Ok(Some(live));
            }
        }
        Ok(None)
    }

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

/// The harness session standing in a tmux pane, answered by each harness's own
/// live registry rather than by a transcript mtime or a tmux scrape. `pane` is
/// the resolved pane id; `mail_dir` is the resolved mailbox for the route
/// fallback.
pub fn session_in_pane(
    registry: &Registry,
    pane: &str,
    mail_dir: &Path,
) -> anyhow::Result<Option<String>> {
    for harness in registry.all() {
        if let Ok(Some(live)) = harness.live().live_session_in_pane(pane) {
            let sessions = harness.live().live_sessions().unwrap_or_default();
            return Ok(Some(interactive_session_id(&live, &sessions)));
        }
    }
    route_session_in_pane(pane, mail_dir)
        .map(|session| session.map(|session| resolve_registered_session(registry, &session)))
}

/// Resolve an observed process or pane to its interactive session. Only an
/// explicit native parent relation may redirect a child row. Cwd and start
/// time are descriptive fields, not identity evidence.
pub(crate) fn interactive_session_id(bound: &LiveSession, _sessions: &[LiveSession]) -> String {
    if bound.scope != LiveSessionScope::Child {
        return bound.session_id.clone();
    }
    if let Some(parent) = &bound.parent_session {
        return parent.clone();
    }
    bound.session_id.clone()
}

fn resolve_registered_session(registry: &Registry, session: &str) -> String {
    for harness in registry.all() {
        let Ok(sessions) = harness.live().live_sessions() else {
            continue;
        };
        if let Some(bound) = sessions
            .iter()
            .find(|candidate| candidate.session_id == session)
        {
            return interactive_session_id(bound, &sessions);
        }
    }
    session.to_owned()
}

// Only claude fills `LiveSession.tmux_pane`; the other three fall back to the
// boop route registry.
fn route_session_in_pane(pane: &str, mail_dir: &Path) -> anyhow::Result<Option<String>> {
    let routes = boop_store::bus::read_routes(mail_dir)?;
    Ok(routes.into_values().find_map(|route| {
        let held = route.tmux.as_deref()?;
        (held.trim_start_matches('%') == pane.trim_start_matches('%'))
            .then_some(route.session_id)
            .flatten()
    }))
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

    #[test]
    fn bound_route_never_selects_another_thread_by_pane() {
        let route = boop_store::bus::route_from_value(&serde_json::json!({
            "kind":"coordinator", "harness":"claude", "tmux":"%1", "session_id":"b"
        }));
        assert_eq!(
            Two.live_session_for_route(&route)
                .unwrap()
                .unwrap()
                .session_id,
            "b"
        );
        let mut stale = route;
        stale.session_id = Some("gone".into());
        assert!(Two.live_session_for_route(&stale).unwrap().is_none());
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
