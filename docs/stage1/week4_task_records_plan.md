# Stage 1 Week 4 Task Records Plan

## Goal

Implement a deterministic Task Record integration layer and Final Gate runtime skeleton. Week 4 connects Stage 0-style task artifacts to the Week 1 validators and Week 3 project status semantics without executing tasks or approving external actions.

## Scope

In scope:

- Load task record bundles from task directories.
- Validate required task record files.
- Validate `completion.json` with the existing completion validator.
- Validate `handoff_pack.json` with the existing handoff pack validator.
- Validate task-level `events.jsonl` with existing replay preflight.
- Treat `run_log.md` as read-only evidence when present.
- Evaluate a Final Gate decision from a task bundle and current project item status.
- Preserve `task completed != project item done`.

Out of scope:

- Model calls.
- Real agents.
- Task execution.
- Shell command execution.
- Sandbox execution.
- Web UI.
- Provider failover.
- Concurrency.
- Dynamic DAG mutation.
- CLI mutation commands.
- Direct project board or event log mutation from Final Gate.

## Files

Create:

- `src/harness_core/task_records.py`
- `src/harness_core/final_gate.py`
- `tests/test_task_records.py`
- `tests/test_final_gate.py`

Modify if needed:

- `src/harness_core/__init__.py`

## Task Record Store

### Responsibilities

`TaskRecordStore` reads Stage 0-style task directories and returns a structured bundle. It does not execute task contents, shell commands, or scripts.

Files:

- Required: `task_spec.json`
- Required: `completion.json`
- Required: `handoff_pack.json`
- Required: `events.jsonl`
- Optional evidence: `run_log.md`

Validation:

- Missing required files become validation errors.
- Invalid JSON files become validation errors.
- `completion.json` is checked with `validate_completion_record`.
- `handoff_pack.json` is checked with `validate_handoff_pack`.
- `events.jsonl` is checked with `validate_replay_preflight_check`.
- `run_log.md` is loaded only as evidence text when present.

### Public API

```python
TaskRecordStore(root_path)
TaskRecordStore.find_task_dirs() -> list[Path]
TaskRecordStore.load_task_bundle(task_dir) -> TaskRecordBundle
TaskRecordStore.validate_task_bundle(task_dir) -> TaskRecordValidationReport
```

### Data Structures

Use stdlib dataclasses:

- `TaskRecordBundle`
- `TaskRecordValidationReport`

`TaskRecordBundle` should include:

- `task_dir`
- `task_spec`
- `completion`
- `handoff_pack`
- `events_path`
- `run_log_path`
- `run_log_text`

`TaskRecordValidationReport` should include:

- `ok`
- `errors`
- `warnings`
- `bundle`

## Final Gate Runner

### Responsibilities

`FinalGateRunner` evaluates whether a task record bundle can move a project item from `review` to `done`. It does not mutate the Project Board or append events.

Inputs:

- `TaskRecordBundle`
- current project item status

Checks:

- Current project item status must be `review`.
- `completion.json` must be valid.
- `handoff_pack.json` must be valid.
- Completed task records with `exit_code == 0` may pass.
- Non-blocking warnings may produce `pass_with_notes`.
- Missing or invalid required artifacts produce `fail`.
- Pending approval requests remain pending evidence and do not imply an approval action was executed.

### Public API

```python
FinalGateRunner()
FinalGateRunner.evaluate(bundle, current_item_status: str) -> FinalGateDecision
```

### Decision Data

Use stdlib dataclasses:

- `FinalGateDecision`

Fields:

- `result`: `pass`, `pass_with_notes`, or `fail`
- `next_project_status`: `done`, `review`, or `failed`
- `reasons`: tuple of strings
- `evidence_refs`: tuple of strings

## Tests

Task Record Store tests:

- Loads a valid Stage 0-style task directory copied to a temporary directory.
- Detects missing `task_spec.json`.
- Detects missing `completion.json`.
- Detects invalid `completion.json`.
- Detects invalid `handoff_pack.json`.
- Does not execute commands or task content.
- Treats `run_log.md` as evidence only.
- Leaves source `docs/stage0` unchanged.

Final Gate tests:

- Valid completed task bundle plus current status `review` returns `pass` and next status `done`.
- Valid completed task bundle plus current status not `review` returns `fail`.
- Missing or invalid completion returns `fail`.
- Missing or invalid handoff pack returns `fail`.
- `pass_with_notes` is possible for non-blocking warnings.
- `approval_request.decision == pending` does not mean approval action was executed.
- Final Gate does not mutate event logs or project board directly.

## Git Plan

Commit 1:

- `docs/stage1/week4_task_records_plan.md`

Commit message:

```text
Plan Stage 1 Week 4 task records
```

Commit 2:

- `src/harness_core/task_records.py`
- `tests/test_task_records.py`
- `src/harness_core/__init__.py` if needed

Commit message:

```text
Implement Stage 1 Week 4 task record store
```

Commit 3:

- `src/harness_core/final_gate.py`
- `tests/test_final_gate.py`
- `src/harness_core/__init__.py` if needed

Commit message:

```text
Implement Stage 1 Week 4 final gate skeleton
```

Before every commit:

```bash
PYTHONPATH=src python3 -m unittest discover -s tests
git status --porcelain
```

## Stop Conditions

Stop and ask if:

- implementation requires modifying `docs/stage0/events.jsonl`
- implementation requires installing a dependency
- implementation requires task command execution
- implementation requires model calls
- implementation requires a mutating CLI command
- event schema needs material changes
- tests require broad rewrites of Week 1, Week 2, or Week 3 components
- scope expands beyond TaskRecordStore or FinalGateRunner
