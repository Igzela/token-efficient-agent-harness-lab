# Current Status

Last updated: 2026-07-12.

## Summary

This repo is a local/small-team self-hosted agent workflow control plane. Rust `engine/` is the sole runtime/API/storage implementation. Active docs stay small and current.

## Complete Tracks

| Track | Status |
|---|---|
| Dispatch kernel | Complete |
| Phase 4 | Historical and complete |
| V2 Real Production Output | Complete |
| Adaptive Fusion | Complete through AF-7 |
| Agent Runtime | Complete and sealed at AR-6 |
| Trusted Local Autonomous Execution | Complete through IAE-3 |
| Token-efficiency scorecards | Canonical hash and derived-metric integrity enforced; native v1 and generic v2 artifacts share one store/API boundary |
| LangGraph importer-first pilot | Complete through real offline LangGraph 1.2.9 evidence, v2 persistence, scenario API/Dashboard, and fixed end-to-end fixtures |
| Local stateful-vs-stateless runner | Implemented with bounded stub/fake/live paths and gated local execution |
| Local runner evidence chain | Storage/API/operator evidence consumption and local artifact import implemented |
| Agent memory policy layer | Implemented inside existing AgentState, AgentStepExecutor, context packing, workflow injection, operator evidence, and scorecard state-byte paths without a new runtime/store |
| PE-1 Token Efficiency Regression Lab | Complete through deterministic registry/report/batch recomputation, bounded evidence, SQLite/PostgreSQL persistence, idempotent import, trends, read-only API/SDK, Dashboard history, and acceptance seal |
| Post-R7 Wire/Type Governance Hardening | Implemented through `scripts/check_wire_codegen_drift.sh` |
| Security and release hardening | Auth scope/expiry/live-clock fixes, persistent redacted provider audit, Rust/Bun advisory gates, SHA-pinned Actions, target-correct release packaging, and atomic upgrade rollback are implemented |

## Active Tracks

| Track | Status |
|---|---|
| Agent Autonomous Maintenance Mode | Active through execution-ready task packets and bounded prerequisite repairs |
| Full Agent Autonomy Mode | Active for repo-scoped planning, design, implementation, review, CI repair, documentation, and merge when changes are testable, observable, CI-gated, and rollbackable |
| Model selection | User/tool controlled; repository policy does not require or forbid a model or reasoning tier |
| Autonomous decision authority | The coding agent may resolve bounded architecture, authority, schema, migration, security, release, and recovery decisions from repository evidence and record material decisions before implementation |
| Local runner operations | Active: validate, export, import, and inspect bounded scorecard artifacts locally |
| Runner integration | Storage/API/operator evidence complete; workflow scheduling of local runner validation is implemented via `LocalRunnerValidationExecutor` in stub mode with automatic native scorecard recording on terminal tick |
| Live provider adapter | Gated ready path: Stub/Fake/Live; Live requires gates, explicit metadata, symbolic credentials, positive pricing, persistent redacted audit, bounded calls/tokens/time/cost, and a kill switch |
| Post-LGB Product Evolution Plan | PE-1 through PE-3 remain acceptance-sealed; PE-4 is under `PE4-POST-CLOSE-REPAIR-1`; PE-5 and PE-6 remain unstarted |

## Planned Product Evolution Stages

The stages are packetized in `docs/NEXT_DECISION.md`. The agent should execute packets marked `READY_FOR_EXECUTION` whose prerequisites are complete, while repairing bounded prerequisite defects or contract drift when needed.

| Stage | Priority | Capability | Current state |
|---|---|---|---|
| PE-1 | P0 | Token Efficiency Regression Lab | Complete and acceptance-sealed |
| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | Complete and acceptance-sealed |
| PE-3 | P1 | Operator Decision Center | Complete and independently acceptance-sealed after PE3-REPAIR-1 and PE3-CLOSE-1 |
| PE-4 | P1/P2 | Trace-backed Policy Replay | Under post-close correctness repair `PE4-POST-CLOSE-REPAIR-1`; the pre-repair closeout is not final acceptance evidence |
| PE-5 | P1.5 | Release Provenance | Not started; inactive in the current bounded objective |
| PE-6 | P2 | Fault Injection and Recovery Drills | Not started; blocked on explicit recovery invariants and affected stage prerequisites |

## PE-1 Acceptance Evidence

