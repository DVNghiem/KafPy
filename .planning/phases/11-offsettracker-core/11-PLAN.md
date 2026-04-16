---
gsd_state_version: 1.0
milestone: v1.3
phase: 11
phase_name: OffsetTracker Core
status: planning_completed
last_updated: "2026-04-16"
last_activity: 2026-04-16 — Plan created
wave: 1
depends_on: []
autonomous: true
---

# Phase 11 Plan: OffsetTracker Core

## Phase Goal

Implement per-topic-partition ack tracking with BTreeSet-based out-of-order buffering and highest-contiguous-offset algorithm. Pure Rust data structures with no PyO3 or async I/O dependencies.

## must_haves (Goal-Backward Verification)

| Requirement | Description |
|-------------|-------------|
| OFFSET-01 | `PartitionState` struct with `committed_offset: i64`, `pending_offsets: BTreeSet<i64>`, `failed_offsets: BTreeSet<i64>` |
| OFFSET-02 | `OffsetTracker` with `HashMap<TopicPartition, PartitionState>` protected by `parking_lot::Mutex` |
| OFFSET-03 | `OffsetTracker::ack(topic, partition, offset)` inserts offset into `pending_offsets`, advances contiguous cursor |
| OFFSET-04 | `OffsetTracker::highest_contiguous(topic, partition) -> Option<i64>` returns highest consecutive offset starting from `committed_offset + 1` |
| OFFSET-05 | `OffsetTracker::should_commit(topic, partition) -> bool` true when `pending_offsets.contains(committed_offset + 1)` |
| OFFSET-06 | `OffsetTracker::mark_failed(topic, partition, offset)` moves offset from `pending_offsets` to `failed_offsets` |
| OFFSET-07 | `OffsetTracker::committed_offset(topic, partition) -> i64` returns last committed offset |

## Key Algorithm

From research:

1. `PartitionState.committed_offset` starts at `-1` (before any message committed)
2. `ack(topic, partition, offset)`:
   - Insert offset into `pending_offsets`
   - While `pending_offsets.contains(committed_offset + 1)`, remove from pending and increment `committed_offset`
3. `highest_contiguous()` returns `committed_offset` (the last safely committed offset)
4. `should_commit()` returns `pending_offsets.contains(committed_offset + 1)`
5. `mark_failed()` removes offset from `pending_offsets` if present, inserts into `failed_offsets`

## Files to Create

| File | Purpose |
|------|---------|
| `src/coordinator/mod.rs` | Module init, public exports |
| `src/coordinator/offset_tracker.rs` | `PartitionState`, `OffsetTracker`, all offset methods |
| `src/coordinator/error.rs` | `CoordinatorError` enum |

## Tasks

### Task 1: Create `src/coordinator/error.rs`

**Purpose:** Define `CoordinatorError` enum for the coordinator module.

**Read first:**
- `/home/nghiem/project/KafPy/src/dispatcher/error.rs` — reference for error enum pattern

**Action:** Create `src/coordinator/error.rs` with a `CoordinatorError` enum using `thiserror`. Include at minimum:
- `TopicPartitionNotFound(String, i32)` — when offset ops are called on unregistered topic-partition
- `InvalidOffset(i64)` — when offset is negative (offsets must be >= 0)

**Verification:**
- `grep -n "CoordinatorError" src/coordinator/error.rs`
- `grep -n "thiserror" src/coordinator/error.rs`

---

### Task 2: Create `src/coordinator/offset_tracker.rs`

**Purpose:** Implement the core offset tracking data structures and algorithms.

**Read first:**
- `/home/nghiem/project/KafPy/Cargo.toml` — confirm `parking_lot` is available
- `/home/nghiem/project/KafPy/src/dispatcher/mod.rs` — reference for module organization
- `/home/nghiem/project/KafPy/.planning/research/FEATURES.md` — algorithm specification from lines 331-367

**Action:** Create `src/coordinator/offset_tracker.rs` with:

```rust
use std::collections::HashMap;
use std::collections::BTreeSet;
use parking_lot::Mutex;
use rdkafka::TopicPartition;

// OFFSET-01: PartitionState struct
pub struct PartitionState {
    pub committed_offset: i64,
    pub pending_offsets: BTreeSet<i64>,
    pub failed_offsets: BTreeSet<i64>,
}

impl PartitionState {
    pub fn new() -> Self;
    pub fn ack(&mut self, offset: i64);  // Insert into pending, advance cursor
    pub fn mark_failed(&mut self, offset: i64);  // Move from pending to failed
}

// OFFSET-02: OffsetTracker with HashMap<TopicPartition, PartitionState>
pub struct OffsetTracker {
    partitions: Mutex<HashMap<Topic_partition_key, PartitionState>>,
}
```

