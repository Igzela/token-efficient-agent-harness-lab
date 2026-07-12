# Next Decision

## Current Direction

The dispatch kernel, V2, Adaptive Fusion AF-0 through AF-7, Agent Runtime AR-0 through AR-6, Trusted Local Autonomous Execution IAE-0 through IAE-3, scorecard integrity hardening, the importer-first external benchmark path, and PE-1 Token Efficiency Regression Lab are complete.

The active direction is the Post-LGB Product Evolution plan. PE-1, PE-2, and PE-3 are complete and acceptance-sealed. PE4-CONTRACT-REPAIR-1, PE4-OFFLINE-1, PE4-READ-1, and PE4-SHADOW-1 are merged; PE4-CANARY-1 is active, while promotion is blocked on the canary merge. PE4-CLOSE-1, PE-5, and PE-6 remain unstarted. This is not AR-7, another LGB ladder, or a second control plane.

`docs/NEXT_DECISION.md` is the single forward-plan artifact. Historical detail remains in `docs/ARCHITECTURE_BOOK.md`, archived plans, merged PRs, and repository history.

## Full Agent Autonomy Mode

Full Agent Autonomy Mode is active for repo-scoped, testable, observable, CI-gated, and rollbackable planning and execution.

### Autonomously maintain and evolve

The coding agent may inspect, plan, implement, test, review, open PRs, repair CI, update active docs, merge eligible work, and continue across packets after refreshing `main`.

The agent may also resolve bounded architecture, authority, schema, migration, security, release, and recovery decisions when current code, merged history, tests, and active documents provide enough evidence for a smallest compatible and rollbackable design. Material decisions must be recorded in an existing authoritative document and verified through a coherent PR before dependent behavior relies on them.

The packet sequence remains the default execution structure. It protects scope, prerequisites, acceptance, compatibility, and rollback without requiring a separate external planner for every implementation detail.

## Model Selection

Model and reasoning-effort selection are user/tool settings. This repository does not require, forbid, or validate a model tier. Model choice does not weaken review, testing, CI, audit, compatibility, or rollback requirements.

## Execution-Ready Packet Protocol

Execute packets marked `READY_FOR_EXECUTION` whose prerequisites are complete. Prefer the earliest packet in the normative sequence unless an explicit independent lane is activated or a prerequisite defect must be repaired first.

Packet states:

- `READY_FOR_EXECUTION` — contract is sufficiently complete and prerequisites are satisfied.
- `BLOCKED_PREREQUISITE` — contract is defined, but an earlier packet must complete first.
- `DECISION_REQUIRED` — implementation depends on a material decision; the agent may resolve it from repository evidence or report the exact unresolved requirement.
- `IN_PROGRESS` — one active branch or PR owns the packet.
- `COMPLETE` — acceptance evidence is merged and active docs are updated.

Every packet inherits these requirements:

| Field | Required contract |
|---|---|
| Goal | One observable result, not a broad stage aspiration |
| Prerequisites | Exact earlier packets or existing contracts that must be complete |
| Owning paths | Existing source/test/document owners to extend |
| Allowed changes | Minimum coherent implementation surfaces |
| Forbidden changes | No parallel runtime, scheduler, store, policy authority, mailbox, artifact truth source, or Dashboard state model without a documented replacement decision |
| Contract | Versioned inputs, outputs, reason codes, bounds, permissions, and failure states |
| Verification | Focused tests plus applicable full repository validation |
| Compatibility | SQLite/PostgreSQL, API/SDK, existing rows, and old callers remain compatible when applicable |
| Rollback | `git revert` plus any bounded cleanup procedure |
| Completion evidence | PR, commit, CI run, test evidence, compatibility, residual risk, and next packet status |
| Stop triggers | Irreversible external action, unavailable required authority/credentials, unresolved material contradiction, or missing recovery path |

Stage prose is context. Packets are the default implementation units, but the agent may update a packet or add the smallest missing contract in the same authoritative file when evidence supports the change.

## Hard Stops

Stop with evidence rather than improvising when any of these applies:

- a real secret would enter version control;
- test or CI evidence would be falsified or a known failure hidden;
- a required rollback or recovery path would be removed without a tested replacement;
- an irreversible external operation lacks explicit authority and tested recovery;
- required human approval, credentials, or external access are unavailable;
- another agent owns conflicting in-progress work that cannot be safely reconciled;
- materially contradictory requirements cannot be resolved from current code, history, tests, and authoritative documents;
- required CI is failed, queued, in progress, or unexpectedly skipped.

A missing bounded design detail, a stale packet, or an initial failed repair is not itself a hard stop. Audit the repository, update the contract, repair the root cause, and continue when the result remains testable, observable, compatible, and rollbackable.

## Post-LGB Product Evolution Plan

