# Architecture Book

Last updated: 2026-07-14.

This is the current architecture baseline for the Token-Efficient Agent Harness Lab. Historical phase plans, closeout reports, and long-form strategy docs are retained in release-tagged git history; `docs/archive/README.md` is the working-tree index.

## Product Boundary

The system is a local/small-team self-hosted macro-orchestrator control plane for studying token-efficient agent workflows. V2 adds auditable real-repository patch/PR production. The approved Trusted Local Autonomous Execution Track may activate bounded provider and agent execution as a coherent local profile while preserving auth, budgets, audit, approval, rollback, and kill controls. It is not a cloud SaaS, hosted multi-tenant service, or direct-deploy tool.

Default posture:

- Provider execution is enabled by a ready `ACP_TRUSTED_LOCAL_PROFILE=1` trusted-local profile or the standalone legacy `ACP_ENABLE_PROVIDER_EXECUTION=1` gate. Both paths require protected auth, configured provider metadata, symbolic credentials, positive pricing/cost caps, audit, redaction, and kill controls; missing prerequisites fail closed.
- Managed CLI execution is default-off. `ACP_ENABLE_CLI_EXECUTION=1` may register Codex through its existing `workspace-write` adapter. Claude Code has a separate default-off gate and registers only after exact regular-file path/version/SHA-256/model admission; its product-only invocation permits at most three tool turns under one pre-reserved token/client-estimate budget, uses safe-mode/no-session, strict-empty-MCP, `dontAsk`, Read/Edit/Write-only access, bounds reads to the app-owned worktree, scopes edits/writes to admitted paths, and explicitly denies Bash, network, notebook, and subagent tools. `ACP_CLI_EXECUTION_KILL_SWITCH=1` disables both. Direct `ACP_EXECUTION_MODE=cli`, `ACP_EXECUTION_MODE=auto`, and multi/CLI dispatch are retired.
- Target output remains off unless `ACP_ENABLE_TARGET_REPO_OUTPUT=1`. V2-3 permits only an app-owned git worktree plus approval-bound patch export or `acp/*` branch push; the registered target working tree and `main` remain protected.
- External runtimes such as LangGraph remain bounded node adapters or trace-ingest targets. They are not core engine dependencies, replacement runtimes, schedulers, queues, permission owners, or authoritative product stores.
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
| Execution | `dispatch_engine.rs`, `provider/`, `cli/cli_node_executor.rs`, `node_executor.rs`, `tool_policy_executor.rs` | Direct noop/provider dispatch plus scheduler-owned provider/CLI/command workflow execution behind explicit gates |
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

The first external baseline remains valid historical offline evidence. Migration v24 additionally introduces one managed `langgraph_external` node contract. The Rust scheduler leases exactly one node and owns admission, retry, pause/resume, concurrency, budgets, and terminal state. The adapter package performs exactly one `graph.invoke`, has no durable queue or product authority, and returns a content-free summary. App-owned invocation receipts and checkpoint metadata bind tenant, workspace, run, node, thread, request/idempotency hash, adapter/runtime versions, result, artifact, lease, and audit identity. SQLite uses an immediate transaction; PostgreSQL uses a transaction-scoped advisory lock so duplicate/concurrent claims resolve to one claimed, completed, busy, or blocked result.

Fixture mode is deterministic and network-free. Live mode is default-off, forbidden in CI, requires auth, an explicit confirmation, symbolic credentials retained by Rust, provider/model identity binding, pricing, per-call/run/daily caps, timeout, token cap, circuit breaker, kill switch, and provider audit. Python never receives a credential or raw provider response: Rust validates a typed provider result and passes only its hash, bounded usage, provenance, and typed decision. A result that may have followed a paid call but cannot be authoritatively persisted becomes `provider_outcome_unknown`, is blocked, and is not automatically retried. Rollback disables the runtime gate and reverts code; additive v24 rows remain inert and auditable unless restored from a verified pre-migration backup. CrewAI and Microsoft Agent Framework remain deferred.

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
| Target repository output | V2-3 plus Product Residual Seal 2 | Controlled git worktree, `acp/*` branch push, patch export, GitHub Draft PR | product approval requires `team:admin`; output requires `dispatch:execute`, exact persisted approval and explicit confirmation; env gates/kill, real verification, content hash, bounded text changes, exact HTTPS host/repository/token-reference controls, no direct `main` writes or merge authority |
| Workers | V2-4 merged | Bounded supervised workers | Dual env gate, atomic DB lease, per-worker heartbeat, concurrency cap, stale recovery audit, authenticated pause/resume/kill |
| Dashboard UX | Task-first workspace | Single output workflow with secondary operations/admin navigation | Task/run/workspace/verification/approval/PR visibility |

Do not create a second runtime kernel for V2. Extend the existing `node_executor`, `workflow_runs`, `scheduler`, `executor_pool`, `provider`, `cli`, `supervised_patch`, `LocalProductStore`, SDK, and dashboard surfaces.

Managed Product Golden Path compilation keeps the public task and plan redacted: `intake_json` exposes only the bounded objective preview and fingerprint, while an internal `_execution_objective_v1` member remains owned by `LocalProductStore` and is removed from every task row returned by SQLite/PostgreSQL. Admission commits that private payload and `product_task.admit` audit atomically in both backends. Compile verifies the exact payload against the intake fingerprint. After lease, the scheduler injects the full objective into the in-memory node input; it is not copied into persisted node JSON, terminal evidence, audit, scorecard, or replay records. New product graphs use `product_apply_binding.v2`, which hash-binds the resolved positive measured-token threshold and complete call/retry budget alongside task/workspace/source/objective/allowed-path/executor/output identity; v1 graph receipts remain readable for compatibility.

The admitted Codex CLI process is explicit non-interactive automation: `--ask-for-approval never` prevents a user-level instruction from pausing after the control plane has already authorized the bounded workspace apply, while `--sandbox workspace-write`, exact `--cd`, the app-owned canonical worktree, tool policy, and allowed paths remain mandatory. Its prompt states that execution authorization excludes artifact approval, output confirmation, branch push, and Draft PR creation. No full-access flag is available. CLI environment remains `PATH` plus the explicit runtime allowlist.

Managed product token evidence is `product_managed_usage.v1` on the existing workflow node. Each attempt records only measured current/cumulative token counts and attempt identity; raw output is excluded. The state is committed with the node result in both SQLite and PostgreSQL and must form a contiguous attempt lineage after restart. `product_apply_binding.v2` also binds `total_calls` and `max_retries`; an excess attempt is rejected before subprocess start. Failed retryable attempts count toward the task total, missing measured usage fails closed before retry, cumulative overage becomes the existing `budget_exhausted` task state, and budget failures are nonretryable. The installed Codex CLI `0.145.0` exposes no task-scoped token-cap option and its documented JSONL usage arrives on `turn.completed`; consequently its `total_tokens` remains an authoritative post-execution measured threshold, not a pre- or during-call cap. The exact Claude admission instead pre-reserves a conservative 792,000-token ceiling for at most three 200,000-context plus 64,000-output turns, permits one scheduler call and no retry, and applies a $2.16 client-estimate cap covering base input, five-minute and one-hour cache writes, cache reads, and output under the source-dated pricing snapshot. It still records measured owner token output after the call. Claude's dollar fields are client-side estimates, not billing receipts, so canonical cost stays unavailable. Effect/outcome-unknown errors retain their stronger outcome status.

### Product output authority and phased receipt

Product output is a two-authority transaction. `product_output_approval.v1` is written through the existing workflow approval owner only after exact task/run/node/workspace/source/artifact/changed-file/verification/output binding is validated; it grants no execution authority. A later output call must supply that approval, the expected-current task version, `dispatch:execute`, and `confirm_output=true`. Confirmation is checked before mutation. The deprecated combined route is only a compatibility composition that must pass both scopes.

