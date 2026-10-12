//! The omp door: delivery and idle stay unreachable. omp names a resume flag,
//! so a dead coordinator pane comes back on its own conversation.

use std::time::Duration;

use anyhow::Result;

use crate::door::{Delivered, Door, IdleNotice};
use crate::live::LiveSession;

pub struct OmpDoor;

impl Door for OmpDoor {
    fn deliver(&self, session: &LiveSession, body: &str) -> Result<Delivered> {
        crate::door::UNREACHABLE.deliver(session, body)
    }

    fn notify_idle(&self, session: &LiveSession, timeout: Duration) -> Result<IdleNotice> {
        crate::door::UNREACHABLE.notify_idle(session, timeout)
    }

    /// `omp --help`: `-r, --resume=<value>` takes an ID prefix, path, or none.
    fn tui_resume_args(&self, session: &str) -> Option<Vec<String>> {
        Some(vec!["--resume".into(), session.into()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RECEIPT. Session ids recorded on omp routes resume by flag.
    #[test]
    fn resume_args_name_the_resume_flag() {
        assert_eq!(
            OmpDoor
                .tui_resume_args("01a12ba7-881d-7000-85d5-c9e58f29319b")
                .unwrap(),
            ["--resume", "01a12ba7-881d-7000-85d5-c9e58f29319b"]
        );
    }
}
