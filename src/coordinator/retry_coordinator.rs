//! RetryCoordinator — tracks per-message retry state and coordinates with OffsetCoordinator.
//!
//! Responsibilities:
//! - Track attempt count per topic-partition-offset
//! - Compute next retry delay via RetrySchedule
//! - Signal DLQ routing when max_attempts exceeded
//! - Does NOT call record_ack until final success (RETRY-03)

use crate::consumer::config::ConsumerConfig;
use crate::failure::FailureReason;
use crate::retry::RetryPolicy;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Per-message retry state
#[derive(Debug, Clone)]
pub struct MessageRetryState {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub attempt: usize,
    pub last_failure: Option<FailureReason>,
    pub first_failure: Option<DateTime<Utc>>,
}

/// RetryCoordinator — thread-safe retry state machine.
///
/// Tracks attempt count per (topic, partition, offset).
/// On retryable failure: increments attempt, computes delay.
/// On max_attempts exceeded: signals DLQ routing.
/// On success: clears state (caller calls record_ack).
pub struct RetryCoordinator {
    state: Mutex<HashMap<RetryKey, MessageRetryState>>,
    default_policy: RetryPolicy,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
struct RetryKey(String, i32, i64);

impl RetryCoordinator {
    /// Create a new RetryCoordinator with default policy from ConsumerConfig.
    pub fn new(config: &ConsumerConfig) -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
            default_policy: config.default_retry_policy.clone(),
        }
    }

    /// Create with explicit policy (for testing).
    pub fn with_policy(policy: RetryPolicy) -> Self {
        Self {
            state: Mutex::new(HashMap::new()),
            default_policy: policy,
        }
    }

    /// Record a failed attempt and return the retry decision.
    ///
    /// Returns `(should_retry, should_dlq, delay)`:
    /// - `should_retry=true, should_dlq=false, delay=Some(duration)` → schedule retry
    /// - `should_retry=false, should_dlq=true, delay=None` → route to DLQ
    pub fn record_failure(
        &self,
        topic: &str,
        partition: i32,
        offset: i64,
        reason: &FailureReason,
    ) -> (bool, bool, Option<Duration>) {
        let key = RetryKey(topic.to_string(), partition, offset);

        // Non-retryable or terminal → go directly to DLQ
        let category = reason.category();
        if category != crate::failure::FailureCategory::Retryable {
            return (false, true, None);
        }

        let mut state_guard = self.state.lock();
        let entry = state_guard.entry(key.clone()).or_insert_with(|| MessageRetryState {
            topic: topic.to_string(),
            partition,
            offset,
            attempt: 0,
            last_failure: None,
            first_failure: None,
        });

        entry.attempt += 1;
        entry.last_failure = Some(reason.clone());
        if entry.first_failure.is_none() {
            entry.first_failure = Some(Utc::now());
        }

        if entry.attempt >= self.default_policy.max_attempts {
            // Max attempts exceeded → DLQ
            state_guard.remove(&key);
            return (false, true, None);
        }

        // Compute next retry delay
        let schedule = self.default_policy.schedule();
        let delay = schedule.next_delay(entry.attempt - 1); // attempt 1 = first retry delay
        (true, false, Some(delay))
    }

    /// Record a successful ack — clears retry state.
    pub fn record_success(&self, topic: &str, partition: i32, offset: i64) {
        let key = RetryKey(topic.to_string(), partition, offset);
        let mut state_guard = self.state.lock();
        state_guard.remove(&key);
    }

    /// Returns the current attempt count for a message.
    pub fn attempt_count(&self, topic: &str, partition: i32, offset: i64) -> usize {
        let key = RetryKey(topic.to_string(), partition, offset);
        let state_guard = self.state.lock();
        state_guard.get(&key).map(|s| s.attempt).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_failure_triggers_retry() {
        let policy = RetryPolicy::default();
        let coordinator = RetryCoordinator::with_policy(policy);
        let reason = FailureReason::Retryable(crate::failure::RetryableKind::NetworkTimeout);

        let (should_retry, should_dlq, delay) = coordinator.record_failure("topic", 0, 100, &reason);

        assert!(should_retry);
        assert!(!should_dlq);
        assert!(delay.is_some());
        assert_eq!(coordinator.attempt_count("topic", 0, 100), 1);
    }

    #[test]
    fn terminal_failure_skips_retry() {
        let coordinator = RetryCoordinator::with_policy(RetryPolicy::default());
        let reason = FailureReason::Terminal(crate::failure::TerminalKind::DeserializationFailed);

        let (should_retry, should_dlq, delay) = coordinator.record_failure("topic", 0, 100, &reason);

        assert!(!should_retry);
        assert!(should_dlq);
        assert!(delay.is_none());
    }

    #[test]
    fn max_attempts_exceeded_returns_no_retry() {
        let policy = RetryPolicy::new(2, Duration::from_millis(100), Duration::from_secs(30), 0.1);
        let coordinator = RetryCoordinator::with_policy(policy);
        let reason = FailureReason::Retryable(crate::failure::RetryableKind::NetworkTimeout);

        // First attempt
        let (should_retry, should_dlq, _) = coordinator.record_failure("topic", 0, 100, &reason);
        assert!(should_retry);
        assert!(!should_dlq);

        // Second attempt
        let (should_retry, should_dlq, _) = coordinator.record_failure("topic", 0, 100, &reason);
        assert!(should_retry);
        assert!(!should_dlq);

        // Third attempt (exceeds max_attempts=2)
        let (should_retry, should_dlq, delay) = coordinator.record_failure("topic", 0, 100, &reason);
        assert!(!should_retry);
        assert!(should_dlq);
        assert!(delay.is_none());
    }

    #[test]
    fn success_clears_retry_state() {
        let coordinator = RetryCoordinator::with_policy(RetryPolicy::default());
        let reason = FailureReason::Retryable(crate::failure::RetryableKind::NetworkTimeout);

        coordinator.record_failure("topic", 0, 100, &reason);
        assert_eq!(coordinator.attempt_count("topic", 0, 100), 1);

        coordinator.record_success("topic", 0, 100);
        assert_eq!(coordinator.attempt_count("topic", 0, 100), 0);
    }
}