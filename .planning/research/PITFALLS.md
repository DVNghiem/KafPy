# Pitfalls: Rust/Python Hybrid Kafka Consumer Framework

This document catalogs domain-specific pitfalls when building a framework that combines Rust runtime performance with Python developer ergonomics for Kafka consumption.

---

## 1. Calling Python from Rust: GIL and Async Blocking

### Pitfall 1.1: Blocking the Tokio Event Loop with GIL Acquisition

**Warning signs (detect early):**
- Non-Python async tasks (HTTP clients, database operations) start timing out unexpectedly
- Log messages show "cannot run the event loop while another loop is running"
- CPU usage spikes but throughput drops
- `tokio::spawn` tasks that call Python via `Python::with_gil` or `Python::attach` block entire worker threads

**Prevention strategy:**
- Use `Python::allow_threads` to release the GIL before performing any async await in Rust
- Move Python GIL calls into `tokio::task::spawn_blocking` — not `spawn` — so they don't block the async executor
- Never call `Python::attach` or `Python::with_gil` inside a `tokio::spawn` task; extract the Python call outside the spawn
- Profile with `py-spy` or `PyO3` GIL metrics to detect GIL contention

**Which phase should address it:**
- Architecture phase: decide on async bridge design
- Implementation phase: enforce GIL-release patterns via code review

### Pitfall 1.2: Deadlocking Between GIL and Rust Mutexes

**Warning signs:**
- Application freezes under load with no exception thrown
- Deadlock detected when one thread holds GIL and waits for a Rust mutex another thread holds while it waits for GIL
- `Python::with_gil` closures that also call `async fn` or lock Rust mutexes

**Prevention strategy:**
- Always release GIL via `Python::allow_threads` before locking a Rust mutex or awaiting
- Structure the async bridge so Python calls are short and release GIL immediately
- Use `PyMutex` from `pyo3::sync` when Rust mutex must be held during Python code

**Which phase should address it:**
- Implementation phase: static analysis rules + code review checklist

### Pitfall 1.3: GIL Lifetime mismatches with Python Objects Across Threads

**Warning signs:**
- `PyErr` exceptions in Python callbacks get dropped or produce wrong values
- Segmentation faults when a Python object is used after its originating thread's GIL pool was released
- `Python::with_gil` closure captures a `Py<T>` that was created on a different thread

**Prevention strategy:**
- Understand that `Py<T>` references are bound to a specific `Python<'py>` lifetime and GIL pool
- Clone `Py<T>` references with `.clone_ref(py)` before passing to other threads
- Use `pyo3-asyncio` only with its documented `scope` and `scope_local` APIs for contextvar propagation

**Which phase should address it:**
- Implementation phase

---

## 2. Backpressure and Memory Management

### Pitfall 2.1: Unbounded Internal Queue Growth

**Warning signs:**
- RSS memory grows linearly with consumer lag
- Pod memory limits hit on startup when backlog exists (e.g., 20K messages x 175KB each causes 1GB+ RSS)
- OOM kills in Kubernetes with default memory limits
- No backpressure applied to incoming poll batches

**Prevention strategy:**
- Set `queued.max.messages.kbytes` (default 1GB per partition) to a fraction of available memory
- Set `max.poll.records` to a small multiple of expected concurrent processing capacity
- Monitor `consumerLag` metric and alert on growth rate
- Implement bounded work queues in the Rust worker pool with explicit `poll()` pause when full

**Which phase should address it:**
- Configuration phase: define Kafka client configuration defaults
- Architecture phase: design worker pool with bounded channels

### Pitfall 2.2: Backpressure Reset on Rebalance

**Warning signs:**
- OOM occurring precisely at rebalance events
- Reactor-Kafka issue: when Flux is paused and rebalance occurs, `pausedByUs` flag is reset to `false`, consumer polls unbounded data
- Consumer lag spikes after deployments or scaling events

**Prevention strategy:**
- After rebalance callback, explicitly re-pause partitions if downstream is slow
- Use `consumer.pause(partitions)` per-partition (not global pause) when processing is congested
- Verify that your library re-applies backpressure state after `onPartitionsAssigned` / incremental assign

**Which phase should address it:**
- Architecture phase: rebalance handler design
- Testing phase: chaos testing with partition revocation under load

### Pitfall 2.3: GIL-Enabled Python Blocking the Rust Event Loop

**Warning signs:**
- Python code (e.g., `pyarrow`, NumPy operations) holds GIL for 5ms+ and blocks async progress
- Network timeouts in non-Python async code while Python code runs
- "Task frozen" symptoms in Tokio tasks that do not interact with Python

