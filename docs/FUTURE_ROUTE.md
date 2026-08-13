# Future Route

Last updated: 2026-08-13.

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
| RWE measurement/decision baseline | existing RWE corpus, protocol, schedule, artifact, and evidence owners | no accepted estimand/sample/reviewer/retention contract or decision-grade corpus | any threshold, reviewer, retention, or evidence-access value lacks a T2/human owner |
| AC1 process supervision | AC0-enumerated subprocess/executor callers | no `ProcessSupervisor` module/interface exists today | placement would create a second runtime/executor owner or child cleanup cannot be proved |
| AC2 typed execution | current executor adapter/node/provider/CLI owners | no accepted cross-executor typed state/outcome/usage contract | any existing outcome cannot map without semantic loss or unknown-as-success coercion |
| AC3 Golden Path split | `product_golden_path.rs` plus LocalProductStore product-task authority | no accepted pure orchestration/effect-port seam | store, approval, output, audit, or external effect ownership becomes ambiguous |
| AC4 transaction views | sole LocalProductStore SQLite/PostgreSQL owners | no named borrowed transaction-view interfaces exist | atomic group, borrow/commit/rollback, parity, or recovery cannot be proved |
| AC5 composition root | current Rust startup, config, HTTP state, provider, and store construction | no single accepted validated composition-root contract | config precedence conflicts, dependency cycle, or secret resolution broadens |
| EC1 causal mutation evidence | existing HE artifact/store owner | `FailurePatternEvidenceV1`, `MutationHypothesisManifestV1`, and `PredictionOutcomeV1` are planned, not current accepted types | identity/causal source can be caller- or candidate-authored, mutable, or unaddressable |
| EC2 real evaluator/holdout | accepted evaluator/evidence owner; current `harness_evolution_eval.rs` remains fixture/default-off | no managed sealed holdout, access mediation, or real acceptance evaluator evidence | fixture result, candidate-controlled rule, leakage, or label uncertainty would be reported as acceptance |
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

Promotion procedure, exactly one packet at a time:

1. prove the accepted predecessor's receipt and disposition in `docs/CURRENT_STATUS.md`; a merely closed PR or nominally completed packet is insufficient;
2. refresh remote `main` and rerun owner/caller inventory; replace globs and planned-seam references with exact files;
3. expand the complete twelve-field contract in `docs/NEXT_DECISION.md`, resolving every `REFRESH_AT_PROMOTION` fact from then-current `main`; if a value still has no accepted owner, stop `DECISION_REQUIRED`;
4. author the machine-readable `weak-agent-dispatch:v1` capsule inside the promoted packet block (allowed paths, ordered steps, verification, rollback, forbidden next actions, external-effect limit);
5. remove the packet from this index and refresh this manifest;
6. have the routing change independently reviewed; a profile alone never starts a coding session.

## Stop and Resume Protocol

On any stop, the worker first prevents new effects, preserves store leases/receipts and restricted evidence, runs safe cleanup/compensation already owned by the active packet, and emits the bounded handoff with `state=DECISION_REQUIRED`, `BLOCKED_PREREQUISITE`, or `OUTCOME_UNKNOWN`. It must state whether an effect may already have occurred; unknown is never rewritten as zero. Resume must refresh main and external state, verify the same packet/evidence/authority identities and rollback state, and continue only from the named `next_permitted_action`. A stale receipt, changed contract, expired authority, missing evidence, or changed effect status requires a new planning/operator decision; never replay the prior command speculatively.

## Portfolio Inventory Manifest

The checked manifest below binds the complete ordered packet ID list, class/tier/risk counts, dependency graph, and the per-packet promotion profile rows. Any addition, removal, reorder, dependency change, profile edit, or generic-content replacement must deliberately refresh this manifest and appear as an independently reviewed planning diff; `scripts/check_agent_handoff.py` rejects silent drift.

The manifest rows are compact arrays of `packet_id`, `class`, `worker_tier`, `risk_class`, and `verification_family`. `execution_profile` is derived as `{packet_id}.v1`. `promotion_requirements` is the shared contract above plus `REFRESH_AT_PROMOTION` for owners, allowed paths, ordered steps, verification, and rollback.

