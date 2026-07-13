# Architecture Book

Last updated: 2026-07-12 (PE-4 post-close semantic, provenance, boundedness, and calibration repair)

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
- store only bounded summary-level evidence and opaque source hashes/references; raw trace material is not accepted.

Non-goals unless a later documented replacement supersedes this boundary:

- no required dependency on LangGraph, CrewAI, Microsoft Agent Framework, or any other external runtime for the core engine;
- no replacement of `workflow_runs`, `scheduler`, `node_executor`, `provider`, `cli`, or `LocalProductStore`;
- no second scheduler, DAG engine, policy kernel, storage layer, mailbox, or side-channel state system;
- no persistence of raw prompts, raw model outputs, transcripts, credentials, secret-shaped values, repository content, or private paths;
- no provider calls in CI;
- no target-output, merge, deploy, release, or protected-branch authority through an adapter.

The scorecard contract is implemented by `scripts/token_efficiency_scorecard.py`, which validates bounded summaries and emits `token_efficiency_scorecard.v1`. Native harness exports retain the read-only `native_scorecard_artifact.v1` envelope. Runtime-neutral imports use the backward-compatible `scorecard_artifact.v2` envelope and preserve the scorecard's real `runtime_kind`; LangGraph evidence is never relabeled `native_harness`. Both versions persist through `LocalProductStore` in the existing `native_scorecard_artifacts` table and existing API/operator evidence boundary. No second schema, store, importer, scheduler, or runner is introduced.

