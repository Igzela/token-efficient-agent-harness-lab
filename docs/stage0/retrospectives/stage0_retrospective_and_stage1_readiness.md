# Stage 0 Retrospective / Stage 1 Readiness Report

Project: `proj_2026_stage0_schema_validation`
Date: 2026-05-15
Source: 5 manual tasks across docs/stage0/

## 1. Executive Summary

**Stage 0 is complete.** All 10 Exit Criteria are satisfied.

| # | Exit Criteria | Status |
|---|--------------|--------|
| 1 | Complete a small Project Board | DONE |
| 2 | At least 5 project items extracted | DONE (5 items) |
| 3 | At least one Project Dependency Graph defined | DONE (5 nodes, 3 edges) |
| 4 | At least 3 items enter task queue simulation | DONE |
| 5 | Manually run through 5 real tasks | 5/5 DONE |
| 6 | Last 3 tasks no longer modify core schema | DONE |
| 7 | At least 2 uses of Advisor Protocol | 2/2 DONE |
| 8 | At least 1 failure enters Fix Loop | 1/1 DONE |
| 9 | All 5 tasks validate Project Board status writeback | 5/5 DONE |
| 10 | Batch Digest enables clear next-step decisions | DONE |

**Final item status:**

| item | type | status | Final Gate |
|------|------|--------|-----------|
| item_001 | module | done | pass |
| item_002 | bug | done | pass |
| item_003 | doc | done | pass |
| item_004 | test_case | done | pass |
| item_005 | module | done | pass |

**Recommendation: Enter Stage 1.** The schemas are validated, the manual simulation exposed concrete implementation requirements, and the issues found are all solvable in code.

## 2. What Worked

### Project Board

The 7-status model (`todo` → `ready` → `running` → `blocked` → `review` → `done` → `failed`) carried all 5 items through their full lifecycle. No status was ambiguous or missing. The `review` status correctly separates task completion from Final Gate approval.

### Project Dependency Graph

Hard dependency (edge_001_002, edge_001_003) and soft dependency (edge_002_005) both behaved as specified. The `downstream_policy` with `on_upstream_success` / `on_upstream_fail` / `on_upstream_partial` is expressive enough for Stage 1 scheduling.

### Project-to-Queue Handoff

`handoff_id` + `scheduling_policy` is a clean contract. The handoff event (`project_to_queue_handoff_created`) correctly records the moment an item enters the task queue, distinct from the item state change.

### Task Records

All 6 files per task directory (`task_spec.json`, `events.jsonl`, `handoff_pack.json`, `completion.json`, `run_log.md`, `retrospective.md`) are sufficient. The `handoff_pack.json` with `structured_fields` + `summary` + `evidence_refs` is particularly valuable for automated verification.

### Project-Level vs Task-Level Events Separation

The rule "project-level events in `docs/stage0/events.jsonl`, task-level events in `tasks/*/events.jsonl`" was easy to follow and produced clean, queryable event streams. No ambiguity about where an event belongs.

### Manual Final Gate

Final Gate correctly enforces the rule: `task completed ≠ project item done`. All 5 items entered `review` after task completion and only moved to `done` after explicit Final Gate verification. The Final Gate mapping (`pass` → `done`, `pass_with_notes` → `review`, `fail` → `failed`) is well-defined.

### Approval Request

Task 004 validated that `approval_request` with `decision: "pending"` can exist in a completed task. The semantic separation between "approval_request template is valid" and "approval action is approved" is critical and worked correctly.

### Advisor Protocol + Fix Loop

Task 005 validated the full failure path: Advisor Preflight (safety check) → node_failed (F008_FORMAT_ERROR, failed_retryable) → Advisor Correction (fix guidance) → Fix Loop (patch + retry) → success. Two Advisor calls with complete `diagnosis`, `recommended_action`, `do_not_do`, `confidence` fields prove the protocol is sufficient.

## 3. Issues Found

### Issue 1: task_spec.json Lacked project_id

**Found in:** Task 001 (discovery), Task 002 (fix)

**Problem:** `task_spec.json` was the only schema without a `project_id` field. All other schemas (project_brief, project_board, project_dependency_graph, events) include it.

**Impact:** Cross-project task queues cannot identify project ownership from task_spec alone.

**Resolution:** Added `project_id` to all 5 task_spec.json files. Backward compatible.

**Stage 1 implication:** Schema cross-check must be a validation step when defining new schemas.

### Issue 2: allowed_files Repeatedly Incomplete (4 Occurrences)

**Found in:** Task 002, Task 003, Task 004, Task 005

**Problem:** Every task discovered that `allowed_files` in project_board.md was incomplete. The Planner/Architect consistently underestimated the files a task needs to touch.

| Task | Original allowed_files | Actual needed | Gap |
|------|----------------------|---------------|-----|
| task-002 | 2 files | 7 files | 5 task_spec.json missing |
| task-003 | 2 files | 5 files | events.jsonl, completion.json, project_board.md missing |
| task-004 | 2 files | 8 files | events.jsonl, completion.json, project_board.md, etc. missing |
| task-005 | 3 files | 9 files | handoff_pack.json, project_board.md, etc. missing |