Artifact-only and patch export complete only after the exact artifact row durably contains an idempotent `product_output_receipt.v1`. Draft PR output uses a progressive `product_output_operation.v1` in that same owner: branch push and PR creation have separate statuses and identities, and each mutation plus audit is one SQLite/PostgreSQL transaction. Each external phase has a durable actor/timestamp claim, monotonic operation version, 15-minute lease, and four-attempt whole-operation limit. The lease covers the full bounded multi-command branch-publish sequence (each git subprocess is capped at 30 seconds) and the two-request GitHub reconciliation/create sequence, rather than a single call. Retry after a confirmed push reuses the `acp/*` branch and attempts or reconciles only the missing Draft PR. Only a bound open Draft PR completes the operation; network unavailable, admission failure, known HTTP failure, outcome unknown, and exhausted attempts do not complete the task. The GitHub adapter is default-off, accepts only `https://api.github.com` and exact `ACP_GITHUB_REPOSITORY_ALLOWLIST` entries matching admitted `https://github.com/owner/repository` targets, reads a symbolic environment credential, redacts responses, and exposes no merge or default-branch write.

Product worktrees and exported patch files remain filesystem-owned when application records use PostgreSQL. SQLite defaults to database-adjacent `workspaces/` and `exports/` roots. PostgreSQL has no filesystem database parent and therefore fails closed unless `ACP_PRODUCT_WORKSPACE_ROOT` names an explicit absolute app-owned root; worktrees use direct children and patch exports use its `exports/` child. The same canonical containment checks apply after worktree creation, and a connection URL is never treated as a filesystem path.

### Product terminal evidence and process outcome

Schema v31 adds `product_task_terminal_evidence` under `LocalProductStore`; it is not a second evidence store. A successful product terminal transition builds `product_task_terminal_evidence.v2` from the exact persisted task, plan/run/node attempt, workspace/source, verification receipt set, supervised artifact, product approval, and output receipt or progressive operation used by that transition. SQLite and PostgreSQL commit the task CAS, transition audit, evidence audit, content-hash-bound evidence row, and owner references in one transaction. The `(task, terminal version, output-result hash)` identity is unique; exact retries return the committed row. Reads and the compatibility emission method are pure reads and append neither audit nor evidence. Missing or stale bindings, evidence-audit failure, or a nonterminal output prevents completion.

Replay is `linked` only when the replay owner binds an artifact to the run's recorder-owned dispatch; a run alone never implies eligibility. Native scorecards are linked only by exact run owner records. Executor type/class and measured usage come from the exact node result; fixture execution is labeled `fixture_deterministic` with usage/cost unavailable, while managed execution links token usage only when the executor owner reports it. Cost remains unavailable without exact usage plus provider/model/pricing authority.

`NodeExecutionOutput` carries `process_outcome.v1`. `CommandNodeExecutor` and the admitted managed CLI executors record the real OS exit code, termination signal when available, timeout, spawn failure, wait failure, or output-read failure without synthesizing exit code 1. Verification receipts use `product_verification_attempt.v2` and succeed only when execution is completed and the process outcome is `exited` with code zero. Executors without an OS-process owner report an explicit unavailable reason.

Product verification samples runtime and persisted authority immediately before and after every command. Automatic API compilation requires an attached, running scheduler and exposes only live executors its configured routing mode can consume; a temporary registration snapshot and fixed `noop`/`stub`/`fail` scheduler are not admission evidence. Product verification is not a general process sandbox: its separate mediation contract admits only the fixed non-writing `echo`/`cat`/`ls`/`head`/`tail`/`grep`/`wc`/`true`/`false`/`test` set, rejects absolute and parent-traversal arguments, clears the environment, rejects workspace symlinks, and binds the exact canonical worktree. Python, `tee`, `sed`, shells, package managers, and arbitrary test runners are not admitted verification commands. Each declared command is a deterministic API-owned `product_verify` managed run under the existing workflow-node lease and `ToolPolicyNodeExecutor` one-use receipt. Its binding includes the pre-command patch hash, so concurrent finalizers reuse one terminal result and a restart after a durable effect cannot recompute a changed workspace as a fresh valid baseline. Before the generic workflow owner persists a result, the product executor replaces command output and error text with hashes while retaining the authoritative process outcome; raw repository contents and stderr are not stored. A stale lease whose one-use receipt was consumed without a terminal result becomes product/workspace `outcome_unknown`; it is never retried as a fresh effect. Verification binds the expected task version and `verifying` state, completed product run, exact completed node attempt/lease/result, current supervised workspace record/canonical path, checked-out source revision, persisted total-elapsed budget, and the exact Git patch hash used by artifact output. Patch observation uses an isolated temporary Git index, includes tracked and untracked output (including tracked changes in ignored directories), and never changes the worktree's real index before a declared command. At the atomic artifact boundary, both backends re-read the current persisted task intake and reject every changed file outside an exact admitted path or its admitted subtree; the verification becomes untrustworthy, the workspace is quarantined, the task becomes `blocked`, and no such artifact can enter `awaiting_approval`. Fixture control scaffolding deletes itself before creating the declared repository change and is never product output. The effective command timeout is the minimum of its declared timeout and the remaining total-elapsed budget. Scheduler/global kill, scheduler/run pause, lost lease or node result, task supersession, workspace removal/replacement, source drift, elapsed-budget exhaustion, or patch mutation after command start produces an audited `authority_lost` receipt. The command result is marked stale and untrustworthy, workspace-originated loss is quarantined, and no artifact is committed. On success, the HTTP owner non-blockingly acquires the scheduler owner plus its worker-shared, storage-free control gate, samples both API flags and the environment pause/kill gates, and holds both guards through bounded patch preparation and the database commit. API controls and worker-observed environment pause/kill therefore have one serialized order relative to artifact insert, workspace update, exact product-task transition to `awaiting_approval`, and both audits in the SQLite immediate or PostgreSQL row-lock transaction; contention and audit failure roll the whole artifact transaction back. The direct store finalizer retains an explicitly labeled manual-operational compatibility path; HTTP finalization never falls back to it.

## Storage

`LocalProductStore` supports SQLite by default and PostgreSQL through the `pg` feature and `ACP_DATABASE_URL`.

