# Architecture Book

Last updated: 2026-06-17

This is the current architecture baseline for the Token-Efficient Agent Harness Lab. Historical phase plans, closeout reports, and long-form strategy docs live under `docs/archive/`.

## Product Boundary

The system is a local/small-team self-hosted macro-orchestrator control plane for studying token-efficient agent workflows. The approved V2 Real Production Output Track extends it toward auditable real-repository patch/PR production while preserving explicit gates, audit, and rollback controls. It is not a cloud SaaS, hosted multi-tenant service, direct-deploy tool, or unattended autonomous-agent runtime.

Default posture:

- Provider execution is off unless `ACP_ENABLE_PROVIDER_EXECUTION=1`.
- CLI execution is off unless `ACP_ENABLE_CLI_EXECUTION=1`.
- App runtime does not write registered target repositories until the V2 target-repo branch/worktree/PR flow lands.
- No release/tag/deploy/apply controls exist in the app runtime.
- No process/container/VM sandbox isolation is implemented; V2-1 is scoped to app-owned workspace confinement unless separately approved.
- Supervised execution operates only in app-owned detached workspaces and remains explicitly gated.

This file is authoritative for current architecture and safety boundaries. Operational procedures are in `docs/RUNBOOK.md`. Archived security and safety notes under `docs/archive/security/` are historical reference; revive or replace them only for an approved boundary-expansion track.

## Runtime Shape

```text
HTTP API / Dashboard / SDKs
        |
        v
DispatchEngine
  TaskAnalyzer -> ModelSelector -> BudgetManager -> Executor -> Evaluation -> Ledger
        |
        v
WorkflowScheduler / DynamicWorkflowController
  run queue -> executor pool -> node executor -> graph mutation -> approval/export gates
        |
        v
LocalProductStore
  SQLite default, PostgreSQL optional, audit, costs, plans, runs, artifacts, config, team
```

## Primary Components

| Area | Owner | Purpose |
|---|---|---|
| API server | `engine/src/http_server/` | Axum API, middleware, auth/rate-limit, dashboard serving |
| Dispatch kernel | `engine/src/dispatch_engine.rs`, `task_analyzer/`, `model_selector.rs`, `budget_manager.rs` | Deterministic dispatch analysis, tier selection, cost gates |
| Execution | `engine/src/executor/`, `provider/`, `cli/`, `node_executor.rs` | Noop/provider/CLI/command execution behind explicit gates |
| Workflow runtime | `engine/src/scheduler.rs`, `workflow/`, `orchestration/` | Persistent workflow runs, DAG state, dynamic recovery, queue/backpressure |
| Storage | `engine/src/storage/local_product_store/` | App-owned SQLite/PostgreSQL state, audit, costs, backups, artifacts |
| Ops hardening | `backup_manager.rs`, `infrastructure/`, `provider/circuit_breaker_provider.rs` | backup/restore, health, metrics, circuit breaker, TLS/env-gated hardening |
| Dashboard | `dashboard/` | Local operations console with guarded app-owned controls and observability views |
| SDKs | `sdk/typescript/`, `sdk/python/` | REST clients for dashboard/API operations |
| Wire contracts | `wire_contract/v1/`, `codegen/` | Cross-language dispatch schemas and generated types |

## Data Ownership

| State | Owner | Writable by app? | Notes |
|---|---|---:|---|
| Registered target repositories | User | No | App runtime must not write targets. Agent maintenance may use branch+PR workflow under playbook gates. |
| Local product store | App | Yes | Dispatches, plans, workflow runs, events, approvals, config, team, costs, audit. |
| App-owned workspaces | App | Yes | Detached supervised execution workspaces; not target repo mutation. |
| Artifacts/exports | App | Yes | Capture requires secret scan/integrity; export requires valid approval binding. |
| Backups | App/operator | Yes | SQLite app-owned backups; PostgreSQL operators use external backup tooling. |

## V2 Real Production Output Architecture

V2 target flow:

```text
connect real repo
  -> create task
  -> prepare isolated app-owned workspace
  -> run gated provider/CLI/command execution
  -> collect code changes and verification evidence
  -> scan/redact secrets and validate integrity
  -> require human approval
  -> push PR branch or export patch
```

The V2 design upgrades old limitations into explicit production guardrails:

| Capability | Current state | V2 target | Guardrail |
|---|---|---|---|
| Workspace execution | App-owned workspace lifecycle and V2-1 safety base exist | Confined app-owned execution workspace | Path-safe IDs, canonical app-store root checks, symlink skip, file/byte ceilings, quarantine/cleanup |
| Provider/CLI output | Env-gated paths exist but are not productized as real-output flow | Real output path under explicit gates | Auth scope, env gate, cost cap, retry/budget breaker, trace, redaction |
| Target repository output | No target repo writes | Branch/worktree/PR branch or patch export | No direct `main` writes, approval binding, evidence bundle, secret-free PR body |
| Workers | Queue/pool primitives exist | Bounded supervised workers | Lease, heartbeat, concurrency cap, stale recovery, pause/kill |
| Dashboard UX | Operator tabs and Mission Control | Single output workflow | Gate/risk/next-step/approval visibility |

