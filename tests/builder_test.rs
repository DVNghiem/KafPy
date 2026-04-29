//! Unit tests for ConsumerConfigBuilder in src/consumer/config.rs.
//!
//! Covers:
//! - Required field validation (brokers, group_id, topics)
//! - Successful build with minimal fields
//! - Successful build with all fields

use _kafpy::consumer::config::{
    AutoOffsetReset, ConsumerConfig, ConsumerConfigBuilder, PartitionAssignmentStrategy,
};
use std::time::Duration;

#[test]
fn consumer_builder_requires_brokers() {
    let result = ConsumerConfigBuilder::new()
        .group_id("test-group")
        .topics(["test-topic"])
        .build();
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("brokers"),
        "expected error about missing 'brokers', got: {msg}"
    );
}

#[test]
fn consumer_builder_requires_group_id() {
    let result = ConsumerConfigBuilder::new()
        .brokers("localhost:9092")
        .topics(["test-topic"])
        .build();
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("group_id"),
        "expected error about missing 'group_id', got: {msg}"
    );
}

#[test]
fn consumer_builder_requires_topics() {
    let result = ConsumerConfigBuilder::new()
        .brokers("localhost:9092")
        .group_id("test-group")
        .build();
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("topic"),
        "expected error about missing topics, got: {msg}"
    );
}

#[test]
fn consumer_builder_success_with_minimal_fields() {
    let config = ConsumerConfigBuilder::new()
        .brokers("localhost:9092")
        .group_id("test-group")
        .topics(["events"])
        .build()
        .expect("build should succeed with minimal required fields");
    assert_eq!(config.brokers, "localhost:9092");
    assert_eq!(config.group_id, "test-group");
    assert_eq!(config.topics, vec!["events"]);
}

#[test]
fn consumer_builder_with_all_fields() {
    let config = ConsumerConfigBuilder::new()
        .brokers("localhost:9092")
        .group_id("test-group")
        .topics(["events", "transactions"])
        .auto_offset_reset(AutoOffsetReset::Latest)
        .enable_auto_commit(true)
        .session_timeout(Duration::from_secs(60))
        .heartbeat_interval(Duration::from_secs(5))
        .max_poll_interval(Duration::from_secs(600))
        .security_protocol("PLAINTEXT")
        .fetch_min_bytes(2048)
        .max_partition_fetch_bytes(2097152)
        .partition_assignment_strategy(PartitionAssignmentStrategy::CooperativeSticky)
        .retry_backoff(Duration::from_millis(200))
        .dlq_topic_prefix("dlq.")
        .drain_timeout(60)
        .build()
        .expect("build should succeed with all fields");
    assert_eq!(config.brokers, "localhost:9092");
    assert_eq!(config.group_id, "test-group");
    assert_eq!(config.topics.len(), 2);
    assert_eq!(config.auto_offset_reset, AutoOffsetReset::Latest);
    assert!(config.enable_auto_commit);
    assert_eq!(config.session_timeout_ms, 60000);
    assert_eq!(config.heartbeat_interval_ms, 5000);
    assert_eq!(config.max_poll_interval_ms, 600000);
    assert_eq!(config.security_protocol.as_deref(), Some("PLAINTEXT"));
    assert_eq!(config.fetch_min_bytes, 2048);
    assert_eq!(config.max_partition_fetch_bytes, 2097152);
    assert_eq!(
        config.partition_assignment_strategy,
        PartitionAssignmentStrategy::CooperativeSticky
    );
    assert_eq!(config.retry_backoff_ms, 200);
    assert_eq!(config.dlq_topic_prefix, "dlq.");
    assert_eq!(config.drain_timeout_secs, 60);
}
