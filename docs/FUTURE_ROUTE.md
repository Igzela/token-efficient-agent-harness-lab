# Future Route

Last updated: 2026-08-16.

This document is the sole long-horizon routing index. Every packet here is `BLOCKED_PREREQUISITE` and routing-only: its order, prerequisite, intended class, bounded sketch, and promotion profile are accepted planning context, not implementation or external-effect authority.

The current executable window and common execution contracts live in `docs/NEXT_DECISION.md`. Accepted truth lives in `docs/CURRENT_STATUS.md`; durable architecture and authority invariants live in `docs/ARCHITECTURE_BOOK.md`; current owners live in `docs/MODULE_MAP.md`; live PR, CI, and review facts come only from a fresh context capsule.

A promotion profile never authorizes execution. It exists so the planning owner can promote one packet at low cost when its accepted predecessor closes: it pre-defines the bounded facts that are stable today and marks every fact that must be refreshed against then-current `main` (`REFRESH_AT_PROMOTION`). Promotion still removes exactly one eligible packet from this index and expands its complete twelve-field contract in `docs/NEXT_DECISION.md`, with an independently reviewed routing diff. Any unresolved value or duplicate packet identity is `DECISION_REQUIRED`.

## Worker Tiers

| Tier | Work a fast/cheap agent may own | Required escalation |
|---|---|---|
| `T0` | Read-only inventory, CodeGraph call paths, deterministic matrices, negative search, digest comparison | T2 resolves omissions, ownership conflicts, and contract choices |
| `T1` | Exact mechanical implementation, migration, codegen, focused tests, docs, and presentation under a frozen contract | T2 owns schema/authority/evaluator/concurrency/recovery decisions and accepts the complete diff |
| `T2` | Primary planning/architecture, statistical and evaluator contracts, store/transaction/controller/recovery seams, independent closeout | T3 is still required for external effects, spend, human GO/NO-GO, adoption, release, or deployment |
| `T3` | Human/operator authority: finite Provider/training/target effects and explicit decisions | Never delegated to model output; one receipt authorizes only its exact named action |

A packet's tier is the highest tier that owns its decisive step. Lower-tier agents still perform all preceding deterministic preparation and hand the exact pause receipt to that owner. The portfolio manifest below assigns each packet its intended tier as a promotion-time candidate.

## Known Planned-Seam Gaps

These are audited absences or unaccepted seams at the current main, not permission to fill them now. Every listed gap is rechecked when its earliest contract packet is promoted; a changed owner or a need for parallel authority is `DECISION_REQUIRED`.

| Route family | Accepted owner to extend | Missing or not-yet-accepted seam | Promotion stop condition |
|---|---|---|---|
| RWE measurement/decision baseline | existing RWE corpus, protocol, schedule, artifact, usage, review, and evidence owners | no accepted decision-grade corpus | any required field lacks a named existing owner, explicit unavailable state, or fail-closed stop rule |
| AC1 process supervision | AC0-enumerated subprocess/executor callers | no `ProcessSupervisor` module/interface exists today | placement would create a second runtime/executor owner or child cleanup cannot be proved |
| AC2 typed execution | current executor adapter/node/provider/CLI owners | AC2 contract and additive boundary core are accepted; caller-wide migration remains unaccepted | any caller has unclassified semantics, loses usage uncertainty, or coerces outcome-unknown into success |
| AC3 Golden Path split | `product_golden_path.rs` plus LocalProductStore product-task authority | no accepted pure orchestration/effect-port seam | store, approval, output, audit, or external effect ownership becomes ambiguous |
| AC4 transaction views | sole LocalProductStore SQLite/PostgreSQL owners | no named borrowed transaction-view interfaces exist | atomic group, borrow/commit/rollback, parity, or recovery cannot be proved |
| AC5 composition root | current Rust startup, config, HTTP state, provider, and store construction | no single accepted validated composition-root contract | config precedence conflicts, dependency cycle, or secret resolution broadens |
| Context working-set projection | existing Repository Context Control Plane plus current prompt/runtime, artifact/evidence, execution-usage, and scorecard owners; then-current accepted license/SBOM/NOTICE owners for any later third-party provenance | no accepted model-visible evidence working-set/residency/rehydration contract; context is assembled through role/capsule/prompt paths and provider prompts without one canonical projection seam; no accepted source-reuse/transplant disposition | projection would become a second memory/store/authority/evaluator, semantic relevance could evict authority/blocker evidence, a reduced item cannot be exact-source-bound and safely rehydrated, license/provenance is unclear or required attribution cannot be retained, a transplant would introduce a second runtime/store/provider/session owner, or upstream semantics cannot map losslessly onto this repository's contract |
| EC1 causal mutation evidence | existing HE artifact/store owner | `FailurePatternEvidenceV1`, `MutationHypothesisManifestV1`, and `PredictionOutcomeV1` are planned, not current accepted types | identity/causal source can be caller- or candidate-authored, mutable, or unaddressable |
| EC2 real evaluator/holdout | accepted evaluator/evidence owner; current `harness_evolution_eval.rs` and `LocalProductStore` remain default-off owners | EC2 evaluator/holdout contract is accepted; managed holdout seal, access mediation, sentinel wiring, and real acceptance evidence remain unimplemented successors | fixture result, candidate-controlled rule, leakage, or label uncertainty would be reported as acceptance |
| EC3-EC5 controls | existing budget/spend, HE, evaluator, artifact, lease, and store owners | no accepted total-lifecycle enforcement, diversity admission, immutable Pareto archive, or HE stop/recovery state machine | a second ledger/controller appears or crash/exactly-once/hidden-reject behavior is unresolved |
| HE memory/skill factor | existing HE experimental artifact/store owner | no HE projection adapter/authority exists | product `durable_memory.rs`, global skills, or summaries would become experiment authority |
| Level-2 | existing HE/scheduler/evaluator/budget/store owners | no bounded Level-2 controller exists | `recursive_execution.rs` task-tree recursion is proposed as controller, or evaluator/budget/stops become mutable |
| Meta/R4 | existing EC/Level-2 owners | no accepted fixed O0/O1 or metacognitive operator adapter | treatment cannot be isolated under an immutable outer shell or can rewrite its contract |
| R5 training | existing artifact/store/evaluator boundaries only | no accepted trainer, external-training adapter, checkpoint owner, or training-effect authority | data rights, base/adapter identity, compute, retry, deletion, or verifier separation is unresolved |
| R6 outer policy | existing controllers/evidence owners | no accepted mapping for parent, lever, or curriculum policy; only one family may be chosen | action space includes evaluator, goals, safety, permissions, budgets, or unbounded recursive depth |
| Dashboard | current Dashboard presentation/data consumers | AC6 data/schema migration and final presentation refresh are distinct; neither is accepted yet | final UI asks for backend/schema authority or optional R4-R6 work is treated as a blocker |

## Promotion Profile Contract

Each packet combines its existing `Prerequisite`, `Class`, `Outcome`, `Allowed delta`, `Exit`, and `Stop` with a compact machine-readable profile row in the Portfolio Inventory Manifest below: intended worker tier, risk class, and likely verification family. The manifest row is the promotion-time candidate, never present edit authority.

The following facts are deliberately **not** frozen in profiles because they depend on then-current accepted `main`; they must be re-derived at promotion time and are recorded only as `REFRESH_AT_PROMOTION`:

