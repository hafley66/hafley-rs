use tracing::Subscriber;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::fmt::writer::BoxMakeWriter;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::{EnvFilter, Layer};

use crate::OutputFormat;

#[derive(Clone, Debug)]
pub struct FormatConfig {
    pub format: OutputFormat,
    pub ansi: bool,
    pub target: bool,
    pub thread_names: bool,
    pub span_events: FmtSpan,
}

impl FormatConfig {
    pub fn standard(format: OutputFormat, ansi: bool) -> Self {
        Self {
            format,
            ansi,
            target: true,
            thread_names: false,
            span_events: FmtSpan::NONE,
        }
    }
}

pub fn env_filter(default_filter: &str) -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter))
}

pub fn format_layer<S>(
    config: FormatConfig,
    writer: BoxMakeWriter,
) -> Box<dyn Layer<S> + Send + Sync>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    match config.format {
        OutputFormat::Human => tracing_subscriber::fmt::layer()
            .with_ansi(config.ansi)
            .with_target(config.target)
            .with_thread_names(config.thread_names)
            .with_span_events(config.span_events)
            .with_writer(writer)
            .boxed(),
        OutputFormat::Json => tracing_subscriber::fmt::layer()
            .json()
            .with_ansi(false)
            .with_target(config.target)
            .with_thread_names(config.thread_names)
            .with_span_events(config.span_events)
            .with_writer(writer)
            .boxed(),
    }
}
