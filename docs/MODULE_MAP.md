# Module Map

The legacy Python reference implementation (`src/harness_core/`) and its test suite (`tests/`) have been retired. The Rust `engine/` is now the sole runtime implementation. Python is retained only as the REST SDK (`sdk/python/`) and utility scripts.

## Rust Engine Modules

| Module | Stage | Purpose | Main public APIs | Related tests |
| --- | --- | --- | --- | --- |
| `engine/src/runtime.rs` | Language Migration Phase 1 | Deterministic Rust fixture runtime for stable timestamps and IDs. | `FixtureRuntime`, `FIXTURE_TIMESTAMP` | `engine/tests/dispatch_parity.rs` |
| `engine/src/event_schema.rs` | Language Migration Phase 1 | Rust event.v1 validation, canonical JSON, and stable idempotency hash helpers. | `validate_event`, `canonical_event_json`, `stable_idempotency_hash` | `engine/tests/dispatch_parity.rs` |
| `engine/src/task_analyzer/` | Language Migration Phase 1 + Architecture Refactor R3 | Rust rule-based task analyzer module directory. `mod.rs` owns `TaskAnalysis`, `RuleBasedTaskAnalyzer`, `analyze()`. `rules.rs` static keyword/phrase/multiplier maps. `classify.rs` domain/intent classification. `risk.rs` risk flag detection and negation. `scoring.rs` complexity, budgets, confidence, risk level, quality, escalation, capabilities, features. | `RuleBasedTaskAnalyzer`, `TaskAnalysis`, `TASK_ANALYSIS_SCHEMA_VERSION`, `analyze` | `engine/tests/dispatch_parity.rs` |
| `engine/src/dispatch_decision.rs` | Language Migration Phase 1 | Rust dispatch decision schema structs used by the parity engine. | `DispatchDecision`, `BudgetReservation`, `ExecutionGate`, `build_dispatch_bundle` | `engine/tests/dispatch_parity.rs` |
| `engine/src/model_selector.rs` | Language Migration Phase 2 | Rust model-tier selector with static routing policy, risk escalation, fallback, shadow routes, and rejected candidates. | `DispatchRoutingPolicy`, `ModelSelector`, `ModelSelection` | `engine/tests/dispatch_parity.rs` |
| `engine/src/budget_manager.rs` | Language Migration Phase 2 | Rust pre-execution token/cost budget reservation manager. | `BudgetManager` | `engine/tests/dispatch_parity.rs` |
| `engine/src/executor_adapter.rs` | Language Migration Phase 2 | Rust executor abstraction with default noop executor; does not call providers. | `Executor`, `NoopExecutor`, `ExecutionResult` | `engine/tests/dispatch_parity.rs` |
| `engine/src/provider/` | Language Migration Phase 2 + Provider Infrastructure + Stage 1 + Productization Phase 1 | Rust provider module directory: boundary trait, config, credential, audit, redaction, transport, cost gate, retry (with RetryFallbackManager), and provider adapters (openai, anthropic, stub). | `Provider`, `DisabledProvider`, `ProviderRequest`, `ProviderResponse`, `ProviderError`, `ProviderConfig`, `CredentialRef`, `RetryPolicy`, `CredentialBoundary`, `ProviderAuditEvent`, `ProviderAuditRecorder`, `redact_secrets`, `redact_audit_fields`, `RetryFallbackManager`, `should_retry`, `compute_delay_ms`, `CostGateConfig`, `check_cost_gates`, `CostGateBlock` | `cargo test -p engine` |
| `engine/src/evaluation_stub.rs` | Language Migration Phase 2 | Rust deterministic evaluation stub for noop execution results and human-review status. | `EvaluationStub`, `EvaluationResult`, `EvaluationCheck` | `engine/tests/dispatch_parity.rs` |
| `engine/src/dispatch_ledger.rs` | Language Migration Phase 2 | Rust dispatch record and bundle ledger structs for audit-chain parity. | `DispatchLedger`, `DispatchRecord`, `DispatchBundle` | `engine/tests/dispatch_parity.rs` |
| `engine/src/dispatch_engine.rs` | Language Migration Phase 2 | Rust dispatch orchestrator that wires analyzer, selector, budget, noop executor, evaluator, and ledger into the exported golden parity path. | `DispatchEngine`, `build_dispatch_bundle` | `engine/tests/dispatch_parity.rs` |
| `engine/src/http_server/` | Language Migration Rust Engine/API Parity + Permission Governance + Cost Governance + Dashboard Controls + Architecture Refactor R1 | Rust HTTP server module directory. `mod.rs` re-exports, request types, openapi_document. `state.rs` AxumApiState/ServerConfig. `middleware.rs` auth, rate-limit, CORS, helpers. `routes.rs` router construction, dashboard serving. `server_context.rs` ServerContext/RouteMatch/match_path. `handlers/` 10 handler modules (health, dispatch, team, keys, costs, backups, audit, provider, dashboard, data_ops). | `ServerContext`, `ServerConfig`, `AxumApiState`, `build_axum_router`, `build_axum_router_with_dashboard`, `openapi_document` | `engine/src/http_server/server_context.rs` (inline tests), `engine/src/http_server/mod.rs` (inline tests), `engine/tests/test_http_server.rs` |
| `engine/src/doc_generator.rs` | Language Migration Rust Engine/API Parity | Rust documentation generator with module/schema registry, Rust source parser, and markdown generation. | `DocGenerator`, `ModuleDoc`, `parse_module_from_source` | `engine/tests/test_doc_generator.rs` |
| `engine/src/storage/local_product_store/` | Agent-Control-Plane Local Small-Team Productization + Permission Governance + Cost Governance + Dashboard Controls + Architecture Refactor R2 | SQLite-backed app-owned local state module directory. `mod.rs` owns `LocalProductStore` struct, constructors, shared helpers, DDL, and re-exports. Domain submodules: `dispatch.rs`, `config.rs`, `team.rs`, `keys.rs`, `audit.rs`, `provider_audit.rs`, `costs.rs`, `migrations.rs`, `integrity.rs`, `export_import.rs`, `boundaries.rs`. | `LocalProductStore`, `local_boundaries`, `IntegrityReport`, `ImportResult`, `ImportCounts` | `engine/tests/test_http_server.rs`, `engine/tests/test_local_product_store.rs`, `engine/tests/test_data_operations.rs`, `engine/tests/test_audit_integrity.rs` |
| `engine/src/cli/` | CLI Executor Routing | Complexity-based dispatch to Claude Code CLI / Codex CLI via subprocess invocation. | `ClaudeCodeCliExecutor`, `CodexCliExecutor`, `MultiExecutor`, `CliConfig` | `cargo test -p engine` (10 CLI tests) |
| `engine/src/workflow/dag_manager/` | Workflow DAG Management + Architecture Refactor R4 | DAG manager module directory. `mod.rs` owns `DAGManager` struct, mutation dispatch, rollback, validation, topological sort, ready-nodes, path finding, re-exports, and tests. `types.rs` DAGNode/DAGEdge/DAGState/DAGMutationProposal/DAGMutationResult. `helpers.rs` cycle detection (iterative DFS), node/edge lookup, approval gating. `mutations.rs` add/remove/rewire/update applicators. `compensate.rs` inverse mutation generator. | `DAGManager`, `DAGNode`, `DAGEdge`, `DAGState`, `DAGMutationProposal`, `DAGMutationResult`, `has_cycle`, `find_node`, `find_edge`, `requires_approval`, `apply_add_node`, `apply_remove_node`, `apply_add_edge`, `apply_remove_edge`, `apply_rewire_edge`, `apply_update_node`, `compensate` | `engine/src/workflow/dag_manager/mod.rs` (24 inline tests) |

