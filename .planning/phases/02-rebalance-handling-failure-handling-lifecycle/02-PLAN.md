---
phase: "02"
slug: rebalance-handling-failure-handling-lifecycle
plan: "01"
type: execute
wave: 1
depends_on: []
files_modified:
  - src/consumer/context.rs
  - src/consumer/runner.rs
  - src/consumer/mod.rs
  - src/runtime/builder.rs
autonomous: true
requirements:
  - CORE-06
  - LIFE-05
  - CORE-08
  - FAIL-01
  - FAIL-02
user_setup: []

must_haves:
  truths:
    - "Rebalance callbacks fire synchronously and commit pending offsets before partition revocation"
    - "Consumer resumes from committed+1 offset after partition reassignment"
    - "CustomConsumerContext implements rdkafka ConsumerContext trait with Debug bound"
    - "cooperative-sticky partition assignment strategy is set by default"
  artifacts:
    - path: "src/consumer/context.rs"
      provides: "CustomConsumerContext with partitions_revoked/assigned callbacks"
      min_lines: 80
    - path: "src/consumer/runner.rs"
      provides: "ConsumerRunner with CustomConsumerContext wiring via create_with_context"
      min_lines: 30
    - path: "src/consumer/mod.rs"
      provides: "Re-exports CustomConsumerContext"
      min_lines: 5
  key_links:
    - from: "src/consumer/context.rs"
      to: "src/offset/offset_tracker.rs"
      via: "OffsetTracker::highest_contiguous() called in partitions_revoked"
      pattern: "offset_tracker\\.highest_contiguous"
    - from: "src/consumer/runner.rs"
      to: "rdkafka::consumer::ConsumerContext"
      via: "create_with_context() method"
      pattern: "create_with_context"
---

<objective>
Create the rebalance handling foundation: CustomConsumerContext with partitions_revoked/assigned callbacks, wire it into ConsumerRunner via create_with_context, and integrate pause/resume for flow control.
</objective>

<context>
@src/consumer/runner.rs
@src/offset/offset_tracker.rs
@src/consumer/mod.rs
@src/runtime/builder.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Create CustomConsumerContext with rebalance callbacks</name>
  <files>src/consumer/context.rs</files>
  <read_first>src/consumer/context.rs, src/consumer/runner.rs, src/consumer/mod.rs, src/runtime/builder.rs</read_first>
  <action>
    Create `src/consumer/context.rs` with a `CustomConsumerContext` struct that:

    1. Holds `Arc<OffsetTracker>`, `Arc<dyn DlqRouter>`, `Arc<SharedDlqProducer>`, and `Arc<parking_lot::Mutex<HashMap<String, bool>>>` for pause state
    2. Implements `rdkafka::consumer::ConsumerContext` (import from `rdkafka::consumer::{ConsumerContext, RebalanceProtocol}`)
    3. In `partitions_revoked`:
       - For each partition in revoked list, call `offset_tracker.highest_contiguous(topic, partition)`
       - If Some(offset), call `consumer.store_offset(topic, partition, offset)` then `consumer.commit_consumer_state(CommitMode::Sync)`
       - Log each commit with topic/partition/offset
    4. In `partitions_assigned`:
       - For each partition, call `offset_tracker.committed_offset(topic, partition)` to get last committed
       - Seek to `committed + 1` using `consumer.seek(topic, partition, committed + 1, 0)`
       - Log each seek with topic/partition/seek_offset
    5. Derive `Debug` for the struct (required by rdkafka ConsumerContext bound)
    6. Add `impl Debug for CustomConsumerContext` block that formats the struct fields

    IMPORTANT PITFALLS:
    - PITFALLS-4.3: Only commit synchronously in partitions_revoked when using cooperative-sticky strategy
    - PITFALLS-4.1: Always seek to committed_offset + 1, not committed_offset
    - PITFALLS-4.2: Never call Python code in rebalance callbacks (rdkafka client thread, no GIL)
    - Use `ConsumerContext` from rdkafka, not `BaseConsumer` directly
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -50</automated>
  </verify>
  <done>
    CustomConsumerContext implements ConsumerContext trait, partitions_revoked commits offsets synchronously, partitions_assigned seeks to committed+1
  </done>
</task>

<task type="auto">
  <name>Task 2: Wire CustomConsumerContext into ConsumerRunner</name>
  <files>src/consumer/runner.rs</files>
  <read_first>src/consumer/runner.rs, src/consumer/mod.rs, src/consumer/context.rs</read_first>
  <action>
    Modify `ConsumerRunner::new()` in `src/consumer/runner.rs` to:

    1. Change `StreamConsumer` type to `StreamConsumer<CustomConsumerContext>` (add generic to ConsumerRunner struct if needed)
    2. Accept `Arc<OffsetTracker>`, `Arc<dyn DlqRouter>`, `Arc<SharedDlqProducer>` as additional constructor parameters
    3. Build `CustomConsumerContext::new(offset_tracker, dlq_router, dlq_producer)`
    4. Use `config.create_with_context(context)` instead of `config.create()` to create the consumer
    5. Add `pause_state: Arc<parking_lot::Mutex<HashMap<String, bool>>>` field to ConsumerRunner for tracking paused partitions
    6. Implement `pause_partition(&self, topic: &str, partition: i32)` that creates TopicPartitionList and calls `self.consumer.pause()`
    7. Implement `resume_partition(&self, topic: &str, partition: i32)` that creates TopicPartitionList and calls `self.consumer.resume()`

    Update `src/consumer/mod.rs` to add:
    ```
    pub mod context;
    pub use context::CustomConsumerContext;
    ```

    The struct definition becomes:
    ```rust
    pub struct ConsumerRunner {
        consumer: Arc<StreamConsumer<CustomConsumerContext>>,
        shutdown_tx: broadcast::Sender<()>,
        coordinator: Option<Arc<ShutdownCoordinator>>,
        pause_state: Arc<parking_lot::Mutex<HashMap<String, bool>>>,
    }
    ```
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -50</automated>
  </verify>
  <done>
    ConsumerRunner created with CustomConsumerContext via create_with_context, pause/resume methods implemented
  </done>
