# Next Decision

## Current Direction

The dispatch kernel, V2, Adaptive Fusion AF-0 through AF-7, Agent Runtime AR-0 through AR-6, Trusted Local Autonomous Execution IAE-0 through IAE-3, scorecard integrity hardening, and the importer-first external benchmark path are complete.

The active direction is the Post-LGB Product Evolution plan, PE-1 through PE-6. This is not AR-7, another LGB ladder, or a second control plane.

`docs/NEXT_DECISION.md` is the single forward-plan artifact. Historical detail remains in `docs/ARCHITECTURE_BOOK.md`, archived plans, merged PRs, and repository history.

## Full Agent Autonomy Mode

Full Agent Autonomy Mode remains active for repo-scoped, testable, observable, and rollbackable execution **inside an approved Terra-ready task packet**.

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

Every packet inherits these required fields:

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

## Stable Tracks

| Track | Status |
|---|---|
| Core dispatch kernel | Complete |
| Architecture refactor R-series | Sealed at R7 |
| V2 Real Production Output | Complete through V2-5 |
| Adaptive Fusion | Complete through AF-7 |
| Agent Runtime | Complete and sealed at AR-6 |
| Trusted Local Autonomous Execution | Complete through IAE-3 |
| External Runtime Benchmark Boundary | Importer-first pilot complete |
| Agent Autonomous Maintenance Mode | Active through Terra-ready packets |
| Full Agent Autonomy Mode | Active inside packet boundaries |

## Post-LGB Product Evolution Plan

Normative order is PE-1, PE-2, PE-3, PE-4, PE-5, and PE-6. PE-5 may proceed after PE-1 when no release work conflicts, but the executor still selects the earliest eligible packet unless the user explicitly activates the PE-5 lane.

| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-1 | P0 | Token Efficiency Regression Lab | Core, persistence, trends, and API/SDK complete; Dashboard and closeout remain |
| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | Packetized; blocked on PE-1 closeout |
| PE-3 | P1 | Operator Decision Center | Packetized; blocked on PE-2 closeout |
| PE-4 | P1/P2 | Trace-backed Policy Replay | Packetized; blocked on PE-3 closeout and trace coverage |
| PE-5 | P1.5 | Release Provenance | Packetized; eligible after PE-1 closeout or by explicit lane activation |
| PE-6 | P2 | Fault Injection and Recovery Drills | Packetized; blocked on explicit subsystem recovery invariants |

## PE-1 — Token Efficiency Regression Lab

Implemented facts:

- canonical `token_efficiency_regression_registry.v1`
- deterministic single-scenario and registry-wide batch reports
- fixed bounded evidence for LangGraph, native deterministic, and local stub scenarios
- SQLite/PostgreSQL `LocalProductStore` persistence and idempotent local import
- deterministic bounded history/trend read model
- read-only HTTP list/detail/trend endpoints and Python/TypeScript SDK readers
- report-only behavior with no CI blocking, provider call, routing change, or mutation authority

### Packet PE1-UI-1 — Dashboard history and trend UX

**State:** `READY_FOR_TERRA`

**Goal:** Existing regression API data is visible in the current Dashboard with scenario history, bounded trend, baseline/best-known context, regression reasons, and evidence links.

**Prerequisites:** PR #175 merged; `/api/v1/regressions`, detail, and trend endpoints available; existing SDK contracts available.

**Owning paths:** `dashboard/`; existing benchmark/scorecard components; existing Dashboard API client; focused Dashboard tests. Update `docs/CURRENT_STATUS.md`, this file, and `docs/MODULE_MAP.md` only if ownership/facts change.

**Allowed changes:** Add or extend current scorecard/benchmark UI components, bounded client calls, loading/empty/error states, deterministic formatting, and tests.

**Forbidden changes:** No new API route, storage table, persistence path, scheduler, provider call, policy authority, write action, or second Dashboard state model. Do not change regression report/trend semantics to make the UI easier.

**Contract:**

- consume existing list/detail/trend responses only
- scenario selection must encode identifiers through existing client helpers
- show outcome and reason codes without converting regressions into passes
- show baseline and best-known evidence roles when present
- evidence links expose only bounded artifact IDs/hashes/metadata already returned by the API
- handle empty history, one-point history, missing baseline, missing best-known, incomparable, quality failure, regression, and pass
- cap rendered history at the server-provided bounded result

