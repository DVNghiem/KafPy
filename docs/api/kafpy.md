# KafPy API Reference

## Top-level Imports

```python
import kafpy
```

The `kafpy` package exposes all public types directly:

| Type | Description |
|------|-------------|
| `kafpy.ConsumerConfig` | Consumer configuration (frozen dataclass) |
| `kafpy.RoutingConfig` | Routing configuration |
| `kafpy.RetryConfig` | Retry configuration |
| `kafpy.BatchConfig` | Batch processing configuration |
| `kafpy.ConcurrencyConfig` | Concurrency configuration |
| `kafpy.Consumer` | Python wrapper around Rust Consumer |
| `kafpy.KafPy` | Main runtime with handler lifecycle |
| `kafpy.KafkaMessage` | Kafka message with typed field access |
| `kafpy.HandlerContext` | Context for handler invocation |
| `kafpy.HandlerResult` | Result of a handler invocation |
| `kafpy.KafPyError` | Base exception (all errors inherit from this) |
| `kafpy.ConsumerError` | Consumer-level errors |
| `kafpy.HandlerError` | Handler processing errors |
| `kafpy.ConfigurationError` | Configuration errors |

## Exception Import

```python
from kafpy.exceptions import KafPyError, ConsumerError, HandlerError, ConfigurationError
```

## Configuration Import

```python
from kafpy.config import ConsumerConfig, RoutingConfig, RetryConfig, BatchConfig, ConcurrencyConfig
```

## Handler Types Import

```python
from kafpy.handlers import KafkaMessage, HandlerContext, HandlerResult, register_handler
```