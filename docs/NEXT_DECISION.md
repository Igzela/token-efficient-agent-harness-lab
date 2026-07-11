# Next Decision

## Current Direction

The dispatch kernel, V2, Adaptive Fusion AF-0 through AF-7, Agent Runtime AR-0 through AR-6, Trusted Local Autonomous Execution IAE-0 through IAE-3, scorecard integrity hardening, the importer-first external benchmark path, and PE-1 Token Efficiency Regression Lab are complete.

The active direction is the Post-LGB Product Evolution plan. PE-2 is complete and acceptance-sealed; PE-3 is active with its contract and derived-queue packets complete. This is not AR-7, another LGB ladder, or a second control plane.

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
| PE-3 | P1 | Operator Decision Center | In progress; contract and derived queue complete, read surfaces next |
| PE-4 | P1/P2 | Trace-backed Policy Replay | Packetized; blocked on PE-3 closeout and trace coverage |
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

PE-3 is active. Its versioned contract and mutation-free derived queue are complete; the read-surface packet is next.

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

### Packet PE3-CLOSE-1 — PE-3 acceptance seal

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE3-ACTIONS-1 complete.

**Goal:** Independently audit contracts, source adapters, derived queue, API/OpenAPI, SDKs, Dashboard, permitted actions, permissions, audit, recovery, SQLite/PostgreSQL compatibility, and rollback; repair bounded defects and activate PE4-CONTRACT-1 without beginning replay implementation.

Later PE-3 packets remain: deterministic derived queue, read-only API/SDK/Dashboard surface, existing-control action adapters, and closeout.

## PE-4 — Trace-backed Policy Replay

### Packet PE4-CONTRACT-1 — Calibration and coverage contract

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE3 closeout and sufficient versioned trace evidence.

PE-4 remains offline-replay first, then shadow, then bounded canary through existing experiment/promotion/pause/rollback owners. Sparse, stale, uncovered, or out-of-distribution evidence must refuse recommendations.

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

1. Merge PE3-CONTRACT-1 after focused/full verification and green CI.
2. Refresh `main`, then execute PE3-QUEUE-1.
3. Continue PE3-READ-1, PE3-ACTIONS-1, and PE3-CLOSE-1 in order; do not begin PE-4 implementation before closeout.
