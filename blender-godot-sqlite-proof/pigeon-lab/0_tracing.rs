//! Adapter-owned telemetry. Core libraries only emit spans and events.
use std::sync::Once;
use tracing_subscriber::{EnvFilter, fmt::format::FmtSpan};

pub fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let filter = EnvFilter::builder()
            .with_default_directive(tracing::level_filters::LevelFilter::WARN.into())
            .from_env_lossy();
        // A host may already own the global subscriber. Never replace it.
        let _ = tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .with_ansi(false)
            .json()
            .with_span_events(FmtSpan::CLOSE)
            .try_init();
    });
}
