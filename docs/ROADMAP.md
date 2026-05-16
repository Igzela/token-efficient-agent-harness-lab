# Roadmap

## Completed Task-Book Scope

The completed scope is Stage 0 through Stage 4. No Stage 5 implementation has been started.

## Stage 0 — Foundation

Completed:

- Canonical architecture book.
- Stage 0 task fixtures and handoff packs.
- Project board and dependency graph documentation.
- Known bad `docs/stage0/events.jsonl` fixture retained for validator coverage.

## Stage 1 — Deterministic Harness Core

Completed:

- Event Store and minimal event schema.
- JSONL Validator and replay preflight checks.
- Projection Store.
- Kernel append contract.
- Project Board Manager.
- Task Queue Manager.
- CLI.
- BatchRunner.
- TaskRecordStore.
- FinalGateRunner.
- Stage 1 acceptance docs.

## Stage 2 — Quality Runtime

Completed:

- Scoring Engine.
- Artifact Gate.
- Quality Gate.
- Evaluation Runner.
- Baseline Manager.
- Trajectory Monitor.
- Quality Digest.
- Stage 2 acceptance docs.

## Stage 3 — Controlled Intelligence Stubs

Completed:

- Advisor Broker.
- Model Gateway Stub.
- Routing Experiment Manager.
- Controlled Model Eval Harness.
- Sampling Runner.
- Skill Extractor.
- Stage 3 acceptance docs.

## Stage 4 — Advanced Runtime Abstractions

Completed:

- Dynamic DAG Manager and DAG mutation records.
- Sandbox Manager file claims.
- Concurrency Controller scheduling.
- Runtime Supervisor.
- Checkpoint Manager and descriptive recovery plans.
- Artifact Lifecycle Manager.
- Health Monitor.
- DashboardSnapshot data model.
- Canonical Stage 4 specs and final acceptance report.

## Optional Future Tracks

These tracks are not part of the completed task-book scope:

- Productionization: durable storage, service boundaries, operational configuration.
- Real model provider integration: authenticated provider clients and policy controls.
- Real sandbox execution: OS/process/container isolation, cleanup, and security model.
- UI/dashboard: presentation layer using the Stage 4 dashboard data model.
- Packaging: installable distribution, versioning, release artifacts.
- Benchmarks: performance baselines and regression tracking.
- Security review: threat model, secret handling, supply-chain checks, sandbox policy.

Each future track requires separate approval and a new implementation plan.