</task>

<task type="auto">
  <name>Task 3: Update RuntimeBuilder to pass dependencies</name>
  <files>src/runtime/builder.rs</files>
  <read_first>src/runtime/builder.rs, src/consumer/runner.rs, src/consumer/context.rs</read_first>
  <action>
    Update `RuntimeBuilder::build()` in `src/runtime/builder.rs`:

    1. Change ConsumerRunner creation to pass offset_tracker, dlq_router, and dlq_producer:
    ```rust
    let context = CustomConsumerContext::new(
        Arc::clone(&offset_tracker),
        Arc::clone(&dlq_router),
        Arc::clone(&dlq_producer),
    );
    let runner = ConsumerRunner::new(rust_config.clone(), None, Arc::clone(&offset_tracker), Arc::clone(&dlq_router), Arc::clone(&dlq_producer))?;
    ```

    2. Update `ConsumerRunner::new()` signature to accept these additional Arc parameters

    3. Ensure `partition.assignment.strategy` defaults to "cooperative-sticky" in `ConsumerConfigBuilder::into_rdkafka_config()`:
    ```rust
    .set("partition.assignment.strategy", "cooperative-sticky")
    ```
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -50</automated>
  </verify>
  <done>
    RuntimeBuilder passes OffsetTracker, DlqRouter, SharedDlqProducer to ConsumerRunner constructor
  </done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| rdkafka client thread -> Rust | Rebalance callbacks fire in rdkafka internal thread |
| ConsumerRunner -> CustomConsumerContext | Shared Arc references passed at construction |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-02-01 | Repudiation | partitions_revoked | mitigate | Log all offset commits with topic/partition/offset at INFO level |
| T-02-02 | Elevation | pause/resume | mitigate | Only pause/resume owned partitions via TopicPartitionList |
| T-02-03 | Denial | rebalance callback | accept | Callback must be fast; only commit + seek, no Python calls |
</threat_model>

<verification>
cargo build --lib
cargo test --lib -- rebalance pause resume
</verification>

<success_criteria>
- CustomConsumerContext struct exists in src/consumer/context.rs
- partitions_revoked commits offsets via store_offset + commit_consumer_state(Sync)
- partitions_assigned seeks to committed_offset + 1
- ConsumerRunner uses create_with_context instead of create
- RuntimeBuilder passes OffsetTracker, DlqRouter, SharedDlqProducer to ConsumerRunner
- cooperative-sticky partition assignment strategy set by default
</success_criteria>

<output>
After completion, create `.planning/phases/02-rebalance-handling-failure-handling-lifecycle/02.1-SUMMARY.md`
</output>
---

---
phase: "02"
slug: rebalance-handling-failure-handling-lifecycle
plan: "02"
type: execute
wave: 1
depends_on: []
files_modified:
  - src/failure/classifier.rs
  - src/failure/reason.rs
  - src/worker_pool/worker.rs
autonomous: true
requirements:
  - FAIL-01
  - FAIL-02
  - FAIL-03
  - FAIL-04
user_setup: []

must_haves:
  truths:
    - "DefaultFailureClassifier.classify() maps Python exception types to FailureReason variants"
    - "Worker loop calls FailureClassifier after Python handler raises exception"
    - "RetryCoordinator.record_failure() is called with classified FailureReason"
  artifacts:
    - path: "src/worker_pool/worker.rs"
      provides: "Worker loop integration with DefaultFailureClassifier"
      min_lines: 30
    - path: "src/python/context.rs"
      provides: "ExecutionContext passed to FailureClassifier"
      min_lines: 10
  key_links:
    - from: "src/worker_pool/worker.rs"
      to: "src/failure/classifier.rs"
      via: "DefaultFailureClassifier::new() instance passed to worker_loop"
      pattern: "FailureClassifier"
    - from: "src/worker_pool/worker.rs"
      to: "src/retry/retry_coordinator.rs"
      via: "retry_coordinator.record_failure() called with classified reason"
      pattern: "record_failure"
---

<objective>
Wire the existing FailureClassifier and RetryCoordinator into the worker loop so that Python handler exceptions are classified and retried according to policy.
</objective>