**Verification:** Component/unit tests for all states above; Dashboard lint, typecheck, production build, static export; applicable full stack and handoff checks; browser DOM smoke when available.

**Compatibility:** Existing Dashboard routes and scorecard views remain functional; no API or storage migration.

**Rollback:** Revert the packet PR; no stored state cleanup.

**Stop triggers:** Required fields are missing from the existing API contract; UI requires new authority or persistence; current Dashboard ownership conflicts with the module map.

### Packet PE1-CLOSE-1 — PE-1 acceptance seal

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE1-UI-1 complete.

**Goal:** Prove every PE-1 acceptance item is implemented and tested, close stale wording, and mark PE-1 complete without adding new capability.

**Allowed changes:** Tests, fixtures, active-doc corrections, and narrow defects found by the audit.

**Forbidden changes:** No CI blocking or mutation authority; no new product feature; no reopening completed phase ladders.

**Acceptance:** Deterministic recomputation; tamper, threshold, quality-failure, missing-baseline, missing-best-known, incomparable, repeat-import, cross-version, trend, API/SDK, and Dashboard evidence all pass. SQLite/PostgreSQL remain aligned. Full CI is green.

**Rollback:** Revert closeout corrections individually; implemented PE-1 functionality remains usable.

**Stop triggers:** Any acceptance gap requires a new behavior contract rather than a bounded defect repair; mark `PLANNER_DECISION_REQUIRED` with evidence.

## PE-2 — Budget Intelligence and Anomaly Auto-Pause

Stage invariants:

- forecasts and anomalies are derived evidence, not facts
- every result records version, window, coverage, confidence, reason codes, and evidence references
- sparse or incomplete data returns `insufficient_evidence`
- automatic pause is allowed only through existing pause/audit controls when policy-enabled and high-confidence
- no automatic termination, silent budget mutation, provider substitution, or opaque forecast

### Packet PE2-CONTRACT-1 — Budget intelligence evidence contract

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE1-CLOSE-1 complete.

**Goal:** Define and validate versioned bounded contracts for forecasts, anomaly findings, confidence, coverage, reason codes, and `insufficient_evidence` outcomes; report-only only.

**Owning paths:** `engine/src/budget_manager.rs`; existing provider audit/cost evidence; scheduler/workflow evidence; `LocalProductStore` validation owners; focused Rust tests; architecture/module docs when durable ownership changes.

**Allowed changes:** Contract structs/validators and deterministic fixtures/tests. Reuse existing evidence owners.

**Forbidden changes:** No pause call, policy mutation, new provider invocation, new pricing source, or Dashboard work.

**Acceptance:** Bounds, canonical serialization/hash where existing evidence uses hashes, malformed/tampered input rejection, sparse data, incomplete pricing, unknown model, clock/window boundaries, and SQLite/PostgreSQL-compatible representation are tested.

**Stop triggers:** Existing cost/audit evidence cannot identify the required dimensions without a new source-of-truth decision.

### Packet PE2-FORECAST-1 — Deterministic budget forecasts

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE2-CONTRACT-1 complete.

**Goal:** Produce read-only expected tokens/spend and exhaustion-time forecasts by run, workspace, provider, and model from existing posted evidence.

**Allowed changes:** Deterministic forecast computation, bounded aggregation, read-model tests.

**Forbidden changes:** No external model call, learned opaque model, budget mutation, reservation change, or pause.

**Contract:** Separate observed values from estimates; include sample count, window, coverage, confidence, pricing completeness, assumptions, and reason codes. Refuse a forecast when evidence is stale, sparse, contradictory, or unpriced.

**Acceptance:** Sparse, zero-usage, bursty, mixed-model, incomplete-pricing, boundary-time, deterministic ordering, and concurrency-safe read tests.

### Packet PE2-ANOMALY-1 — Explainable anomaly detector

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE2-FORECAST-1 complete.

**Goal:** Detect bounded cost, token, retry, latency, context-growth, and model-mix anomalies with explicit thresholds, confidence, and reason codes.

**Allowed changes:** Deterministic/statistical rule implementation over the versioned contract and existing evidence.

