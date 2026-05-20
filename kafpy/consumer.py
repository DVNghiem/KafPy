"""Consumer wrapper for KafPy."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Callable

if TYPE_CHECKING:
    from kafpy.fanout import FanInRegistration, FanOutBuilder

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
        concurrency: int | None = None,
        middleware: list | None = None,
    ) -> None:
        """Register a handler for a topic.

        Args:
            topic: The Kafka topic to subscribe to.
            handler: A callable that takes a KafkaMessage and returns None.
            mode: Optional handler mode ("sync", "batch_sync"). "async" and "batch_async" are not supported.
            batch_max_size: Max messages per batch (batch modes only).
            batch_max_wait_ms: Max wait time per batch in ms (batch modes only).
            timeout_ms: Per-handler execution timeout in milliseconds.
            concurrency: Maximum concurrent executions for this handler. None = no limit.
            middleware: List of middleware instances (e.g., [Logging(), Metrics()]).
        """
        self._consumer.add_handler(topic, handler, mode, batch_max_size, batch_max_wait_ms, timeout_ms, concurrency, middleware)

    def start(self) -> None:
        """Start the consumer. Blocks until the consumer shuts down."""
        return self._consumer.start()

    def stop(self) -> None:
        """Stop the consumer gracefully.

        Initiates drain: waits for in-flight messages to complete,
        then shuts down the consumer through a 4-phase lifecycle
        (Running → Draining → Finalizing → Done).
        """
        self._consumer.stop()

    def __enter__(self) -> "Consumer":
        """Enter the context manager."""
        return self

    def register_fanout(
        self,
        group_name: str,
        sink_topics: list[str],
        handler: Callable[[object], None],
        *,
        max_fan_out: int | None = None,
        timeout_ms: int | None = None,
    ) -> "FanOutBuilder":
        """Register a fan-out group.

        Args:
            group_name: Identifier for this fan-out group.
            sink_topics: List of sink topic names to fan out to.
            handler: Python callable invoked for each sink topic.
            max_fan_out: Maximum concurrent sink branches (default 4, max 64).
            timeout_ms: Per-branch execution timeout in milliseconds.

        Returns:
            FanOutBuilder for configuring before calling .register()
        """
        from kafpy.fanout import FanOutBuilder

        return FanOutBuilder(
            consumer=self,
            group_name=group_name,
            sink_topics=sink_topics,
            handler=handler,
            max_fan_out=max_fan_out,
            timeout_ms=timeout_ms,
        )

    def register_fanin(
        self,
        handler_key: str,
        sources: list[str],
        handler: Callable[[object], None],
        *,
        timeout_ms: int | None = None,
    ) -> "FanInRegistration":
        """Register a fan-in handler: one callback that receives messages from multiple topics.

        Messages from all source topics are merged into a single round-robin stream
        and dispatched to the handler in order.

        Args:
            handler_key: Unique identifier for this handler (used in QueueManager).
            sources: List of Kafka topic names to subscribe to.
            handler: Python callable invoked for each message.
            timeout_ms: Per-handler execution timeout in milliseconds.

        Returns:
            FanInRegistration with handler_key, fan_in_id, and sources.

        Example::

            reg = consumer.register_fanin(
                handler_key="aggregator",
                sources=["topic-a", "topic-b", "topic-c"],
                handler=my_handler,
            )
        """
        from kafpy.fanout import FanInRegistration

        result = self._consumer.register_fanin(
            handler_key,
            sources,
            handler,
            timeout_ms,
        )
        return FanInRegistration(
            handler_key=result.handler_key,
            fan_in_id=result.fan_in_id,
            sources=result.sources,
        )

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        traceback: Any,
    ) -> bool:
        """Exit the context manager — stops the consumer gracefully.

        Args:
            exc_type: Exception type if an error occurred inside the with block.
            exc_val: Exception value if an error occurred.
            traceback: Traceback object if an error occurred.

        Returns:
            False — exceptions are NOT suppressed and propagate normally.
        """
        if exc_type is not None:
            import logging
            logging.getLogger("kafpy").info(
                f"Consumer context exit with exception {exc_type.__name__}, initiating graceful shutdown"
            )
        self.stop()
        return False  # don't suppress exceptions

    def status(self) -> dict[str, Any]:
        """Return the current runtime snapshot as a dictionary.

        Contains worker states, queue depths, accumulator info,
        and consumer lag summary. Zero-cost when not called.

        Returns:
            Dictionary with keys: timestamp, worker_states, queue_depths,
            accumulator_info, consumer_lag_summary.
        """
        return self._consumer.status()