- Current version: v31. v18 adds immutable, hash-bound `budget_evidence_artifacts`; v19 adds `budget_pause_decisions`; v20 adds immutable `offline_replay_artifacts`; v21 adds recorder-owned trace schema/hash bindings to `dispatch_history`; v22 adds Agent Runtime action receipts, configured-allowlist profiles, and one-use tool execution authorizations. v23 adds `durable_memory_versions`, `memory_retrieval_events`, `production_jobs`, `normalized_usage_observations`, `replay_producer_bindings`, and `operator_acknowledgements`. v24 adds scoped `external_runtime_checkpoints` and idempotent `external_runtime_invocations`; v25 adds provider embedding metadata and identity hashes to durable memory versions plus hash-bound `provider_embedding_operations` restart receipts; v26 adds the default-off bounded recursive-execution tree snapshot and node identity index; v27 adds the default-off Harness Evolution laboratory evidence foundation (`harness_evolution_active_identity`, `harness_evolution_proposals`, `harness_evolution_candidates`, `harness_evolution_receipts`) with exactly-once proposal/candidate receipts and immutable active-Harness/evaluator identity binding; v28 adds evaluation bundles, evaluator-owned sealed holdout hashes, Pareto archive entries, and evaluation receipts under equal-budget fixture evaluation; v29 adds PR_READY candidate bundles and finalizer receipts (no PR create/merge, no auto-merge, no improvement claim from fixtures alone); v30 adds the default-off Product Golden Path canonical root task table (`product_tasks`) with idempotent intake, tenant/workspace scope, expected-current versioning, and worktree-first binding; v31 adds canonical content-hash-bound product terminal evidence with exact artifact/approval/output/audit references. These mutations use the existing SQLite/PostgreSQL transaction owners. Normal rollback is a code revert that leaves additive evidence inert. Explicit destructive rollback operations require confirmation, stopped writers, and empty version-owned authority; v26 rollback is limited to an empty recursive surface and commits its audit and version-marker change atomically or not at all; v27/v28 rollbacks are limited to empty evolution evidence/evaluation surfaces; v30 rollback is limited to an empty `product_tasks` surface; v31 rollback is limited to an empty `product_task_terminal_evidence` surface. Old dispatch rows without v21 provenance remain readable but cannot establish trusted replay evidence. No second scheduler, provider/model authority, target-output authority, or cloud multi-tenant isolation claim is added.
- SQLite uses WAL and app-managed backup/restore. Verified restore uses SQLite's online backup API while holding the live app connection owner, so the open connection is replaced atomically at the database level rather than continuing on an unlinked pre-restore inode; checksum and integrity are checked before and after restore.
- PostgreSQL disables app-managed backup; operators use `pg_dump` or managed backup.
- PostgreSQL integration tests are gated behind `cargo test -p engine --features pg-tests`.

## Operator Decision Contracts

PE-3 uses additive `operator_decision_source.v1`, `operator_decision_item.v1`, and `operator_decision_queue.v1` Rust contracts in `engine/src/operator_decision/mod.rs`. They normalize bounded references to existing approval, recovery, rollback, budget, policy, workflow, scheduler, and benchmark evidence without becoming a new source of truth. Resolution is deterministic: severity, fixed source precedence, confidence, observation time, then lexical source ID. Exact source duplicates collapse; equal-ranked incompatible actions become an explicit conflict. Expired, stale, low-confidence, informational, resolved, and insufficient sources cannot produce a ready recommendation.

`LocalProductStore::operator_decision_queue` recomputes the queue through existing SQLite/PostgreSQL readers; it does not persist queue rows, emit audits, or mutate any source. Source reads and returned pagination are bounded, output is deterministically ordered and hash-bound, and an unreadable evidence owner fails the complete derivation closed instead of silently omitting decisions. Restart behavior is therefore the same deterministic recomputation over existing truth owners. Only a future allowlisted adapter may connect `ready` items to existing control owners, and it must preserve their confirmation, permission, audit, idempotency, compensation, and rollback gates. Contract and queue rollback is a code revert with no migration or data cleanup.

The read surface is additive `GET /api/v1/operator/decisions`, documented in OpenAPI and exposed by the existing Python/TypeScript SDKs and Dashboard navigation. It requires `dispatch:read`, returns the hash-bound derived queue plus explicit read-only boundary metadata, accepts bounded pagination/freshness parameters, and offers no mutation control. It may use a supplied timestamp for deterministic evidence review; otherwise it derives using server time. Route rollback is a code revert with no stored state.

PE3-ACTIONS remains a narrowly allowlisted adapter, not an execution authority. Read-only replay may use a deterministic caller time, but mutation compares that time with the store clock, rejects stale or future reads, re-derives the exact bound and current pages, and binds decision, conflict key, resource, action, source kind/ID/hash, pagination, and freshness before invoking a typed owner. Derived sources include bounded original evidence references; absent trustworthy hashes remain absent. Approval resolution is atomic inside the existing workflow owner for SQLite and PostgreSQL. Retry is exposed only for blocked runs with a ready node; terminal failed/completed/cancelled runs are not ready recommendations. Budget pause/recovery retains `dispatch:execute`, policy, audit, idempotency, and recovery gates. Inspect dispatches only to supported read owners and never mutates. Acknowledge persists an exact source hash without approval semantics. Adaptive-policy rollback requires `team:admin`, the exact snapshot, and current-state rebinding. Unsupported source/action pairs fail closed.

PE-3 is acceptance-sealed after the independent PE3-REPAIR-1 repair and PE3-CLOSE-1 audit. The repair corrected observation-time precedence to compare parsed instants, preserving deterministic tie-breakers and all existing authority boundaries. No queue persistence, second action owner, migration, or Dashboard mutation control was introduced.

## Trace-backed Policy Replay Contract

Historical record: PE4-CONTRACT-1 and PR #197 replaced the caller-asserted replay gate with `policy_replay_contract.v2` and `trace_replay_evidence.v1`. Those weaker semantics are superseded by the accepted `PE4-POST-CLOSE-REPAIR-1` v3/v2 boundary. Caller booleans, caller candidate definitions, self-computed hashes, caller coverage claims, and historical v1 artifacts are not current authorization evidence.

PE4-OFFLINE-1 adds deterministic comparable-cohort replay through `OfflineEvaluationEngine::replay_policies`. Explicit versioned current and candidate policy definitions are content-hash bound; observed facts remain separate from counterfactual estimates. Offline replay and shadow comparison remain derived, read-only evidence and cannot mutate live routing or policy.

PE4-READ-1 exposes bounded replay artifacts through the existing `LocalProductStore`, read-only HTTP/OpenAPI, encoded Python/TypeScript SDK readers, and the DynamicRegulator Dashboard. Historical v1 report rows remain readable but non-authorizing.

PE4-SHADOW-1 remains derived and non-mutating. PE4-CANARY-1 reuses `AdaptiveExperimentController`. PE4-PROMOTION-1 reuses `AdaptiveAutoPromotionController` and the existing `LocalProductStore` promotion, policy snapshot, permission, confirmation, audit, pause, compensation, and rollback owners. No offline or shadow result alone authorizes promotion.

Historical pre-repair closeout record: PR #203 merged as `008bc8c8879d6e7c9641fec57aa974f98af1c6b5` from exact head `2110676667dd1b57a36bc6f3744016599a02860a`; exact-head CI `29186113263` and post-merge `main` CI `29186372526` passed all seven required jobs. Its acceptance claim is superseded.

### PE4-POST-CLOSE-REPAIR-1 — accepted correction boundary

PE-4 is acceptance-sealed under PR #206. Trusted replay input is derived only from persisted `dispatch_history` through `RunTraceRecorder`, owner history ID, `dispatch_history_trace_owner.v1`, and an independently checked recorder hash. Raw imports and request deserialization cannot establish trusted eligibility. SQLite and PostgreSQL use aligned additive schema v21 nullable provenance columns; missing or mismatched owner binding fails closed.

The accepted contracts are `policy_replay_contract.v3`, `trace_replay_evidence.v2`, `offline_policy_replay.v2`, and `judge_calibration.v1`. Coverage is integer-based and inclusive at 90%. A central taxonomy distinguishes observation-local, cohort-fatal, and request-fatal failures. Execution terminality, execution outcome, evaluation completion/outcome, overall success, quality, and tool success are distinct. Paired judge calibration requires at least three samples, absolute signed bias at most 0.10, and MAE at most 0.15 when judge evidence is used.

Canonical bytes, raw sections, identifiers, arrays, report cardinalities, references, JSON depth, result size, numeric envelopes, and token sums are bounded. Canonical forms are precomputed before ordering; serialization and overflow fail closed. Caller scope is only a constraint; empirical task/domain/intent/objective, policy/cohort, candidate/member set, complexity, and metric support must come from accepted observations.

Final evidence:

