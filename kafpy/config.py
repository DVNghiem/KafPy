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
    "ObservabilityConfig",
    "FailureCategory",
    "FailureReason",
]

# Module-level constants for validation
ROUTING_MODES: frozenset[str] = frozenset({"pattern", "header", "key", "python", "default"})
AUTO_OFFSET_RESET_VALUES: frozenset[str] = frozenset({"earliest", "latest"})


# ─── Failure Classification ────────────────────────────────────────────────────

class FailureCategory:
    """High-level failure category for classifying message processing errors.

    Mirrors the Rust ``PyFailureCategory`` enum exposed via ``_kafpy``.
    Use these when inspecting failure reasons for retry/DLQ decisions.

    Attributes:
        Retryable: Transient failures that may succeed on retry (e.g., network timeout).
        Terminal: Permanent failures indicating a bad message (e.g., poison message).
        NonRetryable: Failures that should not be retried (e.g., validation error).
    """

    Retryable = "Retryable"
    Terminal = "Terminal"
    NonRetryable = "NonRetryable"


@dataclass(frozen=True)
class FailureReason:
    """A specific failure reason with its category and human-readable description.

    Represents the classification of why a message failed processing,
    used for retry and DLQ routing decisions.

    Attributes:
        category: The failure category (Retryable, Terminal, or NonRetryable).
        description: Human-readable description of the failure.
    """

    category: str
    description: str


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
        retry_policy: Retry policy for handler failures. None uses Rust defaults (3 attempts, 100ms base).
        dlq_topic_prefix: Prefix for DLQ topic names. None defaults to "dlq.".
        drain_timeout_secs: Seconds to wait for graceful shutdown drain. None defaults to 30.
        num_workers: Number of concurrent worker threads. None defaults to 4.
        enable_auto_offset_store: Whether to enable auto offset store. None defaults to False.
        observability_config: Observability configuration (OTLP, tracing, metrics). None uses defaults.
        handler_timeout_ms: Handler execution timeout in milliseconds. If a handler takes longer,
            it is cancelled and treated as a failure (routed to DLQ or retry). None disables timeout.
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
    # ── New fields: retry, DLQ, drain, workers, observability ──
    retry_policy: RetryConfig | None = None
    dlq_topic_prefix: str | None = None
    drain_timeout_secs: int | None = None
    num_workers: int | None = None
    enable_auto_offset_store: bool | None = None
    observability_config: ObservabilityConfig | None = None
    # ── Handler timeout: cancel handlers that exceed this duration (ms) ──
    handler_timeout_ms: int | None = None

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
        if self.num_workers is not None and self.num_workers <= 0:
            raise ValueError("num_workers must be positive")
        if self.drain_timeout_secs is not None and self.drain_timeout_secs <= 0:
            raise ValueError("drain_timeout_secs must be positive")
        if self.handler_timeout_ms is not None and self.handler_timeout_ms <= 0:
            raise ValueError("handler_timeout_ms must be positive")

    def to_rust(self) -> object:
        """Convert to Rust ConsumerConfig for use with the KafPy runtime."""
        import kafpy._kafpy as _kafpy

        # Build optional PyRetryPolicy from RetryConfig if provided
        py_retry_policy = None
        if self.retry_policy is not None:
            py_retry_policy = _kafpy.PyRetryPolicy(
                max_attempts=self.retry_policy.max_attempts,
                base_delay_ms=int(self.retry_policy.base_delay * 1000),
                max_delay_ms=int(self.retry_policy.max_delay * 1000) if self.retry_policy.max_delay is not None else 30000,
                jitter_factor=self.retry_policy.jitter_factor if self.retry_policy.jitter_factor is not None else 0.1,
            )

        # Build optional PyObservabilityConfig from ObservabilityConfig if provided
        py_obs_config = None
        if self.observability_config is not None:
            py_obs_config = _kafpy.PyObservabilityConfig(
                otlp_endpoint=self.observability_config.otlp_endpoint,
                service_name=self.observability_config.service_name,
                sampling_ratio=self.observability_config.sampling_ratio,
                log_format=self.observability_config.log_format,
            )

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
            default_retry_policy=py_retry_policy,
            dlq_topic_prefix=self.dlq_topic_prefix,
            drain_timeout_secs=self.drain_timeout_secs,
            num_workers=self.num_workers,
            enable_auto_offset_store=self.enable_auto_offset_store,
            observability_config=py_obs_config,
            handler_timeout_ms=self.handler_timeout_ms,
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
            Note: converted to milliseconds for the Rust layer.
        max_delay: Maximum delay in seconds between retries. None defaults to 30s.
        jitter_factor: Random jitter factor between 0.0 and 1.0 added to delay. None defaults to 0.1.
    """

    max_attempts: int = 3
    base_delay: float = 0.1
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
class ObservabilityConfig:
    """Observability configuration for metrics and tracing.

    When ``otlp_endpoint`` is None, tracing is disabled (zero-cost).

    Attributes:
        otlp_endpoint: OTLP exporter endpoint (e.g., "http://localhost:4317").
            None means tracing is disabled.
        service_name: Service name for OTLP resource.
        sampling_ratio: Sampling ratio (0.0–1.0). 1.0 = sample everything.
        log_format: Log format — one of "json", "pretty", or "simple".
    """

    otlp_endpoint: str | None = None
    service_name: str = "kafpy"
    sampling_ratio: float = 1.0
    log_format: str = "pretty"

    def __post_init__(self) -> None:
        valid_formats = {"json", "pretty", "simple"}
        if self.log_format not in valid_formats:
            raise ValueError(
                f"log_format must be one of {sorted(valid_formats)}, "
                f"got {self.log_format!r}"
            )
        if not 0.0 <= self.sampling_ratio <= 1.0:
            raise ValueError(
                f"sampling_ratio must be between 0.0 and 1.0, got {self.sampling_ratio}"
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