- exact current-main owner and implementation paths (the packet's `Owner/seam` and allowed-path closure);
- ordered implementation steps and precise verification commands;
- schema, evaluator, authority, spend, provider/model, and live-experiment settings;
- rollback detail, retention, and evidence destinations.

A profile fact that is not `REFRESH_AT_PROMOTION` (packet identity, prerequisite shape, class, worker tier, risk class, verification family, exit, stop) still binds promotion: it may change only through the same independently reviewed routing diff that promotes the packet. `EFFECT` packets always keep worker tier `T3` and risk class `external_effect`.

The active packet named by `docs/NEXT_DECISION.md` is intentionally absent from this successor index until its closeout; its successors may name that current packet as a prerequisite. This keeps one lifecycle owner and avoids duplicating the active packet in the manifest.

Promotion procedure, exactly one packet at a time:

1. prove the accepted predecessor's receipt and disposition in `docs/CURRENT_STATUS.md`; a merely closed PR or nominally completed packet is insufficient;
2. refresh remote `main` and rerun owner/caller inventory; replace globs and planned-seam references with exact files;
3. expand the complete twelve-field contract in `docs/NEXT_DECISION.md`, resolving every `REFRESH_AT_PROMOTION` fact from then-current `main`; if a value still has no accepted owner, stop `DECISION_REQUIRED`;
4. author the bounded autonomous worker dispatch capsule inside the promoted packet block (legacy machine identifier: `weak-agent-dispatch:v1`; allowed paths, ordered steps, verification, rollback, forbidden next actions, external-effect limit);
5. remove the packet from this index and refresh this manifest;
6. have the routing change independently reviewed; a profile alone never starts a coding session.

## Stop and Resume Protocol

On any stop, the worker first prevents new effects, preserves store leases/receipts and restricted evidence, runs safe cleanup/compensation already owned by the active packet, and emits the bounded handoff with `state=DECISION_REQUIRED`, `BLOCKED_PREREQUISITE`, or `OUTCOME_UNKNOWN`. It must state whether an effect may already have occurred; unknown is never rewritten as zero. Resume must refresh main and external state, verify the same packet/evidence/authority identities and rollback state, and continue only from the named `next_permitted_action`. A stale receipt, changed contract, expired authority, missing evidence, or changed effect status requires a new planning/operator decision; never replay the prior command speculatively.

## Portfolio Inventory Manifest

The checked manifest below binds the complete ordered packet ID list, class/tier/risk counts, dependency graph, and the per-packet promotion profile rows. Any addition, removal, reorder, dependency change, profile edit, or generic-content replacement must deliberately refresh this manifest and appear as an independently reviewed planning diff; `scripts/check_agent_handoff.py` rejects silent drift.

The manifest rows are compact arrays of `packet_id`, `class`, `worker_tier`, `risk_class`, and `verification_family`. `execution_profile` is derived as `{packet_id}.v1`. `promotion_requirements` is the shared contract above plus `REFRESH_AT_PROMOTION` for owners, allowed paths, ordered steps, verification, and rollback.

<!-- future-route-inventory:v1
{"dependency_graph_sha256": "5de2ad19e42e1ba90f7dd8763e934af28e952c74973027915f7bcf4b143879ac", "ordered_packet_ids": ["PE7-HE-EC2-HOLDOUT-SEAL-1", "PE7-HE-EC2-SENTINEL-CONFORMANCE-1", "PE7-HE-EC2-PREDICTION-OUTCOME-1", "PE7-HE-EC3-CONTRACT-1", "PE7-HE-EC3-INSTRUMENTATION-1", "PE7-HE-EC3-ENFORCEMENT-1", "PE7-HE-EC4-CONTRACT-1", "PE7-HE-EC4-ADMISSION-1", "PE7-HE-EC4-COVERAGE-CLOSEOUT-1", "PE7-HE-EC5-CONTRACT-1", "PE7-HE-EC5-SELECTION-ARCHIVE-1", "PE7-HE-EC5-STOP-RECOVERY-1", "PE7-HE-LEVEL1-PREFLIGHT-1", "PE7-HE-LEVEL1-RUN-1", "PE7-HE-LEVEL1-CLOSEOUT-1", "PE7-HE-LEVEL1-TRANSFER-PROTOCOL-1", "PE7-HE-LEVEL1-TRANSFER-RUN-1", "PE7-HE-LEVEL1-TRANSFER-ANALYSIS-1", "PE7-MEMORY-SKILL-CONTRACT-1", "PE7-MEMORY-ADAPTER-1", "PE7-SKILL-ADAPTER-1", "PE7-MEMORY-SKILL-RUN-1", "PE7-MEMORY-SKILL-ANALYSIS-1", "PE7-HE-LEVEL2-RULE-AUDIT-1", "PE7-HE-LEVEL2-EVIDENCE-ANALYSIS-1", "PE7-HE-LEVEL2-DECISION-1", "PE7-HE-LEVEL2-CONTROLLER-CONTRACT-1", "PE7-HE-LEVEL2-STATE-PERSISTENCE-1", "PE7-HE-LEVEL2-GENERATION-ORCHESTRATION-1", "PE7-HE-LEVEL2-EVALUATION-SELECTION-1", "PE7-HE-LEVEL2-STOP-RECOVERY-1", "PE7-HE-LEVEL2-SIMULATION-1", "PE7-HE-LEVEL2-PILOT-1", "PE7-HE-LEVEL2-CLOSEOUT-1", "PE7-HE-FINAL-TRANSFER-PROTOCOL-1", "PE7-HE-FINAL-TRANSFER-RUN-1", "PE7-HE-FINAL-TRANSFER-ANALYSIS-1", "PE7-HE-ADOPTION-READINESS-1", "PE7-HE-ADOPTION-DECISION-1", "PE7-HE-META-CLAIM-PROTOCOL-1", "PE7-HE-META-OPERATOR-CONTRACT-1", "PE7-HE-META-CORPUS-EVALUATOR-1", "PE7-HE-META-BUDGET-CONTRACT-1", "PE7-HE-META-O0-BASELINE-1", "PE7-HE-META-O1-CANDIDATE-1", "PE7-HE-META-FIXTURE-PILOT-1", "PE7-HE-META-PILOT-CLOSEOUT-1", "PE7-HE-META-COMPARISON-RUN-1", "PE7-HE-META-REPLICATION-RUN-1", "PE7-HE-META-ANALYSIS-DECISION-1", "PE7-HE-ADVANCED-RECURSION-GATE-1", "PE7-HE-R4-METACOGNITIVE-CONTRACT-1", "PE7-HE-R4-METACOGNITIVE-ADAPTER-1", "PE7-HE-R4-COMPARISON-RUN-1", "PE7-HE-R4-REPLICATION-RUN-1", "PE7-HE-R4-ANALYSIS-DECISION-1", "PE7-HE-R5-WEIGHT-CONTRACT-1", "PE7-HE-R5-WEIGHT-ADAPTER-1", "PE7-HE-R5-FACTORIAL-RUN-1", "PE7-HE-R5-FACTORIAL-ANALYSIS-1", "PE7-HE-R5-COEVOLUTION-RUN-1", "PE7-HE-R5-TRANSFER-REPLICATION-1", "PE7-HE-R5-ANALYSIS-DECISION-1", "PE7-HE-R6-OUTER-POLICY-CONTRACT-1", "PE7-HE-R6-OUTER-POLICY-ADAPTER-1", "PE7-HE-R6-COMPARISON-RUN-1", "PE7-HE-R6-REPLICATION-RUN-1", "PE7-HE-R6-ANALYSIS-DECISION-1", "PE7-DASHBOARD-DISPOSITION-1", "PE7-DASHBOARD-REFRESH-1", "PE7-DASHBOARD-CLOSEOUT-1"], "ordered_packet_ids_sha256": "5a0b0157e74f1bc8ec7445833bfa1642f8b66784f55508ccb60361af9326e99f", "packet_count": 71, "profiles": [["PE7-HE-EC2-HOLDOUT-SEAL-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC2-SENTINEL-CONFORMANCE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC2-PREDICTION-OUTCOME-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC3-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-EC3-INSTRUMENTATION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC3-ENFORCEMENT-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC4-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-EC4-ADMISSION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC4-COVERAGE-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-EC5-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-EC5-SELECTION-ARCHIVE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC5-STOP-RECOVERY-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-LEVEL1-PREFLIGHT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-LEVEL1-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-LEVEL1-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-LEVEL1-TRANSFER-PROTOCOL-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-LEVEL1-TRANSFER-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-LEVEL1-TRANSFER-ANALYSIS-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-MEMORY-SKILL-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-MEMORY-ADAPTER-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-SKILL-ADAPTER-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-MEMORY-SKILL-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-MEMORY-SKILL-ANALYSIS-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-LEVEL2-RULE-AUDIT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-LEVEL2-EVIDENCE-ANALYSIS-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-LEVEL2-DECISION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-LEVEL2-CONTROLLER-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-LEVEL2-STATE-PERSISTENCE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-LEVEL2-GENERATION-ORCHESTRATION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-LEVEL2-EVALUATION-SELECTION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-LEVEL2-STOP-RECOVERY-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-LEVEL2-SIMULATION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-LEVEL2-PILOT-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-LEVEL2-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-FINAL-TRANSFER-PROTOCOL-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-FINAL-TRANSFER-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-FINAL-TRANSFER-ANALYSIS-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-ADOPTION-READINESS-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-ADOPTION-DECISION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-META-CLAIM-PROTOCOL-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-META-OPERATOR-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-META-CORPUS-EVALUATOR-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-META-BUDGET-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-META-O0-BASELINE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-META-O1-CANDIDATE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-META-FIXTURE-PILOT-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-META-PILOT-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-META-COMPARISON-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-META-REPLICATION-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-META-ANALYSIS-DECISION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-ADVANCED-RECURSION-GATE-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-R4-METACOGNITIVE-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-R4-METACOGNITIVE-ADAPTER-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-R4-COMPARISON-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R4-REPLICATION-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R4-ANALYSIS-DECISION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-R5-WEIGHT-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-R5-WEIGHT-ADAPTER-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-R5-FACTORIAL-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R5-FACTORIAL-ANALYSIS-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-R5-COEVOLUTION-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R5-TRANSFER-REPLICATION-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R5-ANALYSIS-DECISION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-R6-OUTER-POLICY-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-R6-OUTER-POLICY-ADAPTER-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-R6-COMPARISON-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R6-REPLICATION-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R6-ANALYSIS-DECISION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-DASHBOARD-DISPOSITION-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-DASHBOARD-REFRESH-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-DASHBOARD-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"]], "profiles_sha256": "cdbb4320fa7df6c5b685ddf84cd5a9ddc0d8da55ba4fba5c7f4b7b75625f18f2", "schema_version": "future_route_inventory.v1"}
-->

# Future Route

Last updated: 2026-08-15.

This document is the sole long-horizon routing index. Every packet here is `BLOCKED_PREREQUISITE` and routing-only: its order, prerequisite, intended class, and bounded sketch are accepted planning context, not implementation or external-effect authority.

The current executable window and common execution contracts live in `docs/NEXT_DECISION.md`. Accepted truth lives in `docs/CURRENT_STATUS.md`; durable architecture and authority invariants live in `docs/ARCHITECTURE_BOOK.md`; current owners live in `docs/MODULE_MAP.md`; live PR, CI, and review facts come only from a fresh context capsule.

When an accepted predecessor closes, do not execute its successor from this file. Refresh remote `main`, reconcile any negative or insufficient disposition, remove exactly one eligible packet from this index, and expand its complete twelve-field contract in `docs/NEXT_DECISION.md`. Any unresolved value or duplicate packet identity is `DECISION_REQUIRED`.

## Stage RWE v2 viability

Viability RUN evidence is accepted on main. The CLOSEOUT packet has left this index. These packets prove lifecycle viability only and do not authorize Architecture Convergence or an economic-improvement claim.


## Stage RWE measurement readiness

These packets make the later comparison decision-grade. All values are frozen before the decision-baseline outcomes are observed.


## Stage Decision-grade pre-AC baseline

This stage is parked. The observed DB run is not a decision-grade baseline and is not an AC prerequisite; any future comparison must be planned as a separate route.

## Stage Architecture Convergence AC0

AC0 inventories and freezes; it makes no production ownership move.

## Stage Architecture Convergence AC1 - process supervision

AC1 remains deferred optional hardening. It is not an executable successor and does not block the product Golden Path or AC2.

## Stage Architecture Convergence AC2 - typed execution

AC2 distinguishes effect and outcome states while leaving admission, leases, spend, verification, approval, output, and adoption with existing owners.

The AC2 contract, typed boundary repair, and enumerated caller migration are accepted; AC1 shared supervision remains deferred optional hardening. The AC3 Golden Path responsibility contract was promoted and its sketch removed from this index with the checked manifest refreshed by accepted PR #479; it is the current provider-free contract-freeze window in `docs/NEXT_DECISION.md`, and the remaining AC3 sketches below are its orchestrator-core and port-migration successors.

## Stage Architecture Convergence AC3 - Golden Path responsibility split

AC3 separates orchestration, store mutation, and external effects without changing state-machine or authority order.

### Packet PE7-HE-EC2-HOLDOUT-SEAL-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC2-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Materialize sealed holdout identities, labels, access mediation, audit, and invalidation controls.

**Allowed delta:** Access/seal/audit controls only; no candidate run or evaluator rule change.

**Exit:** Unauthorized-read, label-tamper, leakage, restart, audit, and deletion/rotation tests pass.

**Stop:** Raw sensitive content would be committed, candidate identity gains access, or seal cannot survive restart.
### Packet PE7-HE-EC2-SENTINEL-CONFORMANCE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC2-HOLDOUT-SEAL-1

**Class:** `IMPLEMENT`

**Outcome:** Wire safety, contamination, and evaluator-gaming sentinels into the existing evaluator path.

**Allowed delta:** Sentinel observation/invalidation only; no scalar override or new evaluator.

**Exit:** Adversarial fixtures prove each sentinel fails closed before Pareto selection and preserves complete rejected-candidate evidence.

**Stop:** A sentinel can be candidate-disabled, mutates labels, or turns uncertainty into pass.
### Packet PE7-HE-EC2-PREDICTION-OUTCOME-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC2-SENTINEL-CONFORMANCE-1

**Class:** `IMPLEMENT`

**Outcome:** Have the existing evaluator path emit immutable `PredictionOutcomeV1` records that compare each frozen fix/regression prediction with actual task, metric, invariant, and missingness evidence.

**Allowed delta:** Derived evaluation evidence and archive projection only; no candidate-authored outcome, evaluator change, selection weight, or safety inference.

**Exit:** Correct/incorrect/partially supported/contradicted/unavailable outcomes, unpredicted regression, incomplete evaluation, tamper, replay, parity, and calibration-summary fixtures pass.

**Stop:** A candidate can write its own outcome, absent regression prediction is treated as safety, prediction accuracy gates selection, or post-result edits can change the manifest.
## Stage Experiment control EC3 - total lifecycle budget

EC3 makes equal total lifecycle budget enforceable; token or call equality alone is insufficient.

### Packet PE7-HE-EC3-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC2-PREDICTION-OUTCOME-1

**Class:** `CONTRACT`

**Outcome:** Freeze lifecycle-cost ontology, trustworthy sources, missingness/eligibility rules, reservation/reconciliation, per-candidate/global envelopes, failure accounting, and the cost of diagnosis, hypothesis construction, prediction, and outcome reconciliation.

**Allowed delta:** No spend or runtime behavior change.

**Exit:** Versioned budget/accounting contract covering generation, evaluation, review, repair, CI, recovery, human effort, and failed attempts.

**Stop:** A material cost class is silently zero, source semantics are ambiguous, or contract creates a second spend owner.
### Packet PE7-HE-EC3-INSTRUMENTATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC3-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Capture and normalize the accepted lifecycle-cost evidence through existing usage/artifact/store owners.

**Allowed delta:** Observation and immutable evidence only; no admission decision yet.

**Exit:** Source/partial/unavailable semantics, failure-path cost retention, restart, and parity tests pass.

**Stop:** Instrumentation drops rejected/failed cost, guesses unavailable values, or exposes sensitive raw evidence.
### Packet PE7-HE-EC3-ENFORCEMENT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC3-INSTRUMENTATION-1

**Class:** `IMPLEMENT`

**Outcome:** Enforce equal candidate/global lifecycle envelopes using existing admission/spend owners and deterministic reconciliation.

**Allowed delta:** Budget admission/stop only under the frozen contract; no evaluator or selection authority.

**Exit:** Overrun, concurrent claim, crash, cancellation, unknown actual cost, and exact-once reconciliation tests pass.

**Stop:** Enforcement double-spends, retries unknown effects, or cannot keep arms comparable.
## Stage Experiment control EC4 - diversity and exploration

EC4 detects duplicates and exploration collapse without treating novelty as authority.

### Packet PE7-HE-EC4-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC3-ENFORCEMENT-1

**Class:** `CONTRACT`

**Outcome:** Freeze exact duplicate and near-duplicate definitions, distance features, family/parent/seed coverage, collapse thresholds, and reporting.

**Allowed delta:** No candidate generation or admission change.

**Exit:** Versioned diversity contract with deterministic thresholds, calibration source, false-positive handling, and no production-authority claim.

**Stop:** Metric depends on sealed outcomes, can be candidate-gamed without sentinel, or lacks deterministic replay.
### Packet PE7-HE-EC4-ADMISSION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC4-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement duplicate/near-duplicate admission and immutable distance evidence.

**Allowed delta:** Diversity admission only; hard safety/quality gates remain separate and prior.

**Exit:** Exact/near duplicate, collision, order, restart, lineage, and rejected-candidate preservation tests pass.

**Stop:** Admissibility becomes a quality score, evidence is nondeterministic, or rejected work disappears.
### Packet PE7-HE-EC4-COVERAGE-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC4-ADMISSION-1

**Class:** `CLOSEOUT`

**Outcome:** Validate family/parent/seed exploration coverage and collapse sentinel behavior on provider-free fixtures.

**Allowed delta:** Evidence/threshold conformance only; no live experiment.

**Exit:** Coverage matrix, collapse triggers, replay determinism, and reporting completeness accepted for EC5.

**Stop:** Fixtures cannot distinguish exploration from superficial textual variation or thresholds require post-result tuning.
## Stage Experiment control EC5 - Pareto, stop, recovery

EC5 freezes hard-gate-first selection and the generation state machine before Level-1.

### Packet PE7-HE-EC5-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC4-COVERAGE-CLOSEOUT-1

**Class:** `CONTRACT`

**Outcome:** Freeze hard-gate order, Pareto objectives, dominance/ties/disagreement, archive semantics, saturation/contamination/gaming/regression/budget/diversity stops, and recovery invariants.

**Allowed delta:** No selection engine or generation execution.

**Exit:** Exact selection/stop/recovery state-transition contract and Level-1 experiment envelope.

**Stop:** A scalar can override a hard gate, objective value bases are incomparable, or restart semantics are ambiguous.
### Packet PE7-HE-EC5-SELECTION-ARCHIVE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC5-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement hard-gate filtering, Pareto comparison, tie/disagreement handling, and an immutable candidate archive retaining causal manifests, counterevidence, and prediction outcomes.

**Allowed delta:** Selection evidence only; no active-Harness replacement or production adoption.

**Exit:** Dominance, incomparable basis, tie, rejection, archive tamper, and full-cost fixtures pass.

**Stop:** Best-only reporting, scalar override, candidate-controlled metric, hidden rejection, or prediction accuracy becoming selection authority becomes possible.
### Packet PE7-HE-EC5-STOP-RECOVERY-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC5-SELECTION-ARCHIVE-1

**Class:** `IMPLEMENT`

**Outcome:** Implement bounded stop, lease, cancellation, restart, exactly-once, and recovery transitions; freeze the exact Level-1 runnable contract.

**Allowed delta:** Laboratory control state only; no Provider call or Level-2 loop.

**Exit:** Crash/concurrency/late-write/stop/replay tests and SQLite/PostgreSQL parity pass; Level-1 contract is hash-bound.

**Stop:** A restart can repeat an effect, budget resets, evaluator changes, or a stopped run can resume without authority.
## Stage Level-1 core

Level-1 runs one generation with memory and skill projections disabled.

### Packet PE7-HE-LEVEL1-PREFLIGHT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC5-STOP-RECOVERY-1

**Class:** `CONTRACT`

**Outcome:** Freeze active Harness, parents, mutation families, causal-manifest identities, seeds, candidate limits, full budgets, evaluator/holdout identities, prediction-outcome rules, authorization package, and immediate preflight.

**Allowed delta:** No candidate generation or holdout access.

**Exit:** Zero-mismatch preflight and one finite experiment authorization request; every identity matches EC1-EC5.

**Stop:** Any mutable/unbound experiment field, stale seal, insufficient capacity, or missing rollback/evidence destination.
### Packet PE7-HE-LEVEL1-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL1-PREFLIGHT-1

**Class:** `EFFECT`

**Outcome:** Execute exactly one bounded generation through candidate creation, diversity admission, full-cost evaluation, hard gates, sealed holdout, and archive.

**Allowed delta:** Registered laboratory effects only; no memory/skill projection, active-Harness adoption, retuning, or second generation.

**Exit:** Every candidate including rejects has terminal lineage, failure-pattern evidence, frozen hypothesis, prediction outcome, cost, evaluator/sentinel, diversity, holdout, archive, cleanup, and restricted evidence.

**Stop:** Any EC stop rule, authority/lease mismatch, contamination, evaluator mutation, budget breach, outcome unknown, or hidden candidate.
### Packet PE7-HE-LEVEL1-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL1-RUN-1

**Class:** `CLOSEOUT`

**Outcome:** Recompute hard gates/Pareto results and prediction calibration, select at most one experimental transfer candidate, and emit PR_READY evidence only.

**Allowed delta:** Analysis/artifact/status only; active Harness remains immutable.

**Exit:** Independent receipt with selected/no-selected disposition, complete archive/cost evidence, attribution limits, and transfer eligibility.

**Stop:** Selection cannot be reproduced, any hard gate failed, evidence is incomplete, or result depends on memory/skill.
## Stage Level-1 transfer pilot

Transfer is separate from development-set selection and grants no production authority.

### Packet PE7-HE-LEVEL1-TRANSFER-PROTOCOL-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL1-CLOSEOUT-1

**Class:** `CONTRACT`

**Outcome:** Seal unseen tasks/task families and, where practical, repository/model/environment strata; freeze baselines, evaluator, budgets, drift, contamination, and decision rules.

**Allowed delta:** No transfer execution or candidate change.

**Exit:** Hash-bound transfer protocol/corpus and zero-mismatch preflight/authorization package.

**Stop:** Candidate or generator influenced the unseen set, strata are not truly unseen, or comparable value semantics are absent.
### Packet PE7-HE-LEVEL1-TRANSFER-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL1-TRANSFER-PROTOCOL-1

**Class:** `EFFECT`

**Outcome:** Execute the selected experimental candidate and frozen baselines on the sealed transfer set.

**Allowed delta:** Registered effects only; no repair or retraining on transfer outcomes.

**Exit:** Complete blinded results, failures, lifecycle cost, drift, cleanup, and evidence for all arms/tasks.

**Stop:** Contamination, evaluator drift, authority failure, outcome unknown, or global transfer stop.
### Packet PE7-HE-LEVEL1-TRANSFER-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL1-TRANSFER-RUN-1

**Class:** `CLOSEOUT`

**Outcome:** Apply the frozen transfer analysis and determine whether evidence is eligible for Level-2 consideration.

**Allowed delta:** Analysis only; development gains cannot offset transfer regression.

**Exit:** Independent GO_ELIGIBLE, NO_GO, or INSUFFICIENT receipt with uncertainty, value-basis, reliability, cost, and drift limitations.

**Stop:** Transfer non-inferiority/hard gate fails, evidence is incomparable, or analysis requires post-hoc exclusions.
## Stage Optional memory and skill factor experiment

This side branch may start after Level-1 closeout but never blocks the core Level-2 route.

### Packet PE7-MEMORY-SKILL-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL1-CLOSEOUT-1

**Class:** `CONTRACT`

**Outcome:** Freeze baseline/no-projection, memory-only, and skill-only arms; projection schema, provenance, expiry, invalidation, deletion/rebuild, leakage, budgets, and attribution.

**Allowed delta:** No projection implementation or experiment. Product durable memory stays a separate domain.

**Exit:** Factorial protocol with identical non-factor conditions and explicit non-authority/sensitive-evidence rules.

**Stop:** Projection can grant routing/spend/evaluator/output/adoption authority, combined arm is introduced post hoc, or raw sensitive evidence lacks approved retention.
### Packet PE7-MEMORY-ADAPTER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-MEMORY-SKILL-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement the bounded experimental memory projection adapter.

**Allowed delta:** Derived/deletable/rebuildable source-bound projection only; no product durable-memory mutation or authority.

**Exit:** Provenance/expiry/invalidation/delete/rebuild/leakage and no-authority tests pass.

**Stop:** Adapter becomes authoritative, persists forbidden raw content, or cannot be fully invalidated.
### Packet PE7-SKILL-ADAPTER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-MEMORY-ADAPTER-1

**Class:** `IMPLEMENT`

**Outcome:** Implement the bounded experimental skill projection adapter under the same factor contract.

**Allowed delta:** Skill-only derived projection; no registry authority, evaluator mutation, or production installation.

**Exit:** Source/version/scope/expiry/delete/rebuild/leakage and no-authority tests pass.

**Stop:** Skill can alter immutable policy/evaluator, execute outside admitted scope, or cannot be reconstructed.
### Packet PE7-MEMORY-SKILL-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-SKILL-ADAPTER-1

**Class:** `EFFECT`

**Outcome:** Execute the frozen baseline, memory-only, and skill-only arms under equal total lifecycle budget.

**Allowed delta:** Registered factor effects only; no combined arm or mid-run projection change.

**Exit:** Complete arm/task evidence, contamination/leakage sentinels, lifecycle cost, cleanup, and restricted/redacted bundles.

**Stop:** Leakage, imbalance, authority import, cross-arm contamination, or registered stop.
### Packet PE7-MEMORY-SKILL-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-MEMORY-SKILL-RUN-1

**Class:** `CLOSEOUT`

**Outcome:** Estimate individual factor effects and decide whether any future combined experiment is justified.

**Allowed delta:** Frozen analysis only; no automatic adoption or combined-arm authorization.

**Exit:** Positive, negative, null, or insufficient factor receipt with attribution and cost limits.

**Stop:** Interaction claims are made without a pre-registered combined arm or projection evidence is not independently reproducible.
## Stage Level-2 GO or NO-GO

This stage audits eligibility and records a human decision; it is not controller implementation.

### Packet PE7-HE-LEVEL2-RULE-AUDIT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL1-TRANSFER-ANALYSIS-1

**Class:** `CONTRACT`

**Outcome:** Verify that the Level-2 decision rule, hard gates, non-inferiority, value basis, uncertainty, lifecycle cost, diversity, contamination, feasibility, and stop thresholds were frozen before relevant outcomes.

**Allowed delta:** Audit only; no post-result threshold selection.

**Exit:** An eligible immutable rule/evidence manifest or DECISION_REQUIRED/NO_GO if preregistration is missing.

**Stop:** Any decisive threshold is post hoc, evidence is incomparable, or implementation feasibility lacks a bounded design.
### Packet PE7-HE-LEVEL2-EVIDENCE-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-RULE-AUDIT-1

**Class:** `CLOSEOUT`

**Outcome:** Independently apply the frozen rule to Golden Path, RWE, Level-1, transfer, cost, diversity, maintenance, review, recovery, and rollback evidence.

**Allowed delta:** Analysis only; no controller design or candidate adoption.

**Exit:** A complete decision dossier with each gate PASS/FAIL/INSUFFICIENT and no scalar override.

**Stop:** Any required evidence is unavailable, evaluator integrity is uncertain, or sensitivity changes the gate result.
### Packet PE7-HE-LEVEL2-DECISION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-EVIDENCE-ANALYSIS-1

**Class:** `CLOSEOUT`

**Outcome:** Obtain the explicit human GO, NO_GO, or DEFER decision bound to the exact dossier and controller envelope.

**Allowed delta:** Decision receipt only. GO grants only the next bounded laboratory contract; it grants no Provider run, adoption, merge, release, or deployment.

**Exit:** Hash-bound signed decision receipt and synchronized route. NO_GO/DEFER is valid completion and rewrites later routing rather than silently continuing.

**Stop:** Decision authority is absent, objections unresolved, or requested envelope exceeds the audited design.
## Stage Bounded Level-2 controller

These packets are eligible only after an explicit GO. They separate state, persistence, orchestration, evaluation, recovery, simulation, live pilot, and analysis.

### Packet PE7-HE-LEVEL2-CONTROLLER-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-DECISION-1

**Class:** `CONTRACT`

**Outcome:** On GO only, freeze generation/candidate limits, state machine, parent rule, APIs, owners, budgets, evaluator separation, stops, restart, cleanup, schema needs, and pilot envelope.

**Allowed delta:** No controller code, schema migration, or Provider effect.

**Exit:** File-level execution-ready contracts for the following controller slices and explicit proof that GO identity/envelope match.

**Stop:** Decision is not GO, any field remains caller/model controlled, or design imports adoption/merge/release authority.
### Packet PE7-HE-LEVEL2-STATE-PERSISTENCE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-CONTROLLER-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement default-off generation/run/candidate state, leases, lineage links, audit, and migrations under LocalProductStore.

**Allowed delta:** Contract-approved additive persistence only; no scheduling or Provider effect.

**Exit:** Migration/rollback, SQLite/PostgreSQL parity, lease, idempotency, tamper, and restart tests pass.

**Stop:** Creates a second store, destructive migration lacks recovery, or lease identity is ambiguous.
### Packet PE7-HE-LEVEL2-GENERATION-ORCHESTRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-STATE-PERSISTENCE-1

**Class:** `IMPLEMENT`

**Outcome:** Implement the fixed-generation scheduler and candidate lifecycle using existing runtime/executor owners.

**Allowed delta:** Provider-free orchestration with stubbed effects only; one selected laboratory parent per generation.

**Exit:** Deterministic order, candidate limits, exact lineage, cancellation, late-write, and no-extra-generation tests pass.

**Stop:** Controller becomes a second scheduler, can self-extend limits, or changes active production Harness.
### Packet PE7-HE-LEVEL2-EVALUATION-SELECTION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-GENERATION-ORCHESTRATION-1

**Class:** `IMPLEMENT`

**Outcome:** Integrate immutable evaluator/sentinels, total lifecycle budgets, diversity admission, hard gates, Pareto archive, and parent selection.

**Allowed delta:** Use EC1-EC5 owners unchanged; integration only.

**Exit:** Adversarial fixtures prove evaluator immutability, full-cost accounting, no scalar override, hidden-reject prevention, and deterministic parent selection.

**Stop:** Controller can alter evaluator/labels, reset budget, select failed candidate, or hide an arm.
### Packet PE7-HE-LEVEL2-STOP-RECOVERY-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-EVALUATION-SELECTION-1

**Class:** `IMPLEMENT`

**Outcome:** Implement global/local stops, saturation, regression, exploitation, diversity-collapse, maintenance-burden, crash, lease, exactly-once, and cleanup behavior.

**Allowed delta:** Stop/recovery transitions only; no live effects.

**Exit:** Fault injection, concurrency, restart, outcome-unknown, cleanup, parity, and terminal-evidence tests pass.

**Stop:** A stopped run can resume without authority, an effect can repeat, or budget/evaluator state is lost.
### Packet PE7-HE-LEVEL2-SIMULATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-STOP-RECOVERY-1

**Class:** `CLOSEOUT`

**Outcome:** Run provider-free deterministic simulations covering success, every stop class, crash points, contamination, gaming, and rollback.

**Allowed delta:** Fixture/simulation evidence only; no Provider or target effect.

**Exit:** Independent conformance receipt, bounded performance/resource evidence, and zero unresolved pilot blocker.

**Stop:** Simulation cannot reproduce a transition, safety invariant fails, or implementation deviates from the GO envelope.
### Packet PE7-HE-LEVEL2-PILOT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-SIMULATION-1

**Class:** `EFFECT`

**Outcome:** Execute one small fixed Level-2 laboratory pilot under a separate finite authorization.

**Allowed delta:** Only the audited generation/candidate/evaluation envelope; no continuation across runs, production adoption, or limit increase.

**Exit:** Every generation/candidate/effect reaches terminal evidence with complete cost, lineage, evaluator, stop, cleanup, and restricted/redacted bundles.

**Stop:** Any mandatory stop, authority drift, outcome unknown, contamination, evaluator mutation, budget breach, or evidence loss.
### Packet PE7-HE-LEVEL2-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-PILOT-1

**Class:** `CLOSEOUT`

**Outcome:** Independently validate the pilot and select at most one experimental Harness for final transfer.

**Allowed delta:** Analysis/status/artifact only; no further generation or active-Harness replacement.

**Exit:** PASS, NO_GO, SATURATED, or INSUFFICIENT receipt with full distribution, Pareto yield, lifecycle cost, maintenance/review/recovery burden, and exact candidate identity.

**Stop:** Best-only reporting, incomplete rejected-candidate evidence, rule change after results, or any hard-gate regression.
## Stage Final sealed transfer

Final transfer is larger and sealed; it is not production adoption.

### Packet PE7-HE-FINAL-TRANSFER-PROTOCOL-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-CLOSEOUT-1

**Class:** `CONTRACT`

**Outcome:** Freeze a larger unseen task/family corpus, baselines, evaluator/labels, budgets, seeds, drift, contamination, stops, analysis, preflight, and finite authorizations.

**Allowed delta:** No execution or candidate repair.

**Exit:** Hash-bound final-transfer protocol/corpus and zero-mismatch authorization package.

**Stop:** Unseen status is compromised, value bases are incomparable, or candidate influenced protocol/corpus.
### Packet PE7-HE-FINAL-TRANSFER-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-FINAL-TRANSFER-PROTOCOL-1

**Class:** `EFFECT`

**Outcome:** Execute the selected experimental Harness and baselines on the final sealed set.

**Allowed delta:** Registered transfer effects only; no repair, learning, or evaluator change from transfer outcomes.

**Exit:** Complete blinded task/arm results, failures, cost, diversity, review/rework/recovery, drift, cleanup, and evidence.

**Stop:** Contamination, authority/evaluator drift, outcome unknown, or registered global stop.
### Packet PE7-HE-FINAL-TRANSFER-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-FINAL-TRANSFER-RUN-1

**Class:** `CLOSEOUT`

**Outcome:** Apply the frozen analysis and bound the strongest supported Harness-improvement claim.

**Allowed delta:** Analysis only; development gains cannot override final-transfer regression.

**Exit:** Independent TRANSFER_PASS, NO_GO, or INSUFFICIENT receipt with exact candidate/baseline identities and limitations.

**Stop:** Any hard gate/non-inferiority/value/cost/uncertainty requirement fails or evidence needs post-hoc exclusion.
## Stage Human adoption branch

Adoption is independent of Meta research and remains a human decision.

### Packet PE7-HE-ADOPTION-READINESS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-FINAL-TRANSFER-ANALYSIS-1

**Class:** `CONTRACT`

**Outcome:** Build the exact candidate artifact/diff, compatibility/migration, maintenance/security, rollout/observability, rollback, CI/review, and unresolved-objection dossier.

**Allowed delta:** Readiness planning/evidence only; no adoption, merge, release, deployment, or installation.

**Exit:** Independent adoption-readiness receipt with bounded canary/rollback proposal and all objections explicit.

**Stop:** Transfer is not PASS, rollback is untested, compatibility/security/maintenance cost is unacceptable, or exact artifact differs.
### Packet PE7-HE-ADOPTION-DECISION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-ADOPTION-READINESS-1

**Class:** `CLOSEOUT`

**Outcome:** Obtain a human ADOPT, DECLINE, or DEFER receipt for the exact candidate and rollout envelope.

**Allowed delta:** Decision only. ADOPT authorizes only a future separately planned adoption implementation packet.

**Exit:** Hash-bound decision and route update; DECLINE/DEFER remains valid research completion.

**Stop:** Decision authority is absent, objections unresolved, or requested rollout exceeds the readiness envelope.
## Stage Meta Improver branch

Meta research asks whether an improvement operator improves the distribution of eligible Harness improvements. It is not implied by one improved descendant and grants no adoption authority.

### Packet PE7-HE-META-CLAIM-PROTOCOL-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-FINAL-TRANSFER-ANALYSIS-1

**Class:** `CONTRACT`

**Outcome:** Decide whether Meta research is justified and, on GO, freeze the bounded second-order claim, estimands, hard gates, effect/error thresholds, domain, stops, and strongest allowed conclusion.

**Allowed delta:** Planning/GO-NO-GO only; no operator implementation or experiment.

**Exit:** Human-approved META_GO or META_NO_GO receipt. NO_GO rewrites the route; GO binds every later Meta packet.

**Stop:** Claim is open-ended, thresholds are post hoc, task/operator sample is infeasible, or authority/retention/spend envelope is unacceptable.
### Packet PE7-HE-META-OPERATOR-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-CLAIM-PROTOCOL-1

**Class:** `CONTRACT`

**Outcome:** On META_GO, freeze O0/O1 operator interfaces, identities, allowed algorithmic difference, input evidence, outputs, lineage, randomness, failure mapping, and non-authorities.

**Allowed delta:** No operator code. O1 may change only the pre-registered improvement policy, never evaluator/labels/authority.

**Exit:** Exact O0/O1 contract and implementation test vectors with one identifiable treatment difference.

**Stop:** Operators differ in budget/evaluator/access/authority, treatment is not isolatable, or O1 can self-modify its contract.
### Packet PE7-HE-META-CORPUS-EVALUATOR-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-OPERATOR-CONTRACT-1

**Class:** `CONTRACT`

**Outcome:** Seal development, fixture-pilot, full-comparison, and replication task families; freeze immutable evaluator/labels, baselines, contamination/gaming sentinels, blinding, and access.

**Allowed delta:** No operator access or experiment.

**Exit:** Disjoint hash-bound corpus/evaluator manifest with unseen-family proof and invalidation rules.

**Stop:** Operator/generator influenced labels or holdout, task families are not independent enough for the claim, or leakage cannot be detected.
### Packet PE7-HE-META-BUDGET-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-CORPUS-EVALUATOR-1

**Class:** `CONTRACT`

**Outcome:** Freeze equal total lifecycle budgets, candidate/generation/task/repetition limits, randomization, missingness, analysis, stop, recovery, and finite authorization envelopes for O0/O1.

**Allowed delta:** No run or operator implementation.

**Exit:** Versioned pre-registration with full-cost eligibility, power/precision sensitivity, seeds, allocation, and no post-pilot tunable claim field.

**Stop:** Equal budget is not enforceable, sample size/spend is unacceptable, or pilot/full/replication boundaries are not disjoint.
### Packet PE7-HE-META-O0-BASELINE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-BUDGET-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement/freeze the baseline improvement operator O0 as a deterministic adapter over existing EC/Level-2 owners.

**Allowed delta:** O0 policy only; no evaluator, budget, authority, adoption, or live effect.

**Exit:** Golden test vectors, lineage, budget requests, failure/stop, replay, and no-authority tests pass.

**Stop:** O0 is not reproducible, imports hidden heuristics/data, or bypasses experiment controls.
### Packet PE7-HE-META-O1-CANDIDATE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-O0-BASELINE-1

**Class:** `IMPLEMENT`

**Outcome:** Implement the pre-registered candidate improvement operator O1 behind the identical interface.

**Allowed delta:** Only the contract-approved operator-policy treatment difference from O0.

**Exit:** Differential tests prove identical authority/evaluator/budget/access and the exact intended policy delta.

**Stop:** Implementation adds another treatment difference, accesses sealed outcomes, self-modifies, or cannot be replayed.
### Packet PE7-HE-META-FIXTURE-PILOT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-O1-CANDIDATE-1

**Class:** `EFFECT`

**Outcome:** Run O0/O1 only on the disjoint fixture-pilot set to verify mechanics, cost capture, stops, and evidence flow; do not estimate the Meta claim.

**Allowed delta:** Finite pilot effects only; full/replication sets remain sealed and claim thresholds cannot change.

**Exit:** Complete pilot evidence and a mechanical PASS/REPAIR_REQUIRED/NO_GO disposition.

**Stop:** Leakage, treatment imbalance, evaluator mutation, outcome unknown, cost incompleteness, or any claim inference from fixture results.
### Packet PE7-HE-META-PILOT-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-FIXTURE-PILOT-1

**Class:** `CLOSEOUT`

**Outcome:** Validate pilot conformance and allow only mechanical repairs that preserve the frozen treatment and claim protocol.

**Allowed delta:** Evidence and explicitly enumerated non-semantic repair decision only; any semantic change requires a new Meta contract/version.

**Exit:** Exact-head conformance receipt and unchanged full-comparison pre-registration, or NO_GO.

**Stop:** Repair changes O0/O1 treatment, thresholds, corpus, evaluator, budgets, allocation, or claim.
### Packet PE7-HE-META-COMPARISON-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-PILOT-CLOSEOUT-1

**Class:** `EFFECT`

**Outcome:** Execute the preregistered randomized/blinded O0/O1 full comparison on unseen task families.

**Allowed delta:** Registered operator experiments only; no tuning, selective rerun, or hidden descendant.

**Exit:** All operators/tasks/repetitions/candidates including failures and rejects have terminal lineage, full cost, evaluator, transfer, stop, cleanup, and evidence.

**Stop:** Any global stop, contamination, imbalance, authority/evaluator drift, outcome unknown, or evidence loss.
### Packet PE7-HE-META-REPLICATION-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-COMPARISON-RUN-1

**Class:** `EFFECT`

**Outcome:** Execute the frozen independent replication/transfer set without inspecting or adapting to comparative conclusions beyond registered safety stops.

**Allowed delta:** Registered replication effects only.

**Exit:** Complete replication evidence bound to the same O0/O1 identities, evaluator, budgets, and claim protocol.

**Stop:** Operator/version changes, holdout leakage, drift beyond limits, outcome unknown, or replication authority failure.
### Packet PE7-HE-META-ANALYSIS-DECISION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-REPLICATION-RUN-1

**Class:** `CLOSEOUT`

**Outcome:** Apply the frozen analysis to operator-level distributions, Pareto yield, improvement cost, eligible-descendant rate, and transfer reliability; issue the bounded Meta claim decision.

**Allowed delta:** Analysis/claim/status only. No further operator update or production adoption.

**Exit:** Independent META_SUPPORTED, META_NOT_SUPPORTED, HARM, or INSUFFICIENT receipt with uncertainty, multiplicity, sensitivity, domain limits, and full failures/costs.

**Stop:** Only one descendant supports the result, replication fails, hard gates fail, evidence is selective, or conclusion exceeds the tested domain.
## Stage Optional advanced recursive research portfolio

This portfolio is optional research after a supported fixed-operator Meta result. It is not production adoption, does not make Dashboard or human adoption depend on speculative research, and never activates mechanically. Every branch needs its own human GO, finite authority, equal total lifecycle budget, sealed comparison/replication sets, and explicit negative-result disposition.

Research-improvement depth here is not `engine/src/recursive_execution.rs` task-tree depth. R4 and R5 are independently attributable sibling axes after R3, not a claim that weight adaptation is intrinsically a deeper meta-level; R6 is eligible only after both axes have explicit dispositions:

| Depth | Mutable object | Required boundary |
|---|---|---|
| R1 | One Harness candidate generation | Existing Level-1 one-generation laboratory |
| R2 | Multiple Harness generations under a fixed controller | Existing bounded Level-2 controller |
| R3 | A fixed improvement operator | Existing Meta O0/O1 branch |
| R4 | The improvement procedure inside a bounded meta-operator | DGM-H-inspired metacognitive branch; outer evaluator, parent rule, budget, authority, and stops remain immutable |
| R5 | Harness plus parameter-efficient model adapters | SIA-inspired weight/harness branch; immutable base checkpoint and separately governed training effects |
| R6 | Exactly one outer search policy family | Parent selection, harness-vs-weight lever selection, or curriculum policy; evaluator and objectives remain external and immutable |
| Not routed | Evaluator, labels, goals, safety policy, permissions, budgets, adoption, release, or deployment | No self-modification path; any proposal is `DECISION_REQUIRED` and requires a new human architecture decision |

Primary research is reference evidence, not an implementation dependency or authority source:

- [Hyperagents / DGM-H](https://arxiv.org/abs/2603.19461) supports testing editable improvement procedures and archive-based stepping stones. Its main experiments retain fixed parent selection and evaluation; its preliminary modifiable-parent experiment did not significantly beat, and had a lower point estimate than, the handcrafted selector. R4 therefore keeps the complete outer shell fixed; parent-policy evolution is deferred to R6.
- [SIA](https://arxiv.org/abs/2605.27276) supports testing harness and LoRA weight updates as distinct levers. Its reported comparison spans three tasks and the paper identifies coupled Goodhart risk from both levers optimizing one verifier. R5 therefore requires a four-arm factorial comparison before any interleaved co-evolution claim.
- The official [Hyperagents](https://github.com/facebookresearch/Hyperagents) and [SIA](https://github.com/hexo-ai/sia) repositories may inform bounded adapters after exact-commit, license, threat, and source-mapping review. They do not become core runtimes, stores, evaluators, trainers, or untrusted-code execution authority.

### Packet PE7-HE-ADVANCED-RECURSION-GATE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-ANALYSIS-DECISION-1 with `META_SUPPORTED`

**Class:** `CONTRACT`

**Outcome:** Decide whether any R4-R6 research is justified and, on human `ADVANCED_GO`, freeze the immutable outer shell, branch-specific claims, maximum depth, mutable surfaces, sandbox, budgets, evidence retention, global stops, and strongest allowed conclusions.

**Allowed delta:** Planning and GO/NO-GO only; no adapter, training, self-modification, Provider request, or target effect.

**Exit:** Hash-bound `ADVANCED_GO` or `ADVANCED_NO_GO` receipt. GO names independently authorized R4 and/or R5 branches and grants neither R6 nor production authority.

**Stop:** Meta evidence is unsupported, recursive depth is unbounded, immutable surfaces are incomplete, oversight/rollback is infeasible, or expected lifecycle cost is unacceptable.

### R4 bounded metacognitive operator (DGM-H-inspired)

### Packet PE7-HE-R4-METACOGNITIVE-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-ADVANCED-RECURSION-GATE-1 with R4 authorized

**Class:** `CONTRACT`

**Outcome:** Freeze the fixed Meta operator baseline and one self-referential treatment whose internal diagnosis, memory, proposal, and modification procedure may edit only an enumerated meta-operator workspace.

**Allowed delta:** Contract only. Evaluator/labels, parent selection, archive admission, budgets, permissions, stops, sandbox, active Harness, adoption, and release remain byte/value/behavior immutable.

**Exit:** Versioned editable-surface manifest, O0/R4 treatment identity, code/data access map, causal-manifest binding, compile/validation rules, equal-budget comparison and replication protocol, and complete rollback.

**Stop:** The treatment difference is not isolatable, generated code can escape the sandbox, or any outer-shell component must become mutable.

### Packet PE7-HE-R4-METACOGNITIVE-ADAPTER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R4-METACOGNITIVE-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement a provider-free, default-off metacognitive operator adapter over existing EC/Level-2 owners, with immutable snapshots of every self-change and no direct external-project dependency.

**Allowed delta:** Contract-approved meta-operator workspace, validation, lineage, archive projection, sandbox mediation, and fixtures only; no live effect or outer-loop change.

**Exit:** Self-edit lineage, stale-parent, tamper, forbidden-surface, compile failure, sandbox escape, rollback, restart, deterministic replay, full-cost, and no-authority tests pass.

**Stop:** Adapter becomes another scheduler/store/evaluator, can rewrite its contract, hides failed descendants, or cannot restore the exact prior operator.

### Packet PE7-HE-R4-COMPARISON-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R4-METACOGNITIVE-ADAPTER-1

**Class:** `EFFECT`

**Outcome:** Execute the frozen fixed-operator versus metacognitive-operator comparison on sealed unseen task families under equal lifecycle budgets and finite authorization.

**Allowed delta:** Registered operator effects only; parent selection/evaluator/thresholds remain fixed and no descendant is adopted.

**Exit:** Every operator, task, descendant, self-edit, reject, failure, cost, stop, cleanup, and causal/prediction record reaches terminal evidence.

**Stop:** Sandbox/authority drift, contamination, evaluator gaming, outcome unknown, selective archive, budget imbalance, or outer-shell change.

### Packet PE7-HE-R4-REPLICATION-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R4-COMPARISON-RUN-1

**Class:** `EFFECT`

**Outcome:** Execute the unchanged R4 protocol on a separately sealed replication domain without adapting to comparative results.

**Allowed delta:** Registered replication effects only.

**Exit:** Complete replication evidence bound to the same operator identities, mutable surface, outer shell, evaluator, budgets, and claim protocol.

**Stop:** Operator/version drift, cross-domain leakage, outcome unknown, or replication requires a post-result repair.

### Packet PE7-HE-R4-ANALYSIS-DECISION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R4-REPLICATION-RUN-1

**Class:** `CLOSEOUT`

**Outcome:** Test whether bounded self-referential modification improves eligible-descendant distribution, transfer reliability, improvement cost, and prediction calibration over the fixed operator.

**Allowed delta:** Analysis, claim, and branch disposition only.

**Exit:** Independent `METACOGNITIVE_SUPPORTED`, `NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` receipt with domain limits and full failures/costs.

**Stop:** One descendant carries the result, replication fails, added complexity erases value, or the conclusion implies open-ended/self-accelerating improvement.

### R5 Harness and weight-adapter co-evolution (SIA-inspired)

### Packet PE7-HE-R5-WEIGHT-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-ADVANCED-RECURSION-GATE-1 with R5 authorized

**Class:** `CONTRACT`

**Outcome:** Freeze a separate training-effect boundary, immutable open-weight base checkpoint, parameter-efficient adapter format, dataset/provenance/license/privacy rules, trainer/optimizer/RNG/compute identities, verifier separation, checkpoint security, budgets, rollback, and four-arm factorial protocol.

**Allowed delta:** Planning only. First-stage weight work is adapter-only (for example LoRA); base or full-model weights, Provider-hosted models, and production model routing remain immutable.

**Exit:** Exact `base`, `harness-only`, `weight-only`, and `harness+weight` arms with matched non-factor conditions, fixed update schedule, disjoint development/transfer sets, finite training authority, artifact retention, and deletion/rollback contract.

**Stop:** Training data rights/provenance are unclear, verifier leakage is possible, compute cannot be bounded, arms are confounded, or a second product store/budget/evaluator is proposed.

### Packet PE7-HE-R5-WEIGHT-ADAPTER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R5-WEIGHT-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement a default-off external-training adapter that records immutable base/adapter/data/trainer/config/seed/compute identities and returns hash-bound artifacts through existing artifact/store owners.

**Allowed delta:** Adapter, validation, sandbox/job mediation, artifact references, redacted receipts, and provider-free fixtures only; model binaries remain outside the repository and no training runs in CI.

**Exit:** Wrong-base, poisoned/malformed adapter, data/config drift, duplicate job, crash, cancellation, outcome unknown, checksum, deletion, rollback, restart, parity, and no-production-route tests pass.

**Stop:** Credentials or training data enter durable evidence, adapter can replace the active model, training effects can retry ambiguously, or external infrastructure becomes a core authority.

### Packet PE7-HE-R5-FACTORIAL-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R5-WEIGHT-ADAPTER-1

**Class:** `EFFECT`

**Outcome:** Execute the preregistered four-arm factorial experiment with fixed update schedules to estimate harness, weight-adapter, and interaction effects before dynamic lever selection.

**Allowed delta:** Registered Harness mutations and adapter-training effects only; no interleaved chooser, full-weight update, production routing, or post-result arm change.

**Exit:** Complete task/arm/checkpoint/candidate/failure/reject/cost/contamination/cleanup evidence with matched budgets and terminal artifact lineage.

**Stop:** Arm imbalance, verifier coupling sentinel, data leakage, catastrophic capability regression, outcome unknown, budget breach, or selective checkpoint reporting.

### Packet PE7-HE-R5-FACTORIAL-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R5-FACTORIAL-RUN-1

**Class:** `CLOSEOUT`

**Outcome:** Estimate main and interaction effects, transfer/non-inferiority, coupled-Goodhart sensitivity, full lifecycle value, and whether a fixed SIA-like lever chooser is eligible for one co-evolution pilot.

**Allowed delta:** Frozen analysis and `COEVOLUTION_ELIGIBLE`/`NO_GO`/`HARM`/`INSUFFICIENT` disposition only.

**Exit:** Independent factorial receipt with all four arms, uncertainty, multiplicity, sensitivity, adapter/base identities, and strongest allowed claim.

**Stop:** Weight-only attribution is unavailable, interaction is post hoc, transfer regresses, or the chooser would be tuned on the comparison result.

### Packet PE7-HE-R5-COEVOLUTION-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R5-FACTORIAL-ANALYSIS-1 with `COEVOLUTION_ELIGIBLE` and separate human authorization

**Class:** `EFFECT`

**Outcome:** Execute one bounded SIA-like pilot in which a frozen lever-selection policy interleaves Harness and adapter-weight updates under the same immutable outer evaluator and budget owners.

**Allowed delta:** Registered interleaving only; the lever selector does not learn or self-modify, and full-model weights remain unchanged.

**Exit:** Every lever decision, causal manifest, trajectory identity, Harness delta, adapter checkpoint, cost, stop, and reject reaches terminal evidence.

**Stop:** Selector or evaluator changes, alternating updates amplify verifier gaming, capability regression crosses a hard gate, or evidence cannot attribute each state transition.

### Packet PE7-HE-R5-TRANSFER-REPLICATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R5-COEVOLUTION-RUN-1

**Class:** `EFFECT`

**Outcome:** Replicate the frozen co-evolution treatment and factorial baselines on a separately sealed unseen task/model family.

**Allowed delta:** Registered replication effects only; no chooser, Harness, optimizer, data, threshold, or adapter repair.

**Exit:** Complete blinded replication evidence with base/adapter lineage, transfer, drift, regression, cost, and cleanup.

**Stop:** Model/task leakage, checkpoint incompatibility, outcome unknown, treatment drift, or replication hard-gate failure.

### Packet PE7-HE-R5-ANALYSIS-DECISION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R5-TRANSFER-REPLICATION-1

**Class:** `CLOSEOUT`

**Outcome:** Decide the bounded Harness+adapter claim and whether full-weight or model-architecture evolution is even planning-eligible.

**Allowed delta:** Analysis and `WEIGHT_COEVOLUTION_SUPPORTED`, `NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` disposition only. Full-weight work remains unrouted unless a later human decision creates a new contract.

**Exit:** Independent receipt covering factorial attribution, co-evolution increment, replication, catastrophic-forgetting/regression, contamination, compute, storage, and rollback costs.

**Stop:** Gains do not survive replication, base-model capabilities regress, checkpoint provenance is incomplete, or the conclusion generalizes beyond the tested adapter/model/domain.

### R6 bounded outer-policy evolution

### Packet PE7-HE-R6-OUTER-POLICY-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R4-ANALYSIS-DECISION-1 and PE7-HE-R5-ANALYSIS-DECISION-1, with at least one supported result and separate human R6 GO

**Class:** `CONTRACT`

**Outcome:** Freeze exactly one mutable outer policy family for the first experiment: parent selection, Harness-vs-weight lever selection, or curriculum proposal. Keep evaluator/labels, task acceptance, hard gates, budgets, permissions, stops, archive integrity, and adoption external and immutable.

**Allowed delta:** Contract only; no simultaneous multi-policy evolution, evaluator evolution, self-generated goals, or live effect.

**Exit:** One identifiable fixed-policy baseline/treatment difference, state/action/outcome schema, off-policy evaluation limits, equal-budget comparison, sealed replication, rollback, and strongest allowed R6 claim.

**Stop:** Policy effects cannot be isolated, policy can choose its own evaluator/data/limits, or recursive depth can grow without a new human decision.

### Packet PE7-HE-R6-OUTER-POLICY-ADAPTER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R6-OUTER-POLICY-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement the single accepted outer-policy treatment behind a deterministic, versioned interface over existing controllers and evidence owners.

**Allowed delta:** Policy adapter, immutable transition evidence, sandbox/fixtures, and rollback only; no live effect or other mutable family.

**Exit:** Action bounds, stale state, tamper, forbidden action, replay, exploration cap, rollback, crash, restart, full-cost, and no-authority tests pass.

**Stop:** Adapter becomes another controller/evaluator, can change its action space, or cannot reproduce the baseline/treatment boundary.

### Packet PE7-HE-R6-COMPARISON-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R6-OUTER-POLICY-ADAPTER-1

**Class:** `EFFECT`

**Outcome:** Execute the fixed versus evolvable outer-policy comparison under finite authority, equal lifecycle budget, sealed tasks, and unchanged R4/R5 components.

**Allowed delta:** Registered policy effects only.

**Exit:** Complete policy-transition, candidate/checkpoint, failure/reject, cost, stop, contamination, and cleanup evidence.

**Stop:** Action-space/evaluator/budget drift, runaway exploration, curriculum leakage, outcome unknown, or selective transition history.

### Packet PE7-HE-R6-REPLICATION-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R6-COMPARISON-RUN-1

**Class:** `EFFECT`

**Outcome:** Execute the unchanged outer-policy comparison on a separately sealed replication family.

**Allowed delta:** Registered replication effects only.

**Exit:** Complete replication evidence bound to the same policy identities, action space, outer shell, evaluator, budgets, and claim protocol.

**Stop:** Policy/version drift, leakage, outcome unknown, or replication requires post-result tuning.

### Packet PE7-HE-R6-ANALYSIS-DECISION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R6-REPLICATION-RUN-1

**Class:** `CLOSEOUT`

**Outcome:** Determine whether one bounded outer-policy family improves the distribution and efficiency of eligible improvements without destabilizing attribution, safety, diversity, or oversight.

**Allowed delta:** Analysis and `OUTER_POLICY_SUPPORTED`, `NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` disposition only; no next recursive level is implied.

**Exit:** Independent receipt with replication, uncertainty, domain/action limits, prediction calibration, full costs, and explicit maximum supported recursive depth.

**Stop:** Results rely on evaluator/goal adaptation, oversight cannot keep pace, or the claim is widened to autonomous open-ended evolution.
## Stage Dashboard last

Presentation work remains the last mandatory product-route surface and never becomes an authority owner. The optional advanced portfolio above does not delay Dashboard once the existing adoption and fixed-operator Meta prerequisites close; if advanced research is started, it remains an independent branch.

### Packet PE7-DASHBOARD-DISPOSITION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-ADOPTION-DECISION-1 and PE7-HE-META-ANALYSIS-DECISION-1

**Class:** `CONTRACT`

**Outcome:** Decide whether stale PR #225 should close and be recreated, or be refreshed, against the final accepted schema and route dispositions.

**Allowed delta:** PR disposition and presentation contract only; no runtime/schema/business behavior.

**Exit:** Exact presentation scope, accepted data projections, accessibility/visual matrix, branch strategy, and rollback.

**Stop:** An upstream branch ended NO_GO and canonical routing has not been synchronized, schema still moves, or requested UI implies backend authority.
### Packet PE7-DASHBOARD-REFRESH-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-DASHBOARD-DISPOSITION-1

**Class:** `IMPLEMENT`

**Outcome:** Apply the accepted presentation-only refresh on the current schema.

**Allowed delta:** CSS/layout/presentation and bounded tests only; no API, runtime, persistence, route, permission, evaluator, budget, adoption, output, or deployment behavior.

**Exit:** Lint/typecheck/tests/build and browser evidence across light/dark, desktop/mobile, keyboard, contrast, reduced motion, overflow, console, and network errors.

**Stop:** Any backend or schema change is needed, stale #225 behavior is imported blindly, or accessibility regresses.
### Packet PE7-DASHBOARD-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-DASHBOARD-REFRESH-1

**Class:** `CLOSEOUT`

**Outcome:** Independently verify exact-head presentation scope and close the final deferred surface.

**Allowed delta:** Review/status/merge eligibility evidence only; no deployment authority.

**Exit:** Exact-head independent PASS, canonical CI, visual evidence digests, clean rollback, and explicit merge decision.

**Stop:** Unreviewed visual delta, missing canonical check, backend behavior change, or unresolved accessibility objection.
## Adoption and claim boundary

candidate generation != causal explanation != prediction accuracy != experimental parent selection != active-Harness adoption != improvement-operator research != weight-adapter training

Each authority has its own evidence and decision. A GO authorizes only its named next packet. A NO_GO, DECLINE, DEFER, SATURATED, HARM, or INSUFFICIENT result is valid completion and requires the canonical route to be rewritten before any non-dependent work proceeds.

## Dashboard boundary

Dashboard work stays presentation-only and last. It may project accepted schemas and evidence but cannot become a workflow, evaluator, spend, approval, adoption, output, merge, release, or deployment owner.
