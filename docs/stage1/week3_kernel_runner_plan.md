# Stage 1 Week 3 Kernel Runner Plan

## Goal

Implement a deterministic local Kernel loop / Batch Runner skeleton that connects the Week 1 runtime library and Week 2 CLI concepts into a controlled MVP runtime flow.

Week 3 introduces controlled event appends to an explicit writable event log. It does not execute tasks, call models, spawn agents, run shell commands, use sandboxes, add Web UI, add provider failover, add concurrency, or mutate a dynamic DAG.

## Scope

In scope:

- Kernel wrapper around Event Store and projections.
- Batch Runner skeleton.
- Read event stream.
- Validate event stream before projection or append.
- Generate projected state.
- Select ready item.
- Build deterministic planned events.
- Validate all planned events in memory before append.
- Append planned events through `EventStore`.
- Generate digest after run.

Out of scope:

- Model calls.
- Real agent execution.
- Web UI.
- Provider failover.
- Routing optimizer.
- Skill extractor.
- Dynamic DAG mutation.
- Fragment integrator.
- Real multi-agent concurrency.
- Build sampling.
- Sandbox execution.
- Shell command execution from tasks.
- Mutating CLI command.

## Files

Create:

- `src/harness_core/kernel.py`
- `src/harness_core/batch_runner.py`
- `tests/test_kernel.py`
- `tests/test_batch_runner.py`

Modify if needed:

- `src/harness_core/__init__.py`

## Kernel Responsibilities

`Kernel` owns one explicit event log path and composes the Week 1 primitives.

Public API:

```python
Kernel(event_log_path)
Kernel.validate() -> ReplayPreflightReport
Kernel.project_state() -> ProjectStateProjection
Kernel.task_queue_state() -> TaskQueueProjection
Kernel.projections() -> ProjectionBundle
Kernel.append_project_event(event: dict) -> None
```

Behavior:

- Validate the event stream with `replay_preflight`.
- Reject invalid event streams before projection or append.
- Replay project state with `replay_project_state`.
- Replay task queue state with `replay_task_queue_state`.
- Replay all projections with `replay_all`.
- Append project-level events through `EventStore.append_event`.
- Append only to the explicit `event_log_path`.

Forbidden append targets:

- `docs/stage0/events.jsonl`
- `tests/fixtures/stage0_events_with_line17_issue.jsonl`
- `tests/fixtures/stage0_events_sanitized.jsonl`

Tests must copy fixtures into temporary paths before append.

## Batch Runner Responsibilities

`BatchRunner` simulates one deterministic local batch step.

Public API:

```python
BatchRunner(kernel)
BatchRunner.list_ready_items() -> list[ProjectItemState]
BatchRunner.run_one_ready_item(item_id: str) -> RunResult
```

`RunResult`:

```python
RunResult:
  item_id: str
  appended_event_ids: tuple[str, ...]
  digest: BatchDigest
```

Behavior:

1. Validate the event log.
2. Load projected project and task state.
3. List project items with status `ready`.
4. Exclude ready items that already have handoff records.
5. For `run_one_ready_item(item_id)`, confirm the item is ready and not already handed off.
6. Pre-build all planned events:
   - `project_item_state_changed`: `ready -> running`
   - `project_to_queue_handoff_created`
   - `project_item_state_changed`: `running -> review`
7. Validate all planned events in memory before appending any.
8. Append events sequentially through `EventStore`.
9. Return appended event IDs and post-run digest.

Week 3 does not need full transaction/rollback, but it must avoid appending obviously invalid partial plans.

## Deterministic Event IDs

Event IDs must be deterministic.

Rules:

- Parse numeric suffix from existing event IDs when possible.
- Next event ID is `max_suffix + 1`.
- Preserve prefix style if possible, for example `evt_20260515_000031`.
- If no numeric suffix exists, fallback to `evt_000001`, `evt_000002`, ...
- Never use wall-clock time or randomness for event IDs in tests.

Timestamps should also be deterministic in tests. Use explicit fixed values or derived values rather than current time.

## Tests

Kernel tests:

- `Kernel.validate()` rejects the bad line 17 fixture.
- `Kernel.validate()` accepts the sanitized fixture.
- `Kernel.project_state()` projects five done items from the sanitized fixture.
- `Kernel.append_project_event()` appends a valid event to a temporary event log.
- Appended temporary event log remains valid JSONL.
- Append to forbidden fixture/source paths is rejected.

Batch Runner tests:

- Lists a ready item from a small temporary fixture.
- Refuses to run when no ready items exist.
- Refuses to run when event log preflight fails.
- Refuses to run an item that is not ready.
- Refuses to run a ready item that already has a handoff.
- Pre-builds and validates all planned events before appending.
- Appends running, handoff, and review events to a temporary event log.
- Event appends preserve JSONL validity.
- Result digest is generated after the run.

## CLI Impact

No Week 3 CLI mutation command.

Do not add `run-one` yet. CLI integration can be considered after Kernel and Batch Runner behavior are accepted.

## Git Plan

Commit 1:

- `docs/stage1/week3_kernel_runner_plan.md`

Commit message:

```text
Plan Stage 1 Week 3 kernel runner
```

Commit 2:

- `src/harness_core/kernel.py`
- `tests/test_kernel.py`
- `src/harness_core/__init__.py` if needed

Commit message:

```text
Implement Stage 1 Week 3 kernel skeleton
```

Commit 3:

- `src/harness_core/batch_runner.py`
- `tests/test_batch_runner.py`
- `src/harness_core/__init__.py` if needed

Commit message:

```text
Implement Stage 1 Week 3 batch runner skeleton
```

Before every commit:

```bash
PYTHONPATH=src python3 -m unittest discover -s tests
git status --porcelain
```

## Stop Conditions

Stop and ask if:

- implementation requires modifying `docs/stage0/events.jsonl`
- implementation requires model calls
- implementation requires task execution or shell command execution
- event schema needs material changes
- tests require broad rewrites of Week 1 or Week 2 components
- CLI mutation command becomes necessary
- scope expands beyond Kernel / BatchRunner skeleton