**Forbidden changes:** No hidden adaptive threshold, provider substitution, pause, termination, or policy mutation.

**Acceptance:** Normal, spike, gradual drift, mixed workloads, sparse history, false-positive boundaries, duplicated evidence, out-of-order evidence, and deterministic recomputation tests.

### Packet PE2-READ-1 — Persistence, API, SDK, and Dashboard read surfaces

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE2-ANOMALY-1 complete.

**Goal:** Expose bounded forecasts and anomaly evidence through existing store/API/SDK/Dashboard owners.

**Allowed changes:** Additive `LocalProductStore` schema only if persistence is required by the contract; read-only HTTP/OpenAPI; Python/TypeScript readers; current Dashboard components; SQLite/PostgreSQL migrations/tests.

**Forbidden changes:** No pause action or second state model.

**Acceptance:** Idempotent persistence if used, bounded pagination, permissions, OpenAPI/router parity, encoded SDK paths, empty/error/sparse UI states, migration compatibility, and full stack verification.

**Stop triggers:** Persistence semantics were not fixed by PE2-CONTRACT-1 or require rewriting existing budget history.

### Packet PE2-PAUSE-1 — Policy-gated high-confidence auto-pause

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE2-READ-1 complete.

**Goal:** Invoke the existing pause mechanism only for policy-enabled, high-confidence supported findings, with complete audit and idempotent recovery behavior.

**Owning paths:** Existing scheduler/workflow pause controls, policy gates, audit, operator evidence, API controls, and focused integration tests.

**Allowed changes:** A narrow decision adapter from validated anomaly evidence to the existing pause endpoint/path; explicit enablement; audit; resume/override evidence.

**Forbidden changes:** No auto-kill, silent budget edit, provider/model substitution, new pause state machine, default enablement, or resume without operator evidence.

**Contract:** Fail closed on missing policy, incomplete pricing, low confidence, stale evidence, unsupported anomaly, audit failure, concurrent duplicate triggers, or unavailable pause owner. Repeated triggers are idempotent. Resume and override preserve cause/evidence.

**Acceptance:** False positives, concurrency, duplicate trigger, audit failure, pause failure compensation, disabled policy, sparse data, incomplete pricing, resume, override, permission, and restart tests.

**Stop triggers:** Existing pause semantics cannot guarantee idempotency/compensation or policy ownership is ambiguous.

### Packet PE2-CLOSE-1 — PE-2 acceptance seal

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE2-PAUSE-1 complete.

**Goal:** Audit the whole PE-2 chain, repair bounded defects, mark PE-2 complete, and activate PE-3-CONTRACT-1.

**Acceptance:** Forecast/anomaly evidence is explainable and bounded; auto-pause is explicitly enabled, high-confidence, audited, idempotent, fail-closed, and recoverable; full CI and SQLite/PostgreSQL checks are green.

## PE-3 — Operator Decision Center

Stage invariants:

- one derived action queue over existing evidence and controls
- no second scheduler, policy authority, approval store, or hidden mutation path
- each item includes reason, severity, confidence, evidence links, recommended action, required authority, age, and resolution state

### Packet PE3-CONTRACT-1 — Decision item and source contract

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE2-CLOSE-1 complete.

**Goal:** Define bounded versioned action-item/source contracts and deterministic source precedence for approvals, blocked/stalled runs, repeated failures, budget risk, benchmark regressions, invalid policy, scheduler state, and rollback candidates.

**Allowed changes:** Contracts, validators, fixtures, reason/severity/confidence taxonomy, and tests.

**Forbidden changes:** No queue persistence, action execution, new authority, or Dashboard.

**Acceptance:** Missing/stale/conflicting sources, permissions metadata, deduplication identity, and evidence-link bounds are tested.

### Packet PE3-QUEUE-1 — Deterministic derived action queue

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE3-CONTRACT-1 complete.

**Goal:** Derive a prioritized, paginated queue with deterministic ordering, deduplication, age, stale invalidation, and resolution projection from existing owners.

**Forbidden changes:** No hidden writes or second state machine. Resolution must map to an existing authoritative fact or a narrowly defined existing-store record fixed in the contract.

**Acceptance:** Duplicate sources, priority ties, stale evidence, resolved/reopened items, missing source, bounded pagination, permissions, and deterministic recomputation.

