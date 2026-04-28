//! PyO3-exposed configuration types for missing features.
//!
//! Mirrors the internal Rust types (RetryPolicy, ObservabilityConfig, FailureReason)
//! so Python users can configure retry, observability, and inspect failure categories.

use pyo3::prelude::*;

use crate::failure::reason::{FailureCategory as RustFailureCategory, FailureReason as RustFailureReason};
use crate::observability::config::{LogFormat as RustLogFormat, ObservabilityConfig as RustObservabilityConfig};
use crate::retry::policy::RetryPolicy;

// ─── PyRetryPolicy ────────────────────────────────────────────────────────────

/// Retry policy configuration for message processing.
///
/// Uses milliseconds for delay values (Python-friendly) and converts
/// to `Duration` internally when converting to the Rust `RetryPolicy`.
///
/// Defaults match the Rust `RetryPolicy::default()`:
/// - max_attempts: 3
/// - base_delay_ms: 100
/// - max_delay_ms: 30000
/// - jitter_factor: 0.1
#[derive(Debug, Clone)]
#[pyclass]
pub struct PyRetryPolicy {
    /// Maximum number of retry attempts before routing to DLQ.
    #[pyo3(get)]
    pub max_attempts: usize,
    /// Initial delay between retries in milliseconds.
    #[pyo3(get)]
    pub base_delay_ms: u64,
    /// Maximum delay cap in milliseconds (exponential growth stops at this value).
    #[pyo3(get)]
    pub max_delay_ms: u64,
    /// Jitter factor in range [0.0, 1.0]. 0.1 means ±10% randomization.
    #[pyo3(get)]
    pub jitter_factor: f64,
}

#[pymethods]
impl PyRetryPolicy {
    #[new]
    #[pyo3(signature = (
        max_attempts=3,
        base_delay_ms=100,
        max_delay_ms=30000,
        jitter_factor=0.1,
    ))]
    pub fn new(
        max_attempts: usize,
        base_delay_ms: u64,
        max_delay_ms: u64,
        jitter_factor: f64,
    ) -> Self {
        Self {
            max_attempts,
            base_delay_ms,
            max_delay_ms,
            jitter_factor,
        }
    }

    /// Convert to the internal Rust `RetryPolicy`.
    ///
    /// Transforms millisecond fields into `Duration` values as expected
    /// by the Rust core.
    pub fn __repr__(&self) -> String {
        format!(
            "RetryPolicy(max_attempts={}, base_delay_ms={}, max_delay_ms={}, jitter_factor={})",
            self.max_attempts, self.base_delay_ms, self.max_delay_ms, self.jitter_factor
        )
    }
}

impl PyRetryPolicy {
    /// Convert to the internal Rust `RetryPolicy`.
    ///
    /// Transforms millisecond fields into `Duration` values as expected
    /// by the Rust core.
    pub fn to_rust(&self) -> RetryPolicy {
        RetryPolicy::new(
            self.max_attempts,
            std::time::Duration::from_millis(self.base_delay_ms),
            std::time::Duration::from_millis(self.max_delay_ms),
            self.jitter_factor,
        )
    }
}

// ─── PyObservabilityConfig ─────────────────────────────────────────────────────

/// Observability configuration for metrics and tracing.
///
/// When `otlp_endpoint` is None, tracing is disabled (zero-cost).
///
/// Defaults:
/// - otlp_endpoint: None
/// - service_name: "kafpy"
/// - sampling_ratio: 1.0
/// - log_format: "pretty"
#[derive(Debug, Clone)]
#[pyclass]
pub struct PyObservabilityConfig {
    /// OTLP exporter endpoint (e.g., "http://localhost:4317").
    /// None means tracing is disabled.
    #[pyo3(get)]
    pub otlp_endpoint: Option<String>,
    /// Service name for OTLP resource.
    #[pyo3(get)]
    pub service_name: String,
    /// Sampling ratio (0.0–1.0). 1.0 = sample everything.
    #[pyo3(get)]
    pub sampling_ratio: f64,
    /// Log format: "json", "pretty", or "simple".
    #[pyo3(get)]
    pub log_format: String,
}

