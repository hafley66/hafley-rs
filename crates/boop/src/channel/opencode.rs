//! Shared child polling used by the Kimi channel.

use std::process::Child;
use std::time::{Duration, Instant};

use anyhow::Result;

/// Reap `child` if it exits within `timeout`; `None` means still running.
pub(crate) fn wait_for(child: &mut Child, timeout: Duration) -> Result<Option<i32>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status.code().unwrap_or(-1)));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}
