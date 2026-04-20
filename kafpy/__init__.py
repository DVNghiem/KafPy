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

# Re-export from Rust extension (_kafpy) — conditional until extension is built
try:
    from ._kafpy import (
        ProducerConfig,
        Producer,
    )
    from ._kafpy import run_scenario_py, run_hardening_checks_py
except ModuleNotFoundError:
    ProducerConfig = None  # type: ignore
    Producer = None  # type: ignore
    run_scenario_py = None  # type: ignore
    run_hardening_checks_py = None  # type: ignore

# Configuration classes (Python wrapper, Phase 34)
from .config import (
    ConsumerConfig,
    RoutingConfig,
    RetryConfig,
    BatchConfig,
    ConcurrencyConfig,
)

# Handler types and registration (Phase 35/36)
from .handlers import (
    KafkaMessage,
    HandlerContext,
    HandlerResult,
    register_handler,
)

# Consumer wrapper (Phase 35)
from .consumer import Consumer

# Runtime with KafPy class (Phase 35)
from .runtime import KafPy

# Exception types (Phase 36)
from .exceptions import (
    KafPyError,
    ConsumerError,
    HandlerError,
    ConfigurationError,
)

# Benchmark types (Phase 43)
from .benchmark import BenchmarkResult

__all__ = [
    # Rust extension types (available when _kafpy is built)
    "ProducerConfig",
    "Producer",
    "run_scenario_py",
    "run_hardening_checks_py",
    # Benchmark result types — Phase 43
    "BenchmarkResult",
    # Configuration types — Phase 34
    "ConsumerConfig",
    "RoutingConfig",
    "RetryConfig",
    "BatchConfig",
    "ConcurrencyConfig",
    # Consumer wrapper and runtime — Phase 35
    "Consumer",
    "KafPy",
    # Handler types and registration — Phase 35/36
    "KafkaMessage",
    "HandlerContext",
    "HandlerResult",
    "register_handler",
    # Exception types — Phase 36
    "KafPyError",
    "ConsumerError",
    "HandlerError",
    "ConfigurationError",
]
