# Architecture Book

Last updated: 2026-07-06 (external runtime benchmark boundary)

This is the current architecture baseline for the Token-Efficient Agent Harness Lab. Historical phase plans, closeout reports, and long-form strategy docs are retained in release-tagged git history; `docs/archive/README.md` is the working-tree index.

## Product Boundary

The system is a local/small-team self-hosted macro-orchestrator control plane for studying token-efficient agent workflows. V2 adds auditable real-repository patch/PR production. The approved Trusted Local Autonomous Execution Track may activate bounded provider and agent execution as a coherent local profile while preserving auth, budgets, audit, approval, rollback, and kill controls. It is not a cloud SaaS, hosted multi-tenant service, or direct-deploy tool.

Default posture:

- Provider execution is enabled by a ready `ACP_TRUSTED_LOCAL_PROFILE=1` trusted-local profile or the standalone legacy `ACP_ENABLE_PROVIDER_EXECUTION=1` gate. Both paths require protected auth, configured provider metadata, symbolic credentials, positive pricing/cost caps, audit, redaction, and kill controls; missing prerequisites fail closed.
- Installed local Claude/Codex CLIs are discovered by default; explicit workflow ticks invoke them. `ACP_ENABLE_CLI_EXECUTION=0` disables this path.
- Target output remains off unless `ACP_ENABLE_TARGET_REPO_OUTPUT=1`. V2-3 permits only an app-owned git worktree plus approval-bound patch export or `acp/*` branch push; the registered target working tree and `main` remain protected.
- External runtimes such as LangGraph, CrewAI, and Microsoft Agent Framework are benchmark or trace-ingest targets only. They are not core engine dependencies, replacement runtimes, or new kernels inside this repository.
- No release/tag/deploy/apply controls exist in the app runtime.
- No process/container/VM sandbox isolation is implemented; V2-1 is scoped to app-owned workspace confinement unless separately approved.
- Supervised execution operates only in app-owned detached workspaces and remains explicitly gated.

This file is authoritative for current architecture and safety boundaries. Operational procedures are in `docs/RUNBOOK.md`. Archived security, safety, and other historical notes in release-tagged git history are reference-only; revive or replace them only for an approved boundary-expansion track.

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

## Token-Efficiency Evidence and External Runtime Benchmark Boundary

This repository is the meta-harness, regulator, evaluator, and evidence layer for token-efficient agent workflows. It may compare the native harness against external runtimes, but it must not become a clone of those runtimes.

External runtimes answer how agents, tools, state, memory, and workflow execution run. This repository answers whether a given run used less context, fewer repeated reads, fewer redundant tool calls, and fewer ineffective repair loops while still passing the task and preserving reviewable evidence.

Allowed external-runtime work:

- ingest bounded, redacted trace summaries from LangGraph, CrewAI, Microsoft Agent Framework, or similar systems;
- normalize native and external traces into the same token-efficiency evidence shape;
- compare `native_control_plane`, `stateless_reread`, `stateful_store`, and `pruned_context` modes;
- preserve runtime kind, runtime version, scenario id, mode, pass/fail status, quality method, token counts, tool-call counts, retry counts, duration, and artifact references;
- store raw trace material only as bounded app-owned artifacts after redaction.

Non-goals unless a later documented replacement supersedes this boundary:

- no required dependency on LangGraph, CrewAI, Microsoft Agent Framework, or any other external runtime for the core engine;
- no replacement of `workflow_runs`, `scheduler`, `node_executor`, `provider`, `cli`, or `LocalProductStore`;
- no second scheduler, DAG engine, policy kernel, storage layer, mailbox, or side-channel state system;
- no persistence of raw prompts, raw model outputs, transcripts, credentials, secret-shaped values, repository content, or private paths;
- no provider calls in CI;
- no target-output, merge, deploy, release, or protected-branch authority through an adapter.

