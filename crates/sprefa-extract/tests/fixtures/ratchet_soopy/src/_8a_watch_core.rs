use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use notify::{Config, RecommendedWatcher};
use notify_debouncer_full::{new_debouncer_opt, DebounceEventResult, Debouncer, RecommendedCache};

use crate::WatchCoalescing;

pub(crate) type WatcherHandle = Debouncer<RecommendedWatcher, RecommendedCache>;
pub(crate) type WatchEvents = Receiver<DebounceEventResult>;

/// Construct one notify-debouncer-full watcher. The library owns raw notify
/// event normalization and rename stitching; Soopy retains only the existing
/// maximum receipt collection to preserve the public coalescing contract.
pub(crate) fn watcher_with_events(
    coalescing: &WatchCoalescing,
) -> Result<(WatcherHandle, WatchEvents)> {
    let (send, events) = mpsc::channel();
    let quiet = Duration::from_millis(coalescing.quiet_ms.into());
    let watcher = new_debouncer_opt(quiet, None, send, RecommendedCache::new(), watcher_config())
        .context("create debounced filesystem watcher")?;
    Ok((watcher, events))
}

/// The full debouncer has already applied the quiet window. This outer loop
/// only collects immediately subsequent debounced receipts, bounded by the
/// existing maximum window, so callers retain one logical rescan per burst.
pub(crate) fn recv_batch(
    events: &WatchEvents,
    first: DebounceEventResult,
    coalescing: &WatchCoalescing,
) -> Vec<DebounceEventResult> {
    let started = Instant::now();
    let quiet = Duration::from_millis(coalescing.quiet_ms.into());
    let maximum = Duration::from_millis(coalescing.max_ms.into());
    let mut batch = vec![first];
    loop {
        let remaining = maximum.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break;
        }
        match events.recv_timeout(quiet.min(remaining)) {
            Ok(event) => batch.push(event),
            Err(mpsc::RecvTimeoutError::Timeout | mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    batch
}

pub(crate) fn watcher_config() -> Config {
    Config::default()
        .with_compare_contents(false)
        .with_follow_symlinks(false)
}