**Impact:** Each task required a "scope correction" mid-execution, documented in run_log.md.

**Stage 1 implication:** `allowed_files completeness checker` is a mandatory pre-flight check. Every task that writes events.jsonl, completion.json, handoff_pack.json, or does Project Board writeback must have those files in allowed_files.

### Issue 3: Project-Level Events Must Be Centralized

**Found in:** Initial planning confusion (Task 001-002 era)

**Problem:** Early plans mixed project-level events into task directories. The rule "project-level events in `docs/stage0/events.jsonl`" had to be explicitly established.

**Stage 1 implication:** Event Router must have explicit routing rules by event_type, not by caller convenience.

### Issue 4: batch_digest Cannot Replace events.jsonl

**Found in:** Task 003 era

**Problem:** batch_digest.md is a human-readable summary, not an event source. It cannot be used for projection replay or audit trails.

**Stage 1 implication:** batch_digest must be generated FROM events + projections, not maintained as a separate source of truth.

### Issue 5: task completed ≠ project item done

**Found in:** Task 001 era

**Problem:** completion.json records task-level completion, but Project Board item status must not go directly to `done`. The `review` status is the correct intermediate state.

**Stage 1 implication:** The state machine must enforce: `COMPLETED` (task queue) → `review` (project board) → Final Gate → `done`.

### Issue 6: approval_request decision pending vs Final Gate pass

**Found in:** Task 004

**Problem:** Task 004's approval_request has `decision: "pending"`. The Final Gate for item_004 validates the approval_request TEMPLATE quality, not the approval action itself. These are distinct verifications.

**Stage 1 implication:** `approval_status` (pending/approved/rejected) and `final_gate_result` (pass/pass_with_notes/fail) must be independent fields. A task can pass Final Gate while its approval_request remains pending.

### Issue 7: failure_code Must Use Canonical Enum

**Found in:** Task 005

**Problem:** Failure codes must use a canonical enum (e.g., `F008_FORMAT_ERROR`) as the primary code. Subcodes (e.g., `handoff_pack_incomplete`) can be task-specific.

**Stage 1 implication:** `failure_code enum validator` must check that the primary code is in the registered enum. Subcodes are freeform but recommended.

### Issue 8: events.jsonl Line 17 Duplicate Event Concatenation

**Found in:** events.jsonl line 17

**Problem:** Line 17 contains TWO identical JSON objects concatenated on the same line without a newline separator. Both objects have `event_id: "evt_20260515_000030"` (item_005 running→review).

Root cause: `printf` wrote 3 events but did not add a trailing newline after the last one. A subsequent `echo >>` appended the third event to line 16 instead of creating line 17. Later, another `echo >>` added a duplicate of the same event, creating the concatenation on line 17.

**Impact:**
1. JSONL parser treats line 17 as a single oversized JSON object — parse failure
2. Even after fixing the newline, the duplicate `event_id` causes projection replay idempotency check anomalies
3. Event count is wrong (17 unique events, but file has 18 lines + 1 concatenation)

**Stage 1 requirement — Event Store hard requirements:**
- **Append-only atomic write**: Each write must emit exactly one complete JSON object followed by `\n`. No concatenation, no partial writes.
- **Newline enforcement**: Every line MUST end with `\n`. The Event Store must reject writes that do not terminate with a newline.
- **JSONL line validator**: Each line must be valid JSON. Reject writes that produce invalid JSON.
- **event_id uniqueness check**: Before appending, check that the `event_id` does not already exist in the store. Reject duplicates.
- **Replay preflight check**: Before projection replay, validate the entire event stream for: valid JSON per line, no duplicate event_ids, monotonically non-decreasing timestamps, no missing parent_event_ids.

## 4. Stage 1 Must-Have Implementation Requirements

| # | Requirement | Rationale |
|---|-------------|-----------|
| 1 | Kernel event append | Atomic, newline-terminated, idempotent event writes |
| 2 | Project Board state projection | From events → current item statuses |
| 3 | Task Queue state projection | From events → queue statuses (QUEUED/RUNNING/COMPLETED/etc.) |
| 4 | Project Board ↔ Task Queue status mapping | §6.8 mapping table implementation |
| 5 | allowed_files completeness checker | Mandatory pre-flight; prevents Issue #2 recurrence |
| 6 | events schema validator | event.v1 field completeness check |
| 7 | completion.json validator | status/exit_code/artifact_refs required fields |
| 8 | handoff_pack validator | structured_fields/summary/evidence_refs required |
| 9 | approval_request validator | decision/options/timeout_policy required |
| 10 | advisor protocol validator | diagnosis/recommended_action/do_not_do/confidence required |
| 11 | failure_code enum validator | Primary code in canonical enum; subcode freeform |
| 12 | batch_digest generator | Auto-generate from events + projections |
| 13 | Final Gate capability | Verify completion + handoff + run_log before item → done |

