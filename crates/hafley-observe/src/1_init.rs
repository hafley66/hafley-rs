use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::{Config, OutputFormat};

pub fn init(config: Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    init_with_writer(config, BoxMakeWriter::new(std::io::stderr))
}

pub fn init_with_writer(
    config: Config,
    writer: BoxMakeWriter,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(config.default_filter));
    let format: Box<dyn Layer<_> + Send + Sync> = match config.format {
        OutputFormat::Human => tracing_subscriber::fmt::layer()
            .with_ansi(config.ansi)
            .with_writer(writer)
            .boxed(),
        OutputFormat::Json => tracing_subscriber::fmt::layer()
            .json()
            .with_ansi(false)
            .with_writer(writer)
            .boxed(),
    };
    tracing_subscriber::registry()
        .with(filter)
        .with(format)
        .try_init()?;
    tracing::debug!(
        service.name = config.service_name,
        service.version = config.service_version,
        process.pid = std::process::id(),
        log.format = config.format.as_str(),
        "observability initialized"
    );
    Ok(())
}
