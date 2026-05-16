# Stage 1 Week 2 Acceptance Report

## 1. Acceptance Summary

**Decision:** ACCEPTED.

Stage 1 Week 2 added a minimal read-only CLI wrapper around the Week 1 runtime library. The CLI is runnable with:

```bash
PYTHONPATH=src python3 -m harness_core.cli <command> <path>
```

Pre-report verification:

```text
PYTHONPATH=src python3 -m unittest discover -s tests
Ran 65 tests in 0.291s
OK
```

Current branch at acceptance: `stage1-week2`.

Working tree before this report: clean.

## 2. CLI Commands Implemented

### validate-events

Validates an event JSONL file by running `validate_jsonl_file(path)` first.

- Exits `0` when JSONL validation and replay preflight pass.
- Exits nonzero when JSONL validation fails.
- Does not run replay preflight after invalid JSONL unless `--verbose` is provided.

### project-state

Replays project state from a validated event stream and prints item status rows.

Output format:

```text
item_id status last_event_id
```

### task-queue

Replays task queue handoff projection and prints handoff rows.

Output format:

```text
handoff_id item_id scheduling_policy event_id
```

### digest

Runs the Week 1 digest stub from projections and prints a deterministic plain-text summary.

Output includes:

- completed items
- blocked or waiting approval items
- failed items
- handoff count
- resolved dependency count

## 3. Smoke Test Results

Smoke checks passed during the Week 2 CLI integration audit:

- `validate-events tests/fixtures/stage0_events_sanitized.jsonl` exits `0`.
- `validate-events tests/fixtures/stage0_events_with_line17_issue.jsonl` exits nonzero and reports line 17 `InvalidJsonLineError`.
- `project-state tests/fixtures/stage0_events_sanitized.jsonl` exits `0` and shows all five items as `done`.
- `task-queue tests/fixtures/stage0_events_sanitized.jsonl` exits `0` and shows handoffs for `item_003`, `item_004`, and `item_005`.
- `digest tests/fixtures/stage0_events_sanitized.jsonl` exits `0` and summarizes completed project state, handoff count `3`, and resolved dependency count `2`.

## 4. Commits

```text
c8c80a8 Plan Stage 1 Week 2 CLI
cc18a61 Implement Stage 1 Week 2 CLI wrapper
```

## 5. Known Gaps Not To Fix Yet

- No `--json`.
- No console script entrypoint.
- No `pyproject.toml`.
- Digest remains stub.
- No CLI config file.
- No colored/rich output.

These are intentional Week 2 gaps and should not be fixed until there is a concrete need.

## 6. Recommendation

Week 2 is accepted.

Recommended Week 3 focus: Kernel loop / batch runner skeleton.

Do not add model calls, Web UI, real agents, provider failover, or concurrency yet.