## 5. Stage 1 Policy Rules Needed

| # | Rule | Trigger | Action |
|---|------|---------|--------|
| 1 | forbidden_files touched | Task attempts to write to forbidden_files path | block immediately |
| 2 | allowed_files incomplete | Pre-flight check finds missing files | scope correction required before execution |
| 3 | completion without handoff_pack | completion.json exists but handoff_pack.json is empty/missing | fail |
| 4 | non-canonical failure_code | failure_code not in registered enum | fail |
| 5 | approval_request decision pending | approval_request.decision = "pending" | item may enter review, but approval action must not execute |
| 6 | task completed ≠ project item done | completion.json status = "completed" | item enters review, not done |
| 7 | project item done only after Final Gate | Final Gate not yet run | item stays in review until Final Gate pass |

## 6. Minimal Stage 1 Runtime Components

| Priority | Component | Description |
|----------|-----------|-------------|
| **P0** | **Harness Kernel** | Event append core — atomic write, newline enforcement, idempotency check |
| **P0** | **Event Store** | Append-only event persistence — JSONL format, event_id uniqueness, replay validation |
| **P0** | **Projection Store** | Materialized views — Project Board state, Task Queue state, dependency graph state |
| **P0** | **Project Board Manager** | Item lifecycle — status transitions, dependency resolution, Final Gate protocol |
| **P0** | **Task Queue Manager** | Queue lifecycle — handoff, scheduling, status mapping to Project Board |
| **P1** | **Validator Suite** | All validators: events schema, completion, handoff_pack, approval_request, advisor protocol, failure_code enum, allowed_files completeness |
| **P1** | **Approval Broker** | Approval request lifecycle — create, decide (approve/reject/defer), timeout policy |
| **P1** | **Advisor Broker** | Advisor Protocol lifecycle — preflight, correction, response recording |
| **P2** | **Batch Digest Generator** | Auto-generate morning dashboard from events + projections |

## 7. Stage 1 Do-Not-Build-Yet

The following components are out of scope for Stage 1 MVP:

| Component | Reason to defer |
|-----------|----------------|
| Web UI | Stage 1 focuses on backend Kernel + projections; UI can wait |
| Routing Optimizer | Single-task sequential execution is sufficient for validation |
| Skill Extractor | Skills can be hardcoded in task_spec for Stage 1 |
| Dynamic DAG Mutation | Dependency graph is static in Stage 1 |
| Fragment Integrator | No fragmented execution in Stage 1 |
| Real multi-agent concurrency | Sequential single-agent execution only |
| Provider failover | Single model provider, no failover needed |
| Build sampling | No performance optimization needed yet |

## 8. Open Questions

| # | Question | Context |
|---|----------|---------|
| 1 | events.jsonl storage location: keep `docs/stage0/events.jsonl` or migrate to `.runtime/events.jsonl`? | `.runtime/` is the conventional runtime data directory, but `docs/` is human-readable. Stage 1 needs a decision on where the canonical event store lives. |
| 2 | Task events: keep per-task `events.jsonl` or consolidate into a single event store? | Per-task files are human-readable and isolated. A single store enables cross-task queries. Stage 1 must choose one or define a clear routing/projection layer. |
| 3 | Final Gate implementation: Kernel/Manager method or independent node? | Both must implement the same protocol: input = completion.json + handoff_pack + run_log; output = pass/pass_with_notes/fail → item status mapping. The implementation form is a Stage 1 design decision. |
| 4 | Scope correction: should expanding allowed_files require explicit approval? | Stage 0 treated it as a documented scope correction, not an approval-requiring action. Stage 1 must decide whether to formalize this as an approval dependency. |
| 5 | README Current Status: should it be generated from batch_digest or maintained manually? | Stage 0 updated it manually each task. Stage 1 could auto-generate it from projections, making it a derived artifact. |

## 9. Recommended Next Step

**Enter Stage 1.**

**Week 1 focus (P0 only):**

1. **Harness Kernel** — implement atomic event append with newline enforcement and event_id uniqueness check
2. **Event Store** — JSONL-based append-only storage with replay validation (line 17 issue is the hard requirement)
3. **Projection Store** — materialized views for Project Board state and Task Queue state
4. **Project Board Manager** — item lifecycle with §6.8 status mapping
5. **Task Queue Manager** — handoff, scheduling, dependency resolution

Do NOT expand to Web UI, routing optimizer, or multi-agent concurrency in Week 1. The goal is to have a working Kernel that can replay Stage 0's events and produce the same Project Board state.

**Success criteria for Stage 1 Week 1:**
- Event Store can ingest all 18 lines of `docs/stage0/events.jsonl` (after fixing line 17) without errors
- Projection replay produces correct item statuses: all 5 items = done
- Task Queue projection correctly maps task statuses to Project Board statuses
- allowed_files completeness checker rejects the original (incomplete) item_002/003/004/005 definitions
