# Phase 13 Plan 01: Fan-Out Python API Summary

**Plan:** 13-fan-out-python-api-01
**Status:** COMPLETE
**Completed:** 2026-05-01

## Objective

Build the Python API for fan-out registration (FANOUT-06), exposing `consumer.register_fanout()` that returns a `FanOutBuilder` with `.max_fan_out(n).register()` chain, producing a `FanOutRegistration` with `fan_out_id`.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Create FanOutBuilderRust in src/python/fan_out_bridge.rs | See git | src/python/fan_out_bridge.rs, src/python/mod.rs |
| 2 | Add register_fanout to PyConsumer and Consumer Python class | See git | src/pyconsumer.rs, kafpy/consumer.py, kafpy/__init__.py |
| 3 | Create Python FanOutBuilder class (kafpy/fanout.py) | See git | kafpy/fanout.py, kafpy/__init__.py |

## Key Files Created/Modified

| File | Change |
|------|--------|
| `src/python/fan_out_bridge.rs` | New — FanOutBuilderRust with global atomic counter for fan_out_id |
| `src/python/mod.rs` | Added `pub mod fan_out_bridge;` |
| `src/pyconsumer.rs` | Added `fan_out_handlers` map, `add_handler_with_fan_out()`, `get_fan_out_handlers()`, `FanOutRegistration` pyclass, `register_fanout()` method, updated `HandlerMetadata` with `fan_out_config` |
| `src/runtime/builder.rs` | Added `fan_out_handlers` parameter to RuntimeBuilder; use pre-built handlers for fan-out sink topics in handler_map construction |
| `kafpy/fanout.py` | New — Python FanOutBuilder and FanOutRegistration dataclasses |
| `kafpy/consumer.py` | Added `register_fanout()` method returning FanOutBuilder |
| `kafpy/__init__.py` | Exported FanOutBuilder, FanOutRegistration |

## Implementation Decisions

- **D-03:** `fan_out_id` generated as `u64` via atomic counter (not UUID) — simpler, unique enough for observability
- **D-06:** Fan-out handlers stored in separate `fan_out_handlers` map in PyConsumer, passed to RuntimeBuilder for handler_map assembly
- Handler metadata uses `PyNone` as marker callback for fan-out sink topics

## Verification

```bash
python -c "from kafpy import FanOutBuilder, FanOutRegistration; print('ok')"
# Output: ok

python -c "
from kafpy import Consumer, FanOutBuilder, FanOutRegistration
from kafpy.config import ConsumerConfig

def my_handler(msg, ctx): pass

config = ConsumerConfig(bootstrap_servers='localhost:9092', group_id='test', topics=[])
consumer = Consumer(config)
builder = consumer.register_fanout('group1', ['topic-a', 'topic-b'], my_handler)
result = builder.max_fan_out(8).register()
print('group_name:', result.group_name)
print('fan_out_id:', result.fan_out_id)
print('sink_topics:', result.sink_topics)
"
# Output: group_name: group1, fan_out_id: 1, sink_topics: ['topic-a', 'topic-b']
```

## Deviations from Plan

None — plan executed as written.

## Threat Flags

None.

## Dependencies

- Phase 11: FanOutConfig, FanOutTracker, SinkConfig
- Phase 12: FanOutTracker::wait_all(), per-sink timeout
- Phase 10: inject_trace_context

## Next Steps

- Phase 13-02: Fan-out metrics with `fan_out_branch_duration_seconds` and `fan_out_branch_total`
- Phase 13-03: Trace context branching per fan-out branch
