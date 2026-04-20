"""Handler types and registration for KafPy."""

from __future__ import annotations

import inspect
from dataclasses import dataclass
from typing import Callable

__all__ = [
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