Normative order is PE-1, PE-2, PE-3, PE-4, PE-5, and PE-6. Do not start PE-3 before PE-2 closeout.

| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-1 | P0 | Token Efficiency Regression Lab | Complete and acceptance-sealed |
| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | Complete and acceptance-sealed |
| PE-3 | P1 | Operator Decision Center | Complete and independently acceptance-sealed |
| PE-4 | P1/P2 | Trace-backed Policy Replay | Contract, offline replay, READ, and SHADOW merged; PE4-CANARY-1 active; promotion blocked on canary merge; PE4-CLOSE-1 remains unstarted |
| PE-5 | P1.5 | Release Provenance | Packetized; inactive unless explicitly activated |
| PE-6 | P2 | Fault Injection and Recovery Drills | Packetized; blocked on explicit recovery invariants |

## PE-1 — Token Efficiency Regression Lab

Implemented and accepted:

- canonical `token_efficiency_regression_registry.v1`;
- deterministic single-scenario and registry-wide batch reports;
- fixed bounded evidence for LangGraph, native deterministic, and local stub scenarios;
- SQLite/PostgreSQL `LocalProductStore` persistence and idempotent local import;
- deterministic bounded history/trend read model;
- read-only HTTP list/detail/trend endpoints and Python/TypeScript SDK readers;
- current Dashboard history/trend UX with all explicit incomplete and failure outcomes;
- report-only behavior with no CI blocking, provider call, routing change, policy mutation, pause authority, or target-repository write.

### Packet PE1-UI-1 — Dashboard history and trend UX

**State:** `COMPLETE`

**Evidence:** PR #177; CI run 29137424748; all seven jobs green. Existing Benchmarks route was extended additively and no API/storage/authority contract changed.

### Packet PE1-CLOSE-1 — PE-1 acceptance seal

**State:** `COMPLETE`

**Acceptance:** Deterministic recomputation; tamper, threshold, quality-failure, missing-baseline, missing-best-known, incomparable, repeat-import, cross-version, trend, API/SDK, Dashboard, SQLite, and PostgreSQL evidence are verified. Full CI is green.

## PE-2 — Budget Intelligence and Anomaly Auto-Pause

Stage invariants:

- forecasts and anomalies are derived evidence, not business facts;
- every result records schema version, evidence window, coverage, confidence, reason codes, and bounded evidence references;
- sparse, stale, contradictory, incomplete, or incomparable evidence returns an explicit bounded outcome such as `insufficient_evidence`;
- observed values and estimates remain separate;
- automatic pause is allowed only through existing pause and audit authority when explicitly policy-enabled and high-confidence;
- automatic pause must be idempotent, observable, reversible, and fail closed;
- no automatic termination, silent budget mutation, provider/model substitution, reservation rewrite, or opaque scoring.

### Packet PE2-CONTRACT-1 — Budget intelligence evidence contract

**State:** `COMPLETE`

**Prerequisite:** PE1-CLOSE-1 complete.

**Goal:** Define and validate versioned bounded contracts for forecasts, anomaly findings, confidence, coverage, reason codes, evidence references, and `insufficient_evidence` outcomes; report-only only.

**Owning paths:** `engine/src/budget_manager.rs`; existing provider audit/cost evidence; scheduler/workflow evidence; `LocalProductStore` validation owners; focused Rust tests; architecture/module docs when durable ownership changes.

**Allowed changes:** Contract structs, enums, validators, canonical serialization/hash where existing evidence uses hashes, and deterministic fixtures/tests. Reuse existing evidence owners.

**Forbidden changes:** No pause call, policy mutation, new provider invocation, new pricing source, persistence migration, API/SDK/Dashboard surface, or business-fact claim.

**Contract:**

- forecast schema: `budget_forecast_evidence.v1`;
- anomaly schema: `budget_anomaly_finding.v1`;
- dimensions are bounded identifiers for run, workspace, provider, and model when present in existing evidence;
- evidence window has explicit inclusive start, exclusive end, generated-at time, sample count, and freshness;
- coverage records observed/required dimensions, pricing completeness, duplicate handling, and missing fields;
- confidence is an explicit enum plus bounded numeric score and deterministic reasons;
- outcome is one of `supported`, `insufficient_evidence`, or `invalid_evidence` at contract level;
- reason codes are stable bounded strings, never free-form authority;
- evidence references contain only existing bounded IDs/hashes/metadata;
- malformed, oversized, tampered, future-window, inverted-window, unknown-model, and incomplete-pricing inputs fail closed or return explicit insufficiency as specified by the validator.

**Verification:** Focused Rust contract/serde/validator tests; malformed and tamper tests; sparse, incomplete-pricing, unknown-model, boundary-time, canonical ordering/hash, and SQLite/PostgreSQL-compatible JSON representation tests; formatting, clippy, full stack, handoff, and CI.

