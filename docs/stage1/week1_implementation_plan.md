# Week 1 Implementation Plan

Source: harness_architecture_book_v0.7.4.1-canonical, docs/stage0/retrospectives/

## Overview

5 engineering tasks over 5 days. Each task produces planning documents under `docs/stage1/`. No runtime code is implemented.

---

## Task 1: Event Store + JSONL Validator

### Objective

Define the append-only event persistence layer with write integrity enforcement and replay validation. Demonstrate detection of Stage 0 events.jsonl line 17 issues.

### Inputs

| Source | Purpose |
|--------|---------|
| `docs/architecture/harness_architecture_book_v0.7.4.1_canonical.md` §7.2 | event.v1 schema definition |
| `docs/stage0/events.jsonl` (18 lines) | Test fixture for replay validation |
| `docs/stage0/tasks/*/events.jsonl` | Task-level event fixtures |
| `docs/stage0/retrospectives/stage0_retrospective_and_stage1_readiness.md` Issue #8 | Line 17 problem specification |

### Outputs

| Artifact | Description |
|----------|-------------|
| `docs/stage1/event_store_requirements.md` | Complete Event Store requirements with all 9 hard requirements |
| `docs/stage1/kernel_spec.md` | Kernel event append interface, write flow, error types, replay interface |

### Allowed Files

- `docs/stage1/event_store_requirements.md`
- `docs/stage1/kernel_spec.md`
- `docs/stage1/week1_implementation_plan.md`

### Forbidden Files

- `src/`, `tests/`, `runtime/`, `.runtime/`, `.git/`
- `docs/stage0/events.jsonl` — read-only reference, do NOT modify

### Acceptance Criteria

1. Append-only atomic write contract defined (1 JSON + `\n` per write)
2. JSONL line validator rules defined (7 rules)
3. event_id uniqueness check defined (global uniqueness)
4. idempotency_key rules defined (same payload = no-op, different payload = reject)
5. Timestamp rules defined (monotonic non-decreasing, equal timestamps allowed, append order is final order)
6. Replay preflight check defined (7 checks)
7. Stage 0 line 17 detection logic demonstrated (concatenation + duplicate event_id)
8. Error types defined (8 types)

### Test Cases

| # | Input | Expected Result |
|---|-------|-----------------|
| TC-1.1 | Valid event JSON + `\n` | Accept |
| TC-1.2 | Valid event JSON without `\n` | Reject (MissingNewlineError) |
| TC-1.3 | Two JSON objects concatenated on one line | Reject (InvalidJsonError) |
| TC-1.4 | Duplicate event_id | Reject (DuplicateEventIdError) |
| TC-1.5 | Duplicate idempotency_key, same payload | Accept as no-op |
| TC-1.6 | Duplicate idempotency_key, different payload | Reject (IdempotencyConflictError) |
| TC-1.7 | Timestamp regression | Warning (TimestampRegressionWarning) |
| TC-1.8 | schema_version ≠ event.v1 | Reject (SchemaViolationError) |
| TC-1.9 | Stage 0 events.jsonl line 17 actual data | Detect concatenation + duplicate |
| TC-1.10 | Stage 0 events.jsonl full (18 lines) | Preflight fails on line 17 |
| TC-1.11 | Sanitized copy (line 17 split, duplicate removed) | Preflight passes |

### Dependencies

None (foundational task)

### Expected Artifacts

- `docs/stage1/event_store_requirements.md`
- `docs/stage1/kernel_spec.md`

---

## Task 2: Projection Store

### Objective

Define the event → materialized view projection rules. Demonstrate that replaying Stage 0 events (sanitized) produces correct Project Board and Task Queue state.

### Inputs

| Source | Purpose |
|--------|---------|
| Task 1 outputs | Event Store interface definition |
| `docs/stage0/events.jsonl` (sanitized copy/fixture, line 17 split and duplicate removed) | Projection data source |
| `docs/stage0/project_board.md` | Expected projection result (5 items = done) |
| `docs/stage0/project_dependency_graph.md` | Expected dependency state |
| `docs/architecture/harness_architecture_book_v0.7.4.1_canonical.md` §6.8 | Status mapping table |

