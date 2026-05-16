# Stage 1 Closeout Plan

## 1. Stage 1 Final Scope

Stage 1 delivers a deterministic local orchestration harness that can:
- Validate an event log
- Replay projections
- Find ready project items
- Run one ready item through the BatchRunner skeleton into review
- Load task records
- Evaluate Final Gate
- Append final project-level state events when allowed
- Generate digest summary
- Expose this as library APIs and tests
- Document final Stage 1 acceptance

## 2. Components Already Completed

| Component | Module | Status |
|---|---|---|
| Event Store | `event_store.py` | Week 1 complete |
| JSONL Validator | `event_store.py` | Week 1 complete |
| Projection Store | `projection_store.py` | Week 1 complete |
| Project Board Manager | `project_board.py` | Week 2 complete |
| Task Queue Manager | `task_queue.py` | Week 2 complete |
| Validator Suite | `validators.py` | Week 2 complete |
| Batch Digest stub | `digest.py` | Week 2 complete |
| CLI read-only wrapper | `cli.py` | Week 2 complete |
| Kernel | `kernel.py` | Week 3 complete |
| BatchRunner | `batch_runner.py` | Week 3 complete |
| TaskRecordStore | `task_records.py` | Week 4 complete |
| FinalGateRunner | `final_gate.py` | Week 4 complete |

## 3. Missing Closeout Piece

**Deterministic orchestration layer** connecting existing components into a single coherent local flow. This is the `Stage1Orchestrator` class.

## 4. Explicit Non-Goals

- No model calls
- No real agents
- No sandbox execution
- No concurrency
- No mutating CLI commands
- No Stage 2 quality/scoring/sampling
- No provider failover
- No dynamic DAG mutation
- No task shell execution
- No Web UI
- No routing optimizer, skill extractor, fragment integrator, or build sampling

## 5. Proposed Final APIs

```
Stage1Orchestrator(event_log_path, task_root=None)

Methods:
  validate() -> ReplayPreflightReport
  projections() -> ProjectionBundle
  project_state() -> ProjectStateProjection
  task_queue_state() -> TaskQueueProjection
  digest() -> BatchDigest
  list_ready_items() -> list[ProjectItemState]
  run_ready_item(item_id: str) -> OrchestrationResult
  evaluate_final_gate(item_id: str, task_dir: Path) -> FinalGateDecision
  apply_final_gate_decision(item_id: str, decision: FinalGateDecision) -> OrchestrationResult
  run_one_step(item_id: str | None = None, task_dir: Path | None = None) -> OrchestrationResult

OrchestrationResult:
  action: str
  item_id: str | None
  appended_event_ids: tuple[str, ...]
  final_gate_result: str | None
  next_status: str | None
  digest_summary: object
  warnings: tuple[str, ...]
```

## 6. Acceptance Tests

1. Orchestrator rejects bad line17 fixture
2. Orchestrator accepts sanitized fixture
3. Orchestrator digest works on sanitized fixture
4. Orchestrator lists ready item from small temp event log
5. Orchestrator run_ready_item moves ready item to review, not done
6. Orchestrator evaluate_final_gate with valid copied task bundle returns pass
7. Orchestrator apply_final_gate_decision appends review -> done
8. Full local deterministic flow: ready -> review -> done
9. No ready item returns no_op without appending
10. Invalid task bundle causes Final Gate fail, no done mark
11. Event log remains valid JSONL after orchestration appends
12. docs/stage0/events.jsonl is never modified

## 7. Stage 1 Final Exit Criteria

- Event stream validation works
- Projection replay works
- Project board state rules work
- Task queue mapping works
- Validator suite works
- Digest stub works
- CLI read-only commands work
- Kernel validates and appends safely
- BatchRunner moves ready item to review
- Task record bundle loads and validates
- Final Gate can move review to done
- Orchestrator composes the deterministic local flow
- All tests pass
- docs/stage0/events.jsonl preserved
