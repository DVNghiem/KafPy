"""KafPy runtime with handler lifecycle."""

from __future__ import annotations

import inspect
from typing import Any, Callable

from ._kafpy import Consumer
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

    def __init__(self, consumer: Consumer) -> None:
        """Initialize KafPy with a Consumer.

        Args:
            consumer: A kafpy.Consumer instance wrapping the Rust consumer.
        """
        self._consumer = consumer
        self._handlers: dict[str, dict[str, Any]] = {}
        self._stopping = False

    def start(self):
        """Start consuming messages.

        Begins the consumer and dispatches messages to registered handlers.
        """
        return self._consumer.start()

    def stop(self) -> None:
        """Stop the consumer gracefully.

        Initiates drain: waits for in-flight messages to complete,
        then shuts down the consumer.
        """
        self._stopping = True
        self._consumer.stop()

    def handler(
        self,
        topic: str,
        *,
        routing: object | None = None,
        timeout_ms: int | None = None,
        concurrency: int | None = None,
        middleware: list | None = None,
    ) -> Callable[[Callable], Callable]:
        """Decorator to register a single-message handler for a topic.

        For batch processing, use :meth:`batch_handler` instead.

        Args:
            topic: The Kafka topic to handle.
            routing: Optional routing configuration.
            timeout_ms: Per-handler execution timeout in milliseconds.
                Overrides ``ConsumerConfig.handler_timeout_ms``.
            concurrency: Maximum concurrent executions of this handler.
                None means no limit (default).
            middleware: List of middleware instances (e.g., [Logging(), Metrics()]).
                Each must implement before(), after(), on_error().
                Built-in middleware: Logging(), Metrics().
                Custom middleware: subclass kafpy.BaseMiddleware.

        Returns:
            A decorator that registers the decorated callable as a handler.

        Example::

            @app.handler(topic="my-topic")
            def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
                return kafpy.HandlerResult(action="ack")

        Example (with middleware)::

            @app.handler(topic="my-topic", middleware=[kafpy.Logging(), kafpy.Metrics()])
            def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
                return kafpy.HandlerResult(action="ack")
        """

        def decorator(fn: Callable) -> Callable:
            self.register_handler(
                topic, fn,
                routing=routing,
                timeout_ms=timeout_ms,
                concurrency=concurrency,
                middleware=middleware,
            )
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
            if inspect.iscoroutinefunction(fn):
                raise TypeError("async batch handlers are not supported")

            def wrapper(msg_dict: dict, ctx_dict: dict):
                msg = KafkaMessage.from_dict(msg_dict)
                ctx = HandlerContext(
                    topic=str(ctx_dict["topic"]),
                    partition=int(ctx_dict["partition"]),
                    offset=int(ctx_dict["offset"]),
                    timestamp=int(ctx_dict.get("timestamp", ctx_dict.get("timestamp_millis", 0))),
                    headers=dict(ctx_dict["headers"]) if ctx_dict.get("headers") else {},
                )
                return fn(msg, ctx)

            self._handlers[topic] = {
                "fn": fn,
                "type": "batch_sync",
                "batch_max_size": max_size,
                "batch_max_wait_ms": max_wait_ms,
                "timeout_ms": timeout_ms,
            }
            self._consumer.add_handler(
                topic, wrapper,
                mode="batch_sync",
                batch_max_size=max_size,
                batch_max_wait_ms=max_wait_ms,
                timeout_ms=timeout_ms,
                concurrency=None,
                middleware=None,
            )
            return fn

        return decorator

    def register_handler(
        self,
        topic: str,
        handler_fn: Callable,
        *,
        routing: object | None = None,
        timeout_ms: int | None = None,
        concurrency: int | None = None,
        middleware: list | None = None,
    ) -> None:
        """Explicitly register a single-message handler for a topic.

        For batch handlers, use :meth:`batch_handler` instead.

        Args:
            topic: The Kafka topic to handle.
            handler_fn: The callable to invoke for each message.
                Must be a regular (non-async) function.
            routing: Optional routing configuration.
            timeout_ms: Per-handler execution timeout in milliseconds.
            concurrency: Maximum concurrent executions for this handler. None = no limit.
            middleware: List of middleware instances (e.g., [Logging(), Metrics()]).

        Raises:
            ValueError: If handler_fn is not callable.
            TypeError: If handler_fn is an async function or async generator.

        Example::

            def handle(msg, ctx):
                return HandlerResult(action="ack")
            app.register_handler("my-topic", handle)
        """
        if not callable(handler_fn):
            raise ValueError(f"handler_fn must be callable, got {type(handler_fn).__name__}")
        if inspect.iscoroutinefunction(handler_fn):
            raise TypeError("async handlers are not supported")
        if inspect.isasyncgenfunction(handler_fn):
            raise TypeError("async handlers are not supported")

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
            "type": "sync",
            "timeout_ms": timeout_ms,
            "concurrency": concurrency,
            "middleware": middleware,
        }

        self._consumer.add_handler(
            topic, wrapper,
            mode=None,
            batch_max_size=None,
            batch_max_wait_ms=None,
            timeout_ms=timeout_ms,
            concurrency=concurrency,
            middleware=middleware,
        )
