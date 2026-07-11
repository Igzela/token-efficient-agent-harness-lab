# Next Decision

## Current Direction

The dispatch kernel, V2, Adaptive Fusion AF-0 through AF-7, Agent Runtime AR-0 through AR-6, Trusted Local Autonomous Execution IAE-0 through IAE-3, scorecard integrity hardening, the importer-first external benchmark path, and PE-1 Token Efficiency Regression Lab are complete.

The active direction is the Post-LGB Product Evolution plan. PE-2 is active; PE-3 is next but not started. This is not AR-7, another LGB ladder, or a second control plane.

`docs/NEXT_DECISION.md` is the single forward-plan artifact. Historical detail remains in `docs/ARCHITECTURE_BOOK.md`, archived plans, merged PRs, and repository history.

## Full Agent Autonomy Mode

Full Agent Autonomy Mode remains active for repo-scoped, testable, observable, and rollbackable execution inside an approved Terra-ready task packet.

### Autonomously maintain and evolve

A Terra Medium Codex executor may inspect, implement, test, review, open a PR, repair ordinary CI failures, update active docs, and merge a packet when all packet and playbook gates pass.

It may not create new architecture directions, authority semantics, migration policy, security policy, release trust policy, or recovery invariants. Those decisions must already be fixed by the external planner in the packet.

## Mandatory Executor Profile

All Codex execution for this plan uses:

- `gpt-5.6-terra`
- reasoning effort `medium`
- review model `gpt-5.6-terra`
- plan-mode reasoning effort `medium`

The project default is `.codex/config.toml`. Do not use Sol or self-escalate reasoning effort. A profile mismatch is a hard stop, not permission to continue with a different model.

## Terra-Ready Packet Protocol

Codex may execute only a packet marked `READY_FOR_TERRA` whose prerequisites are complete. It must choose the earliest such packet in the normative sequence.

Packet states:

- `READY_FOR_TERRA` — contract is complete and prerequisites are satisfied.
- `BLOCKED_PREREQUISITE` — contract is defined, but an earlier packet must complete first.
- `PLANNER_DECISION_REQUIRED` — implementation must not begin until the missing decision is supplied.
- `IN_PROGRESS` — one active branch/PR owns the packet.
- `COMPLETE` — acceptance evidence is merged and active docs are updated.

Every packet inherits these requirements:

| Field | Required contract |
|---|---|
| Goal | One observable result, not a broad stage aspiration |
| Prerequisites | Exact earlier packets or existing contracts that must be complete |
| Owning paths | Existing source/test/document owners to extend |
| Allowed changes | Minimum coherent implementation surfaces |
| Forbidden changes | No parallel runtime, scheduler, store, policy authority, mailbox, artifact truth source, or Dashboard state model |
| Contract | Versioned inputs, outputs, reason codes, bounds, permissions, and failure states |
| Verification | Focused tests plus applicable full repository validation |
| Compatibility | SQLite/PostgreSQL, API/SDK, existing rows, and old callers remain compatible when applicable |
| Rollback | `git revert` plus any bounded cleanup procedure |
| Completion evidence | PR, commit, CI run, test evidence, compatibility, residual risk, and next packet status |
| Stop triggers | Missing contract, authority expansion, irreversible migration, security ambiguity, conflicting active docs/code, or two failed repair cycles |

Stage prose is context only. It does not authorize implementation outside the packets below.

## Hard Stops

Stop with evidence rather than improvising when any of these applies:

- the required packet is not `READY_FOR_TERRA`;
- the executor profile is not Terra Medium;
- code and active documents conflict on authority or ownership;
- a change requires a second runtime, store, scheduler, policy authority, or Dashboard state model;
- a migration is irreversible or lacks SQLite/PostgreSQL compatibility and rollback;
- security, release, provider, pause, or target-output authority is ambiguous;
- two coherent repair cycles fail to resolve the same CI root cause.

## Post-LGB Product Evolution Plan

Normative order is PE-1, PE-2, PE-3, PE-4, PE-5, and PE-6. Do not start PE-3 before PE-2 closeout.

| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-1 | P0 | Token Efficiency Regression Lab | Complete and acceptance-sealed |
| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | Active; anomaly packet ready |
| PE-3 | P1 | Operator Decision Center | Packetized; blocked on PE-2 closeout |
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

**State:** `READY_FOR_TERRA`

**Prerequisite:** PE2-FORECAST-1 complete.

**Goal:** Detect bounded cost, token, retry, latency, context-growth, and model-mix anomalies with explicit thresholds, confidence, coverage, evidence, and reason codes.

