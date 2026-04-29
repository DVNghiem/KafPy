# KafPy Roadmap

**Project:** Rust-Core, Python-Logic Kafka Consumer Framework

---

## Milestones

- ✅ **v1.0 MVP** — Phases 1-6 (shipped 2026-04-29)
- ✅ **v1.1 Async & Concurrency Hardening** — Phases 7-10 (shipped 2026-04-29)
- ⬜ **v2.0** — Fan-out, Fan-in (future)

## Phases

<details>
<summary>✅ v1.1 (Phases 7-10) — SHIPPED 2026-04-29</summary>

- [x] Phase 7: Thread Pool (1/1 plans) — completed 2026-04-29
- [x] Phase 8: Async Timeout (2/2 plans) — completed 2026-04-29
- [x] Phase 9: Handler Middleware (3/3 plans) — completed 2026-04-29
- [x] Phase 10: Streaming Handler (4/4 plans) — completed 2026-04-29

</details>

### ⬜ v2.0 (Future)

- [ ] Phase 11: Fan-Out — One message triggers multiple async handlers/sinks
- [ ] Phase 12: Fan-In — Multiple async sources merged into single handler

---

## Phase Dependencies (v1.1)

```
Phase 7 (Thread Pool)
  └── Phase 8 (Async Timeout)
        └── Phase 9 (Handler Middleware)
              └── Phase 10 (Streaming Handler)
                    └── Phase 11 (Fan-Out) [v2.0]
                          └── Phase 12 (Fan-In) [v2.0]
```

---

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1 | v1.0 | 1/1 | Complete | brownfield |
| 2 | v1.0 | 7/7 | Complete | 2026-04-29 |
| 3 | v1.0 | 1/1 | Complete | 2026-04-29 |
| 4 | v1.0 | 2/2 | Complete | 2026-04-29 |
| 5 | v1.0 | 2/2 | Complete | 2026-04-29 |
| 6 | v1.0 | 2/2 | Complete | 2026-04-29 |
| 7 | v1.1 | 1/1 | Complete | 2026-04-29 |
| 8 | v1.1 | 2/2 | Complete | 2026-04-29 |
| 9 | v1.1 | 3/3 | Complete | 2026-04-29 |
| 10 | v1.1 | 4/4 | Complete | 2026-04-29 |
| 11 | v2.0 | 0/3 | Planned | - |
| 12 | v2.0 | 0/3 | Planned | - |

**Full milestone history at `.planning/milestones/`**

---
*Last updated: 2026-04-29 after v1.1 milestone shipped*