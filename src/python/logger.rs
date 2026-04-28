//! Python logging bridge - forwards log messages to Python's logging module.
//! This is the primary logging channel for KafPy.
//! Users configure logging in their Python code.
//! Rust tracing is used only for internal debug warnings.

use pyo3::prelude::*;

/// Log a message through Python's logging module.
/// Level can be "DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL".
pub fn log(level: &str, message: &str) {
    Python::attach(|py| {
        let logging = match py.import("logging") {
            Ok(l) => l,
            Err(_) => return,
        };
        let logger = match logging.call_method("getLogger", ("kafpy",), None) {
            Ok(l) => l,
            Err(_) => return,
        };
        let level_val = match logging.getattr(level) {
            Ok(v) => v,
            Err(_) => return,
        };
        let level_int: u32 = match level_val.extract() {
            Ok(v) => v,
            Err(_) => return,
        };
        let _ = logger.call_method1("log", (level_int, message));
    });
}

/// Log a formatted message with structured fields through Python logging.
/// Example: log_info("worker started", "worker_id", "0")
pub fn log_format(level: &str, msg: &str, kv: &[(&str, &str)]) {
    let mut full = String::from(msg);
    for (key, val) in kv {
        full.push_str(&format!(" {}={}", key, val));
    }
    log(level, &full);
}