**Allowed changes:** Deterministic/statistical rules over the versioned contract and existing evidence.

**Forbidden changes:** No hidden adaptive threshold, provider/model substitution, pause, termination, budget/policy mutation, persistence/API/Dashboard work, or opaque score.

**Acceptance:** Normal, spike, gradual drift, mixed workloads, sparse history, false-positive boundaries, duplicated evidence, out-of-order evidence, deterministic recomputation, and `insufficient_evidence` tests.

### Packet PE2-READ-1 — Persistence, API, SDK, and Dashboard read surfaces

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE2-ANOMALY-1 complete.

**Goal:** Expose bounded forecasts and anomaly evidence through existing store/API/SDK/Dashboard owners.

**Allowed changes:** Additive `LocalProductStore` schema only if persistence is required by the accepted contract; read-only HTTP/OpenAPI; Python/TypeScript readers; current Dashboard components; SQLite/PostgreSQL migrations/tests.

**Forbidden changes:** No pause action, policy mutation, provider call, business-fact conversion, or second state model.

**Acceptance:** Idempotent persistence if used, bounded pagination, permissions, OpenAPI/router parity, encoded SDK paths, empty/error/sparse UI states, evidence links, migration/backward compatibility, and full stack verification.

**Stop triggers:** Persistence semantics were not fixed by PE2-CONTRACT-1 or require rewriting existing budget history.

### Packet PE2-PAUSE-1 — Policy-gated high-confidence auto-pause

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE2-READ-1 complete.

**Goal:** Invoke the existing pause mechanism only for policy-enabled, high-confidence supported findings, with complete audit and idempotent recovery behavior.

**Owning paths:** Existing scheduler/workflow pause controls, policy gates, audit, operator evidence, API controls, and focused integration tests.

**Allowed changes:** A narrow decision adapter from validated anomaly evidence to the existing pause path; explicit default-off enablement; audit; resume/override evidence.

**Forbidden changes:** No auto-kill, silent budget edit, provider/model substitution, new pause state machine, default enablement, implicit resume, or resume without operator evidence.

**Contract:** Fail closed on missing/disabled policy, incomplete pricing, low confidence, stale evidence, unsupported anomaly, missing coverage, audit failure, concurrent duplicate triggers, or unavailable pause owner. Repeated triggers are idempotent. Resume and override preserve cause and evidence.

**Acceptance:** False positives, concurrency, duplicate trigger, audit failure, pause failure compensation, disabled policy, sparse data, incomplete pricing, resume, override, permission, restart, and rollback tests.

**Stop triggers:** Existing pause semantics cannot guarantee idempotency/compensation, policy ownership is ambiguous, or audit cannot precede/atomically bind the authority decision.

### Packet PE2-CLOSE-1 — PE-2 acceptance seal

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE2-PAUSE-1 complete.

**Goal:** Audit the entire PE-2 chain, repair bounded defects, mark PE-2 complete, and activate PE3-CONTRACT-1 without beginning PE-3.

**Acceptance:** Forecast/anomaly evidence is explainable, deterministic, versioned, bounded, and explicitly insufficient when unsupported; read surfaces are compatible; auto-pause is default-off, explicitly enabled, high-confidence, audited, idempotent, fail-closed, reversible, and recoverable; full CI and SQLite/PostgreSQL checks are green.

## PE-3 — Operator Decision Center

PE-3 is next but not started. It remains blocked until PE2-CLOSE-1 is complete.

### Packet PE3-CONTRACT-1 — Decision item and source contract

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE2-CLOSE-1 complete.

**Goal:** Define bounded versioned action-item/source contracts and deterministic source precedence over existing approvals, workflow/scheduler, budget, benchmark, policy, and rollback evidence. No queue persistence, action execution, new authority, or Dashboard work is authorized by this packet.

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

**State:** `PLANNER_DECISION_REQUIRED`

PE-6 may not inject failures until each affected subsystem has explicit normal-state, failure-state, recovery-success, rollback-success, data-integrity, audit, timeout, and abort invariants. No destructive external testing is authorized.

## Active Routing

1. Execute PE2-ANOMALY-1 from latest `main`.
2. Merge only after focused validation, full CI, architecture/authority review, and no unresolved objection.
3. Refresh `main`, re-read active docs/code, and continue PE2-FORECAST-1, PE2-ANOMALY-1, PE2-READ-1, PE2-PAUSE-1, then PE2-CLOSE-1.
4. After PE-2 closeout, mark PE-3 next but do not start it in the PE-1-to-PE-2 effort.