The scorecard contract is implemented by `scripts/token_efficiency_scorecard.py`, which validates bounded summaries and emits `token_efficiency_scorecard.v1`. Native harness exports use `scripts/native_scorecard_export.py` to read bounded dispatch/workflow/run/evidence JSON, reuse that validator, and emit a read-only `native_scorecard_artifact.v1` JSON envelope. Native workflow completion now also projects completed, failed, blocked, cancelled, and error terminal runs into the same envelope and persists it through `LocalProductStore` in the `native_scorecard_artifacts` table. The automatic projection is metadata/counter-only, idempotent by artifact id plus content hash, rejects raw trace and secret-shaped fields recursively, and is exposed through read-only scorecard APIs; no second schema or storage layer is introduced. Minimum run-level fields are:

```text
adapter_run_id
schema_version
runtime_kind
runtime_version
scenario_id
mode
state_strategy
status
pass_fail_reason
quality_score
quality_method
input_token_total
output_token_total
context_token_total
repeated_context_token_total
retrieved_ref_token_total
tool_call_count
redundant_tool_call_count
retry_count
step_count
duration_ms
estimated_cost_usd
raw_trace_artifact_id
redaction_status
```

Minimum step-level fields are:

```text
adapter_step_id
adapter_run_id
step_index
node_name
agent_role
operation_kind
input_tokens
output_tokens
context_tokens
repeated_context_tokens
retrieved_refs_count
retrieved_ref_tokens
tool_name
tool_call_id
status
error_kind
state_read_bytes
state_write_bytes
started_at
finished_at
```

Derived metrics should include total tokens, context share, repeated-context ratio, tool-redundancy ratio, tokens per passing run, cost per passing run, and step retry ratio. Token reduction is not success unless the task also passes under the same success criterion.

Implementation order remains importer-first:

1. validate a bounded trace summary — implemented in `scripts/token_efficiency_scorecard.py`;
2. emit `token_efficiency_scorecard.v1` evidence — implemented by the validator and native exporter;
3. store scorecard evidence as a read-only app-owned artifact — implemented as `native_scorecard_artifact.v1` persisted through `LocalProductStore.native_scorecard_artifacts`, including automatic persistence at native workflow terminal-state transitions;
4. compute derived metrics — implemented by the validator;
5. expose read-only scorecards — implemented through `GET /api/v1/scorecards?run_id=...`, `GET /api/v1/scorecards?dispatch_id=...`, `GET /api/v1/scorecards/:artifact_id`, and the operator evidence read-model;
6. only then add runtime-specific runners.

The first external baseline should be LangGraph stateful versus stateless reread. CrewAI and Microsoft Agent Framework should wait until native scorecard export and importer validation are stable.

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

- Current version: v16
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

Workflow node execution is explicit through scheduler/tick paths. `CommandNodeExecutor` rejects shell metacharacters, avoids `sh -c`, uses allowlisted binaries, validates supplied workspace cwd, clears inherited environment except `PATH`, caps output, enforces timeout kill, and emits structured results. Installed Claude/Codex CLIs are discovered by default; `ACP_ENABLE_CLI_EXECUTION=0` disables them. The dashboard receives a startup capability snapshot with only enabled/detected booleans; it exposes no binary paths and grants no execution authority. CLI subprocess env is restricted to `PATH` plus `ACP_CLI_ENV_ALLOWLIST`, and output is redacted/capped. Codex uses JSONL with workspace-write sandbox and ephemeral sessions. Provider workflow ticks require a ready trusted-local profile or the standalone legacy provider gate, plus provider configuration, scope, cost gates, audit, and retries.

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

## Agent Runtime (AR-0) Contract

The system is a deterministic workflow/control-plane runtime extended with bounded multi-agent semantics. AR-0 defines the contract baseline. AR-1 (agent identity, state, mailbox), AR-2 (agent step executor), AR-3 (bounded planning/child tasks/handoff), AR-4 (concurrent multi-agent scheduling), AR-5 (review and debate primitives), and AR-6 (operator evidence read-model) are implemented. The track is complete and sealed through AR-6.