- exact final PR head `80d9f9342956e1fd5931b59dcc426908d450b32b`;
- merge commit `f2a736a39e5de82d60da2a0b64d1c255d55ec326`;
- exact-final-head CI run `29190482093`, all seven jobs passed;
- post-merge `main` CI run `29190797214`, all seven jobs passed.

There is no pending PE-4 documentation-head or post-merge verification requirement. The weaker pre-repair semantics are superseded and PE-5 is now activated through `docs/NEXT_DECISION.md`.

## Release Provenance and Recovery Drill Boundaries

PE-5 extends the existing release workflow, package/container builders, dependency locks, installer/upgrader, verification, and atomic rollback owners. It must not create a second release pipeline or artifact truth source. Accepted releases must bind source, workflow, builder, target, dependencies, artifact digest, SBOM, attestation/signing identity, verification result, and rollback target. Production signing identity must be external and ephemeral; persistent private keys and signing credentials on self-hosted workers are forbidden. No PE-5 packet independently authorizes public release, tag, deployment, or publication.

PE-6 validates existing recovery owners using versioned allowlisted drills against disposable local/CI resources only. It may use temporary SQLite, ephemeral PostgreSQL, fake/stub providers, isolated worktrees, child processes, and non-publishing release bundles. It must not corrupt real databases, call real providers, damage registered repositories, publish releases, modify host installations, or create a second recovery authority. Every drill binds normal, failure, recovery, rollback, integrity, audit, timeout/abort, and cleanup invariants.

### PE-5 historical defects and repaired boundary

PRs #210-#211 remain historical implementation/review evidence, but their acceptance meaning is superseded where it relied on one `actions/attest` invocation with `sbom-path`, GitHub-API attestation lookup instead of exact distributed bundles, an unsigned self-consistent provenance file, placeholder lockfile components, a mutable `main` bootstrap, arbitrary rollback strings, incomplete archive bounds, or unverified rollback success claims. Historical `release_provenance.v1` fixtures remain readable only for compatibility tests. At the tool boundary, the legacy `verify` command and legacy attestation generator accept fixture mode/identity only, direct non-fixture verification returns `unsupported`, and legacy evidence cannot return production `verified`; `verify-release` v2 is the sole production release-verification authority.

`PE56-POST-SEAL-REPAIR-1` retains the existing workflow and installer owners and introduces the canonical custom predicate `release_provenance.v2`. Every platform archive has three distinct signed envelopes and files: `<archive>.slsa.bundle.json` for the action's default SLSA v1 predicate, `<archive>.spdx.bundle.json` for the canonical SPDX 2.3 document, and `<archive>.release-manifest.bundle.json` for `<archive>.release-manifest.json`. The three pinned `actions/attest` v4.1.1 calls use `contents: read`, `id-token: write`, `attestations: write`, and `artifact-metadata: write`. The installer and verifier bootstrap assets each receive their own default-SLSA bundle. There is no persistent signing key or self-hosted signing identity.

The signed custom predicate binds repository, full source commit, immutable tag ref, workflow/ref/run/job/builder, target and package kind, artifact name/media type/size/digest, canonical SBOM name/size/digest, lockfile hashes and deterministic inventory hash, build-input hashes, publication mode, exact bootstrap asset digests/source commit/predicate, and explicit rollback state. It intentionally excludes its own bundle digest to avoid a circular hash. The SLSA predicate is action-generated; the entire SPDX document and entire v2 manifest are separately signed predicate content. Unsigned metadata intermediates, checksum sidecars, bundle filenames, verification result files, release notes, and installer messages have no authority by themselves.

Production verification invokes `gh attestation verify --bundle` separately for each exact local bundle with its required predicate type, repository, signer workflow, GitHub OIDC issuer, tag ref, and `--source-digest`. It proves each statement has exactly the archive subject digest, then compares the verified SPDX and custom predicates to the canonical local files. Missing, duplicate, swapped, fixture, API-only, wrong-predicate, wrong-content, wrong-source, wrong-workflow, or wrong-repository evidence fails closed. Fixture bundles use a separate v2 fixture schema and can return only `verified_fixture`.

The SPDX generator parses `Cargo.lock`, `dashboard/bun.lock`, and `sdk/typescript/bun.lock` offline. It emits exact Cargo/npm package versions, purls, source lockfiles, integrity/checksum data where present, and deterministic dependency relationships; malformed, missing, oversized, duplicate, conflicting, or ambiguously resolved lock data fails. Package and container subjects have distinct namespaces and comments.

Production bootstrap is a download → exact local SLSA verification → execution sequence. It requires an exact version, the release source commit, and the local installer bundle; refuses stdin/pipes and `latest`; reverifies its own bytes first; downloads and verifies the separately attested verifier; then downloads the archive and all three package bundles. The verifier runs before archive extraction. Archive inspection bounds compressed bytes, member count, member and total uncompressed bytes, path length, and required files; it rejects duplicate normalized paths, path conflicts, links, devices, FIFOs, sparse/unsupported members, alternate separators, absolute/traversal paths, and unexpected top-level directories. Extraction manually writes ordinary files only after validation.

Rollback state is either `first_release` with no previous target or `previous_release` with exact tag, source commit, compatible target/package kind, artifact name, and digest. Publication accepts `first_release` only when the repository has no existing GitHub release; otherwise it verifies the selected previous release's own exact three bundles and compares those signed bindings before accepting it. Fresh installation stages binary, Dashboard, and optional example data before activation, then requires a bounded execution of the installed binary's `--help` and any explicit bounded operator health command before commit. Health failure runs the same transaction cleanup: no new binary or Dashboard remains, a pre-existing Dashboard is restored, and operator data is preserved. Upgrade preserves binary and Dashboard backups plus a versioned rollback-state file; rollback verifies the old binary digest, exact Dashboard-present/absent state, required restart hook, and health. It emits `UPGRADE_FAILED_ROLLBACK_SUCCEEDED` only after every check passes, otherwise `UPGRADE_FAILED_ROLLBACK_FAILED` and preserves backup evidence. Recovery is idempotent through `--recover`.

No real tag, release, publication, installation, host mutation, or production OIDC exercise was performed by this repair. Acceptance remains under repair until the complete reviewed head and post-merge `main` satisfy the required CI gates.

### PE-6 historical defects and repaired boundary

PRs #212-#213 remain historical evidence, but `fault_drill_result.v1` success is non-authorizing because the old harness synthesized recovery, rollback, integrity, audit, restart, and cleanup passes from a zero exit code, emitted canned observations, and recorded successful drills as one millisecond. The PostgreSQL registry also claimed an interrupted transaction while its test performed only a normal write.

The repaired active contracts are `fault_scenario.v2`, `fault_owner_evidence.v2`, `fault_drill_result.v2`, `fault_drill_report.v2`, and `fault_registry.v2`; v1 is not reinterpreted. The harness provides canonical scenario and disposable output paths to a fixed allowlisted owner command, measures actual command duration with a monotonic clock, keeps configured timeout separate, validates the emitted owner file, and hashes those exact canonical bytes. Exit zero without valid scenario/fault/source/owner/resource-bound evidence fails. Each category is derived only from scenario-specific named owner checks; absent checks remain `unsupported`, never passed. Owner and harness cleanup are independent required observations.

The active v2 claims match their injections: harness child timeout/cleanup; SQLite duplicate terminal replay refusal (not post-commit acknowledgement loss); backup checksum tamper/refusal; workflow command timeout, concurrent authority race, stale lease, and reopen; provider timeout/cancellation, bounded retry, cost pre-gate, redaction, and audit (not process kill); release artifact tamper plus interruption after Dashboard activation with verified rollback; and PostgreSQL failure after a config write but before audit/commit in a real transaction. PostgreSQL proves the partial config and audit rows are absent, retry creates exactly one config/audit authority, the same store remains usable, and cleanup removes the disposable rows. Only the exact GitHub Actions PostgreSQL service is supported; other environments report `unsupported`.

