# Stage 0 — Manual Project Simulation

Source: harness_architecture_book_v0.7.4.1-canonical §10

## Goal

Without a runtime, manually validate:

- Project Board can carry project state
- Project Dependency Graph can express dependencies
- Module Contract can constrain task boundaries
- Test Case Pack can drive acceptance
- Project-to-Queue Handoff is clear
- Task Runtime schema is sufficient
- Batch Digest can serve as the morning dashboard

## Exit Criteria

1. Complete a small Project Board
2. At least 5 project items extracted
3. At least one Project Dependency Graph defined
4. At least 3 items enter task queue simulation
5. Manually run through 5 real tasks
6. Last 3 tasks no longer modify core schema
7. At least 2 uses of Advisor Protocol
8. At least 1 failure enters Fix Loop
9. All 5 tasks validate Project Board status writeback
10. Batch Digest enables clear next-step decisions

## Stage 0 Current Status

Project: `proj_2026_stage0_schema_validation`
Last updated: 2026-05-15T22:05:00+08:00

| item | title | type | status | task dir | notes |
|------|-------|------|--------|----------|-------|
| item_001 | Fill template fields in task_spec.json | module | **done** | task-001-code-small | Final Gate pass |
| item_002 | Fix schema inconsistency (missing project_id) | bug | **done** | task-002-bugfix | Final Gate pass; scope correction recorded |
| item_003 | Update README.md Five Task Templates section | doc | **done** | task-003-doc-update | Final Gate pass; scope correction recorded |
| item_004 | Validate approval_request template | test_case | **done** | task-004-config-rule | Final Gate pass; approval_request decision pending |
| item_005 | Deliberate failure to validate Advisor Protocol | module | **done** | task-005-failure-fix-loop | Final Gate pass; F008_FORMAT_ERROR + Fix Loop + 2 Advisor calls |

Dependency graph: 5 nodes, 3 edges (edge_001_002, edge_001_003, edge_002_005). Two edges resolved.

## Directory Layout

```
stage0/
  README.md                         ← this file
  project_brief.md                  §6.1
  project_board.md                  §6.3
  project_dependency_graph.md       §6.4
  batch_digest.md                   §7.8
  events.jsonl                      §7.2.1 project-level events
  module_contracts/
    module_001.md                   §6.5
    module_002.md                   §6.5
  test_case_packs/
    module_001_tests.md             §6.6
    module_002_tests.md             §6.6
  tasks/
    task-001-code-small/            §10.3 task 1
    task-002-bugfix/                §10.3 task 2
    task-003-doc-update/            §10.3 task 3
    task-004-config-rule/           §10.3 task 4
    task-005-failure-fix-loop/      §10.3 task 5
```

Each task directory contains:

| File | Source | Description |
|------|--------|-------------|
| `task_spec.json` | §10.3 | Task definition from project board item |
| `events.jsonl` | §7.2 | Task-level / node-level event log |
| `handoff_pack.json` | §7.5 | Structured fields + summary + evidence refs |
| `completion.json` | §7.3 | Node completion record |
| `run_log.md` | §7/Memory | Human-readable run trace |
| `retrospective.md` | §7/Memory | Post-task reflection |

## Event Schema Reference

### Project-level events (docs/stage0/events.jsonl)

Project-level events track Project Board state changes, dependency resolution, and handoff.
This file is append-only. Each line is a JSON object conforming to event.v1 schema.

Project-level event types (§7.2.1):

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

### Task-level / node-level events (tasks/*/events.jsonl)

Each task directory has its own events.jsonl for task_state_changed, node_started, artifact_produced, node_completed, and similar execution events.

### Event JSON schema

```json
{
  "event_id": "evt_YYYYMMDD_NNNNNN",
  "schema_version": "event.v1",
  "event_type": "<type>",
  "timestamp": "ISO-8601",
  "producer": { "component_id": "", "component_type": "" },
  "correlation": { "batch_id": "", "task_id": "", "node_id": "", "run_id": "" },
  "severity": "info | warn | error",
  "payload": {},
  "idempotency_key": "",
  "parent_event_id": ""
}
```

## Five Task Templates

