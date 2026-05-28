# Module Map

| Module | Stage | Purpose | Main public APIs | Related tests |
| --- | --- | --- | --- | --- |
| `event_schema.py` | Stage 1 | Minimal event schema and canonical hashing. | `validate_event`, `stable_idempotency_hash` | `test_event_store.py`, `test_validators.py` |
| `event_store.py` | Stage 1 | Append-only JSONL Event Store and replay preflight. | `EventStore`, `validate_jsonl_file`, `replay_preflight`, `load_event_ids` | `test_event_store.py` |
| `projection_store.py` | Stage 1 | Replays valid events into derived projections. | `replay_project_state`, `replay_task_queue_state`, `replay_dependency_state`, `replay_all` | `test_projection_store.py` |
| `project_board.py` | Stage 1 | Project board item transitions and final gate helper. | `ProjectBoardItem`, `transition_item`, `complete_task_to_review`, `final_gate` | `test_project_board.py` |
| `task_queue.py` | Stage 1 | Queue handoff and task status transitions. | `TaskQueueEntry`, `receive_handoff`, `transition_task` | `test_task_queue.py` |
| `validators.py` | Stage 1 | Validation suite for events, handoffs, completions, approvals. | `ValidationResult`, `validate_events_schema`, `validate_handoff_pack`, `validate_completion_record` | `test_validators.py` |
| `digest.py` | Stage 1 | Batch digest generation. | `BatchDigest`, `generate_batch_digest` | `test_digest.py` |
| `cli.py` | Stage 1 | Local command entry points. | `main`, CLI subcommands | `test_cli.py` |
| `kernel.py` | Stage 1 | Minimal event append contract. | `Kernel` | `test_kernel.py` |
| `batch_runner.py` | Stage 1 | Deterministic batch runner. | `BatchRunner`, `RunResult` | `test_batch_runner.py` |
| `task_records.py` | Stage 1 | Task record storage and validation. | `TaskRecordStore`, `TaskRecordBundle`, `TaskRecordValidationReport` | `test_task_records.py` |
| `final_gate.py` | Stage 1 | Final task/project gate decision runner. | `FinalGateRunner`, `FinalGateDecision` | `test_final_gate.py` |
| `orchestrator.py` | Stage 1 | Stage 1 orchestration primitive. | `Stage1Orchestrator`, `OrchestrationResult` | `test_orchestrator.py` |
| `scoring.py` | Stage 2 | Deterministic scoring engine. | `ScoringEngine`, `RunScore`, `TaskScore`, `ArtifactScore` | `test_scoring.py` |
| `artifact_gate.py` | Stage 2 | Artifact quality gate. | `ArtifactGate`, `ArtifactCheck`, `ArtifactGateResult` | `test_artifact_gate.py` |
| `quality_gate.py` | Stage 2 | Quality gate decision manager. | `QualityGateManager`, `QualityGateDecision` | `test_quality_gate.py` |
| `evaluation.py` | Stage 2 | Deterministic evaluation runner. | `EvaluationRunner`, `EvalSpec`, `EvalCase`, `EvaluationReport` | `test_evaluation.py` |
| `baseline.py` | Stage 2 | Baseline comparison records. | `BaselineManager`, `BaselineRecord`, `BaselineComparison` | `test_baseline.py` |
| `trajectory.py` | Stage 2 | Trajectory anomaly detection. | `TrajectoryMonitor`, `TrajectoryReport`, `TrajectoryAnomaly` | `test_trajectory.py` |
| `quality_digest.py` | Stage 2 | Quality result summary. | `QualityDigestGenerator`, `QualityDigest`, `QualityDigestItem` | `test_quality_digest.py` |
| `advisor.py` | Stage 3 | Advisor broker and stub provider. | `AdvisorBroker`, `StubAdvisorProvider`, `AdvisorProtocolValidator` | `test_advisor.py` |
| `model_gateway.py` | Stage 3 | Stubbed model gateway and capability registry. | `ModelGateway`, `StubModelProvider`, `ModelCapabilityRegistry` | `test_model_gateway.py` |
| `routing.py` | Stage 3 | Routing experiment records and reports. | `RoutingExperimentManager`, `RoutingExperimentSpec`, `RoutingPolicy` | `test_routing.py` |
| `model_eval.py` | Stage 3 | Controlled model eval harness using stubs. | `ControlledModelEvalHarness`, `ModelEvalCase`, `ModelEvalReport` | `test_model_eval.py` |
| `sampling.py` | Stage 3 | Deterministic sampling runner. | `SamplingRunner`, `SamplingCandidate`, `SamplingReport` | `test_sampling.py` |
| `skills.py` | Stage 3 | Skill extraction and library primitives. | `SkillExtractor`, `SkillStore`, `SkillLibrary`, `SkillRecord` | `test_skills.py` |
| `dag_manager.py` | Stage 4 | Dynamic DAG state and manager operations. | `DAGManager`, `DAGNode`, `DAGEdge`, `DAGState`, `DAGMutationProposal` | `test_dag_manager.py` |
| `dag_mutations.py` | Stage 4 | Auditable DAG mutation records and helpers. | `DAGMutation`, `DAGMutationLimits`, `validate_dag_mutation`, `create_compensating_mutation` | `test_dag_mutations.py` |
| `sandbox.py` | Stage 4 | Logical sandbox file-claim tracking. | `SandboxManager`, `Sandbox`, `FileClaim`, `ConflictReport` | `test_sandbox.py` |
| `concurrency.py` | Stage 4 | Scheduling-only concurrency controller. | `ConcurrencyController`, `ScheduleBatch`, `FileOverlap` | `test_concurrency.py` |
| `supervisor.py` | Stage 4 | Supplied worker health and checkpoint coordination. | `RuntimeSupervisor`, `WorkerHealth`, `SupervisorReport`, `ComponentHealth` | `test_supervisor.py` |
| `checkpoint.py` | Stage 4 | JSON checkpoint persistence and recovery planning. | `CheckpointManager`, `Checkpoint`, `RecoveryPlan`, `IntegrityCheck` | `test_checkpoint.py` |
| `artifact_lifecycle.py` | Stage 4 | Artifact transition state machine. | `ArtifactLifecycleManager`, `ArtifactRecord`, `ArtifactTransition`, `DependencyUnlock` | `test_artifact_lifecycle.py` |
| `health.py` | Stage 4 | Component health aggregation. | `HealthMonitor`, `HealthReport` | `test_health.py` |
| `dashboard_model.py` | Stage 4 | Read-only dashboard snapshot model. | `DashboardSnapshot` | `test_dashboard_model.py` |
| `app_registry.py` | Harness App | Local app-owned repository registry. | `AppRegistry`, `RepoRef` | `test_app_api.py`, `test_app_api_plans.py` |
| `app_api.py` | Harness App | Pure local API handlers for repo audit, deterministic plans, plan review views, review guidance previews, portfolio triage, and app diagnostics. | `handle_api_request`, `default_plan_store_path` | `test_app_api.py`, `test_app_api_plans.py`, `test_app_api_plan_workbench.py`, `test_app_api_plan_triage.py`, `test_app_api_diagnostics.py` |
| `app_diagnostics.py` | Harness App MVP7 | Read-only app diagnostics for component status, data flow, storage health, recent errors, and debug actions. | `build_app_status`, `build_app_diagnostics`, `derive_recent_errors` | `test_app_diagnostics.py`, `test_app_api_diagnostics.py` |
| `instance_audit.py` | Harness App | Read-only target repository harness-instance audit. | `audit_instance`, `InstanceAuditReport` | `test_instance_audit.py`, `test_app_api.py` |
| `resource_planner.py` | Harness App MVP3 | Deterministic non-executable resource planning. | `DeterministicResourcePlanner`, `PlanningTask`, `ResourcePlan` | `test_resource_planner.py`, `test_app_api_plans.py` |
| `plan_store.py` | Harness App MVP3 | App-owned append-only plan store. | `load_plans`, `save_plan`, `get_plan` | `test_app_api_plans.py`, `test_app_api_plan_workbench.py` |
| `plan_workbench.py` | Harness App MVP4 | Read-only derived plan history, summary, comparison, and review actions. | `list_plan_summaries`, `summarize_plans`, `compare_plans`, `recommend_next_review_action` | `test_plan_workbench.py`, `test_app_api_plan_workbench.py` |
| `review_guidance.py` | Harness App MVP5 | Non-persistent guidance preview derived from stored non-executable plans. | `build_review_guidance`, `derive_review_options`, `derive_evidence_requirements`, `derive_token_efficiency_guidance` | `test_review_guidance.py`, `test_app_api_plan_workbench.py` |
| `plan_triage.py` | Harness App MVP6 | Read-only portfolio triage derived from stored non-executable plans. | `build_portfolio_triage`, `triage_plan`, `classify_plan_bottleneck`, `derive_token_hotspots`, `compute_review_priority` | `test_plan_triage.py`, `test_app_api_plan_triage.py` |
| `errors.py` | Stage 1 | Shared exception classes. | Error classes | Covered through component tests |
| `__init__.py` | Stage 1-4 | Public package export surface. | Re-exported harness APIs | Import coverage across tests |
| `orchestration/schemas.py` | Phase 5 | Frozen dataclasses for workflow graph, nodes, edges, agent roles, conflict records. | `WorkflowGraph`, `WorkflowNode`, `WorkflowEdge`, `AgentRole`, `AgentMessage`, `ConflictRecord` | `test_orchestration_schema.py` |
| `orchestration/agent_role_registry.py` | Phase 5 | Agent role registration, lookup, assignment with concurrency tracking. | `AgentRoleRegistry` | `test_orchestration_registry.py` |
| `orchestration/task_decomposer.py` | Phase 5 | Rule-based TaskAnalysis to WorkflowGraph decomposition (1/2/4 node graphs). | `TaskDecomposer` | `test_orchestration_decomposer.py` |
| `orchestration/dependency_resolver.py` | Phase 5 | Cycle detection (DFS), topological sort, ready-nodes computation. | `DependencyResolver` | `test_orchestration_decomposer.py` |
| `orchestration/work_queue.py` | Phase 5 | Stateless node queue operating on WorkflowGraph as source of truth. | `WorkQueue` | `test_orchestration_work_queue.py` |
| `orchestration/workflow_engine.py` | Phase 5 | Full workflow lifecycle: decompose, execute, resolve, aggregate. | `WorkflowEngine` | `test_orchestration_workflow_engine.py`, `test_orchestration_integration.py`, `test_orchestration_hardening.py` |
| `orchestration/conflict_resolver.py` | Phase 5 | Detects and resolves output, resource, dependency, and budget conflicts. | `ConflictResolver` | `test_orchestration_conflict.py` |
| `orchestration/result_aggregator.py` | Phase 5 | Combines completed node outputs into a final workflow result dict. | `ResultAggregator` | `test_orchestration_integration.py`, `test_orchestration_result_aggregator.py` |
| `orchestration/human_approval_gate.py` | Phase 5 | Checkpoints for human review; triggers on budget threshold or failure. | `HumanApprovalGate` | `test_orchestration_hardening.py`, `test_orchestration_human_approval_gate.py` |
| `orchestration/multi_agent_budget.py` | Phase 5 | Workflow/agent/node-level budget enforcement with overrun strategies. | `MultiAgentBudgetManager` | `test_orchestration_budget.py` |
| `orchestration/__init__.py` | Phase 5 | Barrel re-exports for orchestration package. | Re-exported orchestration APIs | Import coverage across tests |
| `observability.py` | Phase 6A | Structured logging, metrics collector, request tracing. | `StructuredFormatter`, `MetricsCollector`, `RequestTracer`, `setup_structured_logging` | `test_observability.py` |
| `durable_store.py` | Phase 6A | SQLite-backed durable storage for plans, repos, events. | `DurableStore`, `StoredRecord` | `test_durable_store.py` |
| `storage_migrator.py` | Phase 6A | JSON/JSONL → SQLite batch migration. | `migrate_plans_json_to_sqlite`, `migrate_repos_json_to_sqlite`, `migrate_events_jsonl_to_sqlite`, `full_migration`, `MigrationReport`, `FullMigrationReport` | `test_storage_migrator.py` |
| `http_server.py` | Phase 6A | Stdlib HTTP server with route dispatch. | `HarnessHTTPHandler`, `ServerConfig`, `register_route`, `create_server`, `start_server_in_thread` | `test_http_server.py` |
| `health_checker.py` | Phase 6A | Health and readiness probes for storage. | `HealthChecker`, `HealthCheck`, `HealthReport` | `test_health_checker.py` |
| `sdk.py` | Phase 7 | Python SDK for programmatic integration. | `HarnessSDK`, `SDK_SCHEMA_VERSION` | `test_sdk.py` |
| `doc_generator.py` | Phase 7 | Auto-generated markdown docs from source schemas. | `DocGenerator`, `DOC_GENERATOR_SCHEMA_VERSION` | `test_doc_generator.py` |
| `community_profiles.py` | Phase 7 (P7-T3) | Community model profile registry with validation and search. | `CommunityProfileRegistry`, `ModelProfile`, `COMMUNITY_PROFILE_SCHEMA_VERSION` | `test_community_profiles.py` |
| `tool_adapter.py` | Phase 7 (P7-T4) | External tool registration and stub execution. | `ToolAdapterManager`, `ToolDefinition`, `ToolExecutionRequest`, `TOOL_ADAPTER_SCHEMA_VERSION` | `test_tool_adapter.py` |
| `dashboard.py` | Phase 7 (P7-T5) | Experiment tracking, search, summary computation. | `DispatchDashboard`, `ExperimentResult`, `DashboardSummary`, `DASHBOARD_SCHEMA_VERSION` | `test_dashboard.py` |
| `benchmark.py` | Phase 7 (P7-T8) | Model comparison benchmarks, leaderboard, task/result CRUD. | `BenchmarkSuite`, `BenchmarkTask`, `BenchmarkResult`, `BENCHMARK_SCHEMA_VERSION` | `test_benchmark.py` |
| `wire_contract/v1/*.schema.json` | Language Migration Phase 0 | Frozen dispatch JSON schemas for Python/Rust semantic parity. | `dispatch_request`, `task_analysis`, `dispatch_decision`, `execution_result`, `evaluation_result`, `dispatch_bundle` schemas | `test_dispatch_wire_contract.py`, `tests/integration/parity/run.py` |
| `tests/integration/parity/` | Language Migration Phase 0 | Stdlib-only Python reference parity runner and golden fixture generator. | `run_parity_checks`, `write_python_golden_fixtures`, `normalize_dynamic_values` | `test_dispatch_wire_contract.py` |
| `engine/src/runtime.rs` | Language Migration Phase 1 | Deterministic Rust fixture runtime for stable timestamps and IDs. | `FixtureRuntime`, `FIXTURE_TIMESTAMP` | `engine/tests/dispatch_parity.rs` |
| `engine/src/event_schema.rs` | Language Migration Phase 1 | Rust event.v1 validation, canonical JSON, and stable idempotency hash helpers. | `validate_event`, `canonical_event_json`, `stable_idempotency_hash` | `engine/tests/dispatch_parity.rs` |
| `engine/src/task_analyzer.rs` | Language Migration Phase 1 | Rust rule-based task analyzer parity implementation. | `RuleBasedTaskAnalyzer`, `TaskAnalysis`, `analyze` | `engine/tests/dispatch_parity.rs` |
| `engine/src/dispatch_decision.rs` | Language Migration Phase 1 | Rust dispatch decision schema structs used by the parity engine. | `DispatchDecision`, `BudgetReservation`, `ExecutionGate`, `build_dispatch_bundle` | `engine/tests/dispatch_parity.rs` |
| `engine/src/model_selector.rs` | Language Migration Phase 2 | Rust model-tier selector with static routing policy, risk escalation, fallback, shadow routes, and rejected candidates. | `DispatchRoutingPolicy`, `ModelSelector`, `ModelSelection` | `engine/tests/dispatch_parity.rs` |
| `engine/src/budget_manager.rs` | Language Migration Phase 2 | Rust pre-execution token/cost budget reservation manager. | `BudgetManager` | `engine/tests/dispatch_parity.rs` |
| `engine/src/executor_adapter.rs` | Language Migration Phase 2 | Rust executor abstraction with default noop executor; does not call providers. | `Executor`, `NoopExecutor`, `ExecutionResult` | `engine/tests/dispatch_parity.rs` |
| `engine/src/evaluation_stub.rs` | Language Migration Phase 2 | Rust deterministic evaluation stub for noop execution results and human-review status. | `EvaluationStub`, `EvaluationResult`, `EvaluationCheck` | `engine/tests/dispatch_parity.rs` |
| `engine/src/dispatch_ledger.rs` | Language Migration Phase 2 | Rust dispatch record and bundle ledger structs for audit-chain parity. | `DispatchLedger`, `DispatchRecord`, `DispatchBundle` | `engine/tests/dispatch_parity.rs` |
| `engine/src/dispatch_engine.rs` | Language Migration Phase 2 | Rust dispatch orchestrator that wires analyzer, selector, budget, noop executor, evaluator, and ledger into the exported golden parity path. | `DispatchEngine`, `build_dispatch_bundle` | `engine/tests/dispatch_parity.rs` |
| `engine/src/http_server.rs` | Language Migration Rust Engine/API Parity | Rust HTTP server context plus local axum router for health, readiness, OpenAPI JSON, and deterministic dispatch; includes auth, scope checks, rate limit checks, and CORS headers. | `ServerContext`, `ServerConfig`, `AxumApiState`, `build_axum_router`, `openapi_document` | `engine/src/http_server.rs` (inline tests), `engine/tests/test_http_server.rs` |
| `engine/src/doc_generator.rs` | Language Migration Rust Engine/API Parity | Rust documentation generator with module/schema registry, Rust source parser, and markdown generation. | `DocGenerator`, `ModuleDoc`, `parse_module_from_source` | `engine/tests/test_doc_generator.rs` |
| `codegen/generate_wire_types.py` | Agent-Control-Plane Phase 5 | Deterministic wire-contract type generator for SDK surfaces. | `main`, `render_ts`, `render_python` | `python3 codegen/generate_wire_types.py` |
| `sdk/typescript/` | Agent-Control-Plane Phase 5 | TypeScript REST SDK package for health, readiness, OpenAPI, and deterministic dispatch endpoints. | `AgentControlPlaneClient`, generated wire types | `corepack pnpm build`, `npm pack --dry-run` |
| `sdk/python/` | Agent-Control-Plane Phase 5 | Python REST SDK package for health, readiness, OpenAPI, and deterministic dispatch endpoints. | `AgentControlPlaneClient`, generated wire types | `PYTHONPATH=src python3 -m unittest discover -s tests`, `python -m build` |
