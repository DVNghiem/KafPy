# Rust-Core, Python-Logic Kafka Consumer Framework

## Overview

This project is a high-performance Kafka consumer framework built around a simple idea: keep the runtime core in Rust, while allowing application teams to write all business logic in Python.

Rust is responsible for the parts that need strong concurrency, low memory overhead, predictable performance, and safe systems-level control. Python is responsible for message handling logic, routing customization, and the developer-facing programming model.

The goal is to combine the execution efficiency of Rust with the flexibility and productivity of Python.

## Core Idea

The framework provides a Rust-based Kafka consumer engine that manages the full message-consumption lifecycle. Instead of implementing message processing directly in Rust, users register Python handlers that are invoked by the Rust runtime.

At a high level, the system works like this:

1. A Rust Kafka consumer reads messages from one or more topics.
2. The Rust runtime converts Kafka messages into owned internal message objects.
3. Messages are routed to internal queues based on topic names or custom routing rules.
4. The runtime manages concurrency, batching, backpressure, retries, offset tracking, and lifecycle coordination.
5. Python handlers receive the message payloads and execute the business logic.
6. Rust remains responsible for execution control, delivery semantics, memory usage, and operational safety.

## Why This Exists

Most teams want Python for fast business logic development, but Python alone is not ideal for a high-throughput Kafka runtime that needs strict control over memory, queue pressure, partition coordination, and concurrency.

Rust is a better fit for:

- High-throughput message ingestion
- Bounded queues and backpressure
- Low-overhead concurrency with Tokio
- Partition-aware offset tracking
- Retry and DLQ orchestration
- Graceful shutdown and rebalance handling
- Predictable memory behavior
- Production-grade observability

Python is still a strong fit for:

- Business rules
- Data transformation
- Integrations with existing Python ecosystems
- Fast experimentation
- Developer ergonomics

This project aims to let each language do what it does best.

## Design Principles

### Rust owns the runtime

Rust should own all infrastructure and execution control concerns, including:

- Kafka consumption
- Dispatching and internal routing
- Queueing and backpressure
- Worker orchestration
- Offset and commit management
- Retry scheduling and DLQ flow
- Shutdown and rebalance lifecycle
- Metrics, tracing, and runtime health

### Python owns the business logic

Python should be the place where users define message handlers and business behavior.

The framework should expose a clean Python-facing API so users can write code like:

```python
@app.handler("orders.created")
def handle_order(message):
    ...
```

The Python layer should feel like a real framework, not like thin bindings over Rust internals.

### Public APIs must reflect reality

If a config, option, or public API is exposed, it must have a real runtime effect.

The project should avoid:

- Placeholder abstractions
- Decorative config objects
- Fake extension points
- Features that exist only in naming but are not wired into execution

The codebase should remain minimal, honest, and maintainable.

### Built for real concurrency, not fake parallelism

Rust can schedule and coordinate thousands of in-flight messages efficiently, but Python execution must still respect the realities of the Python runtime.

Because business logic runs in Python, the framework should treat the Python execution layer as a controlled boundary. The design should avoid pretending that unlimited Rust task spawning automatically means unlimited Python execution parallelism.

## High-Level Architecture

## 1. Kafka Ingestion Layer

A Rust consumer built on top of `rust-rdkafka` receives messages from Kafka topics and converts them into owned internal message structures.

This layer should not expose borrowed Kafka lifetimes outside the ingestion boundary.

## 2. Dispatch and Routing Layer

The runtime routes messages into handler-specific queues.

Routing should support:

- Exact topic-based routing as the default fast path
- Optional routing by headers or keys
- Optional Python-based custom routing as a fallback, not the default hot path

## 3. Queueing and Backpressure Layer

Each handler should have a bounded queue.

The framework must use bounded channels and explicit queue-pressure strategies so memory usage remains predictable. When downstream execution falls behind, the runtime should be able to slow intake or pause the right partitions safely.

## 4. Python Execution Layer

Python handlers are registered through a PyO3-compatible boundary.

Rust workers pull messages from internal queues and invoke Python callbacks safely. Rust continues to own worker scheduling, queue draining, and execution coordination.

## 5. Offset and Delivery Layer

The framework should support at-least-once delivery first.

Offset tracking must be partition-aware. Commits should only advance when contiguous acknowledged offsets are safe to commit.

## 6. Failure Handling Layer

Failures should be classified and handled by Rust.

This includes:

- Retryable errors
- Non-retryable errors
- Retry scheduling with backoff and jitter
- DLQ handoff after retries are exhausted or failure is terminal

## 7. Lifecycle and Operations Layer

The framework should support:

- Graceful shutdown
- Queue draining
- Rebalance-safe partition handling
- Metrics and tracing
- Runtime introspection
- Benchmarking and production tuning

## Intended Developer Experience

The final developer experience should be Python-first.

A user should be able to install the package, define handlers, configure a runtime, and start consuming messages without needing to understand the internal Rust machinery.

Example style:

```python
from framework import App

app = App(
    brokers="localhost:9092",
    group_id="billing-workers",
)

@app.handler("payments.completed", concurrency=32)
def handle_payment(message):
    # business logic here
    return {"ok": True}

app.run()
```

Advanced users should still be able to configure:

- Retry policies
- Batch size and flush timing
- Handler concurrency
- Routing behavior
- DLQ strategy
- Observability settings
- Shutdown and drain behavior

## Non-Goals

This project should avoid becoming:

- A Rust-only framework with token Python bindings
- A fake abstraction layer full of unused config
- A mini rule engine for everything
- A platform that exposes more features than it truly implements
- A design that mixes business logic with runtime control responsibilities

## Success Criteria

This project is successful if it achieves the following:

- Python developers can write handlers easily
- Rust controls the hard runtime problems well
- The system handles high-throughput workloads with bounded memory
- Public APIs match real behavior
- Concurrency and backpressure are explicit and safe
- Offset progression is correct
- Retries and DLQ flows are reliable
- Shutdown and rebalance behavior are production-safe
- Observability and benchmarks make the system explainable
- The codebase stays clean, modular, and honest

## Long-Term Vision

The long-term vision is to create a production-grade framework for teams that want Python productivity without giving up the performance and operational discipline of a Rust runtime.

In short:

- Rust is the engine.
- Python is the control surface for business logic.
- Kafka is the message backbone.
- The framework provides the safe, high-performance bridge between them.
