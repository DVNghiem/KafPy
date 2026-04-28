# Kafka Consumer Framework Features Research

**Context**: High-performance Kafka consumer with Rust runtime and Python business logic.
**Target users**: Python developers who want Kafka integration without dealing with Rust internals.
**Research date**: 2026-04-28

---

## Frameworks Analyzed

| Framework | Language | Type | Performance | Maintenance |
|-----------|----------|------|-------------|-------------|
| **confluent-kafka-python** | Python + C (librdkafka) | Low-level client | Highest | Active (Confluent) |
| **aiokafka** | Python | Async client | High | Active |
| **Faust** | Python | Stream processing | Medium | Fork maintained |
| **bytewax** | Rust + Python | Stream processing | High | Active |
| **kafka-python** | Pure Python | Client | Low | Deprecated |

---

## Table Stakes Features

*Must-have features - users will leave if missing.*

### Core Consumer
| Feature | Complexity | Dependencies | Notes |
|---------|------------|--------------|-------|
| **Consumer groups** | Low | rdkafka | Dynamic partition assignment across consumers |
| **Topic subscription** (by name + pattern) | Low | rdkafka | `subscribe(['topic*'])` regex support |
| **Manual offset commit** | Low | rdkafka | `commit()` after processing |
| **Auto offset commit** | Low | rdkafka | Configurable interval |
| **Rebalance listener** | Medium | rdkafka | `on_partitions_revoked/assigned` callbacks |
| **Graceful start/stop** | Low | rdkafka | Deterministic resource cleanup |
| **Pause/resume partitions** | Low | rdkafka | Flow control |

### Message Handling
| Feature | Complexity | Dependencies | Notes |
|---------|------------|--------------|-------|
| **Deserialization** (key + value) | Low | Python | JSON, msgpack, avro via schema registry |
| **Headers access** | Low | rdkafka | Propagate trace context |
| **Timestamp access** | Low | rdkafka | Event time vs processing time |
| **Partition/offset access** | Low | rdkafka | Debugging, idempotency |
| **Consumer group metadata** | Low | rdkafka | `group_id`, `member_id` |

### Producer (minimal for consume-process-produce)
| Feature | Complexity | Dependencies | Notes |
|---------|------------|--------------|-------|
| **Basic produce** | Low | rdkafka | Send processed results downstream |
| **Transactional produce** | Medium | rdkafka | Exactly-once semantics |

### Operations
| Feature | Complexity | Dependencies | Notes |
|---------|------------|--------------|-------|
| **Configuration via Python dict** | Low | Python layer | Map 1:1 to rdkafka config |
| **SSL/SASL support** | Low | rdkafka | Standard Kafka security |
| **Context manager support** (`with`) | Low | Python layer | Clean `__enter__`/`__exit__` |
| **Health checks / readiness** | Low | Python layer | For K8s probes |

---

## Differentiators

*Competitive advantages - the reason users choose this over others.*

### 1. State Management with Persistence
**Complexity**: High | **Dependencies**: RocksDB or SQLite

```python
# User-defined state survives restarts
@app.agent(topic)
async def process(stream):
    async for event in stream:
        state.count += 1  # Automatic persistence
```

- Key-value tables partitioned by key
- Standby replicas for fast recovery
- Integration with Kafka changelog topic

**Why it matters**: Users want "stateful streams" without deploying a separate database.

### 2. Windowed Aggregations
**Complexity**: High | **Dependencies**: State management

- Tumbling windows (fixed, non-overlapping)
- Hopping windows (fixed, overlapping)
- Sliding windows (based on record content)
- Session windows

**Why it matters**: Time-based analytics are the #1 use case for stream processing.

### 3. Dead Letter Queue (DLQ) Handling
**Complexity**: Medium | **Dependencies**: Separate topic or side-effect

```python
@handler.handle(errors=ValidationError)
async def dlq_handler(msg, error):
    await dlq_topic.send(msg.value, headers={'error': str(error)})
```

- Automatic routing of poison pills
- Configurable retry with backoff
- DLQ topic management

**Why it matters**: One bad message shouldn't block the entire pipeline. User complaints about this are frequent.

### 4. Exactly-Once Semantics (Transactional)
**Complexity**: High | **Dependencies**: Transactions API

- Atomic consume-transform-produce
- Offset commit within transaction
- Fencing of duplicate producers

**Why it matters**: Financial and order-processing systems require exactly-once.

### 5. Observability / Metrics
**Complexity**: Medium | **Dependencies**: Prometheus, OpenTelemetry

- Consumer lag metrics
- Commit success/failure rates
- Processing latency histograms
- Integration with Python logging

**Why it matters**: Production monitoring without custom instrumentation.

### 6. Recovery and Fault Tolerance
**Complexity**: High | **Dependencies**: State storage, changelog

- Automatic state recovery on restart
- Standby table replicas
- Rescaling support