### Packet PE3-READ-1 — API, SDK, and Dashboard decision center

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE3-QUEUE-1 complete.

**Goal:** Expose the derived queue and evidence through existing read-only API/SDK/Dashboard owners.

**Acceptance:** Router/OpenAPI parity, permission tests, encoded SDK paths, pagination, empty/error/stale UI, evidence navigation, and no mutation path in read components.

### Packet PE3-ACTION-1 — Existing-control action adapters

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE3-READ-1 complete.

**Goal:** Let an authorized operator invoke only already-existing mutation endpoints from an action item, preserving each endpoint's permission, approval, audit, idempotency, and rollback behavior.

**Forbidden changes:** No generic execute-anything endpoint, implicit approval, hidden scheduler command, policy mutation, or new authority.

**Acceptance:** Required-authority mismatch, stale item, duplicate request, endpoint failure, audit failure, compensation, permission, and evidence-to-action traceability tests.

**Stop triggers:** A recommended action has no existing authoritative endpoint or requires altered approval semantics.

### Packet PE3-CLOSE-1 — PE-3 acceptance seal

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE3-ACTION-1 complete.

**Goal:** Audit deterministic derivation, deduplication, stale invalidation, permissions, pagination, resolution, and action traceability; then activate PE4-CONTRACT-1.

## PE-4 — Trace-backed Policy Replay

Stage invariants:

- observed and estimated outcomes remain separate
- recommendations are refused for sparse, stale, uncovered, or out-of-distribution evidence
- progression is offline replay, then shadow evaluation, then bounded canary
- reuse existing experiment, promotion, pause, and rollback controls

### Packet PE4-CONTRACT-1 — Calibration and coverage contract

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE3-CLOSE-1 complete and sufficient versioned trace evidence exists.

**Goal:** Define versioned calibration slices by task class, complexity, provider/model, and execution profile, including sample size, window, coverage, confidence, OOD, and refusal reason codes.

**Owning paths:** `engine/src/feedback/run_trace_recorder.rs`, `engine/src/feedback/policy_simulator.rs`, existing experiment evidence, focused tests.

**Forbidden changes:** No live influence, promotion, routing mutation, or heuristic removal outside supported slices.

**Acceptance:** Sparse/stale/OOD/contradictory trace behavior, leakage prevention, deterministic partitions, and observed-vs-estimated separation.

### Packet PE4-REPLAY-1 — Offline trace-backed policy replay

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE4-CONTRACT-1 complete.

**Goal:** Compare candidate policies offline on success, quality, cost, latency, retries, and review outcomes for covered slices; refuse unsupported recommendations.

**Allowed changes:** Incremental replacement of fixed heuristic estimates only where coverage passes; retain explicit fallback/refusal elsewhere.

**Acceptance:** Golden replay, coverage boundary, OOD, stale trace, policy-version mismatch, deterministic result, and no-live-influence tests.

### Packet PE4-SHADOW-1 — Shadow policy evaluation

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE4-REPLAY-1 complete.

**Goal:** Run candidate policy decisions in shadow through existing experiment/evidence owners without changing live routing.

**Acceptance:** Live/shadow separation, correlation, bounded evidence, failure isolation, restart, and operator visibility tests.

### Packet PE4-CANARY-1 — Guarded canary and promotion integration

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE4-SHADOW-1 complete with explicit threshold evidence.

**Goal:** Reuse existing canary, promotion snapshot, pause, and rollback paths for a bounded candidate policy only after offline and shadow gates pass.

**Forbidden changes:** No automatic broad rollout, bypassed approval, missing rollback snapshot, or promotion on insufficient/OOD evidence.

**Acceptance:** Threshold pass/fail, concurrent promotion, pause, rollback, snapshot integrity, stale evidence, permission, and restart tests.

**Stop triggers:** Existing promotion/rollback cannot represent the candidate safely or threshold policy is not specified.

### Packet PE4-CLOSE-1 — PE-4 acceptance seal

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE4-CANARY-1 complete.

**Goal:** Prove the offline-to-shadow-to-canary chain and activate the next eligible stage packet.

## PE-5 — Release Provenance

Stage invariants:

