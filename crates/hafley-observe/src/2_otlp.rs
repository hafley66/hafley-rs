use std::time::Duration;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::{BatchConfigBuilder, BatchSpanProcessor, SdkTracer, SdkTracerProvider};
use opentelemetry_sdk::Resource;
use tracing::Subscriber;
use tracing_subscriber::registry::LookupSpan;

use crate::Config;

pub const DEFAULT_OTLP_ENDPOINT: &str = "http://127.0.0.1:4318/v1/traces";

pub type OtlpError = Box<dyn std::error::Error + Send + Sync>;

/// Configuration for one opt-in OTLP/HTTP-protobuf trace exporter.
///
/// Span completion enqueues work and does not wait for network export. The SDK
/// drops completed spans when `max_queue_size` is full. `export_timeout` bounds
/// each HTTP export. Callers choose the provider deadline passed to
/// `shutdown_with_timeout`; the SDK's dedicated batch processor also has an
/// upstream fixed five-second force-flush channel wait.
#[derive(Clone, Debug)]
pub struct OtlpConfig {
    pub endpoint: String,
    pub export_timeout: Duration,
    pub scheduled_delay: Duration,
    pub max_queue_size: usize,
    pub max_export_batch_size: usize,
}

impl OtlpConfig {
/// Creates the explicit localhost-only HTTP/protobuf trace endpoint used for local proof.
    pub fn localhost() -> Self {
        Self {
            endpoint: DEFAULT_OTLP_ENDPOINT.to_owned(),
            export_timeout: Duration::from_secs(3),
            scheduled_delay: Duration::from_secs(5),
            max_queue_size: 2_048,
            max_export_batch_size: 512,
        }
    }
}

/// Builds a provider with a native OTLP/HTTP-protobuf batch exporter.
///
/// This function creates no global provider. Retain the returned provider until
/// shutdown, then call `force_flush` and `shutdown_with_timeout` explicitly.
pub fn otlp_provider(config: &Config, otlp: OtlpConfig) -> Result<SdkTracerProvider, OtlpError> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_protocol(opentelemetry_otlp::Protocol::HttpBinary)
        .with_endpoint(otlp.endpoint)
        .with_timeout(otlp.export_timeout)
        .build()?;
    let batch_config = BatchConfigBuilder::default()
        .with_max_queue_size(otlp.max_queue_size)
        .with_max_export_batch_size(otlp.max_export_batch_size)
        .with_scheduled_delay(otlp.scheduled_delay)
        .build();
    let resource = Resource::builder()
        .with_service_name(config.service_name)
        .with_attributes([KeyValue::new("service.version", config.service_version)])
        .build();

    let processor = BatchSpanProcessor::builder(exporter)
        .with_batch_config(batch_config)
        .build();

    Ok(SdkTracerProvider::builder()
        .with_resource(resource)
        .with_span_processor(processor)
        .build())
}

/// Converts `tracing` spans into OpenTelemetry spans for `provider`.
///
/// The layer records only fields emitted by the application's `tracing` spans.
pub fn otlp_layer<S>(
    provider: &SdkTracerProvider,
    instrumentation_scope: &'static str,
) -> tracing_opentelemetry::OpenTelemetryLayer<S, SdkTracer>
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    tracing_opentelemetry::layer().with_tracer(provider.tracer(instrumentation_scope))
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
    use tracing_subscriber::layer::SubscriberExt;

    use crate::{Config, OutputFormat};

    use super::{otlp_layer, otlp_provider, OtlpConfig};

    #[test]
    fn layer_exports_tracing_span_attributes_and_provider_shuts_down() {
        let exporter = InMemorySpanExporter::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(exporter.clone())
            .build();
        let subscriber = tracing_subscriber::registry().with(otlp_layer(&provider, "observe-test"));

        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("physics.step", tick = 7_u64, bodies = 2_u64);
            let _entered = span.enter();
        });

        provider.force_flush().unwrap();
        let spans = exporter.get_finished_spans().unwrap();
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "physics.step");
        let attributes = spans[0]
            .attributes
            .iter()
            .map(|attribute| (attribute.key.as_str().to_owned(), attribute.value.to_string()))
            .collect::<Vec<_>>();
        assert!(attributes.contains(&("tick".to_owned(), "7".to_owned())));
        assert!(attributes.contains(&("bodies".to_owned(), "2".to_owned())));
        provider.shutdown().unwrap();
        assert!(exporter.is_shutdown_called());
    }

    #[test]
    #[ignore = "requires a loopback OTLP/HTTP collector"]
    fn http_exporter_sends_resource_and_parent_child_spans() {
        let config = Config {
            service_name: "hafley-observe-proof",
            service_version: "0.1.0-proof",
            default_filter: "info",
            format: OutputFormat::Json,
            ansi: false,
        };
        let provider = otlp_provider(
            &config,
            OtlpConfig {
                endpoint: std::env::var("HAFLEY_OTLP_PROOF_ENDPOINT").unwrap(),
                export_timeout: Duration::from_secs(1),
                scheduled_delay: Duration::from_secs(60),
                max_queue_size: 16,
                max_export_batch_size: 16,
            },
        )
        .unwrap();
        let subscriber = tracing_subscriber::registry().with(otlp_layer(&provider, "proof"));

        tracing::subscriber::with_default(subscriber, || {
            let parent = tracing::info_span!("proof.parent", tick = 7_u64);
            let _parent = parent.enter();
            let child = tracing::info_span!("proof.child", bodies = 2_u64);
            let _child = child.enter();
        });

        provider.force_flush().unwrap();
        provider.shutdown_with_timeout(Duration::from_secs(2)).unwrap();
    }

    #[test]
    fn unavailable_loopback_export_does_not_block_span_completion() {
        let config = Config {
            service_name: "hafley-observe-unavailable",
            service_version: "0.1.0-test",
            default_filter: "info",
            format: OutputFormat::Json,
            ansi: false,
        };
        let provider = otlp_provider(
            &config,
            OtlpConfig {
                endpoint: "http://127.0.0.1:9".to_owned(),
                export_timeout: Duration::from_millis(100),
                scheduled_delay: Duration::from_secs(60),
                max_queue_size: 16,
                max_export_batch_size: 16,
            },
        )
        .unwrap();
        let subscriber = tracing_subscriber::registry().with(otlp_layer(&provider, "unavailable"));

        let completed_at = Instant::now();
        tracing::subscriber::with_default(subscriber, || {
            let span = tracing::info_span!("proof.unavailable", tick = 8_u64);
            let _entered = span.enter();
        });
        assert!(completed_at.elapsed() < Duration::from_millis(10));
        assert!(provider.force_flush().is_err());
        let _ = provider.shutdown_with_timeout(Duration::from_millis(250));
    }
}