**Compatibility:** Additive Rust contracts only; no persisted row, API, SDK, Dashboard, reservation, pricing, or runtime behavior changes.

**Rollback:** Revert the packet PR; no data cleanup.

**Stop triggers:** Existing posted cost/audit evidence cannot identify the required dimensions without a new source-of-truth decision; canonical time semantics conflict across owners; a contract would imply pause or budget authority.

### Packet PE2-FORECAST-1 — Deterministic budget forecasts

**State:** `COMPLETE`

**Prerequisite:** PE2-CONTRACT-1 complete.

**Goal:** Produce read-only expected tokens/spend and exhaustion-time forecasts by run, workspace, provider, and model from existing posted evidence.

**Allowed changes:** Deterministic forecast computation and bounded aggregation over the versioned contract and existing evidence.

**Forbidden changes:** No external model call, learned/opaque model, budget mutation, reservation change, pricing invention, persistence/API/Dashboard work, or pause.

**Contract:** Separate observed values from estimates; include sample count, window, coverage, confidence, pricing completeness, assumptions, and reason codes. Refuse a forecast when evidence is stale, sparse, contradictory, duplicated beyond deterministic reconciliation, or unpriced.

**Acceptance:** Sparse, zero-usage, bursty, mixed-model, incomplete-pricing, boundary-time, deterministic ordering, duplicated/out-of-order evidence, and concurrency-safe read tests.

### Packet PE2-ANOMALY-1 — Explainable anomaly detector

**State:** `COMPLETE`

**Prerequisite:** PE2-FORECAST-1 complete.

**Goal:** Detect bounded cost, token, retry, latency, context-growth, and model-mix anomalies with explicit thresholds, confidence, coverage, evidence, and reason codes.

**Allowed changes:** Deterministic/statistical rules over the versioned contract and existing evidence.

**Forbidden changes:** No hidden adaptive threshold, provider/model substitution, pause, termination, budget/policy mutation, persistence/API/Dashboard work, or opaque score.

**Acceptance:** Normal, spike, gradual drift, mixed workloads, sparse history, false-positive boundaries, duplicated evidence, out-of-order evidence, deterministic recomputation, exact coverage metadata, invalid-evidence preservation, and `insufficient_evidence` tests.

### Packet PE2-READ-1 — Persistence, API, SDK, and Dashboard read surfaces

**State:** `COMPLETE`

**Prerequisite:** PE2-ANOMALY-1 complete.

**Goal:** Expose bounded forecasts and anomaly evidence through existing store/API/SDK/Dashboard owners.

**Allowed changes:** Additive `LocalProductStore` schema only if persistence is required by the accepted contract; read-only HTTP/OpenAPI; Python/TypeScript readers; current Dashboard components; SQLite/PostgreSQL migrations/tests.

**Forbidden changes:** No pause action, policy mutation, provider call, business-fact conversion, or second state model.

**Acceptance:** Idempotent persistence if used, bounded pagination, permissions, OpenAPI/router parity, encoded SDK paths, empty/error/sparse UI states, evidence links, migration/backward compatibility, and full stack verification.

**Stop triggers:** Persistence semantics were not fixed by PE2-CONTRACT-1 or require rewriting existing budget history. The agent may define the smallest additive persistence contract in this file or `docs/ARCHITECTURE_BOOK.md` before implementation when current store conventions make the answer clear.

### Packet PE2-PAUSE-1 — Policy-gated high-confidence auto-pause

**State:** `COMPLETE`

**Prerequisite:** PE2-READ-1 complete.

**Goal:** Invoke the existing pause mechanism only for policy-enabled, high-confidence supported findings, with complete audit and idempotent recovery behavior.

**Owning paths:** Existing scheduler/workflow pause controls, policy gates, audit, operator evidence, API controls, and focused integration tests.

**Allowed changes:** A narrow decision adapter from validated anomaly evidence to the existing pause path; explicit default-off enablement; audit; resume/override evidence.

**Forbidden changes:** No auto-kill, silent budget edit, provider/model substitution, new pause state machine, default enablement, implicit resume, or resume without operator evidence.

**Contract:** Fail closed on missing/disabled policy, incomplete pricing, low confidence, stale evidence, unsupported anomaly, missing coverage, audit failure, concurrent duplicate triggers, or unavailable pause owner. Repeated triggers are idempotent. Resume and override preserve cause and evidence.

**Acceptance:** False positives, concurrency, duplicate trigger, audit failure, pause failure compensation, disabled policy, sparse data, incomplete pricing, resume, override, permission, restart, and rollback tests.

**Stop triggers:** Existing pause semantics cannot guarantee idempotency/compensation, policy ownership is ambiguous, or audit cannot precede/atomically bind the authority decision. The agent may first implement a separate evidence-backed contract or compatibility repair PR; it must not silently create a second pause authority.