### Definition

`AgentRuntime` is a contract — not an implementation — that specifies how workflow execution is extended with durable agent identity, mailbox delivery, persistent agent state, agent-authored planning, agent-to-agent delegation, cross-agent review, and bounded concurrent step semantics. Every AR phase must implement a subset of this contract by extending existing modules, never by creating a parallel runtime kernel.

### What AgentRuntime Is Not

- Not a full multi-agent runtime implementation. AR-0 is the contract; AR-1 through AR-6 implement bounded multi-agent semantics (identity, mailbox, state, step executor, planning, handoff, concurrency, debate, operator evidence). The track is complete and sealed.
- Not a second runtime kernel. All AR phases extend `workflow_runs`, `scheduler`, `node_executor`, `provider`, `cli`, `storage/local_product_store`, http_server, SDK, and dashboard — never a parallel scheduler, DAG engine, storage layer, or hidden side-channel mailbox.
- Not a replacement for existing safety gates. Provider calls, CLI execution, target-output approval, cost caps, audit, redaction, kill switches, and rollback remain authoritative.
- Not an autonomous loop authority. No AR phase creates unbounded agent goals, unbounded recursive planning, or automatic merge/deploy/release authority.

### Extension Model

AR phases consume the following existing modules, extending them through focused additions:

| Existing module | AR ownership | AR additions |
|---|---|---|
| `engine/src/orchestration/schemas.rs` | Agent/message type contracts | AR-1: `AgentState`, `MailboxMessage` types with delivery status, correlation IDs, role/capability profiles |
| `engine/src/storage/local_product_store/` | Durable agent state, mailbox, agent events | AR-1: `agent_state` table, `agent_mailbox` table with send/read/ack/reply, bounded summary/redaction columns, CRUD methods, audit events |
| `engine/src/workflow/` and `engine/src/scheduler.rs` | Graph mutation, queue leases, wakeups, bounded concurrency | Agent step scheduling, child-task node creation, claim policy for concurrent agent steps |
| `engine/src/node_executor.rs` | Bounded `agent_step` executor | **AR-2 implemented**: `AgentStepExecutor` implementing `NodeExecutor` with `AgentAction` enum, `AgentDecisionFn` closure, env gate + kill switch, observe/decide/act/persist lifecycle, audit events, 11 tests |
| `engine/src/provider/` and `engine/src/cli/` | Gated action execution | Agent actions only through existing provider/CLI gates, cost controls, audit, and redaction |
| `engine/src/http_server/` | Agent-readable endpoints, operator controls | Agent state/mailbox/status endpoints using existing auth scopes |
| `SDKs` and `dashboard/` | Operator visibility, guarded agent controls | Agent state inspection, mailbox counts, step traces, kill/pause controls |
| Existing audit, auth, cost, redaction, kill, rollback, target-output approval | Cross-cutting safety | Every AR phase must document which safety boundaries apply and how they remain enforced |

### Durable Entities (AR status)

| Entity | Owner (module) | AR phase | Purpose |
|---|---|---|---|
| `AgentState` | `storage/local_product_store/` | **AR-1 implemented** | Durable agent identity `(agent_id, run_id)`, role, capability profile, objective, status, bounded scratchpad summary, redaction filter reference, last activity timestamp |
| `AgentMessage` | `storage/local_product_store/` | **AR-1 implemented** | Mailbox row `(message_id, correlation_id, from, to, run_id, node_id, body_ref, status, created_at, read_at, ack_at)` with send/read/ack/reply transitions, audit events, and secret-shaped content rejection |
| `AgentStep` (node-level) | `node_executor.rs` | **AR-2 implemented**: `AgentStepExecutor` runs the observe/decide/act/persist loop within existing node lease/cap/kill boundaries. `AgentAction` enum covers Wait, Complete, scratchpad, mailbox, notes, observations. No scheduler change yet. |
| Agent-child-task proposals | `workflow/` | Bounded workflow node/edge creation requests with auth scope validation and operator visibility |
| Debate/review threads | `workflow/` | Multi-agent review artifacts with verdicts, dissent, evidence links, and merge/approval gates |
| Agent claim/concurrency state | `scheduler.rs` | Extended lease/claim policy for concurrent agent steps with resource locks, joins, and conflict handling |

