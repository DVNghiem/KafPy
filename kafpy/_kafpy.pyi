from dataclasses import dataclass

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
    ): ...

    @staticmethod
    def from_env() -> "ConsumerConfig": ...


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
    config: ConsumerConfig

    def __init__(self, config: ConsumerConfig) -> None: ...
    def add_handler(self, topic: str, handler: callable[[KafkaMessage], None]) -> None: ...
    async def start(self) -> None: ...
    def stop(self) -> None: ...


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