### Packet PE2-CLOSE-1 — PE-2 acceptance seal

**State:** `COMPLETE`

**Prerequisite:** PE2-PAUSE-1 complete.

**Goal:** Audit the entire PE-2 chain, repair bounded defects, mark PE-2 complete, and activate PE3-CONTRACT-1 without beginning PE-3.

**Acceptance:** Forecast/anomaly evidence is explainable, deterministic, versioned, bounded, and explicitly insufficient when unsupported; read surfaces are compatible; auto-pause is default-off, explicitly enabled, high-confidence, audited, idempotent, fail-closed, reversible, and recoverable; full CI and SQLite/PostgreSQL checks are green.

## PE-3 — Operator Decision Center

PE-3 is complete and acceptance-sealed. PE3-REPAIR-1 corrected observation-time ordering by comparing parsed instants, and PE3-CLOSE-1 independently re-audited the contract, derived queue, read surfaces, action adapters, existing owners, compatibility, and rollback boundaries.

### Packet PE3-CONTRACT-1 — Decision item and source contract

**State:** `COMPLETE`

**Prerequisite:** PE2-CLOSE-1 complete.

**Goal:** Define bounded versioned action-item/source contracts and deterministic source precedence over existing approvals, workflow/scheduler, budget, benchmark, policy, rollback, and recovery evidence. No queue persistence, action execution, new authority, or Dashboard work is authorized by this packet.

**Contract:** `operator_decision_source.v1` and `operator_decision_item.v1` are derived-evidence contracts. Sources carry bounded source/resource/conflict IDs, source state, requested action, severity, confidence, observation/expiry times, reason codes, evidence references, and a canonical hash. Items carry an explicit `ready`, `conflict`, `expired`, `insufficient_evidence`, or `resolved` outcome. Only `ready` may contain a recommended action.

**Deterministic resolution:** Critical before warning before info; then source precedence `approval > recovery > rollback > budget > policy > workflow > scheduler > benchmark`; then confidence, newest observation, and lexical source ID. Exact source duplicates collapse. Equal severity, precedence, and confidence with incompatible actions fails closed as `conflict`. Expired, stale, low-confidence, informational, resolved, and insufficient sources never become executable recommendations.

**Compatibility and authority:** Additive Rust contracts and pure resolution semantics only. No persisted rows, API/SDK/Dashboard changes, source mutation, action dispatch, approval decision, pause/resume/retry/rollback call, provider call, or target write.

**Rollback:** Revert the packet PR; no data migration or cleanup.

### Packet PE3-QUEUE-1 — Deterministic derived decision queue

**State:** `COMPLETE`

**Prerequisite:** PE3-CONTRACT-1 complete.

**Goal:** Adapt existing approval, workflow, scheduler, budget, benchmark, policy, rollback, and recovery evidence into the contract and derive one bounded, deterministic, mutation-free queue.

**Owning paths:** Existing `LocalProductStore` readers, operator-evidence handler, and `operator_decision.rs`. The queue is recomputed from existing truth owners; it is not a new persisted source of truth.

**Acceptance:** Empty, duplicate, conflict, expiry, stale, sparse, cross-run, source failure, precedence, deterministic ordering, bounded pagination, restart, SQLite, and PostgreSQL evidence tests. No action execution or Dashboard work.

**Implementation:** `operator_decision_queue.v1` is recomputed from existing LocalProductStore readers for approvals, workflow state, scheduler heartbeat, budget anomalies, benchmark reports, policy proposals, and rollback/recovery audit evidence. It is bounded to 100 source rows per owner and 100 returned items, hash-bound, deterministically ordered, and fail-closed when an owner cannot be read. Queue reads create no rows or audit events. SQLite tests cover empty, deterministic, read-only, cross-run, pagination, restart, and source-failure behavior; the PostgreSQL integration test covers equivalent requested-approval derivation.

**Compatibility and rollback:** No table, migration, HTTP, SDK, Dashboard, action, or authority change. SQLite and PostgreSQL use their existing readers. Revert the packet PR; no cleanup is required because the queue is never persisted.

### Packet PE3-READ-1 — API, SDK, and Dashboard decision center

**State:** `COMPLETE`

**Prerequisite:** PE3-QUEUE-1 complete.

**Goal:** Expose the derived queue through bounded `dispatch:read` HTTP/OpenAPI, Python SDK, TypeScript SDK, and the existing Dashboard navigation/state owner with explicit empty, insufficient, conflict, expired, resolved, and error states.

**Acceptance:** Encoded paths, pagination, permission, compatibility, redaction, evidence links, deterministic UI ordering, and no hidden mutation controls.

**Implementation:** `GET /api/v1/operator/decisions` exposes the derived `operator_decision_queue.v1` under existing `dispatch:read`. It validates bounded freshness and uses a caller-supplied timestamp when deterministic replay is needed. The route, OpenAPI, Python SDK, TypeScript SDK, and Dashboard Decision Center share the same read-only envelope. The Dashboard renders ready, conflict, expired, insufficient-evidence, and resolved items with bounded source references but no execution controls.