### Required Safety Invariants (all AR phases)

1. **No unbounded autonomous loops.** Every agent step must have call, token, cost, time, concurrency, retry, and lease bounds. No AR phase may create an unbounded `while` loop, recursive self-calling agent, or unattended goal-creation path.
2. **No direct target-repository `main` writes.** Agent-authored output must still flow through V2 target-output approval gates, evidence capture, integrity checks, and explicit human confirmation.
3. **No hidden raw model memory.** Durable agent state must be summary-bounded, redacted, and auditable. Raw prompts, raw model outputs, and transcripts must never be persisted in agent state, mailbox, or debate artifacts.
4. **No secret persistence.** Mailbox body, agent scratchpad, debate artifacts, and review evidence must reject, truncate, or redact secret-shaped content before storage. Existing secret-scan and redaction paths must be composed into every AR storage write.
5. **No authority bypass.** Agent-created tasks, delegation messages, review requests, and child nodes must flow through the existing auth scopes, scheduler admission, cost gates, audit events, kill switches, and rollback boundaries.
6. **No parallel runtime kernel.** Every AR phase extends `workflow_runs`, `scheduler`, `node_executor`, `provider`, `cli`, `storage/local_product_store`, http_server, SDK, or dashboard. No second scheduler, DAG engine, storage layer, or hidden side-channel mailbox may be introduced.
7. **No automatic merge/deploy/release authority.** Agent output reaching target-output gates still requires approval, evidence verification, and the existing V2-3 export/PR gate. No AR phase may grant merge, deploy, or release authority to an agent.
8. **Rollback must be atomic.** Every AR phase must ship a reversible schema migration (up + down) and document the rollback procedure before the phase is merged. AR-1 uses forward-only migrations (existing repo convention); see AR-1 rollback below. If a future phase adds no storage, the rollback is a code revert plus documented data-consistency step.

### Rollback Model

- **Code revert**: A PR that introduces AR code can be reverted by reverting the merge commit. No irreversible data writes outside app-owned storage.
- **Schema rollback**: Down migrations must be provided for any new agent state or mailbox table. The rollback procedure must delete agent data and confirm no residual agent state remains.
- **Gate disable**: Every AR runtime addition must be behind an env gate (e.g., `ACP_ENABLE_AGENT_RUNTIME=0`) default-off, so operators can disable the new path without reverting code.
- **Data retention**: Agent state and mailbox data in app-owned storage is safe to delete on rollback — it is not user data, target-repository data, or credentials.
- **Kill switch**: A global kill switch for agent step execution must be present before any AR-2 merge, independent of per-agent bounds.

### AR Phase Status

**AR-1 (agent identity, state, mailbox) — implemented.** Durable `agent_state` and `agent_mailbox` tables with SQLite schema, migration v15, `LocalProductStore` CRUD methods, send/read/ack/reply, correlation IDs, run/node links, secret redaction, size caps, and audit events. Tests pass. No agent step executor, scheduler changes, provider/CLI calls, or dashboard UI.

**AR-1 rollback.** Forward-only migrations are the existing repo convention. To roll back AR-1:
  1. Revert the merge commit (`git revert <sha>`).
  2. Stop the runtime (no agent step executor consumes these tables yet).
  3. Drop the tables manually: `DROP TABLE IF EXISTS agent_mailbox; DROP TABLE IF EXISTS agent_state;`
  4. Set `PRAGMA user_version = 13` (SQLite) or delete the `schema_migrations` row for version 14 (PostgreSQL).
  5. Confirm no residual agent state remains.
  6. No env gate is needed for AR-1 storage: these tables are inert without an agent step executor (AR-2). No target-repository data, credentials, provider state, scheduler state, or raw model content is stored. All data is app-owned and safe to delete.

