# Module Map

Last updated: 2026-06-17

This map is for code ownership and verification routing. It is intentionally not a phase history. Historical module/phase narratives live in `docs/archive/`.

The Rust `engine/` is the sole runtime implementation. Python is retained as REST SDK and utility scripts only. The current product is a local/small-team self-hosted macro-orchestrator control plane. The approved V2 Real Production Output Track must extend existing modules rather than creating a parallel coding-agent runtime.

## Ownership

| Module | Stage | Purpose | Verification |
|---|---|---|---|
| `engine/src/main.rs` | active runtime | Engine entrypoint and local server startup | `cargo test -p engine` |
| `engine/src/http_server/` | active runtime/API | Axum routes, middleware, auth/rate-limit, static dashboard serving | `cargo test -p engine --test test_http_server` |
| `engine/src/dispatch_engine.rs` | active dispatch | Wires analysis, model selection, budget, executor, evaluation, ledger | `cargo test -p engine --test test_dispatch_engine` |
| `engine/src/task_analyzer/` | active dispatch | Rule-based task domain/intent/risk/complexity analysis | `cargo test -p engine --test dispatch_parity` |
| `engine/src/model_selector.rs` | active dispatch | Tier selection, constraints, fallback, shadow route metadata | `cargo test -p engine --test dispatch_parity` |
| `engine/src/budget_manager.rs` | active dispatch | Token/cost reservation and cost gate checks | `cargo test -p engine --test dispatch_parity` |
| `engine/src/executor/`, `engine/src/executor_adapter.rs` | active execution | Noop/provider/CLI/hybrid executor integration | `cargo test -p engine` |
| `engine/src/provider/` | env-gated execution | Provider adapters, workflow-node executor, retry/cost gates, audit/redaction, circuit breaker wrapper | `cargo test -p engine` |
| `engine/src/cli/` | env-gated execution | Claude Code / Codex CLI executor path with restricted env and redacted/capped output | `cargo test -p engine` |
| `engine/src/node_executor.rs` | supervised execution | Workflow node executors, command allowlist, timeout, structured output | `cargo test -p engine --lib node_executor` |
| `engine/src/scheduler.rs` | active workflow | Persistent scheduler, lease recovery, dynamic mode, executor-pool binding | `cargo test -p engine --lib scheduler` |
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
| `scripts/` | active ops | Local doctor, smoke, release checklist, drift guards, pilot/soak scripts | script-specific `--help` or smoke commands |
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
- V2-3 target repo PR flow: start with supervised patch storage/API plus a small engine-owned git/PR helper; update SDK/dashboard only when API shapes change.
- V2-4 worker queue: start with `scheduler.rs`, `workflow/run_queue.rs`, `executor_pool.rs`, and `storage/local_product_store/heartbeat.rs`.
- V2-5 product UX: start with `dashboard/src/components/MissionControl.tsx`, `SupervisedPatch.tsx`, `RuntimeGates.tsx`, and `dashboard/src/lib/api-client.ts`.
- Safety boundary changes: update `docs/ARCHITECTURE_BOOK.md` before implementation; use archived security docs only as historical reference.
- Documentation set changes: keep the active docs set limited to the six files listed in `docs/CURRENT_STATUS.md`.

## Guardrails

- R-series is sealed at R7. R8 is not approved.
- Do not create a parallel scheduler, DAG kernel, policy engine, storage layer, or dashboard data model.
- V2 Real Production Output is approved only through the phase plan in `docs/NEXT_DECISION.md`; do not skip phases or merge half-built runtime authority.
- Do not add provider/CLI default-on execution, direct target-repository `main` writes, process/container/VM sandbox behavior, hosted/cloud deployment, release/tag/deploy controls, provider failover, or unattended autonomous-agent loops without separate explicit approval.
- Any V2 real capability must include an env/auth gate, audit event, tests, and rollback/kill path before it is usable.
- Wire-codegen drift guard: `scripts/check_wire_codegen_drift.sh`.
- Run `uv run --no-project python scripts/check_agent_handoff.py` before committing handoff changes.
