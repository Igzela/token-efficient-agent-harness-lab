# MVP Component Scope

Source: harness_architecture_book_v0.7.4.1-canonical, docs/stage0/retrospectives/

## Overview

Stage 1 Week 1 implements 7 MVP components. This document defines each component's interface, boundaries, and dependencies.

---

## 1. Harness Kernel

**Purpose:** Core write interface for all events. All state changes must go through the Kernel's event append API.

**Interface:**
```
kernel.event_append(event: Event) → AppendResult
kernel.replay(store_path: string) → ReplayResult
kernel.validate_event(event: Event) → ValidationResult
```

**Boundary:**
- Writes: event store (append only)
- Reads: event store (for uniqueness check)
- Does NOT: execute tasks, call models, manage sandboxes

**Dependencies:** None (foundational component)

**Spec:** `docs/stage1/kernel_spec.md`

---

## 2. Event Store

**Purpose:** Append-only event persistence in JSONL format. Enforces write integrity and provides replay validation.

**Interface:**
```
event_store.append(event: Event) → AppendResult
event_store.validate_line(line: string) → ValidationResult
event_store.check_uniqueness(event_id: string) → boolean
event_store.check_idempotency(key: string, payload_hash: string) → IdempotencyResult
event_store.replay_preflight(stream: string[]) → PreflightResult
event_store.get_events(filter?: EventFilter) → Event[]
```

**Boundary:**
- Storage: JSONL files (one per store instance)
- Write contract: atomic append, one JSON per line, newline terminated
- Validation: JSONL line validity, event_id uniqueness, idempotency_key conflict detection

**Dependencies:** Kernel (writes via Kernel API)

**Requirements:** `docs/stage1/event_store_requirements.md`

---

## 3. Projection Store

**Purpose:** Materialized views computed from events. Provides current state without re-scanning the full event stream.

**Projections:**

### 3a. Project Board State Projection

```
projection.project_board_state(events) → Map<item_id, ItemState>

ItemState:
  item_id: string
  status: todo | ready | running | blocked | review | done | failed
  blocked_reason: string | null
  last_event_id: string
  last_updated: timestamp
```

Source events: `project_item_state_changed`, `project_board_item_updated`

### 3b. Task Queue State Projection

```
projection.task_queue_state(events) → Map<task_id, QueueState>

QueueState:
  task_id: string
  queue_status: QUEUED | RUNNING | COMPLETED | FAILED | ...
  project_board_status: ready | running | blocked | review | done | failed
  blocked_reason: string | null
  last_event_id: string
```

Source events: `task_state_changed`, `project_to_queue_handoff_created`

### 3c. Dependency Graph State Projection

```
projection.dependency_graph_state(events) → GraphState

GraphState:
  nodes: Map<node_id, NodeState>
  edges: Map<edge_id, EdgeState>

NodeState:
  node_id: string
  item_id: string
  status: string

EdgeState:
  edge_id: string
  from_node: string
  to_node: string
  resolved: boolean
```

Source events: `project_dependency_resolved`, `project_item_state_changed`

**Replay interface:**
```
projection.replay(events: Event[]) → ProjectionResult
```

Replay is idempotent: replaying the same events always produces the same state.

**Dependencies:** Event Store (reads events)

---

## 4. Project Board Manager

**Purpose:** Manages project item lifecycle — status transitions, Final Gate protocol, allowed_files completeness checking.

**Interface:**
```
board.transition(item_id, new_status, reason) → TransitionResult
board.final_gate(item_id, completion, handoff, run_log) → GateResult
board.check_allowed_files(item_id, required_files) → CompletenessResult
board.get_item(item_id) → Item
board.get_all_items() → Item[]
```

### State Machine (7 states)

```
todo → ready → running → blocked → review → done → failed
```

Legal transitions:
| From | To | Condition |
|------|-----|-----------|
| todo | ready | Dependencies satisfied |
| ready | running | Task queue handoff |
| running | blocked | Dependency/approval/provider block |
| running | review | Task completed |
| blocked | running | Block resolved |
| blocked | failed | Unrecoverable block |
| review | done | Final Gate pass |
| review | review | Final Gate pass_with_notes |
| review | failed | Final Gate fail |
| done | (terminal) | — |
| failed | (terminal) | — |

### Final Gate Protocol

Input:
- `completion.json` (task completion record)
- `handoff_pack.json` (structured fields + summary + evidence_refs)
- `run_log.md` (human-readable trace)

Output:
- `pass` → item status = done
- `pass_with_notes` → item status = review (stays in review)
- `fail` → item status = failed

### allowed_files Completeness Checker

Checks that an item's `allowed_files` includes all files the task will actually need to write:
- `events.jsonl` (if task writes events)
- `completion.json` (if task produces completion)
- `handoff_pack.json` (if task produces handoff)
- `project_board.md` (if task does status writeback)

