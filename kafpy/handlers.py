"""Handler types and registration for KafPy."""

from __future__ import annotations

import inspect
from dataclasses import dataclass
from typing import Callable

from enum import Enum
from .config import FailureCategory, FailureReason
from .exceptions import HandlerError

__all__ = [
    "KafkaMessage",
    "HandlerContext",
    "HandlerResult",
    "HandlerAction",
    "FailureCategory",
    "FailureReason",
    "register_handler",
    "stream_handler",
]


@dataclass(frozen=True)
class HandlerContext:
    """Context for a handler invocation.

    Provides metadata about the Kafka message being handled.
    This is a stub — full implementation in Phase 36 or later.
    """

    topic: str
    partition: int
    offset: int
    timestamp: int
    headers: dict[str, str]


class HandlerAction(str, Enum):
    """Enum of possible actions a handler can return.

    The runtime interprets these to decide the next step for a message.

    Members:
        ACK: Message processed successfully — commit offset.
        NACK: Message failed, should be retried.
        DLQ: Message failed permanently, route to dead letter queue.
        RETRY: Message should be retried with backoff.
    """
    ACK = "ack"
    NACK = "nack"
    DLQ = "dlq"
    RETRY = "retry"


@dataclass(frozen=True)
class HandlerResult:
    """Result of a handler invocation.

    Indicates the action the runtime should take next.

    Example::

        return HandlerResult(action=HandlerAction.ACK)
        return HandlerResult(action="ack")  # string form also accepted
    """

    action: str | HandlerAction

    def __post_init__(self) -> None:
        if isinstance(self.action, HandlerAction):
            # Normalize to string value
            object.__setattr__(self, "action", self.action.value)
        elif self.action not in (e.value for e in HandlerAction):
            raise ValueError(
                f"Invalid action {self.action!r}. Must be one of: "
                f"{[e.value for e in HandlerAction]}"
            )


def register_handler(
    topic: str,
    handler: Callable,
    *,
    routing: object | None = None,
) -> None:
    """Register a handler for a topic.

    Args:
        topic: The Kafka topic to handle.
        handler: The callable to invoke for messages on this topic.
        routing: Optional routing configuration.

    Raises:
        ValueError: If the handler is not callable.
    """
    if not callable(handler):
        raise ValueError(f"handler must be callable, got {type(handler).__name__}")

    # Detect handler type via callable inspection (D-02)
    if inspect.iscoroutinefunction(handler):
        handler_type = "async"
    elif inspect.isasyncgenfunction(handler):
        handler_type = "batch_async"
    elif inspect.isgeneratorfunction(handler):
        handler_type = "batch_sync"
    else:
        handler_type = "sync"


def stream_handler(
    topic: str,
    *,
    name: str | None = None,
    retries: int | None = None,
    timeout: float | None = None,
    middleware: list | None = None,
):
    """Decorator for registering a persistent async iterable handler.

    Unlike @handler which processes single messages, @stream_handler
    creates a long-lived handler that continuously yields messages
    from Kafka until stopped.

    The decorated function must be an async generator::

        @stream_handler(topic="my-topic")
        async def handler(msg, ctx):
            async for msg in kafka_consumer:
                await process(msg)
                yield  # checkpoint for backpressure

    Args:
        topic: Kafka topic to subscribe to.
        name: Optional handler name for observability.
        retries: Number of retry attempts on failure.
        timeout: Handler execution timeout in seconds.
        middleware: List of middleware instances (Logging, Metrics).

    Returns:
        Decorated async generator function.

    Raises:
        TypeError: If the decorated function is not an async generator.
    """
    def decorator(func):
        # Detect async generator — async generators return False for iscoroutinefunction
        # but True for isasyncgenfunction
        if not inspect.isasyncgenfunction(func):
            raise TypeError("@stream_handler requires an async generator function")

        handler_name = name or getattr(func, "__name__", "stream_handler")

        # Register with streaming_async mode
        _register_handler(
            topic=topic,
            handler=func,
            mode="streaming_async",  # Per D-07: explicit streaming mode
            name=handler_name,
            retries=retries,
            timeout=timeout,
            middleware=middleware,
        )
        return func
    return decorator


def _register_handler(
    topic: str,
    handler: Callable,
    *,
    mode: str = "sync",
    name: str | None = None,
    retries: int | None = None,
    timeout: float | None = None,
    middleware: list | None = None,
) -> None:
    """Internal handler registration (stub for future Rust integration).

    Args:
        topic: The Kafka topic.
        handler: The callable to invoke.
        mode: Handler mode (sync, async, batch_async, batch_sync, streaming_async).
        name: Optional handler name.
        retries: Optional retry count.
        timeout: Optional timeout in seconds.
        middleware: Optional middleware list.
    """
    # Stub — actual registration via Rust PythonHandler
    pass


@dataclass(frozen=True)
class KafkaMessage:
    """Kafka message with typed field access.

    Attributes:
        topic: The Kafka topic this message was consumed from.
        partition: The partition number this message came from.
        offset: The message offset in the partition.
        key: The message key as bytes, or None if not set (D-08: silent None return).
        payload: The message payload as bytes, or None if not set.
        headers: List of (key, value) header tuples.
        timestamp_millis: Message timestamp in milliseconds since epoch, or None.
        _trace_context: Internal trace context dict (not for public use).

    D-08: Accessing .key when None returns None silently — no exception raised.
    D-09: Wrong-type access (e.g., calling get_key_as_string when key is bytes, or
          calling get_payload_as_string when payload is not valid UTF-8) raises HandlerError.
    """

    topic: str
    partition: int
    offset: int
    key: bytes | None
    payload: bytes | None
    headers: list[tuple[str, bytes | None]]
    timestamp_millis: int | None = None
    _trace_context: dict[str, str] | None = None

    def get_key_as_string(self) -> str | None:
        """Decode key as UTF-8 string.

        Returns:
            The decoded key as a string, or None if key is absent.

        Raises:
            HandlerError: If the key is not None and cannot be decoded as UTF-8.
        """
        if self.key is None:
            return None
        try:
            return self.key.decode("utf-8")
        except UnicodeDecodeError as e:
            raise HandlerError(
                message=f"key is not valid UTF-8: {e}",
                error_code=None,
                partition=self.partition,
                topic=self.topic,
            ) from e

    def get_payload_as_string(self) -> str | None:
        """Decode payload as UTF-8 string.

        Returns:
            The decoded payload as a string, or None if payload is absent.

        Raises:
            HandlerError: If the payload is not None and cannot be decoded as UTF-8.
        """
        if self.payload is None:
            return None
        try:
            return self.payload.decode("utf-8")
        except UnicodeDecodeError as e:
            raise HandlerError(
                message=f"payload is not valid UTF-8: {e}",
                error_code=None,
                partition=self.partition,
                topic=self.topic,
            ) from e

    @classmethod
    def from_dict(cls, data: dict) -> KafkaMessage:
        """Construct a KafkaMessage from a dictionary (e.g., from Rust PyDict).

        Args:
            data: Dictionary with keys: topic, partition, offset, key, payload,
                  headers, timestamp_millis, _trace_context.

        Returns:
            A new KafkaMessage instance.
        """
        return cls(
            topic=str(data["topic"]),
            partition=int(data["partition"]),
            offset=int(data["offset"]),
            key=data.get("key"),
            payload=data.get("payload"),
            headers=list(data.get("headers", [])),
            timestamp_millis=data.get("timestamp_millis"),
            _trace_context=data.get("_trace_context"),
        )
