from dataclasses import dataclass
from typing import Optional

# ─── Consumer Configuration ────────────────────────────────────────────────────

@dataclass
class ConsumerConfig:
    brokers: str
    group_id: str
    topics: list[str]
    auto_offset_reset: str
    enable_auto_commit: bool
    session_timeout_ms: int
    heartbeat_interval_ms: int
    max_poll_interval_ms: int
    security_protocol: str | None
    sasl_mechanism: str | None
    sasl_username: str | None
    sasl_password: str | None
    fetch_min_bytes: int
    max_partition_fetch_bytes: int
    partition_assignment_strategy: str
    retry_backoff_ms: int
    message_batch_size: int
    default_retry_policy: PyRetryPolicy | None
    dlq_topic_prefix: str | None
    drain_timeout_secs: int | None
    num_workers: int | None
    enable_auto_offset_store: bool | None
    observability_config: PyObservabilityConfig | None
    handler_timeout_ms: int | None

    def __init__(
        self,
        brokers: str,
        group_id: str,
        topics: list[str],
        auto_offset_reset: str = "earliest",
        enable_auto_commit: bool = False,
        session_timeout_ms: int = 30000,
        heartbeat_interval_ms: int = 3000,
        max_poll_interval_ms: int = 300000,
        security_protocol: str | None = None,
        sasl_mechanism: str | None = None,
        sasl_username: str | None = None,
        sasl_password: str | None = None,
        fetch_min_bytes: int = 1,
        max_partition_fetch_bytes: int = 1048576,
        partition_assignment_strategy: str = "roundrobin",
        retry_backoff_ms: int = 100,
        message_batch_size: int = 100,
        default_retry_policy: PyRetryPolicy | None = None,
        dlq_topic_prefix: str | None = None,
        drain_timeout_secs: int | None = None,
        num_workers: int | None = None,
        enable_auto_offset_store: bool | None = None,
        observability_config: PyObservabilityConfig | None = None,
        handler_timeout_ms: int | None = None,
    ): ...

    @staticmethod
    def from_env() -> "ConsumerConfig": ...


# ─── Producer Configuration ────────────────────────────────────────────────────

@dataclass
class ProducerConfig:
    brokers: str
    message_timeout_ms: int
    queue_buffering_max_messages: int
    queue_buffering_max_kbytes: int
    batch_num_messages: int
    compression_type: str
    linger_ms: int
    request_timeout_ms: int
    retry_backoff_ms: int
    retries: int
    max_in_flight: int
    enable_idempotence: bool
    acks: str
    security_protocol: str | None
    sasl_mechanism: str | None
    sasl_username: str | None
    sasl_password: str | None

    def __init__(
        self,
        brokers: str,
        message_timeout_ms: int = 30000,
        queue_buffering_max_messages: int = 100000,
        queue_buffering_max_kbytes: int = 1048576,
        batch_num_messages: int = 10000,
        compression_type: str = "snappy",
        linger_ms: int = 5,
        request_timeout_ms: int = 30000,
        retry_backoff_ms: int = 100,
        retries: int = 2147483647,
        max_in_flight: int = 5,
        enable_idempotence: bool = True,
        acks: str = "all",
        security_protocol: str | None = None,
        sasl_mechanism: str | None = None,
        sasl_username: str | None = None,
        sasl_password: str | None = None,
    ): ...

    @staticmethod
    def from_env() -> "ProducerConfig": ...


# ─── Kafka Message ────────────────────────────────────────────────────────────

@dataclass
class KafkaMessage:
    topic: str
    partition: int
    offset: int
    key: None | bytes
    payload: None | bytes
    timestamp_millis: None | int
    headers: list[tuple[str, bytes | None]]

    def get_key(self) -> None | bytes: ...
    def get_headers(self) -> list[tuple[str, bytes | None]]: ...
    def get_timestamp_millis(self) -> None | int: ...


# ─── Consumer ──────────────────────────────────────────────────────────────────

