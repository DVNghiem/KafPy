# Phase 36: Error Handling - Context

**Gathered:** 2026-04-20
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 36 delivers: Python exception hierarchy (KafPyError base), Rust-to-Python error translation at PyO3 boundary, meaningful exception messages, and HandlerError for KafkaMessage access errors.
</domain>

<decisions>
## Implementation Decisions

### Exception Class Structure (ERR-01, ERR-02)
- **D-01:** Exception classes inherit from KafPyError, not Exception directly
- **D-02:** Subclasses carry structured attributes: error_code (Kafka error code), partition, topic where relevant
- **D-03:** Class hierarchy: KafPyError → ConsumerError, HandlerError, ConfigurationError
- **D-04:** All exceptions exported from kafpy.exceptions only (ERR-05)

### Rust Error Translation (ERR-03)
- **D-05:** PyO3 boundary catches Rust errors and maps to Python exceptions
- **D-06:** Context preserved: Kafka error code + source module + formatted message
- **D-07:** Each Rust error type maps to specific Python exception (KafkaError → ConsumerError, etc.)

### KafkaMessage Access Errors (ERR-04)
- **D-08:** Accessing .key when None returns None silently — no exception raised
- **D-09:** Wrong-type access (e.g., accessing wrong type field) raises HandlerError

### Error Message Format
- **D-10:** Human-readable structured strings: "Consumer error: NOT_LEADER (error 6) on topic-0@partition 3"
- **D-11:** Messages include Kafka error code, error name, topic, and partition where applicable

### Deferred Ideas
None
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Context
- `.planning/phases/35-handler-registration-runtime/35-CONTEXT.md` — Phase 35 handler stubs (HandlerContext, HandlerResult)
- `.planning/REQUIREMENTS.md` § ERR-01 through ERR-05 — Error handling requirements for Phase 36
- `kafpy/__init__.py` — Current public API imports (no exceptions module yet)

### No external specs — requirements fully captured in decisions above
</canonical_refs>

<codebase_context>
## Existing Code Insights

### Reusable Assets
- No existing exceptions.py — will be created in Phase 36
- _kafpy module may have Rust errors to map

### Established Patterns
- Frozen dataclasses from Phase 34/35: use same pattern for exception classes if needed
- Python exception hierarchy follows standard Python conventions (Exception as base)

### Integration Points
- kafpy/exceptions.py will be imported by kafpy/__init__.py
- Exceptions used in kafpy/handlers.py, kafpy/consumer.py, kafpy/runtime.py
</codebase_context>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches.
</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.
</deferred>
