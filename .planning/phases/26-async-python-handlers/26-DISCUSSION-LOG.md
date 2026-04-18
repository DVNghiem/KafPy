# Phase 26: Async Python Handlers - Discussion Log

> **Audit trail only.** Do not use as input to planning or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-18
**Phase:** 26-async-python-handlers
**Areas discussed:** Coroutine detection, GIL release strategy, Async error handling

---

## Area 1: Coroutine Detection

| Option | Description | Selected |
|--------|-------------|----------|
| inspect.iscoroutinefunction() | Clean Python API; detects decorated async def | |
| __code__.co_flags & CO_COROUTINE | Direct flag check; precise but won't detect through wrappers | |
| Dual-check | iscoroutinefunction() first, flag fallback; most robust | |
| **Cached at registration** | **Detect once, cache mode on PythonHandler, no re-inspection per call** | ✓ |

**User's choice:** Check and cache for optimize performance
**Notes:** Detection at registration time, result cached on PythonHandler. No per-call re-inspection overhead.

---

## Area 2: GIL Release Strategy

| Option | Description | Selected |
|--------|-------------|----------|
| Trust pyo3-async-runtimes | into_future releases GIL at every await; documented invariant | |
| Runtime assertions | Add assertions GIL is not held at Rust await points | |
| **Custom CFFI bridge** | **Python C API directly; PyGILState_Ensure/Release; custom Future; explicit control** | ✓ |

**User's choice:** Implement from scratch and use CFFI Python API
**Notes:** Custom bridge instead of pyo3-async-runtimes. GIL acquired only during polling, released at every await. Aligns with rdkafka-lite CFFI usage.

---

## Area 3: Async Error Handling

| Option | Description | Selected |
|--------|-------------|----------|
| **Try/catch in poll (Recommended)** | Python exceptions caught inside Future::poll; returned as ExecutionResult::Error | ✓ |
| Result via Python exception | Exception raised through PyErr; caught at registration boundary | |

**User's choice:** Try/catch in poll (Recommended)
**Notes:** Exceptions caught inside Future::poll, surfaced as ExecutionResult::Error / BatchExecutionResult::AllFailure. Keeps Rust error handling clean.

---

*Phase: 26-async-python-handlers*
*Discussion complete: 2026-04-18*