### Outputs

| Artifact | Description |
|----------|-------------|
| `docs/stage1/mvp_component_scope.md` (Projection Store section) | Projection rules, replay interface, idempotency guarantee |

### Allowed Files

- `docs/stage1/mvp_component_scope.md`
- `docs/stage1/week1_implementation_plan.md`

### Forbidden Files

- `src/`, `tests/`, `runtime/`, `.runtime/`, `.git/`
- `docs/stage0/` — read-only reference, do NOT modify

### Acceptance Criteria

1. event_type → projection rule mapping table defined
2. Project Board projection: replay of sanitized Stage 0 events → 5 items = done
3. Task Queue projection: correct §6.8 status mapping
4. Dependency Graph projection: resolved edges correctly tracked
5. Replay is idempotent (same events → same state)
6. Uses sanitized copy/fixture for replay testing (original docs/stage0/events.jsonl unchanged)

### Test Cases

| # | Input | Expected Result |
|---|-------|-----------------|
| TC-2.1 | Sanitized copy of Stage 0 events.jsonl (line 17 split, duplicate removed) | 5 items = done |
| TC-2.2 | Stage 0 events.jsonl original (with line 17 issues) | Replay preflight fails before projection |
| TC-2.3 | Only first 10 events | item_001=done, item_002=done, item_003=review, item_004=todo, item_005=todo |
| TC-2.4 | Replay same events twice | Same result (idempotent) |
| TC-2.5 | Manual RUNNING → COMPLETED event | Project board item = review |

### Dependencies

Task 1 (Event Store interface)

### Expected Artifacts

- `docs/stage1/mvp_component_scope.md` (Projection Store section)

---

## Task 3: Project Board Manager

### Objective

Define the Project Board item lifecycle: 7-state state machine with legal transition matrix, Final Gate protocol, and allowed_files completeness checker.

### Inputs

| Source | Purpose |
|--------|---------|
| `docs/architecture/harness_architecture_book_v0.7.4.1_canonical.md` §6.3 | Project Board schema |
| `docs/architecture/harness_architecture_book_v0.7.4.1_canonical.md` §6.8 | Status mapping |
| `docs/stage0/project_board.md` | Reference implementation |
| `docs/stage0/retrospectives/stage0_retrospective_and_stage1_readiness.md` Issue #2 | allowed_files incompleteness (4 occurrences) |
| `docs/stage0/retrospectives/stage0_retrospective_and_stage1_readiness.md` Issue #5 | task completed ≠ item done |
| `docs/stage0/retrospectives/stage0_retrospective_and_stage1_readiness.md` Issue #6 | approval_request pending vs Final Gate |

### Outputs

| Artifact | Description |
|----------|-------------|
| `docs/stage1/mvp_component_scope.md` (Project Board Manager section) | State machine, Final Gate protocol, allowed_files checker |
| `docs/stage1/validator_suite_requirements.md` (Validator 7: allowed_files) | allowed_files completeness checker rules |

### Allowed Files

- `docs/stage1/mvp_component_scope.md`
- `docs/stage1/validator_suite_requirements.md`
- `docs/stage1/week1_implementation_plan.md`

### Forbidden Files

- `src/`, `tests/`, `runtime/`, `.runtime/`, `.git/`

### Acceptance Criteria

1. 7-state state machine defined with all legal transitions
2. Illegal transitions rejected (e.g., todo → done skipping intermediate states)
3. Final Gate protocol: task completed → review → Final Gate → done/failed
4. Final Gate input: completion.json + handoff_pack + run_log
5. Final Gate output mapping: pass → done, pass_with_notes → review, fail → failed
6. allowed_files completeness checker detects Stage 0's 4 incomplete items
7. approval_request decision=pending does NOT block item from entering review
8. approval_request decision=pending DOES block approval action execution