<context>
@src/worker_pool/worker.rs
@src/worker_pool/mod.rs
@src/failure/classifier.rs
@src/failure/reason.rs
@src/retry/retry_coordinator.rs
@src/python/context.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Create DefaultFailureClassifier instance in RuntimeBuilder</name>
  <files>src/runtime/builder.rs</files>
  <read_first>src/failure/classifier.rs, src/failure/reason.rs, src/worker_pool/worker.rs</read_first>
  <action>
    In `src/runtime/builder.rs`, add `DefaultFailureClassifier` instantiation:

    1. Import `crate::failure::DefaultFailureClassifier` in the builder
    2. Create the classifier: `let failure_classifier = Arc::new(DefaultFailureClassifier::new())` after dlq components
    3. Pass `Arc::clone(&failure_classifier)` to WorkerPool::new() alongside other dependencies

    Update WorkerPool::new signature in pool.rs to accept:
    `failure_classifier: Arc<dyn FailureClassifier>`

    WorkerPool must store and pass it to each worker_loop call.
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -50</automated>
  </verify>
  <done>
    DefaultFailureClassifier instantiated in RuntimeBuilder and passed to WorkerPool
  </done>
</task>

<task type="auto">
  <name>Task 2: Pass FailureClassifier to worker_loop and call classify()</name>
  <files>src/worker_pool/worker.rs, src/worker_pool/pool.rs</files>
  <read_first>src/worker_pool/worker.rs, src/worker_pool/pool.rs, src/failure/classifier.rs</read_first>
  <action>
    In `src/worker_pool/worker.rs`:

    1. Add `failure_classifier: Arc<dyn FailureClassifier>` parameter to `worker_loop()` function signature
    2. After Python handler returns ExecutionResult::Error { ref exception, .. }:
       - Construct ExecutionContext from msg
       - Call `failure_classifier.classify(exception, &ctx)` to get `FailureReason`
       - Replace the existing direct FailureReason usage with the classified reason
    3. Ensure the classified reason is passed to:
       - `crate::failure::logging::log_failure(&ctx, &classified_reason, ...)`
       - `offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, &classified_reason)`
       - `handle_execution_failure(..., &classified_reason, ...)`

    In `src/worker_pool/pool.rs`:
    1. Add `failure_classifier: Arc<dyn FailureClassifier>` field to WorkerPool struct
    2. Pass it to each `worker_loop(...)` call in the spawn loop
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -50</automated>
  </verify>
  <done>
    worker_loop calls failure_classifier.classify() after Python exception, uses classified FailureReason for retry/DLQ decisions
  </done>
</task>

<task type="auto">
  <name>Task 3: Verify FailureClassifier tests exist and pass</name>
  <files>src/failure/classifier.rs, src/failure/tests.rs</files>
  <read_first>src/failure/classifier.rs, src/failure/tests.rs, src/failure/reason.rs</read_first>
  <action>
    Read `src/failure/classifier.rs` and `src/failure/tests.rs` to verify:

    1. DefaultFailureClassifier implements FailureClassifier trait correctly
    2. TimeoutError -> Retryable(NetworkTimeout)
    3. DeserializationError -> Terminal(DeserializationFailed)
    4. KeyError/ValueError -> NonRetryable(ValidationError)
    5. Unknown exceptions -> NonRetryable(BusinessLogicError)

    If tests don't exist in tests.rs, run the existing tests:
    ```bash
    cargo test --lib failure
    ```

    Add any missing test cases to src/failure/tests.rs to cover:
    - KafkaError -> Retryable(BrokerUnavailable)
    - PanicException -> Terminal(HandlerPanic)
    - ConfigError -> NonRetryable(ConfigurationError)
  </action>
  <verify>
    <automated>cargo test --lib -- failure 2>&1 | tail -30</automated>
  </verify>
  <done>
    DefaultFailureClassifier tests pass, all exception type mappings verified
  </done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| Python exception -> FailureClassifier | Exception info crosses FFI boundary via PyErr |
| FailureClassifier -> RetryCoordinator | Classified reason used for retry state machine |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-02-04 | Information Disclosure | FailureClassifier | mitigate | Only use exception type name for classification, not full traceback |
| T-02-05 | Denial | infinite retry | mitigate | RetryCoordinator enforces max_attempts cap |
</threat_model>

<verification>
cargo build --lib
cargo test --lib -- failure classifier
</verification>

<success_criteria>
- DefaultFailureClassifier instantiated in RuntimeBuilder
- FailureClassifier passed to worker_loop via WorkerPool
- worker_loop calls failure_classifier.classify(exception, ctx) after ExecutionResult::Error
- Classified FailureReason used for logging, mark_failed, and handle_execution_failure
- All existing failure tests pass
</success_criteria>

<output>
After completion, create `.planning/phases/02-rebalance-handling-failure-handling-lifecycle/02.2-SUMMARY.md`
</output>
---

---
phase: "02"
slug: rebalance-handling-failure-handling-lifecycle
plan: "03"
type: execute
wave: 1
depends_on: []
files_modified:
  - src/shutdown/shutdown.rs
  - src/runtime/mod.rs
  - src/worker_pool/pool.rs
  - src/pyconsumer.rs
autonomous: true
requirements:
  - LIFE-01
  - LIFE-02
  - LIFE-03
  - LIFE-04
user_setup: []

must_haves:
  truths:
    - "SIGTERM triggers ShutdownCoordinator.begin_draining() which returns cancellation tokens"
    - "ShutdownCoordinator transitions: Running -> Draining -> Finalizing -> Done"
    - "WorkerPool.shutdown() drains within drain_timeout then calls flush_failed_to_dlq()"
  artifacts:
    - path: "src/runtime/mod.rs"
      provides: "Runtime entry point with SIGTERM handler"
      min_lines: 50
  key_links:
    - from: "src/runtime/mod.rs"
      to: "src/shutdown/shutdown.rs"
      via: "ShutdownCoordinator::begin_draining() called on SIGTERM"
      pattern: "begin_draining"
    - from: "src/worker_pool/pool.rs"
      to: "src/offset/offset_tracker.rs"
      via: "flush_failed_to_dlq() called during shutdown"
      pattern: "flush_failed_to_dlq"
