"""Consumer wrapper for KafPy."""

from __future__ import annotations

from typing import Any, Callable

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
        import kafpy._kafpy as _kafpy

        # Convert Python config to Rust config
        rust_config = config.to_rust()
        self._consumer: _kafpy.Consumer = _kafpy.Consumer(rust_config)

    def add_handler(
        self,
        topic: str,
        handler: Callable[[object], None],
        *,
        mode: str | None = None,
        batch_max_size: int | None = None,
        batch_max_wait_ms: int | None = None,
        timeout_ms: int | None = None,
    ) -> None:
        """Register a handler for a topic.

        Args:
            topic: The Kafka topic to subscribe to.
            handler: A callable that takes a KafkaMessage and returns None.
            mode: Optional handler mode ("sync", "async", "batch_sync", "batch_async").
            batch_max_size: Max messages per batch (batch modes only).
            batch_max_wait_ms: Max wait time per batch in ms (batch modes only).
            timeout_ms: Per-handler execution timeout in milliseconds.
        """
        self._consumer.add_handler(topic, handler, mode, batch_max_size, batch_max_wait_ms, timeout_ms)

    def start(self) -> object:
        """Start the consumer. Returns an awaitable coroutine."""
        return self._consumer.start()

    def stop(self) -> None:
        """Stop the consumer gracefully.

        Initiates drain: waits for in-flight messages to complete,
        then shuts down the consumer through a 4-phase lifecycle
        (Running → Draining → Finalizing → Done).
        """
        self._consumer.stop()

    def status(self) -> dict[str, Any]:
        """Return the current runtime snapshot as a dictionary.

        Contains worker states, queue depths, accumulator info,
        and consumer lag summary. Zero-cost when not called.

        Returns:
            Dictionary with keys: timestamp, worker_states, queue_depths,
            accumulator_info, consumer_lag_summary.
        """
        return self._consumer.status()