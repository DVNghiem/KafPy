# Phase 34: Rebalance Handling - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-19
**Phase:** 34-rebalance-handling
**Areas discussed:** PartitionOwnership states, RebalanceEvent channel, Dispatcher pause/resume, record_ack guard, flush_pending_retries

## Ownership + Guard (RB-01, RB-04)

| Option | Description | Selected |
|--------|-------------|----------|
| States: Owned/Revoked only | Simple, no intermediate state | |
| States: Owned/Revoked/PendingAssign | Handles assignment transition window | ✓ |
| Per-message ownership check | Every ack checked against PartitionOwnership | ✓ |
| Per-partition boolean cache | Cache is_owned per partition, invalidate on revoke | |

**User's choice:** 3 states (Owned/Revoked/PendingAssign), per-message check
**Notes:** PendingAssign handles the window between broker notification and fully owning the partition

## RebalanceEvent Channel (RB-02, RB-03)

| Option | Description | Selected |
|--------|-------------|----------|
| Broadcast channel | Single producer, multiple consumers | ✓ |
| Watch channel | Single consumer only | |
| Polling assignment | Periodically compare assignment() snapshots | |

**User's choice:** Broadcast channel + ConsumerRunner assignment comparison in run() loop
**Notes:** Matches existing research from ARCHITECTURE.md

## flush_pending_retries (RB-05)

| Option | Description | Selected |
|--------|-------------|----------|
| Same as flush_failed_to_dlq | Reuse existing OffsetCoordinator method | |
| Separate RetryCoordinator method | New flush_pending_retries() on RetryCoordinator | ✓ |

**User's choice:** New `flush_pending_retries()` on RetryCoordinator
**Notes:** Separate from `flush_failed_to_dlq` which handles committed-but-failed offsets

## Deferred Ideas

- Python-visible rebalance callbacks (v1.9 opt-in)
- Incremental rebalance (KIP-848, Kafka 4.0+)

---
*Discussion logged: 2026-04-19*
