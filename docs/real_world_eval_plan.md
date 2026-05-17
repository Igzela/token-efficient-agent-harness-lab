# Real-World Read-Only Evaluation Track

## Status

This is a post-closeout optional evaluation track for the completed Stage 0-4 harness.

It is not Stage 5. It does not add runtime features, model calls, task execution,
sandbox execution, provider integration, deployment, concurrency, or a Web UI.

## Goal

Use copied real-project-shaped fixture data to evaluate whether the completed
Stage 0-4 harness can read, validate, project, score, and summarize realistic
issue/task records without mutating the source project or executing any work.

## Hard Boundaries

- Fixture input must live under `tests/fixtures/real_world_eval/`.
- Tests must read only copied fixture data from `tests/fixtures/`.
- Tests must not read from, write to, or mutate any external source project.
- Tests must not modify `docs/stage0/events.jsonl`.
- Tests must not call real models or provider APIs.
- Tests must not execute tasks, shell task bodies, sandboxes, containers, VMs, or
  concurrent workers.
- Tests must not install dependencies.
- Runtime modules must remain unchanged for the first pass.

## Allowed Existing APIs

The first pass may use only existing Stage 0-4 APIs:

- `validate_replay_preflight_check` or `replay_preflight`
- `replay_all`
- `generate_batch_digest`
- `TaskRecordStore`
- `FinalGateRunner`
- `ScoringEngine`

## Copied Fixture Format

Each copied real-world evaluation fixture is a self-contained directory:

```text
tests/fixtures/real_world_eval/<fixture_id>/
  README.md
  project_events.jsonl
  task-records/
    <task_id>/
      task_spec.json
      events.jsonl
      completion.json
      handoff_pack.json
      run_log.md
```

### `README.md`

Describes fixture provenance at a high level without linking to or requiring the
source project. It should state that the fixture is copied, sanitized, and safe
for read-only tests.

### `project_events.jsonl`

Contains project-level `event.v1` records accepted by the existing replay
preflight and projection APIs. Supported project-level event types for the first
pass are:

- `project_item_state_changed`
- `project_to_queue_handoff_created`
- `project_dependency_resolved`

The stream must be deterministic, valid JSONL, and free of duplicate
`event_id` values.

### `task-records/<task_id>/`

Contains a Stage 0-style task record bundle loadable by `TaskRecordStore`.

- `task_spec.json`: copied issue/task metadata, sanitized into a JSON object.
- `events.jsonl`: task-local event stream used as evidence only.
- `completion.json`: deterministic completion evidence; no execution occurs.
- `handoff_pack.json`: structured evidence and summary for final-gate review.
- `run_log.md`: copied/sanitized narrative evidence. It is never executed.

Task records may describe command-like text only as inert evidence. Tests must
not interpret or execute it.

## First Read-Only Test

The first test should:

1. Load `tests/fixtures/real_world_eval/project-alpha/project_events.jsonl`.
2. Run replay preflight validation.
3. Replay projections with `replay_all`.
4. Generate a batch digest with `generate_batch_digest`.
5. Load the copied task record with `TaskRecordStore`.
6. Evaluate the loaded bundle with `FinalGateRunner`.
7. Score the bundle with `ScoringEngine`.
8. Assert that fixture file bytes are unchanged before and after the read-only
   API calls.

This proves the existing harness can evaluate copied real-project-shaped data
without adding runtime behavior or mutating source fixtures.
