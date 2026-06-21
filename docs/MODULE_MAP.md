# Module Map

Last updated: 2026-06-21

This map is for code ownership and verification routing. It is intentionally not a phase history. Historical module/phase narratives live in `docs/archive/`.

The Rust `engine/` is the sole runtime implementation. Python is retained as REST SDK and utility scripts only. The current product is a local/small-team self-hosted macro-orchestrator control plane. The approved V2 Real Production Output Track must extend existing modules rather than creating a parallel coding-agent runtime.

## Ownership

| Module | Stage | Purpose | Verification |
|---|---|---|---|
| `engine/src/main.rs` | active runtime | Engine entrypoint and local server startup | `cargo test -p engine` |
| `engine/src/http_server/` | active runtime/API | Axum routes, middleware, auth/rate-limit, static dashboard serving, read-only runtime capability snapshots | `cargo test -p engine --test test_http_server` |
| `engine/src/dispatch_engine.rs` | active dispatch | Wires analysis, model selection, budget, executor, evaluation, ledger | `cargo test -p engine --test test_dispatch_engine` |
| `engine/src/task_analyzer/` | active dispatch | Rule-based task domain/intent/risk/complexity analysis | `cargo test -p engine --test dispatch_parity` |
| `engine/src/model_selector.rs` | active dispatch | Tier selection, constraints, fallback, shadow route metadata | `cargo test -p engine --test dispatch_parity` |
| `engine/src/budget_manager.rs` | active dispatch | Token/cost reservation and cost gate checks | `cargo test -p engine --test dispatch_parity` |
| `engine/src/executor/`, `engine/src/executor_adapter.rs` | active execution | Noop/provider/CLI/hybrid executor integration | `cargo test -p engine` |
| `engine/src/provider/` | env-gated execution | Provider adapters, AF-3 bounded adaptive execution, endpoint/model binding, workflow-node executors, cost/token/time/call gates, audit/redaction, circuit breakers | `cargo test -p engine --test test_adaptive_fusion_execution` |
| `engine/src/feedback/` | active policy evidence | Run traces, shadow routes, simulation/proposals, guarded adjustment snapshots, AF-0 planning, AF-1 registry, AF-2 offline evaluation | `cargo test -p engine --test test_adaptive_fusion --test test_model_endpoint_registry --test test_offline_fusion_evaluation` |
| `engine/src/cli/` | local CLI execution | Default local CLI discovery, explicit workflow ticks, path-free dashboard capability summary, Claude JSON/Codex JSONL adapters, restricted env, redacted/capped output | `cargo test -p engine` |
| `engine/src/node_executor.rs` | supervised execution | Workflow node executors, command allowlist, timeout, structured output | `cargo test -p engine --lib node_executor` |
| `engine/src/target_repo_output.rs` | env-gated target output | Controlled git worktree, patch export, `acp/*` branch push, optional idempotent GitHub PR creation, remote/auth validation | `cargo test -p engine --test test_target_repo_output` |
| `engine/src/scheduler.rs` | env-gated workers | Bounded supervised worker threads, queue ticks, pause/kill, heartbeat, lease recovery, dynamic mode, executor-pool binding | `cargo test -p engine --lib scheduler` |
| `engine/src/workflow/` | active workflow | DAG mutation, dynamic controller, context pack, run queue, backpressure, decisions | `cargo test -p engine --lib workflow` |
| `engine/src/orchestration/` | partial workflow | Decomposition, conflict resolution, approval gate, aggregation helpers | `cargo test -p engine` |
| `engine/src/quality/`, `engine/src/routing/` | partial policy | Quality/evaluation bridges and routing feedback/advisory logic | `cargo test -p engine` |
| `engine/src/executor_pool.rs` | active resources | Executor capacity, cooldown, selection, metrics | `cargo test -p engine --lib executor_pool` |
| `engine/src/storage/local_product_store/` | active storage | SQLite/PostgreSQL app-owned state, migrations, audit, costs, plans, runs, artifacts | `cargo test -p engine --test test_local_product_store` |
| `engine/src/storage/backup_manager.rs` | active ops | SQLite backup, verify, restore support | `cargo test -p engine` |
| `engine/src/infrastructure/` | active ops/security | Auth, rate limiting, circuit breaker, plugin registry helpers | `cargo test -p engine` |
| `dashboard/` | active UI | Local operations console with guarded app-owned controls | `cd dashboard && bun run typecheck && bun run build:static` |
| `sdk/typescript/` | active SDK | TypeScript REST SDK and generated wire re-exports | `cd sdk/typescript && bun run build && bun run test` |
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
| archived history | Long-form phase plans, closeouts, and legacy architecture details under `docs/archive/`. |

