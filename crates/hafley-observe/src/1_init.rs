use std::sync::Arc;

use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::flush::{Sink, Writer};
use crate::{env_filter, format_layer, log_sink_layer, Config, FormatConfig, SinkLayer};

pub fn init(config: Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    init_with_writer(config, BoxMakeWriter::new(std::io::stderr))
}

pub fn init_with_writer(
    config: Config,
    writer: BoxMakeWriter,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    init_with_sinks(config, writer, Vec::new())
}

/// `init_with_writer` plus host-owned sinks. Each sink gets its own `Writer`
/// under the configured flush strategy and sees the same filtered rows as the
/// built-in sqlite sink.
pub fn init_with_sinks(
    config: Config,
    writer: BoxMakeWriter,
    sinks: Vec<Arc<dyn Sink>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let flush = config.flush();
    // An empty `Vec` layer answers `Interest::never` and silences every event.
    let host_sinks: Option<Vec<SinkLayer>> = (!sinks.is_empty()).then(|| {
        sinks
            .into_iter()
            .map(|sink| SinkLayer::new(Arc::new(Writer::new(sink, flush))))
            .collect()
    });
    let filter = env_filter(config.default_filter);
    let format = format_layer(FormatConfig::standard(config.format, config.ansi), writer);
    crate::instruments::install(&config);
    crate::instruments::start(&config);
    let subscriber = tracing_subscriber::registry()
        .with(crate::chrome_layer())
        .with(crate::instruments::context_layer())
        .with(crate::instruments::span_layer(&config))
        .with(crate::instruments::proc_layer())
        .with(crate::tracy_layer())
        .with(crate::rusage_layer())
        .with(log_sink_layer(flush))
        .with(host_sinks)
        .with(filter)
        .with(format);
    subscriber.with(crate::otlp_layer(&config)).try_init()?;
    startup(&config);
    Ok(())
}

pub fn startup(config: &Config) {
    tracing::debug!(
        service.name = config.service_name,
        service.version = config.service_version,
        process.pid = std::process::id(),
        log.format = config.format.as_str(),
        log.flush = config.flush().as_str(),
        "observability initialized"
    );
}