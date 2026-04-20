---
phase: "06-type-safety"
plan: "01"
subsystem: "routing"
tags: ["type-safety", "handlerid", "error-handling", "send-sync"]
dependency_graph:
  requires: []
  provides:
    - "HandlerId newtype in routing/context.rs"
    - "Unified error re-exports in src/error.rs"
    - "Send+Sync compile-time assertions"
  affects:
    - "routing/decision.rs"
    - "routing/key.rs"
    - "routing/chain.rs"
    - "routing/header.rs"
    - "routing/topic_pattern.rs"
    - "routing/python_router.rs"
    - "dispatcher/queue_manager.rs"
    - "dispatcher/consumer_dispatcher.rs"
tech_stack:
  added:
    - "HandlerId newtype wrapper struct"
    - "src/error.rs unified re-export module"
    - "Send+Sync assertion tests"
  patterns:
    - "Newtype pattern for type safety"
    - "Sentinel value for unset HandlerId"
key_files:
  created:
    - "src/error.rs"
  modified:
    - "src/routing/context.rs"
    - "src/routing/decision.rs"
    - "src/routing/key.rs"
    - "src/routing/chain.rs"
    - "src/routing/header.rs"
    - "src/routing/topic_pattern.rs"
    - "src/routing/python_router.rs"
    - "src/dispatcher/queue_manager.rs"
    - "src/dispatcher/consumer_dispatcher.rs"
    - "src/lib.rs"
decisions:
  - "HandlerId is a newtype wrapper (struct HandlerId(String)) to prevent accidental interchange with topic names"
  - "Sentinel value '__UNSET__' used for unset default_handler in RoutingChain"
  - "HashMap<String, HandlerEntry> in QueueManager unchanged - HandlerId converted via as_str() at API boundary"
  - "All test assertions updated to use HandlerId::new() and .as_str() for comparisons"
metrics:
  duration: "2026-04-20T15:31:13Z - 2026-04-20T16:00:00Z"
  completed: "2026-04-20"
---

# Phase 6 Plan 1: Type Safety Summary

**One-liner:** HandlerId newtype prevents routing/topic conflation; unified error module improves discoverability

## Completed Tasks

| # | Name | Commit | Files |
|---|------|--------|-------|
| 1 | HandlerId newtype in context.rs | 2999aa5 | src/routing/context.rs |
| 2 | Route variant doc comment | 2999aa5 | src/routing/decision.rs |
| 3 | Update key.rs tests | 2999aa5 | src/routing/key.rs |
| 4 | Create src/error.rs | 2999aa5 | src/error.rs |
| 5 | Fix queue_manager and consumer_dispatcher | 2999aa5 | src/dispatcher/queue_manager.rs, src/dispatcher/consumer_dispatcher.rs |
| 6 | Send+Sync assertions | 2999aa5 | src/lib.rs |
| 7 | Fix all remaining test assertions | 2999aa5 | src/routing/chain.rs, src/routing/header.rs, src/routing/topic_pattern.rs, src/routing/python_router.rs |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Missing Display and is_empty on HandlerId**
- **Found during:** Task 1 - compile after newtype implementation
- **Issue:** HandlerId lacked `Display` and `is_empty` needed by existing code
- **Fix:** Added `impl Display for HandlerId` and `is_empty()` method
- **Files modified:** src/routing/context.rs
- **Commit:** 2999aa5

**2. [Rule 1 - Bug] Test assertions comparing HandlerId to string literals**
- **Found during:** Task 3 - running tests after newtype
- **Issue:** Multiple test files had assertions like `if id == "handler1"` which no longer worked with newtype
- **Fix:** Updated all assertions to use `id.as_str() == "handler1"`
- **Files modified:** src/routing/chain.rs, src/routing/header.rs, src/routing/topic_pattern.rs, src/routing/python_router.rs
- **Commit:** 2999aa5

**3. [Rule 3 - Blocking] QueueManager HashMap lookup requires String key**
- **Found during:** Task 5 - compile check
- **Issue:** QueueManager uses `HashMap<String, HandlerEntry>` but we were passing `&HandlerId` to `.get()`
- **Fix:** Changed to `guard.get(handler_id.as_str())` using the `Borrow<str>` impl
- **Files modified:** src/dispatcher/queue_manager.rs, src/dispatcher/consumer_dispatcher.rs
- **Commit:** 2999aa5

**4. [Rule 1 - Bug] RoutingChain sentinel value**
- **Found during:** Task 6 - compile check
- **Issue:** `HandlerId::new()` no longer takes 0 arguments - needed a sentinel for unset default_handler
- **Fix:** Changed to `HandlerId::new("__UNSET__")` sentinel value
- **Files modified:** src/routing/chain.rs
- **Commit:** 2999aa5

## Known Stubs

None - all stubs resolved.

## Threat Flags

None - type safety refactor does not introduce new threat surface.

## Verification

```bash
cargo build --lib  # PASSES
cargo check --lib  # PASSES
# Note: test linking fails due to pre-existing Python dev environment issue (undefined Python symbols)
```

## Self-Check

- [x] HandlerId is a newtype struct, not a type alias
- [x] src/error.rs re-exports: DispatchError, ConsumerError, CoordinatorError, PyError
- [x] Borrow<str> implemented for HandlerId
- [x] All Send+Sync assertions present in lib.rs
- [x] All existing tests updated (handler_id.is_empty() -> sentinel, string comparisons -> as_str())

## TDD Gate Compliance

Not a TDD plan - refactor-only implementation.
