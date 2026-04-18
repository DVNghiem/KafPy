// src/observability/config.rs
// Observability configuration for metrics and tracing

/// Log format for tracing-subscriber fmt layer.
#[derive(Debug, Clone, Copy, Default)]
pub enum LogFormat {
    Json,
    Pretty,
    #[default]
    Simple,
}

/// Observability configuration for KafPy.
///
/// Allows user to configure OTLP endpoint, service name, sampling rate,
/// and log format. Zero-cost when otlp_endpoint is None.
#[derive(Debug, Clone)]
pub struct ObservabilityConfig {
    /// OTLP exporter endpoint (e.g., "http://localhost:4317").
    /// None = tracing disabled (zero-cost).
    pub otlp_endpoint: Option<String>,
    /// Service name for OTLP resource.
    pub service_name: String,
    /// Sampling ratio (0.0 to 1.0). 1.0 = sample everything.
    pub sampling_ratio: f64,
    /// Log format for tracing-subscriber fmt layer.
    pub log_format: LogFormat,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: None, // Tracing disabled by default — zero-cost
            service_name: "kafpy".to_string(),
            sampling_ratio: 1.0,
            log_format: LogFormat::Pretty,
        }
    }
}
