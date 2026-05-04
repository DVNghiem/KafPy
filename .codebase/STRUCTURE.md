# KafPy Directory Structure

```
KafPy/
├── Cargo.toml                  # Rust crate config (name=KafPy, lib=_kafpy, cdylib+rlib)
├── pyproject.toml              # Python package config (maturin build, Python ≥3.11)
├── docker-compose.yaml         # 3-node KRaft Kafka cluster + UI
├── mkdocs.yml                  # Documentation config
├── LICENSE                     # BSD-3-Clause
├── README.md                   # Project readme
├── AGENTS.md                   # Agent instructions
├── idea.md                     # Design notes
├── BENCHMARK-METHODOLOGY.md    # Benchmark methodology doc
├── test.py                     # Ad-hoc integration test script
│
├── src/                         # ─── Rust source (crate: _kafpy) ───
│   ├── lib.rs                  # PyO3 module init, benchmark bindings, Send+Sync assertions
│   ├── config.rs                # ConsumerConfig, ProducerConfig (PyO3-exposed, with builder)
│   ├── pyconfig.rs              # PyRetryPolicy, PyObservabilityConfig, PyFailureCategory, PyFailureReason
│   ├── kafka_message.rs         # KafkaMessage (PyO3-exposed)
│   ├── produce.rs               # PyProducer (PyO3-exposed)
│   ├── pyconsumer.rs            # PyConsumer (PyO3-exposed), HandlerMetadata, fan-out/fan-in registration
│   ├── error.rs                 # Public error type (PyError)
│   ├── errors.rs                # Internal error macros
│   ├── logging.rs               # Internal logger init
│   ├── rayon_pool.rs            # Rayon thread pool for parallel batch processing
│   │
│   ├── consumer/                # Pure Rust Kafka consumer core
│   │   ├── mod.rs               # Re-exports: ConsumerConfig, ConsumerRunner, OwnedMessage
│   │   ├── config.rs            # ConsumerConfigBuilder (Rust-only, builder pattern)
│   │   ├── runner.rs            # ConsumerRunner, ConsumerStream, ConsumerTask
│   │   ├── message.rs           # OwnedMessage, MessageRef, MessageTimestamp
│   │   ├── context.rs           # CustomConsumerContext (rdkafka callbacks)
│   │   └── error.rs             # ConsumerError variants
│   │
│   ├── dispatcher/              # Message routing to per-handler queues
│   │   ├── mod.rs               # Dispatcher, DispatchOutcome, send() API
│   │   ├── consumer_dispatcher.rs # ConsumerDispatcher (pauses/resumes partitions)
│   │   ├── queue_manager.rs     # QueueManager with per-topic semaphores
│   │   ├── backpressure.rs      # BackpressureAction, PauseOnFullPolicy
│   │   └── error.rs             # DispatchError (4 variants per DISP-19)
│   │
│   ├── python/                  # PyO3 handler execution
│   │   ├── mod.rs               # Module re-exports
│   │   ├── handler.rs           # PythonHandler, HandlerMode enum
│   │   ├── executor.rs          # Executor trait and implementation
│   │   ├── execution_result.rs  # ExecutionResult, HandlerOutcome
│   │   ├── context.rs           # PythonExecutionContext
│   │   ├── streaming.rs         # Streaming handler support
│   │   ├── batch.rs             # Batch handler support
│   │   ├── async_bridge.rs      # Async PyO3 bridge
│   │   ├── fan_out_bridge.rs    # Fan-out Python bridge
│   │   ├── fan_in_bridge.rs     # Fan-in Python bridge
│   │   └── logger.rs            # Python logging bridge
│   │
│   ├── worker_pool/             # Worker management
│   │   ├── mod.rs               # WorkerPool, WorkerPoolConfig
│   │   ├── pool.rs              # Pool management, task routing
│   │   ├── worker.rs            # Individual worker logic
│   │   ├── batch_loop.rs        # Batch mode worker loop
│   │   ├── streaming_loop.rs    # Streaming mode worker loop
│   │   ├── fan_out.rs           # Fan-out worker logic and FanOutConfig
│   │   ├── fan_in_loop.rs       # Fan-in aggregation loop
│   │   ├── accumulator.rs       # Message accumulator for batching
│   │   ├── concurrency.rs       # Per-handler concurrency control
│   │   └── state.rs             # Worker pool state tracking
│   │
│   ├── coordinator/              # Offset commit coordinator
│   │   ├── mod.rs               # OffsetCoordinator
│   │   └── error.rs              # Coordinator errors
│   │
│   ├── offset/                  # Offset tracking
│   │   ├── mod.rs               # Re-exports
│   │   ├── offset_tracker.rs    # Highest-contiguous-offset algorithm
│   │   └── commit_task.rs       # Offset commit task
│   │
│   ├── shutdown/                # 4-phase shutdown
│   │   ├── mod.rs               # Re-exports
│   │   └── shutdown.rs          # ShutdownCoordinator, ShutdownPhase enum
│   │
│   ├── retry/                   # Retry scheduling
│   │   ├── mod.rs               # Re-exports
│   │   ├── policy.rs            # RetryPolicy (exponential backoff + jitter)
│   │   └── retry_coordinator.rs # RetryCoordinator
│   │
│   ├── failure/                 # Failure classification
│   │   ├── mod.rs               # Re-exports
│   │   ├── reason.rs            # FailureReason (23 variants)
│   │   ├── classifier.rs        # FailureClassifier
│   │   ├── logging.rs           # Failure event logging
│   │   └── tests.rs             # Unit tests for classification
│   │
│   ├── dlq/                     # Dead Letter Queue
│   │   ├── mod.rs               # Re-exports
│   │   ├── router.rs            # DlqRouter trait
│   │   ├── produce.rs           # Fire-and-forget DLQ produce
│   │   └── metadata.rs          # DlqMetadata envelope
│   │
│   ├── observability/            # Metrics and tracing
│   │   ├── mod.rs               # Re-exports
│   │   ├── config.rs            # ObservabilityConfig
│   │   ├── metrics.rs           # MetricsSink, metric labels, handler metrics
│   │   ├── tracing.rs           # Tracing configuration
│   │   └── runtime_snapshot.rs  # RuntimeSnapshot (queue depths, worker states)
│   │
│   ├── routing/                  # Message routing
│   │   ├── mod.rs               # Re-exports
│   │   ├── context.rs           # RoutingContext (zero-copy)
│   │   ├── decision.rs          # RoutingDecision enum
│   │   ├── router.rs            # Router trait
│   │   ├── topic_pattern.rs     # TopicPatternRouter (regex/glob)
│   │   ├── header.rs            # HeaderRouter
│   │   ├── key.rs               # KeyRouter
│   │   ├── chain.rs             # ChainedRouter (middleware pattern)
│   │   ├── config.rs            # RoutingConfig
│   │   └── python_router.rs     # PythonRouter (custom Python routing)
│   │
│   ├── middleware/               # Handler middleware
│   │   ├── mod.rs               # Re-exports
│   │   ├── traits.rs            # HandlerMiddleware trait
│   │   ├── chain.rs             # MiddlewareChain
│   │   ├── logging.rs           # Logging middleware
│   │   ├── metrics.rs           # Metrics middleware
│   │   └── python.rs            # PythonMiddleware (bridges PyO3)
│   │
│   ├── runtime/                  # Runtime assembly
│   │   ├── mod.rs               # Re-exports
│   │   └── builder.rs           # RuntimeBuilder (wires all components)
│   │
│   └── benchmark/               # Benchmark infrastructure
│       ├── mod.rs               # Re-exports
│       ├── runner.rs            # BenchmarkRunner
│       ├── scenarios.rs         # Throughput/Latency/Failure scenarios
│       ├── results.rs           # BenchmarkResult
│       ├── measurement.rs       # Latency measurement (TDigest)
│       └── hardening.rs         # Production readiness checks
│
├── kafpy/                        # ─── Python package ───
│   ├── __init__.py              # Public API re-exports, logging setup
│   ├── _kafpy.pyi               # Type stubs for Rust extension
│   ├── config.py                # ConsumerConfig, RoutingConfig, RetryConfig, etc.
│   ├── consumer.py              # Consumer wrapper class
│   ├── handlers.py              # KafkaMessage, HandlerContext, HandlerResult, HandlerAction
│   ├── runtime.py               # KafPy class (handler decorator, run/stop)
│   ├── fanout.py                # FanOutBuilder, FanOutRegistration
│   ├── exceptions.py            # KafPyError, ConsumerError, HandlerError, ConfigurationError
│   └── benchmark.py             # BenchmarkResult, ScenarioConfig, run_scenario, CLI
│
├── tests/                        # ─── Test files ───
│   ├── test_exceptions.py       # Python exception hierarchy tests (ERR-01--05)
│   ├── builder_test.rs          # Rust ConsumerConfigBuilder tests
│   └── dispatcher_test.rs       # Rust Dispatcher tests (DISP-01--20)
│
├── docs/                         # ─── Documentation ───
│   ├── index.md, getting-started.md, installation.md
│   ├── api/, architecture/, guides/, contributing/
│   ├── consumer.md, handlers.md, routing.md
│   ├── configuration.md, error-handling.md
│   ├── benchmark.md, best-practices.md, use-cases.md
│
├── .github/                      # GitHub config (likely empty)
├── .claude/                      # Claude Code config
└── .opencode/                    # OpenCode config
```