| # | item_id | task dir | type | objective | status |
|---|---------|----------|------|-----------|--------|
| 1 | item_001 | task-001-code-small | code_small_change | 验证 task_spec.json 模板能否表达 Stage 0 任务：填写 5 个 task 目录的模板字段 | done |
| 2 | item_002 | task-002-bugfix | bugfix | 修复 task_spec.json 缺少 project_id 字段的 schema inconsistency | done |
| 3 | item_003 | task-003-doc-update | doc_update | 更新 README.md 的 Five Task Templates 表格，加入实际项目 item_id 和 objective | review |
| 4 | item_004 | task-004-config-rule | config_or_rule_change | 验证 task-004 run_log.md 中的 approval_request 模板能否表达审批流程 | todo |
| 5 | item_005 | task-005-failure-fix-loop | failure_then_fix_loop | 故意制造一次失败，验证 Advisor Protocol 和 Fix Loop 记录流程是否完整 | ready |

## Project Board Status Semantics

| Status | Meaning |
|--------|---------|
| `todo` | Not ready for queue entry — dependencies unmet or not yet planned |
| `ready` | Can enter Batch Task Queue — all dependencies satisfied |
| `running` | A task is executing on this item |
| `blocked` | Blocked by dependency, approval, provider, or failure |
| `review` | Task execution complete, awaiting Final Gate / human verification |
| `done` | Final Gate passed, item fully completed |
| `failed` | Failed, no auto-retry |

## Project Board ↔ Task Queue Status Mapping

Task status changes must map back to Project Board items (§6.8):

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

Final Gate mapping: pass → done, pass_with_notes → review, fail → failed.

## Responsibility Boundaries

| Artifact | Owner | Purpose |
|----------|-------|---------|
| `docs/stage0/project_board.md` | Project Architect / PM | Project state source — records item statuses, contracts, dependencies |
| `docs/stage0/events.jsonl` | Kernel (manual sim) | Project-level event log — item state changes, dependency resolution, handoff |
| `docs/stage0/batch_digest.md` | Human reviewer | Morning dashboard — summarizes completed/blocked/failed tasks, recommends next actions |
| `docs/stage0/tasks/*/events.jsonl` | Task runner | Task-level / node-level event log — task_state_changed, node_started, artifact_produced, node_completed |
| `docs/stage0/tasks/*/completion.json` | Task runner | Node completion record — status, exit_code, artifacts |
| `docs/stage0/tasks/*/run_log.md` | Task runner | Human-readable trace — includes bug reproduction, advisor calls, scope corrections |

Key distinction:
- `docs/stage0/events.jsonl` = project-level events (Project Board state, dependency graph, handoff)
- `tasks/*/events.jsonl` = task-level events (task state, node lifecycle, artifacts)
- `batch_digest.md` = human-readable summary, NOT an event log

## Lessons Learned

### Task 001: task_spec.json lacked project_id field

**Finding:** `task_spec.json` was the only schema without a `project_id` field. All other schemas (project_brief, project_board, project_dependency_graph, events) include project_id.

**Fix:** Added `"project_id": "proj_2026_stage0_schema_validation"` to all 5 task_spec.json files (executed in Task 002).

**Lesson:** When defining a new schema, cross-check all fields against existing schemas to ensure consistency.

### Task 002: allowed_files was incomplete

**Finding:** item_002's `allowed_files` in project_board.md only listed 2 files, but fixing the schema required modifying 5 task_spec.json files.

**Fix:** Expanded allowed_files to 7 paths.

**Lesson:** Planner / Architect should include a completeness check for allowed_files — when a task targets a schema used across multiple files, allowed_files must list all affected files. This should be a pre-flight check item.

### General: Scope correction is not越权

When a task discovers that its allowed_files is incomplete, expanding the list is a scope correction, not an unauthorized modification. The run_log must document:
- Original allowed_files
- Why the scope was underestimated
- Corrective action taken
- Recommendation for future prevention

## Next Steps

1. Final Gate for item_003 (this doc update)
2. Run item_004 (approval_request validation — no dependencies)
3. Run item_005 (Advisor Protocol / Fix Loop validation)
4. Complete batch_digest.md with all 5 tasks
5. Review retrospective.md across all tasks for schema/process improvements
6. Assess Stage 0 exit criteria completion