**Prevention strategy:**
- Move CPU-intensive or blocking Python calls to `spawn_blocking` or dedicated thread pool
- Use free-threaded Python (3.13+) where GIL contention is reduced
- Monitor GIL hold times; warn if any Python call exceeds 1ms in async context

**Which phase should address it:**
- Performance testing phase

---

## 3. Async Rust + Sync Python Execution Model

### Pitfall 3.1: Mismatched Event Loop Context

**Warning signs:**
- `asyncio.get_running_loop()` fails inside Rust threads
- Python `RuntimeError: Cannot run the event loop while another loop is running` in pyo3-asyncio
- Context variables (`contextvars`) not propagated across async boundaries

**Prevention strategy:**
- Use `pyo3_asyncio::scope` / `pyo3_asyncio::scope_local` to attach Python event loop to Rust tasks
- Do not assume Python async code runs on the same thread as Rust async code
- Use `pyo3_asyncio::get_current_locals` to retrieve task-local context from Python

**Which phase should address it:**
- Architecture phase: async bridge design
- Integration testing phase

### Pitfall 3.2: Sync Python Handler Blocking the Async Rust Pipeline

**Warning signs:**
- Python handler is defined as `sync fn` but framework expects `async fn`
- Entire async pipeline stalls when Python handler acquires GIL
- High latency spikes correlated with Python handler execution

**Prevention strategy:**
- Define Python handler interface as `async fn` explicitly
- Wrap sync Python handlers in `asyncio.to_thread` or `run_in_executor` at the Python boundary
- Use `pyo3_asyncio` to convert Rust async futures to Python awaitables rather than blocking Rust on sync Python

**Which phase should address it:**
- Interface design phase

### Pitfall 3.3: Signal Handling (SIGINT) Not Propagated to Python

**Warning signs:**
- `KeyboardInterrupt` not caught in Python handler on Ctrl+C
- `pyo3` initializes Python with `Py_InitializeEx(0)` which disables signal handlers
- Python asyncio event loop does not shut down gracefully on SIGINT

**Prevention strategy:**
- Register signal handlers in Rust that propagate to Python `asyncio.loop.add_signal_handler`
- Test SIGINT lifecycle: send Ctrl+C to a running consumer and verify graceful shutdown
- Consider `pyo3_asyncio::tokio::run` with proper signal handling setup

**Which phase should address it:**
- Shutdown design phase

---

## 4. Offset Tracking Bugs

### Pitfall 4.1: Off-by-One Error on Seek After Cache Expiration

**Warning signs:**
- Exactly 1 duplicate message per rebalance or cache expiration event
- Commit log shows `clearCacheAndResetPositions` seeks to `lastConsumedOffsets.offset` instead of `offset + 1`
- Reproduction: block longer than `max.poll.interval.ms` triggers cache expiration

**Prevention strategy:**
- When seeking to stored offset after rebalance, always seek to `lastConsumedOffset + 1` to avoid reprocessing last successfully processed message
- Add integration test: process N messages, block past `max.poll.interval.ms`, verify no duplicates on resume
- Document that offset commit must happen AFTER processing completes, not just after poll

**Which phase should address it:**
- Unit testing phase + integration testing phase

### Pitfall 4.2: Reordering of Offset Commit Requests Across Partitions

**Warning signs:**
- Committed offset for one partition moves "backwards" in time (e.g., from 12 to 11)
- Race condition between goroutines committing for different partitions
- More likely with high-frequency manual commits and multi-partition consumption

**Prevention strategy:**
- Serialize all offset commits through a single dedicated task/thread
- Use atomic batch commits: commit offsets for all partitions in one call rather than per-partition
- Add a mutex or channel between the commit caller and the commit execution task
- In librdkafka: avoid calling `rd_kafka_commit` from multiple threads concurrently

**Which phase should address it:**
- Architecture phase: commit coordination design

### Pitfall 4.3: Rebalance-in-progress Offset Commit Causes ILLEGAL_GENERATION

**Warning signs:**
- `ILLEGAL_GENERATION` error returned from offset commit during rebalance
- Consumer loses assignment after commit attempt
- Happens when using EAGER rebalance strategy with manual commits

**Prevention strategy:**
- Do NOT call commit from within rebalance callback for EAGER strategies
- For cooperative-sticky: use `incremental_unassign` and do NOT commit before unassign
- Only commit offsets when a message or partition EOF is returned from `poll()`
- Handle `RebalanceInProgressException` by retrying commit after rebalance completes

