use std::path::PathBuf;
use std::sync::Mutex;

use tracing::Subscriber;
use tracing_chrome::{ChromeLayerBuilder, FlushGuard};
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Layer};

pub const TRACE_PATH_VARIABLE: &str = "HAFLEY_TRACE";

// A host may std::process::exit, which skips Drop, so the guard lives in a
// process-global slot and finish_trace is called explicitly at every exit site.
static TRACE_GUARD: Mutex<Option<FlushGuard>> = Mutex::new(None);

pub fn trace_path() -> Option<PathBuf> {
    std::env::var_os(TRACE_PATH_VARIABLE)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

pub fn chrome_layer<S>() -> Option<Box<dyn Layer<S> + Send + Sync>>
where
    S: Subscriber + for<'a> LookupSpan<'a> + Send + Sync,
{
    let path = trace_path()?;
    let (layer, guard) = ChromeLayerBuilder::new()
        .file(path)
        .include_args(true)
        .build();
    // The export carries its own filter so a quiet stderr default cannot
    // silently produce an empty timeline.
    let layer = layer.with_filter(EnvFilter::new(
        std::env::var("HAFLEY_TRACE_FILTER").unwrap_or_else(|_| "trace".to_string()),
    ));
    if let Ok(mut slot) = TRACE_GUARD.lock() {
        *slot = Some(guard);
    }
    Some(layer.boxed())
}

pub fn finish_trace() {
    if let Ok(mut slot) = TRACE_GUARD.lock() {
        slot.take();
    }
}
