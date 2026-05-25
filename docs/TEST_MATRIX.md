# Test Matrix

## Summary

Command:

```bash
PYTHONPATH=src python3 -m unittest discover -s tests
```

Current result: 897 tests pass.

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
| `tests/test_real_world_eval.py` | Real-World Read-Only Evaluation Track fixtures |
| `tests/test_app_api.py` | Local app repo registry and read-only audit API |
| `tests/test_resource_planner.py` | MVP3 deterministic non-executable resource planner |
| `tests/test_app_api_plans.py` | MVP3 plan API and app-owned plan store boundaries |
| `tests/test_plan_workbench.py` | MVP4 read-only plan summaries, comparisons, and review actions |
| `tests/test_app_api_plan_workbench.py` | MVP4 plan workbench and MVP5 review guidance API endpoints plus target-repo read-only regression |
| `tests/test_review_guidance.py` | MVP5 non-persistent review guidance derivation and boundary checks |
| `tests/test_plan_triage.py` | MVP6 read-only portfolio triage, review priority, bottlenecks, and token hotspots |
| `tests/test_app_api_plan_triage.py` | MVP6 plan triage API endpoint, filtering, limit validation, and read-only regressions |
| `tests/test_app_diagnostics.py` | MVP7 read-only app diagnostics, component status, storage health, and recent derived errors |
| `tests/test_app_api_diagnostics.py` | MVP7 app diagnostics API endpoints and read-only regressions |
| `tests/test_dashboard_static.py` | Dashboard non-executable guidance wording, MVP8 operations console layout guard, minimal first-screen action guard, and button-label guard |

## Fixture Coverage

| Fixture | Purpose |
| --- | --- |
| `tests/fixtures/stage0_events_with_line17_issue.jsonl` | Preserves the Stage 0 known bad line 17 issue as a validator fixture. |
| `tests/fixtures/stage0_events_sanitized.jsonl` | Valid sanitized Stage 0 event stream for replay/projection tests. |
| `tests/fixtures/real_world_eval/project-alpha/` | First-pass copied real-project-shaped read-only evaluation fixture. |
| `tests/fixtures/real_world_eval/doc-update-project/` | Documentation-only copied fixture shape. |
| `tests/fixtures/real_world_eval/bugfix-project/` | Bugfix copied fixture shape with artifact and scoring evidence. |
| `tests/fixtures/real_world_eval/config-rule-project/` | Config/rule-change copied fixture shape with file policy evidence. |
| `tests/fixtures/real_world_eval/failure-fix-loop-project/` | Failure/fix-loop copied fixture shape with canonical failure code evidence. |
| `tests/fixtures/real_world_eval/cross-task-dependency-project/` | Multi-item copied fixture shape with dependency resolution evidence. |
| `tests/fixtures/README.md` | Fixture notes. |

`docs/stage0/events.jsonl` is not modified by tests and remains the original known-bad source fixture.

## Real-World Read-Only Evaluation Track

This post-closeout optional track is covered by `tests/test_real_world_eval.py`.
It is not Stage 5 and does not change runtime behavior.

| Coverage | Details |
| --- | --- |
| Fixtures | `project-alpha`, `doc-update-project`, `bugfix-project`, `config-rule-project`, `failure-fix-loop-project`, `cross-task-dependency-project` |
| Components | Replay preflight, projections, batch digest, task records, final gate, artifact gate, scoring, quality gate, quality digest, selected validators |
| Boundaries | Read-only committed fixtures; no model calls; no task execution; no sandbox execution; no external project mutation; no `docs/stage0/events.jsonl` changes |

## Component Coverage By Stage

| Stage | Coverage |
| --- | --- |
| Stage 1 | Event store, validators, projections, CLI, kernel, batch runner, task records, final gate, orchestrator. |
| Stage 2 | Scoring, artifact gate, quality gate, evaluation, baseline, trajectory, quality digest. |
| Stage 3 | Advisor broker, model gateway stub, routing, model eval harness, sampling, skill extractor. |
| Stage 4 | DAG manager/mutations, sandbox claims, concurrency scheduler, supervisor, checkpoint/recovery, artifact lifecycle, health, dashboard model. |
| Harness App | Read-only local app API, deterministic non-executable planning, app-owned plan store, plan review workbench, non-persistent review guidance preview, portfolio triage, operations diagnostics, operations console simplification, static dashboard boundary checks. |

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
