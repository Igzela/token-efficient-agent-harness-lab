# Project Architecture Audit

## 1. Stage Completion Status

| Stage | Status | Evidence |
| --- | --- | --- |
| Stage 0 | Complete | Canonical architecture book, task fixtures, known bad `docs/stage0/events.jsonl` validator fixture, and Stage 0 retrospective/readiness docs. |
| Stage 1 | Complete | Event Store, JSONL validation, kernel append contract, projections, project board, task queue, CLI, task records, and final acceptance docs. |
| Stage 2 | Complete | Quality runtime: scoring, artifact gate, quality gate, evaluation runner, baseline manager, trajectory monitor, quality digest, and final acceptance docs. |
| Stage 3 | Complete | Controlled intelligence stubs: advisor broker, model gateway stub, routing experiments, controlled model evaluation, sampling runner, skill extractor, and final acceptance docs. |
| Stage 4 | Complete | Advanced runtime abstractions: DAG mutation, sandbox claims, checkpoint/recovery planning, scheduler, artifact lifecycle, health aggregation, dashboard model, and final acceptance docs. |

## 2. Component Map

| Component | Module | Purpose |
| --- | --- | --- |
| Event Store | `event_store.py`, `event_schema.py` | Append-only JSONL event persistence, schema validation, idempotency, replay preflight. |
| Projection Store | `projection_store.py` | Replays validated event logs into project, queue, and dependency projections. |
| Project Board Manager | `project_board.py` | Deterministic project board transitions and allowed-file checks. |
| Task Queue Manager | `task_queue.py` | Handoff intake and task status transitions. |
| Validator Suite | `validators.py` | Stage 1 validation helpers for events, handoffs, completions, failure codes, approvals, and replay checks. |
| Digest | `digest.py`, `quality_digest.py` | Batch and quality digest generation. |
| CLI | `cli.py` | Local command entry points for validation workflows. |
| Kernel | `kernel.py` | Minimal event append contract over EventStore. |
| BatchRunner | `batch_runner.py` | Deterministic batch processing primitive. |
| TaskRecordStore | `task_records.py` | Task record persistence and validation. |
| FinalGateRunner | `final_gate.py` | Final decision gate for task/project completion. |
| Orchestrator | `orchestrator.py` | Stage 1 orchestration across kernel, queue, and records. |
| Scoring Engine | `scoring.py` | Deterministic scoring of artifacts, tasks, and runs. |
| Artifact Gate | `artifact_gate.py` | Stage 2 artifact validation gate. |
| Quality Gate | `quality_gate.py` | Stage 2 quality decision gate. |
| Evaluation Runner | `evaluation.py` | Local deterministic evaluation runner. |
| Baseline Manager | `baseline.py` | Baseline comparison records. |
| Trajectory Monitor | `trajectory.py` | Trajectory anomaly detection. |
| Quality Digest | `quality_digest.py` | Quality result summarization. |
| Advisor Broker | `advisor.py` | Stubbed advisor protocol and budget validation. |
| Model Gateway Stub | `model_gateway.py` | Stubbed model tier/capability gateway with no real provider calls. |
| Routing Experiment Manager | `routing.py` | Deterministic routing experiment records. |
| Controlled Model Eval Harness | `model_eval.py` | Controlled model evaluation harness using stubs. |
| Sampling Runner | `sampling.py` | Deterministic sampling candidate runner. |
| Skill Extractor | `skills.py` | Local skill extraction/library primitives. |
| DAG Manager | `dag_manager.py`, `dag_mutations.py` | Dynamic DAG state, mutation proposals, mutation records, approval checks, rollback records. |
| Sandbox Manager | `sandbox.py` | Logical sandbox and file-claim tracking. |
| Concurrency Controller | `concurrency.py` | Scheduling-only concurrency batch selection. |
| Runtime Supervisor | `supervisor.py` | Supplied worker health assessment and checkpoint coordination. |
| Checkpoint Manager | `checkpoint.py` | Deterministic JSON checkpoint persistence and descriptive recovery plans. |
| Artifact Lifecycle Manager | `artifact_lifecycle.py` | Artifact state transitions and dependency unlock records. |
| Health Monitor | `health.py` | Aggregates supplied component health. |
| Dashboard Data Model | `dashboard_model.py` | Read-only dashboard snapshot dataclass. |

## 3. Boundary Audit

Confirmed boundaries:

- No real model calls.
- No real agents.
- No real sandbox, process, container, or VM isolation.
- No real concurrent workers.
- No Web UI implementation.
- No provider failover.
- No destructive filesystem behavior in runtime modules.
- `docs/stage0/events.jsonl` is preserved as the known bad Stage 0 validator fixture.

Stage 3 and Stage 4 intentionally model control-plane concepts with deterministic stubs and records. They do not execute autonomous work.

## 4. Data Flow

```text
EventStore
  -> ProjectionStore
  -> Kernel
  -> BatchRunner
  -> TaskRecordStore
  -> FinalGateRunner
  -> Orchestrator
  -> Quality Runtime
  -> Controlled Intelligence Stubs
  -> Stage 4 Runtime Abstractions
```

Expanded flow:

- EventStore validates and appends canonical JSONL events.
- ProjectionStore replays valid events into derived project/queue/dependency state.
- Kernel gives the minimal append contract to higher-level orchestration.
- BatchRunner and TaskRecordStore provide deterministic local run structure.
- FinalGateRunner and Orchestrator coordinate completion decisions.
- Quality Runtime includes scoring, artifact/quality gates, evaluations, baselines, trajectory checks, and digests.
- Controlled Intelligence Stubs include advisor, model gateway, routing, model eval, sampling, and skills without real provider calls.
- Stage 4 Runtime Abstractions add DAG mutation, sandbox claims, scheduling, checkpoint/recovery planning, artifact lifecycle, health, and dashboard data.

## 5. Known Gaps

- Production persistence.
- Real provider integration.
- Real sandbox execution.
- Actual concurrent workers.
- UI implementation.
- Deployment packaging.
- Security hardening.
- Performance benchmarking.
- External API integration.

These gaps are outside the Stage 0-4 task-book scope.

## 6. Recommendation

The project is complete as a local deterministic harness lab. The next phase should be separately approved as productionization or real-provider integration, not treated as automatic Stage 5.