#[pymethods]
impl PyObservabilityConfig {
    #[new]
    #[pyo3(signature = (
        otlp_endpoint=None,
        service_name="kafpy".to_string(),
        sampling_ratio=1.0,
        log_format="pretty".to_string(),
    ))]
    pub fn new(
        otlp_endpoint: Option<String>,
        service_name: String,
        sampling_ratio: f64,
        log_format: String,
    ) -> Self {
        Self {
            otlp_endpoint,
            service_name,
            sampling_ratio,
            log_format,
        }
    }

    /// Convert to the internal Rust `ObservabilityConfig`.
    pub fn __repr__(&self) -> String {
        format!(
            "ObservabilityConfig(otlp_endpoint={:?}, service_name={:?}, sampling_ratio={}, log_format={:?})",
            self.otlp_endpoint, self.service_name, self.sampling_ratio, self.log_format
        )
    }
}

impl PyObservabilityConfig {
    /// Convert to the internal Rust `ObservabilityConfig`.
    pub fn to_rust(&self) -> RustObservabilityConfig {
        let log_format = match self.log_format.as_str() {
            "json" => RustLogFormat::Json,
            "simple" => RustLogFormat::Simple,
            _ => RustLogFormat::Pretty,
        };
        RustObservabilityConfig {
            otlp_endpoint: self.otlp_endpoint.clone(),
            service_name: self.service_name.clone(),
            sampling_ratio: self.sampling_ratio,
            log_format,
        }
    }
}

// ─── PyFailureCategory ──────────────────────────────────────────────────────────

/// High-level failure category for classifying message processing errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[pyclass(eq, eq_int)]
pub enum PyFailureCategory {
    /// Transient failures that may succeed on retry (e.g., network timeout).
    Retryable,
    /// Permanent failures that indicate a bad message (e.g., poison message).
    Terminal,
    /// Failures that should not be retried but aren't necessarily bad messages.
    NonRetryable,
}

#[pymethods]
impl PyFailureCategory {
    pub fn __repr__(&self) -> String {
        match self {
            Self::Retryable => "FailureCategory.Retryable".to_string(),
            Self::Terminal => "FailureCategory.Terminal".to_string(),
            Self::NonRetryable => "FailureCategory.NonRetryable".to_string(),
        }
    }
}

impl From<RustFailureCategory> for PyFailureCategory {
    fn from(cat: RustFailureCategory) -> Self {
        match cat {
            RustFailureCategory::Retryable => PyFailureCategory::Retryable,
            RustFailureCategory::Terminal => PyFailureCategory::Terminal,
            RustFailureCategory::NonRetryable => PyFailureCategory::NonRetryable,
        }
    }
}

// ─── PyFailureReason ────────────────────────────────────────────────────────────

/// A specific failure reason with its category and human-readable description.
///
/// Represents the classification of why a message failed processing,
/// used for retry and DLQ routing decisions.
#[derive(Debug, Clone)]
#[pyclass]
pub struct PyFailureReason {
    /// The failure category (Retryable, Terminal, or NonRetryable).
    #[pyo3(get)]
    pub category: PyFailureCategory,
    /// Human-readable description of the failure.
    #[pyo3(get)]
    pub description: String,
}

#[pymethods]
impl PyFailureReason {
    #[new]
    pub fn new(category: PyFailureCategory, description: String) -> Self {
        Self { category, description }
    }

    pub fn __repr__(&self) -> String {
        format!("FailureReason(category={}, description={:?})", self.category.__repr__(), self.description)
    }
}

impl From<RustFailureReason> for PyFailureReason {
    fn from(reason: RustFailureReason) -> Self {
        let category: PyFailureCategory = reason.category().into();
        let description = reason.to_string();
        Self { category, description }
    }
}