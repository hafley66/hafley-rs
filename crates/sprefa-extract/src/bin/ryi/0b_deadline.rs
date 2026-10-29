//! `--timeout` for `graph` and `query --scmpp`, and a daemon request's cancel flag: a timer
//! interrupts SQLite at the deadline or on cancel, Rust loops poll `expired`, and work failing past it exits 3.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use rusqlite::InterruptHandle;

/// How often the timer looks at the deadline and the cancel flag.
const POLL: Duration = Duration::from_millis(100);

/// `at: None` never expires; `cancel` is the request's flag, set when its client goes away.
#[derive(Clone, Default)]
pub struct Deadline {
    pub at: Option<Instant>,
    pub cancel: Option<Arc<AtomicBool>>,
}

impl Deadline {
    pub fn expired(&self) -> bool {
        self.cancelled() || self.at.is_some_and(|at| Instant::now() >= at)
    }

    pub fn cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|cancel| cancel.load(Ordering::Acquire))
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
    within_or_cancel(handle, secs, None, verb, work)
}

/// `within`, also stopped by `cancel`: the timer checks both every `POLL` and interrupts SQLite.
pub fn within_or_cancel<T>(
    handle: Option<InterruptHandle>,
    secs: Option<u64>,
    cancel: Option<Arc<AtomicBool>>,
    verb: &str,
    work: impl FnOnce(&Deadline) -> Result<T, Box<dyn std::error::Error>>,
) -> Result<T, Box<dyn std::error::Error>> {
    let deadline = Deadline {
        at: secs.map(|secs| Instant::now() + Duration::from_secs(secs)),
        cancel,
    };
    if deadline.at.is_none() && deadline.cancel.is_none() {
        return work(&deadline);
    }
    let (done, wait) = mpsc::channel::<()>();
    let watched = deadline.clone();
    let timer = std::thread::spawn(move || loop {
        match wait.recv_timeout(POLL) {
            Err(mpsc::RecvTimeoutError::Timeout) if watched.expired() => {
                if let Some(handle) = &handle {
                    handle.interrupt();
                }
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            _ => return,
        }
    });
    let answer = work(&deadline);
    let _ = done.send(());
    let _ = timer.join();
    match answer {
        // @eprintln-ok: CLI-UX stop, off the fact stream, exit 3.
        Err(_) if deadline.cancelled() => Err(crate::RyiExit::new(3, format!("{verb}: cancelled")).into()),
        Err(_) if deadline.expired() => Err(crate::RyiExit::new(
            3,
            format!("{verb}: query exceeded {}s", secs.unwrap_or_default()),
        )
        .into()),
        answer => answer,
    }
}