- deterministic single-scenario and registry-wide report recomputation is covered by focused Python tests;
- tamper, threshold, quality-failure, missing-baseline, missing-best-known, incomparable, cross-version, and registry-coverage cases are covered;
- SQLite and PostgreSQL regression artifact persistence, repeat import, deterministic trends, and audit behavior are covered;
- list/detail/trend HTTP routes and Python/TypeScript SDK readers are covered;
- Dashboard tests cover all six outcomes, absent evidence roles, empty and one-point histories, transitions, encoded paths, and exclusion of raw fields;
- PR #177 CI run 29137424748 passed all seven jobs, including PostgreSQL, Rust, TypeScript/Dashboard, Python, Docker, native runtime, full-stack cutover, formatting, clippy, and dependency audits;
- PE-1 remains report-only: no CI blocking, provider calls, routing mutation, policy mutation, pause authority, or target-repository writes were added.

## PE-2 Contract Evidence

- `budget_forecast_evidence.v1` and `budget_anomaly_finding.v1` are additive Rust contracts under the existing budget owner;
- scope, evidence windows, freshness, sample counts, coverage, pricing completeness, duplicate counts, confidence, stable reason codes, bounded references, and deterministic hashes are explicit;
- `supported`, `insufficient_evidence`, and `invalid_evidence` remain distinct outcomes;
- observed values are structurally separate from estimates and anomaly measurements;
- malformed, oversized, tampered, sparse, contradictory, unpriced, missing-dimension, and invalid-window evidence fails closed or remains explicitly insufficient;
- no persistence, API, SDK, Dashboard, provider, reservation, policy, pause, or target-output behavior changes are part of the contract packet.

## PE-2 Forecast Evidence

- deterministic forecasts use only bounded posted observations and explicit half-open evidence windows;
- observed token/cost totals remain separate from linear horizon estimates and exhaustion time;
- sparse, stale, mixed required dimensions, missing dimensions, excessive duplicates, conflicting duplicates, and incomplete pricing return explicit bounded outcomes;
- provider audit adaptation does not invent run, workspace, model, content hash, or non-USD pricing facts;
- focused tests cover zero usage, bursty usage, mixed workloads, boundary time, deterministic ordering, duplicate reconciliation, conflicting evidence, and concurrent reads;
- no persistence, API, SDK, Dashboard, budget/reservation mutation, policy, pause, or target-output behavior changes are part of the forecast packet.

## PE-2 Anomaly Evidence

- deterministic rules cover cost, token, retry, latency, context-growth, and model-mix findings over explicit equal-duration windows;
- supported normal evidence remains `detected=false`, while sparse, stale, mixed, incomplete, or excessive-duplicate evidence remains explicitly insufficient;
- coverage metadata derives `missing_fields` only from fields absent in filtered evidence and preserves observed dimensions on applicable invalid-evidence paths;
- conflicting duplicates and malformed metric evidence fail closed as versioned `invalid_evidence` findings with bounded references and reason codes;
- thresholds are explicit and deterministic; equality does not create a false positive and no adaptive or opaque score is introduced;
- no persistence, API, SDK, Dashboard, provider substitution, budget/reservation mutation, policy, pause, termination, or target-output behavior changes are part of the anomaly packet.

## Current Gaps

- PE4-CONTRACT-REPAIR-1 through PE4-CLOSE-1 are historical merged work. Their weaker replay semantics are superseded while `PE4-POST-CLOSE-REPAIR-1` is in progress; the prior closeout claim is not acceptance evidence for this repair.
- `policy_simulator.rs` still relies on fixed estimates rather than trace-calibrated replay.
- SBOM, signing, and provenance attestations are not yet part of the release contract.
- There is no systematic fault-injection and recovery-drill harness.
- External runtime runners, scheduled external execution, CrewAI, and Microsoft Agent Framework integrations remain unauthorized unless a later evidence-backed decision changes the importer-first boundary.
- Remote adapter support for the local runner remains deferred.
- Keep existing runtime/module ownership boundaries unless an execution-ready packet or documented replacement decision explicitly changes them.

## PE-2 Read Evidence

- schema v18 adds immutable, hash-bound budget forecast/anomaly envelopes to the existing SQLite/PostgreSQL `LocalProductStore` boundary;
- persistence is validated, idempotent, deterministically ordered, tamper-detecting, and bounded to limit 100 with offset capped at 10,000;
- read-only `dispatch:read` HTTP/OpenAPI plus Python/TypeScript SDK readers preserve encoded artifact paths and existing callers;
- the existing Benchmark Dashboard route exposes explicit supported, insufficient, invalid, empty, and error states with bounded evidence IDs, hashes, and reason codes;
- no pause, policy, provider, budget/reservation, scheduler, termination, or target-output authority is added.