Do not create a second runtime kernel for V2. Extend the existing `node_executor`, `workflow_runs`, `scheduler`, `executor_pool`, `provider`, `cli`, `supervised_patch`, `LocalProductStore`, SDK, and dashboard surfaces.

## Storage

`LocalProductStore` supports SQLite by default and PostgreSQL through the `pg` feature and `ACP_DATABASE_URL`.

- Current version: v13
- SQLite uses WAL and app-managed backup/restore.
- PostgreSQL disables app-managed backup; operators use `pg_dump` or managed backup.
- PostgreSQL integration tests are gated behind `cargo test -p engine --features pg-tests`.

## Execution Modes

`ACP_EXECUTION_MODE` controls dispatch execution:

| Mode | Behavior |
|---|---|
| `off` | Default noop behavior; no external calls |
| `provider` | Provider API only; requires provider gate/auth/cost controls |
| `cli` | CLI executor only; requires CLI gate |
| `auto` | Hybrid provider/CLI routing by complexity threshold |

Workflow node execution is explicit through scheduler/tick paths. `CommandNodeExecutor` rejects shell metacharacters, avoids `sh -c`, uses allowlisted binaries, validates supplied workspace cwd, clears inherited environment except `PATH`, caps captured output, enforces timeout kill, and emits structured results. Claude/Codex CLI execution remains a separate explicit opt-in path.

## Workflow Model

`WorkflowGraph` is the canonical planning/persistence model. Dynamic workflow mode can:

1. observe a failed or low-quality node,
2. mutate the persisted graph with fix/test follow-up nodes,
3. record mutation and orchestration decisions,
4. resume the run,
5. pause for approval/export when required.

The runtime path is intentionally built on existing `workflow_runs`, `scheduler`, `node_executor`, `executor_pool`, `run_queue`, `backpressure`, and `DynamicWorkflowController` modules. Do not create a parallel scheduler, DAG kernel, or policy engine without explicit approval.

## Dashboard Boundary

The dashboard is a local operations console with guarded app-owned controls. It is not globally read-only:

- Observability views read dispatches, workflow graph state, queue/pool state, health, costs, audit, artifacts, and decisions.
- Guarded controls can mutate app-owned state: team/API keys, backups, workflow tick/cancel, policy proposal lifecycle, and supervised patch approval/export.
- Backend auth/scopes, confirmation flags where implemented, and audit logging are the actual safety boundary.
- Dashboard controls must not write target repositories, deploy/release/apply code, broaden provider/CLI gates, or bypass backend authorization.

## Current Gaps

These are accepted current limitations, not hidden TODOs:

- V2 real output is authorized but not yet complete; each phase must land behind explicit gates.
- V2-1 app-owned workspace hardening is implemented, but it is not hard process/container/VM sandboxing and does not authorize target-repository writes.
- Hard process/container/VM sandbox isolation is not implemented and is not part of V2-1 unless separately approved.
- Target-repository writes remain unavailable until V2-3 implements controlled branch/worktree/PR output.
- Provider/CLI execution remains default-off even after V2-2.
- Bounded supervised workers are planned for V2-4; unattended autonomous-agent loops remain out of scope.
- Cloud SaaS, hosted/cloud deployment, multi-tenant service, direct release/tag/deploy/apply controls, and provider failover remain out of scope.
- Some routing, quality, and orchestration modules remain partially active rather than unified under one policy layer.

Boundary expansion outside the approved V2 track requires a new plan, threat-model update, focused tests, and explicit human approval.

## Active Verification

Primary local verification:

```bash
bash scripts/verify_rust_typescript_stack.sh
uv run --no-project python scripts/check_agent_handoff.py
```

Additional focused checks:

```bash
cargo test -p engine
cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests
ACP_TEST_DATABASE_URL=postgres://user:pass@localhost:5432/testdb cargo test -p engine --features pg-tests
```

## Historical References

Archived materials are retained for audit/history, not daily reading:

- `docs/archive/dispatch/DISPATCHER_KERNEL_V0_ARCHITECTURE.md`
- `docs/archive/strategy/DYNAMIC_GLOBAL_REGULATOR_PLAN.md`
- `docs/archive/ops/DATA_DIRECTORY.md`
- `docs/archive/security/`
- `docs/archive/SESSION_START_HERE.md`
- `docs/archive/DOCS_INVENTORY.md`
- `docs/archive/phase-closeouts/`
- `docs/archive/validation/LIVE_E2E_VALIDATION_REPORT.md`