**Which phase should address it:**
- Rebalance handler implementation

### Pitfall 4.4: Consumer Resumes from Paused Offset Instead of Committed Offset

**Warning signs:**
- After rebalance, consumer resumes from internal (paused) offset, ignoring committed offset
- Another consumer processed messages in the meantime, but first consumer replays them
- Observed when consumer was paused at time of rebalance

**Prevention strategy:**
- On `onPartitionsAssigned`, call `consumer.seek(partition, committedOffset)` for each assigned partition
- Always compare incoming message offsets with committed offsets; skip messages older than committed
- Use `consumer.committed(partitions)` to retrieve committed offsets post-assignment

**Which phase should address it:**
- Rebalance handler implementation + testing phase

### Pitfall 4.5: Using Stale In-Memory Cache After Rebalance

**Warning signs:**
- Consumer takes back a partition it previously lost and replays messages based on in-memory cache
- Application uses a cache of N messages (e.g., sliding window of 5) and cache is not cleared on revoke
- Observing offset going backwards after redeploy

**Prevention strategy:**
- Clear all in-memory per-partition caches in `onPartitionsRevoked` callback
- Treat partition revocation as cache invalidation event
- Architecture should not rely on in-memory offsets; always use broker-committed offsets for recovery

**Which phase should address it:**
- Rebalance handler implementation + architecture phase

---

## 5. Retry / DLQ Implementation Mistakes

### Pitfall 5.1: Infinite Retries Blocking Consumer and Triggering Rebalances

**Warning signs:**
- Consumer lag continuously grows
- Kafka broker interprets slow consumer, triggering rebalances
- `max.poll.interval.ms` exceeded repeatedly
- "Failing OffsetCommit request since the consumer is not part of an active group" in logs

**Prevention strategy:**
- Distinguish recoverable (transient) vs non-recoverable (deterministic) errors
- For transient errors: use capped exponential backoff (e.g., 3-5 retries with 1s, 2s, 4s backoff)
- For non-recoverable errors (schema mismatch, bad data): route directly to DLQ after 1-2 attempts
- Monitor retry count and alert on DLQ growth rate

**Which phase should address it:**
- Error handling design phase

### Pitfall 5.2: DLQ Without Redrive Path (Graveyard Pattern)

**Warning signs:**
- DLQ accumulates messages that are never reprocessed
- No alerting on DLQ message volume
- Failed messages are "parked" indefinitely with no way to recover

**Prevention strategy:**
- Implement a redrive mechanism: humans can pull messages from DLQ, repair, and republish
- Common patterns: dedicated DLQ consumer that replays, parking-lot topic, or manual replay tooling
- Monitor DLQ depth as a first-class metric alongside consumer lag
- Set TTL/expiration on DLQ messages to avoid unbounded accumulation

**Which phase should address it:**
- Architecture phase + operations setup

### Pitfall 5.3: Strict Ordering Broken by DLQ Routing

**Warning signs:**
- Downstream logic expects FIFO per partition but DLQ reprocesses out of order
- Event sourcing application with out-of-sequence events after DLQ reprocessing
- "Parking lot" retry strategy creates temporal disorder

**Prevention strategy:**
- Do not use DLQ for partitioned topics where ordering within partition is critical
- If DLQ is needed: replay DLQ messages back to original partition key to maintain ordering
- Consider per-message retry with delay headers rather than DLQ for ordering-sensitive flows

**Which phase should address it:**
- Architecture phase: determine ordering requirements per topic

### Pitfall 5.4: Retrying Non-Retryable Errors (Deterministic Failures)

**Warning signs:**
- Same message repeatedly fails with same error (e.g., `InvalidFormatException`, `SchemaViolation`)
- Retry count header keeps incrementing but never reaches DLQ
- DLQ is not reached because retries are infinite for all error types

**Prevention strategy:**
- Classify exceptions at message processing time: retryable vs non-retryable
- Apply retry only to transient errors (timeout, temporary DB unavailability)
- Non-retryable errors should skip retries and go directly to DLQ
- Set `maxAttempts` per exception type in error handler configuration

**Which phase should address it:**
- Error handling design phase

---

## 6. Shutdown and Rebalance Handling: Data Loss or Duplication

### Pitfall 6.1: Not Committing Pending Offsets Before Revocation

**Warning signs:**
- Messages processed but not committed are lost on rebalance
- `onPartitionsRevoked` only calls rollback, not commit
- Later attempts to commit receive `RebalanceInProgressException`
- Downstream sees duplicate processing from OTHER consumer (the one that picked up the partitions)

