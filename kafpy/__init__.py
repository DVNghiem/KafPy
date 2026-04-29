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

# ─── Python Logging Configuration ──────────────────────────────────────────────
#
# KafPy uses Python's standard logging module. All log messages from the Rust
# extension are forwarded to Python logging via the "kafpy" logger.
#
# Users can customise logging:
#
#     import logging
#     logging.basicConfig(
#         level=logging.INFO,
#         format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
#     )
#     logging.getLogger("kafpy").setLevel(logging.DEBUG)
#
import logging as _logging
_logger = _logging.getLogger("kafpy")
if not _logger.hasHandlers():
    _handler = _logging.StreamHandler()
    _handler.setFormatter(_logging.Formatter(
        "%(asctime)s - %(name)s - %(levelname)s - %(message)s"
    ))
    _logger.addHandler(_handler)
_logger.setLevel(_logging.INFO)

# Initialize KafPy's Python logging system.
# Users can customize logging via Python's standard logging module:
#
#     import logging
#     logging.basicConfig(
#         level=logging.INFO,
#         format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
#     )
#
# KafPy logs through the "kafpy" logger. To see more detail:
#
#     logging.getLogger("kafpy").setLevel(logging.DEBUG)
import logging
_logger = logging.getLogger("kafpy")
_logger.setLevel(logging.INFO)
# If no handlers are configured, add a basic handler to stderr
if not _logger.handlers:
    _handler = logging.StreamHandler()
    _handler.setFormatter(logging.Formatter(
        "%(asctime)s - %(name)s - %(levelname)s - %(message)s"
    ))
    _logger.addHandler(_handler)

# Re-export from Rust extension (_kafpy) — conditional until extension is built
try:
    from ._kafpy import (
        ProducerConfig,
        Producer,
    )
    from ._kafpy import run_scenario_py, run_hardening_checks_py
    # New PyO3-exposed types
    from ._kafpy import PyRetryPolicy, PyObservabilityConfig
    from ._kafpy import PyFailureCategory, PyFailureReason
except ModuleNotFoundError:
    ProducerConfig = None  # type: ignore
    Producer = None  # type: ignore
    run_scenario_py = None  # type: ignore
    run_hardening_checks_py = None  # type: ignore
    PyRetryPolicy = None  # type: ignore
    PyObservabilityConfig = None  # type: ignore
    PyFailureCategory = None  # type: ignore
    PyFailureReason = None  # type: ignore

# Configuration classes (Python wrapper, Phase 34)
from .config import (
    ConsumerConfig,
    RoutingConfig,
    RetryConfig,
    BatchConfig,
    ConcurrencyConfig,
    ObservabilityConfig,
    FailureCategory,
    FailureReason,
)

# Handler types and registration (Phase 35/36)
from .handlers import (
    KafkaMessage,
    HandlerContext,
    HandlerResult,
    register_handler,
    stream_handler,
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


class BaseMiddleware:
    """Base class for user-defined middleware.

    Subclass and override before(), after(), on_error().
    All methods are optional — default implementations are no-ops.

    Example::

        class MyMiddleware(BaseMiddleware):
            def before(self, ctx):
                print(f"Handling message on {ctx['topic']}")

            def after(self, ctx, result, elapsed_ms):
                print(f"Handler completed in {elapsed_ms}ms with result={result}")
    """

    def before(self, ctx: dict) -> None:
        """Called before the handler is invoked."""
        pass

    def after(self, ctx: dict, result: str, elapsed_ms: float) -> None:
        """Called after the handler succeeds.

        Args:
            ctx: ExecutionContext dict with topic, partition, offset, etc.
            result: Result label (e.g., "ok", "error", "timeout")
            elapsed_ms: Wall-clock time in milliseconds since before() was called
        """
        pass

    def on_error(self, ctx: dict, result: str) -> None:
        """Called when the handler invocation returns an error.

        Args:
            ctx: ExecutionContext dict with topic, partition, offset, etc.
            result: Result label (e.g., "error", "timeout")
        """
        pass


class Logging(BaseMiddleware):
    """Built-in logging middleware.

    Emits tracing span events on handler start/complete/error with trace context.
    Uses Rust tracing/logging infrastructure for structured output.
    """

    pass


class Metrics(BaseMiddleware):
    """Built-in metrics middleware.

    Records kafpy.handler.latency histogram and kafpy.message.throughput counter
    per handler invocation. Uses pre-registered Prometheus metrics.
    """

    pass


__all__ = [
    # Rust extension types (available when _kafpy is built)
    "ProducerConfig",
    "Producer",
    "run_scenario_py",
    "run_hardening_checks_py",
    "PyRetryPolicy",
    "PyObservabilityConfig",
    "PyFailureCategory",
    "PyFailureReason",
    # Benchmark result types — Phase 43
    "BenchmarkResult",
    # Configuration types — Phase 34
    "ConsumerConfig",
    "RoutingConfig",
    "RetryConfig",
    "BatchConfig",
    "ConcurrencyConfig",
    "ObservabilityConfig",
    "FailureCategory",
    "FailureReason",
    # Consumer wrapper and runtime — Phase 35
    "Consumer",
    "KafPy",
    # Handler types and registration — Phase 35/36
    "KafkaMessage",
    "HandlerContext",
    "HandlerResult",
    "HandlerAction",
    "register_handler",
    "stream_handler",
    # Exception types — Phase 36
    "KafPyError",
    "ConsumerError",
    "HandlerError",
    "ConfigurationError",
    # Middleware classes — Phase 09
    "BaseMiddleware",
    "Logging",
    "Metrics",
]