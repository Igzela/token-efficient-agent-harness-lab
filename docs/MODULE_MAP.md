# Module Map

Last updated: 2026-06-25

This map is for code ownership and verification routing. It is intentionally not a phase history. Historical module/phase narratives are retained in release-tagged git history; `docs/archive/README.md` is the working-tree index.

The Rust `engine/` is the sole runtime implementation. Python is retained as REST SDK and utility scripts only. The current product is a local/small-team self-hosted macro-orchestrator control plane. The approved V2 Real Production Output Track must extend existing modules rather than creating a parallel coding-agent runtime.

## Ownership

| Module | Stage | Purpose | Verification |
|---|---|---|---|
| `engine/src/main.rs` | active runtime | Engine entrypoint and local server startup | `cargo test -p engine` |
| `engine/src/trusted_local.rs` | active runtime policy | IAE-1 profile readiness plus IAE-2 fail-closed bounded task-advancement authority and effective runtime gates | `cargo test -p engine --test test_trusted_local_profile` |
| `engine/src/http_server/` | active runtime/API | Axum routes, middleware, auth/rate-limit, static dashboard serving, guarded adaptive completion routing, runtime capability snapshots, and IAE-3 authority/bounds/observation/scheduler operator evidence | `cargo test -p engine --test test_http_server --test test_adaptive_completion_api` |
| `engine/src/dispatch_engine.rs` | active dispatch | Wires analysis, model selection, budget, executor, evaluation, ledger | `cargo test -p engine --test test_dispatch_engine` |
| `engine/src/task_analyzer/` | active dispatch | Rule-based task domain/intent/risk/complexity analysis | `cargo test -p engine --test dispatch_parity` |
| `engine/src/model_selector.rs` | active dispatch | Tier selection, constraints, fallback, shadow route metadata | `cargo test -p engine --test dispatch_parity` |
| `engine/src/budget_manager.rs` | active dispatch | Token/cost reservation and cost gate checks | `cargo test -p engine --test dispatch_parity` |
| `engine/src/executor/`, `engine/src/executor_adapter.rs` | active execution | Noop/provider/CLI/hybrid executor integration | `cargo test -p engine` |
| `engine/src/provider/` | guarded execution | Provider adapters, bounded adaptive single/fallback/parallel-panel fusion execution, pinned IAE-2 workflow worker, endpoint/model binding, cost/token/time/call/concurrency gates, audit/redaction, circuit breakers | `cargo test -p engine --test test_adaptive_fusion_execution --test test_trusted_local_task_advancement` |
| `engine/src/feedback/` | active policy evidence | Run traces, shadow routes, AF-0 planning, AF-1 registry, AF-2 offline evaluation, AF-4 contextual policy, AF-6 candidate generation/experiments/auto promotion | `cargo test -p engine --test test_adaptive_candidate_generation --test test_contextual_adaptive_policy --test test_adaptive_online_experiments --test test_adaptive_auto_promotion` |
| `engine/src/cli/` | local CLI execution | Default local CLI discovery, explicit workflow ticks, path-free dashboard capability summary, Claude JSON/Codex JSONL adapters, restricted env, redacted/capped output | `cargo test -p engine` |
| `engine/src/node_executor.rs` | supervised execution | Workflow node executors, command allowlist, timeout, structured output | `cargo test -p engine --lib node_executor` |
| `engine/src/target_repo_output.rs`, `engine/src/target_repo_output/authority.rs` | env-gated target output | Controlled git worktree, patch export, `acp/*` branch push, optional idempotent GitHub PR creation, and centralized target-output authority policy for gates, branch/remote/text/path validation | `cargo test -p engine --test test_target_repo_output` |
| `engine/src/scheduler.rs` | guarded workers | Bounded supervised worker threads, queue ticks, pause/kill, heartbeat, lease recovery, dynamic mode, legacy executor-pool binding, and pinned IAE-2 adaptive execution | `cargo test -p engine --lib scheduler --test test_trusted_local_task_advancement` |
| `engine/src/workflow/` | active workflow | DAG mutation, dynamic controller, context pack, run queue, backpressure, decisions | `cargo test -p engine --lib workflow` |
| `engine/src/orchestration/` | partial workflow | Decomposition, conflict resolution, approval gate, aggregation helpers, and the current `AgentMessage` schema foundation for future Agent Runtime work | `cargo test -p engine` |
| `engine/src/quality/`, `engine/src/routing/` | partial policy | Quality/evaluation bridges and routing feedback/advisory logic | `cargo test -p engine` |
| `engine/src/executor_pool.rs` | active resources | Executor capacity, cooldown, selection, metrics | `cargo test -p engine --lib executor_pool` |
| `engine/src/storage/local_product_store/` | active storage | SQLite/PostgreSQL app-owned state plus schema catalog ownership for versions, dialect DDL, migrations, audit, costs, plans, runs, workflow queue/lease helpers, artifacts, adaptive policy snapshots, and safe observation summaries | `cargo test -p engine --test test_local_product_store --test test_data_operations --test test_adaptive_observation_capture` |
| `engine/src/storage/backup_manager.rs` | active ops | SQLite backup, verify, restore support | `cargo test -p engine` |
| `engine/src/infrastructure/` | active ops/security | Auth, rate limiting, circuit breaker, plugin registry helpers | `cargo test -p engine` |
| `dashboard/` | active UI | Local operations console with guarded app-owned controls, Adaptive Fusion policy/completion bindings, IAE authority/bounds evidence, redacted audit actions, scheduler pause/resume/kill, and policy rollback | `cd dashboard && bun run typecheck && bun run build:static` |
| `sdk/typescript/` | active SDK | TypeScript REST SDK, Adaptive Fusion policy/completion/operator-status controls, and generated wire re-exports | `cd sdk/typescript && bun run build && bun run test` |
| `sdk/python/` | active SDK | Python REST SDK | `cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests` |
| `wire_contract/v1/`, `codegen/` | active governance | JSON schemas and deterministic generated Rust/TS/Python wire types | `bash scripts/check_wire_codegen_drift.sh` |
| `scripts/` | active ops | Local doctor, release/smoke checks, drift guards, and real output pilots | script-specific checks, including `scripts/real_output_pilots.py` and `scripts/check_release_contract.sh` |
| `deploy/`, `docker-compose.yml` | optional local packaging | Dockerfiles and compose profiles for local engine/dashboard packaging | `docker compose build` |

