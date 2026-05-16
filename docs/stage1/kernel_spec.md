# Harness Kernel Specification

Source: harness_architecture_book_v0.7.4.1-canonical §7.2, docs/stage0/retrospectives/ §4,#1

## Overview

The Harness Kernel is the core write interface for the Event Store. All state changes (project-level and task-level) must go through the Kernel's event append API.

## Event Append Interface

```
kernel.event_append(event: Event) → AppendResult
```

### Write Flow

```
1. Validate JSON structure (must be valid JSON object)
2. Validate schema (event.v1 required fields present)
3. Validate timestamp (ISO-8601, non-decreasing warning)
4. Check event_id uniqueness (reject if duplicate)
5. Check idempotency_key:
   a. If key exists + payload hash matches → accept as no-op (do not append)
   b. If key exists + payload hash differs → reject IdempotencyConflictError
   c. If key is new → proceed
6. Serialize to JSON string
7. Append trailing \n
8. Atomic write to store
9. Verify write (read-back check optional)
10. Return AppendResult
```

### AppendResult

```
AppendResult:
  status: "accepted" | "no_op" | "rejected"
  event_id: string
  line_number: int  (if accepted)
  error: ErrorDetail | null  (if rejected)
```

### Error Types

| Error | Condition | Action |
|-------|-----------|--------|
| `MissingNewlineError` | Internal: newline not appended | Should never surface (step 7 adds it) |
| `InvalidJsonError` | Input is not valid JSON | Reject |
| `DuplicateEventIdError` | `event_id` already in store | Reject |
| `IdempotencyConflictError` | `idempotency_key` duplicate, payload differs | Reject |
| `SchemaViolationError` | Missing required fields or wrong `schema_version` | Reject |
| `TimestampRegressionWarning` | Timestamp earlier than previous | Warn, accept |

## Project-Level Event Types

Minimum Stage 1 required (§7.2.1):

```
project_item_state_changed
project_to_queue_handoff_created
project_dependency_resolved
project_board_item_updated
```

Full recommended enum:

```
project_created
project_brief_updated
project_board_created
project_board_item_updated
project_item_state_changed
project_dependency_graph_created
project_dependency_graph_updated
project_dependency_resolved
project_to_queue_handoff_created
module_contract_created
module_contract_updated
test_case_pack_created
test_case_pack_updated
```

## Task-Level / Node-Level Event Types

```
task_state_changed
node_started
node_completed
node_failed
artifact_produced
advisor_requested
advisor_response_received
```

## Event Ownership Rules

| Event Type | Store Location | Producer |
|------------|---------------|----------|
| `project_item_state_changed` | Project-level store | Kernel / Final Gate |
| `project_to_queue_handoff_created` | Project-level store | Batch Intake |
| `project_dependency_resolved` | Project-level store | Dependency Manager |
| `project_board_item_updated` | Project-level store | Kernel |
| `task_state_changed` | Task-level store | Task Queue Manager |
| `node_started` | Task-level store | Node Runner |
| `node_completed` | Task-level store | Node Runner |
| `node_failed` | Task-level store | Node Runner / Fallback |
| `artifact_produced` | Task-level store | Node Runner |
| `advisor_requested` | Task-level store | Advisor Broker |
| `advisor_response_received` | Task-level store | Advisor Broker |

## Event Routing

The Kernel routes events to the correct store based on `event_type`:

- Events with `correlation.project_id` and no `correlation.task_id` → project-level store
- Events with `correlation.task_id` → task-level store
- Events with both → task-level store (task is the execution context)

## Idempotency Key Format

Recommended format: `{entity}:{from_state}:{to_state}:{version}`

Examples:
- `item_001:review:done:v1`
- `stage0_task_001:pending:queued:v1`
- `edge_001_003:resolved:v1`

## Replay Interface

```
kernel.replay(store_path: string) → ReplayResult
```

Replay flow:
1. Run `replay_preflight()` on the store file
2. If preflight fails → return error, do not replay
3. If preflight passes → iterate events in append order
4. For each event → apply to projection store
5. Return ReplayResult with final state

## Replay Validation (Preflight)

Before replay, the Kernel MUST run preflight validation:

1. Every line is valid JSON
2. Every event has `schema_version = "event.v1"`
3. No duplicate `event_id`
4. Timestamps are non-decreasing (warning if not)
5. All `parent_event_id` references exist in the stream
6. Every line ends with `\n`
7. No line contains multiple JSON objects

If any check fails → replay is aborted with a detailed error report.
