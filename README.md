# Token-Efficient Agent Harness Lab

## What This Project Is

Token-Efficient Agent Harness Lab is a local deterministic harness for studying event-sourced agent workflow infrastructure from Stage 0 through Stage 4. It includes JSONL event validation, projections, project/task workflow primitives, quality gates, controlled intelligence stubs, and Stage 4 runtime-control abstractions.

Current status: Stage 0-4 complete, Harness App MVP0-MVP8 complete, Trials 0-3 closed, Dispatch Kernel Phases 1-6B-3 stable, Phase 7 SDK + DocGenerator implemented, language migration Phase 0 wire schemas/golden parity implemented, security hardening complete (2081 tests).

**New sessions should start with [docs/SESSION_START_HERE.md](docs/SESSION_START_HERE.md).**

Coding agents may autonomously advance safe repository work inside the documented boundaries. They must keep `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`, and this README current after each commit-sized change, then run `python3 scripts/check_agent_handoff.py` before commit.

## What This Project Is Not

This repository is not a production autonomous-agent runtime. It does not call real model providers, run real agents, isolate work in real sandboxes, spawn production concurrent workers, provide provider failover, or ship a production Web UI. The local Harness app dashboard is read-only/non-executable over app-owned state.

## How To Run Tests

```bash
PYTHONPATH=src python3 -m unittest discover -s tests
```

Current result: 2081 tests pass.

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
- No production Web UI, deployment, or remote service.
- Local dashboard views remain non-executable and target repositories remain read-only.
- No destructive runtime filesystem behavior.

## Repository Structure

```text
src/harness_core/        Python harness modules
tests/                   Deterministic unit tests and fixtures
wire_contract/v1/        Frozen dispatch JSON schemas for Python/Rust parity
tests/integration/parity/ Stdlib parity runner for dispatch golden fixtures
docs/stage0/             Stage 0 architecture fixtures and task book data
docs/stage1/             Event store, validator, kernel, CLI, task-record docs
docs/stage2/             Quality runtime specs and acceptance
docs/stage3/             Controlled intelligence stub specs and acceptance
docs/stage4/             Runtime abstraction specs and acceptance
web/dashboard/           Local non-executable Harness app dashboard
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

## Harness App MVPs

- MVP0: read-only harness instance auditor.
- MVP1: static audit dashboard.
- MVP2: local read-only control plane.
- MVP3: deterministic non-executable planning kernel.
- MVP4: read-only plan review workbench for plan history, summary, comparison, and advisory review actions.
- MVP5: non-persistent review guidance preview for stored plans, evidence requirements, and token-efficiency guidance.
- MVP6: read-only planning portfolio triage for review priority, bottlenecks, and token hotspots.
- MVP7: read-only operations and debug dashboard for component status, data flow, storage health, recent errors, and debug actions.
- MVP8: operations console simplification that keeps the first screen focused on status, health, errors, and two primary actions while moving tools into collapsed sections.

Demo packaging: [`docs/demo/README.md`](docs/demo/README.md)

## CA-7 Sealed Baseline Status

Controlled Adaptive Orchestrator Kernel minimum threshold reached (CA-0 through CA-7 all passed). The current harness policy baseline is sealed. Future policy changes require the policy candidate lifecycle and governance approval path.

Full closeout report: [`docs/CA7_CONTROLLED_ADAPTIVE_CLOSEOUT_REPORT.md`](docs/CA7_CONTROLLED_ADAPTIVE_CLOSEOUT_REPORT.md)

## Next Recommended Work

Keep the repo moving through the autonomous maintainer loop: repair verification drift, keep docs current, fix focused regressions, and advance architecture-book-defined dispatch-kernel work when it remains deterministic, local, test-first, and free of real providers, real sandbox/process execution, target repo writes, deployment, or real worker processes. Any work that adds productionization, real model provider integration, real sandbox execution, approval/run controls, deployment, or a user-facing product surface still requires explicit approval.