<!-- future-route-inventory:v1
{"dependency_graph_sha256": "96e6a7dfe235a9f11d955c6caee9a1466a6e5b3c3a058be58505363d93a20c0d", "ordered_packet_ids": ["PE7-RWE-V2-VIABILITY-CLOSEOUT-1", "PE7-RWE-MR-ESTIMANDS-1", "PE7-RWE-MR-CORPUS-SAMPLING-1", "PE7-RWE-MR-OPERATIONS-EVIDENCE-1", "PE7-RWE-MR-PROTOCOL-FREEZE-1", "PE7-RWE-DB-SNAPSHOT-CORPUS-1", "PE7-RWE-DB-PREFLIGHT-1", "PE7-RWE-DB-RUN-1", "PE7-RWE-DB-ANALYSIS-1", "PE7-AC0-RUNTIME-INVENTORY-1", "PE7-AC0-DATA-CONTRACT-INVENTORY-1", "PE7-AC0-TRACE-ORDER-FREEZE-1", "PE7-AC1-CONTRACT-1", "PE7-AC1-SUPERVISOR-CORE-1", "PE7-AC1-CALLER-MIGRATION-1", "PE7-AC2-CONTRACT-1", "PE7-AC2-BOUNDARY-CORE-1", "PE7-AC2-CALLER-MIGRATION-1", "PE7-AC3-CONTRACT-1", "PE7-AC3-ORCHESTRATOR-CORE-1", "PE7-AC3-PORT-MIGRATION-1", "PE7-AC4-CONTRACT-1", "PE7-AC4-VIEWS-CORE-1", "PE7-AC4-CALLER-MIGRATION-1", "PE7-AC5-CONTRACT-1", "PE7-AC5-ROOT-CORE-1", "PE7-AC5-MODULE-MIGRATION-1", "PE7-AC6-CONTRACT-1", "PE7-AC6-RUST-CODEGEN-1", "PE7-AC6-SDK-MIGRATION-1", "PE7-AC6-DASHBOARD-MIGRATION-1", "PE7-AC6-COMPATIBILITY-CLOSEOUT-1", "PE7-AC7-REMOVAL-MANIFEST-1", "PE7-AC7-CLEANUP-1", "PE7-AC7-CLOSEOUT-1", "PE7-RWE-CR-RECONSTRUCTION-1", "PE7-RWE-CR-PROTOCOL-PREFLIGHT-1", "PE7-RWE-CR-RUN-1", "PE7-RWE-CR-ANALYSIS-1", "PE7-HE-EC1-CONTRACT-1", "PE7-HE-EC1-IDENTITY-LINEAGE-1", "PE7-HE-EC1-CAUSAL-MANIFEST-1", "PE7-HE-EC1-MUTATION-REGISTRY-1", "PE7-HE-EC2-CONTRACT-1", "PE7-HE-EC2-HOLDOUT-SEAL-1", "PE7-HE-EC2-SENTINEL-CONFORMANCE-1", "PE7-HE-EC2-PREDICTION-OUTCOME-1", "PE7-HE-EC3-CONTRACT-1", "PE7-HE-EC3-INSTRUMENTATION-1", "PE7-HE-EC3-ENFORCEMENT-1", "PE7-HE-EC4-CONTRACT-1", "PE7-HE-EC4-ADMISSION-1", "PE7-HE-EC4-COVERAGE-CLOSEOUT-1", "PE7-HE-EC5-CONTRACT-1", "PE7-HE-EC5-SELECTION-ARCHIVE-1", "PE7-HE-EC5-STOP-RECOVERY-1", "PE7-HE-LEVEL1-PREFLIGHT-1", "PE7-HE-LEVEL1-RUN-1", "PE7-HE-LEVEL1-CLOSEOUT-1", "PE7-HE-LEVEL1-TRANSFER-PROTOCOL-1", "PE7-HE-LEVEL1-TRANSFER-RUN-1", "PE7-HE-LEVEL1-TRANSFER-ANALYSIS-1", "PE7-MEMORY-SKILL-CONTRACT-1", "PE7-MEMORY-ADAPTER-1", "PE7-SKILL-ADAPTER-1", "PE7-MEMORY-SKILL-RUN-1", "PE7-MEMORY-SKILL-ANALYSIS-1", "PE7-HE-LEVEL2-RULE-AUDIT-1", "PE7-HE-LEVEL2-EVIDENCE-ANALYSIS-1", "PE7-HE-LEVEL2-DECISION-1", "PE7-HE-LEVEL2-CONTROLLER-CONTRACT-1", "PE7-HE-LEVEL2-STATE-PERSISTENCE-1", "PE7-HE-LEVEL2-GENERATION-ORCHESTRATION-1", "PE7-HE-LEVEL2-EVALUATION-SELECTION-1", "PE7-HE-LEVEL2-STOP-RECOVERY-1", "PE7-HE-LEVEL2-SIMULATION-1", "PE7-HE-LEVEL2-PILOT-1", "PE7-HE-LEVEL2-CLOSEOUT-1", "PE7-HE-FINAL-TRANSFER-PROTOCOL-1", "PE7-HE-FINAL-TRANSFER-RUN-1", "PE7-HE-FINAL-TRANSFER-ANALYSIS-1", "PE7-HE-ADOPTION-READINESS-1", "PE7-HE-ADOPTION-DECISION-1", "PE7-HE-META-CLAIM-PROTOCOL-1", "PE7-HE-META-OPERATOR-CONTRACT-1", "PE7-HE-META-CORPUS-EVALUATOR-1", "PE7-HE-META-BUDGET-CONTRACT-1", "PE7-HE-META-O0-BASELINE-1", "PE7-HE-META-O1-CANDIDATE-1", "PE7-HE-META-FIXTURE-PILOT-1", "PE7-HE-META-PILOT-CLOSEOUT-1", "PE7-HE-META-COMPARISON-RUN-1", "PE7-HE-META-REPLICATION-RUN-1", "PE7-HE-META-ANALYSIS-DECISION-1", "PE7-HE-ADVANCED-RECURSION-GATE-1", "PE7-HE-R4-METACOGNITIVE-CONTRACT-1", "PE7-HE-R4-METACOGNITIVE-ADAPTER-1", "PE7-HE-R4-COMPARISON-RUN-1", "PE7-HE-R4-REPLICATION-RUN-1", "PE7-HE-R4-ANALYSIS-DECISION-1", "PE7-HE-R5-WEIGHT-CONTRACT-1", "PE7-HE-R5-WEIGHT-ADAPTER-1", "PE7-HE-R5-FACTORIAL-RUN-1", "PE7-HE-R5-FACTORIAL-ANALYSIS-1", "PE7-HE-R5-COEVOLUTION-RUN-1", "PE7-HE-R5-TRANSFER-REPLICATION-1", "PE7-HE-R5-ANALYSIS-DECISION-1", "PE7-HE-R6-OUTER-POLICY-CONTRACT-1", "PE7-HE-R6-OUTER-POLICY-ADAPTER-1", "PE7-HE-R6-COMPARISON-RUN-1", "PE7-HE-R6-REPLICATION-RUN-1", "PE7-HE-R6-ANALYSIS-DECISION-1", "PE7-DASHBOARD-DISPOSITION-1", "PE7-DASHBOARD-REFRESH-1", "PE7-DASHBOARD-CLOSEOUT-1"], "ordered_packet_ids_sha256": "8525d033b211cf27a262dc997ac1e637bc5cda181626af457dbcf337d279f3e4", "packet_count": 115, "profiles": [["PE7-RWE-V2-VIABILITY-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-RWE-MR-ESTIMANDS-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-RWE-MR-CORPUS-SAMPLING-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-RWE-MR-OPERATIONS-EVIDENCE-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-RWE-MR-PROTOCOL-FREEZE-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-RWE-DB-SNAPSHOT-CORPUS-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-RWE-DB-PREFLIGHT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-RWE-DB-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-RWE-DB-ANALYSIS-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-AC0-RUNTIME-INVENTORY-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-AC0-DATA-CONTRACT-INVENTORY-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-AC0-TRACE-ORDER-FREEZE-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-AC1-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-AC1-SUPERVISOR-CORE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC1-CALLER-MIGRATION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC2-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-AC2-BOUNDARY-CORE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC2-CALLER-MIGRATION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC3-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-AC3-ORCHESTRATOR-CORE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC3-PORT-MIGRATION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC4-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-AC4-VIEWS-CORE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC4-CALLER-MIGRATION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC5-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-AC5-ROOT-CORE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC5-MODULE-MIGRATION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC6-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-AC6-RUST-CODEGEN-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC6-SDK-MIGRATION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC6-DASHBOARD-MIGRATION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC6-COMPATIBILITY-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-AC7-REMOVAL-MANIFEST-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-AC7-CLEANUP-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AC7-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-RWE-CR-RECONSTRUCTION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-RWE-CR-PROTOCOL-PREFLIGHT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-RWE-CR-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-RWE-CR-ANALYSIS-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-EC1-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-EC1-IDENTITY-LINEAGE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC1-CAUSAL-MANIFEST-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC1-MUTATION-REGISTRY-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC2-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-EC2-HOLDOUT-SEAL-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC2-SENTINEL-CONFORMANCE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC2-PREDICTION-OUTCOME-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC3-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-EC3-INSTRUMENTATION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC3-ENFORCEMENT-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC4-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-EC4-ADMISSION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC4-COVERAGE-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-EC5-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-EC5-SELECTION-ARCHIVE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-EC5-STOP-RECOVERY-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-LEVEL1-PREFLIGHT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-LEVEL1-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-LEVEL1-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-LEVEL1-TRANSFER-PROTOCOL-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-LEVEL1-TRANSFER-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-LEVEL1-TRANSFER-ANALYSIS-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-MEMORY-SKILL-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-MEMORY-ADAPTER-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-SKILL-ADAPTER-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-MEMORY-SKILL-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-MEMORY-SKILL-ANALYSIS-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-LEVEL2-RULE-AUDIT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-LEVEL2-EVIDENCE-ANALYSIS-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-LEVEL2-DECISION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-LEVEL2-CONTROLLER-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-LEVEL2-STATE-PERSISTENCE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-LEVEL2-GENERATION-ORCHESTRATION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-LEVEL2-EVALUATION-SELECTION-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-LEVEL2-STOP-RECOVERY-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-LEVEL2-SIMULATION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-LEVEL2-PILOT-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-LEVEL2-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-FINAL-TRANSFER-PROTOCOL-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-FINAL-TRANSFER-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-FINAL-TRANSFER-ANALYSIS-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-ADOPTION-READINESS-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-ADOPTION-DECISION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-META-CLAIM-PROTOCOL-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-META-OPERATOR-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-META-CORPUS-EVALUATOR-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-META-BUDGET-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-META-O0-BASELINE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-META-O1-CANDIDATE-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-META-FIXTURE-PILOT-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-META-PILOT-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-META-COMPARISON-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-META-REPLICATION-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-META-ANALYSIS-DECISION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-ADVANCED-RECURSION-GATE-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-R4-METACOGNITIVE-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-R4-METACOGNITIVE-ADAPTER-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-R4-COMPARISON-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R4-REPLICATION-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R4-ANALYSIS-DECISION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-R5-WEIGHT-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-R5-WEIGHT-ADAPTER-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-R5-FACTORIAL-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R5-FACTORIAL-ANALYSIS-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-R5-COEVOLUTION-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R5-TRANSFER-REPLICATION-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R5-ANALYSIS-DECISION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-HE-R6-OUTER-POLICY-CONTRACT-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-HE-R6-OUTER-POLICY-ADAPTER-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-HE-R6-COMPARISON-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R6-REPLICATION-RUN-1", "EFFECT", "T3", "external_effect", "external_effect_evidence"], ["PE7-HE-R6-ANALYSIS-DECISION-1", "CLOSEOUT", "T2", "none", "evidence_review"], ["PE7-DASHBOARD-DISPOSITION-1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-DASHBOARD-REFRESH-1", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-DASHBOARD-CLOSEOUT-1", "CLOSEOUT", "T2", "none", "evidence_review"]], "profiles_sha256": "b7111a0cbcc682f8c2d6406af2d9b9a5b8c0a809c617d754c95e98f7cc61c6cc", "schema_version": "future_route_inventory.v1"}
-->

# Future Route

Last updated: 2026-08-13.

This document is the sole long-horizon routing index. Every packet here is `BLOCKED_PREREQUISITE` and routing-only: its order, prerequisite, intended class, and bounded sketch are accepted planning context, not implementation or external-effect authority.

The current executable window and common execution contracts live in `docs/NEXT_DECISION.md`. Accepted truth lives in `docs/CURRENT_STATUS.md`; durable architecture and authority invariants live in `docs/ARCHITECTURE_BOOK.md`; current owners live in `docs/MODULE_MAP.md`; live PR, CI, and review facts come only from a fresh context capsule.

When an accepted predecessor closes, do not execute its successor from this file. Refresh remote `main`, reconcile any negative or insufficient disposition, remove exactly one eligible packet from this index, and expand its complete twelve-field contract in `docs/NEXT_DECISION.md`. Any unresolved value or duplicate packet identity is `DECISION_REQUIRED`.

## Stage RWE v2 viability

These packets prove lifecycle viability only. They do not authorize Architecture Convergence or an economic-improvement claim.

### Packet PE7-RWE-V2-VIABILITY-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-V2-VIABILITY-RUN-1

**Class:** `CLOSEOUT`

**Outcome:** Independently validate, redact, digest, and classify the v2 run without another Provider request.

**Allowed delta:** Evidence validation and canonical status only. Do not rerun a failed cell, retune the envelope, repair code, or upgrade the claim.

**Exit:** A durable redacted receipt bound to the restricted bundle digest and exact run/cell identities, with VIABLE, CONTROLLED_FAILURE, OUTCOME_UNKNOWN, or INSUFFICIENT disposition.

**Stop:** Raw/redacted mismatch, missing failure/cost evidence, unverifiable cleanup, or any claim stronger than lifecycle viability.
## Stage RWE measurement readiness

These packets make the later comparison decision-grade. All values are frozen before the decision-baseline outcomes are observed.

### Packet PE7-RWE-MR-ESTIMANDS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-V2-VIABILITY-CLOSEOUT-1

**Class:** `CONTRACT`

**Outcome:** Freeze the decision question, primary estimands, hard-gate outcomes, inferential unit, eligible value bases, minimum meaningful effects, non-inferiority margins, and missing/outcome-unknown rules.

**Allowed delta:** Planning evidence only. Repetitions remain nested measurements, not independent tasks; scalar summaries cannot override hard gates.

**Exit:** An independently reviewed estimand ledger with every threshold source, uncertainty target, and human value judgment explicit.

**Stop:** A threshold is chosen from favorable observed direction, value semantics are incomparable, or an authority-critical value lacks an owner.
### Packet PE7-RWE-MR-CORPUS-SAMPLING-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-MR-ESTIMANDS-1

**Class:** `CONTRACT`

**Outcome:** Freeze task-family strata, repositories/languages/difficulty coverage, inclusion/exclusion, contamination screening, repetition nesting, sample-size method, and maximum experiment envelope.

**Allowed delta:** No task execution. Viability variance may inform precision but cannot tune toward a favorable effect or substitute repeated cells for task coverage.

**Exit:** A versioned corpus-selection and sampling manifest with power/precision assumptions, sensitivity analysis, finite upper bound, and replacement rules fixed before outcomes.

**Stop:** Required coverage, spend ceiling, task availability, contamination control, or statistically defensible precision cannot be accepted.
### Packet PE7-RWE-MR-OPERATIONS-EVIDENCE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-MR-CORPUS-SAMPLING-1

**Class:** `CONTRACT`

**Outcome:** Freeze reviewer identity/blinding/disagreement rules, environment and drift capture, lifecycle-cost completeness, reconstructable Harness artifacts, and restricted raw/redacted retention/deletion/access policy.

**Allowed delta:** No Provider call or persistence schema change. Reuse existing artifact/evidence owners; define unavailable evidence honestly.

**Exit:** An operations/evidence manifest covering toolchain, dependencies, model-return identity, price source, runner, CI, human/review/rework/recovery cost, retention, and old-Harness reconstruction.

**Stop:** Reviewer independence, sensitive-evidence handling, environment reconstruction, cost completeness, or drift observation remains undefined.
### Packet PE7-RWE-MR-PROTOCOL-FREEZE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-MR-OPERATIONS-EVIDENCE-1

**Class:** `CLOSEOUT`

**Outcome:** Assemble and independently verify the complete decision-baseline plus contemporary-replay protocol.

**Allowed delta:** Mechanical canonicalization, hashing, validation, and review only; no new threshold or design choice after this packet begins.

**Exit:** One versioned hash-bound protocol, corpus-selection rule, authorization envelope template, analysis plan, and reconstructability manifest with zero unresolved decision field.

**Stop:** Cross-document contradiction, post-outcome tunable field, missing owner, excessive unaffordable envelope, or incomplete rollback/retention contract.
## Stage Decision-grade pre-AC baseline

This stage freezes the runnable artifacts, executes the accepted baseline once, then analyzes it separately.

### Packet PE7-RWE-DB-SNAPSHOT-CORPUS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-MR-PROTOCOL-FREEZE-1

**Class:** `IMPLEMENT`

**Outcome:** Materialize the frozen task artifacts and a reconstructable pre-AC Harness/config/toolchain snapshot under existing artifact owners.

**Allowed delta:** Provider-free artifact production only. Do not change task semantics, evaluator, budget, runtime owner, or accepted Harness behavior.

**Exit:** Hash-verified corpus and rebuildable old-Harness bundle whose provider-free golden traces match accepted main.

**Stop:** A task cannot be legally retained/replayed, snapshot reconstruction is nondeterministic, or artifact storage would create a second owner.
### Packet PE7-RWE-DB-PREFLIGHT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-DB-SNAPSHOT-CORPUS-1

**Class:** `CONTRACT`

**Outcome:** Run the complete provider-free baseline preflight and prepare finite per-run authorization packages.

**Allowed delta:** No Provider effect or result observation. Validate corpus/snapshot/protocol hashes, capacity, principals, target state, evidence destinations, and drift baseline.

**Exit:** Current zero-mismatch preflight receipts and explicit operator authorization requests bounded by the accepted experiment envelope.

**Stop:** Capacity, price, Provider identity, target safety, reviewer availability, retention, or any binding is stale or unavailable.
### Packet PE7-RWE-DB-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-DB-PREFLIGHT-1

**Class:** `EFFECT`

**Outcome:** Execute the frozen pre-AC decision baseline under the accepted allocation and finite authorizations.

**Allowed delta:** Only registered task executions and reviews. No selective rerun, hidden failure, task substitution, threshold change, or mid-run protocol repair.

**Exit:** Every scheduled unit is terminal with attempts, failures, usage, lifecycle cost, reviewer evidence, drift covariates, cleanup, and raw/redacted bundle bindings.

**Stop:** A registered global stop rule fires, comparability breaks, authority expires, outcome becomes unknown, contamination occurs, or evidence capture fails.
### Packet PE7-RWE-DB-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-DB-RUN-1

**Class:** `CLOSEOUT`

**Outcome:** Apply the frozen analysis plan and decide whether the pre-AC baseline is sufficient and safe to carry into convergence.

**Allowed delta:** Analysis and evidence sealing only. Preserve all missingness/failures and do not modify the protocol after unblinding.

**Exit:** An independent uncertainty-aware receipt with GO, NO_GO, or INSUFFICIENT disposition and the exact reconstructable old-Harness identity.

**Stop:** Analysis cannot be reproduced, hard gates fail, cost evidence is ineligible, or the data do not support the pre-registered estimands.
## Stage Architecture Convergence AC0

AC0 inventories and freezes; it makes no production ownership move.

### Packet PE7-AC0-RUNTIME-INVENTORY-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-DB-ANALYSIS-1

**Class:** `CONTRACT`

**Outcome:** Enumerate every production subprocess spawn/kill/reap site, executor adapter, environment/config read, timeout/cancellation path, and affected test fixture.

**Allowed delta:** Inventory and call-graph evidence only; no refactor or deletion.

**Exit:** A zero-unknown runtime/executor matrix with exact callers, owners, failure semantics, golden traces, and candidate migration groups.

**Stop:** A spawn/effect path cannot be classified, ownership conflicts, or static search disagrees with executable traces.
### Packet PE7-AC0-DATA-CONTRACT-INVENTORY-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC0-RUNTIME-INVENTORY-1

**Class:** `CONTRACT`

**Outcome:** Enumerate Golden Path responsibilities, store transaction entries, schemas/codegen/SDK/Dashboard projections, config construction, and legacy abstractions.

**Allowed delta:** Inventory only. Do not introduce transaction views, schema sources, composition roots, or replacement modules.

**Exit:** One owner/caller/transaction/projection/legacy matrix with compatibility and rollback obligations for AC1-AC7.

**Stop:** A current owner is ambiguous, a legacy surface still has unknown callers, or SQLite/PostgreSQL behavior cannot be mapped.
### Packet PE7-AC0-TRACE-ORDER-FREEZE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC0-DATA-CONTRACT-INVENTORY-1

**Class:** `CLOSEOUT`

**Outcome:** Freeze provider-free golden traces, AC dependency order, rollback points, and the exact file-level AC1 contract; bound the candidate surfaces for AC2-AC7.

**Allowed delta:** Evidence/decision closeout only; no ownership move.

**Exit:** An independently reviewed AC manifest with zero unknown production caller and an execution-ready AC1 contract.

**Stop:** Inventory contradicts the planned order, a golden trace cannot be stabilized, or a boundary would require a second owner.
## Stage Architecture Convergence AC1 - process supervision

AC1 converges admitted subprocess lifecycle without taking scheduler, spend, executor-policy, sandbox, verifier, or recovery-state authority.

### Packet PE7-AC1-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC0-TRACE-ORDER-FREEZE-1

**Class:** `CONTRACT`

**Outcome:** Freeze ProcessSupervisor interfaces, process identity, executable/args/env/cwd, stdio limits, timeout/cancel/kill/reap, child cleanup, outcome taxonomy, adapters, and caller migration order.

**Allowed delta:** Current-main contract expansion only; no process behavior change.

**Exit:** Exact allowed paths, API shape, failure mapping, ownership non-goals, conformance matrix, and rollback sequence.

**Stop:** Any process family requires incompatible authority, sandbox policy, retry policy, or unowned cleanup semantics.
### Packet PE7-AC1-SUPERVISOR-CORE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC1-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Add the shared supervisor core and typed process outcome behind existing behavior.

**Allowed delta:** Additive core only; existing callers remain on compatibility adapters and observed behavior stays golden-trace equivalent.

**Exit:** Focused timeout/cancel/kill/reap/output-bound tests prove no orphan child and no authority import.

**Stop:** Core needs scheduler/lease/spend ownership, changes retry semantics, or cannot preserve platform-specific cleanup.
### Packet PE7-AC1-CALLER-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC1-SUPERVISOR-CORE-1

**Class:** `IMPLEMENT`

**Outcome:** Migrate the AC0-enumerated caller groups to ProcessSupervisor and close the AC1 compatibility layer.

**Allowed delta:** Only enumerated callers and mechanically required tests/docs; no unlisted spawn site or legacy deletion outside the contract.

**Exit:** Zero direct production spawn outside approved supervisor internals, all golden traces/parity/restart tests pass, and AC2 receives refreshed exact owners.

**Stop:** An unlisted caller appears, behavior changes, orphan cleanup regresses, or a compatibility adapter still has production demand.
## Stage Architecture Convergence AC2 - typed execution

AC2 distinguishes effect and outcome states while leaving admission, leases, spend, verification, approval, output, and adoption with existing owners.

### Packet PE7-AC2-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC1-CALLER-MIGRATION-1

**Class:** `CONTRACT`

**Outcome:** Freeze the typed execution state/outcome/usage contract and executor-specific mapping table.

**Allowed delta:** No wire/schema or runtime change until compatibility and failure mappings are accepted.

**Exit:** Exact variants for admission, prepared, effect-not-started, effect-started, known/unknown outcome, cancellation, terminal failure, and evidence completeness.

**Stop:** A state cannot be derived from trustworthy owner evidence or would imply unsafe retry.
### Packet PE7-AC2-BOUNDARY-CORE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC2-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement the typed boundary and adapters without migrating all callers.

**Allowed delta:** Additive types/mappers only; no second executor, journal, scheduler, budget, or public behavior owner.

**Exit:** Exhaustive mapping tests, unknown-outcome negative tests, serialization compatibility where applicable, and no caller-visible semantic drift.

**Stop:** Mapping guesses effect status, drops usage uncertainty, or requires evaluator/verifier policy.
### Packet PE7-AC2-CALLER-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC2-BOUNDARY-CORE-1

**Class:** `IMPLEMENT`

**Outcome:** Migrate enumerated executors/callers and remove only superseded internal result plumbing approved by the contract.

**Allowed delta:** Mechanical caller migration and local compatibility cleanup only.

**Exit:** All production execution paths emit the typed boundary, outcome unknown stays non-success/non-retry, and AC3 receives refreshed golden traces.

**Stop:** A caller has unclassified semantics, public compatibility breaks, or removal reaches beyond the approved internal surface.
## Stage Architecture Convergence AC3 - Golden Path responsibility split

AC3 separates orchestration, store mutation, and external effects without changing state-machine or authority order.

### Packet PE7-AC3-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC2-CALLER-MIGRATION-1

**Class:** `CONTRACT`

**Outcome:** Freeze the Golden Path responsibility matrix, state transitions, audit identities, pure inputs/outputs, effect ports, store commands, and migration sequence.

**Allowed delta:** No endpoint, state, persistence, Provider, approval, output, or terminal behavior change.

**Exit:** A file-level extraction contract with golden-trace equivalence and exact forbidden ownership imports.

**Stop:** Responsibility cannot be separated without changing authority order or creating a second state machine.
### Packet PE7-AC3-ORCHESTRATOR-CORE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC3-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Extract the pure orchestration decision layer behind current entrypoints.

**Allowed delta:** Pure computation and compatibility façade only; no direct store or external effect in the extracted core.

**Exit:** Deterministic transition-table tests and replayed golden traces match prior behavior.

**Stop:** The core needs ambient environment, credentials, transactions, network, filesystem effects, or mutable global state.
### Packet PE7-AC3-PORT-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC3-ORCHESTRATOR-CORE-1

**Class:** `IMPLEMENT`

**Outcome:** Route store mutations and external effects through the accepted ports and migrate existing entrypoints.

**Allowed delta:** Adapter/migration only; existing store/effect owners retain authority and public behavior.

**Exit:** End-to-end, restart, idempotency, audit, terminal, cleanup, and output traces remain equivalent with no duplicate path.

**Stop:** An adapter becomes a policy owner, transaction ordering changes, or old and new effect paths can both execute.
## Stage Architecture Convergence AC4 - transaction views

AC4 adds borrowed domain views over one underlying LocalProductStore transaction; views never become repositories or stores.

### Packet PE7-AC4-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC3-PORT-MIGRATION-1

**Class:** `CONTRACT`

**Outcome:** Freeze only the repeated cross-domain mutation groups that justify transaction views, including borrow/commit/rollback rules and backend parity.

**Allowed delta:** No schema or transaction behavior change.

**Exit:** Exact WorkflowTx/ProductTaskTx/ManagedAcceptanceTx/RweTx method list, call sites, invariants, and forbidden nested commits.

**Stop:** A proposed view owns policy, caching, queuing, independent connection/commit, or cannot map across both backends.
### Packet PE7-AC4-VIEWS-CORE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC4-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement the accepted borrowed transaction views for SQLite and PostgreSQL.

**Allowed delta:** Additive internal API only; same underlying transaction/connection, locks, audit, and rollback semantics.

**Exit:** Backend-focused atomicity, rollback, failure injection, idempotency, and no-nested-commit tests.

**Stop:** Backend semantics diverge, a view can outlive/commit independently, or migration would require destructive schema change.
### Packet PE7-AC4-CALLER-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC4-VIEWS-CORE-1

**Class:** `IMPLEMENT`

**Outcome:** Migrate only the contract-enumerated cross-domain callers and close redundant transaction plumbing.

**Allowed delta:** Mechanical migration; no new repository abstraction or broad store rewrite.

**Exit:** SQLite/PostgreSQL parity, concurrent/restart traces, one atomic commit boundary, and no remaining approved duplicate mutation path.

**Stop:** A caller requires different atomicity, a compatibility path stays active, or lock ordering/deadlock risk changes.
## Stage Architecture Convergence AC5 - composition root

AC5 centralizes parsing, validation, and dependency construction while keeping external-effect modes explicit and default-off.

### Packet PE7-AC5-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC4-CALLER-MIGRATION-1

**Class:** `CONTRACT`

**Outcome:** Freeze configuration sources, precedence, validated types, dependency graph, runtime modes, secret-resolution boundary, and module migration batches.

**Allowed delta:** No configuration behavior change and no new environment variable.

**Exit:** One composition manifest with exact defaults, conflicts, validation errors, owner paths, and staged rollback.

**Stop:** Two accepted sources conflict, a secret would move earlier than the send boundary, or a module requires service-locator/global-registry behavior.
### Packet PE7-AC5-ROOT-CORE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC5-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement validated configuration/dependency construction in the existing Rust startup composition surface.

**Allowed delta:** Additive root and compatibility injection only; no module migration or mode-default change beyond the contract.

**Exit:** Deterministic parse/validation/conflict/default-off tests and no credential persistence/logging.

**Stop:** Root takes runtime policy owned elsewhere, requires mutable globals, or cannot reproduce accepted startup behavior.
### Packet PE7-AC5-MODULE-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC5-ROOT-CORE-1

**Class:** `IMPLEMENT`

**Outcome:** Migrate contract-enumerated modules off independent shared environment/config interpretation and remove approved legacy reads.

**Allowed delta:** Mechanical dependency injection and local cleanup only.

**Exit:** Negative environment-read search, all runtime modes/golden traces pass, defaults stay off, and rollback restores the compatibility injection layer.

**Stop:** An unlisted module, hidden precedence rule, credential exposure, or externally visible mode change appears.
## Stage Architecture Convergence AC6 - schema projections

AC6 makes affected Rust contracts authoritative and migrates each consumer family separately.

### Packet PE7-AC6-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC5-MODULE-MIGRATION-1

**Class:** `CONTRACT`

**Outcome:** Freeze authoritative Rust types, wire/schema projections, compatibility matrix, version/deprecation window, migration ordering, and rollback.

**Allowed delta:** No field/type change until old-reader/new-writer and consumer impact are explicit.

**Exit:** Exact type/field/version manifest and generated-artifact ownership with no consumer-defined authority.

**Stop:** A consumer has incompatible semantics, destructive migration lacks recovery, or two schema owners remain.
### Packet PE7-AC6-RUST-CODEGEN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC6-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement the Rust source types and deterministic schema/codegen projections.

**Allowed delta:** Only contract-approved additive/versioned type changes and generator updates; consumers remain compatibility-backed.

**Exit:** Drift guard, deterministic regeneration, Rust/wire validation, and old-reader/new-writer tests pass.

**Stop:** Generated output is nondeterministic, hand-edited projection is required, or rollback cannot read persisted/API data.
### Packet PE7-AC6-SDK-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC6-RUST-CODEGEN-1

**Class:** `IMPLEMENT`

**Outcome:** Migrate SDK consumers to generated/versioned contracts.

**Allowed delta:** SDK projection/adapters/tests only; no backend authority or Dashboard change.

**Exit:** SDK compatibility and type tests pass with deprecated paths explicitly bounded.

**Stop:** SDK requires a divergent type owner, silent field reinterpretation, or immediate incompatible removal.
### Packet PE7-AC6-DASHBOARD-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC6-SDK-MIGRATION-1

**Class:** `IMPLEMENT`

**Outcome:** Migrate Dashboard data projections to the accepted generated/versioned contracts without presentation redesign.

**Allowed delta:** Data/type adapters and tests only; no workflow, evaluator, spend, approval, adoption, or output authority.

**Exit:** Typecheck/build/projection tests and representative old/new payload fixtures pass.

**Stop:** UI needs backend policy, schema ownership, or presentation-only PR #225 content to complete the migration.
### Packet PE7-AC6-COMPATIBILITY-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC6-DASHBOARD-MIGRATION-1

**Class:** `CLOSEOUT`

**Outcome:** Verify every affected producer/consumer, deprecation window, migration, rollback, and drift guard before AC7.

**Allowed delta:** Evidence and status only; no legacy deletion.

**Exit:** Zero unexplained drift, complete consumer matrix, accepted compatibility receipt, and exact AC7 removal candidates.

**Stop:** Any active reader/writer remains unknown or rollback cannot restore service/data compatibility.
## Stage Architecture Convergence AC7 - obsolete cleanup

AC7 deletes only surfaces proven obsolete by accepted inventory and completed migrations.

### Packet PE7-AC7-REMOVAL-MANIFEST-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC6-COMPATIBILITY-CLOSEOUT-1

**Class:** `CONTRACT`

**Outcome:** Freeze a deletion manifest grouped by one canonical owner and rollback point, with zero-caller proof and compatibility disposition per item.

**Allowed delta:** Reference searches and evidence only; no deletion.

**Exit:** Exact files/symbols/tests/docs to delete, replacement owner, negative searches, fixture/script/SDK/Dashboard/replay checks, and batch order.

**Stop:** Any production, recovery, replay, fixture, script, or consumer dependency remains.
### Packet PE7-AC7-CLEANUP-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC7-REMOVAL-MANIFEST-1

**Class:** `IMPLEMENT`

**Outcome:** Delete the approved obsolete batches and mechanically repair references.

**Allowed delta:** Deletion only; no new feature, owner, schema, abstraction, or behavior.

**Exit:** Every manifest item is removed or explicitly deferred; security/dead-surface, full tests, parity, and golden traces pass.

**Stop:** A hidden caller appears, deletion changes behavior, or one PR would cross owner/rollback groups not authorized for consolidation.
### Packet PE7-AC7-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC7-CLEANUP-1

**Class:** `CLOSEOUT`

**Outcome:** Independently verify convergence completeness and preserve the reconstructable pre/post Harness identities.

**Allowed delta:** Evidence/status only.

**Exit:** Accepted AC closeout receipt, zero unowned compatibility island, implementation-cost aggregation, rollback index, and contemporary-replay inputs.

**Stop:** An obsolete path still executes, golden traces differ without accepted reason, or old/new Harness reconstruction is incomplete.
## Stage Contemporary old/new RWE replay

The causal comparison uses reconstructable old/new Harnesses in one randomized/interleaved controlled time window.

### Packet PE7-RWE-CR-RECONSTRUCTION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC7-CLOSEOUT-1

**Class:** `IMPLEMENT`

**Outcome:** Rebuild and provider-free validate the frozen pre-AC Harness beside the accepted post-AC Harness under isolated identities.

**Allowed delta:** Reconstruction adapters/artifacts only; neither Harness behavior, corpus, evaluator, or Provider route changes.

**Exit:** Both Harnesses pass registered provider-free traces and bind exact binaries/config/toolchains without shared mutable state.

**Stop:** Old Harness cannot be reproduced, isolation fails, or compatibility shims change the measured behavior.
### Packet PE7-RWE-CR-PROTOCOL-PREFLIGHT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-CR-RECONSTRUCTION-1

**Class:** `CONTRACT`

**Outcome:** Freeze randomization/interleaving, allocation concealment, drift covariates, capacity, finite authorizations, and immediate preflight.

**Allowed delta:** No live execution or post-AC threshold change; reuse the pre-registered measurement protocol.

**Exit:** Zero-mismatch preflight and operator authorization packages for both arms in the same bounded window.

**Stop:** Provider/model/environment identity cannot be kept comparable, capacity causes arm-time confounding, or old/new evidence paths can collide.
### Packet PE7-RWE-CR-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-CR-PROTOCOL-PREFLIGHT-1

**Class:** `EFFECT`

**Outcome:** Execute the randomized/interleaved old/new replay exactly once under accepted global stop rules.

**Allowed delta:** Registered effects only; no arm-specific retry, schedule change, or protocol repair.

**Exit:** Complete blinded arm assignments, attempts, lifecycle costs, drift, review, failures, cleanup, and restricted/redacted evidence.

**Stop:** Allocation integrity breaks, drift exceeds registered bounds, one arm loses authority/capacity, outcome unknown occurs, or global stop fires.
### Packet PE7-RWE-CR-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-CR-RUN-1

**Class:** `CLOSEOUT`

**Outcome:** Estimate the registered AC effect and decide Harness-Evolution eligibility.

**Allowed delta:** Frozen analysis only; historical before/after evidence remains secondary.

**Exit:** Hard-gate-first uncertainty/Pareto receipt with GO, NO_GO, or INSUFFICIENT disposition and all drift/cost limitations.

**Stop:** Non-inferiority, reliability, lifecycle cost, comparability, or evidence-completeness gate fails.
## Stage Experiment control EC1 - identity, lineage, mutation

EC1 makes candidate provenance immutable before evaluator or selection work. It also freezes a causal-mutation evidence chain without creating a second failure-intelligence owner:

- `FailurePatternEvidenceV1` separates observed verifier/runtime facts, causal status (`unknown`, `hypothesized`, `supported`, or `disputed`), counterevidence, Harness addressability, and the exact mutable surface. Existing feedback traces, pattern detection, and outcome attribution are inputs, not causal authority.
- `MutationHypothesisManifestV1` binds one candidate to the failure evidence, exact proposed delta, expected improvement and regression surfaces, metric direction and threshold, preserved invariants, and a pre-registered evaluation plan before candidate execution.
- `PredictionOutcomeV1` is written only after evaluation by the existing evaluator path. It records actual deltas, missing or contradictory evidence, prediction error, and calibration. It is derived audit evidence, never admission, safety, Pareto-selection, parent-selection, or adoption authority.

All three records are redacted, hash-bound, replayable, and must retain `unknown` or `disputed` rather than converting uncertainty or model confidence into fact.

### Packet PE7-HE-EC1-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-CR-ANALYSIS-1

**Class:** `CONTRACT`

**Outcome:** Freeze active-Harness, candidate, parent, generator, lineage, mutation-family, identity-hash, invalidation, budget, `FailurePatternEvidenceV1`, `MutationHypothesisManifestV1`, and `PredictionOutcomeV1` bindings.

**Allowed delta:** No candidate generation, evaluation, or persistence change.

**Exit:** Exact identity/lineage and causal-evidence schemas plus a pre-registered mutation registry with ownership, redaction, counterevidence, addressability, and non-authority rules.

**Stop:** Identity or cause can be caller/model asserted, lineage can be rewritten, uncertainty cannot be represented, or mutation scope can reach evaluator/authority policy.
### Packet PE7-HE-EC1-IDENTITY-LINEAGE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC1-CONTRACT-1

**Class:** `IMPLEMENT`

**Outcome:** Implement immutable identity and lineage recording under existing artifact/store owners, including source identities for later causal manifests.

**Allowed delta:** Contract-approved records, hashes, validation, and projections only; no selection or adoption.

**Exit:** Tamper/replay/duplicate/restart/parity tests prove immutable ancestry, exact active-Harness binding, and no orphan causal-evidence reference.

**Stop:** Requires a second store, mutable ancestry, candidate-controlled identity, or destructive migration.
### Packet PE7-HE-EC1-CAUSAL-MANIFEST-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC1-IDENTITY-LINEAGE-1

**Class:** `IMPLEMENT`

**Outcome:** Implement validation and immutable persistence for source-bound failure-pattern evidence and pre-execution mutation hypotheses by extending the existing Harness-Evolution artifact/store owner.

**Allowed delta:** Redacted records, hashes, validation, and feedback-evidence adapters only; no candidate execution, evaluator result, selection, or admission-policy change.

**Exit:** Unknown/disputed cause, counterevidence, addressability, invariant, prediction, tamper, duplicate, restart, SQLite/PostgreSQL parity, forbidden-sensitive-field, and proposal-binding tests pass.

**Stop:** Requires a parallel failure-intelligence store, treats confidence as causal proof, permits post-execution hypothesis edits, or cannot distinguish observation from inference.
### Packet PE7-HE-EC1-MUTATION-REGISTRY-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC1-CAUSAL-MANIFEST-1

**Class:** `IMPLEMENT`

**Outcome:** Implement the accepted mutation-family registry and bounded generator adapters, requiring each generated candidate to bind an addressable causal manifest.

**Allowed delta:** Registry/adapters/tests only; no evaluator, parent-selection, spend, merge, or production authority.

**Exit:** Unknown family rejection, unaddressable-pattern rejection, hypothesis/delta digest binding, scope containment, deterministic seed binding, and complete lineage tests pass.

**Stop:** Generator can edit registry/policy/evaluator or escape its admitted Harness surface.
## Stage Experiment control EC2 - evaluator and holdout

EC2 seals evaluation authority and threat controls before candidate experiments.

### Packet PE7-HE-EC2-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC1-MUTATION-REGISTRY-1

**Class:** `CONTRACT`

**Outcome:** Freeze evaluator constellation, sealed holdout, reviewer policy, immutable labels, access classes, contamination/gaming/safety sentinels, invalidation, and evaluator-owned `PredictionOutcomeV1` derivation rules.

**Allowed delta:** No evaluator implementation or holdout access.

**Exit:** Threat model and exact evaluator/label/access/outcome manifest reusing existing verification/replay/scorecard/review owners, with prediction accuracy explicitly non-authoritative.

**Stop:** Candidate path can observe or mutate labels/rubric, sentinel independence is unprovable, or a second evaluator owner is proposed.
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
