use std::cell::Cell;
use std::collections::{BTreeMap, VecDeque};
use std::io::Write;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub time_ms: Option<u64>,
    pub logs_per_callsite: Option<usize>,
    pub memory_bytes: Option<usize>,
}

impl Budget {
    pub const fn new(
        time_ms: Option<u64>,
        logs_per_callsite: Option<usize>,
        memory_bytes: Option<usize>,
    ) -> Self {
        Self {
            time_ms,
            logs_per_callsite,
            memory_bytes,
        }
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::new(Some(30_000), Some(1_000), None)
    }
}

#[derive(Default)]
struct Counts {
    sites: Mutex<BTreeMap<String, usize>>,
    events: Mutex<VecDeque<String>>,
    last_event_at: AtomicU64,
}

struct CountLayer(Arc<Counts>);

const RING_CAPACITY: usize = 256;

#[derive(Default)]
struct EventFields(Vec<String>);

impl tracing::field::Visit for EventFields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.0.push(format!("{}={value:?}", field.name()));
    }
}

impl<S: Subscriber> Layer<S> for CountLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let site = format!(
            "{}:{}:{}",
            metadata.file().unwrap_or("<unknown>"),
            metadata.line().unwrap_or_default(),
            metadata.target()
        );
        *self.0.sites.lock().unwrap().entry(site).or_default() += 1;
        let mut fields = EventFields::default();
        event.record(&mut fields);
        let message = format!(
            "{} {} {}",
            epoch_millis(),
            metadata.target(),
            fields.0.join(" ")
        );
        self.0
            .last_event_at
            .store(epoch_millis(), Ordering::Relaxed);
        let mut events = self.0.events.lock().unwrap();
        if events.len() == RING_CAPACITY {
            events.pop_front();
        }
        events.push_back(message);
    }
}

static TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

thread_local! {
    static RUN_DEPTH: Cell<usize> = const { Cell::new(0) };
}

struct RunDepthGuard;

impl RunDepthGuard {
    fn enter() -> Self {
        RUN_DEPTH.with(|depth| depth.set(depth.get() + 1));
        Self
    }
}

impl Drop for RunDepthGuard {
    fn drop(&mut self) {
        RUN_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

/// Runs one annotated test with an isolated tracing dispatcher and checks its
/// configured per-callsite log and elapsed-time budgets after the body returns.
pub fn run<T>(name: &'static str, budget: Budget, body: impl FnOnce() -> T) -> T {
    let nested = RUN_DEPTH.with(|depth| depth.get() > 0);
    let _serial = (!nested).then(|| {
        TEST_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    });
    let _depth = RunDepthGuard::enter();
    let counts = Arc::new(Counts::default());
    let replay = std::env::var_os("OH_REPLAY").is_some();
    let terminating =
        Arc::clone(TERMINATION_REQUESTED.get_or_init(|| Arc::new(AtomicBool::new(false))));
    if !nested && !replay {
        terminating.store(false, Ordering::Relaxed);
    }
    let signal_id = if !nested && !replay {
        signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&terminating)).ok()
    } else {
        None
    };
    if budget.memory_bytes.is_some() {
        crate::allocation::enable();
    }
    let memory_start = crate::allocation::reset_peak();
    let start = Instant::now();
    let result = if replay {
        let subscriber = tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
            .with(CountLayer(Arc::clone(&counts)));
        tracing::subscriber::with_default(subscriber, || catch_unwind(AssertUnwindSafe(body)))
    } else {
        let subscriber = tracing_subscriber::registry().with(CountLayer(Arc::clone(&counts)));
        tracing::subscriber::with_default(subscriber, || catch_unwind(AssertUnwindSafe(body)))
    };
    let elapsed = start.elapsed();

    let should_replay =
        !nested && !replay && (terminating.load(Ordering::Relaxed) || result.is_err());
    if should_replay {
        drain(&counts, seed());
        replay_test(name, seed());
    }
    if let Some(id) = signal_id {
        signal_hook::low_level::unregister(id);
    }
    let result = match result {
        Ok(value) => value,
        Err(payload) => resume_unwind(payload),
    };

    if let Some(limit) = budget.time_ms {
        assert!(
            elapsed <= Duration::from_millis(limit),
            "oh::test {name} exceeded time budget: {}ms > {limit}ms",
            elapsed.as_millis()
        );
    }
    if let Some(limit) = budget.logs_per_callsite {
        let exceeded: Vec<_> = counts
            .sites
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, count)| **count > limit)
            .map(|(site, count)| format!("{site}: {count} > {limit}"))
            .collect();
        assert!(
            exceeded.is_empty(),
            "oh::test {name} exceeded per-callsite log budget: {}",
            exceeded.join(", ")
        );
    }
    if let Some(limit) = budget.memory_bytes {
        assert!(
            crate::allocation::active(),
            "oh::test {name} has a memory budget but the test binary did not install oh::counting_allocator!()"
        );
        let growth = crate::allocation::peak_bytes().saturating_sub(memory_start);
        assert!(
            growth <= limit,
            "oh::test {name} exceeded memory budget: {growth} bytes > {limit} bytes"
        );
    }
    result
}

/// Returns the seed carried by the current test process and any replay child.
pub fn seed() -> u64 {
    std::env::var("OH_SEED")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(epoch_millis)
}

/// Returns whether the test process received SIGTERM while an `oh::test` ran.
pub fn termination_requested() -> bool {
    TERMINATION_REQUESTED
        .get()
        .is_some_and(|requested| requested.load(Ordering::Relaxed))
}

static TERMINATION_REQUESTED: OnceLock<Arc<AtomicBool>> = OnceLock::new();

fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn drain(counts: &Counts, seed: u64) {
    let drained_at = epoch_millis();
    let last_event_at = counts.last_event_at.load(Ordering::Relaxed);
    let receipt = format!("drained_at={drained_at} last_event_at={last_event_at} seed={seed}\n");
    let mut stderr = std::io::stderr().lock();
    for event in counts.events.lock().unwrap().iter() {
        let _ = writeln!(stderr, "oh::ring {event}");
    }
    let _ = writeln!(stderr, "oh::drain {receipt}");
    let _ = stderr.flush();
    if let Some(path) = std::env::var_os("OH_DRAIN_PATH") {
        let _ = std::fs::write(path, receipt);
    }
}

fn replay_test(name: &str, seed: u64) {
    let exact = std::thread::current().name().unwrap_or(name).to_owned();
    let status = std::env::current_exe().and_then(|executable| {
        std::process::Command::new(executable)
            .arg("--exact")
            .arg(exact)
            .arg("--nocapture")
            .env("OH_REPLAY", "1")
            .env("OH_SEED", seed.to_string())
            .status()
    });
    match status {
        Ok(status) if status.success() => {}
        Ok(status) => panic!("oh::test {name} replay exited with {status}"),
        Err(error) => panic!("oh::test {name} replay failed to start: {error}"),
    }
}
