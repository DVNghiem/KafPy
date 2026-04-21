"""KafPy runtime with handler lifecycle."""

from __future__ import annotations

import inspect
import threading
from typing import Callable

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
        self._handlers: dict[str, dict[str, object]] = {}
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
    ) -> Callable[[Callable], Callable]:
        """Decorator to register a handler for a topic.

        Args:
            topic: The Kafka topic to handle.
            routing: Optional routing configuration.

        Returns:
            A decorator that registers the decorated callable as a handler.

        Example::

            @app.handler(topic="my-topic")
            def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
                return kafpy.HandlerResult(action="ack")
        """

        def decorator(fn: Callable) -> Callable:
            self.register_handler(topic, fn, routing=routing)
            return fn

        return decorator

    def register_handler(
        self,
        topic: str,
        handler_fn: Callable,
        *,
        routing: object | None = None,
    ) -> None:
        """Explicitly register a handler for a topic.

        Args:
            topic: The Kafka topic to handle.
            handler_fn: The callable to invoke for messages.
            routing: Optional routing configuration.

        Example::

            def handle(msg, ctx):
                return HandlerResult(action="ack")
            app.register_handler("my-topic", handle)
        """
        if not callable(handler_fn):
            raise ValueError(f"handler_fn must be callable, got {type(handler_fn).__name__}")

        # Detect handler type via callable inspection (D-02)
        if inspect.iscoroutinefunction(handler_fn):
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
                timestamp=int(ctx_dict["timestamp"]),
                headers=dict(ctx_dict["headers"]) if ctx_dict.get("headers") else {},
            )
            return handler_fn(msg, ctx)

        self._handlers[topic] = {
            "fn": handler_fn,
            "routing": routing,
            "type": handler_type,
        }

        # Also register with the Rust consumer so it can dispatch messages
        self._consumer.add_handler(topic, wrapper)