**AR-2 (agent step executor) — implemented.** An `AgentStepExecutor` in `node_executor.rs` implements `NodeExecutor` with a one-step `observe → decide → act → persist` lifecycle. `AgentAction` enum supports Wait, Complete, ReadMailbox, AckMessage, UpdateScratchpadSummary, EmitNote, RecordObservation, and Unsupported (fails closed). The executor loads `AgentState`, counts mailbox backlog, calls the injected `AgentDecisionFn`, dispatches the chosen action via existing `LocalProductStore` methods, appends audit events per transition, and returns a structured `NodeExecutionOutput`. Env gate `ACP_ENABLE_AGENT_RUNTIME=1` is required for live execution; kill switch `ACP_AGENT_RUNTIME_KILL_SWITCH=1` overrides. Fails closed on: missing agent state, missing `agent_id`, unsupported action, disabled runtime, or killed runtime. 11 tests pass. No provider/CLI calls, no scheduler change, no DB migration, no dashboard UI, no concurrent agent semantics.

**AR-2 rollback.** AR-2 adds code only — no storage schema changes. Rollback is a clean revert of the merge commit. The `ACP_AGENT_RUNTIME_KILL_SWITCH` env gate was added per the AR-0 safety invariant requirement; after revert, both `ACP_ENABLE_AGENT_RUNTIME` and `ACP_AGENT_RUNTIME_KILL_SWITCH` become inert (no code reads them). No data cleanup is needed because AR-2 uses existing AR-1 tables. The `NodeExecutor` trait already supports `agent_step` as a `task_type`; after revert, unknown `task_type` falls through to existing error handling.

**AR-3 (bounded planning, child tasks, handoff) — implemented.** `engine/src/storage/local_product_store/schema.rs` v15 adds an `agent_proposals` table. `AgentMessageKind::ProposalUpdate`, `AgentAction::ProposeChildTask`, `AgentAction::RequestHandoff`, `AgentAction::AcceptHandoff`, `AgentAction::RejectHandoff`, and `AgentAction::CancelProposal` are implemented in the step executor with redaction, size caps, and safety gates. 12 dedicated tests pass. See `docs/NEXT_DECISION.md` § Agent Runtime Track.

**AR-4 (bounded concurrent multi-agent scheduling) — implemented.** Adds `agent_max_concurrent_global` (default 2) and `agent_max_concurrent_per_run` (default 1) to `SchedulerConfig` with env overrides. Cap enforcement is race-condition-free inside the lease transaction. Audit events cover the full lifecycle. The scheduler runtime passes caps on every tick. 8 new tests pass. See `docs/NEXT_DECISION.md` § Agent Runtime Track.

**AR-5 (bounded review and debate primitives) — implemented.** CAS-style debate round update, bounded review/debate primitives, and state-machine correctness fixes. Tests pass. See `docs/NEXT_DECISION.md` for details.

**AR-6 (operator evidence read-model) — implemented.** Adds a read-only operator evidence surface at `GET /api/v1/operator/evidence/:run_id`. It aggregates agent state, mailbox/proposal counts, blocked signals, and sanitized audit events. No new execution authority; AR-1 to AR-5 runtime semantics unchanged. Provider/CLI authority unchanged. Target-output approval unchanged. No autonomous merge/deploy/release authority added.

**Later AR phases — not implemented:**

- Any provider/CLI execution path changes
- Any AR-specific DB migration beyond AR-3 v15
- Any scheduler lease/claim policy change
- Any hidden mailbox, side channel, or second runtime kernel
- Any automatic target-output merge/deploy/release authority

