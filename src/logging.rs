pub struct Logger;

impl Logger {
    /// Initialize structured logging
    pub fn init() {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info,consumer=debug".into()),
            )
            .with_line_number(true)
            .with_thread_ids(true)
            .with_file(true)
            .init();
    }
}
