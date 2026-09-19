//! The kimi door: there is none. The kimi TUI is a terminal program with no
//! socket, no server and no registry file; `kimi acp` and `kimi --wire` are
//! separate headless processes, so reaching kimi means running one of those
//! as a lane rather than writing to the TUI a user is sitting in front of.

use std::time::Duration;

use anyhow::Result;

use crate::door::{Delivered, Door, IdleNotice};
use crate::live::{LiveSession, LiveSessions};

/// What a caller is told when it tries to reach a kimi TUI.
pub const NO_DOOR: &str = "kimi TUI exposes no door; spawn a lane";

pub struct KimiDoor;

impl LiveSessions for KimiDoor {
    /// kimi keeps no live-session registry, so nothing is reported rather
    /// than a guess assembled from panes or transcript mtimes.
    fn live_sessions(&self) -> Result<Vec<LiveSession>> {
        Ok(Vec::new())
    }
}

impl Door for KimiDoor {
    fn deliver(&self, _session: &LiveSession, _body: &str) -> Result<Delivered> {
        Ok(Delivered::Unreachable(NO_DOOR.into()))
    }

    fn notify_idle(&self, _session: &LiveSession, _timeout: Duration) -> Result<IdleNotice> {
        anyhow::bail!("{NO_DOOR}")
    }

    /// `kimi -S <id>` in its long spelling; `kimi --help` names no other.
    fn tui_resume_args(&self, session: &str) -> Option<Vec<String>> {
        Some(vec!["--session".into(), session.into()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::HarnessId;

    /// RECEIPT. Every kimi call answers the same way, with no transport tried.
    #[test]
    fn kimi_reports_no_door() {
        let session = crate::door::tests::probe(HarnessId::Kimi);
        assert!(KimiDoor.live_sessions().unwrap().is_empty());
        assert_eq!(
            KimiDoor.deliver(&session, "ping").unwrap(),
            Delivered::Unreachable(NO_DOOR.into())
        );
        assert!(KimiDoor
            .notify_idle(&session, Duration::from_millis(1))
            .is_err());
    }

    /// RECEIPT. `kimi --help`: `-S, --session [id]` resumes; nothing else does.
    #[test]
    fn resume_args_name_the_session_flag() {
        assert_eq!(
            KimiDoor.tui_resume_args("ses_9a").unwrap(),
            ["--session", "ses_9a"]
        );
    }
}
