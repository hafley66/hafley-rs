use std::cell::Cell;
use std::collections::BTreeMap;
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

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
}

struct CountLayer(Arc<Counts>);

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
    let subscriber = tracing_subscriber::registry().with(CountLayer(Arc::clone(&counts)));
    let memory_start = CountingAllocator::reset_peak();
    let start = Instant::now();
    let result =
        tracing::subscriber::with_default(subscriber, || catch_unwind(AssertUnwindSafe(body)));
    let elapsed = start.elapsed();

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
            ALLOCATOR_ACTIVE.load(Ordering::Relaxed),
            "oh::test {name} has a memory budget but the test binary did not install oh::counting_allocator!()"
        );
        let growth = CountingAllocator::peak_bytes().saturating_sub(memory_start);
        assert!(
            growth <= limit,
            "oh::test {name} exceeded memory budget: {growth} bytes > {limit} bytes"
        );
    }
    result
}

/// A process-wide live and peak allocation counter for consumers that install
/// it as their `#[global_allocator]`. Tests decorated with `oh::test` run
/// serially inside each process, so the live/peak delta belongs to one test.
pub struct CountingAllocator;

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);
static ALLOCATOR_ACTIVE: AtomicBool = AtomicBool::new(false);

impl CountingAllocator {
    pub fn live_bytes() -> usize {
        LIVE_BYTES.load(Ordering::Relaxed)
    }

    pub fn reset_peak() -> usize {
        let live = Self::live_bytes();
        PEAK_BYTES.store(live, Ordering::Relaxed);
        live
    }

    pub fn peak_bytes() -> usize {
        PEAK_BYTES.load(Ordering::Relaxed)
    }
}

unsafe impl std::alloc::GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let ptr = unsafe { std::alloc::System.alloc(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        let ptr = unsafe { std::alloc::System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        unsafe { std::alloc::System.dealloc(ptr, layout) };
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        let next = unsafe { std::alloc::System.realloc(ptr, layout, new_size) };
        if !next.is_null() {
            if new_size >= layout.size() {
                record_alloc(new_size - layout.size());
            } else {
                LIVE_BYTES.fetch_sub(layout.size() - new_size, Ordering::Relaxed);
            }
        }
        next
    }
}

fn record_alloc(size: usize) {
    ALLOCATOR_ACTIVE.store(true, Ordering::Relaxed);
    let live = LIVE_BYTES.fetch_add(size, Ordering::Relaxed) + size;
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}

/// Installs the allocation counter in the downstream test binary.
#[macro_export]
macro_rules! counting_allocator {
    () => {
        #[global_allocator]
        static OH_COUNTING_ALLOCATOR: $crate::testkit::CountingAllocator =
            $crate::testkit::CountingAllocator;
    };
}
