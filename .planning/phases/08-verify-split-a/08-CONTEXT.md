# Phase 08: Verify SPLIT-A Requirements - Context

**Gathered:** 2026-04-25
**Status:** Ready for planning
**Source:** Gap closure from v2.0 MILESTONE-AUDIT.md

<domain>
## Phase Boundary

Phase 08 (verify-split-a) verifies that SPLIT-A requirements (SPLIT-A-01 through SPLIT-A-06) from Phase 02 (split-worker-pool) are present in the codebase and produces a VERIFICATION.md.

</domain>

<decisions>
## Implementation Decisions

| REQ | Requirement | Expected Location | Status |
|-----|-------------|------------------|--------|
| SPLIT-A-01 | PartitionAccumulator from mod.rs to batch/ module | src/worker_pool/accumulator.rs | Verify present |
| SPLIT-A-02 | batch_worker_loop() to batch.rs | src/worker_pool/batch_loop.rs | Verify present |
| SPLIT-A-03 | worker_loop() to worker.rs | src/worker_pool/worker.rs | Verify present |
| SPLIT-A-04 | WorkerPool struct to pool.rs | src/worker_pool/pool.rs | Verify present |
| SPLIT-A-05 | Rename PartitionAccumulator to PerPartitionBuffer | src/worker_pool/accumulator.rs | Verify renamed |
| SPLIT-A-06 | Move BatchAccumulator to python/batch.rs | src/python/batch.rs | Verify present |

</decisions>

<canonical_refs>
## Canonical References

- `.planning/phases/02-split-worker-pool/02-01-SUMMARY.md` — SPLIT-A-01,05,06 claims
- `.planning/phases/02-split-worker-pool/02-02-SUMMARY.md` — SPLIT-A-02,03,04 claims
- `.planning/REQUIREMENTS.md` — SPLIT-A-01 through SPLIT-A-06
</canonical_refs>