**Compatibility and rollback:** Additive route and SDK methods only; no migration, queue persistence, permission, audit, or action-owner change. Revert the packet PR to remove the route and UI; existing rows and clients remain unchanged.

### Packet PE3-ACTIONS-1 — Existing-control action adapters

**State:** `COMPLETE`

**Prerequisite:** PE3-READ-1 complete.

**Goal:** Map only explicitly allowlisted ready decisions to existing approval, pause, resume, retry, rollback, acknowledge, and inspect owners. Preserve each owner's permission, confirmation, audit, idempotency, compensation, restart, and rollback gates.

**Forbidden:** No generic action executor, new authority table, implicit confirmation, cross-resource action, automatic execution, or bypass of existing control endpoints.

**Contract decision:** `POST /api/v1/operator/decisions/{decision_id}/actions` is an allowlisted adapter, not an execution authority. Every request must carry explicit confirmation, the exact derived queue hash, generation timestamp, freshness bound, limit, and offset from its read response. The adapter recomputes that exact queue page, rejects hash/source/ready/action mismatch, and invokes an existing owner only. `approve`/`reject` require an approval source and record a new decision through the existing approval owner; `resume` and `retry` require a workflow source; `pause` requires a budget anomaly source, `dispatch:execute`, and the existing enabled budget auto-pause policy. `rollback`, `inspect`, and `acknowledge` fail closed until a compatible existing owner is explicitly available. The Python and TypeScript SDKs expose this explicit request shape; no Dashboard action control is added by this packet.

**Rollback:** Revert the adapter route and module. No migration or new stored state exists; existing owner audit and compensation records remain authoritative.

### Packet PE3-REPAIR-1 — Independent merged-chain repair

**State:** `COMPLETE`

**Prerequisite:** PE3-ACTIONS-1 complete.

**Goal:** Repair independently demonstrated PE-3 defects without creating a generic action executor or a second approval, pause, workflow, scheduler, audit, or rollback authority.

**Contract:** Read-only deterministic replay may retain a caller-supplied time. Mutation validates that time against the store clock, rejects stale/future reads, re-derives the exact bound page and exact current page, and binds decision ID, conflict key, resource, action, source kind, source ID, source hash, page, and freshness before owner invocation. Derived sources preserve bounded original evidence IDs and trustworthy hashes without fabricating absent hashes. Retry is ready only for blocked runs with a ready node. Approval resolution is atomic in the existing workflow owner across SQLite/PostgreSQL. Unsupported rollback, inspect, and acknowledge remain explicit fail-closed actions.

**Acceptance:** Focused freshness, tamper, source-change/resolution, page/order, hash/decision replay, cross-kind identity, approve/reject, retry terminal/no-ready/repeat/concurrency, resume compensation, permission, audit, restart, SQLite/PostgreSQL, unsupported-action, and observation-instant ordering tests; full exact-head CI; no temporary workflow or repair file in the final diff.

**Completion evidence:** PR #195 merged as `8efe09b5fd2346b7e12ff3fc7cd897d6177c7eae` from exact head `fc547fd5c42d1ace86c58fd6a291aadeaad60272`; exact-head CI run `29180711721` passed all seven required jobs.

**Rollback:** Revert the repair PR. No migration or queue cleanup; existing owner audit records remain authoritative.

### Packet PE3-CLOSE-1 — PE-3 acceptance seal

**State:** `COMPLETE`

**Prerequisite:** PE3-REPAIR-1 complete.

**Goal:** Independently audit contracts, source adapters, derived queue, API/OpenAPI, SDKs, Dashboard, permitted actions, permissions, audit, recovery, SQLite/PostgreSQL compatibility, and rollback; repair bounded defects and activate PE4-CONTRACT-1 without beginning replay implementation.

**Acceptance:** Recheck deterministic precedence/deduplication/conflict/expiry behavior, source-owner completeness and source-read failure, queue immutability/pagination/restart across SQLite and PostgreSQL, read API/OpenAPI/SDK/Dashboard compatibility, and adapter confirmation, exact-page hash binding, permission order, audit ownership, unsupported-action fail-closed behavior, and rollback. Closeout may repair only independently demonstrated PE-3 defects. It must leave no new queue store, action authority, scheduler, approval system, or Dashboard mutation control.

**Completion evidence:** Independent review of the merged PE-3 chain, focused action authorization regression, existing queue/read/Dashboard/PostgreSQL evidence, the full repository verification baseline, exact-head green CI, and synchronized authoritative documents. PE4-CONTRACT-REPAIR-1 is ready for execution; rollback remains a revert of the individual PE-3 PRs and no migration cleanup is required.

