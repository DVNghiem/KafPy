---
phase: 13-consumer-runner-store-offset
plan: '01'
type: execute
wave: 1
depends_on: []
files_modified:
  - src/consumer/runner.rs
  - src/consumer/config.rs
autonomous: true
requirements:
  - COMMIT-01
  - COMMIT-02
  - CONFIG-01
  - CONFIG-02
user_setup: []
must_haves:
  truths:
    - "ConsumerRunner::store_offset persists offset to rdkafka in-memory state"
    - "store_offset is called before commit in the two-phase guard"
    - "enable_auto_offset_store=false disables automatic rdkafka offset writes"
    - "enable_auto_commit=false disables automatic rdkafka commit"
  artifacts:
    - path: src/consumer/runner.rs
      provides: ConsumerRunner::store_offset method
      exports:
        - store_offset
    - path: src/consumer/config.rs
      provides: enable_auto_offset_store field, builder method, and rdkafka config
      exports:
        - ConsumerConfig::enable_auto_offset_store
        - ConsumerConfigBuilder::enable_auto_offset_store
  key_links:
    - from: src/coordinator/commit_task.rs
      to: src/consumer/runner.rs
      via: runner.store_offset(topic, partition, offset)
      pattern: OffsetCommitter::commit_partition calls store_offset before commit
---

<objective>
Add `store_offset()` method to `ConsumerRunner` and disable auto-offset-store in `ConsumerConfig`. This enables two-phase manual offset management (store then commit) required for at-least-once delivery guarantees.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
</execution_context>

<context>
@.planning/phases/13-consumer-runner-store-offset/13-CONTEXT.md
@.planning/REQUIREMENTS.md
@src/consumer/runner.rs
@src/consumer/config.rs
</context>

<interfaces>
<!-- Key types and contracts the executor needs. Extracted from codebase. -->

From `src/consumer/runner.rs`:
```rust
pub struct ConsumerRunner {
    consumer: Arc<StreamConsumer>,   // rdkafka::consumer::StreamConsumer
    shutdown_tx: broadcast::Sender<()>,
}

impl ConsumerRunner {
    // Pattern for blocking rdkafka calls (used by commit):
    pub fn commit(&self) -> Result<(), ConsumerError> {
        self.consumer
            .commit_consumer_state(rdkafka::consumer::CommitMode::Async)
            .map_err(ConsumerError::from)?;
        Ok(())
    }
}
```

From `src/consumer/config.rs`:
```rust
pub struct ConsumerConfig {
    pub enable_auto_commit: bool,   // already defaults to false
    // ... other fields
}

pub struct ConsumerConfigBuilder {
    enable_auto_commit: bool,  // defaults to false
    // ... other fields
}

impl ConsumerConfigBuilder {
    pub fn enable_auto_commit(mut self, enabled: bool) -> Self { ... }
}

// Pattern for adding new fields: add to struct, builder, build(), and into_rdkafka_config()
```

From `src/consumer/error.rs` (via runner.rs import):
```rust
pub enum ConsumerError {
    // existing variants; store_offset errors use ConsumerError::from(KafkaError)
}
```
</interfaces>

<tasks>

<task type="auto">
  <name>Task 1: Add `ConsumerRunner::store_offset`</name>
  <files>src/consumer/runner.rs</files>
  <action>
Add `pub async fn store_offset(&self, topic: &str, partition: i32, offset: i64) -> Result<(), ConsumerError>` to `ConsumerRunner` (after the `commit` method, around line 122).

The method must:
1. Call `tokio::task::spawn_blocking` to run the blocking rdkafka `store_offset` call on the blocking thread pool
2. Inside the blocking closure, call `self.consumer.store_offset(topic, partition, offset)` and map errors via `ConsumerError::from`
3. Return the `Result`

Use the `tokio::task::spawn_blocking` pattern — spawn_blocking takes ownership of captured variables. Clone `Arc<StreamConsumer>` before capturing if needed.

Example signature pattern (adapt from `commit`):
```rust
pub async fn store_offset(&self, topic: &str, partition: i32, offset: i64) -> Result<(), ConsumerError> {
    let consumer = Arc::clone(&self.consumer);
    tokio::task::spawn_blocking(move || {
        consumer.store_offset(topic, partition, offset)
            .map_err(ConsumerError::from)
    })
    .await
    .map_err(|_| ConsumerError::Internal("spawn_blocking panicked".into()))?
}
```

Do NOT import `tokio::task::spawn_blocking` separately — it is available via `tokio::task`.
  </action>
  <verify>
    <automated>cargo build --package kafpy 2>&1 | head -50</automated>
  </verify>
  <done>`ConsumerRunner::store_offset(topic, partition, offset)` compiles and returns `Result<(), ConsumerError>`</done>
</task>

<task type="auto">
  <name>Task 2: Add `enable_auto_offset_store` to ConsumerConfig</name>
  <files>src/consumer/config.rs</files>
  <action>
Add `enable_auto_offset_store: bool` field to `ConsumerConfig` struct (field after `enable_auto_commit`):

```rust
pub enable_auto_offset_store: bool,
```

Add to `ConsumerConfigBuilder` struct:
```rust
enable_auto_offset_store: bool,
```

In `ConsumerConfigBuilder::new()`, set default:
```rust
enable_auto_offset_store: false,
```

Add builder method to `ConsumerConfigBuilder`:
```rust
pub fn enable_auto_offset_store(mut self, enabled: bool) -> Self {
    self.enable_auto_offset_store = enabled;
    self
}
```

In `ConsumerConfig::build()` (the builder's `build` method), add to the `Ok(ConsumerConfig { ... })`:
```rust
enable_auto_offset_store: self.enable_auto_offset_store,
```

In `ConsumerConfig::into_rdkafka_config()`, add after the `enable.auto.commit` line:
```rust
.set("enable.auto.offset.store", self.enable_auto_offset_store.to_string())
```

Note: Per D-04, `enable_auto_commit` is already correctly set to `false` (CONFIG-01 satisfied — no change needed).
  </action>
  <verify>
    <automated>cargo build --package kafpy 2>&1 | head -50</automated>
  </verify>
  <done>`ConsumerConfig` has `enable_auto_offset_store` field defaulting to `false`, builder method exists, and `into_rdkafka_config()` sets `enable.auto.offset.store=false`</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| {app} -> rdkafka | Blocking C library call crosses here via spawn_blocking |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-13-01 | Availability | store_offset | accept | rdkafka store_offset is idempotent; retry on failure is correct behavior |
</threat_model>

<verification>
- `cargo build --package kafpy` succeeds with no errors
- `cargo clippy --package kafpy -- -D warnings` passes
- `cargo fmt --check` passes
</verification>

<success_criteria>
1. `ConsumerRunner::store_offset(topic, partition, offset)` exists and calls rdkafka `store_offset` via `spawn_blocking`
2. `ConsumerConfig` has `enable_auto_offset_store: bool` defaulting to `false`
3. `ConsumerConfigBuilder` has `.enable_auto_offset_store(bool)` builder method
4. `into_rdkafka_config()` sets `enable.auto.offset.store=false`
5. CONFIG-01 (`enable.auto.commit=false`) already satisfied per D-04 — confirmed present in existing code
6. All four requirements (COMMIT-01, COMMIT-02, CONFIG-01, CONFIG-02) are addressed
</success_criteria>

<output>
After completion, create `.planning/phases/13-consumer-runner-store-offset/13-01-SUMMARY.md`
</output>
