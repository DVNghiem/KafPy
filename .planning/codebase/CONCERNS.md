# Codebase Concerns

**Analysis Date:** 2026-04-15

## Critical Issues

### 1. No Test Coverage

The codebase has zero tests. There are no files in `tests/` directory and no `#[cfg(test)]` modules in any source files.

- **Files affected:** All source files
- **Impact:** No way to verify correctness, regression detection, or refactoring safety
- **Fix approach:** Add unit tests for error types, config parsing, message routing. Add integration tests for Consumer and Producer with a real Kafka instance or testcontainers.

---

### 2. Consumer Config Ignores User Settings

In `src/consume.rs` lines 106-116, the `create_consumer` method hardcodes values that override user-provided configuration:

```rust
client_config
    .set("fetch.min.bytes", "1048576")  // Ignores config.fetch_min_bytes
    .set("max.partition.fetch.bytes", "10485760")  // Ignores config.max_partition_fetch_bytes
```

- **Files:** `src/consume.rs` (lines 106-116)
- **Impact:** User-specified `fetch_min_bytes` and `max_partition_fetch_bytes` are silently discarded
- **Fix approach:** Use `config.fetch_min_bytes` and `config.max_partition_fetch_bytes` instead of hardcoded values

---

### 3. Unwrap on Arc in add_handler Causes Panic

In `src/consume.rs` line 38-39:

```rust
let router = Arc::get_mut(&mut self.message_router)
    .expect("Failed to get mutable reference to message_router");
```

This will panic if there are any other `Arc` clones of `self.message_router` alive elsewhere. The design requires exclusive access which is fragile.

- **Files:** `src/consume.rs` (line 38-39)
- **Impact:** Panic if message_router is shared across async tasks
- **Fix approach:** Use `Arc<Mutex<MessageRouter>>` or `Arc<RwLock<MessageRouter>>` to allow safe concurrent access

---

### 4. Commented Exit Calls in Shutdown Signal

In `src/consume.rs` lines 238 and 242:

```rust
// std::process::exit(0);
```

The shutdown handlers receive signals but do not actually terminate the process. This leaves the consumer in a zombie state after Ctrl+C or SIGTERM.

- **Files:** `src/consume.rs` (lines 236-244)
- **Impact:** Graceful shutdown signal received but process continues running
- **Fix approach:** Either implement proper shutdown via CancellationToken propagation (already done), or remove the signal handlers entirely since CancellationToken already works

---

## High Priority Concerns

### 5. No Error Recovery or Dead Letter Queue

In `src/consume.rs` line 206-207:

```rust
Err(e) => {
    error!("Failed to process message: {}", e);
    // Implement retry logic or dead letter queue here
}
```

Failed messages are logged and dropped. There is no retry mechanism, no DLQ, and no way for users to recover from processing failures.

- **Files:** `src/consume.rs` (lines 204-208)
- **Impact:** Data loss when message processing fails
- **Fix approach:** Add configurable retry with exponential backoff, and implement a dead letter queue (either a dedicated Kafka topic or a fallback handler)

---

### 6. CustomConsumerContext Has No Functionality

The `CustomConsumerContext` struct in `src/consume.rs` is created with no custom fields and provides only logging callbacks. The rebalance callbacks log but do not perform any recovery action.

- **Files:** `src/consume.rs` (lines 248-288)
- **Impact:** Rebalance events are logged but no action is taken on rebalance failure
- **Fix approach:** Either implement meaningful rebalance handling (store offsets, trigger recovery) or remove the unused context and use `DefaultConsumerContext`

---

### 7. Producer Lazy Init with RwLock Has Race Condition Potential

The `PyProducer` in `src/produce.rs` uses `Arc<RwLock<Option<FutureProducer>>>` for lazy initialization. However, `send_sync` creates its own `tokio::runtime::Runtime` which conflicts with the async runtime already running.

- **Files:** `src/produce.rs` (lines 136-137, 228-229)
- **Impact:** Nested runtime creation may panic in single-threaded executor or cause resource exhaustion
- **Fix approach:** Remove `send_sync` and `in_flight_count` methods that require blocking, or use `block_on` from the existing runtime instead of creating a new one

---

### 8. MessageRouter Does Not Match by Wildcard Correctly

In `src/message_processor.rs` line 86:

```rust
if processor.supported_topic() == message.topic || processor.supported_topic() == "*" {
```

The MessageRouter's `supported_topic` returns `"*"` but the routing logic does not correctly match. If a processor registers for `"*"` it will match all messages, but the routing logic has the processor matching twice (once for exact topic, once for wildcard).

- **Files:** `src/message_processor.rs` (lines 82-107)
- **Impact:** Messages may be processed multiple times if a processor registers for `"*"` and a specific topic
- **Fix approach:** Remove wildcard matching and require explicit topic registration, or deduplicate matches

---

## Medium Priority Concerns

### 9. SASL Password Passed as Plain String

In `src/config.rs` and `src/produce.rs`/`src/consume.rs`, SASL credentials are passed directly to rdkafka via `set("sasl.password", ...)`. If logs are set to debug level, these credentials could appear in log output.