### Test Cases

| # | Input | Expected Result |
|---|-------|-----------------|
| TC-3.1 | item status=todo, new status=ready | Legal transition |
| TC-3.2 | item status=todo, new status=done | Illegal (skip) |
| TC-3.3 | task completed, Final Gate not run | item = review |
| TC-3.4 | task completed, Final Gate pass | item = done |
| TC-3.5 | task completed, Final Gate fail | item = failed |
| TC-3.6 | task completed, Final Gate pass_with_notes | item = review (stays) |
| TC-3.7 | Stage 0 item_002 original allowed_files (2 files) | Incomplete (FAIL) |
| TC-3.8 | Stage 0 item_005 current allowed_files (9 files) | Complete (PASS) |
| TC-3.9 | approval_request.decision=pending | Item can enter review |
| TC-3.10 | approval_request.decision=pending | Approval action blocked |

### Dependencies

Task 1 (Event Store for event emission)

### Expected Artifacts

- `docs/stage1/mvp_component_scope.md` (Project Board Manager section)
- `docs/stage1/validator_suite_requirements.md` (Validator 7)

---

## Task 4: Task Queue Manager

### Objective

Define the Task Queue lifecycle: handoff reception, 16-state status machine, and task → project board status mapping.

### Inputs

| Source | Purpose |
|--------|---------|
| `docs/architecture/harness_architecture_book_v0.7.4.1_canonical.md` §6.7 | Project-to-Queue Handoff |
| `docs/architecture/harness_architecture_book_v0.7.4.1_canonical.md` §6.8 | Status mapping (16 states) |
| `docs/stage0/project_board.md` | Handoff reference |
| `docs/stage0/retrospectives/stage0_retrospective_and_stage1_readiness.md` | "Project-to-Queue Handoff" section |

### Outputs

| Artifact | Description |
|----------|-------------|
| `docs/stage1/mvp_component_scope.md` (Task Queue Manager section) | Handoff interface, status machine, mapping rules |

### Allowed Files

- `docs/stage1/mvp_component_scope.md`
- `docs/stage1/week1_implementation_plan.md`

### Forbidden Files

- `src/`, `tests/`, `runtime/`, `.runtime/`, `.git/`

### Acceptance Criteria

1. Handoff interface only accepts status=ready items
2. 16 task queue statuses defined (§6.8)
3. Each task queue status maps to a project board status
4. Handoff event includes handoff_id and scheduling_policy
5. Only sequential scheduling_policy supported in Week 1

### Test Cases

| # | Input | Expected Result |
|---|-------|-----------------|
| TC-4.1 | item status=ready, scheduling=sequential | Accept, generate handoff_created event |
| TC-4.2 | item status=todo | Reject (dependencies not met) |
| TC-4.3 | task RUNNING | project board = running |
| TC-4.4 | task COMPLETED | project board = review |
| TC-4.5 | task FAILED | project board = failed |
| TC-4.6 | task WAITING_APPROVAL | project board = blocked (approval) |
| TC-4.7 | task CANCELLED_BY_DEPENDENCY | project board = failed |
| TC-4.8 | task PAUSED_BUDGET | project board = blocked (budget) |
| TC-4.9 | task BLOCKED_UPSTREAM_FAILED | project board = blocked (upstream_failed) |

### Dependencies

Task 3 (Project Board status machine)

### Expected Artifacts

- `docs/stage1/mvp_component_scope.md` (Task Queue Manager section)

---

## Task 5: Validator Suite + Digest Stub

### Objective

Define all 8 validators with clear pass/fail rules, and define the Batch Digest generator stub interface.

### Inputs

