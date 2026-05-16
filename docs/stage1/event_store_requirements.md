# Event Store Requirements

Source: harness_architecture_book_v0.7.4.1-canonical §7.2, docs/stage0/retrospectives/ Issue #8

## Overview

The Event Store is the append-only persistence layer for all events (project-level and task-level). It enforces write integrity and provides replay validation.

## Hard Requirements

### 1. Append-Only Atomic Write

Each write operation must emit exactly one complete JSON object followed by `\n`.

- No concatenation of multiple JSON objects on a single line
- No partial writes (either the full JSON + newline is written, or nothing is)
- Write failure must not leave partial data in the store

### 2. Newline Enforcement

Every line MUST end with `\n`.

- The Event Store MUST reject writes that do not terminate with a newline
- A JSON object without a trailing newline is an error, not a valid incomplete write

### 3. One JSON Object Per Line (JSONL Format)

Each line in the store must contain exactly one JSON object.

- Lines containing multiple concatenated JSON objects MUST be rejected
- Lines containing non-JSON content MUST be rejected

### 4. JSONL Line Validator

Each line must be valid JSON conforming to event.v1 schema.

Required fields:
- `event_id` (string, format: `evt_YYYYMMDD_NNNNNN`)
- `schema_version` (string, must be `"event.v1"`)
- `event_type` (string, must be in registered event type enum)
- `timestamp` (string, ISO-8601 format)
- `producer` (object: `component_id`, `component_type`)
- `correlation` (object: `batch_id`, `task_id` or `project_id`, `node_id`, `run_id`)
- `severity` (string: `"info"` | `"warn"` | `"error"`)
- `payload` (object)
- `idempotency_key` (string)
- `parent_event_id` (string or null)

### 5. event_id Uniqueness Check

- `event_id` MUST be globally unique within a single Event Store instance
- Before appending, the store MUST check that the `event_id` does not already exist
- Duplicate `event_id` MUST be rejected regardless of payload content

### 6. idempotency_key Rules

`idempotency_key` is separate from `event_id` and supports idempotent retries:

- If `idempotency_key` already exists AND payload hash is identical to the existing event → treat as duplicate no-op (accept silently, do not append a new line)
- If `idempotency_key` already exists BUT payload hash differs → reject with `IdempotencyConflictError`
- If `idempotency_key` is new → accept and append

This allows safe retry of the same logical event without creating duplicates, while catching conflicting events that reuse the same key.

### 7. Timestamp Rules

- `timestamp` must be monotonic non-decreasing within a stream where ordering is required
- Equal timestamps are allowed (events may occur in the same instant)
- Final event sequence is determined by append order, not timestamp order
- A timestamp that is earlier than the previous event's timestamp SHOULD produce a warning, not a hard rejection (clock skew is possible)

### 8. Replay Preflight Check

Before projection replay, validate the entire event stream for:

1. **Valid JSON per line**: every line parses as valid JSON
2. **No duplicate event_ids**: each `event_id` appears at most once
3. **Monotonic non-decreasing timestamps**: no timestamp regression (warning level)
4. **No missing parent_event_ids**: if `parent_event_id` is set, the referenced event must exist in the stream
5. **No line concatenation**: each line contains exactly one JSON object
6. **Newline terminated**: every line ends with `\n`
7. **schema_version = event.v1**: every event has the correct schema version

### 9. Stage 0 Line 17 Detection

The Event Store MUST be able to detect the known issues in `docs/stage0/events.jsonl` line 17:

**Issue A — Line concatenation:**
Line 17 contains two JSON objects concatenated without a newline separator:
```
{...}"event_id":"evt_20260515_000030",...}{..."event_id":"evt_20260515_000030",...}
```
Detection: JSONL line validator fails — the concatenated string is not valid JSON.

**Issue B — Duplicate event_id:**
Both objects on line 17 have `event_id: "evt_20260515_000030"`.
Detection: after splitting (if possible), event_id uniqueness check rejects the duplicate.

**Expected detection output:**
```
Line 17: INVALID — not valid JSON (two JSON objects concatenated without newline separator)
Duplicate event_id: evt_20260515_000030 (appears on line 17 twice)
Replay preflight: FAILED — 2 issues found
```

## Error Types

| Error | Condition | Severity |
|-------|-----------|----------|
| `MissingNewlineError` | Write without trailing `\n` | reject |
| `InvalidJsonError` | Line is not valid JSON | reject |
| `DuplicateEventIdError` | `event_id` already exists | reject |
| `IdempotencyConflictError` | `idempotency_key` duplicate with different payload | reject |
| `SchemaViolationError` | Missing required fields or wrong `schema_version` | reject |
| `TimestampRegressionWarning` | Timestamp earlier than previous event | warning |
| `MissingParentEventError` | `parent_event_id` references non-existent event | warning |
| `LineConcatenationError` | Multiple JSON objects on one line | reject |

## Interface

```
event_store.append(event: Event) → AppendResult
event_store.validate_line(line: string) → ValidationResult
event_store.check_uniqueness(event_id: string) → boolean
event_store.check_idempotency(idempotency_key: string, payload_hash: string) → IdempotencyResult
event_store.replay_preflight(stream: string[]) → PreflightResult
event_store.get_events(filter?: EventFilter) → Event[]
```

## Test Fixtures

| Fixture | Source | Purpose |
|---------|--------|---------|
| `stage0_project_events` | `docs/stage0/events.jsonl` (18 lines) | Replay validation, line 17 detection |
| `stage0_task_005_events` | `docs/stage0/tasks/task-005-failure-fix-loop/events.jsonl` (10 events) | Task-level event validation |
| `sanitized_stage0_events` | Derived from `docs/stage0/events.jsonl` with line 17 split and duplicate removed | Clean replay test (original file unchanged) |