- signing material remains outside the repository
- provenance binds source commit, workflow, target, dependency state, and artifact digest
- installer/upgrade verification fails closed while preserving atomic rollback
- current audits and target-correct packaging remain mandatory

### Packet PE5-SBOM-1 — Deterministic SBOM generation

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE1-CLOSE-1 complete and no conflicting release PR is active; user may explicitly activate this lane.

**Goal:** Produce bounded SPDX or CycloneDX SBOMs for release binaries and container images through existing release workflows.

**Owning paths:** `.github/workflows/release.yml`, release/container scripts, release-contract checks.

**Forbidden changes:** No release publication, persistent signing key, disabled dependency audit, or target relabeling.

**Acceptance:** Deterministic content where practical, target/dependency correctness, missing component failure, tamper tests, and dry-run artifacts.

### Packet PE5-ATTEST-1 — Build provenance attestations

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE5-SBOM-1 complete.

**Goal:** Generate provenance tying source, workflow, target, dependencies/SBOM, and final digests.

**Acceptance:** Wrong commit/target/digest/workflow rejection, reproducible verification fixture, and no secret material.

### Packet PE5-SIGN-1 — Artifact and image signing

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE5-ATTEST-1 complete.

**Goal:** Sign release artifacts/images through external or ephemeral CI identity and verify signatures without storing private signing material in the repository.

**Acceptance:** Valid, missing, wrong identity, wrong target, expired/revoked where supported, and tampered signature tests. CI tests use disposable fixtures only.

**Stop triggers:** Trust root, identity policy, or key custody is unspecified.

### Packet PE5-INSTALL-1 — Installer and upgrade provenance enforcement

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE5-SIGN-1 complete.

**Goal:** Require the configured SBOM/provenance/signature evidence before install/upgrade, and preserve existing atomic rollback on verification or activation failure.

**Acceptance:** Missing/tampered/mismatched evidence, network interruption, partial download, activation failure, prior-version restoration, and target correctness tests.

### Packet PE5-CLOSE-1 — PE-5 acceptance seal

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE5-INSTALL-1 complete.

**Goal:** Audit provenance from build to install and mark PE-5 complete without publishing an external release unless separately authorized.

## PE-6 — Fault Injection and Recovery Drills

Stage invariants:

- drills are bounded, deterministic, local/CI-safe, and isolated from real external state
- each drill fixes permitted data loss, authority behavior, fail-closed expectation, recovery sequence, cleanup, and required evidence before implementation
- reports record recovery success, divergence, duplicate execution, data loss, fail-open violations, and recovery time

### Packet PE6-INVARIANTS-1 — Recovery invariant catalog and harness contract

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE4-CLOSE-1 complete; PE5-CLOSE-1 complete for release/upgrade drills.

**Goal:** Encode bounded versioned drill/invariant/report contracts and reusable local fault seams without executing destructive external operations.

**Owning paths:** focused engine integration tests, storage/provider/scheduler fault seams, backup/restore, upgrade rollback, and CI tooling.

**Forbidden changes:** No production fault toggle exposed remotely, no real provider sabotage, no destructive shared database test, and no unbounded retry.

**Acceptance:** Contract validation, cleanup enforcement, timeout, fail-closed default, deterministic report, and test-environment isolation.

### Packet PE6-STORAGE-1 — Audit, database, and artifact recovery drills

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE6-INVARIANTS-1 complete.

**Goal:** Test audit-store failure, database interruption/restart, bounded state loss, artifact corruption, backup/restore, and integrity recovery.

**Acceptance:** SQLite and PostgreSQL scenarios, no fail-open authority, permitted data-loss assertion, duplicate detection, cleanup, and recovery-time evidence.

### Packet PE6-RUNTIME-1 — Provider, budget, scheduler, and mailbox drills

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE6-STORAGE-1 complete.

**Goal:** Test provider timeout/invalid response, budget concurrency, scheduler restart, pause recovery, and duplicate mailbox delivery through existing seams.

**Acceptance:** No double execution/charge, preserved audit, fail-closed gates, idempotent recovery, bounded retries, and cleanup.

### Packet PE6-RELEASE-1 — Upgrade and provenance failure drills

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE6-RUNTIME-1 and PE5-CLOSE-1 complete.

