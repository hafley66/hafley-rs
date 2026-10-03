//! `--timeout` for `graph` and `query --scmpp`: a timer interrupts SQLite at the
//! deadline, Rust loops poll `expired`, and work failing past it exits 3.
use std::sync::mpsc;
use std::time::{Duration, Instant};

use rusqlite::InterruptHandle;

/// `None` never expires.
pub struct Deadline {
    pub at: Option<Instant>,
}

impl Deadline {
    pub fn expired(&self) -> bool {
        self.at.is_some_and(|at| Instant::now() >= at)
    }
}

/// Runs `work` under a `secs` budget (`None`: unbounded); `verb` prefixes the exit-3 message.
/// Without a SQLite `handle`, `work` alone polls `expired`.
pub fn within<T>(
    handle: Option<InterruptHandle>,
    secs: Option<u64>,
    verb: &str,
    work: impl FnOnce(&Deadline) -> Result<T, Box<dyn std::error::Error>>,
) -> Result<T, Box<dyn std::error::Error>> {
    let Some(secs) = secs else {
        return work(&Deadline { at: None });
    };
    let budget = Duration::from_secs(secs);
    let deadline = Deadline {
        at: Some(Instant::now() + budget),
    };
    let (done, wait) = mpsc::channel::<()>();
    let timer = std::thread::spawn(move || {
        if let (Err(mpsc::RecvTimeoutError::Timeout), Some(handle)) = (wait.recv_timeout(budget), handle) {
            handle.interrupt();
        }
    });
    let answer = work(&deadline);
    let _ = done.send(());
    let _ = timer.join();
    match answer {
        // @eprintln-ok: CLI-UX stop, off the fact stream, exit 3.
        Err(_) if deadline.expired() => {
            Err(crate::RyiExit::new(3, format!("{verb}: query exceeded {secs}s")).into())
        }
        answer => answer,
    }
}