PE-6 still adds no runtime scheduler, storage, provider, release, audit, or recovery authority. All resources remain disposable and provider/release external actions remain disabled. Acceptance remains under repair until the complete reviewed head and post-merge `main` satisfy the required CI gates.

## Execution Modes

`ACP_EXECUTION_MODE` controls only the direct-dispatch surface:

| Mode | Behavior |
|---|---|
| `off` | Default noop behavior; no external calls |
| `provider` | Provider API only; requires provider gate/auth/cost controls |
| `cli` | Retired; startup fails so CLI cannot bypass workflow tool policy |
| `auto` | Retired on direct dispatch; startup points operators to scheduler `auto`/`pool` |

Workflow node execution is explicit through scheduler/tick paths. `ACP_SCHEDULER_EXECUTOR=auto` or `pool` is the sole hybrid routing entry: it may select an exact gated provider node or an explicitly enabled policy-wrapped CLI executor from the next ready node's persisted task contract and current pool state. It does not revive the old direct complexity router. A CLI route requires authenticated startup and the node workspace to equal both the stored and currently canonical app-owned supervised-patch workspace bound to the run; its managed invocation hash and v2 task/budget binding are recomputed immediately before the one-use execution claim. Missing, terminal, replaced, relative, escaping, unavailable, corrupt, unbound, over-call, or over-retry inputs fail before a model process. There is no `.` or engine-checkout fallback. Claude additionally revalidates exact binary identity immediately before launch, binds model and worst-case token/cost limits into node metadata, and requires task `total_calls=1`, `max_retries=0`, sufficient tokens, and concurrency one. Its child environment admits only bounded process variables and explicitly selected first-party credential variables; proxy/base-URL, model, cloud-provider, and TLS-routing overrides are discarded. `CommandNodeExecutor` rejects shell metacharacters, avoids `sh -c`, uses allowlisted binaries, validates supplied workspace cwd, clears inherited environment except `PATH`, caps output, enforces timeout kill, and emits structured results. The dashboard receives a startup capability snapshot with only enabled/detected booleans; it exposes no binary paths and grants no execution authority. Other CLI subprocess environments retain the existing `PATH` plus `ACP_CLI_ENV_ALLOWLIST` behavior, and output is redacted. Owner-reported Claude model identity and measured tokens are preserved; its dollar result remains an explicitly non-authoritative estimate and canonical cost stays unavailable without billing evidence. Provider workflow ticks require a ready trusted-local profile or the standalone legacy provider gate, plus provider configuration, scope, cost gates, audit, and retries. The live local comparison runner additionally requires a persistent `LocalProductStore` audit sink, positive input/output pricing, a pre-call worst-case token/cost reservation, shared run/daily caps, provider timeout, and `ACP_LOCAL_RUNNER_KILL_SWITCH`; missing evidence fails closed before a provider call.

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