**Key implementation details:**
- `TopicPartition` from `rdkafka::TopicPartition` — use `topic()` and `partition()` methods
- HashMap key: `(topic: String, partition: i32)` tuple
- `Mutex` from `parking_lot` (not `tokio::sync::Mutex`)
- All state access through `partitions.lock().get(&tp_key)` or `partitions.lock().entry(tp_key)`
- Initial `committed_offset = -1` (per research: "before any message committed")

**OFFSET-03 (`ack`):**
```rust
pub fn ack(&self, topic: &str, partition: i32, offset: i64) {
    let tp = (topic.to_string(), partition);
    let mut guard = self.partitions.lock();
    let state = guard.entry(tp).or_insert_with(PartitionState::new);
    state.ack(offset);
}
```

Where `PartitionState::ack`:
```rust
fn ack(&mut self, offset: i64) {
    self.pending_offsets.insert(offset);
    self.failed_offsets.remove(&offset);  // Remove from failed if retry succeeded
    // Advance contiguous cursor
    loop {
        let next = self.committed_offset + 1;
        if self.pending_offsets.remove(&next) {
            self.committed_offset = next;
        } else {
            break;
        }
    }
}
```

**OFFSET-04 (`highest_contiguous`):**
```rust
pub fn highest_contiguous(&self, topic: &str, partition: i32) -> Option<i64> {
    let tp = (topic.to_string(), partition);
    let guard = self.partitions.lock();
    guard.get(&tp).map(|s| s.committed_offset)
}
```

**OFFSET-05 (`should_commit`):**
```rust
pub fn should_commit(&self, topic: &str, partition: i32) -> bool {
    let tp = (topic.to_string(), partition);
    let guard = self.partitions.lock();
    guard.get(&tp).map_or(false, |s| {
        s.pending_offsets.contains(&(s.committed_offset + 1))
    })
}
```

**OFFSET-06 (`mark_failed`):**
```rust
pub fn mark_failed(&self, topic: &str, partition: i32, offset: i64) {
    let tp = (topic.to_string(), partition);
    let mut guard = self.partitions.lock();
    if let Some(state) = guard.get_mut(&tp) {
        state.mark_failed(offset);
    }
}
```

**OFFSET-07 (`committed_offset`):**
```rust
pub fn committed_offset(&self, topic: &str, partition: i32) -> i64 {
    let tp = (topic.to_string(), partition);
    let guard = self.partitions.lock();
    guard.get(&tp).map_or(-1, |s| s.committed_offset)
}
```

**Unit tests (required in the same file):**
- Sequential acks: ack 10, 11, 12 → highest_contiguous = 12
- Out-of-order: ack 10, ack 12, ack 11 → highest_contiguous = 12
- Should_commit: ack 10 → should_commit = true
- Failed offset: ack 10, ack 11, mark_failed(10) → pending still has 10+1=11, commit still viable
- Starting state: committed_offset = -1 for new partition

**Verification:**
- `grep -n "impl PartitionState" src/coordinator/offset_tracker.rs`
- `grep -n "impl OffsetTracker" src/coordinator/offset_tracker.rs`
- `grep -n "fn ack" src/coordinator/offset_tracker.rs`
- `grep -n "fn highest_contiguous" src/coordinator/offset_tracker.rs`
- `grep -n "fn should_commit" src/coordinator/offset_tracker.rs`
- `grep -n "fn mark_failed" src/coordinator/offset_tracker.rs`
- `grep -n "fn committed_offset" src/coordinator/offset_tracker.rs`
- `grep -n "#[cfg(test)]" src/coordinator/offset_tracker.rs`
- `grep -n "mod tests" src/coordinator/offset_tracker.rs`

---

### Task 3: Create `src/coordinator/mod.rs`

**Purpose:** Module init with public exports.

**Read first:**
- `/home/nghiem/project/KafPy/src/dispatcher/mod.rs` — module pattern reference

**Action:** Create `src/coordinator/mod.rs`:
```rust
pub mod error;
pub mod offset_tracker;

pub use error::CoordinatorError;
pub use offset_tracker::{OffsetTracker, PartitionState};
```

**Verification:**
- `grep -n "pub mod error" src/coordinator/mod.rs`
- `grep -n "pub mod offset_tracker" src/coordinator/mod.rs`
- `grep -n "pub use" src/coordinator/mod.rs`

---

### Task 4: Add coordinator module to `src/lib.rs`

**Purpose:** Wire coordinator into the crate's public API.

**Read first:**
- `/home/nghiem/project/KafPy/src/lib.rs`

**Action:** Add to `src/lib.rs`:
```rust
pub mod coordinator;
```

**Verification:**
- `grep -n "pub mod coordinator" src/lib.rs`

---

### Task 5: Verify compilation

**Action:** Run `cargo build` to confirm all files compile without errors.

**Verification:**
- `cargo build --lib 2>&1 | tail -20` — no errors
- `cargo check 2>&1 | tail -20` — no errors

---

### Task 6: Run unit tests

**Action:** Run `cargo test offset_tracker` to execute unit tests in offset_tracker.rs.

**Verification:**
- `cargo test offset_tracker -- --nocapture` — tests pass
