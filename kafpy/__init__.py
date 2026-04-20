"""
KafPy: A Pythonic Kafka consumer framework built on Rust.

Public API entry point. All public types are re-exported here for convenience::

    import kafpy

    config = kafpy.ConsumerConfig(brokers="localhost:9092", group_id="my-group", topics=["my-topic"])
    app = kafpy.KafPy(config)
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
    Consumer,
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

# Re-export stubs for future phases (implemented in Phases 35-37)
try:
    from .handlers import (
        HandlerContext,
        HandlerResult,
    )
except ImportError:
    # Added in Phase 35 (Handler Registration & Runtime)
    HandlerContext = None
    HandlerResult = None

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

try:
    from .consumer import KafPy
except ImportError:
    # Added in Phase 34 (Configuration Model) — main runtime wrapper
    KafPy = None

__all__ = [
    # Rust extension types (available now)
    "ConsumerConfig",  # from config.py (Python wrapper)
    "ProducerConfig",
    "KafkaMessage",
    "Consumer",
    "Producer",
    # Configuration types — Phase 34
    "RoutingConfig",
    "RetryConfig",
    "BatchConfig",
    "ConcurrencyConfig",
    # Handler types — Phase 35
    "HandlerContext",
    "HandlerResult",
    # Exception types — Phase 36
    "KafPyError",
    "ConsumerError",
    "HandlerError",
    "ConfigurationError",
    # Main runtime — Phase 34
    "KafPy",
]