These phases are described in `docs/NEXT_DECISION.md`. AR-0 does not implement them, does not claim they are implemented, and does not create infrastructure that presupposes a specific implementation.

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
- Provider API output is available through the ready trusted-local profile or standalone legacy gates. Installed local CLI discovery defaults on, while each execution still requires an explicit workflow tick.
- Hard process/container/VM sandbox isolation is not implemented and is not part of V2-1 unless separately approved.
- V2-3 controlled target output is merged. It creates no merge/deploy/apply authority and preserves the registered target working tree and `main`.
- GitHub PR creation is default-off and adds no merge authority.
- Bounded supervised workers are merged in V2-4 and Mission Control product output UX is merged in V2-5; unattended autonomous-agent loops remain out of scope.
- Bounded multi-agent runtime semantics are implemented through Agent Runtime AR-0 through AR-6. The track is sealed; extending the AR phase ladder requires a new decision baseline — see `docs/NEXT_DECISION.md` § Agent Runtime AR-0 through AR-6 Closeout.
- External runtime adapters are not implemented. Current architecture permits only importer-first, benchmark-oriented trace normalization through existing evidence, artifact, audit, and storage boundaries.
- Cloud SaaS, hosted/cloud deployment, multi-tenant service, and direct release/tag/deploy/apply controls are not implemented. Full Agent Autonomy Mode may evolve these repo-scoped designs through documented, testable, observable, reviewable, and rollbackable changes. The only hard stops are real-secret commits, falsified test/CI evidence, intentionally hidden failures, removed rollback paths, and irreversible external destruction without recovery.
- Some routing, quality, and orchestration modules remain partially active rather than unified under one policy layer.

The Adaptive Fusion Routing track approved on 2026-06-21 extends `model_selector`, `feedback`, `provider`, storage, and the existing HTTP/workflow/executor boundaries without creating a parallel routing, policy, workflow, or storage kernel. AF-0 through AF-2 provide pure planning, endpoint metadata, and offline evaluation. AF-3 through AF-6 add authenticated bounded execution, contextual policy, parallel panel fusion with serial judge/synthesis, safe observation summaries, controlled experiments, evidence-driven promotion, and `POST /api/v1/adaptive-fusion/completions`. Legacy independent provider/adaptive/experiment/promotion/default-routing gates remain supported.

IAE-1 composes those gates behind `ACP_TRUSTED_LOCAL_PROFILE=1`. The resolver validates protected auth, fixed endpoint metadata, symbolic credential availability, strictly positive endpoint pricing, and positive per-dispatch/daily cost caps. IAE-2 adds a separate `ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT=1` acknowledgement for bounded background advancement through the existing scheduler. Existing call/token/time/concurrency ceilings, provider/model identity, redacted/capped outputs, provider and selection audit events, safe observations, snapshots/rollback, circuit breakers, pause controls, and kill switches remain authoritative in their owning modules. Missing prerequisites and unreadable policy/cost context fail closed; runtime pause/kill state does not destroy readiness, allowing controlled recovery. Target-repository output remains separately gated and never writes registered `main`.

IAE-3 does not add another control kernel. The dashboard snapshot derives effective authority, cost/traffic/worker bounds, safe observation aggregates, and a secret-free scheduler summary from the existing trusted-local, adaptive policy, cost gate, observation store, and scheduler modules. Live completion readiness mirrors the completion handler and requires provider/adaptive/auth gates, executor, registry, local storage, and a clear fusion kill switch. Default routing, experiment, and auto-promotion effective authority reuse that readiness; experiment and promotion policy validation also fails closed and returns only stable blocker codes. Scheduler pause/resume/kill uses the existing confirmed `dispatch:execute` endpoint. Recent evidence uses the existing `audit:read` endpoint with redaction and renders only action, resource, and timestamp; audit details, raw model content, credentials, repository content, and private paths are excluded. Policy rollback continues through the existing hash-bound snapshot endpoint.

Full Agent Autonomy Mode permits boundary expansion beyond V2, Adaptive Fusion Routing, and IAE when the change has a documented plan, threat-model update where relevant, focused tests, observable evidence, CI review, and a rollback path.

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

Archived materials are retained for audit/history, not daily reading. See `docs/archive/README.md` for the retained index and use release-tagged git history for the archived artifacts.