**Why it matters**: Users expect "it just works" after a crash.

### 7. Schema Registry Integration
**Complexity**: Medium | **Dependencies**: Schema Registry client

- Avro/Protobuf serialization
- Schema evolution handling
- Compatibility checking

**Why it matters**: Enterprise users have existing schemas.

### 8. Pythonic High-Level API
**Complexity**: Medium | **Dependencies**: Python layer over rdkafka

```python
# Simple @agent decorator vs manual poll loop
@consumer.agent(topic)
async def handler(events):
    async for event in events:
        await process(event)
```

- Agent/decorator-based API
- Type hints throughout
- Async-first design

**Why it matters**: aiokafka requires too much boilerplate; confluent-kafka is too low-level.

---

## Anti-Features

*Deliberately NOT built - things that would hurt the project.*

### 1. Complex DSL or Custom Language
**Avoid**: Kafka Streams-style DSL embedded in Python

Users explicitly want "just Python". They will not learn a new DSL.

```
Anti-pattern:
    stream.filter(lambda x: x.active).map(lambda x: x.amount)

Correct:
    async for event in stream:
        if event.active:
            await process(event)
```

### 2. Heavyweight Infrastructure Requirements
**Avoid**: ZooKeeper, Kafka Streams-style cluster mode, separate processing nodes

Users want: `pip install kafpy` and go.

### 3. Pure Python GIL-Bound Processing
**Avoid**: Processing messages in Python threads

The entire value proposition is Rust performance. Any CPU-bound Python processing should be optional and explicit, not the default path.

### 4. Hidden Magic / Implicit Behavior
**Avoid**:
- Automatic message batching users can't see
- Silent retries users can't configure
- Implicit thread pools

Users need to understand latency and throughput implications.

### 5. GUI or Visual Programming
**Avoid**: Drag-and-drop pipeline builders

Target users are Python developers who prefer code.

### 6. Multi-language Support Initially
**Avoid**: Supporting JavaScript, Java, Go, etc.

Python is the focus. Rust-Python binding complexity is enough.

### 7. Full Kafka Streams Compatibility
**Avoid**: 1:1 API parity with Kafka Streams

This is a Python library, not Kafka Streams port. Clean Python API > API compatibility.

### 8. Built-in ML/Data Science Libraries
**Avoid**: NumPy, Pandas, MLflow integration in core

Users can bring their own. Core should stay lean.

---

## Feature Complexity Matrix

| Feature | Complexity | Effort | Risk |
|---------|------------|--------|------|
| Basic consumer (poll, commit) | Low | Low | Low |
| Consumer groups | Low | Low | Low |
| Async/await API | Low | Low | Low |
| SSL/SASL | Low | Low | Low |
| Rebalance listener | Medium | Medium | Low |
| Context managers | Low | Low | Low |
| DLQ handling | Medium | Medium | Medium |
| State tables | High | High | High |
| Windowed aggregations | High | High | High |
| Exactly-once transactions | High | High | High |
| Recovery/fault tolerance | High | High | High |
| Observability/metrics | Medium | Medium | Medium |
| Schema registry | Medium | Medium | Medium |
| Distributed scaling | High | High | High |

---

## Dependencies Between Features

```
Basic Consumer (prerequisite for all)
    |
    +-- Rebalance Listener
    |       +-- Manual offset commit
    |
    +-- State Management
    |       +-- Windowed aggregations
    |       +-- Recovery/fault tolerance
    |
    +-- DLQ Handling
    |       +-- Retry with backoff
    |
    +-- Observability
            +-- Consumer lag metrics
```

**Recommended launch order**:
1. Basic consumer + consumer groups + manual/auto commit (table stakes)
2. Async/await API + context managers (Pythonic)
3. Rebalance listener + basic DLQ routing (production-ready)
4. State tables + windowing (differentiators)
5. Observability (production polish)

---

## User Pain Points (from Issues)

Based on analysis of confluent-kafka-python, aiokafka, and Faust issues:

| Pain Point | Library | Impact |
|------------|---------|--------|
| Cannot instantiate Message for testing | confluent-kafka | Testing difficulty |
| No context manager support | confluent-kafka | Resource leaks |
| Silent disconnections | confluent-kafka | Hidden failures |
| Performance regression (2.13.0) | confluent-kafka | Throughput issues |
| DLQ handling missing | aiokafka | Pipeline fragility |
| Noisy logging on close | confluent-kafka | Debugging difficulty |
| Consumer.poll() blocking | confluent-kafka | Control flow issues |
| Batch processing complexity | aiokafka | Latency trade-offs |

---

## Summary

| Category | Count | Key Insight |
|----------|-------|-------------|
| Table stakes | 12 | "It works like other Kafka clients" |
| Differentiators | 8 | State + windowing + DLQ + recovery |
| Anti-features | 8 | DSL, magic, heavyweight infra |

**Core thesis**: Rust performance with Python ergonomics, no magic.