## PE-2 Auto-Pause Evidence

- schema v19 stores one idempotent decision per run/evidence artifact in the existing SQLite/PostgreSQL owner;
- policy defaults disabled and requires explicit confirmation, `dispatch:execute`, high confidence, complete pricing, critical severity, fresh supported evidence, and exact run scope;
- decision persistence, audit, and the existing `workflow_runs.pause_reason` update are one transaction, so audit or pause failures compensate by rollback;
- duplicate/concurrent triggers return the same decision; audited resume/override preserves the cause and evidence hash and prevents replay of recovered evidence;
- generic pause mutation cannot clear or replace an active budget pause; no auto-kill, implicit resume, budget edit, provider/model substitution, or second pause state machine was added.

## PE-2 Acceptance Evidence

- contract, forecast, anomaly, read-surface, and pause packets merged as PRs #179, #180, #181, #183, and #184;
- exact-head CI runs 29138661868, 29139346681, 29141664483, 29142831181, and 29143549699 passed all required jobs, including PostgreSQL, Rust, TypeScript/Dashboard, Python, Docker, native runtime, and full-stack cutover;
- schema v19 migration, deterministic evidence recomputation, tamper rejection, bounded API/SDK/Dashboard reads, default-off policy, permission/confirmation gates, false-positive boundaries, concurrent idempotency, audit/pause rollback, restart, resume, and override behavior are covered;
- PE-2 introduced no auto-kill, silent budget mutation, provider/model substitution, implicit recovery, second pause owner, or target-repository authority.

## PE-3 Contract Evidence

- `operator_decision_source.v1` and `operator_decision_item.v1` normalize bounded references to existing evidence owners and remain derived evidence;
- deterministic resolution orders severity, source precedence, confidence, observation time, and source ID; duplicates collapse and equal-ranked incompatible actions fail closed;
- `ready`, `conflict`, `expired`, `insufficient_evidence`, and `resolved` remain explicit, and only `ready` may recommend an action;
- canonical hashes bind source and item content; tamper, expiry, staleness, low confidence, missing sources, resolution, and ordering are covered by focused Rust tests;
- no persistence, API, SDK, Dashboard, provider, policy, approval, pause/resume/retry/rollback, or target-output authority is added by the contract packet.

## PE-3 Independent Repair and Acceptance Evidence

- PR #195 repaired PE3-REPAIR-1's independently found timestamp-ordering defect: validated RFC3339 observation times are compared by instant rather than by textual representation, with a focused offset-boundary regression test;
- PR #195 merged as `8efe09b5fd2346b7e12ff3fc7cd897d6177c7eae` from exact head `fc547fd5c42d1ace86c58fd6a291aadeaad60272`; exact-head CI run `29180711721` passed all seven required jobs;
- the merged repair chain binds mutation actions to store-time freshness, exact queue pages and current state, source identity/hash, action, resource, conflict key, pagination, permission, confirmation, existing audit, owner idempotency, compensation, restart, and rollback behavior;
- focused evidence covers canonical contracts, precedence/deduplication/conflict, exact source identity, bounded original references without fabricated hashes, expiry/staleness, SQLite/PostgreSQL approval atomicity, blocked Retry and terminal behavior, budget pause/recovery, resume compensation, unsupported actions, concurrent actions, restart, API/OpenAPI, Python/TypeScript SDKs, and the read-only Dashboard;
- PE3-CLOSE-1 independently re-audited the merged chain and found no remaining PE-3 defect. PE-3 is acceptance-sealed; rollback remains a revert of the individual repair or closeout PRs, with no migration cleanup.

## PE-4 Contract Repair Evidence