## PE-4 — Trace-backed Policy Replay

### Packet PE4-CONTRACT-1 — Calibration and coverage contract

**State:** `COMPLETE`

**Prerequisite:** PE3 closeout and sufficient versioned trace evidence.

**Goal:** Define a versioned, deterministic replay eligibility contract over existing `feedback_trace.v1` and offline-evaluation owners before adding replay persistence, live routing, or promotion behavior.

**Contract decision:** The original `policy_replay_contract.v1` design boundary accepts only bounded, non-secret `feedback_trace.v1` observations with a parseable timestamp, stable trace/dispatch identity, task class, selected candidate identity, terminal outcome, measured cost/latency, and a compatible quality measurement. A replay cohort is comparable only when its task class, objective, candidate definition, measurement schema, and time window match; duplicate identities, inconsistent candidate definitions, missing measurements, stale traces, and mixed incompatible schemas are rejected with sorted reason codes. Eligibility requires at least 30 accepted observations per compared candidate, at least 3 paired judge/reference samples per judge, no more than 10% rejected or uncovered observations, and a configurable maximum trace age no greater than 30 days. A candidate outside the observed task-class, objective, endpoint/member set, complexity bucket, or cost/latency envelope is `out_of_distribution`; any insufficient, stale, incomparable, uncovered, or OOD cohort produces a hash-bound refusal, never a recommendation. PE4-CONTRACT-REPAIR-1 implements this boundary as the stricter normalized `policy_replay_contract.v2`/`trace_replay_evidence.v1` contract below.

**Authority and progression:** Offline reports remain derived evidence, `shadow_only`, and non-mutating. PE4-REPLAY-1 may reuse `RunTraceRecorder` and `OfflineEvaluationEngine` only after this contract is implemented and tested. Shadow, bounded canary, promotion, pause, and rollback must call their existing owners; no offline or shadow result changes live policy. Existing `ContextualPolicyPromotion` retains its confirmation, evidence, rollout, pause, and rollback gates.

**Acceptance and rollback:** The durable contract text merged in PR #192. PR #193 added only an initial caller-asserted eligibility prototype; its booleans and manually supplied candidate data are not accepted as trace, coverage, calibration, comparability, or OOD evidence and are superseded by PE4-CONTRACT-REPAIR-1. Rollback is a revert with no migration or cleanup.

### Packet PE4-CONTRACT-REPAIR-1 — Real trace-backed replay evidence

**State:** `COMPLETE`

**Prerequisite:** PE3-CLOSE-1 complete and sufficient versioned trace evidence available from existing owners.

**Goal:** Replace or subordinate the #193 prototype with deterministic normalized replay observations derived from existing RunTrace, persisted feedback/attribution evidence, offline evaluation, policy simulation, and compatible quality evidence.

**Contract:** `policy_replay_contract.v2` and `trace_replay_evidence.v1` normalize only fields derived from `RunTrace` and persisted trace sections. They bind trace/dispatch identity, observation time, task class/domain/intent/objective, candidate identity/version/definition hash, endpoint/member set when present, routing/policy binding, measurement schema, complexity bucket, terminal outcome, latency, tokens, measured or posted cost, retries, quality meaning, judge/reference pairing, bounded source references, and canonical content hashes. Caller booleans, caller candidate definitions, caller coverage claims, and sample-count-only calibration are not inputs to trust. Accepted and rejected observations, malformed/stale/duplicate/tampered evidence, incompatible cohorts, missing measurements, incomplete pricing, unmeasured/unpriced values, sparse and incomplete paired coverage, actual judge calibration, cost/latency/token/retry envelopes, and scope-based OOD reasons are explicit deterministic outputs. Any refusal remains `shadow_only` and hash-bound.

**Forbidden:** No live policy mutation, provider call, hidden threshold, opaque authoritative score, automatic substitution, budget mutation, new experiment/pause/promotion/rollback owner, target-repository write, or PE-5/PE-6 work.

**Owning paths:** `engine/src/feedback/replay_eligibility.rs`, `engine/src/feedback/run_trace_recorder.rs`, `engine/src/feedback/outcome_attributor.rs`, `engine/src/feedback/offline_evaluation.rs`, `engine/src/feedback/mod.rs`, and focused feedback tests. No persistence or wire surface is added by this packet.

**Acceptance:** Versioned normalized-observation, cohort, coverage, calibration, eligibility, and refusal contracts; trace-hash and contradictory-section tests; a safe adapter into the existing offline evaluator; no silent serialization fallback; no fabricated evidence hash; no live mutation; exact-head full CI. Rollback is a code revert with no migration or cleanup.