The system is a deterministic workflow/control-plane runtime extended with bounded multi-agent semantics. AR-0 defines the contract baseline. AR-1 (agent identity, state, mailbox), AR-2 (agent step executor), AR-3 (bounded planning/child tasks/handoff), AR-4 (concurrent multi-agent scheduling), AR-5 (review and debate primitives), and AR-6 (operator evidence read-model) are implemented. AR-7 (bounded recursive task trees) is implemented as a default-off runtime extension under the same scheduler, executor, and storage owners (PR #239); Harness evolution and any evolution gate remain unavailable.

### Definition

`AgentRuntime` is the implemented bounded extension of workflow execution with durable agent identity, mailbox delivery, persistent run-scoped state, agent-authored planning, agent-to-agent delegation, cross-agent review, and concurrent step semantics. The AR-0 contract still defines its invariants; production routing extends existing modules and never creates a parallel runtime kernel.

### What AgentRuntime Is Not

- Not an unbounded autonomous multi-agent platform. AR-1 through AR-7 implement only the bounded semantics listed here, and the production integration connects them without adding a second runtime or authority owner.
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
| `engine/src/provider/` and `engine/src/cli/` | Gated decision/tool execution | Provider decisions use exact configured-model, cost, audit, and redaction gates; CLI tools execute only as policy-wrapped workflow nodes |
| `engine/src/http_server/` | Agent-readable endpoints, operator controls | Agent state/mailbox/status endpoints using existing auth scopes |
| `SDKs` and `dashboard/` | Operator visibility, guarded agent controls | Agent state inspection, mailbox counts, step traces, kill/pause controls |
| Existing audit, auth, cost, redaction, kill, rollback, target-output approval | Cross-cutting safety | Every AR phase must document which safety boundaries apply and how they remain enforced |

### Durable Entities (AR status)

| Entity | Owner (module) | AR phase | Purpose |
|---|---|---|---|
| `AgentState` | `storage/local_product_store/` | **AR-1 implemented** | Durable agent identity `(agent_id, run_id)`, role, capability profile, objective, status, bounded scratchpad summary, `metadata_json.memory_digest`, redaction filter reference, last activity timestamp |
| `AgentMessage` | `storage/local_product_store/` | **AR-1 implemented** | Mailbox row `(message_id, correlation_id, from, to, run_id, node_id, body_ref, status, created_at, read_at, ack_at)` with send/read/ack/reply transitions, audit events, and secret-shaped content rejection |
| `AgentStep` (node-level) | `node_executor.rs`, scheduler/executor pool, provider decision owner | **Production-integrated**: exact `agent_step` routing runs one observe/decide/act/persist action within existing lease/cap/kill boundaries, with strict typed provider decisions and atomic action receipts. |
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
8. **Rollback must be atomic.** Every AR phase must ship a documented, testable rollback procedure before merge. New authority schemas require an atomic downgrade operation or a forward-compatible code-revert path that leaves authoritative evidence inert; destructive downgrade must refuse active evidence. AR-1 uses the repository's earlier forward-only migration convention; see AR-1 rollback below. If a future phase adds no storage, rollback is a code revert plus a documented data-consistency step.

### Rollback Model

- **Code revert**: A PR that introduces AR code can be reverted by reverting the merge commit. No irreversible data writes outside app-owned storage.
- **Schema rollback**: New authority tables require an explicit backend-aligned downgrade contract. The preferred operational rollback is a code revert that leaves evidence inert. A destructive downgrade may remove only tables proven empty after writers stop; it must never delete live agent, mailbox, proposal, receipt, approval, audit, or recovery evidence merely to move a version marker.
- **Gate disable**: Every AR runtime addition must be behind an env gate (e.g., `ACP_ENABLE_AGENT_RUNTIME=0`) default-off, so operators can disable the new path without reverting code.
- **Data retention**: Agent state, mailbox, proposal, and receipt rows are app-owned evidence. A code rollback leaves them inert; destructive deletion requires a separately reviewed migration proving no live lease, approval, recovery, or audit dependency.
- **Kill switch**: A global kill switch for agent step execution must be present before any AR-2 merge, independent of per-agent bounds.

### AR Phase Status

**AR-1 (agent identity, state, mailbox) — implemented.** Durable `agent_state` and `agent_mailbox` tables with SQLite/PostgreSQL migration v14, `LocalProductStore` CRUD methods, send/read/ack/reply, correlation IDs, run/node links, secret redaction, size caps, and audit events. The original AR-1 slice added no executor or scheduler behavior; the production integration described below now owns those rows through the existing runtime.

**AR-1 rollback.** The original isolated-storage rollback is superseded now that production `agent_step` execution owns AR-1 rows. Do not manually drop `agent_mailbox`, `agent_state`, or remove the v14 migration marker. Use the integrated AR-2 rollback below: activate the kill switch, stop admission, drain or pause active leases, preserve a verified backup, and revert the integration while leaving authoritative rows inert. Any later destructive data removal requires a separately reviewed migration that proves there are no live receipts, messages, proposals, approvals, or recovery dependencies.

**AR-2 (agent step executor) — production-integrated.** `AgentStepExecutor` implements a one-step `observe → decide → act → persist` lifecycle. A confirmed typed plan selects exact task type `agent_step`; wildcard executors cannot claim it. Run creation initializes the bound `AgentState`, and the existing Rust scheduler/executor pool owns admission, lease, accounting, cooldown, retries, pause/resume, restart, and AR-4 global/per-run concurrency. Normal and dynamic scheduler paths share `ACP_SCHEDULER_MAX_RETRIES`; the default remains zero, values outside `0..=10` fail closed, and only failures already classified retryable consume that bounded retry budget. Lease startup validation requires the durable reclaim timeout to exceed the longest registered execution timeout plus a scheduling margin, and completion is attempt-CAS-bound so a stale worker cannot overwrite a recovered claim. `ACP_ENABLE_AGENT_RUNTIME=1` is required for execution and `ACP_AGENT_RUNTIME_KILL_SWITCH=1` overrides it. Capability profiles authorize only their mapped typed actions; `wait`/`complete` remain universally bounded terminal actions. The provider-backed decision source is registered only behind existing provider execution/readiness, authentication, pricing/cost, timeout/circuit-breaker, redaction, and audit boundaries. It performs one provider call, requires the node model to equal the provider default model before cost reservation, and accepts only a strict, size-bounded `agent_action.v1` union; unknown fields, malformed/oversized/unauthorized actions, and cross-run/agent/node identities fail closed. Deterministic decision fixtures remain test-only and CI performs no paid calls.

**Agent memory policy layer — implemented as AR maintenance, not a new AR phase.** Memory is embedded in existing `AgentState` and context assembly paths. `engine/src/agent_memory.rs` normalizes bounded memory digests from `AgentState.metadata_json["memory_digest"]` with scratchpad fallback, redacts secret-shaped values and private paths, filters source refs, estimates state bytes, and builds node-metadata-only memory context. `AgentStepExecutor` observe attaches bounded `memory_digest`, `memory_context`, and `memory_state_read_bytes` to `AgentStepContext`; `UpdateScratchpadSummary` keeps `scratchpad_summary` as the human-readable summary and synchronizes `metadata_json.memory_digest` through the existing `update_agent_state` metadata merge path. Workflow tick context injection may include bounded memory metadata for `agent_step` nodes through the existing `context_injection` object, preserving `injection_surface = "node_metadata_only"`. Operator evidence exposes only aggregate memory metadata (`memory_digest_present`, source-ref count, updated timestamp, estimated bytes), never raw memory, scratchpad, objective, prompts, outputs, transcripts, private paths, credentials, or metadata JSON. No DB table, migration, scheduler, mailbox, storage layer, external framework, provider call, target-output authority, or new runtime is introduced.

**AR-2 rollback.** Set `ACP_AGENT_RUNTIME_KILL_SWITCH=1` first and allow/force active leases to reach a safe terminal or paused state. The default path then reverts the integration merge and leaves forward-compatible migration v22 inert. If destructive local cleanup is explicitly desired after backup and integrity verification, stop every v22 writer, invoke the explicit-confirmation `LocalProductStore::rollback_v22_to_v21` operation while the v22 code is still installed, and only then revert the integration merge; the operation refuses non-empty authority tables and atomically audits the empty-table downgrade. Do not manually drop receipt, authorization, or configured-allowlist evidence. Reverting never authorizes an unknown `agent_step`: exact reserved routing fails explicitly when no executor is registered.

**AR-3 (bounded planning, child tasks, handoff) — implemented.** `engine/src/storage/local_product_store/schema.rs` v15 adds an `agent_proposals` table. `AgentMessageKind::ProposalUpdate`, `AgentAction::ProposeChildTask`, `AgentAction::RequestHandoff`, `AgentAction::AcceptHandoff`, `AgentAction::RejectHandoff`, and `AgentAction::CancelProposal` are implemented in the step executor with redaction, size caps, and safety gates. 12 dedicated tests pass. See merged history for the original packet.

**AR-4 (bounded concurrent multi-agent scheduling) — implemented.** Adds `agent_max_concurrent_global` (default 2) and `agent_max_concurrent_per_run` (default 1) to `SchedulerConfig` with env overrides. Cap enforcement is race-condition-free inside the lease transaction. Audit events cover the full lifecycle. The scheduler runtime passes caps on every tick. 8 new tests pass.

**AR-5 (bounded review and debate primitives) — implemented.** CAS-style debate round update, bounded review/debate primitives, and state-machine correctness fixes. Tests pass.

**AR-6 (operator evidence read-model) — implemented.** Adds a read-only operator evidence surface at `GET /api/v1/operator/evidence/:run_id`. It aggregates agent state, mailbox/proposal counts, blocked signals, and sanitized audit events. No new execution authority; AR-1 to AR-5 runtime semantics unchanged. Provider/CLI authority unchanged. Target-output approval unchanged. No autonomous merge/deploy/release authority added.

**AR-7 (bounded recursive task trees) — implemented, default-off (PR #239).** `engine/src/recursive_execution.rs` owns admission policy only; `engine/src/storage/local_product_store/recursive_execution.rs` owns v26 persistence (`recursive_execution_trees`, `recursive_execution_nodes`) with SQLite/PostgreSQL parity and integrity coverage. A model may only submit a bounded `ChildTaskProposal`; the control plane derives and persists root/parent identity, depth, objective fingerprint, equal-or-narrowing capability profile (non-heritable capabilities such as `handoff` are stripped so the recorded profile matches granted authority), per-node and remaining tree budgets, and ancestor fingerprints. Acceptance creates ordinary persisted workflow nodes leased by the existing scheduler — no recursive function loop, second runtime, queue, mailbox, or storage authority. Initial hard limits: depth 2, 3 accepted children per node, 12 tree nodes per root run, 3 globally leased recursive nodes, one retry per node; child scope/capabilities may only equal or narrow the parent and capability escalation fails closed. The thirteen packet failure reasons plus tree/usage/identity reasons remain distinguishable and auditable. `ACP_RECURSIVE_EXECUTION_ENABLED=1` is required for admission and `ACP_RECURSIVE_EXECUTION_KILL_SWITCH=1` overrides it; scheduler pause/kill reconciles tree state transactionally. Duplicate refusal uses the versioned deterministic lexical-equivalence contract (declared normalization and synonym vocabulary only; no provider-grade semantic equivalence is claimed). Late usage after lease loss is charged exactly once and an over-budget late receipt terminalizes the remaining tree with terminal workflow-node sync. Rollback: kill switch, pause admission, drain/block leases, preserve evidence, revert code; v26 rows stay inert by default and destructive downgrade refuses non-empty authority. No root-goal creation, scope broadening, external mutation authorization, Harness evolution, or recursive self-improvement is added.

**AR runtime integration storage and idempotency.** Migration v22 adds `agent_action_receipts`, `tool_allowlist_profiles`, and `tool_execution_authorizations` for both SQLite and PostgreSQL and includes them in integrity coverage. `(run_id, node_id)` is the action-receipt identity. Receipt claim, every mailbox/state/proposal/review/debate mutation, and audit append commit in one backend transaction; a retry with the same action hash returns the recorded result, while a changed agent or action hash fails closed. This covers process restart and concurrent claims without adding a queue or event store.

**Tool execution policy.** Every production Command/CLI construction used by scheduler fallback, executor-pool workers, explicit workflow tick, and supervised-patch verification is wrapped by `ToolPolicyNodeExecutor`. Once a profile is configured its allowlist is authoritative, including the empty set. Pre-hook block/error fails closed; enrichment is capped, hashed, injected only into node metadata, and audit-bound to ordered hook IDs. Approval-required capabilities create an existing workflow approval plus an exact run/node/tool/profile/action-hash authorization. Only an authorized `dispatch:execute` operator decision can approve it, and the authorization is atomically consumed before one subprocess invocation. Non-approval execution claims a synthetic consumed receipt in the same authority table before the effect. Rejection, replay, and duplicate claims fail closed. Any failure after an effect receipt is claimed is non-retryable and explicitly outcome-unknown; post hooks may change terminal policy status but cannot rewrite the inner executor's token, cost, or latency fields. Direct multi/CLI dispatch is retired so it cannot bypass this owner. Supervised-patch verification uses an atomic canonical workspace/operation/attempt binding and reuses or conflicts rather than launching duplicate effects after restart or concurrent requests. Its run is marked `api_owned_supervised_patch` in the existing pause owner field and excluded from both scheduler admission modes, preventing a background worker from stealing the node. The API handler rejects the invocation before creating a run or effect when a mounted scheduler lease is shorter than the bound command/CLI timeout plus scheduling margin. Corrupt persisted policy JSON fails closed instead of being reinterpreted as absent metadata. Unknown HTTP tick executors and unavailable scheduler executors fail explicitly; neither path falls back to a successful noop.

**Durable memory and retrieval.** The run-scoped digest remains the small working-memory layer. v23 durable memory is an app-owned version graph bound to tenant, workspace, optional agent/task, source ID/hash, confidence, freshness, expiry, conflict key, supersession, tombstone, actor, and record hash. Exact scope equality is required for every mutation and retrieval. A per-memory PostgreSQL advisory lock and SQLite immediate transaction serialize revisions; expected-version mismatch is the stable conflict outcome. Conflict sets are bounded to one atomically resolvable pair; a third incompatible fact fails before mutation until the existing pair is explicitly resolved. Retrieval reads only latest current non-stale, non-expired, non-tombstoned, non-conflicting records, ranks cosine similarity with deterministic tie-breakers, applies Top-K/token/byte bounds, and records metadata-only candidate/selection evidence. `local_hash_v1` is default-off, explicitly enabled, disabled in CI, and labeled `harness_derived`; fixture vectors are test-only. v25 adds provider embedding metadata and identity hashes to the same app-owned version table. The only production provider contract is the fixed OpenRouter embeddings endpoint with `nvidia/llama-nemotron-embed-vl-1b-v2:free` at 1,536 dimensions; it additionally requires the existing provider execution gate (legacy switch or ready trusted-local profile) and authenticated runtime mode. Each call rejects secret-shaped outbound input before claiming a request, then revalidates catalog canonical identity, text input, exact context contract, and every documented catalog charge dimension. Extra documented dimensions are accepted only when explicitly zero; unknown, malformed, missing required, or nonzero prices fail closed. Catalog price values and the harness-pinned selection effective date have explicit combined provenance, so the immutable contract does not change merely because a restart crosses UTC midnight. The read-only catalog request alone has bounded retry; each embedding POST is sent once through a reusable fixed-worker, bounded-queue transport executor that shuts down with its owner. The complete scope/source/model/pricing catalog contract, verified-zero reservation, `request_sent` audit, attempt count, and canonical receipt hash commit atomically under unique `(memory_id, version)` ownership. A completed receipt stores the numeric vector and metadata before the memory CAS, so an exact retry after restart revalidates and reuses it without another POST. A post-send result is definitive only when stable typed provider evidence proves a pre-effect refusal; 408, every 5xx, ambiguous or untyped status, transport loss, body-read, malformed-response, wrong-model, and invalid-vector results are outcome-unknown. Automatic replay remains forbidden. An authenticated typed reconciliation owner may authorize a bounded retry only for a definitive failure; an unknown result may be source/hash-acknowledged for audit but stays permanently blocked from another POST. Reconciliation idempotency revalidates the exact prior action/evidence audit binding. A separate confirmed re-embedding owner creates a new immutable memory version under the current contract. Full provider identity, dimension, context, pricing, and selection-date provenance used by durable rows and operation receipts live in an append-only supported registry so a reviewed contract rotation retains inspection and deterministic re-embedding of historical rows. Competing revisions fail before a second POST. Symbolic credentials, the existing audit owner, timeout, separate catalog/POST circuit breakers, and kill switch are reused. Stored vectors bind normalized-content/vector hashes and provider/model/pricing provenance to tenant/workspace/agent/task/run, memory/version, and source identity. Reads and full integrity checks revalidate those bindings and reject stale source/version, cross-scope, model, dimension, or incomplete metadata rather than degrading. SQLite v25 DDL plus marker commit in one immediate transaction and only empty partial tables are reconstructed; occupied partial tables fail closed. PostgreSQL v25 DDL and marker commit under a schema-scoped transaction advisory lock with the same rule. The provider-receipt Dashboard subview is tenant-scoped operator metadata: `team:admin` sees only the authenticated tenant, a `health:read`-only identity receives an empty receipt view, and all visible rows remain content/vector/credential-redacted. Lexical fallback is used only when the request opts into the labeled degradation. Scheduler context assembly injects immutable references and bounded content before the leased node executes and records read bytes and estimated token contribution.

**Pricing applicability refinement.** The fixed text-only embedding request records every modeled catalog dimension as `Applicable`, `NotApplicable`, or `Unknown`. `prompt` and `request` are applicable and must be present with explicit zero; `completion` is endpoint-schema-required and must be explicit zero even though vector requests cannot consume completion tokens. Image, search, reasoning, cache, and discount fields are non-applicable only for this fixed text request: a present value must be explicit zero, while an omitted value is persisted as explicit `null`, never normalized to a source price of zero. Unknown, unmodeled, malformed, nonzero, or missing applicable fields fail before POST. The applicability map and nullable source prices are bound in the operation receipt; older all-numeric v25 receipts remain readable for integrity and recovery but cannot authorize a new request without the current map.

**Budget intelligence production.** The existing scheduler/run owner invokes one fenced producer after terminal native scorecard persistence; an authenticated confirmed recompute endpoint invokes the same bounded owner. Normalization selects one authoritative source layer for each call across scorecard, provider audit, dispatch/CLI, adaptive, and workflow evidence, binds exact source kind/ID/hash, suppresses component workflow events when their aggregate scorecard exists, and preserves missing dimensions. Provider-reported, tokenizer-exact, harness-derived, estimated, and unavailable provenance remain distinct. `production_jobs` uses a deterministic key plus owner/token/expiry fencing so retry, concurrent scheduling, and restart cannot double-count or fork authority. Scheduler recovery stores one bounded ascending run cursor and rotating retry set in app config; it wraps only after reaching the end, so later runs progress while failed work remains retryable without a second task queue. Forecast and anomaly artifacts are immutable; unsupported or incomplete evidence yields explicit bounded outcomes. Only the existing typed operator decision and budget pause/recovery owners can mutate run state.

**Replay and promotion production.** A versioned replay production profile in app-owned config allows normal dispatch persistence to invoke a bounded provider-free producer. The producer re-reads recorder-owned dispatch traces, applies calibration/coverage/OOD/freshness gates, stores an immutable replay artifact, and persists its exact input/policy/dispatch binding. A scheduler-owned persistent ascending history cursor plus rotating bounded retry set recovers immediate-call failures and restart gaps without adding another scheduler or queue. The authenticated generate endpoint is deterministic recomputation, not a policy mutation. Promotion enters only through `promote_adaptive_fusion_policy_with_evidence_chain`, which validates the original binding, candidate/active identity, current-state rebinding, freshness, permission, confirmation, snapshot, and rollback target in the mutation transaction. Observation summaries no longer call a legacy mutation path. Typed inspect, acknowledgement, and rollback remain distinct operations; acknowledgement is stable across queue re-derivation, binds exact source kind/ID/hash, and is never approval. Replay alone is never authority.

**Unchanged non-authorities.** No hidden mailbox, second scheduler/runtime kernel, provider framework, automatic target-output merge/deploy/release authority, protected-branch write, or cloud multi-tenant isolation claim is introduced. Provider embeddings remain trusted-local, default-off, and scope-bound rather than a cloud multi-tenant service. Managed LangGraph and comparative benchmark live modes are explicitly gated; fixture completion is not external acceptance.

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
- Provider API output is available through the ready trusted-local profile or standalone legacy gates. Managed CLI execution is default-off: Codex requires its explicit authenticated app-workspace binding, while Claude additionally requires the separate exact binary/model/budget admission. The independent kill switch fails both closed.
- Hard process/container/VM sandbox isolation is not implemented and is not part of V2-1 unless separately approved.
- V2-3 controlled target output is merged. It creates no merge/deploy/apply authority and preserves the registered target working tree and `main`.
- GitHub PR creation is default-off and adds no merge authority.
- Bounded supervised workers are merged in V2-4 and Mission Control product output UX is merged in V2-5; unattended autonomous-agent loops remain out of scope.
- Bounded multi-agent runtime semantics are implemented through Agent Runtime AR-0 through AR-6 and are production-routed through exact `agent_step` plans, scheduler leases, typed provider decisions, atomic receipts, and v23 durable cross-run retrieval. v25 provider embeddings reuse the existing guarded provider boundary and remain default-off; the local vector strategy stays separately labeled `harness_derived` and lexical fallback stays explicit.
- The bounded LangGraph importer remains supported. The managed adapter is now a separate versioned package invoked once per Rust-leased node; its summary is automatically normalized into the existing scorecard store. A canonical native/LangGraph benchmark runs four memory strategies plus static-all/Top-K tool discovery through explicit runtime executables. The guarded OpenRouter live path accepts only the exact current public free Hy3 identity after fresh model/endpoint catalog validation; every modeled charge field must be present and zero, known irrelevant extras must also be explicit zero, and unknown or malformed dimensions fail closed. The request pins the catalog endpoint, disables fallbacks, requires requested parameters, applies zero prompt/completion/request/image price ceilings, and propagates the canonical output limit into every provider POST. All four memory strategies and both tool-discovery variants execute through the same bounded provider/audit/cost owner; tool prompt, output, latency, quality, and cost evidence is derived from those provider runs rather than fixture constants. The catalog evidence hash becomes the shared pricing identity for both runtimes and the redacted evidence summary is bound into the report. A guarded live run persists its twelve runtime/strategy scorecards through the existing app-owned artifact owner, so scenario matrices and Dashboard evidence use the same source-bound records; fixture evidence is deterministic and does not write product state. A provider-backed report is valid only when the guarded operator command records nonzero app-owned audit evidence and source hashes.
- Cloud SaaS, hosted/cloud deployment, multi-tenant service, and direct release/tag/deploy/apply controls are not implemented. PE-5 adds release evidence and verification, not app-runtime release authority.
- PE-5's earlier acceptance semantics are superseded by the implemented `PE56-POST-SEAL-REPAIR-1` three-bundle, v2 manifest, immutable bootstrap, real-SBOM, explicit rollback, bounded archive, and verified restoration contracts; acceptance remains pending exact-head and post-merge CI. No production identity or public release is authorized for the repair.
- PE-6's earlier exit-code-derived evidence and claims stronger than injected faults are superseded by owner-emitted v2 evidence and claim-aligned injections; acceptance remains pending the same CI evidence. No runtime/release authority is added.
- Some routing, quality, and orchestration modules remain partially active rather than unified under one policy layer.

The Adaptive Fusion Routing track extends `model_selector`, `feedback`, `provider`, storage, and existing HTTP/workflow/executor boundaries without creating a parallel routing, policy, workflow, or storage kernel. AF-0 through AF-6 add planning, endpoint metadata, offline evaluation, authenticated bounded execution, contextual policy, panel fusion, safe observations, controlled experiments, evidence-driven promotion, and guarded completions. Legacy independent gates remain supported.

IAE-1 composes those gates behind `ACP_TRUSTED_LOCAL_PROFILE=1`. IAE-2 adds bounded background advancement through the existing scheduler. Existing call/token/time/concurrency ceilings, provider/model identity, redacted outputs, audit, observations, snapshots/rollback, circuit breakers, pause, and kill controls remain authoritative. Target-repository output remains separately gated and never writes registered `main`.

IAE-3 does not add another control kernel. Dashboard authority derives from existing trusted-local, adaptive policy, cost, observation, scheduler, audit, and rollback owners. Raw model content, credentials, repository content, and private paths remain excluded.

Full Agent Autonomy Mode permits boundary expansion when the change has a documented plan, threat-model update where relevant, focused tests, observable evidence, CI review, compatibility, and rollback.

## Event-Driven Agent Orchestrator

The GitHub Actions orchestrator is a separate repository-maintenance control plane, not an engine runtime replacement. It is disabled by default and is governed by exactly one open control Issue with identity label `agent-control`, title `[agent-control] Orchestrator controls`, and marker `<!-- agent-orchestrator-control:v1 -->`. `agent-orchestrator-enabled` permits work only when `agent-emergency-stop` is absent; `agent-auto-merge-enabled` additionally permits merge. Missing, duplicate, malformed, closed, or unreadable control state fails closed. The only mutation permitted for an already-active failed workflow while stopped is the state owner's idempotent release of its exact active-capacity label into a non-running blocked state; review and repair cleanup are exact-head bound and cannot dispatch or authorize work. CI terminal and no-op outcomes persist exact issue/PR/head/run bindings through the state owner, record terminal-resolution intent before compensation, require complete production run identity, revalidate capacity immediately before the terminal label transition, and leave no authorized follow-up dispatch. The dispatch controller and all state-mutating orchestrator workflows share one repository-wide concurrency group; an emergency-stop controller command uses cancel-in-progress on that same group so the stop preempts a queued or running mutation lane.

Vader runs short-lived Codex CLI processes using its cached interactive login. Codex gets an isolated worktree and no workflow GitHub or push credential. It must leave the recorded worktree HEAD unchanged, stage only local changes, and return an untrusted binary `agent.patch` plus schema-versioned `agent-result.json`. A task Issue must declare `<!-- agent-orchestrator-scope:v1 {"allowed_paths":[...]} -->`; the GitHub-hosted finalizer independently validates that scope together with the manifest/bindings/checksum/size/path list, rejects forbidden paths or a moved remote head, applies the patch to a clean exact checkout, recomputes changed paths, rechecks live controls, then owns the commit, branch push, PR update, state write, and exact-head CI dispatch. Pre-commit validation is deliberately structural and bounded: artifact schema and digest checks, exact identity and scope binding, staged-path recomputation, and `git diff --cached --check`. Behavioral acceptance is supplied only by the canonical exact-head CI acquired after push; the finalizer does not claim that arbitrary task-specific tests ran before commit.

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
