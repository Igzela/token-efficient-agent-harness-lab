# Stage 1 — MVP Batch Runner

Source: harness_architecture_book_v0.7.4.1-canonical §9, docs/stage0/retrospectives/

## Goal

Implement the minimal runtime infrastructure that can replay Stage 0's events and produce the same Project Board state. No real model calls, no Web UI, no multi-agent concurrency.

## Components (Week 1)

| # | Component | Priority | Description |
|---|-----------|----------|-------------|
| 1 | Harness Kernel | P0 | Event append core — atomic write, newline enforcement, idempotency check |
| 2 | Event Store | P0 | Append-only event persistence — JSONL format, event_id uniqueness, replay validation |
| 3 | Projection Store | P0 | Materialized views — Project Board state, Task Queue state, dependency graph state |
| 4 | Project Board Manager | P0 | Item lifecycle — status transitions, dependency resolution, Final Gate protocol |
| 5 | Task Queue Manager | P0 | Queue lifecycle — handoff, scheduling, status mapping to Project Board |
| 6 | Validator Suite | P1 | All validators: events schema, completion, handoff_pack, approval_request, advisor protocol, failure_code enum, allowed_files completeness |
| 7 | Batch Digest Generator stub | P2 | Auto-generate morning dashboard from events + projections |

## Explicitly NOT in Week 1

| Component | Reason to defer |
|-----------|----------------|
| Web UI | Protocol closure first; UI deferred to Stage 3/4 (§12.15) |
| Routing Optimizer | Stage 3 |
| Skill Extractor | Stage 3 |
| Dynamic DAG Mutation | Stage 4 |
| Fragment Integrator | Stage 3 |
| Real multi-agent concurrency | Stage 1: sequential single-agent only |
| Provider failover | Stage 1: single model provider |
| Build sampling | Stage 2 |
| Real model calls | Week 1: infrastructure only, no API calls |
| Sandbox Manager | Week 1: no real sandbox execution |
| Model Gateway | Week 1: no real API connection |
| Approval Broker (full) | Week 1: validator only, no broker |
| Advisor Broker (full) | Week 1: validator only, no broker |

## Directory Layout

```
stage1/
  README.md                           ← this file
  week1_implementation_plan.md        ← 5 engineering tasks detailed breakdown
  mvp_component_scope.md              ← 7 MVP component interfaces and boundaries
  event_store_requirements.md         ← Event Store hard requirements
  validator_suite_requirements.md     ← 8 validator definitions
  kernel_spec.md                      ← Kernel event append spec
```

## Week 1 Success Criteria

1. Event Store can ingest all 18 lines of `docs/stage0/events.jsonl` (with line 17 split and duplicate removed) without errors
2. Replay preflight detects line 17 issues (concatenation + duplicate event_id) in the original file
3. Projection replay produces correct item statuses: all 5 items = done
4. Task Queue projection correctly maps task statuses to Project Board statuses
5. allowed_files completeness checker rejects the original (incomplete) item_002/003/004/005 definitions
6. All 8 validators pass on Stage 0 task-004 and task-005 fixtures

## Recommended Implementation Order

```
Day 1:  Event Store + JSONL Validator
Day 2:  Projection Store
Day 3:  Project Board Manager
Day 4:  Task Queue Manager
Day 5:  Validator Suite + Digest Stub
```

## Source Data

All source data is in `docs/stage0/` (read-only reference):

| File | Purpose |
|------|---------|
| `docs/stage0/events.jsonl` | Project-level events (18 lines, line 17 has issues) |
| `docs/stage0/project_board.md` | Project Board state (5 items, all done) |
| `docs/stage0/project_dependency_graph.md` | Dependency graph (5 nodes, 3 edges) |
| `docs/stage0/batch_digest.md` | Batch Digest (5 completed tasks) |
| `docs/stage0/tasks/*/events.jsonl` | Task-level events |
| `docs/stage0/tasks/*/completion.json` | Completion records |
| `docs/stage0/tasks/*/handoff_pack.json` | Handoff packs |

## Constraints

- Do NOT modify `src/`, `tests/`, `runtime/`, `.runtime/`, `.git/`
- Do NOT modify `docs/stage0/` (read-only reference)
- Do NOT implement runtime code
- Do NOT install dependencies
- Do NOT commit git
- Only create/modify files under `docs/stage1/`
