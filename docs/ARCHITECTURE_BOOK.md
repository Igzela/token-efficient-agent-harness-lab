# Architecture Book

Last updated: 2026-06-20

This is the current architecture baseline for the Token-Efficient Agent Harness Lab. Historical phase plans, closeout reports, and long-form strategy docs live under `docs/archive/`.

## Product Boundary

The system is a local/small-team self-hosted macro-orchestrator control plane for studying token-efficient agent workflows. V2 adds auditable real-repository patch/PR production. The approved Trusted Local Autonomous Execution Track may activate bounded provider and agent execution as a coherent local profile while preserving auth, budgets, audit, approval, rollback, and kill controls. It is not a cloud SaaS, hosted multi-tenant service, or direct-deploy tool.

Default posture:

- Provider execution is off unless `ACP_ENABLE_PROVIDER_EXECUTION=1`.
- Installed local Claude/Codex CLIs are discovered by default; explicit workflow ticks invoke them. `ACP_ENABLE_CLI_EXECUTION=0` disables this path.
- Target output remains off unless `ACP_ENABLE_TARGET_REPO_OUTPUT=1`. V2-3 permits only an app-owned git worktree plus approval-bound patch export or `acp/*` branch push; the registered target working tree and `main` remain protected.
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
| Registered target repositories | User | Controlled refs only | V2-3 may add app-owned worktree metadata and an approved `acp/*` branch; it must not modify the registered working tree or `main`. Agent maintenance separately follows playbook gates. |
| Local product store | App | Yes | Dispatches, plans, workflow runs, events, approvals, config, team, costs, audit. |
| App-owned workspaces | App | Yes | Copy workspace or controlled detached git worktree outside the target path. |
| Artifacts/exports | App | Yes | Capture binds patch content and verification evidence; real export/push requires secret scan, integrity, confirmation, scope, gate, and approval binding. |
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
| Provider/CLI output | V2-2 gated workflow-node output path exists | Real output path under explicit gates | Auth scope, env gate, cost cap, retry/budget breaker, trace, redaction |
| Target repository output | V2-3 plus closeout merged in implementation branch | Controlled git worktree, `acp/*` branch push, patch export, optional GitHub PR | `dispatch:execute`, env gates/kill, explicit confirmation, same-run approval binding, real verification evidence, content hash, text-only bounded changed files, HTTPS remote/host/token controls, no direct `main` writes or merge authority |
| Workers | V2-4 merged | Bounded supervised workers | Dual env gate, atomic DB lease, per-worker heartbeat, concurrency cap, stale recovery audit, authenticated pause/resume/kill |
| Dashboard UX | Task-first workspace | Single output workflow with secondary operations/admin navigation | Task/run/workspace/verification/approval/PR visibility |

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

Workflow node execution is explicit through scheduler/tick paths. `CommandNodeExecutor` rejects shell metacharacters, avoids `sh -c`, uses allowlisted binaries, validates supplied workspace cwd, clears inherited environment except `PATH`, caps output, enforces timeout kill, and emits structured results. Installed Claude/Codex CLIs are discovered by default; `ACP_ENABLE_CLI_EXECUTION=0` disables them. The dashboard receives a startup capability snapshot with only enabled/detected booleans; it exposes no binary paths and grants no execution authority. CLI subprocess env is restricted to `PATH` plus `ACP_CLI_ENV_ALLOWLIST`, and output is redacted/capped. Codex uses JSONL with workspace-write sandbox and ephemeral sessions. Provider workflow ticks still require `ACP_ENABLE_PROVIDER_EXECUTION=1`, provider configuration, scope, cost gates, audit, and retries.

Workspace verification is a separate allowlisted command path for the supported Rust, JavaScript, Python, Go, and Make toolchains. It records command, status, output/error, latency, timeout, attempt, and timestamp in workspace evidence. A failed check may request at most two CLI repairs; exhausted verification records `verification_failed` and blocks target output.

