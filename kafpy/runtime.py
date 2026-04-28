"""KafPy runtime with handler lifecycle."""

from __future__ import annotations

import inspect
import threading
from typing import Any, Callable

from .handlers import KafkaMessage, HandlerContext

__all__ = [
    "KafPy",
]


class KafPy:
    """Main KafPy runtime for consuming Kafka messages.

    Create a KafPy instance with a Consumer, register handlers using the
    @app.handler decorator or register_handler(), then call run() to start
    consuming messages.

    Example::

        config = kafpy.ConsumerConfig(
            bootstrap_servers="localhost:9092",
            group_id="my-group",
            topics=["my-topic"],
        )
        consumer = kafpy.Consumer(config)
        app = kafpy.KafPy(consumer)

        @app.handler(topic="my-topic")
        def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
            print(f"Received: {msg.value}")
            return kafpy.HandlerResult(action="ack")

        app.run()
    """

    def __init__(self, consumer: object) -> None:
        """Initialize KafPy with a Consumer.

        Args:
            consumer: A kafpy.Consumer instance wrapping the Rust consumer.
        """
        self._consumer = consumer
        self._handlers: dict[str, dict[str, Any]] = {}
        self._stopping = False
        self._loop_thread: threading.Thread | None = None

    def start(self) -> object:
        """Start consuming messages.

        Begins the consumer and dispatches messages to registered handlers.
        Returns an awaitable coroutine.
        """
        return self._consumer.start()

    def stop(self) -> None:
        """Stop the consumer gracefully.

        Initiates drain: waits for in-flight messages to complete,
        then shuts down the consumer.
        """
        self._stopping = True
        self._consumer.stop()

    def run(self) -> None:
        """Run the consumer until stop() is called.

        Blocks the current thread. Calls start() internally.
        """
        self.start()
        # Simple blocking loop - proper event loop integration comes later
        while not self._stopping:
            import time
            time.sleep(0.1)

    def handler(
        self,
        topic: str,
        *,
        routing: object | None = None,
        timeout_ms: int | None = None,
    ) -> Callable[[Callable], Callable]:
        """Decorator to register a handler for a topic.

        Args:
            topic: The Kafka topic to handle.
            routing: Optional routing configuration.
            timeout_ms: Per-handler execution timeout in milliseconds.
                Overrides ``ConsumerConfig.handler_timeout_ms``.

        Returns:
            A decorator that registers the decorated callable as a handler.

        Example::

            @app.handler(topic="my-topic")
            def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
                return kafpy.HandlerResult(action="ack")
        """

        def decorator(fn: Callable) -> Callable:
            self.register_handler(topic, fn, routing=routing, timeout_ms=timeout_ms)
            return fn

        return decorator

    def batch_handler(
        self,
        topic: str,
        *,
        max_size: int = 100,
        max_wait_ms: int = 1000,
        timeout_ms: int | None = None,
    ) -> Callable[[Callable], Callable]:
        """Decorator to register a batch handler for a topic.

        Batch handlers receive a list of messages instead of one at a time,
        enabling higher throughput for bulk processing workloads.

        Args:
            topic: The Kafka topic to handle.
            max_size: Maximum number of messages per batch (default 100).
            max_wait_ms: Maximum time to wait before dispatching a batch (default 1000).
            timeout_ms: Per-handler execution timeout in milliseconds.
                Overrides ``ConsumerConfig.handler_timeout_ms``.

        Returns:
            A decorator that registers the decorated callable as a batch handler.

        Example::

            @app.batch_handler(topic="my-topic", max_size=50, max_wait_ms=500)
            def handle_batch(messages: list[kafpy.KafkaMessage], ctx) -> kafpy.HandlerResult:
                for msg in messages:
                    process(msg)
                return kafpy.HandlerResult(action="ack")
        """

        def decorator(fn: Callable) -> Callable:
            # Detect if the batch handler is async
            if inspect.iscoroutinefunction(fn):
                mode = "batch_async"
            else:
                mode = "batch_sync"

            self.register_handler(
                topic, fn,
                routing=None,
                handler_mode=mode,
                batch_max_size=max_size,
                batch_max_wait_ms=max_wait_ms,
                timeout_ms=timeout_ms,
            )
            return fn

        return decorator

    def register_handler(
        self,
        topic: str,
        handler_fn: Callable,
        *,
        routing: object | None = None,
        handler_mode: str | None = None,
        batch_max_size: int | None = None,
        batch_max_wait_ms: int | None = None,
        timeout_ms: int | None = None,
    ) -> None:
        """Explicitly register a handler for a topic.

        Args:
            topic: The Kafka topic to handle.
            handler_fn: The callable to invoke for messages.
            routing: Optional routing configuration.
            handler_mode: Override auto-detected handler mode.
                One of "sync", "async", "batch_sync", "batch_async".
            batch_max_size: Max messages per batch (batch modes only).
            batch_max_wait_ms: Max wait time per batch in ms (batch modes only).
            timeout_ms: Per-handler execution timeout in milliseconds.

        Example::

            def handle(msg, ctx):
                return HandlerResult(action="ack")
            app.register_handler("my-topic", handle)
        """
        if not callable(handler_fn):
            raise ValueError(f"handler_fn must be callable, got {type(handler_fn).__name__}")

        # Detect handler type via callable inspection (D-02)
        if handler_mode is not None:
            handler_type = handler_mode
        elif inspect.iscoroutinefunction(handler_fn):
            handler_type = "async"
        elif inspect.isasyncgenfunction(handler_fn):
            handler_type = "batch_async"
        elif inspect.isgeneratorfunction(handler_fn):
            handler_type = "batch_sync"
        else:
            handler_type = "sync"

        # Wrap handler to convert dicts to proper types
        def wrapper(msg_dict: dict, ctx_dict: dict):
            msg = KafkaMessage.from_dict(msg_dict)
            ctx = HandlerContext(
                topic=str(ctx_dict["topic"]),
                partition=int(ctx_dict["partition"]),
                offset=int(ctx_dict["offset"]),
                timestamp=int(ctx_dict.get("timestamp", ctx_dict.get("timestamp_millis", 0))),
                headers=dict(ctx_dict["headers"]) if ctx_dict.get("headers") else {},
            )
            return handler_fn(msg, ctx)

        self._handlers[topic] = {
            "fn": handler_fn,
            "routing": routing,
            "type": handler_type,
            "batch_max_size": batch_max_size,
            "batch_max_wait_ms": batch_max_wait_ms,
            "timeout_ms": timeout_ms,
        }

        # Also register with the Rust consumer so it can dispatch messages
        self._consumer.add_handler(
            topic, wrapper,
            mode=handler_type if handler_type != "sync" else None,
            batch_max_size=batch_max_size,
            batch_max_wait_ms=batch_max_wait_ms,
            timeout_ms=timeout_ms,
        )
