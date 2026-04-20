# Phase 36: Error Handling - Discussion Log

> **Audit trail only.** Decisions are captured in CONTEXT.md.

**Date:** 2026-04-20
**Phase:** 36-error-handling
**Areas discussed:** Exception class structure, Rust error translation, KafkaMessage access errors, Error message format

---

## Exception Class Structure

| Option | Description | Selected |
|--------|-------------|----------|
| Message only | Just the error message string — keep it simple | |
| Attributes on subclasses | Subclasses carry relevant structured data (partition, topic, error_code) | ✓ |

**User's choice:** Attributes on subclasses (structured data)
**Notes:** User said "follow big tech suggest for me" — big tech Python Kafka clients use structured exception attributes

---

## Rust Error Translation

| Option | Description | Selected |
|--------|-------------|----------|
| Error type + message only | Map Rust error type → Python exception class with a formatted message | |
| Include error code + source | Preserve Kafka error code, source module, and formatted message | ✓ |

**User's choice:** Include error code + source
**Notes:** User said "follow big tech and suggest for me" — big tech preserves Kafka error codes across boundaries

---

## KafkaMessage Access Errors

| Option | Description | Selected |
|--------|-------------|----------|
| Raise HandlerError | Accessing a None key raises HandlerError — handler must check .key first | |
| Return None silently | Return None — caller checks has_key() first (not implemented) | ✓ |

**User's choice:** Return None silently
**Notes:** User said "follow big tech and suggest for me" — confluent-kafka-python returns None for None keys

---

## Error Message Format

| Option | Description | Selected |
|--------|-------------|----------|
| Structured strings | Human-readable strings with context: 'Consumer error: NOT_LEADER (error 6) on topic-0@partition 3' | ✓ |
| Pure strings | Simple string messages only — no structured parsing needed | |

**User's choice:** Structured strings
**Notes:** Human-readable with Kafka error code, error name, topic, and partition

---

## Deferred Ideas

None