@dataclass
class Consumer:
    config: ConsumerConfig

    def __init__(self, config: ConsumerConfig) -> None: ...
    def add_handler(self, topic: str, handler: callable[[KafkaMessage], None], *, mode: str | None = None, batch_max_size: int | None = None, batch_max_wait_ms: int | None = None, timeout_ms: int | None = None, concurrency: int | None = None) -> None: ...
    async def start(self) -> None: ...
    def stop(self) -> None: ...
    def status(self) -> dict: ...
    def __enter__(self) -> Consumer: ...
    def __exit__(self, exc_type: type[BaseException] | None, exc_val: BaseException | None, traceback: Any) -> bool: ...


# ─── Producer ──────────────────────────────────────────────────────────────────

@dataclass
class Producer:
    config: ProducerConfig

    def __init__(self, config: ProducerConfig) -> None: ...
    async def init(self) -> None: ...
    async def send(
        self,
        topic: str,
        key: bytes | None = None,
        payload: bytes | None = None,
        partition: int | None = None,
        headers: list[tuple[str, bytes]] | None = None,
    ) -> tuple[int, int]: ...
    def send_sync(
        self,
        topic: str,
        key: bytes | None = None,
        payload: bytes | None = None,
        partition: int | None = None,
        headers: list[tuple[str, bytes]] | None = None,
    ) -> tuple[int, int]: ...
    async def flush(self, timeout_ms: int | None = None) -> None: ...
    def in_flight_count(self) -> int: ...


# ─── Retry Policy ──────────────────────────────────────────────────────────────

class PyRetryPolicy:
    """Retry policy configuration for message processing.

    Uses milliseconds for delay values (Python-friendly) and converts
    to Duration internally when passing to Rust RetryPolicy.

    Defaults match the Rust RetryPolicy::default():
    - max_attempts: 3
    - base_delay_ms: 100
    - max_delay_ms: 30000
    - jitter_factor: 0.1
    """

    max_attempts: int
    base_delay_ms: int
    max_delay_ms: int
    jitter_factor: float

    def __init__(
        self,
        max_attempts: int = 3,
        base_delay_ms: int = 100,
        max_delay_ms: int = 30000,
        jitter_factor: float = 0.1,
    ): ...
    def __repr__(self) -> str: ...


# ─── Observability Configuration ───────────────────────────────────────────────

class PyObservabilityConfig:
    """Observability configuration for metrics and tracing.

    When otlp_endpoint is None, tracing is disabled (zero-cost).

    Defaults:
    - otlp_endpoint: None
    - service_name: "kafpy"
    - sampling_ratio: 1.0
    - log_format: "pretty"
    """

    otlp_endpoint: str | None
    service_name: str
    sampling_ratio: float
    log_format: str

    def __init__(
        self,
        otlp_endpoint: str | None = None,
        service_name: str = "kafpy",
        sampling_ratio: float = 1.0,
        log_format: str = "pretty",
    ): ...
    def __repr__(self) -> str: ...


# ─── Failure Classification ─────────────────────────────────────────────────────

class PyFailureCategory:
    """High-level failure category for classifying message processing errors.

    Members:
        Retryable: Transient failures that may succeed on retry.
        Terminal: Permanent failures indicating a bad message.
        NonRetryable: Failures that should not be retried.
    """
    Retryable: int  # enum value
    Terminal: int  # enum value
    NonRetryable: int  # enum value

    def __repr__(self) -> str: ...


class PyFailureReason:
    """A specific failure reason with its category and description.

    Represents the classification of why a message failed processing,
    used for retry and DLQ routing decisions.
    """

    category: PyFailureCategory
    description: str

    def __init__(self, category: PyFailureCategory, description: str) -> None: ...
    def __repr__(self) -> str: ...


# ─── Benchmark Functions ───────────────────────────────────────────────────────

def run_scenario_py(scenario_name: str, config_json: str) -> str: ...
def run_hardening_checks_py(result_json: str) -> str: ...