| Source | Purpose |
|--------|---------|
| `docs/architecture/harness_architecture_book_v0.7.4.1_canonical.md` §7.2-§7.8 | Schema definitions |
| `docs/stage0/tasks/task-005-failure-fix-loop/` (all files) | Failure + advisor + fix loop fixture |
| `docs/stage0/tasks/task-004-config-rule/run_log.md` | approval_request fixture |
| `docs/stage0/retrospectives/stage0_retrospective_and_stage1_readiness.md` Issue #7 | failure_code canonical enum |
| `docs/stage0/retrospectives/stage0_retrospective_and_stage1_readiness.md` §4 | 13 must-have requirements |

### Outputs

| Artifact | Description |
|----------|-------------|
| `docs/stage1/validator_suite_requirements.md` | All 8 validators defined with pass/fail rules and test cases |
| `docs/stage1/week1_implementation_plan.md` | Batch Digest stub interface |

### Allowed Files

- `docs/stage1/validator_suite_requirements.md`
- `docs/stage1/week1_implementation_plan.md`

### Forbidden Files

- `src/`, `tests/`, `runtime/`, `.runtime/`, `.git/`

### Acceptance Criteria

1. 8 validators defined with input/output/pass/fail rules
2. failure_code enum validator recognizes canonical codes (F001-F010)
3. handoff_pack validator passes on Stage 0 task-005 fixture
4. completion.json validator detects _template: true vs false
5. Advisor protocol validator validates each call's schema; call count is task-specific
6. For task-005 fixture: expected_min_advisor_calls = 2
7. Batch Digest generator stub interface defined (events + projections → YAML)

### Test Cases

| # | Input | Expected Result |
|---|-------|-----------------|
| TC-5.1 | Stage 0 task-005 completion.json | PASS |
| TC-5.2 | Stage 0 task-005 handoff_pack.json | PASS |
| TC-5.3 | completion.json missing exit_code | FAIL |
| TC-5.4 | handoff_pack missing evidence_refs | FAIL |
| TC-5.5 | failure_code = "F008_FORMAT_ERROR" | PASS |
| TC-5.6 | failure_code = "some_random_string" | FAIL |
| TC-5.7 | approval_request.decision = "pending" | PASS |
| TC-5.8 | approval_request missing timeout_policy | FAIL |
| TC-5.9 | Advisor record only 1 call, task fixture requires 2 → FAIL | FAIL |
| TC-5.10 | Stage 0 task-005 events.jsonl (10 events) | PASS |

### Dependencies

Task 1 (Event Store for replay preflight), Task 2 (Projection Store for digest generation)

### Expected Artifacts

- `docs/stage1/validator_suite_requirements.md`

---

## Recommended Implementation Order

```
Day 1:  Task 1 — Event Store + JSONL Validator
        ├── Write event_store_requirements.md
        └── Write kernel_spec.md

Day 2:  Task 2 — Projection Store
        └── Write mvp_component_scope.md (Projection Store section)

Day 3:  Task 3 — Project Board Manager
        ├── Update mvp_component_scope.md (Project Board Manager section)
        └── Write validator_suite_requirements.md (Validator 7: allowed_files)

Day 4:  Task 4 — Task Queue Manager
        └── Update mvp_component_scope.md (Task Queue Manager section)

Day 5:  Task 5 — Validator Suite + Digest Stub
        ├── Complete validator_suite_requirements.md (all 8 validators)
        └── Finalize week1_implementation_plan.md
```

## Day 1 Detailed Steps

1. Create `docs/stage1/README.md` — Stage 1 overview (done)
2. Create `docs/stage1/event_store_requirements.md`:
   - Define 9 hard requirements
   - Define error types (8 types)
   - Define interface (6 methods)
   - Demonstrate line 17 detection logic
   - Define idempotency_key rules (same payload = no-op, different payload = reject)
3. Create `docs/stage1/kernel_spec.md`:
   - Define `event_append()` 10-step write flow
   - Define AppendResult type
   - Define event type enums (project-level + task-level)
   - Define event ownership rules
   - Define replay interface and preflight validation
   - Define idempotency_key conflict handling
