use crate::observability::config::ObservabilityConfig;
use tracing_subscriber::fmt::format::FmtSpan;

pub struct Logger;

impl Logger {
    /// Initialize structured logging with ObservabilityConfig.
    ///
    /// Applies log format (json/pretty/simple) and per-component log levels.
    /// Wires tracing_log::LogTracer for Python log forwarding (OBS-38).
    pub fn init(config: &ObservabilityConfig) {
        // Build EnvFilter from per-component log levels (OBS-37)
        let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| {
                let levels = &config.component_log_levels;
                format!(
                    "kafpy.worker_loop={},kafpy.dispatcher={},kafpy.accumulator={},info",
                    levels.worker_loop,
                    levels.dispatcher,
                    levels.accumulator
                )
                .into()
            });

        match config.log_format {
            crate::observability::LogFormat::Json => {
                tracing_subscriber::fmt()
                    .with_env_filter(env_filter)
                    .with_line_number(true)
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_span_events(FmtSpan::CLOSE)
                    .json()
                    .init();
            }
            crate::observability::LogFormat::Pretty => {
                tracing_subscriber::fmt()
                    .with_env_filter(env_filter)
                    .with_line_number(true)
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_span_events(FmtSpan::CLOSE)
                    .pretty()
                    .init();
            }
            crate::observability::LogFormat::Simple | crate::observability::LogFormat::Default => {
                tracing_subscriber::fmt()
                    .with_env_filter(env_filter)
                    .with_line_number(true)
                    .with_thread_ids(true)
                    .with_file(true)
                    .with_span_events(FmtSpan::CLOSE)
                    .init();
            }
        }

        // Wire Python logging to tracing via LogTracer (OBS-38)
        let _ = tracing_log::LogTracer::init();
    }
}