- **Files:** `src/consume.rs` (lines 131-133), `src/produce.rs` (lines 277-279), `src/config.rs`
- **Impact:** Credentials may be leaked in logs if tracing is set to debug
- **Fix approach:** Ensure logging filter excludes sensitive config values, or mask passwords in debug output

---

### 10. Payload Converted to String Without Validation

In `src/kafka_message.rs` lines 56-59:

```rust
let payload = message
    .payload_view::<str>()
    .and_then(|res| res.ok())
    .map(|s| s.to_string());
```

If the payload is not valid UTF-8, `payload_view::<str>` returns `None` and the payload is silently dropped (set to `None`). Binary payloads are lost.

- **Files:** `src/kafka_message.rs` (lines 56-59)
- **Impact:** Non-UTF-8 message payloads are silently discarded
- **Fix approach:** Preserve the raw bytes in a separate field, or provide both `payload_str` (Option<String>) and `payload_bytes` (Option<Vec<u8>))

---

### 11. Python GIL Release in Async Handler

In `src/message_processor.rs` lines 26-49, the Python callback runs inside `Python::attach`. The code spawns a tokio task for async callbacks but the callback result is checked synchronously first.

- **Files:** `src/message_processor.rs` (lines 26-49)
- **Impact:** If the Python callback is slow, it blocks the Tokio executor thread while holding the GIL
- **Fix approach:** Move the entire callback invocation (including sync-to-async conversion) into a `tokio::task::spawn_blocking` call

---

### 12. Environment Variable Parsing with unwrap_or_else

In `src/config.rs` lines 125-164 and 246-285, the `from_env` method uses `unwrap_or_else` with `parse()?` which may return confusing error messages if environment variables are malformed.

- **Files:** `src/config.rs`
- **Impact:** Type mismatch errors (e.g., non-numeric string in numeric field) produce cryptic messages
- **Fix approach:** Provide context in error messages indicating which environment variable failed

---

### 13. No Graceful Degradation on Kafka Connection Failure

The consumer in `src/consume.rs` attempts to subscribe and start consuming immediately. If Kafka is unavailable, the error propagates and the consumer dies without retry logic.

- **Files:** `src/consume.rs` (lines 151-152)
- **Impact:** Transient network failures kill the consumer with no automatic reconnection
- **Fix approach:** Add reconnection with exponential backoff, or use rdkafka's built-in `reconnect.backoff.ms` settings more aggressively

---

### 14. Compression Type Not Validated

In `src/config.rs` and `src/produce.rs`, the `compression_type` field accepts any string value without validation. Invalid compression types will cause rdkafka to fail at producer creation time.

- **Files:** `src/config.rs`, `src/produce.rs`
- **Impact:** Invalid compression type results in cryptic rdkafka error at runtime
- **Fix approach:** Validate compression_type against known values (none, gzip, snappy, lz4, zstd) in config constructor

---

## Low Priority Concerns

### 15. Unused Dependencies in Cargo.toml

The `num_cpus` crate is listed in dependencies but never imported or used in the codebase.

- **Files:** `Cargo.toml` (line 25)
- **Impact:** Extra dependency with no benefit
- **Fix approach:** Remove `num_cpus` from dependencies

---

### 16. DomainError Enum Never Used

The `DomainError` enum in `src/errors.rs` is defined but never referenced in the codebase. The `#[allow(dead_code)]` attribute indicates this is a stub.

- **Files:** `src/errors.rs` (lines 18-41)
- **Impact:** Dead code that may confuse future maintainers
- **Fix approach:** Either implement DomainError usage or remove it

---

### 17. Release Profile Panics Abort

In `Cargo.toml` lines 43-44:

```toml
[profile.release]
panic = "abort"
```

This is appropriate for production builds but means panics do not produce helpful stack traces in error reports.

- **Files:** `Cargo.toml` (line 44)
- **Impact:** Production panics produce minimal debugging information
- **Fix approach:** Consider `panic = "unwind"` with `RUST_BACKTRACE=1` for better production diagnostics, or ensure a panic handler logs appropriately

---

## Documentation Issues

### 18. README.md May Lack Examples

The pyproject.toml references `README.md` as readme but its contents and completeness have not been verified for sufficient usage examples.

- **Files:** `README.md`
- **Impact:** Users may not understand how to use the library
- **Fix approach:** Ensure README contains minimal working examples for both Consumer and Producer

---

## Security Considerations

### 19. No TLS/SSL Configuration for Kafka Connection

The codebase supports `security_protocol` but there is no explicit configuration for SSL/TLS certificate validation, trust stores, or hostname verification.

- **Files:** `src/consume.rs`, `src/produce.rs`, `src/config.rs`
- **Impact:** Cannot securely connect to Kafka clusters requiring TLS
- **Fix approach:** Add `ssl_ca_location`, `ssl_certificate_location`, `ssl_key_location` config options for mTLS support

---

### 20. No Input Sanitization on Topic Names

Topic names provided by Python users are passed directly to rdkafka. Invalid topic names may cause cryptic errors.

- **Files:** `src/consume.rs`, `src/produce.rs`
- **Impact:** Poor error messages for invalid topic names
- **Fix approach:** Validate topic names against Kafka's naming rules (alphanumeric, hyphens, underscores, dots)

---

*Concerns audit: 2026-04-15*