- Historical record: PR #197 merged the replacement for the caller-asserted replay gate with `policy_replay_contract.v2` and `trace_replay_evidence.v1` under the existing feedback owner. `PE4-POST-CLOSE-REPAIR-1` supersedes those weaker semantics with v3/v2 while this repair is active.
- `ReplayTraceInput` verifies a canonical `RunTrace` content hash; normalized observations derive candidate identity/version/definition, task context, policy binding, measurement schema, terminal outcome, measured or posted cost, latency, tokens, retries, quality, and judge/reference pairs from persisted trace sections only.
- Accepted and rejected evidence, bounded source references, deterministic cohort/coverage/envelope output, actual paired judge calibration, stale/malformed/duplicate/incompatible/missing/unpriced/unmeasured/OOD/tampered reason codes, and `shadow_only` non-mutation are explicit.
- `OfflineEvaluationEngine::evaluate_trace_evidence` is the safe bridge; the older caller-shaped constructor remains compatibility-only and is not an eligibility or trust source. PR #197 merged with no migration, HTTP/API/SDK/Dashboard, routing, policy, provider, budget, experiment, pause, promotion, or production-state mutation.

## PE-4 Offline Replay Evidence

- PE4-OFFLINE-1 derives eligibility inside `OfflineEvaluationEngine::replay_policies` from raw `ReplayEligibilityRequest` trace-owner input; callers cannot supply an authoritative `ReplayEligibilityResult`.
- Explicit versioned current and candidate policy definitions are hash-bound. Reports separate observed facts from comparable-cohort counterfactual estimates and retain source trace/evidence hashes, candidate definitions, policy versions, and a report hash.
- Deterministic outcomes include sufficient, insufficient, incompatible cohort, stale, tampered, uncalibrated, and out-of-distribution. The replay report is always shadow-only and sets every live-influence flag false; it makes no provider call and mutates no production state.
- Focused replay tests cover observed/counterfactual separation, determinism, policy binding, source tampering, stale input, and OOD candidate selection. Historically, PE4-READ-1 added schema v20 `offline_replay_artifacts` storage with SQLite/PostgreSQL parity, hash-bound idempotent recording, metadata-only audit, deterministic paginated/status-filtered read routes, OpenAPI parity, encoded Python/TypeScript readers, and explicit DynamicRegulator empty/insufficient-invalid-OOD/error states. Those v1 report rows remain readable but non-authorizing; no live mutation, provider call, or promotion authority is added.

## PE-4 Shadow Evidence

- PE4-SHADOW-1 adds a hash-bound `ShadowReplayComparison` from the offline report. It preserves observed facts separately from counterfactual predictions, source trace/evidence coverage, explicit drift and insufficiency/OOD states, and all non-mutation flags. It cannot call providers or change routing/policy state. PR #200 is merged at `54b3d46192f1de9b0bbaf1a1d83a7abaafff0201`; exact-head CI `29184759873` and post-merge `main` CI `29185040397` passed all seven required jobs.

## PE-4 Canary Evidence

- PE4-CANARY-1 extends the existing `AdaptiveExperimentController` with a default-off, hash-bound canary decision contract. It requires exact policy/candidate/scope bindings, bounded 1–5% rollout and 24-hour maximum duration, minimum shadow evidence, explicit confirmation and permission, and the existing gate, pause, kill, compensation, and rollback metadata. Decisions are deterministic and restart/idempotency safe; the packet does not add persistence, provider calls, live routing mutation, or a second experiment authority.

## PE-4 Promotion Evidence

- PE4-PROMOTION-1 extends the existing promotion owner with a hash-bound chain containing sufficient offline, derived shadow, and started canary evidence. It re-derives shadow evidence from the offline report, validates actual judge calibration when present, binds candidate/policy/scope/trace hashes and rollback target, and applies quality, cost, latency, sample, confidence, and failure guardrails. The caller-only promotion path and local-store path fail closed with `complete_evidence_chain_required`; accepted promotion retains existing confirmation, permission, audit, snapshot, pause, and rollback owners.

## PE-4 Acceptance Evidence

- Independent closeout re-audited trace grounding, comparability, accepted/rejected coverage, calibration, stale/tampered/unpriced/unmeasured/OOD refusals, offline non-mutation, shadow derivation, canary bounds and gates, promotion chain/guardrails, permissions, confirmation, audit, pause, compensation, restart/idempotency, rollback, API/OpenAPI/SDK/Dashboard read compatibility, and SQLite/PostgreSQL parity.
- Focused closeout checks passed: CANARY 3/3, PROMOTION 5/5, offline replay 14/14, bounded offline replay HTTP read, and clean SQLite integrity. Existing merged PE3/PE4 tests cover concurrency, restart, migration, PostgreSQL, permission, confirmation, audit, pause, compensation, and rollback paths.
- PE4-PROMOTION-1 merged at `92b53e9abf2bebd51bddc6c0f7db880edabd396b` from exact head `230dba873365657db9881d7f661fd44a93164b45`; exact-head CI `29185784794` passed all seven jobs, and post-merge main CI `29186024415` passed all seven jobs.

