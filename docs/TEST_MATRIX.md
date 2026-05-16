# Test Matrix

## Summary

Command:

```bash
PYTHONPATH=src python3 -m unittest discover -s tests
```

Current result: 344 tests pass.

## Test Files

| Test file | Component coverage |
| --- | --- |
| `tests/test_event_store.py` | EventStore, JSONL validation, idempotency, replay preflight |
| `tests/test_projection_store.py` | Projection replay for project, queue, dependencies |
| `tests/test_project_board.py` | Project board transitions and final gate helper |
| `tests/test_task_queue.py` | Queue handoff and task transitions |
| `tests/test_validators.py` | Validator suite |
| `tests/test_digest.py` | Batch digest |
| `tests/test_cli.py` | CLI validation commands |
| `tests/test_kernel.py` | Kernel append contract |
| `tests/test_batch_runner.py` | BatchRunner |
| `tests/test_task_records.py` | TaskRecordStore |
| `tests/test_final_gate.py` | FinalGateRunner |
| `tests/test_orchestrator.py` | Stage1Orchestrator |
| `tests/test_scoring.py` | ScoringEngine |
| `tests/test_artifact_gate.py` | ArtifactGate |
| `tests/test_quality_gate.py` | QualityGateManager |
| `tests/test_evaluation.py` | EvaluationRunner |
| `tests/test_baseline.py` | BaselineManager |
| `tests/test_trajectory.py` | TrajectoryMonitor |
| `tests/test_quality_digest.py` | QualityDigestGenerator |
| `tests/test_advisor.py` | AdvisorBroker and stub advisor provider |
| `tests/test_model_gateway.py` | ModelGateway stub |
| `tests/test_routing.py` | RoutingExperimentManager |
| `tests/test_model_eval.py` | ControlledModelEvalHarness |
| `tests/test_sampling.py` | SamplingRunner |
| `tests/test_skills.py` | SkillExtractor and skill storage |
| `tests/test_dag_manager.py` | DAGManager and mutation proposals |
| `tests/test_dag_mutations.py` | DAGMutation records, approval, limits, compensating mutations |
| `tests/test_sandbox.py` | SandboxManager file claims |
| `tests/test_concurrency.py` | ConcurrencyController scheduling |
| `tests/test_supervisor.py` | RuntimeSupervisor health and recovery descriptions |
| `tests/test_checkpoint.py` | CheckpointManager persistence, integrity, recovery planning |
| `tests/test_artifact_lifecycle.py` | ArtifactLifecycleManager transitions and dependency unlocks |
| `tests/test_health.py` | HealthMonitor aggregation |
| `tests/test_dashboard_model.py` | DashboardSnapshot |

## Fixture Coverage

| Fixture | Purpose |
| --- | --- |
| `tests/fixtures/stage0_events_with_line17_issue.jsonl` | Preserves the Stage 0 known bad line 17 issue as a validator fixture. |
| `tests/fixtures/stage0_events_sanitized.jsonl` | Valid sanitized Stage 0 event stream for replay/projection tests. |
| `tests/fixtures/README.md` | Fixture notes. |

`docs/stage0/events.jsonl` is not modified by tests and remains the original known-bad source fixture.

## Component Coverage By Stage

| Stage | Coverage |
| --- | --- |
| Stage 1 | Event store, validators, projections, CLI, kernel, batch runner, task records, final gate, orchestrator. |
| Stage 2 | Scoring, artifact gate, quality gate, evaluation, baseline, trajectory, quality digest. |
| Stage 3 | Advisor broker, model gateway stub, routing, model eval harness, sampling, skill extractor. |
| Stage 4 | DAG manager/mutations, sandbox claims, concurrency scheduler, supervisor, checkpoint/recovery, artifact lifecycle, health, dashboard model. |

## Known Missing Production Tests

The following are intentionally absent because they are outside the Stage 0-4 local deterministic harness scope:

- Real model provider integration tests.
- Real agent execution tests.
- Real sandbox/container/process isolation tests.
- Actual concurrent worker/thread/process tests.
- Web UI/browser tests.
- Deployment packaging tests.
- External API integration tests.
- Production persistence migration tests.
- Security hardening and penetration tests.
- Performance benchmark regression tests.
