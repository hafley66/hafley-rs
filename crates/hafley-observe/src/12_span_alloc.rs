//! Process and span allocation counters backed by tracking-allocator.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tracking_allocator::{AllocationGroupId, AllocationRegistry, AllocationTracker};

pub type ProcessAllocator = tracking_allocator::Allocator<std::alloc::System>;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static INSTALLED: OnceLock<()> = OnceLock::new();
static GROUPS: OnceLock<Mutex<HashMap<usize, Arc<AtomicUsize>>>> = OnceLock::new();

fn groups() -> &'static Mutex<HashMap<usize, Arc<AtomicUsize>>> {
    GROUPS.get_or_init(|| Mutex::new(HashMap::new()))
}

struct ProcessTracker;

impl AllocationTracker for ProcessTracker {
    fn allocated(
        &self,
        _addr: usize,
        object_size: usize,
        _wrapped_size: usize,
        group: AllocationGroupId,
    ) {
        ACTIVE.store(true, Ordering::Relaxed);
        let live = LIVE.fetch_add(object_size, Ordering::Relaxed) + object_size;
        PEAK.fetch_max(live, Ordering::Relaxed);
        if group != AllocationGroupId::ROOT {
            if let Some(counter) = groups().lock().unwrap().get(&group.as_usize().get()) {
                counter.fetch_add(object_size, Ordering::Relaxed);
            }
        }
    }

    fn deallocated(
        &self,
        _addr: usize,
        object_size: usize,
        _wrapped_size: usize,
        _source_group: AllocationGroupId,
        _current_group: AllocationGroupId,
    ) {
        LIVE.fetch_sub(object_size, Ordering::Relaxed);
    }
}

pub fn enable() {
    INSTALLED.get_or_init(|| {
        AllocationRegistry::set_global_tracker(ProcessTracker)
            .expect("hafley-observe allocation tracker already installed");
        AllocationRegistry::enable_tracking();
    });
}

pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

pub fn live_bytes() -> usize {
    LIVE.load(Ordering::Relaxed)
}

pub fn reset_peak() -> usize {
    let live = live_bytes();
    PEAK.store(live, Ordering::Relaxed);
    live
}

pub fn peak_bytes() -> usize {
    PEAK.load(Ordering::Relaxed)
}

#[macro_export]
macro_rules! counting_allocator {
    () => {
        #[global_allocator]
        static OH_COUNTING_ALLOCATOR: $crate::allocation::ProcessAllocator =
            $crate::allocation::ProcessAllocator::system();
    };
}

#[cfg(feature = "span-alloc")]
mod spans {
    use super::*;
    use tracing::Subscriber;
    use tracing_subscriber::layer::{Context, Layer};
    use tracing_subscriber::registry::LookupSpan;
    use tracking_allocator::{AllocationGroupToken, AllocationLayer};

    static SPANS: OnceLock<Mutex<HashMap<u64, (usize, Arc<AtomicUsize>)>>> = OnceLock::new();

    fn spans() -> &'static Mutex<HashMap<u64, (usize, Arc<AtomicUsize>)>> {
        SPANS.get_or_init(|| Mutex::new(HashMap::new()))
    }

    /// Attach a new allocation group before entering the span.
    pub fn attach(span: &tracing::Span) {
        let Some(id) = span.id() else { return };
        let Some(token) = AllocationGroupToken::register() else {
            return;
        };
        let group = token.id().as_usize().get();
        let counter = Arc::new(AtomicUsize::new(0));
        groups().lock().unwrap().insert(group, Arc::clone(&counter));
        spans()
            .lock()
            .unwrap()
            .insert(id.into_u64(), (group, counter));
        token.attach_to_span(span);
    }

    pub fn allocated_bytes(span: &tracing::Span) -> Option<usize> {
        let id = span.id()?.into_u64();
        allocated_bytes_by_id(id)
    }

    pub fn allocated_bytes_by_id(id: u64) -> Option<usize> {
        Some(spans().lock().unwrap().get(&id)?.1.load(Ordering::Relaxed))
    }

    pub struct SpanTotals;

    impl<S> Layer<S> for SpanTotals
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        fn on_close(&self, id: tracing::Id, ctx: Context<'_, S>) {
            let Some((group, count)) = spans().lock().unwrap().remove(&id.into_u64()) else {
                return;
            };
            groups().lock().unwrap().remove(&group);
            let Some(span) = ctx.span(&id) else { return };
            tracing::debug!(
                target: "span_alloc",
                span = span.name(),
                span_id = id.into_u64(),
                "mem.span_alloc_bytes" = count.load(Ordering::Relaxed),
                "span allocation sampled"
            );
        }
    }

    pub fn layers<S>() -> Vec<Box<dyn Layer<S> + Send + Sync>>
    where
        S: Subscriber + for<'a> LookupSpan<'a> + Send + Sync,
    {
        enable();
        vec![Box::new(AllocationLayer::<S>::new()), Box::new(SpanTotals)]
    }
}

#[cfg(feature = "span-alloc")]
pub use spans::{allocated_bytes, attach, layers};

#[cfg(not(feature = "span-alloc"))]
pub fn layers<S>() -> Vec<Box<dyn tracing_subscriber::Layer<S> + Send + Sync>>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a> + Send + Sync,
{
    Vec::new()
}

#[cfg(feature = "span-alloc")]
pub use spans::allocated_bytes_by_id;

#[cfg(not(feature = "span-alloc"))]
pub fn allocated_bytes_by_id(_id: u64) -> Option<usize> {
    None
}

#[cfg(not(feature = "span-alloc"))]
pub fn attach(_span: &tracing::Span) {}

pub fn tracked(span: tracing::Span) -> tracing::Span {
    attach(&span);
    span
}