**Goal:** Test interrupted upgrade, invalid provenance/signature, corrupted artifact, activation failure, and restoration of the previous verified version.

**Acceptance:** Atomic rollback, target correctness, no partial active installation, retained evidence, and cleanup.

### Packet PE6-OPS-1 — Drill reporting and proven runbook procedures

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE6-RELEASE-1 complete.

**Goal:** Expose bounded drill reports through existing operator evidence/read surfaces and add only successfully tested operator procedures to `docs/RUNBOOK.md`.

**Forbidden changes:** No new operational fact store or untested runbook claim.

### Packet PE6-CLOSE-1 — Final evolution-plan acceptance seal

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE6-OPS-1 complete.

**Goal:** Audit PE-1 through PE-6 boundaries, evidence, CI, rollback, active docs, and remaining risks; mark the plan complete without inventing PE-7.

## Cross-Stage Rules

- Reuse existing `LocalProductStore`, workflow, scheduler, provider, feedback, operator-evidence, Dashboard, SDK, release, and recovery owners.
- Do not add a second scheduler, graph kernel, mailbox, storage layer, policy authority, artifact truth source, or Dashboard state model.
- Use bounded versioned contracts and deterministic recomputation where practical.
- Maintain SQLite/PostgreSQL compatibility when storage changes.
- Separate facts, estimates, recommendations, and live actions.
- Start with observation before granting new mutation authority.
- Provider calls remain forbidden in CI.
- Do not persist raw prompts, outputs, transcripts, secrets, repository payloads, or other sensitive runtime content in evidence.
- Include compatibility, residual risk, and rollback in every PR.
- One active product packet at a time unless the user explicitly activates the independent PE-5 lane.

## Hard Stops

Agents must not commit credentials, falsify evidence, hide failures, remove recovery paths, bypass auth/budget/audit/approval controls, create unbounded execution, persist raw sensitive runtime content, or perform irreversible external destruction without recovery.

Additional Terra execution hard stops:

- no `READY_FOR_TERRA` packet with satisfied prerequisites
- current model profile is not Terra Medium
- packet conflicts with code or another authoritative document
- an unspecified architecture, authority, schema, migration, security, trust, signing, or recovery decision is required
- two coherent repair cycles fail on the same root cause

## Auto-Merge Policy

Scoped docs, tests, fixes, implementation, migration, security, release, and authority changes may be merged autonomously when the packet is ready, scope/risk are reviewed, all CI is green, the handoff guard passes, evidence is truthful, and rollback is clear. External release publication or irreversible effects require separate verified authority.

## Minimum Verification

Run focused checks plus applicable full repository validation:

```bash
cargo fmt --all -- --check
cargo clippy -p engine --all-targets --all-features -- -D warnings
cargo test -p engine
cargo test -p engine --features pg-tests -- --test-threads=1
PYTHONPATH=src uv run --no-project python -m unittest discover -s tests
bash scripts/verify_rust_typescript_stack.sh
bash scripts/check_wire_codegen_drift.sh
uv run --no-project python tools/check_security_baseline.py
uv run --no-project python scripts/check_agent_handoff.py
git diff --check
```

Add release, browser, Docker, migration, backup/restore, or fault-specific checks when those surfaces change.

## Documentation Maintenance

Use existing active docs only. Put durable architecture in `ARCHITECTURE_BOOK.md`, current facts in `CURRENT_STATUS.md`, forward authority and packets here, ownership in `MODULE_MAP.md`, validation discipline in `REAL_WORLD_TESTING_PLAYBOOK.md`, and proven operator procedures in `RUNBOOK.md`.

## Before Starting Autonomous Work

1. Confirm Terra Medium execution profile.
2. Read `AGENTS.md`, `docs/CURRENT_STATUS.md`, this file, and `docs/MODULE_MAP.md`.
3. Select the earliest `READY_FOR_TERRA` packet whose prerequisites are complete.
4. Audit current code before assuming functionality is absent.
5. Restate packet scope, non-goals, ownership, and stop triggers.
6. Implement on a branch, test, review, and repair ordinary CI failures for at most two coherent cycles.
7. Update active docs with facts only.
8. Report packet, model profile, PR, commits, compatibility, evidence, CI, residual risk, rollback, and next packet state.