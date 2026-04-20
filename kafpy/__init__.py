"""
KafPy: A Pythonic Kafka consumer framework built on Rust.

Public API entry point. All public types are re-exported here for convenience::

    import kafpy

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

Exception types are available via ``kafpy.exceptions``:
    from kafpy.exceptions import KafPyError, ConsumerError, HandlerError, ConfigurationError

Configuration types are available via ``kafpy.config``:
    from kafpy.config import ConsumerConfig, ProducerConfig

For handler and message types, use ``kafpy.handlers``:
    from kafpy.handlers import KafkaMessage, HandlerContext, HandlerResult
"""

__version__ = "0.1.0"

# Re-export from Rust extension (_kafpy)
from ._kafpy import (
    ProducerConfig,
    KafkaMessage,
    Producer,
)

# Configuration classes (Python wrapper, Phase 34)
from .config import (
    ConsumerConfig,
    RoutingConfig,
    RetryConfig,
    BatchConfig,
    ConcurrencyConfig,
)

# Handler types and registration (Phase 35)
from .handlers import (
    HandlerContext,
    HandlerResult,
    register_handler,
)

# Consumer wrapper (Phase 35)
from .consumer import Consumer

# Runtime with KafPy class (Phase 35)
from .runtime import KafPy

# Exception types (Phase 36 — stub)
try:
    from .exceptions import (
        KafPyError,
        ConsumerError,
        HandlerError,
        ConfigurationError,
    )
except ImportError:
    # Added in Phase 36 (Error Handling)
    KafPyError = None
    ConsumerError = None
    HandlerError = None
    ConfigurationError = None

__all__ = [
    # Rust extension types (available now)
    "ProducerConfig",
    "KafkaMessage",
    "Producer",
    # Configuration types — Phase 34
    "ConsumerConfig",
    "RoutingConfig",
    "RetryConfig",
    "BatchConfig",
    "ConcurrencyConfig",
    # Consumer wrapper and runtime — Phase 35
    "Consumer",
    "KafPy",
    # Handler types and registration — Phase 35
    "HandlerContext",
    "HandlerResult",
    "register_handler",
    # Exception types — Phase 36
    "KafPyError",
    "ConsumerError",
    "HandlerError",
    "ConfigurationError",
]
