# Roadmap: KafPy — v1.5 Extensible Routing

## Milestones

- **v1.0 Core Consumer Refactor** — Phases 1-5 (shipped 2026-04-15)
- **v1.1 Dispatcher Layer** — Phases 6-8 (shipped 2026-04-16)
- **v1.2 Python Execution Lane** — Phases 9-10 (shipped 2026-04-16)
- **v1.3 Offset Commit Coordinator** — Phases 11-16 (shipped 2026-04-17)
- **v1.4 Failure Handling & DLQ** — Phases 17-20 (shipped 2026-04-17)

## Phases

### v1.5 Extensible Routing (Planned)

- [ ] **Phase 21: Routing Core** — RoutingContext, RoutingDecision, Router trait, TopicPatternRouter, HeaderRouter, KeyRouter, RoutingChain, CONFIG-01, CONFIG-02
- [ ] **Phase 22: Python Integration** — PythonRouter with spawn_blocking, Py<PyAny> callback
- [ ] **Phase 23: Dispatcher Integration** — wire Router into ConsumerDispatcher, RoutingDecision handling (Drop→offset advance, Reject→DLQ, Defer→default)

## Phase Details

### Phase 21: Routing Core

**Goal**: Users can configure topic/header/key routing rules with zero-copy routing context, evaluated via a precedence chain in Rust.

**Depends on**: Nothing (first phase of v1.5)

**Requirements**: ROUTER-01, ROUTER-02, ROUTER-03, ROUTER-04, ROUTER-05, ROUTER-06, ROUTER-07, CONFIG-01, CONFIG-02

**Success Criteria** (what must be TRUE):
1. RoutingContext holds topic name, headers, key, and payload as zero-copy references (no heap allocation, no copies)
2. RoutingDecision enum has four variants: Route(handler_id), Drop, Reject(reason), Defer
3. Router trait has one method: `route(&self, ctx: &RoutingContext) -> RoutingDecision`
4. TopicPatternRouter matches topic names against glob/regex patterns
5. HeaderRouter matches by header key presence or value pattern
6. KeyRouter matches by message key prefix or exact match
7. RoutingChain evaluates routers in precedence order: pattern → header → key → python → default
8. ConsumerConfig exposes `routing_rules: Vec<RoutingRule>` with pattern/header/key config + target handler + priority

**Plans**: 3 plans in 2 waves

Plans:
- [ ] 21-01-PLAN.md — Core types: RoutingContext, RoutingDecision, HandlerId, Router trait
- [ ] 21-02-PLAN.md — Three routers: TopicPatternRouter, HeaderRouter, KeyRouter
- [ ] 21-03-PLAN.md — RoutingChain, RoutingRule config, ConsumerConfig integration

### Phase 22: Python Integration

**Goal**: Python code can optionally participate in routing, but only when all Rust routers return Defer.

**Depends on**: Phase 21

**Requirements**: PYROUTER-01, PYROUTER-02, PYROUTER-03

**Success Criteria** (what must be TRUE):
1. PythonRouter stores a `Py<PyAny>` callback callable that receives routing context
2. PythonRouter is called via spawn_blocking (GIL release during Python execution)
3. Python callback returns a RoutingDecision (via Py<PyAny>) — Drop/Reject/Defer or Route(handler_id)
4. PythonRouter is last in chain precedence — only invoked when all prior routers returned Defer

**Plans**: TBD

### Phase 23: Dispatcher Integration

**Goal**: Router is called before queue selection in ConsumerDispatcher, with RoutingDecision properly wired to offset commit, DLQ, and default handler paths.

**Depends on**: Phase 21, Phase 22

**Requirements**: DISPATCH-01, DISPATCH-02

**Success Criteria** (what must be TRUE):
1. ConsumerDispatcher accepts a Router on construction and calls it before queue selection
2. RoutingDecision::Drop causes offset advance only (no handler invocation, no DLQ)
3. RoutingDecision::Reject causes DLQ routing with failure metadata
4. RoutingDecision::Defer selects the default handler (backpressure + queue selection proceeds normally)
5. Routing precedence chain (pattern → header → key → python → default) is enforced end-to-end
6. No payload copies are introduced in the routing path
7. Backpressure integration with pause/resume remains functional

**Plans**: TBD

---

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 21. Routing Core | 0/3 | Not started | - |
| 22. Python Integration | 0/1 | Planned    |  |
| 23. Dispatcher Integration | 0/- | Not started | - |

---

## Coverage Map

| Requirement | Phase |
|-------------|-------|
| ROUTER-01 | Phase 21 |
| ROUTER-02 | Phase 21 |
| ROUTER-03 | Phase 21 |
| ROUTER-04 | Phase 21 |
| ROUTER-05 | Phase 21 |
| ROUTER-06 | Phase 21 |
| ROUTER-07 | Phase 21 |
| CONFIG-01 | Phase 21 |
| CONFIG-02 | Phase 21 |
| PYROUTER-01 | Phase 22 |
| PYROUTER-02 | Phase 22 |
| PYROUTER-03 | Phase 22 |
| DISPATCH-01 | Phase 23 |
| DISPATCH-02 | Phase 23 |

**Total:** 14/14 requirements mapped

---

*Last updated: 2026-04-17*
