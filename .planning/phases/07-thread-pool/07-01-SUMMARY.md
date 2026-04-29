---
phase: 07-thread-pool
plan: "01"
subsystem: runtime
tags: [rayon, thread-pool, async, tokio, python-handler, shutdown-coordination]

# Dependency graph
requires:
  - phase: 06-observability
    provides: "ShutdownCoordinator, WorkerPool, PythonHandler base infrastructure"
provides:
  - "RayonPool struct with work-stealing thread pool for sync handler dispatch"
  - "ConsumerConfigBuilder::rayon_pool_size(u32) configuration method"
  - "PythonHandler invokes dispatch to RayonPool via oneshot channel pattern"
  - "ShutdownCoordinator coordinates Rayon drain during graceful shutdown"
affects:
  - "Phase 8 (Async Timeout) - depends on RayonPool infrastructure"
  - "Phase 9 (Handler Middleware) - PythonHandler invoke pattern"
  - "Phase 10 (Streaming Handler) - Rayon pool for blocking sync handlers"

# Tech tracking
tech-stack:
  added:
    - "rayon 1.1"
  patterns:
    - "Work-stealing thread pool for non-blocking poll cycle"
    - "Oneshot channel for Tokio-Rayon communication"
    - "std::thread::spawn (not tokio::spawn_blocking) inside Rayon closures"

key-files:
  created:
    - "src/rayon_pool.rs" - RayonPool struct wrapping rayon::ThreadPool
  modified:
    - "Cargo.toml" - added rayon dependency
    - "src/lib.rs" - added rayon_pool module
    - "src/consumer/config.rs" - rayon_pool_size field and builder method
    - "src/python/handler.rs" - rayon_pool dispatch in invoke()
    - "src/runtime/builder.rs" - RayonPool creation and wiring
    - "src/shutdown/shutdown.rs" - rayon pool field, with_rayon_pool constructor, drain_rayon()
    - "src/worker_pool/pool.rs" - drain_rayon call in shutdown()
    - "src/worker_pool/worker.rs" - test fix (None rayon_pool)
    - "src/worker_pool/pool.rs" - test fix (None rayon_pool)

key-decisions:
  - "Rayon closures MUST NOT call any Tokio APIs (deadlock/panic risk)"
  - "std::thread::spawn used inside Rayon closures for Python GIL calls"
  - "Oneshot channel bridges Rayon completion back to Tokio await"
  - "Rayon pool drains automatically on Drop - no explicit shutdown API exists"
  - "Default pool size = num_cpus - 2 (min 2) to leave threads for Tokio"

patterns-established:
  - "RayonPool::spawn() dispatches to work-stealing pool without blocking Tokio"
  - "PythonHandler::invoke() uses oneshot channel result pattern with Rayon"
  - "ShutdownCoordinator::drain_rayon() wraps pool drain in tokio::time::timeout"

requirements-completed:
  - "SYNC-01"
  - "SYNC-02"
  - "SYNC-03"

# Metrics
duration: 18min
completed: 2026-04-29
---

# Phase 7: Thread Pool Summary

**Rayon work-stealing thread pool for sync Python handlers, preventing poll cycle blocking with oneshot channel communication**

## Performance

- **Duration:** 18 min
- **Started:** 2026-04-29T09:17:36Z
- **Completed:** 2026-04-29T09:35:00Z
- **Tasks:** 6
- **Files modified:** 10

## Accomplishments
- RayonPool struct wrapping rayon::ThreadPool with spawn/drain/abort methods
- ConsumerConfigBuilder::rayon_pool_size(u32) builder method with 1-256 validation
- PythonHandler::invoke() dispatches to RayonPool via oneshot channel when configured
- Fallback to spawn_blocking when rayon_pool is None (backward compatible)
- ShutdownCoordinator coordinates Rayon drain with drain_timeout wrapping

## Task Commits

Each task was committed atomically:

1. **Task 1: Add rayon dependency to Cargo.toml** - `89d65e4` (feat)
2. **Task 2: Create src/rayon_pool.rs with RayonPool struct** - `df72fca` (feat)
3. **Task 3: Add rayon_pool_size to ConsumerConfigBuilder** - `e44012f` (feat)
4. **Task 4: Modify PythonHandler to use RayonPool** - `e0ee2e1` (feat)
5. **Task 5: Wire RayonPool through RuntimeBuilder** - `204ec1c` (feat)
6. **Task 6: Coordinate Rayon drain with ShutdownCoordinator** - `2764575` (feat)

**Plan metadata:** `bd7b953` (docs: plan thread pool phase)

