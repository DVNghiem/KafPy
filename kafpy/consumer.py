"""Consumer wrapper for KafPy."""

from __future__ import annotations

from typing import Callable

__all__ = [
    "Consumer",
]


class Consumer:
    """Python wrapper around the Rust Consumer from _kafpy.

    Provides the same interface as the Rust consumer with Python-friendly
    type hints. Consumers are created via kafpy.Consumer(config), which
    returns this Python wrapper that delegates to the Rust implementation.
    """

    def __init__(self, config: object) -> None:
        """Initialize a Consumer with the given configuration.

        Args:
            config: A ConsumerConfig instance with bootstrap_servers, group_id, topics, etc.
        """
        import _kafpy

        # Convert Python config to Rust config
        rust_config = config.to_rust()
        self._consumer: _kafpy.Consumer = _kafpy.Consumer(rust_config)

    def add_handler(self, topic: str, handler: Callable[[object], None]) -> None:
        """Register a handler for a topic.

        Args:
            topic: The Kafka topic to subscribe to.
            handler: A callable that takes a KafkaMessage and returns None.
        """
        self._consumer.add_handler(topic, handler)

    def start(self) -> None:
        """Start the consumer."""
        self._consumer.start()

    def stop(self) -> None:
        """Stop the consumer."""
        self._consumer.stop()