- Historical pre-repair evidence: PE4-CLOSE-1 is merged as PR #203 at `008bc8c8879d6e7c9641fec57aa974f98af1c6b5` from exact head `2110676667dd1b57a36bc6f3744016599a02860a`; exact-head CI `29186113263` passed all seven jobs, and final post-merge main CI `29186372526` passed all seven jobs. Its PE-4 acceptance claim is superseded by `PE4-POST-CLOSE-REPAIR-1`; PE-5 and PE-6 remain unstarted.

## PE-4 Post-close Repair — PE4-POST-CLOSE-REPAIR-1

- **State:** `IN_PROGRESS` on `codex/pe4-post-close-repair`; starting `main` after the separately merged documentation-only PR #205 is `0f92dadc6cf1cb712231dbb917bf9904f8346d86`. The pre-repair PE-4 closeout is not final acceptance evidence.
- **Coverage contract:** `policy_replay_contract.v3` uses an inclusive integer 90% accepted boundary. Observation-local stale, missing, incomplete, malformed non-authoritative, and ordinary uncovered observations count against coverage; tamper, sensitive data, conflicting identity, inconsistent candidate definitions, canonicalization/serialization failure, and irreconcilable cohort failures are cohort-fatal; request/contract failures are request-fatal. IDs, reason codes, references, ordering, and hashes remain sorted and bounded.
- **Provenance:** trusted replay input is derived only from the existing `dispatch_history` owner through `RunTraceRecorder`, bound to owner history ID, `dispatch_history_trace_owner.v1`, and the owner-stored recorder hash. The public raw constructor and request deserialization cannot establish eligibility. SQLite and PostgreSQL add aligned schema version 21 nullable provenance columns; old rows without binding, mismatched IDs/schema/hashes, tampering, and restart inconsistencies refuse as `untrusted_trace_source`.
- **Schema and outcomes:** normalized evidence is `trace_replay_evidence.v2`; offline reports/policies are `offline_policy_replay.v2`; judge calibration is `judge_calibration.v1` with minimum 3 paired samples, absolute signed-bias tolerance 0.10, and MAE tolerance 0.15. Existing v1 offline rows remain readable as historical-only and non-authorizing. Recorder semantics distinguish terminal execution, execution outcome, evaluation completion/outcome, overall dispatch success, quality, and tool-success; failed quality and failed execution samples remain valid negative evidence when complete and consistent.
- **Boundedness and OOD:** request/trace/result canonical bytes, raw sections, JSON depth, identifiers, task fields, member endpoints, candidates, judges/pairs, evidence references, report arrays, reason codes, and token/retry/metric envelopes are explicitly bounded. Canonical forms are precomputed before ordering and failure paths are fail-closed. Caller scope is a constraint only; empirical support for task/domain/intent/objective, policy/cohort, candidate definition/member set, complexity, and measured cost/latency/tokens/retries must come from accepted observations, otherwise replay returns explicit OOD or insufficient evidence.
- **Downstream boundary:** offline replay, v2 artifact persistence, HTTP/OpenAPI, SDKs, Dashboard, shadow, canary, promotion, pause, and rollback continue to reuse existing owners. Historical v1 reports cannot authorize shadow, canary, or promotion. PE-5 and PE-6 remain unstarted. Final acceptance, exact-head CI, same-PR documentation evidence, merge, and post-merge `main` verification are pending.

## Handoff Guard Anchors

Branch: use latest `main` unless continuing an owned PR.

Tests: run focused checks and full stack verification before merge.

Full Agent Autonomy Mode remains active for repo-scoped, testable, observable, CI-gated, rollbackable work.

Execution governance: `scripts/check_agent_handoff.py` validates packet, autonomy, documentation, schema, wire, toolchain, and secret-scan anchors without enforcing a model choice.

Post-R7 Wire/Type Governance Hardening remains implemented through `scripts/check_wire_codegen_drift.sh`.

## Active Documentation

- `AGENTS.md`
- `docs/ARCHITECTURE_BOOK.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
- `docs/RUNBOOK.md`

Do not add new roadmap/status/policy documents by default. Put current direction and execution-ready task packets in the active surfaces above.