---

<objective>
Implement SIGTERM handling that triggers the 4-phase ShutdownCoordinator lifecycle, and verify that WorkerPool.shutdown() drains and flushes failed messages to DLQ.
</objective>

<context>
@src/shutdown/shutdown.rs
@src/runtime/mod.rs
@src/runtime/builder.rs
@src/worker_pool/pool.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add SIGTERM handler to Runtime entry point</name>
  <files>src/runtime/mod.rs</files>
  <read_first>src/shutdown/shutdown.rs, src/runtime/mod.rs, src/runtime/builder.rs</read_first>
  <action>
    Modify `src/runtime/mod.rs` to add a `run_with_sigterm()` function:

    1. Create a new async function `pub async fn run_with_sigterm(runtime: Runtime)` in `src/runtime/mod.rs`
    2. Inside, spawn a task that waits for SIGTERM using `tokio::signal::unix::signal(SignalKind::terminate())`
    3. When SIGTERM is received, call `runtime.coordinator.begin_draining()` to initiate the shutdown sequence
    4. Log the receipt of SIGTERM at INFO level with the drain_timeout
    5. Keep the existing `Runtime::run()` as `run_blocking()` for cases where SIGTERM handling is not needed

    The implementation pattern:
    ```rust
    use tokio::signal::unix::{signal, SignalKind};
    
    pub async fn run_with_sigterm(runtime: Runtime) {
        let coordinator = Arc::clone(&runtime.coordinator);
        
        tokio::spawn(async move {
            let mut sigterm = signal(SignalKind::terminate()).unwrap();
            sigterm.recv().await;
            tracing::info!(
                drain_timeout_secs = coordinator.drain_timeout().as_secs(),
                "received SIGTERM, initiating graceful shutdown"
            );
            let _ = coordinator.begin_draining();
        });
        
        runtime.run().await;
    }
    ```

    IMPORTANT: Install SIGTERM handler BEFORE runtime.run() starts to avoid race condition.
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -50</automated>
  </verify>
  <done>
    run_with_sigterm() function exists, spawns SIGTERM handler task, calls coordinator.begin_draining() on signal
  </done>
</task>

<task type="auto">
  <name>Task 2: Verify WorkerPool.shutdown() calls flush_failed_to_dlq()</name>
  <files>src/worker_pool/pool.rs</files>
  <read_first>src/worker_pool/pool.rs, src/shutdown/shutdown.rs, src/offset/offset_tracker.rs</read_first>
  <action>
    Read `src/worker_pool/pool.rs` and verify the `shutdown()` method:

    1. It should call `worker_cancel.cancel()` to stop workers from accepting new work
    2. It should wait for in-flight work with `tokio::time::timeout(drain_timeout, ...)`
    3. After workers drain (or timeout), it should call `offset_tracker.flush_failed_to_dlq(dlq_router, dlq_producer)`
    4. Then call `coordinator.begin_finalizing()` to transition Draining -> Finalizing
    5. Finally call `coordinator.set_done()` to transition Finalizing -> Done

    The shutdown sequence in WorkerPool::shutdown() should be:
    ```rust
    // 1. Signal workers to stop accepting new work
    worker_cancel.cancel();
    
    // 2. Wait for in-flight with drain timeout
    let drain_result = tokio::time::timeout(
        coordinator.drain_timeout(),
        self.run_until_idle()
    ).await;
    
    // 3. Flush failed messages to DLQ
    offset_tracker.flush_failed_to_dlq(&dlq_router, &dlq_producer);
    
    // 4. Enter finalization phase
    coordinator.begin_finalizing();
    
    // 5. Mark done
    coordinator.set_done();
    ```

    If flush_failed_to_dlq is NOT called in shutdown(), add it.
  </action>
  <verify>
    <automated>cargo test --lib -- shutdown 2>&1 | tail -30</automated>
  </verify>
  <done>
    WorkerPool.shutdown() flushes failed messages to DLQ before finalizing offsets
  </done>
</task>

<task type="auto">
  <name>Task 3: Wire SIGTERM handler into PyConsumer runtime</name>
  <files>src/pyconsumer.rs</files>
  <read_first>src/pyconsumer.rs, src/runtime/mod.rs, src/runtime/builder.rs</read_first>
  <action>
    Read `src/pyconsumer.rs` to understand how Consumer.start() calls Runtime::run():

    1. Change the call from `runtime.run().await` to `runtime.run_with_sigterm().await` (or similar)
    2. If Runtime doesn't expose run_with_sigterm directly, expose it via a new method

    Look for where `Runtime::run()` is called in the PyConsumer flow and update it to use the SIGTERM-aware entry point.
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -50</automated>
  </verify>
  <done>
    PyConsumer.start() calls the SIGTERM-aware runtime entry point
  </done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| SIGTERM signal -> Runtime | OS signal handled in dedicated Tokio task |
| ShutdownCoordinator -> components | CancellationToken broadcast to all components |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-02-06 | Denial | drain_timeout | mitigate | drain_timeout should be terminationGracePeriod - 5s per PITFALLS-5 |
| T-02-07 | Elevation | SIGTERM handler | accept | Only calls begin_draining, no privileged operations |
</threat_model>

