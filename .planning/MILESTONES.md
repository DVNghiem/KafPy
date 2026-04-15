# Milestones

## v1.0 — Core Consumer Refactor

**Completed:** 2026-04-15

**Goal:** Consolidate duplicate consumer/message code into `src/consumer/` with clean Python-integration boundary.

**What shipped:**
- `src/consumer/` module: `ConsumerConfigBuilder`, `OwnedMessage`, `ConsumerRunner`, `ConsumerStream`, `ConsumerTask`
- Deleted duplicate `src/consume.rs` and `src/message_processor.rs`
- Updated `src/pyconsumer.rs` as PyO3 bridge to pure-Rust consumer
- Fixed all PyO3 compilation errors (GIL API, `c""` literals, `StreamExt` imports)

---

*Last updated: 2026-04-15*