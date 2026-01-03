from dataclasses import dataclass

@dataclass
class KafkaConfig:
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

    def __init__(
        self,
        brokers: str,
        group_id: str,
        topics: list[str],
        auto_offset_reset: str,
        enable_auto_commit: bool,
        session_timeout_ms: int,
        heartbeat_interval_ms: int,
        max_poll_interval_ms: int,
        security_protocol: str | None,
        sasl_mechanism: str | None,
        sasl_username: str | None,
        sasl_password: str | None,
        fetch_min_bytes: int,
        max_partition_fetch_bytes: int,
        partition_assignment_strategy: str,
        retry_backoff_ms: int,
        message_batch_size: int,
    ): ...


@dataclass
class AppConfig:
    kafka: KafkaConfig
    processing_timeout_ms: int
    graceful_shutdown_timeout_ms: int

    def __init__(
        self,
        kafka: KafkaConfig,
        processing_timeout_ms: int,
        graceful_shutdown_timeout_ms: int,
    ): ...
    
    @staticmethod
    def from_env() -> "AppConfig": ...
    def processing_timeout(self) -> int: ...
    def graceful_shutdown_timeout(self) -> int: ...
    def get_kafka_properties(self) -> dict[str, str]: ...

@dataclass
class KafkaMessage:
    topic: str
    partition: int
    offset: int
    key: None | bytes
    payload: None | bytes
    headers: list[tuple[str, bytes | None]]

    def get_key(self) -> None | bytes: ...
    def get_headers(self) -> list[tuple[str, bytes | None]]: ...


@dataclass
class Consumer:
    config: AppConfig

    def __init__(self, config: AppConfig) -> None: ...
    def add_handler(self, topic: str, handler: callable[[KafkaMessage], None]) -> None: ...
    async def start(self) -> None: ...
    def stop(self) -> None: ...

