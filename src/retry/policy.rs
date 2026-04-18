use rand::Rng;
use std::time::Duration;

/// Retry policy configuration for message processing.
///
/// # Defaults
/// - max_attempts: 3
/// - base_delay: 100ms
/// - max_delay: 30s
/// - jitter_factor: 0.1
#[derive(Debug, Clone, PartialEq)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts before routing to DLQ.
    pub max_attempts: usize,
    /// Initial delay between retries.
    pub base_delay: Duration,
    /// Maximum delay cap (exponential growth stops at this value).
    pub max_delay: Duration,
    /// Jitter factor in range [0.0, 1.0]. 0.1 means ±10% randomization.
    pub jitter_factor: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            jitter_factor: 0.1,
        }
    }
}

impl RetryPolicy {
    /// Creates a new RetryPolicy with custom values.
    pub fn new(
        max_attempts: usize,
        base_delay: Duration,
        max_delay: Duration,
        jitter_factor: f64,
    ) -> Self {
        Self {
            max_attempts,
            base_delay,
            max_delay,
            jitter_factor,
        }
    }

    /// Returns the schedule for computing retry delays.
    pub fn schedule(&self) -> RetrySchedule {
        RetrySchedule {
            base_delay: self.base_delay,
            max_delay: self.max_delay,
            jitter_factor: self.jitter_factor,
        }
    }
}

/// Computes retry delays using exponential backoff with jitter.
///
/// Formula: min(base_delay * 2^attempt, max_delay) * (1 - jitter_factor + rng * jitter_factor * 2)
pub struct RetrySchedule {
    base_delay: Duration,
    max_delay: Duration,
    jitter_factor: f64,
}

impl RetrySchedule {
    /// Computes the delay for a given attempt number (0-indexed).
    ///
    /// Attempt 0 = first retry (after initial failure).
    /// Attempt 1 = second retry, etc.
    pub fn next_delay(&self, attempt: usize) -> Duration {
        // Exponential backoff: base_delay * 2^attempt
        let base_ms = self.base_delay.as_secs_f64();
        let max_ms = self.max_delay.as_secs_f64();
        let exp_delay = base_ms * 2_f64.powf(attempt as f64);

        // Cap at max_delay
        let capped = f64::min(exp_delay, max_ms);

        // Apply jitter: multiplier = 1 - jitter_factor + rng * jitter_factor * 2
        // This gives range [1 - jitter_factor, 1 + jitter_factor]
        let jitter_multiplier = {
            let mut rng = rand::thread_rng();
            let jitter: f64 = rng.gen_range(0.0..1.0);
            1.0 - self.jitter_factor + jitter * self.jitter_factor * 2.0
        };

        let final_secs = capped * jitter_multiplier;
        Duration::from_secs_f64(final_secs)
    }
}