## Module Classes

| Class | Modules |
|---|---|
| active | Runtime/API/storage/dashboard/SDK/codegen/script paths listed above. |
| partial | `engine/src/orchestration/`, `engine/src/quality/`, `engine/src/routing/`, `engine/src/ecosystem/`, selected `engine/src/harness/` helpers. These are implemented and tested, but not all are first-class runtime control layers. |
| reference-only | `engine/src/event_source/`, `engine/src/event_schema.rs`, `engine/src/errors.rs`. Kept for wire/event compatibility context; do not wire as a parallel store/runtime. |
| archived history | Long-form phase plans, closeouts, and legacy architecture details retained in release-tagged git history. |

## Change Routing

- Dispatch/routing behavior: start with `dispatch_engine.rs`, `task_analyzer/`, `model_selector.rs`, `budget_manager.rs`, then update wire contracts if response shapes change.
- Workflow execution: start with `scheduler.rs`, `workflow/`, `node_executor.rs`, and `executor_pool.rs`.
- Storage or schema: start with `storage/local_product_store/schema.rs`, update schema version docs in `docs/ARCHITECTURE_BOOK.md` when the version changes, and run relevant SQLite/PostgreSQL tests.
- Dashboard or SDK: update API types/clients and dashboard components together when response shapes change.
- V2-1 execution safety: start with `storage/local_product_store/supervised_patch.rs`, `http_server/handlers/supervised_patch.rs`, `node_executor.rs`, and focused path/secret/timeout/quarantine tests.
- V2-2 provider/CLI output: start with `provider/`, `cli/`, `http_server/handlers/workflow_runs.rs`, `executor/`, `dispatch_engine.rs`, and provider/CLI audit/cost/redaction tests.
- V2-3 target repo PR flow: `target_repo_output/authority.rs` owns target-output authority policy; `target_repo_output.rs` owns git/process safety; supervised patch storage owns workspace/artifact/evidence/approval binding; HTTP owns scope/gate/confirmation/audit; SDK/dashboard API contracts mirror the endpoint.
- V2-4 worker queue: `scheduler.rs` owns worker lifecycle/control; `workflow_runs/queue_lease.rs` owns workflow-run queue/lease SQL, row projection, stale-lease classification, and queue summary helpers; `workflow_runs.rs` owns the `LocalProductStore` public methods and transactional mutations; `run_queue.rs` and `executor_pool.rs` own admission/capacity; `heartbeat.rs` persists aggregate worker health; scheduler HTTP handler and SDKs expose controls.
- V2-5 product UX: `dashboard/src/components/MissionControl.tsx` owns the primary output workflow; `SupervisedPatch.tsx` owns detailed workspace/artifact operations; `SchedulerStatus.tsx` owns worker control/detail; `dashboard/src/lib/api-client.ts` owns dashboard API bindings.
- Adaptive Fusion Routing: candidate generation, contextual policy, experiments, and auto promotion live under `feedback/`; bounded execution and the IAE-2 persisting worker live under `provider/adaptive_execution.rs`; observations and policy snapshots use `storage/local_product_store/`; guarded completion/default routing lives under `http_server/handlers/adaptive_completions.rs` and `dispatch.rs`; `trusted_local.rs` composes IAE-1/IAE-2 authority; `scheduler.rs` pins the trusted adaptive executor; IAE-3 operator aggregation lives in the dashboard snapshot handler, reuses the runtime experiment/promotion validators, and reuses scheduler/audit/policy endpoints; dashboard and TypeScript SDK mirror the guarded API contracts.
- Agent Runtime: start with `orchestration/schemas.rs` for agent/message type contracts. Durable entity ownership: `AgentState` → `storage/local_product_store/schema.rs`, `AgentMessage` mailbox → `storage/local_product_store/schema.rs`, `AgentStep` executor → `node_executor.rs`, agent-child-task proposals → `workflow/` and `scheduler.rs`, debate/review threads → `workflow/`, agent claim/concurrency → `scheduler.rs`. No second scheduler, mailbox side channel, DAG kernel, or storage layer. See `docs/ARCHITECTURE_BOOK.md` § Agent Runtime (AR-0) Contract for the full ownership table.
- Safety boundary changes: update `docs/ARCHITECTURE_BOOK.md` before implementation; use archived security docs only as historical reference.
- Documentation set changes: keep the active docs set limited to the six files listed in `docs/CURRENT_STATUS.md`.