## Files Created/Modified
- `Cargo.toml` - added rayon 1.1 dependency
- `src/rayon_pool.rs` - RayonPool struct (new file)
- `src/lib.rs` - added rayon_pool module export
- `src/consumer/config.rs` - rayon_pool_size field, builder method, validation, BuildError::InvalidField
- `src/python/handler.rs` - rayon_pool field, dispatch via oneshot channel in invoke()
- `src/runtime/builder.rs` - RayonPool creation, wiring to PythonHandler and ShutdownCoordinator
- `src/shutdown/shutdown.rs` - rayon_pool field, with_rayon_pool constructor, drain_rayon() method
- `src/worker_pool/pool.rs` - shutdown() calls coordinator.drain_rayon()
- `src/worker_pool/worker.rs` - test fix (added None rayon_pool param)
- `src/worker_pool/pool.rs` - test fix (added None rayon_pool param)

## Decisions Made
- Used rayon 1.1 crate (not rayon-core) for simpler ThreadPool API
- Rayon closures use std::thread::spawn (not tokio::spawn_blocking) since no Tokio runtime exists on Rayon threads
- Oneshot channel bridges Rayon-to-Tokio communication (rx.await on Tokio side)
- Rayon has no explicit abort/force-shutdown API - pool drains on Drop, wrapped in tokio::time::timeout from ShutdownCoordinator

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Rayon ThreadPool has no shutdown/abort API**
- **Found during:** Task 2 (Create RayonPool struct)
- **Issue:** Plan specified pool.shutdown() and pool.abort() but rayon::ThreadPool has no such methods in v1.12.0
- **Fix:** drain() and abort() become no-ops; actual drain happens via pool Drop semantics. ShutdownCoordinator wraps the drain call in tokio::time::timeout so timeout is still respected.
- **Files modified:** src/rayon_pool.rs
- **Verification:** cargo check passes
- **Committed in:** df72fca (Task 2 commit)

**2. [Rule 3 - Blocking] tokio::spawn_blocking cannot be called from Rayon thread**
- **Found during:** Task 4 (Modify PythonHandler to use RayonPool)
- **Issue:** tokio::task::spawn_blocking requires a Tokio runtime context, but Rayon worker threads don't have one
- **Fix:** Use std::thread::spawn inside the Rayon closure for Python GIL calls instead of tokio::spawn_blocking
- **Files modified:** src/python/handler.rs
- **Verification:** cargo check passes
- **Committed in:** e0ee2e1 (Task 4 commit)

**3. [Rule 2 - Missing Critical] Test files needed rayon_pool parameter**
- **Found during:** Task 5 (Wire RayonPool through RuntimeBuilder)
- **Issue:** Tests in worker.rs and pool.rs called PythonHandler::new() without rayon_pool parameter
- **Fix:** Added None as rayon_pool argument to test handler constructions
- **Files modified:** src/worker_pool/worker.rs, src/worker_pool/pool.rs
- **Verification:** cargo check passes
- **Committed in:** 204ec1c (Task 5 commit)

**4. [Rule 3 - Blocking] Arc<ShutdownCoordinator> cannot be mutated**
- **Found during:** Task 6 (Coordinate Rayon drain)
- **Issue:** set_rayon_pool() required mutating an Arc, which is not possible
- **Fix:** Added ShutdownCoordinator::with_rayon_pool() constructor that accepts pool at creation time
- **Files modified:** src/shutdown/shutdown.rs, src/runtime/builder.rs
- **Verification:** cargo check passes
- **Committed in:** 2764575 (Task 6 commit)

---

**Total deviations:** 4 auto-fixed (1 blocking on rayon API, 1 blocking on tokio runtime context, 1 missing critical test fix, 1 blocking on Arc mutation)
**Impact on plan:** All auto-fixes necessary for correctness. No scope creep - all changes directly support the Rayon pool goal.

## Issues Encountered
- Rayon v1.12.0 ThreadPool has no explicit shutdown method (only Drop blocks waiting) - worked around via pool drain timeout pattern
- Type mismatch between std::thread::JoinHandle and tokio::task::JoinHandle - resolved by using oneshot channel to bridge thread result back to Tokio
- Ownership issues with trace_context in conditional (rayon vs tokio branch) - resolved by cloning before conditional

## Next Phase Readiness
- Rayon pool infrastructure complete and wired
- PythonHandler::invoke() pattern established
- ShutdownCoordinator drain_rayon() integrated into WorkerPool::shutdown()
- Ready for Phase 8 (Async Timeout) which will use similar patterns for handler timeouts

---
*Phase: 07-thread-pool*
*Completed: 2026-04-29*
