use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::{env_filter, format_layer, Config, FormatConfig};

pub fn init(config: Config) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    init_with_writer(config, BoxMakeWriter::new(std::io::stderr))
}

pub fn init_with_writer(
    config: Config,
    writer: BoxMakeWriter,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let filter = env_filter(config.default_filter);
    let format = format_layer(FormatConfig::standard(config.format, config.ansi), writer);
    tracing_subscriber::registry()
        .with(filter)
        .with(format)
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
        "observability initialized"
    );
}
