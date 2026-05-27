# Agent Instructions

This repository is the Token-Efficient Agent Harness Lab.

## Current Status

The Stage 0-4 task-book scope is complete.

- Stage 0: schema validation and manual workflow simulation complete.
- Stage 1: deterministic local runtime complete.
- Stage 2: quality runtime complete.
- Stage 3: controlled intelligence stubs complete.
- Stage 4: advanced runtime abstractions complete.
- Project closeout complete.
- CA-7 sealed baseline complete.
- Harness App MVP0-MVP8 complete (local operations console).
- Trial 0 closed — real target `PASS` verdict.
- Trial 1 closed — `ACCEPTABLE_FOR_MULTI_TASK_TRIAL_AFTER_HARDENING`.
- Reliability Hardening 1 complete (negated risk and triage differentiation).
- GitHub private repository published.

This project is now in post-closeout maintenance mode. The local dashboard is complete as a prototype. New UI, product, or production tracks still require explicit scope and human approval.

## Default Agent Behavior

Agents must not assume a new Stage 5 exists.

Before doing any work:

1. Inspect the current branch and working tree.
2. Run the test suite unless the task is documentation-only.
3. Confirm the requested task is inside the approved scope.
4. Ask for confirmation before starting any new product track.

## Hard Boundaries

Do not modify:

- docs/stage0/events.jsonl

Do not add without explicit human approval:

- real model API calls
- API keys or provider credentials
- real autonomous agents
- real sandbox/process/container/VM execution
- real concurrent workers
- Web UI implementation
- provider failover
- production deployment
- destructive filesystem operations
- Stage 5 implementation

## Current Safe Work Categories

Allowed by default:

- documentation cleanup
- README / roadmap / module map updates
- test-only improvements
- CI maintenance
- GitHub issue planning
- security review planning
- packaging planning
- architecture audit updates

Requires explicit approval:

- productionization
- real provider integration
- real sandbox execution
- UI/dashboard implementation
- benchmarking framework
- deployment work
- broad runtime refactors

## Test Command

Run:

PYTHONPATH=src python3 -m unittest discover -s tests

## Repository Principles

- Preserve deterministic behavior.
- Prefer small, reviewable commits.
- Keep tests passing.
- Do not expand architecture without documenting the new track first.
- Treat future work as optional tracks, not automatic continuation of the completed Stage 0-4 task book.