**Completion evidence:** PR #197 merged as `d38d31bf17824e1587003268f5d6f58c6ba4afdd` from exact head `0af41d2b26f26f81fc9c7ae95066bfe3d3183cf7`; exact-head CI `29182261087` and post-merge `main` CI `29182505029` passed all seven required jobs. The merged contract derives all eligibility from trace-owner input and keeps the compatibility constructor non-authoritative.

### Packet PE4-OFFLINE-1 — Deterministic comparable-cohort replay

**State:** `COMPLETE`

**Prerequisite:** PE4-CONTRACT-REPAIR-1 merged with post-merge `main` CI green (`d38d31bf17824e1587003268f5d6f58c6ba4afdd`, CI `29182505029`).

**Goal:** Evaluate accepted trace-derived cohorts against explicit current and candidate policy descriptions without changing production state.

**Owning paths:** `engine/src/feedback/offline_evaluation.rs`, `engine/src/feedback/policy_simulator.rs`, and focused offline replay tests.

**Contract:** Consume raw `ReplayEligibilityRequest` trace-owner input and derive accepted `trace_replay_evidence.v1` observations inside the existing eligibility owner; callers cannot establish `eligible`, coverage, calibration, or accepted evidence by supplying a result. Compare explicit versioned current and candidate policy definitions while keeping observed facts separate from counterfactual estimates. Bind every report to trace/evidence hashes and candidate/policy versions, and return deterministic `insufficient_evidence`, `incompatible_cohort`, `stale`, `tampered`, `uncalibrated`, and `out_of_distribution` outcomes. No provider call, substitution, live routing, opaque authority score, or mutation.

**Acceptance and rollback:** Comparable task/objective/measurement/policy/member/complexity cohorts, paired coverage, calibrated judge evidence, cost/latency/token/retry envelopes, deterministic ordering, and non-mutation tests pass. Revert the packet; no migration or cleanup.

**Completion evidence:** PR #198 merged as `5a4a3e049574f54500dfdf4dc312f68ac5b6d78d` from exact head `0ab7f8c8274ba2fce2c90aaaf7d6d3a04d4560dd`; exact-head CI `29183389219` passed all seven required jobs. Post-merge `main` CI `29183843076` is the current monitored run. The merged implementation derives eligibility internally from raw trace-owner input, keeps observed facts separate from estimates, and remains shadow-only/non-mutating.

### Packet PE4-READ-1 — Read-only replay evidence surfaces

**State:** `COMPLETE`

**Prerequisite:** PE4-OFFLINE-1 merged with post-merge `main` CI green (`5a4a3e049574f54500dfdf4dc312f68ac5b6d78d`; post-merge CI `29183843076` passed all seven jobs).

**Goal:** Expose bounded accepted replay and comparison evidence through existing read owners only.

**Owning paths:** `engine/src/storage/local_product_store/offline_replay_artifacts.rs` and additive schema v20 migration, existing scorecard HTTP/OpenAPI handlers/routes, Python/TypeScript SDK readers, and the existing `DynamicRegulator` Dashboard surface.

**Contract:** Persist only validated `offline_replay_artifact.v1` envelopes whose report, policy hashes, eligibility hash, source evidence hashes, schema version, and shadow-only flags verify. Writes are idempotent and audit metadata-only; reads use deterministic bounded ordering/pagination and validate stored JSON on every read. HTTP/OpenAPI/SDK/Dashboard readers expose empty, insufficient, invalid, stale, tampered, OOD, and error states. SQLite v20 and PostgreSQL v20 remain aligned, old rows/callers remain compatible, and no live policy mutation or new evidence authority is added.

**Acceptance and rollback:** PR #199 merged as `92ade400174ee49d1efa3d1447830d936aa3e4b6` from exact head `5c78ca1d5aa1b93516f15822991f3edcfa3072f2`; exact-head CI `29184325125` passed all seven jobs after repairing the integrity owner, and post-merge main CI `29184652464` passed all seven jobs. HTTP/OpenAPI parity, encoded SDK paths, migration compatibility, SQLite/PostgreSQL, Dashboard states, idempotent hash-bound recording, tamper rejection, integrity-table coverage, and read-only permission tests pass. Revert the additive route/storage changes; preserve existing data.

### Packet PE4-SHADOW-1 — Shadow comparison only

**State:** `COMPLETE`

**Prerequisite:** PE4-READ-1 merged as PR #199 with exact-head CI green; post-merge main CI `29184652464` passed all seven required jobs.

**Goal:** Compare predicted and observed quality, cost, latency, retry, and coverage using the existing shadow-routing/evaluation owners.

**Owning paths:** `engine/src/feedback/shadow_router.rs`, existing evaluation/attribution owners, audit and provider-adapter gates.

**Contract:** Shadow only; bind policy/candidate/trace versions and hashes; record drift, insufficiency, stale, tampered, uncalibrated, and OOD evidence; provider adapters may be used only through existing authorization, cost, audit, timeout, and kill gates.