## SDK and Dashboard

| Module | Stage | Purpose | Main public APIs | Related tests |
| --- | --- | --- | --- | --- |
| `sdk/typescript/` | Agent-Control-Plane Phase 5 + Local Small-Team Productization + Dashboard Controls | TypeScript REST SDK package. | `AgentControlPlaneClient`, generated wire types | `bun run build`, `bun run test`, `npm pack --dry-run` |
| `sdk/python/` | Agent-Control-Plane Phase 5 + Local Small-Team Productization + Dashboard Controls | Python REST SDK package. | `AgentControlPlaneClient`, generated wire types | `cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests` |
| `dashboard/` | Agent-Control-Plane Phase 6 + Local Small-Team Productization + Dashboard Controls | Next.js dashboard for live local dispatches, dispatch detail drill-down, routing choices, team/API-key management, costs, settings with provider health, health, backups, and audit log. | App Router page, `fetchHealth`, `fetchDashboard`, `fetchDispatchDetail`, `fetchBackups`, `fetchAudit`, `fetchProviderHealth`, readonly lint guard, `build:static` | `bun run lint`, `bun run typecheck`, `bun run build`, `bun run build:static` |

## Codegen and Wire Contracts

| Module | Stage | Purpose | Main public APIs | Related tests |
| --- | --- | --- | --- | --- |
| `codegen/generate_wire_types.py` | Agent-Control-Plane Phase 5 | Deterministic wire-contract type generator for SDK surfaces. | `main`, `render_ts`, `render_python` | `python3 codegen/generate_wire_types.py` |
| `wire_contract/v1/*.schema.json` | Language Migration Phase 0 | Frozen dispatch JSON schemas for cross-language semantic parity. | `dispatch_request`, `task_analysis`, `dispatch_decision`, `execution_result`, `evaluation_result`, `dispatch_bundle` schemas | `cargo test -p engine` (Rust parity tests) |

## Utility Scripts

| Module | Stage | Purpose | Main public APIs | Related tests |
| --- | --- | --- | --- | --- |
| `scripts/smoke_native_runtime.py` | Agent-Control-Plane Native Local Runtime | Stdlib smoke test for the Rust engine. | `main` | `python3 scripts/smoke_native_runtime.py` |
| `scripts/verify_rust_typescript_stack.sh` | Rust + TypeScript Cutover | Primary cutover verification. | shell entrypoint | `bash scripts/verify_rust_typescript_stack.sh` |
| `scripts/check_agent_handoff.py` | Agent workflow | Handoff documentation integrity guard. | `main` | `uv run --no-project python scripts/check_agent_handoff.py` |
| `tools/check_security_baseline.py` | CA-7 Security | Security baseline checker (secret scan, import scan, routing guard, governance guard, event guard). | `main`, `check_secret_scan`, `check_import_scan`, `check_active_routing`, `check_governance_boundary`, `check_stage0_event_guard` | `tools/test_security_baseline.py` |

## Infrastructure

| Module | Stage | Purpose | Main public APIs | Related tests |
| --- | --- | --- | --- | --- |
| `deploy/`, `docker-compose.yml` | Agent-Control-Plane Phase 7 | Optional local Docker build/run definitions for Rust API and dashboard. | `deploy/Dockerfile.engine`, `deploy/Dockerfile.dashboard`, compose services | `docker compose build`, `docker compose up --build -d` |
| `docs/AGENT_CONTROL_PLANE_MIGRATION_CLOSEOUT.md` | Agent-Control-Plane Phase 8 | Migration closeout record. | Closeout status | `uv run --no-project python scripts/check_agent_handoff.py` |
