# Stage 4 — Advanced Runtime Plan

## Purpose

Stage 4 adds deterministic runtime-control abstractions on top of the completed Stage 0-3 harness. The goal is to plan and audit advanced runtime behavior without crossing into uncontrolled autonomy, real process supervision, real sandboxing, real concurrent execution, Web UI work, provider failover, or live model calls.

## Canonical Planning Documents

The Stage 4 audit trail is defined by this plan and four detailed specs:

- `dag_mutation_spec.md` — Dynamic DAG Manager and DAG Mutation Protocol
- `sandbox_concurrency_spec.md` — Sandbox Manager file claims and Concurrency Controller scheduling
- `supervisor_recovery_spec.md` — Runtime Supervisor, health records, checkpoints, and descriptive recovery
- `artifact_lifecycle_dashboard_spec.md` — Artifact lifecycle, runtime health aggregation, and dashboard data model

Non-canonical predecessor drafts must not remain in `docs/stage4` after reconciliation.

## Scope

Stage 4 includes:

- Dynamic DAG Manager and auditable DAG mutation protocol
- File-claim sandbox abstraction with conflict detection
- Scheduling-only concurrency controller
- Runtime supervisor data model and deterministic health assessment
- JSON checkpoint save/load/latest lookup and descriptive recovery plans
- Artifact lifecycle state machine and dependency unlock records
- Health report aggregation
- Dashboard snapshot data model only
- Integration audit and final acceptance report

## Explicit Non-Goals

Stage 4 does not include:

- Real sandbox execution, process isolation, containers, or VMs
- Real worker process execution, killing, or restarting
- Real concurrent worker spawning
- Arbitrary shell command execution from tasks
- Real model API calls or provider failover
- Web UI implementation
- Production deployment
- Broad rewrites of Stage 1-3 modules

## Implementation Steps

1. Create the `stage4-advanced-runtime` branch.
2. Commit the canonical planning/spec documents.
3. Implement Dynamic DAG Manager and mutation protocol.
4. Implement Sandbox Manager file claims.
5. Implement Runtime Supervisor and Checkpoint/Recovery.
6. Implement Concurrency Controller scheduling.
7. Implement Artifact Lifecycle, Health Monitor, and DashboardSnapshot.
8. Run integration audit.
9. Commit final acceptance report.
10. Stop; do not automatically start Stage 5.

## Design Constraints

- All state-changing operations must be auditable.
- Rollback means a compensating event or mutation, not deleting or rewriting history.
- Determinism is required: no randomness, no wall-clock dependency in tests, and no external network calls.
- Tests may use `tempfile.TemporaryDirectory`; production-like filesystem mutation is out of scope.
- `docs/stage0/events.jsonl` is immutable and remains a fixture with its known bad line.
- All components are data/control abstractions unless a later stage explicitly approves execution behavior.

## Exit Criteria

Stage 4 can be accepted when:

- The canonical planning docs are committed.
- Each Stage 4 component has focused unit tests.
- Full test suite passes with `PYTHONPATH=src python3 -m unittest discover -s tests`.
- Git status has no unexpected untracked or modified Stage 4 files.
- The final acceptance report documents scope boundaries, commits, test result, known gaps, and recommended next stage.
