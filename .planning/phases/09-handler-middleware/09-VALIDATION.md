---
phase: "09"
slug: handler-middleware
date: "2026-04-29"
---

# Phase 09: Handler Middleware — Validation Strategy

## Test Dimensions

| # | Dimension | What to Verify | Method |
|---|-----------|----------------|--------|
| 1 | MIDW-01 HandlerMiddleware trait | Trait object-safe, hooks called in order | cargo check --lib + integration test |
| 2 | MIDW-01 MiddlewareChain composition | before() natural order, after()/on_error() reverse | cargo test -- middleware |
| 3 | MIDW-02 Logging middleware | Span events on handler start/complete/error | cargo test -- logging |
| 4 | MIDW-03 Metrics middleware | Latency histogram + throughput counter recorded | cargo test -- metrics |
| 5 | MIDW-04 Python API middleware param | add_handler accepts middleware=[...] and wires to PythonHandler | cargo test -- python_middleware |
| 6 | GIL boundary safety | Python middleware calls via spawn_blocking | cargo test -- gil |
| 7 | End-to-end middleware chain | Full flow: before → handler → after/on_error | cargo test -- middleware_e2e |
| 8 | VALIDATION.md exists | This file is present and tests are automated | grep VALIDATION.md |

## Automated Test Commands

```bash
# Dimension 1: HandlerMiddleware trait object-safe
cargo check --lib 2>&1 | tail -5

# Dimension 2: MiddlewareChain composition order
cargo test --lib -- middleware_chain 2>&1 | tail -10

# Dimension 3: Logging spans on handler lifecycle
cargo test --lib -- logging_middleware 2>&1 | tail -10

# Dimension 4: Metrics latency + throughput recorded
cargo test --lib -- metrics_middleware 2>&1 | tail -10

# Dimension 5: Python API middleware parameter
cargo test --lib -- python_middleware 2>&1 | tail -10

# Dimension 6: GIL boundary safety
cargo test --lib -- gil_boundary 2>&1 | tail -10

# Dimension 7: End-to-end middleware chain
cargo test --lib -- middleware_e2e 2>&1 | tail -10

# Dimension 8: VALIDATION.md present (this command)
test -f .planning/phases/09-handler-middleware/09-VALIDATION.md && echo "VALIDATION.md exists"
```

## Success Criteria

All 8 dimensions must return exit code 0 (tests pass or cargo check succeeds).

## Notes

- Dimensions 3 and 4 can be verified together (same middleware module)
- Dimension 8 is a meta-check confirming this file exists
- Python integration tests require `pip install -e .` in a Python env with PyO3 bindings

---

*Phase: 09-handler-middleware*
*Validation strategy: 2026-04-29*