Before persistence, the store recomputes derived metrics from trusted run counters, canonicalizes the scorecard JSON, recomputes SHA-256, and rejects derived or content-hash mismatches. New imports are bounded to a 1 MiB artifact, 1 KiB JSON strings/keys, 1,000 array items, 128 object fields, and 16 nested levels; the file importer enforces the byte ceiling before parsing. These write-time limits do not alter legacy-row reads. V2 artifacts require a comparison contract binding scenario/task digests, runtime/version, provider/model, tokenizer, pricing/rates, quality method/threshold, evaluator version, redaction policy, retry policy, and seed. Scenario comparison requires exactly one explicit `stateless_reread` baseline and one `stateful_store` candidate with identical contracts. Token or cost advantage is reported only when both runs meet the shared quality threshold. Minimum run-level fields are:

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
comparison_contract (required for scorecard_artifact.v2)
evidence_provenance (optional bounded capture metadata)
```

Step-level fields are optional for summary-only external evidence. When steps are supplied, their count and bounded fields are validated:

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

Derived metrics include total tokens, context share, repeated-context ratio, tool-redundancy ratio, tokens per passing run, cost per passing run, and step retry ratio. They are never trusted from input. Token reduction is not success unless both baseline and candidate meet the same quality threshold.

Implementation order remains importer-first:

1. validate a bounded trace summary — implemented in `scripts/token_efficiency_scorecard.py`;
2. emit `token_efficiency_scorecard.v1` evidence — implemented by the validator and native exporter;
3. store scorecard evidence as a read-only app-owned artifact — native v1 and generic v2 both persist through `LocalProductStore.native_scorecard_artifacts`;
4. recompute derived metrics and canonical content hash — implemented in Python normalization and Rust persistence;
5. expose read-only scorecards — implemented through run, dispatch, artifact-id, and scenario queries plus operator evidence;
6. import bounded LangGraph summaries and compare strict same-scenario stateful/stateless scorecards — implemented in `scripts/langgraph_trace_import.py` and the shared comparator;
7. persist and display the first real importer-first pilot — implemented with fixed offline LangGraph 1.2.9 fixtures and Dashboard scenario deltas; the hash-bound `tools/capture_langgraph_pilot.py` is a developer-invoked one-shot evidence harness with no app, scheduler, provider, store, or CI integration;
8. consider embedded/scheduled/provider-backed runtime-specific runners only through a later explicit decision.

The first external baseline is complete: LangGraph stateful store versus stateless reread, captured offline with no provider/model call. This one-shot developer capture authorization does not change the product's importer-first boundary and cannot execute through the app. Rollback requires no database migration: revert the implementation commit. Existing v2 rows remain JSON-readable in the old table/API; optional data removal should use a verified backup rather than an automatic destructive down migration. CrewAI and Microsoft Agent Framework remain deferred.

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

- Current version: v21. v18 adds immutable, hash-bound `budget_evidence_artifacts`; v19 adds `budget_pause_decisions`, the atomic audit/recovery record for the existing workflow-run pause owner; v20 adds `offline_replay_artifacts`, an immutable hash-bound read model for trace-backed offline replay reports; v21 adds nullable recorder-owned trace schema/hash bindings to `dispatch_history` for trusted replay provenance. Auto-pause is default-off and requires explicit `dispatch:execute` confirmation plus validated, fresh, pricing-complete, high-confidence critical anomaly evidence scoped to the target run. Decision insert, audit, and pause update commit atomically; repeated evidence is idempotent, and resume/override requires a bounded operator reason while preserving cause and evidence hash. Offline replay recording is idempotent, metadata-only audited, and validates report/policy/source hashes on write and read. No kill, provider/model, reservation, scheduler, routing, policy, experiment, promotion, or target-output authority is added. Rollback is a code revert that leaves additive tables inert; existing pause/recovery and replay evidence remains readable in storage. Old dispatch rows without v21 provenance remain readable but cannot establish trusted replay evidence.
- SQLite uses WAL and app-managed backup/restore.
- PostgreSQL disables app-managed backup; operators use `pg_dump` or managed backup.
- PostgreSQL integration tests are gated behind `cargo test -p engine --features pg-tests`.

## Operator Decision Contracts

PE-3 uses additive `operator_decision_source.v1`, `operator_decision_item.v1`, and `operator_decision_queue.v1` Rust contracts in `engine/src/operator_decision/mod.rs`. They normalize bounded references to existing approval, recovery, rollback, budget, policy, workflow, scheduler, and benchmark evidence without becoming a new source of truth. Resolution is deterministic: severity, fixed source precedence, confidence, observation time, then lexical source ID. Exact source duplicates collapse; equal-ranked incompatible actions become an explicit conflict. Expired, stale, low-confidence, informational, resolved, and insufficient sources cannot produce a ready recommendation.

`LocalProductStore::operator_decision_queue` recomputes the queue through existing SQLite/PostgreSQL readers; it does not persist queue rows, emit audits, or mutate any source. Source reads and returned pagination are bounded, output is deterministically ordered and hash-bound, and an unreadable evidence owner fails the complete derivation closed instead of silently omitting decisions. Restart behavior is therefore the same deterministic recomputation over existing truth owners. Only a future allowlisted adapter may connect `ready` items to existing control owners, and it must preserve their confirmation, permission, audit, idempotency, compensation, and rollback gates. Contract and queue rollback is a code revert with no migration or data cleanup.

The read surface is additive `GET /api/v1/operator/decisions`, documented in OpenAPI and exposed by the existing Python/TypeScript SDKs and Dashboard navigation. It requires `dispatch:read`, returns the hash-bound derived queue plus explicit read-only boundary metadata, accepts bounded pagination/freshness parameters, and offers no mutation control. It may use a supplied timestamp for deterministic evidence review; otherwise it derives using server time. Route rollback is a code revert with no stored state.

PE3-ACTIONS remains a narrowly allowlisted adapter, not an execution authority. Read-only replay may use a deterministic caller time, but mutation compares that time with the store clock, rejects stale or future reads, re-derives the exact bound and current pages, and binds decision, conflict key, resource, action, source kind/ID/hash, pagination, and freshness before invoking an owner. Derived sources include bounded original evidence references; absent trustworthy hashes remain absent. Approval resolution is atomic inside the existing workflow owner for SQLite and PostgreSQL. Retry is exposed only for blocked runs with a ready node; terminal failed/completed/cancelled runs are not ready recommendations. Budget pause/recovery retains `dispatch:execute`, policy, audit, idempotency, and recovery gates. Unsupported rollback/inspect/acknowledge requests fail closed. Rollback is a code revert with no migration or queue cleanup.

PE-3 is acceptance-sealed after the independent PE3-REPAIR-1 repair and PE3-CLOSE-1 audit. The repair corrected observation-time precedence to compare parsed instants, preserving deterministic tie-breakers and all existing authority boundaries. No queue persistence, second action owner, migration, or Dashboard mutation control was introduced.

## Trace-backed Policy Replay Contract

Historical record: PE4-CONTRACT-1 and PR #197 replaced the caller-asserted replay gate with `policy_replay_contract.v2` and `trace_replay_evidence.v1` in `engine/src/feedback/replay_eligibility.rs`. `PE4-POST-CLOSE-REPAIR-1` supersedes those weaker semantics with v3/v2 while the repair is active. The owner still derives normalized observations, bounded original references, and canonical content hashes from `RunTrace`/`RunTraceRecorder`, persisted feedback/attribution sections, and actual evaluation sections; caller booleans, candidate definitions, and coverage claims are never evidence.

PE4-OFFLINE-1 adds deterministic comparable-cohort replay through `OfflineEvaluationEngine::replay_policies`. It accepts raw `ReplayEligibilityRequest` trace-owner input and derives eligibility internally, so a caller cannot establish completeness or trust by submitting a fabricated eligible result. Explicit versioned current and candidate policy definitions are content-hash bound; observed facts remain separate from counterfactual estimates, which may only reuse accepted comparable candidate cohorts and retain source trace/evidence hashes. Insufficient, incompatible, stale, tampered, uncalibrated, and OOD outcomes are explicit and hash-bound. The report is always shadow-only with all live-influence flags false, no provider/model substitution or provider call, and no mutation of routing, policy, budgets, experiments, or production state.

Offline replay and shadow comparison remain derived, read-only evidence and cannot mutate live routing or policy. Canary, promotion, pause, resume, and rollback must reuse existing owners and retain confirmation, permission, audit, idempotency, scope, duration, recovery, and rollback gates. No offline or shadow result alone may authorize promotion, and no replacement owner is permitted without an explicit replacement decision, compatibility/migration evidence, and rollback.

PE4-READ-1 exposes the existing offline replay report through `LocalProductStore::offline_replay_artifacts`, `GET /api/v1/offline-replays`, and the encoded Python/TypeScript SDK readers. The artifact is bounded to the existing metadata-only artifact size/depth limits, ordered by store sequence, filtered by explicit replay status, and revalidated on every read so tampering is an error rather than an empty result. SQLite v20 uses an additive migration; PostgreSQL current DDL and the v19-to-v20 migration create the same table/index contract, and the integrity owner includes the new table. The existing DynamicRegulator renders empty, insufficient/invalid/OOD, and transport-error states. There is no write HTTP route, provider invocation, policy mutation, or promotion authority.

PE4-SHADOW-1 adds `ShadowRouter::compare_replay_report` as a derived, hash-bound comparison owner. It carries observed facts separately from counterfactual predictions, source trace/evidence coverage, explicit drift boundary and insufficiency/OOD statuses, and all live-influence flags remain false. It does not invoke providers or mutate routing, policy, experiments, budgets, or production state.

PE4-CANARY-1 extends the existing `AdaptiveExperimentController` rather than creating a second experiment owner. `AdaptiveCanaryDecision` is a deterministic, hash-bound, non-persistent decision envelope requiring exact policy/candidate/scope bindings, trace-backed shadow coverage, explicit confirmation and permission, bounded 1–5% traffic and 24-hour duration, existing gate/pause/kill controls, compensation metadata, and an exact rollback target. Repeated evaluation is idempotent and restart-safe; the decision contract does not call providers, mutate live routing or policy, or authorize full rollout. Promotion remains a separate later owner and requires the complete evidence chain.

PE4-PROMOTION-1 extends the existing `AdaptiveAutoPromotionController` and `LocalProductStore` promotion owner. `AdaptivePromotionEvidenceChain` binds a sufficient offline report, the exact shadow comparison re-derived from that report, a validated started canary decision, rollout scope, rollback target, and canonical content hash. Promotion rejects caller-only evidence, invalid or stale/tampered/uncalibrated/OOD evidence, incompatible candidate/policy/trace bindings, missing coverage, and failed sample/confidence/quality/cost/latency/failure guardrails. The accepted policy records the source chain hash and continues through existing confirmation, permission, audit, snapshot, pause, compensation, and rollback behavior; offline/shadow evidence alone never authorizes promotion.

Historical pre-repair closeout record: PE4-CLOSE-1 independently re-audited the complete trace-to-promotion chain and found no remaining implementation defect. PR #203 merged as `008bc8c8879d6e7c9641fec57aa974f98af1c6b5` from exact head `2110676667dd1b57a36bc6f3744016599a02860a`; exact-head CI `29186113263` and final post-merge `main` CI `29186372526` passed all seven required jobs. That acceptance claim is superseded by `PE4-POST-CLOSE-REPAIR-1`, which retains the derived, non-mutating-before-guarded-promotion, and rollbackable boundaries; PE-5 and PE-6 are not activated.

### PE4-POST-CLOSE-REPAIR-1 — active correction boundary

The pre-repair PE-4 closeout is under correction and must not be used as final acceptance evidence for repaired semantics. The repair keeps the existing owners and binds trusted replay input to the persisted `dispatch_history` row through `RunTraceRecorder`, the owner history ID, `dispatch_history_trace_owner.v1`, and an independently checked recorder hash. The public raw import constructor remains permanently untrusted, and request deserialization cannot construct an eligible replay request. SQLite and PostgreSQL use the same additive schema v21 nullable columns; missing or mismatched owner bindings fail closed and do not fabricate a hash.

The current contracts are `policy_replay_contract.v3`, `trace_replay_evidence.v2`, `offline_policy_replay.v2`, and `judge_calibration.v1`. Coverage is integer-based and inclusive at 90%; the central rejection taxonomy distinguishes observation-local coverage failures from cohort-fatal evidence failures and request-fatal contract failures. Recorder outcome semantics distinguish terminal execution, execution result, evaluation completion/result, overall dispatch success, quality, and tool success, so completed-but-low-quality and failed-but-measured observations remain valid negative samples when consistent. Judge calibration uses paired judge/reference values, at least 3 samples, absolute bias tolerance 0.10, and MAE tolerance 0.15; non-judge quality paths do not require calibration.

All relevant canonical bytes, raw sections, identifiers, arrays, report cardinalities, references, JSON depth, result size, numeric envelopes, and token additions are bounded. Canonical representations are precomputed before deterministic ordering, and serialization/overflow failures fail closed. Caller-provided scope is only a constraint; empirical task/domain/intent/objective, candidate definition/member set, policy/cohort, complexity, and cost/latency/token/retry support must be present in accepted observations. Unsupported counterfactuals return explicit OOD or insufficient evidence. Current v2 artifacts may authorize existing downstream validation; old v1 artifacts remain readable as historical-only and cannot authorize shadow, canary, or promotion. Rollback is a code revert with v21 columns and old rows preserved/inert.

The PE-4 correction is acceptance-sealed under packet `PE4-POST-CLOSE-REPAIR-1` in the single implementation PR #206. The exact implementation head `655483670214741817f713d1715b1630c7ddedff` passed full required CI run `29190133210` (all seven jobs green); the final documentation-head CI and post-merge `main` verification are required release evidence and are recorded with the PR. The weaker pre-repair semantics are superseded, existing permission/confirmation/audit/pause/compensation/snapshot/rollback owners remain authoritative, and PE-5/PE-6 remain unstarted.

## Execution Modes

`ACP_EXECUTION_MODE` controls dispatch execution:

| Mode | Behavior |
|---|---|
| `off` | Default noop behavior; no external calls |
| `provider` | Provider API only; requires provider gate/auth/cost controls |
| `cli` | CLI executor only; requires CLI gate |
| `auto` | Hybrid provider/CLI routing by complexity threshold |

Workflow node execution is explicit through scheduler/tick paths. `CommandNodeExecutor` rejects shell metacharacters, avoids `sh -c`, uses allowlisted binaries, validates supplied workspace cwd, clears inherited environment except `PATH`, caps output, enforces timeout kill, and emits structured results. Installed Claude/Codex CLIs are discovered by default; `ACP_ENABLE_CLI_EXECUTION=0` disables them. The dashboard receives a startup capability snapshot with only enabled/detected booleans; it exposes no binary paths and grants no execution authority. CLI subprocess env is restricted to `PATH` plus `ACP_CLI_ENV_ALLOWLIST`, and output is redacted/capped. Codex uses JSONL with workspace-write sandbox and ephemeral sessions. Provider workflow ticks require a ready trusted-local profile or the standalone legacy provider gate, plus provider configuration, scope, cost gates, audit, and retries. The live local comparison runner additionally requires a persistent `LocalProductStore` audit sink, positive input/output pricing, a pre-call worst-case token/cost reservation, shared run/daily caps, provider timeout, and `ACP_LOCAL_RUNNER_KILL_SWITCH`; missing evidence fails closed before a provider call.

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

Dynamic controller tick and mutation ceilings are recovered from the durable workflow event log on every tick, so scheduler reconstruction and process restart cannot reset the per-run bounds. Dynamic pool routing may use an existing feedback suggestion only when the suggested executor is registered and can be acquired; explicit executor configuration remains pinned. Per-run controller errors propagate to scheduler worker error evidence instead of being reported as a successful tick.

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
| `AgentState` | `storage/local_product_store/` | **AR-1 implemented** | Durable agent identity `(agent_id, run_id)`, role, capability profile, objective, status, bounded scratchpad summary, `metadata_json.memory_digest`, redaction filter reference, last activity timestamp |
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

**AR-1 (agent identity, state, mailbox) — implemented.** Durable `agent_state` and `agent_mailbox` tables with SQLite/PostgreSQL migration v14, `LocalProductStore` CRUD methods, send/read/ack/reply, correlation IDs, run/node links, secret redaction, size caps, and audit events. Tests pass. No agent step executor, scheduler changes, provider/CLI calls, or dashboard UI.

**AR-1 rollback.** Forward-only migrations are the existing repo convention. To roll back AR-1:
  1. Revert the merge commit (`git revert <sha>`).
  2. Stop the runtime (no agent step executor consumes these tables yet).
  3. Drop the tables manually: `DROP TABLE IF EXISTS agent_mailbox; DROP TABLE IF EXISTS agent_state;`
  4. Set `PRAGMA user_version = 13` (SQLite) or delete the `schema_migrations` row for version 14 (PostgreSQL).
  5. Confirm no residual agent state remains.
  6. No env gate is needed for AR-1 storage: these tables are inert without an agent step executor (AR-2). No target-repository data, credentials, provider state, scheduler state, or raw model content is stored. All data is app-owned and safe to delete.

**AR-2 (agent step executor) — implemented.** An `AgentStepExecutor` in `node_executor.rs` implements `NodeExecutor` with a one-step `observe → decide → act → persist` lifecycle. `AgentAction` enum supports Wait, Complete, ReadMailbox, AckMessage, UpdateScratchpadSummary, EmitNote, RecordObservation, and Unsupported (fails closed). The executor loads `AgentState`, counts mailbox backlog, calls the injected `AgentDecisionFn`, dispatches the chosen action via existing `LocalProductStore` methods, appends audit events per transition, and returns a structured `NodeExecutionOutput`. Env gate `ACP_ENABLE_AGENT_RUNTIME=1` is required for live execution; kill switch `ACP_AGENT_RUNTIME_KILL_SWITCH=1` overrides. Fails closed on: missing agent state, missing `agent_id`, unsupported action, disabled runtime, or killed runtime. 11 tests pass. No provider/CLI calls, no scheduler change, no DB migration, no dashboard UI, no concurrent agent semantics.

**Agent memory policy layer — implemented as AR maintenance, not a new AR phase.** Memory is embedded in existing `AgentState` and context assembly paths. `engine/src/agent_memory.rs` normalizes bounded memory digests from `AgentState.metadata_json["memory_digest"]` with scratchpad fallback, redacts secret-shaped values and private paths, filters source refs, estimates state bytes, and builds node-metadata-only memory context. `AgentStepExecutor` observe attaches bounded `memory_digest`, `memory_context`, and `memory_state_read_bytes` to `AgentStepContext`; `UpdateScratchpadSummary` keeps `scratchpad_summary` as the human-readable summary and synchronizes `metadata_json.memory_digest` through the existing `update_agent_state` metadata merge path. Workflow tick context injection may include bounded memory metadata for `agent_step` nodes through the existing `context_injection` object, preserving `injection_surface = "node_metadata_only"`. Operator evidence exposes only aggregate memory metadata (`memory_digest_present`, source-ref count, updated timestamp, estimated bytes), never raw memory, scratchpad, objective, prompts, outputs, transcripts, private paths, credentials, or metadata JSON. No DB table, migration, scheduler, mailbox, storage layer, external framework, provider call, target-output authority, or new runtime is introduced.

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
- The bounded LangGraph importer and same-scenario comparison are implemented, but no external runtime runner is embedded and no real LangGraph benchmark evidence is persisted automatically. Current architecture permits only importer-first, operator-supplied trace-summary normalization through existing evidence, artifact, audit, and storage boundaries.
- Cloud SaaS, hosted/cloud deployment, multi-tenant service, and direct release/tag/deploy/apply controls are not implemented. Full Agent Autonomy Mode may evolve these repo-scoped designs through documented, testable, observable, reviewable, and rollbackable changes. The only hard stops are real-secret commits, falsified test/CI evidence, intentionally hidden failures, removed rollback paths, and irreversible external destruction without recovery.
- Some routing, quality, and orchestration modules remain partially active rather than unified under one policy layer.

The Adaptive Fusion Routing track approved on 2026-06-21 extends `model_selector`, `feedback`, `provider`, storage, and the existing HTTP/workflow/executor boundaries without creating a parallel routing, policy, workflow, or storage kernel. AF-0 through AF-2 provide pure planning, endpoint metadata, and offline evaluation. AF-3 through AF-6 add authenticated bounded execution, contextual policy, parallel panel fusion with serial judge/synthesis, safe observation summaries, controlled experiments, evidence-driven promotion, and `POST /api/v1/adaptive-fusion/completions`. Legacy independent provider/adaptive/experiment/promotion/default-routing gates remain supported.

IAE-1 composes those gates behind `ACP_TRUSTED_LOCAL_PROFILE=1`. The resolver validates protected auth, fixed endpoint metadata, symbolic credential availability, strictly positive endpoint pricing, and positive per-dispatch/daily cost caps. IAE-2 adds a separate `ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT=1` acknowledgement for bounded background advancement through the existing scheduler. Existing call/token/time/concurrency ceilings, provider/model identity, redacted/capped outputs, provider and selection audit events, safe observations, snapshots/rollback, circuit breakers, pause controls, and kill switches remain authoritative in their owning modules. Missing prerequisites and unreadable policy/cost context fail closed; runtime pause/kill state does not destroy readiness, allowing controlled recovery. Target-repository output remains separately gated and never writes registered `main`.

IAE-3 does not add another control kernel. The dashboard snapshot derives effective authority, cost/traffic/worker bounds, safe observation aggregates, and a secret-free scheduler summary from the existing trusted-local, adaptive policy, cost gate, observation store, and scheduler modules. Live completion readiness mirrors the completion handler and requires provider/adaptive/auth gates, executor, registry, local storage, and a clear fusion kill switch. Default routing, experiment, and auto-promotion effective authority reuse that readiness; experiment and promotion policy validation also fails closed and returns only stable blocker codes. Scheduler pause/resume/kill uses the existing confirmed `dispatch:execute` endpoint. Recent evidence uses the existing `audit:read` endpoint with redaction and renders only action, resource, and timestamp; audit details, raw model content, credentials, repository content, and private paths are excluded. Policy rollback continues through the existing hash-bound snapshot endpoint.

Full Agent Autonomy Mode permits boundary expansion beyond V2, Adaptive Fusion Routing, and IAE when the change has a documented plan, threat-model update where relevant, focused tests, observable evidence, CI review, and a rollback path.

## Event-Driven Agent Orchestrator

The GitHub Actions orchestrator is a separate repository-maintenance control plane, not an engine runtime replacement. It is disabled by default and is governed by exactly one open control Issue with identity label `agent-control`, title `[agent-control] Orchestrator controls`, and marker `<!-- agent-orchestrator-control:v1 -->`. `agent-orchestrator-enabled` permits work only when `agent-emergency-stop` is absent; `agent-auto-merge-enabled` additionally permits merge. Missing, duplicate, malformed, closed, or unreadable control state fails closed.

Vader runs short-lived Codex CLI processes using its cached interactive login. Codex gets an isolated worktree and no workflow GitHub or push credential. It must leave the recorded worktree HEAD unchanged, stage only local changes, and return an untrusted binary `agent.patch` plus schema-versioned `agent-result.json`. A task Issue must declare `<!-- agent-orchestrator-scope:v1 {"allowed_paths":[...]} -->`; the GitHub-hosted finalizer independently validates that scope together with the manifest/bindings/checksum/size/path list, rejects forbidden paths or a moved remote head, applies the patch to a clean exact checkout, recomputes changed paths, rechecks live controls, then owns the commit, branch push, PR update, state write, and exact-head CI dispatch.

`AGENT_PUSH_TOKEN` is a fine-grained PAT with only Contents read/write. It exists only in each finalizer's isolated push step; all other GitHub actions use `${{ github.token }}` with explicit permissions. The push step uses a temporary `GIT_ASKPASS` directory and does not alter Vader's global Git or GitHub CLI configuration. After a push, finalization observes one exact branch/SHA `tests` run, falls back to one `workflow_dispatch` only when no natural run appears, persists the selected run and duplicate IDs, and makes workflow completion handling idempotent. CI failure repair starts from the exact failed canonical run ID and head, fetches bounded redacted evidence in a GitHub-hosted preparation job, and reuses the artifact finalizer. A fresh read-only Vader review may authorize only exact `PASS`; `agent-merge-ready` waits without consuming capacity when auto-merge is disabled, while non-PASS output is `agent-review-blocked` and requires an explicit retry. Merge revalidates exact head, binding, seven canonical jobs, review/objections, and current mergeability, then relies on the GitHub merge API to enforce any server-side rulesets.

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