## Change Routing

- Dispatch/routing behavior: start with `dispatch_engine.rs`, `task_analyzer/`, `model_selector.rs`, `budget_manager.rs`, then update wire contracts if response shapes change.
- Workflow execution: start with `scheduler.rs`, `workflow/`, `node_executor.rs`, and `executor_pool.rs`.
- Storage or schema: start with `storage/local_product_store/`, update schema version docs in `docs/ARCHITECTURE_BOOK.md`, and run relevant SQLite/PostgreSQL tests.
- Dashboard or SDK: update API types/clients and dashboard components together when response shapes change.
- V2-1 execution safety: start with `storage/local_product_store/supervised_patch.rs`, `http_server/handlers/supervised_patch.rs`, `node_executor.rs`, and focused path/secret/timeout/quarantine tests.
- V2-2 provider/CLI output: start with `provider/`, `cli/`, `http_server/handlers/workflow_runs.rs`, `executor/`, `dispatch_engine.rs`, and provider/CLI audit/cost/redaction tests.
- V2-3 target repo PR flow: `target_repo_output.rs` owns git/process safety; supervised patch storage owns workspace/artifact/evidence/approval binding; HTTP owns scope/gate/confirmation/audit; SDK/dashboard API contracts mirror the endpoint.
- V2-4 worker queue: `scheduler.rs` owns worker lifecycle/control; `workflow_runs.rs` owns atomic lease/stale recovery; `run_queue.rs` and `executor_pool.rs` own admission/capacity; `heartbeat.rs` persists aggregate worker health; scheduler HTTP handler and SDKs expose controls.
- V2-5 product UX: `dashboard/src/components/MissionControl.tsx` owns the primary output workflow; `SupervisedPatch.tsx` owns detailed workspace/artifact operations; `SchedulerStatus.tsx` owns worker control/detail; `dashboard/src/lib/api-client.ts` owns dashboard API bindings.
- Adaptive Fusion Routing: AF-0 through AF-2 live under `feedback/`; AF-3 execution lives under `provider/adaptive_execution.rs` and the existing workflow tick/`NodeExecutor` boundary; AF-4 must reuse the existing feedback policy/snapshot gates.
- Safety boundary changes: update `docs/ARCHITECTURE_BOOK.md` before implementation; use archived security docs only as historical reference.
- Documentation set changes: keep the active docs set limited to the six files listed in `docs/CURRENT_STATUS.md`.

## Guardrails

- R-series is sealed at R7. R8 is not approved.
- Do not create a parallel scheduler, DAG kernel, policy engine, storage layer, or dashboard data model.
- V2 Real Production Output is approved only through the phase plan in `docs/NEXT_DECISION.md`; do not skip phases or merge half-built runtime authority.
- Adaptive Fusion Routing is approved only through AF-0 to AF-5 in `docs/NEXT_DECISION.md`; AF-0 through AF-2 remain shadow/offline, while AF-3 live influence requires an explicit authenticated adaptive workflow tick.
- Do not add default-on provider API execution, direct target-repository `main` writes, process/container/VM sandbox behavior, hosted/cloud deployment, app-runtime release/deploy controls, provider failover outside AF-3 gates, or unattended autonomous-agent loops without separate explicit approval.
- Any V2 real capability must include an env/auth gate, audit event, tests, and rollback/kill path before it is usable.
- Wire-codegen drift guard: `scripts/check_wire_codegen_drift.sh`.
- Run `uv run --no-project python scripts/check_agent_handoff.py` before committing handoff changes.
