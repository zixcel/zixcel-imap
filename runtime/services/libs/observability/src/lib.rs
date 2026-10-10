use anyhow::Result;
use opentelemetry::KeyValue;
use opentelemetry::global;
use opentelemetry::metrics::{Counter, Histogram, Meter};
use opentelemetry::trace::{TraceContextExt, TracerProvider as _};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace as sdktrace;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{
    DefaultOnBodyChunk, DefaultOnEos, DefaultOnRequest, MakeSpan, OnFailure, OnResponse, TraceLayer,
};
use tracing::field::Empty;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub struct Config {
    pub service_name: String,
    pub service_version: Option<String>,
    pub log_filter: Option<String>,
    pub otlp_endpoint: Option<String>,
}

impl Config {
    pub fn from_env(service_name: impl Into<String>) -> Self {
        let service_name = service_name.into();
        let service_name = std::env::var("OTEL_SERVICE_NAME").unwrap_or(service_name);
        let service_version = std::env::var("OTEL_SERVICE_VERSION").ok();
        let log_filter = std::env::var("RUST_LOG").ok();
        let otlp_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
        Self {
            service_name,
            service_version,
            log_filter,
            otlp_endpoint,
        }
    }
}

pub struct Guard {
    provider: sdktrace::SdkTracerProvider,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.provider.shutdown();
    }
}

static HTTP_METRICS: OnceLock<Arc<HttpMetrics>> = OnceLock::new();
static JOB_METRICS: OnceLock<JobMetrics> = OnceLock::new();

pub fn init(config: Config) -> Result<Guard> {
    global::set_text_map_propagator(TraceContextPropagator::new());

    let resource = Resource::builder_empty()
        .with_attributes(
            [
                Some(KeyValue::new("service.name", config.service_name.clone())),
                config
                    .service_version
                    .clone()
                    .map(|version| KeyValue::new("service.version", version)),
            ]
            .into_iter()
            .flatten(),
        )
        .build();

    let mut provider = sdktrace::SdkTracerProvider::builder().with_resource(resource);
    if let Some(endpoint) = &config.otlp_endpoint {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(endpoint)
            .build()?;
        provider = provider.with_batch_exporter(exporter);
    }
    let provider = provider.build();
    let tracer = provider.tracer(config.service_name.clone());
    global::set_tracer_provider(provider.clone());

    init_metrics(&config.service_name);

    let filter = config
        .log_filter
        .as_deref()
        .and_then(|value| EnvFilter::try_new(value).ok())
        .or_else(|| EnvFilter::try_from_default_env().ok())
        .unwrap_or_else(|| EnvFilter::new("info"));

    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .with_filter(filter);

    tracing_subscriber::registry()
        .with(otel_layer)
        .with(fmt_layer)
        .init();

    Ok(Guard { provider })
}

fn init_metrics(service_name: &str) {
    let meter = global::meter_provider().meter_with_scope(
        opentelemetry::InstrumentationScope::builder(service_name.to_owned()).build(),
    );
    let _ = HTTP_METRICS.set(Arc::new(HttpMetrics::new(&meter)));
    let _ = JOB_METRICS.set(JobMetrics::new(&meter));
}

#[derive(Debug)]
struct HttpMetrics {
    request_counter: Counter<u64>,
    error_counter: Counter<u64>,
    latency_histogram: Histogram<f64>,
}

impl HttpMetrics {
    fn new(meter: &Meter) -> Self {
        Self {
            request_counter: meter
                .u64_counter("http.server.requests")
                .with_description("Total HTTP requests")
                .build(),
            error_counter: meter
                .u64_counter("http.server.errors")
                .with_description("HTTP requests with server errors")
                .build(),
            latency_histogram: meter
                .f64_histogram("http.server.duration")
                .with_description("HTTP server request duration")
                .with_unit("s")
                .build(),
        }
    }

    fn record(&self, status: http::StatusCode, latency: Duration) {
        let status_code = status.as_u16() as i64;
        let attrs = [KeyValue::new("http.status_code", status_code)];
        self.request_counter.add(1, &attrs);
        if status.is_server_error() {
            self.error_counter.add(1, &attrs);
        }
        self.latency_histogram.record(latency.as_secs_f64(), &attrs);
    }

    fn record_failure(&self, latency: Duration) {
        let attrs = [KeyValue::new("http.status_code", 0)];
        self.request_counter.add(1, &attrs);
        self.error_counter.add(1, &attrs);
        self.latency_histogram.record(latency.as_secs_f64(), &attrs);
    }
}

#[derive(Clone)]
pub struct HttpMetricsOnResponse {
    metrics: Option<Arc<HttpMetrics>>,
}

impl<B> OnResponse<B> for HttpMetricsOnResponse {
    fn on_response(self, response: &http::Response<B>, latency: Duration, _span: &tracing::Span) {
        if let Some(metrics) = self.metrics.as_ref() {
            metrics.record(response.status(), latency);
        }
    }
}

#[derive(Clone)]
pub struct HttpMetricsOnFailure {
    metrics: Option<Arc<HttpMetrics>>,
}

impl<FailureClass> OnFailure<FailureClass> for HttpMetricsOnFailure {
    fn on_failure(&mut self, _failure: FailureClass, latency: Duration, _span: &tracing::Span) {
        if let Some(metrics) = self.metrics.as_ref() {
            metrics.record_failure(latency);
        }
    }
}

#[derive(Debug)]
struct JobMetrics {
    run_counter: Counter<u64>,
    error_counter: Counter<u64>,
}

impl JobMetrics {
    fn new(meter: &Meter) -> Self {
        Self {
            run_counter: meter
                .u64_counter("job.runs")
                .with_description("Background job runs")
                .build(),
            error_counter: meter
                .u64_counter("job.errors")
                .with_description("Background job failures")
                .build(),
        }
    }

    fn record(&self, job_name: &str, success: bool) {
        let attrs = [KeyValue::new("job", job_name.to_string())];
        self.run_counter.add(1, &attrs);
        if !success {
            self.error_counter.add(1, &attrs);
        }
    }
}

pub fn record_job_result(job_name: &str, success: bool) {
    if let Some(metrics) = JOB_METRICS.get() {
        metrics.record(job_name, success);
    }
}

#[derive(Clone)]
pub struct HttpMakeSpan;

impl<B> MakeSpan<B> for HttpMakeSpan {
    fn make_span(&mut self, request: &http::Request<B>) -> tracing::Span {
        let span = tracing::info_span!(
            "http.request",
            method = %request.method(),
            path = %request.uri().path(),
            trace_id = Empty,
            span_id = Empty
        );
        let context = span.context();
        let span_ref = context.span();
        let span_context = span_ref.span_context();
        if span_context.is_valid() {
            span.record("trace_id", tracing::field::display(span_context.trace_id()));
            span.record("span_id", tracing::field::display(span_context.span_id()));
        }
        span
    }
}

pub fn http_trace_layer<B>() -> TraceLayer<
    SharedClassifier<ServerErrorsAsFailures>,
    HttpMakeSpan,
    DefaultOnRequest,
    HttpMetricsOnResponse,
    DefaultOnBodyChunk,
    DefaultOnEos,
    HttpMetricsOnFailure,
>
where
    HttpMakeSpan: MakeSpan<B>,
{
    let metrics = HTTP_METRICS.get().cloned();
    TraceLayer::new_for_http()
        .make_span_with(HttpMakeSpan)
        .on_response(HttpMetricsOnResponse {
            metrics: metrics.clone(),
        })
        .on_failure(HttpMetricsOnFailure { metrics })
}