<verification>
cargo build --lib
cargo test --lib -- shutdown
</verification>

<success_criteria>
- run_with_sigterm() function exists in src/runtime/mod.rs
- SIGTERM handler spawns before runtime.run() starts
- SIGTERM triggers coordinator.begin_draining()
- WorkerPool.shutdown() calls flush_failed_to_dlq() before finalizing
- ShutdownCoordinator phases transition: Running -> Draining -> Finalizing -> Done
</success_criteria>

<output>
After completion, create `.planning/phases/02-rebalance-handling-failure-handling-lifecycle/02.3-SUMMARY.md`
</output>
---

---
phase: "02"
slug: rebalance-handling-failure-handling-lifecycle
plan: "04"
type: execute
wave: 2
depends_on:
  - "02.1"
  - "02.2"
  - "02.3"
files_modified:
  - src/dispatcher/backpressure.rs
  - src/dispatcher/consumer_dispatcher.rs
  - src/offset/offset_tracker.rs
autonomous: true
requirements:
  - CORE-08
  - OFF-04
user_setup: []

must_haves:
  truths:
    - "BackpressureAction::FuturePausePartition triggers ConsumerRunner.pause_partition()"
    - "Terminal failure sets has_terminal=true which blocks should_commit()"
    - "Poison message blocks partition advance but other partitions continue"
  artifacts:
    - path: "src/dispatcher/consumer_dispatcher.rs"
      provides: "Dispatcher integration with pause/resume for backpressure"
      min_lines: 20
  key_links:
    - from: "src/dispatcher/consumer_dispatcher.rs"
      to: "src/consumer/runner.rs"
      via: "pause_partition/resume_partition called on BackpressureAction"
      pattern: "pause_partition|resume_partition"
    - from: "src/offset/offset_tracker.rs"
      to: "src/offset/offset_tracker.rs"
      via: "should_commit() gates on has_terminal"
      pattern: "has_terminal"
---

<objective>
Integrate BackpressureAction::FuturePausePartition with ConsumerRunner pause/resume, and verify terminal failure blocking works correctly.
</objective>

<context>
@src/dispatcher/backpressure.rs
@src/dispatcher/consumer_dispatcher.rs
@src/consumer/runner.rs
@src/offset/offset_tracker.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Implement actual partition pause on FuturePausePartition action</name>
  <files>src/dispatcher/consumer_dispatcher.rs</files>
  <read_first>src/dispatcher/backpressure.rs, src/dispatcher/consumer_dispatcher.rs, src/consumer/runner.rs</read_first>
  <action>
    Read `src/dispatcher/consumer_dispatcher.rs` to find where `BackpressureAction::FuturePausePartition` is handled:

    1. Find the code that handles `BackpressureAction::FuturePausePartition(topic)` after `on_queue_full()` returns it
    2. Change it from a no-op to actually call `consumer_runner.pause_partition(topic, partition)`
    3. Store the paused state in a HashMap<String, bool> keyed by "topic-partition" to track pause state
    4. After a configurable delay or when queue depth drops below threshold, call `resume_partition()`
    5. Use `tokio::time::timeout()` or a background task to auto-resume after backpressure subsides

    The pause/resume should be per-partition, not per-topic. Extract partition from the message that triggered the backpressure.

    Implementation:
    ```rust
    BackpressureAction::FuturePausePartition(topic) => {
        if let Some(partition) = queue_manager.get_partition_for_topic(&topic) {
            if !self.is_paused(&topic, partition) {
                tracing::info!(topic = %topic, partition = partition, "pausing partition due to backpressure");
                self.consumer_runner.pause_partition(&topic, partition)?;
                self.set_paused(&topic, partition, true);
                
                // Schedule resume after a grace period
                let runner = self.consumer_runner.clone();
                let topic_clone = topic.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    tracing::info!(topic = %topic_clone, partition = partition, "resuming partition after backpressure");
                    let _ = runner.resume_partition(&topic_clone, partition);
                });
            }
        }
    }
    ```
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -50</automated>
  </verify>
  <done>
    FuturePausePartition action actually pauses the partition, auto-resumes after grace period
  </done>
</task>

