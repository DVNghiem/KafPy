"""Tests for bootstrap_timeout_ms configuration.

This test verifies that the bootstrap_timeout_ms configuration option exists and can be set
on ConsumerConfig to control how long consumer.start() blocks when the Kafka broker is unavailable.

When Kafka broker is unavailable, consumer.start() blocks for ~20 seconds in the background thread.
The bootstrap_timeout_ms config should enable fail-fast behavior.
"""

import pytest


class TestBootstrapTimeoutConfig:
    """Test that bootstrap_timeout_ms configuration option exists and works."""

    def test_consumer_config_accepts_bootstrap_timeout_ms_kwarg(self):
        """Bootstrap timeout: ConsumerConfig should accept bootstrap_timeout_ms as a keyword argument."""
        from kafpy import ConsumerConfig

        # ConsumerConfig constructor should accept bootstrap_timeout_ms
        config = ConsumerConfig(
            bootstrap_servers="localhost:9092",
            group_id="test-group",
            topics=["test-topic"],
            bootstrap_timeout_ms=3000,
        )
        assert config is not None

    def test_consumer_config_exposes_bootstrap_timeout_ms(self):
        """Bootstrap timeout: ConsumerConfig should expose bootstrap_timeout_ms attribute."""
        from kafpy import ConsumerConfig

        config = ConsumerConfig(
            bootstrap_servers="localhost:9092",
            group_id="test-group",
            topics=["test-topic"],
            bootstrap_timeout_ms=3000,
        )

        # The config should have a bootstrap_timeout_ms attribute
        assert hasattr(config, 'bootstrap_timeout_ms'), (
            "ConsumerConfig missing bootstrap_timeout_ms attribute"
        )
        assert config.bootstrap_timeout_ms == 3000

    def test_consumer_config_bootstrap_timeout_ms_defaults_to_none(self):
        """Bootstrap timeout: when not set, bootstrap_timeout_ms should be None."""
        from kafpy import ConsumerConfig

        config = ConsumerConfig(
            bootstrap_servers="localhost:9092",
            group_id="test-group",
            topics=["test-topic"],
        )

        # When not set, bootstrap_timeout_ms should be None
        assert hasattr(config, 'bootstrap_timeout_ms'), (
            "ConsumerConfig missing bootstrap_timeout_ms attribute"
        )
        assert config.bootstrap_timeout_ms is None

    def test_consumer_created_with_bootstrap_timeout_ms(self):
        """Bootstrap timeout: Consumer should work with config containing bootstrap_timeout_ms."""
        from kafpy import Consumer, ConsumerConfig

        config = ConsumerConfig(
            bootstrap_servers="localhost:9092",
            group_id="test-group",
            topics=["test-topic"],
            bootstrap_timeout_ms=3000,
        )

        # Should be able to create a Consumer with this config
        consumer = Consumer(config)
        assert consumer is not None
