"""Handler types and registration for KafPy."""

from __future__ import annotations

import inspect
from dataclasses import dataclass
from typing import Callable

from .exceptions import HandlerError

__all__ = [
    "KafkaMessage",
    "HandlerContext",
    "HandlerResult",
    "register_handler",
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


@dataclass(frozen=True)
class HandlerResult:
    """Result of a handler invocation.

    Indicates the action the runtime should take next.
    This is a stub — full implementation in Phase 36 or later.
    """

    action: str


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
        timestamp: Message timestamp in milliseconds since epoch.
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
    timestamp: int
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
                  headers, timestamp, _trace_context.

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
            timestamp=int(data.get("timestamp", 0)),
            _trace_context=data.get("_trace_context"),
        )
