// src/observability/config.rs
// Observability configuration for metrics and tracing

use tracing::Level;

/// Log format for tracing-subscriber fmt layer.
#[derive(Debug, Clone, Copy, Default)]
pub enum LogFormat {
    Json,
    Pretty,
    #[default]
    Simple,
}

/// Per-component log levels for structured logging (OBS-37).
#[derive(Debug, Clone)]
pub struct ComponentLogLevels {
    pub worker_loop: Level,
    pub dispatcher: Level,
    pub accumulator: Level,
}

impl Default for ComponentLogLevels {
    fn default() -> Self {
        Self {
            worker_loop: Level::INFO,
            dispatcher: Level::DEBUG,
            accumulator: Level::DEBUG,
        }
    }
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
    /// Polling interval for Kafka metrics (consumer_lag, highwater, etc.).
    /// Default: 10 seconds.
    pub kafka_poll_interval: std::time::Duration,
    /// Per-component log levels (OBS-37).
    pub component_log_levels: ComponentLogLevels,
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            otlp_endpoint: None, // Tracing disabled by default — zero-cost
            service_name: "kafpy".to_string(),
            sampling_ratio: 1.0,
            log_format: LogFormat::Pretty,
            kafka_poll_interval: std::time::Duration::from_secs(10),
            component_log_levels: ComponentLogLevels::default(),
        }
    }
}