<task type="auto">
  <name>Task 2: Verify terminal failure blocking in OffsetTracker</name>
  <files>src/offset/offset_tracker.rs</files>
  <read_first>src/offset/offset_tracker.rs, src/failure/reason.rs, src/dispatcher/backpressure.rs</read_first>
  <action>
    Read `src/offset/offset_tracker.rs` to verify OFF-04:

    1. Find `should_commit()` method - it should return `false` when `has_terminal == true`
    2. Find where `has_terminal` is set - it should be set when `FailureCategory::Terminal` is passed to `mark_failed()`
    3. Verify `has_terminal` is set ONCE and never cleared (idempotent)

    The key code:
    ```rust
    // In should_commit():
    if s.has_terminal {
        return false;  // D-01: terminal blocks commit for this partition only
    }
    ```

    ```rust
    // In mark_failed() via OffsetCoordinator:
    if reason.category() == FailureCategory::Terminal {
        state.has_terminal = true;  // D-03: set once, never clear
    }
    ```

    Run existing tests:
    ```bash
    cargo test --lib -- terminal
    ```

    If tests don't exist or fail, fix the implementation to ensure:
    - should_commit() returns false when has_terminal is true
    - has_terminal is set idempotently (multiple calls don't change state)
  </action>
  <verify>
    <automated>cargo test --lib -- terminal should_commit 2>&1 | tail -30</automated>
  </verify>
  <done>
    should_commit() returns false when has_terminal=true, terminal is set idempotently on Terminal failure
  </done>
</task>

<task type="auto">
  <name>Task 3: Test terminal blocking with poison message</name>
  <files>src/offset/offset_tracker.rs</files>
  <read_first>src/offset/offset_tracker.rs, src/failure/reason.rs</read_first>
  <action>
    Add a test in `src/offset/offset_tracker.rs` `#[cfg(test)]` module:

    ```rust
    #[test]
    fn terminal_failure_blocks_commit_for_partition() {
        let tracker = OffsetTracker::new();
        
        // Ack some offsets
        tracker.ack("topic", 0, 10);
        tracker.ack("topic", 0, 11);
        assert!(tracker.should_commit("topic", 0)); // has_terminal=false
        
        // Mark offset 12 as terminal failure
        let terminal_reason = FailureReason::Terminal(TerminalKind::PoisonMessage);
        tracker.mark_failed("topic", 0, 12);
        // Manually set terminal via offset_coordinator
        // (For this test, directly set via internal state if accessible)
        
        // Verify: should_commit returns false for topic 0
        // But topic 1 should still be able to commit
        assert!(!tracker.should_commit("topic", 0)); // blocked by terminal
        assert!(tracker.should_commit("topic", 1)); // different partition, not blocked
    }
    ```

    Run the test:
    ```bash
    cargo test --lib -- terminal_failure_blocks
    ```
  </action>
  <verify>
    <automated>cargo test --lib -- terminal_failure_blocks 2>&1 | tail -20</automated>
  </verify>
  <done>
    Test passes: terminal failure on partition 0 blocks should_commit for partition 0 only
  </done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| BackpressurePolicy -> ConsumerRunner | Pause/resume called on shared consumer |
| OffsetTracker -> should_commit | Terminal state gates commit for specific partition |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-02-08 | Denial | infinite pause | mitigate | Auto-resume after grace period (30s) to prevent permanent blocking |
| T-02-09 | Denial | terminal blocks all | mitigate | has_terminal is per-partition, not per-consumer |
</threat_model>

<verification>
cargo build --lib
cargo test --lib -- pause terminal should_commit
</verification>

<success_criteria>
- FuturePausePartition action calls pause_partition() on ConsumerRunner
- ConsumerRunner.pause_partition() calls consumer.pause() on TopicPartitionList
- should_commit() returns false when has_terminal=true
- Different partitions are independent (partition A terminal doesn't block partition B)
</success_criteria>

<output>
After completion, create `.planning/phases/02-rebalance-handling-failure-handling-lifecycle/02.4-SUMMARY.md`
</output>
---

---
phase: "02"
slug: rebalance-handling-failure-handling-lifecycle
plan: "05"
type: execute
wave: 2
depends_on:
  - "02.1"
  - "02.2"
files_modified:
  - src/dlq/produce.rs
  - src/dlq/router.rs
  - src/dlq/metadata.rs
  - src/routing/chain.rs
autonomous: true
requirements:
  - FAIL-05
  - FAIL-06
  - FAIL-07
  - MSG-03
user_setup: []

must_haves:
  truths:
    - "SharedDlqProducer.produce_async() sends DlqMetadata as headers"
    - "DefaultDlqRouter computes topic = prefix + original_topic"
    - "KeyRouter.route() returns RoutingDecision::Route on match, Defer on no match"
  artifacts:
    - path: "src/dlq/produce.rs"
      provides: "SharedDlqProducer.produce_async() with metadata_to_headers()"
      min_lines: 30
    - path: "src/dlq/router.rs"
      provides: "DefaultDlqRouter::route() computing dlq prefix"
      min_lines: 20
  key_links:
    - from: "src/worker_pool/mod.rs"
      to: "src/dlq/produce.rs"
      via: "dlq_producer.produce_async(tp.topic, tp.partition, payload, key, &metadata)"
      pattern: "produce_async"
    - from: "src/worker_pool/mod.rs"
      to: "src/dlq/router.rs"
      via: "dlq_router.route(&metadata) -> TopicPartition"
      pattern: "route"
---

<objective>
Verify and test the DLQ routing and production pipeline, and confirm key-based routing is wired as fallback after topic routing.
</objective>

<context>
@src/dlq/produce.rs
@src/dlq/router.rs
@src/dlq/metadata.rs
@src/worker_pool/mod.rs
@src/routing/chain.rs
@src/routing/key.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Verify DLQ metadata headers are correct</name>
  <files>src/dlq/produce.rs</files>
  <read_first>src/dlq/produce.rs, src/dlq/metadata.rs, src/dlq/router.rs</read_first>
  <action>
    Read `src/dlq/produce.rs` to verify `metadata_to_headers()` method:

    1. It should produce headers:
       - `dlq.original_topic` - original topic name
       - `dlq.partition` - original partition
       - `dlq.offset` - original offset
       - `dlq.reason` - failure reason string
       - `dlq.attempts` - attempt count
       - `dlq.first_failure` - RFC3339 timestamp
       - `dlq.last_failure` - RFC3339 timestamp

    2. Verify `produce_async()` calls `metadata_to_headers()` and includes headers in the FutureRecord

    3. Run existing tests:
    ```bash
    cargo test --lib -- dlq_metadata dlq_producer
    ```

    If tests don't exist, add a test that verifies headers are correctly formatted:
    ```rust
    #[test]
    fn dlq_metadata_headers_complete() {
        let metadata = DlqMetadata::new(
            "original-topic".into(), 0, 100,
            "Terminal: PoisonMessage".into(), 3,
            chrono::Utc::now(), chrono::Utc::now(),
        );
        let headers = SharedDlqProducer::metadata_to_headers(&metadata);
        let keys: Vec<_> = headers.iter().map(|(k, _)| k.as_str()).collect();
        assert!(keys.contains(&"dlq.original_topic"));
        assert!(keys.contains(&"dlq.partition"));
        assert!(keys.contains(&"dlq.offset"));
        assert!(keys.contains(&"dlq.reason"));
        assert!(keys.contains(&"dlq.attempts"));
    }
    ```
  </action>
  <verify>
    <automated>cargo test --lib -- dlq_metadata dlq_producer dlq_router 2>&1 | tail -30</automated>
  </verify>
  <done>
    metadata_to_headers() produces all required DLQ headers, produce_async() includes them in Kafka record
  </done>
</task>

<task type="auto">
  <name>Task 2: Verify DefaultDlqRouter computes topic correctly</name>
  <files>src/dlq/router.rs</files>
  <read_first>src/dlq/router.rs, src/dlq/metadata.rs, src/dlq/produce.rs</read_first>
  <action>
    Read `src/dlq/router.rs` to verify `DefaultDlqRouter::route()`:

    1. It should compute: `topic = format!("{}{}", self.dlq_topic_prefix, metadata.original_topic)`
    2. Partition should be `metadata.original_partition`

    3. Run existing tests:
    ```bash
    cargo test --lib -- dlq_router
    ```

    If tests don't exist or don't cover this, add:
    ```rust
    #[test]
    fn default_dlq_router_topic_format() {
        let router = DefaultDlqRouter::new("dlq.".to_string());
        let metadata = DlqMetadata::new(
            "my-topic".into(), 3, 500,
            "Retryable".into(), 2,
            chrono::Utc::now(), chrono::Utc::now(),
        );
        let tp = router.route(&metadata);
        assert_eq!(tp.topic, "dlq.my-topic");
        assert_eq!(tp.partition, 3);
    }
    ```
  </action>
  <verify>
    <automated>cargo test --lib -- dlq_router 2>&1 | tail -20</automated>
  </verify>
  <done>
    DefaultDlqRouter.route() returns topic = dlq_prefix + original_topic, partition preserved
  </done>
</task>

<task type="auto">
  <name>Task 3: Verify key-based routing chain position</name>
  <files>src/routing/chain.rs</files>
  <read_first>src/routing/chain.rs, src/routing/key.rs, src/runtime/builder.rs</read_first>
  <action>
    Read `src/routing/chain.rs` to verify the routing chain order:

    1. The chain order should be: TopicPatternRouter -> HeaderRouter -> KeyRouter -> PythonRouter -> Default
    2. KeyRouter should be at position 3 (fallback after topic and header)
    3. KeyRouter.route() should return `RoutingDecision::Route(handler_id)` on match, `RoutingDecision::Defer` on no match

    2. In `src/runtime/builder.rs`, verify `RoutingChain::new()` is called with:
    ```rust
    let chain = RoutingChain::new()
        .with_topic_router(topic_router)
        .with_header_router(header_router)
        .with_key_router(key_router)  // MSG-03: key-based routing fallback
        .with_default_handler(default_handler.into());
    ```

    3. Run existing key router tests:
    ```bash
    cargo test --lib -- key_router KeyRouter
    ```

    Verify all key router tests pass:
    - exact_bytes match returns Route
    - exact_bytes no match returns Defer
    - prefix_bytes match returns Route
    - prefix_str match returns Route
    - no key returns Defer
  </action>
  <verify>
    <automated>cargo test --lib -- key_router KeyRouter 2>&1 | tail -30</automated>
  </verify>
  <done>
    RoutingChain order is topic -> header -> key -> python -> default, key returns Route on match, Defer on no match
  </done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| WorkerPool -> SharedDlqProducer | Fire-and-forget channel send |
| DLQ metadata -> Kafka headers | User-controlled data in headers |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-02-10 | Denial | DLQ channel full | mitigate | Bounded channel (100), warning logged on drop, not blocking |
| T-02-11 | Information Disclosure | DLQ headers | accept | Metadata is operational data, no sensitive secrets |
</threat_model>

<verification>
cargo build --lib
cargo test --lib -- dlq key_router
</verification>

<success_criteria>
- SharedDlqProducer.produce_async() includes metadata headers
- DefaultDlqRouter.route() computes topic = prefix + original_topic
- RoutingChain order includes KeyRouter at position 3 (after topic, header)
- KeyRouter.route() returns Route on match, Defer on no match
</success_criteria>

<output>
After completion, create `.planning/phases/02-rebalance-handling-failure-handling-lifecycle/02.5-SUMMARY.md`
</output>
---

---
phase: "02"
slug: rebalance-handling-failure-handling-lifecycle
plan: "06"
type: execute
wave: 2
depends_on:
  - "02.1"
  - "02.2"
files_modified:
  - src/pyconfig.rs
  - kafpy/config.py
autonomous: true
requirements:
  - CONF-03
user_setup: []

must_haves:
  truths:
    - "RetryConfig fields map to PyRetryPolicy: max_attempts, base_delay_ms, max_delay_ms, jitter_factor"
    - "PyRetryPolicy.to_rust() produces RetryPolicy with correct Duration values"
  artifacts:
    - path: "src/pyconfig.rs"
      provides: "PyRetryPolicy.to_rust() conversion method"
      min_lines: 15
  key_links:
    - from: "kafpy/config.py"
      to: "src/pyconfig.rs"
      via: "_kafpy.PyRetryPolicy(max_attempts=..., base_delay_ms=..., ...)"
      pattern: "PyRetryPolicy"
---

<objective>
Verify that RetryConfig in Python correctly maps to PyRetryPolicy and to_rust() in Rust produces correct RetryPolicy.
</objective>

<context>
@src/pyconfig.rs
@kafpy/config.py
@src/retry/policy.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Verify PyRetryPolicy.to_rust() produces correct RetryPolicy</name>
  <files>src/pyconfig.rs</files>
  <read_first>src/pyconfig.rs, kafpy/config.py, src/retry/policy.rs</read_first>
  <action>
    Read `src/pyconfig.rs` to verify `PyRetryPolicy::to_rust()`:

    1. It should convert:
       - `max_attempts` -> `RetryPolicy.max_attempts`
       - `base_delay_ms` (u64) -> `RetryPolicy.base_delay` as `Duration::from_millis(base_delay_ms)`
       - `max_delay_ms` (u64) -> `RetryPolicy.max_delay` as `Duration::from_millis(max_delay_ms)`
       - `jitter_factor` -> `RetryPolicy.jitter_factor`

    2. Verify `PyRetryPolicy::to_rust()` is called in `RuntimeBuilder::build()` when creating `ConsumerConfigBuilder`:
    ```rust
    if let Some(ref py_retry) = self.config.default_retry_policy {
        config_builder = config_builder.default_retry_policy(py_retry.to_rust());
    }
    ```

    3. Run existing tests:
    ```bash
    cargo test --lib -- retry_config PyRetryPolicy
    ```

    If tests don't exist, add tests:
    ```rust
    #[test]
    fn py_retry_policy_to_rust_default() {
        let py_policy = PyRetryPolicy::new(3, 100, 30000, 0.1);
        let rust_policy = py_policy.to_rust();
        assert_eq!(rust_policy.max_attempts, 3);
        assert_eq!(rust_policy.base_delay, Duration::from_millis(100));
        assert_eq!(rust_policy.max_delay, Duration::from_millis(30000));
        assert!((rust_policy.jitter_factor - 0.1).abs() < f64::EPSILON);
    }
    ```
  </action>
  <verify>
    <automated>cargo test --lib -- retry_config PyRetryPolicy 2>&1 | tail -30</automated>
  </verify>
  <done>
    PyRetryPolicy.to_rust() correctly converts ms fields to Duration, max_attempts and jitter_factor preserved
  </done>
</task>

<task type="auto">
  <name>Task 2: Verify Python RetryConfig dataclass matches PyRetryPolicy</name>
  <files>kafpy/config.py</files>
  <read_first>kafpy/config.py, src/pyconfig.rs</read_first>
  <action>
    Read `kafpy/config.py` to verify `RetryConfig` dataclass:

    1. It should have fields:
       - `max_attempts: int = 3`
       - `base_delay: float = 0.1` (seconds)
       - `max_delay: float | None = None` (seconds)
       - `jitter_factor: float | None = None`

    2. In `ConsumerConfig.to_rust()`, verify it converts to `PyRetryPolicy`:
    ```python
    if self.retry_policy is not None:
        py_retry_policy = _kafpy.PyRetryPolicy(
            max_attempts=self.retry_policy.max_attempts,
            base_delay_ms=int(self.retry_policy.base_delay * 1000),
            max_delay_ms=int(self.retry_policy.max_delay * 1000) if self.retry_policy.max_delay is not None else 30000,
            jitter_factor=self.retry_policy.jitter_factor if self.retry_policy.jitter_factor is not None else 0.1,
        )
    ```

    3. Verify default values:
       - base_delay 0.1s -> 100ms
       - max_delay None -> defaults to 30000ms in PyRetryPolicy
       - jitter_factor None -> defaults to 0.1

    This is CONF-03: Python RetryConfig dataclass with correct fields and defaults.
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -20</automated>
  </verify>
  <done>
    Python RetryConfig fields match PyRetryPolicy, seconds-to-ms conversion correct, None defaults applied
  </done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| Python RetryConfig -> PyRetryPolicy | Seconds to milliseconds conversion |
| PyRetryPolicy -> RetryPolicy | Duration construction |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-02-12 | Interception | delay values | accept | No sensitive data, operational config only |
</threat_model>

<verification>
cargo build --lib
cargo test --lib -- retry_config PyRetryPolicy
</verification>

<success_criteria>
- PyRetryPolicy.to_rust() converts base_delay_ms to Duration::from_millis
- PyRetryPolicy.to_rust() converts max_delay_ms to Duration::from_millis
- Python RetryConfig.base_delay (float seconds) multiplied by 1000 for ms
- Python None values default correctly in PyRetryPolicy
</success_criteria>

<output>
After completion, create `.planning/phases/02-rebalance-handling-failure-handling-lifecycle/02.6-SUMMARY.md`
</output>