**Acceptance and rollback:** Shadow non-mutation, version binding, drift, insufficiency, permission, audit, timeout, kill, and restart tests pass. Revert the packet; no live route or policy changes remain.

**Completion evidence:** PR #200 merged as `54b3d46192f1de9b0bbaf1a1d83a7abaafff0201` from exact head `51a4b3a8b89ad1f65d74767b452e2546cf2526d7`; exact-head CI `29184759873` passed all seven required jobs, and post-merge `main` CI `29185040397` passed all seven jobs. The merged owner remains derived and shadow-only.

### Packet PE4-CANARY-1 — Bounded canary through the existing experiment owner

**State:** `IN_PROGRESS`

**Prerequisite:** PE4-SHADOW-1 merged as PR #200 with exact-head CI and post-merge `main` CI green; the existing `AdaptiveExperimentController` is the verified canary authority.

**Goal:** Add default-off, explicitly confirmed, permissioned, bounded, reversible canary execution through the existing experiment owner.

**Owning paths:** `adaptive_experiment.rs`, `adaptive_auto_promotion.rs`, existing pause/audit/workflow owners, and restart/fault tests.

**Contract:** Require exact policy/candidate versions, minimum compatible evidence, bounded scope and duration, explicit confirmation and permission, existing audit/idempotency, automatic pause through the existing pause authority, compensation, restart safety, and rollback. No direct full rollout or second canary state machine.

**Acceptance and rollback:** Default-off, scope/duration, minimum evidence, pause, compensation, restart, audit, idempotency, and rollback tests pass. Revert the packet and leave prior experiment state untouched.

**Implementation boundary:** This packet adds only a deterministic, hash-bound decision envelope and focused owner tests. It does not add a persistence table, HTTP mutation route, provider call, live routing/policy mutation, or parallel experiment state machine. Audit and actual activation remain with the existing experiment/operator owners.

### Packet PE4-PROMOTION-1 — Authoritative guarded promotion

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE4-CANARY-1 merged with post-merge `main` CI green and existing promotion authority verified.

**Goal:** Extend the existing promotion owner so promotion requires the complete compatible evidence chain.

**Owning paths:** `contextual_policy.rs`, `adaptive_auto_promotion.rs`, existing policy snapshot, permission, confirmation, audit, pause/resume, compensation, restart, and rollback owners.

**Contract:** Require compatible offline, shadow, and bounded canary evidence; coverage; actual calibration where applicable; non-OOD evidence; quality/cost/latency guardrails; exact policy versions; source hashes; rollout scope; and rollback target. Offline or shadow evidence alone never authorizes promotion.

**Acceptance and rollback:** Confirmation, permission, audit, idempotency, pause, resume, rollback, restart, compensation, version/hash binding, and fail-closed negative-path tests pass. Revert the packet through the existing promotion owner; no parallel authority is created.

### Packet PE4-CLOSE-1 — Independent PE-4 acceptance seal

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE4-PROMOTION-1 merged with post-merge `main` CI green.

**Goal:** Independently audit and acceptance-seal the complete PE-4 chain.

**Audit:** Trace grounding, comparability, coverage, calibration, OOD, offline non-mutation, read/API/OpenAPI/SDK/Dashboard compatibility, SQLite/PostgreSQL parity, shadow non-mutation, canary safety, promotion authority, permissions, confirmation, audit, pause, resume, compensation, rollback, restart, active-document consistency, and residual risks.

**Acceptance and rollback:** Repair any discovered defect in a separate coherent implementation PR before closeout; merge only with exact-head 7/7 CI and green post-merge `main` CI. Mark PE-4 complete only in the closeout evidence. Revert the individual implementation PRs; preserve existing owner data and recovery paths.

## PE-5 — Release Provenance

### Packet PE5-SBOM-1 — Deterministic SBOM generation

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** explicit independent-lane activation and no conflicting release PR.

PE-5 retains external/ephemeral signing identity, source/workflow/target/dependency/artifact binding, fail-closed installer verification, and atomic rollback. It is not active in this effort.

## PE-6 — Fault Injection and Recovery Drills

### Packet PE6-INVARIANTS-1 — Recovery invariants contract

**State:** `DECISION_REQUIRED`

The agent may define recovery invariants from existing subsystem contracts and tests before any injection work. Each affected subsystem must have explicit normal-state, failure-state, recovery-success, rollback-success, data-integrity, audit, timeout, and abort invariants. No destructive external testing is authorized.

## Active Routing

1. Monitor the exact-head CI for PE4-CANARY-1 and merge it only after all seven required jobs are green and no unresolved objection remains; then refresh `main` and verify post-merge CI.
2. Execute PE4-PROMOTION-1 as a separate coherent PR after the canary merge and post-merge verification; then execute PE4-CLOSE-1 independently.
3. Leave PE-5 and PE-6 unstarted.
