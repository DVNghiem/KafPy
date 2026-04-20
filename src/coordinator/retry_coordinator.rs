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
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::time::Duration;

/// Per-message retry state as an explicit enum.
///
/// Replaces the previous `MessageRetryState` struct which relied on HashMap
/// presence/absence to indicate state. The enum makes illegal states
/// unrepresentable and enables exhaustive match checking.
#[derive(Debug, Clone)]
pub enum RetryState {
    /// Message is being tracked for retry attempts.
    Retrying {
        /// Topic name.
        topic: String,
        /// Partition number.
        partition: i32,
        /// Message offset.
        offset: i64,
        /// Current attempt number (1-based).
        attempt: usize,
        /// Most recent failure reason.
        last_failure: FailureReason,
        /// Time of first failure attempt.
        first_failure: DateTime<Utc>,
    },
    /// Message has exhausted all retry attempts and should be routed to DLQ.
    Exhausted {
        /// Topic name.
        topic: String,
        /// Partition number.
        partition: i32,
        /// Message offset.
        offset: i64,
        /// Final failure reason that caused exhaustion.
        last_failure: FailureReason,
        /// When the message first failed.
        first_failure: DateTime<Utc>,
    },
}

impl RetryState {
    /// Returns the topic for this retry state.
    pub fn topic(&self) -> &str {
        match self {
            RetryState::Retrying { topic, .. } => topic,
            RetryState::Exhausted { topic, .. } => topic,
        }
    }

    /// Returns the partition for this retry state.
    pub fn partition(&self) -> i32 {
        match self {
            RetryState::Retrying { partition, .. } => *partition,
            RetryState::Exhausted { partition, .. } => *partition,
        }
    }

    /// Returns the offset for this retry state.
    pub fn offset(&self) -> i64 {
        match self {
            RetryState::Retrying { offset, .. } => *offset,
            RetryState::Exhausted { offset, .. } => *offset,
        }
    }

    /// Returns the attempt count (only valid for Retrying state, 0 for Exhausted).
    pub fn attempt(&self) -> usize {
        match self {
            RetryState::Retrying { attempt, .. } => *attempt,
            RetryState::Exhausted { .. } => 0,
        }
    }
}

/// RetryCoordinator — thread-safe retry state machine.
///
/// Tracks attempt count per (topic, partition, offset).
/// On retryable failure: increments attempt, computes delay.
/// On max_attempts exceeded: signals DLQ routing.
/// On success: clears state (caller calls record_ack).
pub struct RetryCoordinator {
    state: Mutex<HashMap<RetryKey, RetryState>>,
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

        // Check if already exhausted (idempotent - calling record_failure after exhaustion)
        if let Some(RetryState::Exhausted { .. }) = state_guard.get(&key) {
            return (false, true, None);
        }

        match state_guard.entry(key) {
            Entry::Occupied(mut entry) => {
                let retry_state = entry.get();
                if let RetryState::Retrying {
                    attempt,
                    last_failure,
                    first_failure,
                    ..
                } = retry_state
                {
                    let new_attempt = *attempt + 1;

                    if new_attempt >= self.default_policy.max_attempts {
                        // Max attempts exceeded → transition to Exhausted
                        let new_state = RetryState::Exhausted {
                            topic: topic.to_string(),
                            partition,
                            offset,
                            last_failure: reason.clone(),
                            first_failure: *first_failure,
                        };
                        entry.insert(new_state);
                        return (false, true, None);
                    }

                    // Increment attempt and update failure
                    let schedule = self.default_policy.schedule();
                    let delay = schedule.next_delay(new_attempt - 1); // attempt 1 = first retry delay

                    // Update to new retry state with incremented attempt
                    let new_state = RetryState::Retrying {
                        topic: topic.to_string(),
                        partition,
                        offset,
                        attempt: new_attempt,
                        last_failure: reason.clone(),
                        first_failure: *first_failure,
                    };
                    entry.insert(new_state);

                    (true, false, Some(delay))
                } else {
                    // Exhausted state - should have been caught above
                    (false, true, None)
                }
            }
            Entry::Vacant(entry) => {
                // First failure for this message
                let first_failure = Utc::now();

                if 1 >= self.default_policy.max_attempts {
                    // Only one attempt allowed and it failed
                    let new_state = RetryState::Exhausted {
                        topic: topic.to_string(),
                        partition,
                        offset,
                        last_failure: reason.clone(),
                        first_failure,
                    };
                    entry.insert(new_state);
                    return (false, true, None);
                }

                // Schedule first retry
                let schedule = self.default_policy.schedule();
                let delay = schedule.next_delay(0); // attempt 1 = first retry delay

                let new_state = RetryState::Retrying {
                    topic: topic.to_string(),
                    partition,
                    offset,
                    attempt: 1,
                    last_failure: reason.clone(),
                    first_failure,
                };
                entry.insert(new_state);

                (true, false, Some(delay))
            }
        }
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
        state_guard.get(&key).map(|s| s.attempt()).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retryable_failure_triggers_retry() {
        let policy = RetryPolicy::default();
        let coordinator = RetryCoordinator::with_policy(policy);
        let reason = FailureReason::Retryable(crate::failure::reason::RetryableKind::NetworkTimeout);

        let (should_retry, should_dlq, delay) =
            coordinator.record_failure("topic", 0, 100, &reason);

        assert!(should_retry);
        assert!(!should_dlq);
        assert!(delay.is_some());
        assert_eq!(coordinator.attempt_count("topic", 0, 100), 1);
    }

    #[test]
    fn terminal_failure_skips_retry() {
        let coordinator = RetryCoordinator::with_policy(RetryPolicy::default());
        let reason = FailureReason::Terminal(crate::failure::TerminalKind::DeserializationFailed);

        let (should_retry, should_dlq, delay) =
            coordinator.record_failure("topic", 0, 100, &reason);

        assert!(!should_retry);
        assert!(should_dlq);
        assert!(delay.is_none());
    }

    #[test]
    fn max_attempts_exceeded_returns_no_retry() {
        let policy = RetryPolicy::new(2, Duration::from_millis(100), Duration::from_secs(30), 0.1);
        let coordinator = RetryCoordinator::with_policy(policy);
        let reason = FailureReason::Retryable(crate::failure::reason::RetryableKind::NetworkTimeout);

        // First attempt
        let (should_retry, should_dlq, _) = coordinator.record_failure("topic", 0, 100, &reason);
        assert!(should_retry);
        assert!(!should_dlq);

        // Second attempt
        let (should_retry, should_dlq, _) = coordinator.record_failure("topic", 0, 100, &reason);
        assert!(should_retry);
        assert!(!should_dlq);

        // Third attempt (exceeds max_attempts=2)
        let (should_retry, should_dlq, delay) =
            coordinator.record_failure("topic", 0, 100, &reason);
        assert!(!should_retry);
        assert!(should_dlq);
        assert!(delay.is_none());
    }

    #[test]
    fn success_clears_retry_state() {
        let coordinator = RetryCoordinator::with_policy(RetryPolicy::default());
        let reason = FailureReason::Retryable(crate::failure::reason::RetryableKind::NetworkTimeout);

        coordinator.record_failure("topic", 0, 100, &reason);
        assert_eq!(coordinator.attempt_count("topic", 0, 100), 1);

        coordinator.record_success("topic", 0, 100);
        assert_eq!(coordinator.attempt_count("topic", 0, 100), 0);
    }
}