V2-4 supervised workers reuse the same scheduler and workflow lease path. Legacy startup uses `ACP_ENABLE_SCHEDULER=1` and `ACP_ENABLE_SUPERVISED_WORKERS=1`. IAE-2 may instead enable that path with `ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT=1`, but only when IAE-1 is ready and the scheduler executor is `adaptive_provider`. The adaptive executor is injected and pinned, bypassing generic pool selection. `ACP_SUPERVISED_WORKER_COUNT` is constrained by `ACP_SCHEDULER_MAX_CONCURRENT` and a hard cap of 32; malformed or non-positive trusted-local numeric configuration fails closed. Each worker claims at most one node per cycle, refreshes active policies/evidence/daily cost before execution, and uses the existing adaptive call/token/cost/time/concurrency, identity, redaction, audit, pause, and kill controls. Worker heartbeat state is persisted in existing scheduler heartbeat metadata. `POST /api/v1/scheduler/control` requires `dispatch:execute` and confirmation for pause/resume/kill. Workers only consume already-created workflow runs and do not create autonomous goals or loops.

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
- Dashboard controls may invoke V2-3 output only through the guarded backend contract; they must not write target working trees or `main`, deploy/release/apply code, broaden gates, or bypass backend authorization.

## Current Gaps

These are accepted current limitations, not hidden TODOs:

- V2 real output and its closeout implementation are complete; `v0.1.0` published-asset installation verification passed.
- V2-1 app-owned workspace hardening is implemented, but it is not hard process/container/VM sandboxing and does not authorize target-repository writes.
- Provider API output remains gated. Installed local CLI discovery defaults on, while each execution still requires an explicit workflow tick.
- Hard process/container/VM sandbox isolation is not implemented and is not part of V2-1 unless separately approved.
- V2-3 controlled target output is merged. It creates no merge/deploy/apply authority and preserves the registered target working tree and `main`.
- GitHub PR creation is default-off and adds no merge authority.
- Bounded supervised workers are merged in V2-4 and Mission Control product output UX is merged in V2-5; unattended autonomous-agent loops remain out of scope.
- Cloud SaaS, hosted/cloud deployment, multi-tenant service, and direct release/tag/deploy/apply controls remain out of scope. Current adaptive provider execution is local/small-team, explicit, bounded, authenticated, and gated; IAE may compose those gates behind a validated trusted-local profile.
- Some routing, quality, and orchestration modules remain partially active rather than unified under one policy layer.

The Adaptive Fusion Routing track approved on 2026-06-21 extends `model_selector`, `feedback`, `provider`, storage, and the existing HTTP/workflow/executor boundaries without creating a parallel routing, policy, workflow, or storage kernel. AF-0 through AF-2 provide pure planning, endpoint metadata, and offline evaluation. AF-3 through AF-6 add authenticated bounded execution, contextual policy, parallel panel fusion with serial judge/synthesis, safe observation summaries, controlled experiments, evidence-driven promotion, and `POST /api/v1/adaptive-fusion/completions`. Legacy independent provider/adaptive/experiment/promotion/default-routing gates remain supported.

IAE-1 composes those gates behind `ACP_TRUSTED_LOCAL_PROFILE=1`. The resolver validates protected auth, fixed endpoint metadata, symbolic credential availability, strictly positive endpoint pricing, and positive per-dispatch/daily cost caps. IAE-2 adds a separate `ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT=1` acknowledgement for bounded background advancement through the existing scheduler. Existing call/token/time/concurrency ceilings, provider/model identity, redacted/capped outputs, provider and selection audit events, safe observations, snapshots/rollback, circuit breakers, pause controls, and kill switches remain authoritative in their owning modules. Missing prerequisites and unreadable policy/cost context fail closed; runtime pause/kill state does not destroy readiness, allowing controlled recovery. Target-repository output remains separately gated and never writes registered `main`.

Boundary expansion outside the approved V2, Adaptive Fusion Routing, and IAE tracks requires a new plan, threat-model update, focused tests, and explicit human approval.

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
