use std::sync::Arc;

use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::layer::Layered;
use tracing_subscriber::{EnvFilter, Layer, Registry};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::flush::{Sink, Writer};

type Base = Layered<EnvFilter, Registry>;
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
    // Every layer is boxed over one small subscriber type. Stacking them with
    // nested `.with` monomorphized each layer per `Layered<..>` depth: 4 GB rlibs.
    let mut layers: Vec<Box<dyn Layer<Base> + Send + Sync>> = Vec::new();
    layers.extend(crate::chrome_layer());
    layers.extend(crate::instruments::context_layer());
    layers.extend(crate::instruments::span_layer(&config));
    layers.extend(crate::instruments::proc_layer());
    layers.extend(crate::tracy_layer());
    layers.extend(crate::rusage_layer());
    layers.extend(log_sink_layer(flush));
    layers.extend(sinks.into_iter().map(|sink| {
        Box::new(SinkLayer::new(Arc::new(Writer::new(sink, flush)))) as Box<dyn Layer<Base> + Send + Sync>
    }));
    layers.push(format_layer(FormatConfig::standard(config.format, config.ansi), writer));
    layers.extend(crate::otlp_layer(&config));
    crate::instruments::install(&config);
    crate::instruments::start(&config);
    tracing_subscriber::registry()
        .with(env_filter(config.default_filter))
        .with(layers)
        .try_init()?;
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