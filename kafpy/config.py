"""Configuration classes for KafPy consumer framework."""

from __future__ import annotations

from dataclasses import dataclass
from typing import TYPE_CHECKING

__all__ = [
    "ConsumerConfig",
    "RoutingConfig",
    "RetryConfig",
    "BatchConfig",
    "ConcurrencyConfig",
]

# Module-level constants for validation
ROUTING_MODES: frozenset[str] = frozenset({"pattern", "header", "key", "python", "default"})
AUTO_OFFSET_RESET_VALUES: frozenset[str] = frozenset({"earliest", "latest"})


@dataclass(frozen=True)
class ConsumerConfig:
    """Consumer configuration for KafPy.

    Wraps the Rust ConsumerConfig for Python-side validation and type-safety.
    Use ``to_rust()`` to convert to the Rust ConsumerConfig for use with the runtime.

    Attributes:
        bootstrap_servers: Comma-separated list of Kafka broker addresses.
        group_id: Consumer group identifier.
        topics: List of topics to subscribe to.
        auto_offset_reset: Where to start if no offset exists ("earliest" or "latest").
        enable_auto_commit: Whether to auto-commit offsets.
        session_timeout_ms: Session timeout in milliseconds.
        heartbeat_interval_ms: Heartbeat interval in milliseconds.
        max_poll_interval_ms: Maximum poll interval in milliseconds.
        security_protocol: Security protocol (None, "PLAINTEXT", "SSL", "SASL_PLAINTEXT", "SASL_SSL").
        sasl_mechanism: SASL mechanism (None, "PLAIN", "SCRAM-SHA-256", "SCRAM-SHA-512").
        sasl_username: SASL username.
        sasl_password: SASL password.
        fetch_min_bytes: Minimum bytes to fetch per request.
        max_partition_fetch_bytes: Maximum bytes per partition to fetch.
        partition_assignment_strategy: Partition assignment strategy ("roundrobin", "range", "cooperative-sticky").
        retry_backoff_ms: Retry backoff interval in milliseconds.
        message_batch_size: Number of messages per batch.
    """

    bootstrap_servers: str
    group_id: str
    topics: list[str]
    auto_offset_reset: str = "earliest"
    enable_auto_commit: bool = False
    session_timeout_ms: int = 30000
    heartbeat_interval_ms: int = 3000
    max_poll_interval_ms: int = 300000
    security_protocol: str | None = None
    sasl_mechanism: str | None = None
    sasl_username: str | None = None
    sasl_password: str | None = None
    fetch_min_bytes: int = 1
    max_partition_fetch_bytes: int = 1048576
    partition_assignment_strategy: str = "roundrobin"
    retry_backoff_ms: int = 100
    message_batch_size: int = 100

    def __post_init__(self) -> None:
        if self.auto_offset_reset not in AUTO_OFFSET_RESET_VALUES:
            raise ValueError(
                f"auto_offset_reset must be one of {sorted(AUTO_OFFSET_RESET_VALUES)}, "
                f"got {self.auto_offset_reset!r}"
            )
        if self.session_timeout_ms <= 0:
            raise ValueError("session_timeout_ms must be positive")
        if self.heartbeat_interval_ms <= 0:
            raise ValueError("heartbeat_interval_ms must be positive")
        if self.max_poll_interval_ms <= 0:
            raise ValueError("max_poll_interval_ms must be positive")
        if not self.bootstrap_servers:
            raise ValueError("bootstrap_servers must be non-empty")

    def to_rust(self) -> object:
        """Convert to Rust ConsumerConfig for use with the KafPy runtime."""
        import _kafpy

        return _kafpy.ConsumerConfig(
            brokers=self.bootstrap_servers,
            group_id=self.group_id,
            topics=self.topics,
            auto_offset_reset=self.auto_offset_reset,
            enable_auto_commit=self.enable_auto_commit,
            session_timeout_ms=self.session_timeout_ms,
            heartbeat_interval_ms=self.heartbeat_interval_ms,
            max_poll_interval_ms=self.max_poll_interval_ms,
            security_protocol=self.security_protocol,
            sasl_mechanism=self.sasl_mechanism,
            sasl_username=self.sasl_username,
            sasl_password=self.sasl_password,
            fetch_min_bytes=self.fetch_min_bytes,
            max_partition_fetch_bytes=self.max_partition_fetch_bytes,
            partition_assignment_strategy=self.partition_assignment_strategy,
            retry_backoff_ms=self.retry_backoff_ms,
            message_batch_size=self.message_batch_size,
        )


@dataclass(frozen=True)
class RoutingConfig:
    """Routing configuration for handler selection.

    Attributes:
        routing_mode: How to route messages to handlers ("default", "pattern", "header", "key", "python").
        fallback_handler: Name of fallback handler when no match is found.
    """

    routing_mode: str = "default"
    fallback_handler: str | None = None

    def __post_init__(self) -> None:
        if self.routing_mode not in ROUTING_MODES:
            raise ValueError(
                f"routing_mode must be one of {sorted(ROUTING_MODES)}, "
                f"got {self.routing_mode!r}"
            )


@dataclass(frozen=True)
class RetryConfig:
    """Retry configuration for handler failures.

    Attributes:
        max_attempts: Maximum number of retry attempts (0 means no retry).
        base_delay: Base delay in seconds between retries (exponential backoff).
        max_delay: Maximum delay in seconds between retries.
        jitter_factor: Random jitter factor between 0.0 and 1.0 added to delay.
    """

    max_attempts: int = 3
    base_delay: float = 1.0
    max_delay: float | None = None
    jitter_factor: float | None = None

    def __post_init__(self) -> None:
        if self.max_attempts < 0:
            raise ValueError("max_attempts must be non-negative")
        if self.base_delay < 0:
            raise ValueError("base_delay must be non-negative")
        if self.max_delay is not None and self.max_delay < 0:
            raise ValueError("max_delay must be non-negative")
        if self.jitter_factor is not None and not (0.0 <= self.jitter_factor <= 1.0):
            raise ValueError(
                f"jitter_factor must be between 0.0 and 1.0, got {self.jitter_factor}"
            )


@dataclass(frozen=True)
class BatchConfig:
    """Batch processing configuration.

    Attributes:
        max_batch_size: Maximum number of messages per batch.
        max_batch_timeout_ms: Maximum time to wait for a batch in milliseconds.
    """

    max_batch_size: int = 100
    max_batch_timeout_ms: int = 1000

    def __post_init__(self) -> None:
        if self.max_batch_size <= 0:
            raise ValueError("max_batch_size must be positive")
        if self.max_batch_timeout_ms <= 0:
            raise ValueError("max_batch_timeout_ms must be positive")


@dataclass(frozen=True)
class ConcurrencyConfig:
    """Concurrency configuration for handler workers.

    Attributes:
        num_workers: Number of concurrent worker threads or tasks.
    """

    num_workers: int = 1

    def __post_init__(self) -> None:
        if self.num_workers <= 0:
            raise ValueError("num_workers must be positive")