**Prevention strategy:**
- In `onPartitionsRevoked`, commit all pending offsets for revoked partitions before calling `assign(NULL)`
- NiFi issue: `ConsumerLease.onPartitionsRevoked` must track uncommitted offsets and commit them
- Manual commit must be inside the revoke callback, before the revoke completes
- For cooperative rebalancing: commit pending offsets for partitions being revoked, then call `incremental_unassign`

**Which phase should address it:**
- Rebalance handler implementation

### Pitfall 6.2: Closing Consumer Triggers Racing Offset Commit

**Warning signs:**
- During graceful shutdown, consumer unsubscribes, then another consumer picks up partitions before offsets committed
- Offsets are committed "out of range" or "backwards"
- Rolling restart causes message duplication across partitions

**Prevention strategy:**
- Use coordinated shutdown: stop consuming NEW messages, finish processing pending, commit all, THEN close
- Set `enable.auto.commit=false` and `auto.offset.reset=none` for manual commit mode
- In librdkafka: do NOT call `rd_kafka_commit` during close; instead rely on revoke callback
- Test rolling restart scenarios in staging with representative traffic

**Which phase should address it:**
- Shutdown design phase + staging testing

### Pitfall 6.3: Cooperative Rebalance with Manual Commit Causes Duplicate Messages

**Warning signs:**
- Partition revocation/assignment cycles 7-8 times before stabilizing
- Messages consumed during rebalance process are received as duplicates
- Manual commit on each message causes `ILLEGAL_GENERATION` errors

**Prevention strategy:**
- Do NOT call manual commit between `ERR__REVOKE_PARTITIONS` and `ERR__ASSIGN_PARTITIONS` callbacks in cooperative rebalancing
- Let the consumer commit automatically or commit only in the consume loop (on message or EOF)
- See Karafka fix: remove manual commit from rebalance callback path

**Which phase should address it:**
- Rebalance handler implementation + integration testing

### Pitfall 6.4: Resume After Rebalance Continues from Wrong Offset (Paused Consumer)

**Warning signs:**
- Consumer paused via `consumer.pause(partitions)` gets reassigned same partition during scaling
- On reassignment, continues from where it left off (its own internal offset) rather than committed offset
- Another consumer has processed messages in the meantime

**Prevention strategy:**
- On partition reassignment, always compare `consumer.position(partition)` against `consumer.committed(partition)`
- If `position > committed`, seek back to `committed`
- Use the workaround pattern: log warning and skip if incoming offset < committed offset

**Which phase should address it:**
- Rebalance handler implementation

### Pitfall 6.5: Interrupted Shutdown (SIGTERM) Causes Message Loss

**Warning signs:**
- Kubernetes pod termination: messages in-flight are dropped
- No graceful drain period before container is killed
- `session.timeout.ms` expires before consumer can commit on shutdown

**Prevention strategy:**
- Implement graceful SIGTERM handling: drain for bounded time, commit offsets, then exit
- Set `internal.leave.group.on.close=true` for controlled shutdown
- Configure `connection.max.idle.ms` lower than drain timeout to force reconnect if needed
- Test SIGTERM under load in Kubernetes environment

**Which phase should address it:**
- Shutdown design phase + operations setup

---

## Cross-Cutting Patterns

### Phase Mapping Summary

| Phase | Pitfalls to Address |
|-------|-------------------|
| Architecture | 2.2, 3.1, 4.2, 4.5, 5.2, 5.3, 6.1 |
| Interface Design | 3.2 |
| Rebalance Handler | 4.3, 4.4, 6.1, 6.3, 6.4 |
| Error Handling | 5.1, 5.2, 5.4 |
| Shutdown Design | 6.2, 6.5 |
| Configuration | 2.1 |
| Unit Testing | 4.1 |
| Integration Testing | 4.1, 4.4, 6.3, 6.5 |
| Chaos / Load Testing | 2.2, 6.3 |
| Operations Setup | 5.2, 6.5 |

### Detection Checklist

- [ ] GIL metrics instrumented and alerted
- [ ] Memory growth rate monitored per partition backlog
- [ ] Consumer lag metric with alerting threshold
- [ ] DLQ depth as first-class metric
- [ ] Rebalance count and duplicate message rate in test harnesses
- [ ] Offset commit ordering verified across partitions
- [ ] SIGTERM graceful shutdown tested under load
- [ ] Integration test: process-then-block-past-max_poll_interval-resume verifies no duplicates
