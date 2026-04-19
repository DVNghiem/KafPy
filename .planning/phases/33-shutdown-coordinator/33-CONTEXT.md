# Phase 33: ShutdownCoordinator - Context

**Gathered:** 2026-04-19
**Status:** Ready for planning
**Source:** v1.8 milestone goals + research

## Phase Boundary

Create ShutdownCoordinator with 3-phase shutdown sequence, drain timeout, and correct component shutdown order.

## Implementation Decisions

### LSC-01: ShutdownPhase enum
- States: Running, Draining, Finalizing, Done
- Explicit transitions only (no boolean soup)

### LSC-02: Consumer::stop() → ShutdownCoordinator
- ShutdownCoordinator owns shutdown orchestration
- Consumer::stop() signals coordinator, not direct component stop

### LSC-03: Shutdown order (CRITICAL - prevents deadlock)
1. Dispatcher stop first (stop new dispatches)
2. WorkerPool drain (wait with timeout)
3. OffsetCommitter finalize (commit safe offsets)
4. Components close in reverse dependency order

### LSC-04: Drain timeout
- Default 30s, configurable
- Force-abort fallback after timeout
- Structured log warning on timeout

### LSC-05: rd_kafka_consumer_close()
- Must be called on ConsumerRunner drop
- Leaves consumer group cleanly (no sticky rebalance)

## Deferred Ideas
- Per-handler drain timeout (v1.9)