## Guardrails

- Full Agent Autonomy Mode may replace the R7 architecture baseline or existing module ownership only through an explicit documented change with focused tests, observability, review, and rollback.
- Avoid parallel scheduler, DAG kernel, policy engine, storage layer, or dashboard data model implementations unless the replacement plan explains migration and rollback.
- Existing V2 behavior follows the completed phase plan in `docs/NEXT_DECISION.md`; any replacement or authority expansion must be documented, tested, observable, reviewed, and rollbackable before merge.
- Adaptive Fusion Routing is complete through AF-7. IAE-1 composes provider calls, experiments, promotion, and default routing behind the validated trusted-local profile; IAE-2 may advance existing queued adaptive runs through the same bounded path. The underlying paths remain authenticated, bounded, audited, redacted, and killable.
- Repo-scoped target-output, sandbox, hosted/deploy workflow, and authority changes may proceed through Full Agent Autonomy Mode. Existing controls remain current implementation details until a documented, tested, observable, reviewable, and rollbackable replacement lands.
- Any V2 real capability must include an env/auth gate, audit event, tests, and rollback/kill path before it is usable.
- Wire-codegen drift guard: `scripts/check_wire_codegen_drift.sh`.
- Run `uv run --no-project python scripts/check_agent_handoff.py` before committing handoff changes.