Stage 0 reference: items 002, 003, 004, 005 all had incomplete allowed_files.

**Dependencies:** Event Store (emits events on state change)

---

## 5. Task Queue Manager

**Purpose:** Manages task queue lifecycle — handoff reception, scheduling, status mapping to Project Board.

**Interface:**
```
queue.receive_handoff(handoff) → HandoffResult
queue.transition(task_id, new_status) → TransitionResult
queue.map_to_project_board(task_status) → project_board_status
queue.get_task(task_id) → Task
queue.get_all_tasks() → Task[]
```

### Handoff Reception

Only accepts items with `status = ready` on the Project Board.

Input: `handoff_id`, `item_id`, `scheduling_policy`
Output: task queue entry with status `QUEUED`

### Task Queue Status (16 states, §6.8)

```
QUEUED, TRIAGED, READY, READY_READONLY, READY_WRITE,
RUNNING,
WAITING_APPROVAL, PAUSED_BUDGET, WAITING_DEPENDENCY,
BLOCKED, BLOCKED_UPSTREAM_FAILED, BLOCKED_APPROVAL, BLOCKED_PROVIDER,
COMPLETED, FAILED, CANCELLED_BY_DEPENDENCY
```

### Status Mapping (§6.8)

| Task Queue Status | Project Board Status |
|-------------------|---------------------|
| QUEUED / TRIAGED / READY / READY_READONLY / READY_WRITE | ready |
| RUNNING | running |
| WAITING_APPROVAL / BLOCKED_APPROVAL | blocked (approval) |
| PAUSED_BUDGET | blocked (budget) |
| WAITING_DEPENDENCY / BLOCKED_UPSTREAM_FAILED | blocked (dependency) |
| BLOCKED / BLOCKED_PROVIDER | blocked (generic) |
| COMPLETED | review |
| FAILED / CANCELLED_BY_DEPENDENCY | failed |

Week 1 only supports `scheduling_policy: sequential`.

**Dependencies:** Project Board Manager (status mapping), Event Store (emits events)

---

## 6. Validator Suite

**Purpose:** Validates all Stage 1 artifacts against their schemas.

**Validators:**

| # | Validator | Input | Key Rules |
|---|-----------|-------|-----------|
| 1 | events schema | event JSON | event.v1 fields complete |
| 2 | completion.json | completion record | status, exit_code, artifact_refs required |
| 3 | handoff_pack | handoff pack | structured_fields, summary, evidence_refs required |
| 4 | approval_request | approval record | decision, options, timeout_policy required |
| 5 | advisor protocol | advisor call record | diagnosis, recommended_action, do_not_do, confidence required; call count is task-specific |
| 6 | failure_code enum | failure_code string | primary code in canonical enum; subcode freeform |
| 7 | allowed_files completeness | item + required files | all required files listed |
| 8 | replay preflight | event stream | valid JSON, no duplicate event_id, non-decreasing timestamps, no missing parent refs |

**Interface:**
```
validator.validate(artifact_type, artifact) → ValidationResult
validator.validate_all(artifacts: Map<type, artifact>) → AggregateResult
```

**Dependencies:** Event Store (replay preflight), Project Board Manager (allowed_files)

**Requirements:** `docs/stage1/validator_suite_requirements.md`

---

## 7. Batch Digest Generator (Stub)

**Purpose:** Auto-generate morning dashboard from events + projections.

**Interface:**
```
digest.generate(events, projections) → DigestYAML
```

**Output format:** YAML matching `batch_digest` schema (§7.8):
```yaml
batch_digest:
  batch_id:
  overnight_summary:
  completed_tasks:
  blocked_or_waiting_approval:
  failed_tasks:
  risk_cost_report:
  recommended_actions:
```

Week 1: stub only — defines the interface but does not implement full generation logic.

**Dependencies:** Projection Store, Event Store

---

## Component Dependency Graph

```
Event Store ←── Harness Kernel (writes)
    ↑
    ├── Projection Store (reads events)
    │       ↑
    │       ├── Batch Digest Generator (reads projections)
    │       └── Validator Suite (replay preflight)
    │
    ├── Project Board Manager (emits events, reads events)
    │       ↑
    │       └── Task Queue Manager (status mapping, emits events)
    │
    └── Validator Suite (validates artifacts)
```

## Cross-Cutting Concerns

### Event Ownership

- Project-level events → `docs/stage0/events.jsonl` (or Stage 1 equivalent)
- Task-level events → `tasks/*/events.jsonl`

### Schema Source of Truth

All schemas referenced from: `docs/architecture/harness_architecture_book_v0.7.4.1-canonical.md` §7

### Stage 0 Data as Test Fixtures

All `docs/stage0/` data serves as read-only test fixtures for Stage 1 validation. The original files must not be modified.
