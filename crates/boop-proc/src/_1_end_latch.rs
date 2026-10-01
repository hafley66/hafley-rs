//! One terminal result row per supervisor process.
//!
//! A tmux server's death ends a lane on two threads at once: SIGHUP reaches
//! the signal thread, and the main thread's own-pane probe (or its next print
//! into the dead pty) ends `supervise`. Each wrote a result row, so the parent
//! read rc=129 then rc=101, and the main thread kept writing after the signal
//! thread had started the exit. The latch makes the first end the only end.

use std::sync::{Mutex, PoisonError};

struct Ends {
    by_main: bool,
    by_signal: bool,
}

static ENDS: Mutex<Ends> = Mutex::new(Ends {
    by_main: false,
    by_signal: false,
});

/// The main thread's terminal row. Skipped when the signal thread has begun
/// the exit; that thread holds the latch until `_exit`, so a main thread that
/// reaches here after the signal blocks until the process is gone.
pub(crate) fn main_end(write: impl FnOnce()) {
    let mut ends = ENDS.lock().unwrap_or_else(PoisonError::into_inner);
    if ends.by_signal {
        return;
    }
    write();
    ends.by_main = true;
}

/// The signal thread's terminal row, then `_exit` with the latch still held.
/// A main thread that already wrote its row leaves the signal nothing to add.
pub(crate) fn signal_end(write: impl FnOnce() -> i32, skipped_code: i32) -> ! {
    let mut ends = ENDS.lock().unwrap_or_else(PoisonError::into_inner);
    ends.by_signal = true;
    let code = if ends.by_main { skipped_code } else { write() };
    // SAFETY: _exit is async-signal-safe and touches no Rust state. It runs
    // with the latch held on purpose: nothing else may write after this row.
    unsafe { libc::_exit(code) }
}
