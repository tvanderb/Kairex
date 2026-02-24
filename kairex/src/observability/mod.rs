use std::env;

use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

/// Guard that flushes the OpenTelemetry trace pipeline on drop.
pub struct OtelGuard {
    provider: Option<SdkTracerProvider>,
}

impl Drop for OtelGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.provider.take() {
            if let Err(e) = provider.shutdown() {
                eprintln!("failed to shutdown tracer provider: {e}");
            }
        }
    }
}

/// Initialize structured logging, metrics, and trace export.
///
/// - JSON logs to stdout (controlled by RUST_LOG, default: info)
/// - Prometheus metrics on 0.0.0.0:9090
/// - OTLP trace export to OTEL_EXPORTER_OTLP_ENDPOINT (default: http://tempo:4317)
///   Set to "" to disable trace export.
pub fn init() -> OtelGuard {
    // 1. Prometheus metrics HTTP listener on :9090
    metrics_exporter_prometheus::PrometheusBuilder::new()
        .with_http_listener(([0, 0, 0, 0], 9090))
        .install()
        .expect("failed to install Prometheus metrics exporter");

    // 2. OpenTelemetry tracer (if endpoint configured)
    let otel_endpoint =
        env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_else(|_| "http://tempo:4317".to_string());

    let (otel_layer, provider) = if otel_endpoint.is_empty() {
        (None, None)
    } else {
        match build_otel(otel_endpoint) {
            Some((layer, provider)) => (Some(layer), Some(provider)),
            None => (None, None),
        }
    };

    // 3. Compose: registry + otel_layer (first, wraps Registry directly) + json_fmt + env_filter
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .flatten_event(true)
        .with_span_list(true);

    tracing_subscriber::registry()
        .with(otel_layer)
        .with(json_layer)
        .with(env_filter)
        .init();

    OtelGuard { provider }
}

fn build_otel(
    endpoint: String,
) -> Option<(
    tracing_opentelemetry::OpenTelemetryLayer<
        tracing_subscriber::Registry,
        opentelemetry_sdk::trace::Tracer,
    >,
    SdkTracerProvider,
)> {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .map_err(|e| eprintln!("failed to build OTLP exporter: {e}, traces disabled"))
        .ok()?;

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(Resource::builder().with_service_name("kairex").build())
        .build();

    opentelemetry::global::set_tracer_provider(provider.clone());

    let tracer = provider.tracer("kairex");
    let layer = tracing_opentelemetry::layer().with_tracer(tracer);

    Some((layer, provider))
}
