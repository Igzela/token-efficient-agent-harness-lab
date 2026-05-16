# Stage 1 Week 2 CLI Plan

## Goal

Add a minimal read-only command-line wrapper around the Stage 1 Week 1 runtime library.

The CLI should let an operator validate event streams and inspect projections from a terminal without introducing model calls, agent execution, Web UI, provider failover, concurrency, or new scheduling behavior.

## Scope

In scope:

- CLI module runnable with `python3 -m harness_core.cli`
- `validate-events <path>`
- `project-state <path>`
- `task-queue <path>`
- `digest <path>`
- stdlib-only implementation
- read-only access to event files

Out of scope:

- Web UI
- model calls
- runtime loop
- real agent execution
- provider failover
- routing optimizer
- dynamic DAG mutation
- concurrency
- packaging metadata or console entrypoint

## Files

Create:

- `src/harness_core/cli.py`
- `tests/test_cli.py`

Do not add `pyproject.toml` yet. Use:

```bash
PYTHONPATH=src python3 -m harness_core.cli <command> <path>
```

## Commands

### validate-events

```bash
python3 -m harness_core.cli validate-events <path>
```

Behavior:

1. Run `validate_jsonl_file(path)` first.
2. If JSONL validation has errors, print them and exit nonzero.
3. Do not run `replay_preflight(path)` after invalid JSONL unless `--verbose` is provided.
4. If JSONL validation passes, run `replay_preflight(path)`.
5. Exit nonzero if preflight reports errors.

### project-state

```bash
python3 -m harness_core.cli project-state <path>
```

Behavior:

- Run projection replay through `replay_project_state(path)`.
- Print Project Board item statuses sorted by `item_id`.
- Exit nonzero if replay preflight fails.

### task-queue

```bash
python3 -m harness_core.cli task-queue <path>
```

Behavior:

- Run `replay_task_queue_state(path)`.
- Print handoff records in append order.
- Include `handoff_id`, `item_id`, `scheduling_policy`, and `event_id`.
- Exit nonzero if replay preflight fails.

### digest

```bash
python3 -m harness_core.cli digest <path>
```

Behavior:

- Run `replay_all(path)`.
- Run `generate_batch_digest(projections)`.
- Print completed, blocked, and failed items plus handoff and resolved dependency counts.
- Keep this as a stub summary, not full digest YAML rendering.

## Output

Default output is deterministic plain text.

`--json` is allowed only if it remains simple, deterministic, and fully tested with `json.loads`. If it complicates implementation, defer it as a TODO.

Errors go to stderr. Successful command output goes to stdout.

## Tests

Use `unittest` and invoke the CLI through:

```bash
PYTHONPATH=src python3 -m harness_core.cli ...
```

Required tests:

- `validate-events` passes `tests/fixtures/stage0_events_sanitized.jsonl`
- `validate-events` fails `tests/fixtures/stage0_events_with_line17_issue.jsonl`
- non-verbose `validate-events` does not emit duplicate preflight diagnostics after JSONL failure
- verbose `validate-events` may include preflight diagnostics
- `project-state` shows all five Stage 0 items as `done`
- `task-queue` shows handoffs for `item_003`, `item_004`, and `item_005`
- `digest` shows five completed items, handoff count `3`, and resolved dependency count `2`
- CLI exits nonzero on invalid input path

## Git Plan

Commit 1:

- `docs/stage1/week2_cli_plan.md` only

Commit 2:

- `src/harness_core/cli.py`
- `tests/test_cli.py`

Before each commit:

```bash
PYTHONPATH=src python3 -m unittest discover -s tests
git status --porcelain
```
