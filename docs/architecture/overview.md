# Architecture Overview

## High-Level Architecture

KafPy is a PyO3 native extension where Rust provides the runtime/core engine and Python holds the business logic.

```mermaid
graph TB
    subgraph Python["Python Layer (kafpy/)"]
        PYC[Consumer<br/>Producer<br/>Config]
    end

    subgraph PyO3["PyO3 Binding Layer (_kafpy)"]
        PYO3[lib.rs<br/>#[pymodule] _kafpy]
    end

    subgraph RustCore["Rust Core (src/)"]
        CONS[consumer/]
        DISP[dispatcher/]
        WORK[worker_pool/]
        ROUT[routing/]
        OFF[offset/]
        SHDN[shutdown/]
        RETRY[retry/]
        DLQ[dlq/]
        OBS[observability/]
        PY[runtime/<br/>python/]
    end

    subgraph Kafka["Kafka (rdkafka)"]
        KAFKA[Bootstrap Servers]
    end

    Python --> PyO3
    PyO3 --> RustCore
    RustCore --> Kafka
    Kafka --> RustCore

    style Python fill:#f5f5f5
    style PyO3 fill:#fff3e0
    style RustCore fill:#e3f2fd
    style Kafka fill:#ffecb3
```

## Module Organization

```mermaid
graph TD
    subgraph Public["Public API (#[pymodule])"]
        lib[lib.rs<br/>_kafpy module]
        km[KafkaMessage]
        pc[PyConsumer]
        pp[PyProducer]
        cc[ConsumerConfig]
        pc2[ProducerConfig]
    end

    subgraph PyBound["Python Boundary (python/)"]
        ph[PythonHandler]
        ex[Executor trait]
        er[ExecutionResult]
        ctx[ExecutionContext]
    end

    subgraph Core["Pure Rust Core (no PyO3)"]
        subgraph Cons["Consumer Core"]
            cr[consumer/runner.rs]
            cm[consumer/message.rs]
            cc2[consumer/config.rs]
        end

        subgraph Disp["Dispatcher"]
            d[Dispatcher]
            qm[QueueManager]
            bp[BackpressurePolicy]
        end

        subgraph Work["Worker Pool"]
            wp[WorkerPool]
            wl[worker_loop]
            bl[batch_worker_loop]
            st[WorkerState<br/>BatchState]
        end

        subgraph Rout["Routing"]
            rc[RoutingChain]
            r[Router trait]
            hi[HandlerId newtype]
            rd[RoutingDecision]
        end
    end

    subgraph Internal["Internal Modules (not PyO3-exposed)"]
        off[offset/]
        shdn[shutdown/]
        retry[retry/]
        dlq[dlq/]
        obs[observability/]
        rt[runtime/]
        fail[failure/]
    end

    Public --> PyBound
    PyBound --> Core
    Core --> Internal

    classDef public fill:#c8e6c9
    classDef pybound fill:#ffe0b2
    classDef core fill:#bbdefb
    classDef internal fill:#e1bee7

    class lib,km,pc,pp,cc,pc2 public
    class ph,ex,er,ctx pybound
    class cr,cm,cc2,d,qm,bp,wp,wl,bl,st,rout,r,hi,rd core
    class off,shdn,retry,dlq,obs,rt,fail internal
```

## Key Design Decisions

| Decision | Rationale | Location |
|----------|-----------|----------|
| Rust core / Python business logic | Performance + idiomatic bindings | src/lib.rs |
| rdkafka for Kafka protocol | Battle-tested, async-capable | Cargo.toml |
| Tokio for async runtime | Native rdkafka compat, mpsc channels | Cargo.toml |
| PyO3-free consumer core | Clean separation, testable without Python | src/consumer/ |
| Per-topic bounded queue dispatch | Isolated backpressure per topic | src/dispatcher/ |
| BackpressurePolicy trait | Extensible backpressure (Drop/Wait/FuturePausePartition) | src/dispatcher/backpressure.rs |
| Executor trait | Future retry/commit/async/batch policies plug in here | src/python/executor.rs |
| OffsetCoordinator trait | Separates offset tracking from Executor policy | src/offset/offset_coordinator.rs |
| Highest contiguous offset commit | Only commit when all prior offsets acked | src/offset/offset_tracker.rs |
| store_offset + commit coordination | enable.auto.offset.store=false, explicit coordination | src/offset/ |
| RetryCoordinator 3-tuple | (should_retry, should_dlq, delay) controls retry and DLQ routing | src/retry/retry_coordinator.rs |
| HandlerId newtype wrapper | Prevents accidental interchange with topic names | src/routing/context.rs |