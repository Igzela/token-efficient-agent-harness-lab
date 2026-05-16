# Token-Efficient Agent Harness Lab

## What This Project Is

Token-Efficient Agent Harness Lab is a local deterministic harness for studying event-sourced agent workflow infrastructure from Stage 0 through Stage 4. It includes JSONL event validation, projections, project/task workflow primitives, quality gates, controlled intelligence stubs, and Stage 4 runtime-control abstractions.

Current status: Stage 0-4 complete.

## What This Project Is Not

This repository is not a production autonomous-agent runtime. It does not call real model providers, run real agents, isolate work in real sandboxes, spawn production concurrent workers, provide provider failover, or implement a Web UI.

## How To Run Tests

```bash
PYTHONPATH=src python3 -m unittest discover -s tests
```

Current closeout result: 344 tests pass.

## How To Run The CLI

Example event validation command:

```bash
PYTHONPATH=src python3 -m harness_core.cli validate-events docs/stage0/events.jsonl
```

`docs/stage0/events.jsonl` intentionally contains a known bad line and is preserved as a validator fixture.

## Safety Boundaries

- No real model calls.
- No real agents.
- No real sandbox/process/container/VM isolation.
- No production concurrency or real concurrent workers.
- No provider failover.
- No Web UI implementation.
- No destructive runtime filesystem behavior.

## Repository Structure

```text
src/harness_core/        Python harness modules
tests/                   Deterministic unit tests and fixtures
docs/stage0/             Stage 0 architecture fixtures and task book data
docs/stage1/             Event store, validator, kernel, CLI, task-record docs
docs/stage2/             Quality runtime specs and acceptance
docs/stage3/             Controlled intelligence stub specs and acceptance
docs/stage4/             Runtime abstraction specs and acceptance
docs/MODULE_MAP.md       Module-to-stage reference
docs/ROADMAP.md          Completed stages and optional future tracks
docs/TEST_MATRIX.md      Test coverage matrix
```

## Stage Summary

- Stage 0: architecture, fixtures, task packs, and known validator issue.
- Stage 1: Event Store, JSONL Validator, projections, kernel, CLI, task records.
- Stage 2: scoring, gates, evaluation, baselines, trajectory, quality digest.
- Stage 3: advisor/model gateway stubs, routing, controlled eval, sampling, skills.
- Stage 4: DAG mutation, sandbox claims, scheduling, checkpoint/recovery planning, artifact lifecycle, health, dashboard data model.

## Next Recommended Work

Stop here for the completed task-book scope. Any next phase should be separately approved as productionization, real model provider integration, real sandbox execution, UI/dashboard implementation, deployment packaging, benchmarking, or security review.
