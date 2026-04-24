# Message Flow

## Consumer Message Flow

Detailed flow from Kafka message arrival to Python handler execution and offset commit.

```mermaid
sequenceDiagram
    participant K as Kafka<br/>(rdkafka)
    participant CR as ConsumerRunner
    participant D as Dispatcher
    participant QM as QueueManager
    participant BP as BackpressurePolicy
    participant RC as RoutingChain
    participant WP as WorkerPool
    participant PY as PythonHandler
    participant OC as OffsetCoordinator
    participant P as Python

    Note over K,P: Message Ingestion Phase
    K->>CR: KafkaStream::next() returns OwnedMessage
    CR->>D: dispatch(message)
    D->>QM: check_queue_depth(handler_id)
    QM-->>D: queue_depth
    D->>BP: evaluate_backpressure(handler_id, depth)
    BP-->>D: BackpressureAction

    alt BackpressureAction::Drop
        D-->>CR: DispatchOutcome::Backpressured
        Note over CR: Advance offset, skip handler
    else BackpressureAction::Wait
        D->>D: mpsc::send() blocks
    else BackpressureAction::FuturePausePartition
        D->>CR: pause_partition(partition)
    end

    D->>D: mpsc::send(message)
    D-->>CR: DispatchOutcome::Dispatched

    Note over CR,K: Continues polling

    Note over CR,P: Handler Execution Phase
    WP->>WP: worker_loop() pops from queue
    WP->>RC: route(message)
    RC-->>WP: RoutingDecision::Route(handler_id)
    WP->>PY: invoke(message, context)
    Note over PY: spawn_blocking acquires GIL

    rect rgb(255, 248, 225)
        Note over PY,P: GIL Boundary (spawn_blocking)
        PY->>P: Python callback execution
        P-->>PY: HandlerResult
    end

    PY-->>WP: ExecutionResult

    Note over WP,P: GIL released after spawn_blocking

    alt ExecutionResult::Success
        WP->>OC: record_ack(topic, partition, offset)
        OC->>OC: track_highest_contiguous(partition, offset)
    else ExecutionResult::Retry
        WP->>WP: schedule_retry(message, attempt)
    else ExecutionResult::Dlq
        WP->>WP: route_to_dlq(message, metadata)
    end

    Note over OC,P: Offset Commit Phase
    OC->>K: commit_offsets(partition, offset)
    K-->>OC: commit result
```

## Producer Message Flow

```mermaid
sequenceDiagram
    participant P as Python
    participant PP as PyProducer
    participant FP as FutureProducer
    participant K as Kafka

    P->>PP: producer.send(topic, key, payload)
    PP->>FP: FutureProducer::send(record)
    FP-->>PP: Future<DeliveryResult>
    PP-->>P: await future via future_into_py()

    Note over PP,P: Async context via pyo3-async-runtimes

    FP->>K: Produce request
    K-->>FP: Delivery report (partition, offset)
    FP-->>PP: DeliveryResult
    PP-->>P: (partition, offset) or raise exception
```

## Routing Chain Precedence

```mermaid
flowchart TB
    MSG[Incoming Message] --> TOPIC[Extract Topic]

    TOPIC --> PATTERN{TopicPattern<br/>matches?}
    PATTERN -->|Yes| ROUTE_Pattern[Route to<br/>HandlerId]
    PATTERN -->|No| HEADER{Header<br/>present?}

    HEADER -->|Yes| HEADER_ROUTE[HeaderRouter<br/>returns HandlerId]
    HEADER -->|No| KEY{Key<br/>present?}

    KEY -->|Yes| KEY_ROUTE[KeyRouter<br/>returns HandlerId]
    KEY -->|No| PYTHON{Python Router<br/>available?}

    PYTHON -->|Yes| PY_ROUTE[PythonRouter<br/>dynamic route]
    PYTHON -->|No| DEFAULT{RoutingChain<br/>has default?}

    DEFAULT -->|Yes| DEFAULT_ROUTE[Route to<br/>default HandlerId]
    DEFAULT -->|No| REJECT[RoutingDecision::Reject<br/>→ DLQ]

    ROUTE_Pattern --> DONE[Execute Handler]
    HEADER_ROUTE --> DONE
    KEY_ROUTE --> DONE
    PY_ROUTE --> DONE
    DEFAULT_ROUTE --> DONE
```

## Batch Handler Flow

```mermaid
flowchart TB
    MSG[Message] --> PARTITION{Partition<br/>Accumulator exists?}

    PARTITION -->|New| CREATE[Create PartitionAccumulator]
    PARTITION -->|Exists| ADD[Add to batch buffer]

    CREATE --> CHECK_SIZE{size >=<br/>batch_size?}
    ADD --> CHECK_SIZE

    CHECK_SIZE -->|No| WAIT[Wait for timeout]
    CHECK_SIZE -->|Yes| FLUSH[Flush batch<br/>to Python handler]

    WAIT --> TIMER{Timer<br/>expires?}
    TIMER -->|Yes| FLUSH

    FLUSH --> PY[Python handler<br/>batch callback]
    PY --> RESULT{BatchResult}

    RESULT -->|AllSuccess| ACK[ack all offsets]
    RESULT -->|PartialFailure| RETRY[Retry failed<br/>individually]
    RESULT -->|AllFailure| DLQ[Route all<br/>to DLQ]

    ACK --> CLEAR[Clear buffer<br/>Continue]
    RETRY --> DLQ_ROUTE[DLQ for failed]
    DLQ --> CLEAR
```

## Retry/DLQ Flow

```mermaid
flowchart LR
    E[Execution<br/>Error] --> FC[FailureClassifier]

    FC --> FC_R[FailureCategory]

    FC_R -->|Retryable| RP[RetryPolicy]
    FC_R -->|Permanent| DLQ[→ DLQ]

    RP --> BACKOFF{Exponential<br/>Backoff}

    BACKOFF --> ATTEMPT{attempt <<br/>max_attempts?}

    ATTEMPT -->|Yes| RETRY[Schedule retry<br/>with delay]
    ATTEMPT -->|No| DLQ

    RETRY --> WAIT[Wait for<br/>backoff delay]
    WAIT --> REEXEC[Re-execute<br/>handler]

    REEXEC --> E

    DLQ --> DM[DlqMetadata<br/>envelope]
    DM --> DP[DlqRouter<br/>produce]
    DP --> DK[Kafka DLQ topic]
```