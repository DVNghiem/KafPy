"""Exception classes for KafPy."""

from __future__ import annotations

from dataclasses import dataclass
import re

__all__ = [
    "KafPyError",
    "ConsumerError",
    "HandlerError",
    "ConfigurationError",
]


@dataclass(frozen=True)
class KafPyError(Exception):
    """Base exception for all KafPy errors.

    All framework exceptions inherit from this, not from Exception directly.
    Carries structured context: error_code, partition, topic where applicable.

    Attributes:
        message: Human-readable error description.
        error_code: Kafka error code (int) when applicable, None otherwise.
        partition: Kafka partition number when applicable, None otherwise.
        topic: Kafka topic name when applicable, None otherwise.
    """

    message: str
    error_code: int | None = None
    partition: int | None = None
    topic: str | None = None

    def __str__(self) -> str:
        return self.message

    def __repr__(self) -> str:
        parts = [f"message={self.message!r}"]
        if self.error_code is not None:
            parts.append(f"error_code={self.error_code}")
        if self.partition is not None:
            parts.append(f"partition={self.partition}")
        if self.topic is not None:
            parts.append(f"topic={self.topic!r}")
        return f"{type(self).__name__}({', '.join(parts)})"


@dataclass(frozen=True)
class ConsumerError(KafPyError):
    """Raised for consumer-level errors.

    Covers Kafka errors, subscription issues, message receive failures,
    and serialization errors.

    D-10 format: "Consumer error: NOT_LEADER (error 6) on topic-0@partition 3"
    """

    def __str__(self) -> str:
        base = self.message
        if self.topic is not None and self.partition is not None:
            return f"{base} on {self.topic}@{self.partition}"
        return base


@dataclass(frozen=True)
class HandlerError(KafPyError):
    """Raised for handler processing errors.

    Covers Python handler exceptions, wrong-type message field access,
    and handler panics caught at the PyO3 boundary.
    """

    pass


@dataclass(frozen=True)
class ConfigurationError(KafPyError):
    """Raised for configuration errors.

    Covers invalid config values, missing required fields,
    and Rust config build failures.
    """

    pass


# Rust-to-Python error translation helpers
# D-07: Each Rust error type maps to specific Python exception

RUST_TO_PYTHON: dict[str, type[KafPyError]] = {
    "Consumer": ConsumerError,
    "Producer": ConsumerError,
    "Config": ConfigurationError,
}


def translate_rust_error(module: str, error_string: str) -> KafPyError:
    """Translate a Rust error string to the appropriate Python exception.

    Rust errors are logged with context-preserving messages (D-06/D-10).
    Parses the error string to extract error_code and topic/partition info.

    Args:
        module: The Rust module name that generated the error (e.g., "Consumer").
        error_string: The formatted error string from Rust.

    Returns:
        The appropriate KafPyError subclass instance with structured fields.

    D-10 format example: "Consumer error: NOT_LEADER (error 6) on topic-0@partition 3"
    """
    # Parse error code: e.g., "NOT_LEADER (error 6)"
    code_match = re.search(r'\(error (\d+)\)', error_string)
    error_code: int | None = int(code_match.group(1)) if code_match else None

    # Parse topic@partition: e.g., "on topic-0@partition 3"
    topic_match = re.search(r'on ([^@]+)@partition (\d+)', error_string)
    topic: str | None = topic_match.group(1) if topic_match else None
    partition: int | None = int(topic_match.group(2)) if topic_match else None

    exc_type = RUST_TO_PYTHON.get(module, KafPyError)
    return exc_type(
        message=error_string,
        error_code=error_code,
        partition=partition,
        topic=topic,
    )
