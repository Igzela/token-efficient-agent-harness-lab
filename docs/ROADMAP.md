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

## Completed Harness App Prototype

Completed after the Stage 0-4 task-book scope:

- Harness App MVP0: read-only harness instance auditor.
- Harness App MVP1: static audit dashboard.
- Harness App MVP2: local read-only control plane.
- Harness App MVP3: deterministic non-executable planning kernel.
- Harness App MVP4: read-only plan review workbench.
- Harness App MVP5: non-persistent review guidance preview.
- Harness App MVP6: read-only planning portfolio triage.
- Harness App MVP7: read-only operations and debug dashboard.
- Harness App MVP8: operations console simplification.
- CA-7 sealed baseline.
- Trial 0: real target acceptance — `PASS` verdict.
- Trial 1: multi-task budget validation — `ACCEPTABLE_FOR_MULTI_TASK_TRIAL_AFTER_HARDENING`.
- Reliability Hardening 1: negated risk handling and triage differentiation.

## Optional Future Tracks

These tracks are not part of the completed scope and require separate approval:

- Productionization: durable storage, service boundaries, operational configuration.
- Real model provider integration: authenticated provider clients and policy controls.
- Real sandbox execution: OS/process/container isolation, cleanup, and security model.
- Further UI/dashboard work beyond the existing local operations console.
- Packaging: installable distribution, versioning, release artifacts.
- Benchmarks: performance baselines and regression tracking.
- Security review: threat model, secret handling, supply-chain checks, sandbox policy.
- Trial 2 on additional real local projects.

Each future track requires separate approval and a new implementation plan.
