# Future Route

Last updated: 2026-08-09.

This document is the sole long-horizon routing index. Every packet here is `BLOCKED_PREREQUISITE` and routing-only: its order, prerequisite, intended class, and bounded sketch are accepted planning context, not implementation or external-effect authority.

The current executable window and common execution contracts live in `docs/NEXT_DECISION.md`. Accepted truth lives in `docs/CURRENT_STATUS.md`; durable architecture and authority invariants live in `docs/ARCHITECTURE_BOOK.md`; current owners live in `docs/MODULE_MAP.md`; live PR, CI, and review facts come only from a fresh context capsule.

When an accepted predecessor closes, do not execute its successor from this file. Refresh remote `main`, reconcile any negative or insufficient disposition, remove exactly one eligible packet from this index, and expand its complete twelve-field contract in `docs/NEXT_DECISION.md`. Any unresolved value or duplicate packet identity is `DECISION_REQUIRED`.

## Weak-Agent Full-Course Contract

“Weak agent can complete the full course” means a T0/T1 worker can carry every deterministic step, emit an exact pause receipt at a real T2/T3 gate, and resume from that receipt without reconstructing hidden context. It never means that a cheap agent may choose architecture, statistical thresholds, evaluator rules, credentials, spend, adoption, release, or another person's approval.

For every packet, the worker follows this fixed loop:

1. refresh accepted `main`, run `uv run --no-project python scripts/project_context.py`, and bind the packet to the resulting main SHA;
2. prove the prerequisite's accepted receipt and disposition; a merely closed PR or nominally completed packet is insufficient;
3. load only `START_HERE.md`, the current status/window, this packet and its execution profile, the named owner sections, and exact source/tests; use CodeGraph before textual search;
4. revalidate every listed owner/path and every `[planned seam; not current code]` against that main; convert candidate paths into a closed allowed-path list in `docs/NEXT_DECISION.md`;
5. execute the class protocol and ordered work without changing frozen values, crossing a gate, or beginning a successor;
6. run every packet verification plus applicable repository baseline checks, record failures and consumed cost, and independently inspect the complete diff/evidence;
7. emit the handoff below, then stop or promote exactly one successor through the planning owner.

The bounded handoff is: `packet_id`, `accepted_main_sha`, `profile_id`, `worker_tier`, `prerequisite_receipt_ids`, `completed_step`, `state`, `blocker_code`, `changed_paths`, `verification_results`, `external_effect_count`, `authority_consumed`, `evidence_ids_and_sha256`, `rollback_state`, `next_owner`, `next_permitted_action`, and `forbidden_next_actions`. It contains no credential, prompt/output/transcript, private path, or unredacted repository content.

## Worker Tiers

| Tier | Work a fast/cheap agent may own | Required escalation |
|---|---|---|
| `T0` | Read-only inventory, CodeGraph call paths, deterministic matrices, negative search, digest comparison | T2 resolves omissions, ownership conflicts, and contract choices |
| `T1` | Exact mechanical implementation, migration, codegen, focused tests, docs, and presentation under a frozen contract | T2 owns schema/authority/evaluator/concurrency/recovery decisions and accepts the complete diff |
| `T2` | Primary planning/architecture, statistical and evaluator contracts, store/transaction/controller/recovery seams, independent closeout | T3 is still required for external effects, spend, human GO/NO-GO, adoption, release, or deployment |
| `T3` | Human/operator authority: finite Provider/training/target effects and explicit decisions | Never delegated to model output; one receipt authorizes only its exact named action |

A packet's tier is the highest tier that owns its decisive step. Lower-tier agents should still perform all preceding deterministic preparation and hand the exact pause receipt to that owner.

## Cheap-Agent Dispatch Protocol

Future profiles are not directly executable queue entries. The current plan parser already models `goal`, `allowed_paths`, `prerequisites`, `forbidden_changes`, `verification`, and `rollback`, and the claim-bound prompt transports all six fields. However, autonomous plan execution remains deliberately fail-closed with `plan_lane_deferred_until_terminal_owners`; CI, review, repair, and terminal ownership are not yet plan-aware. Do not remove that stop or treat this document as a claim token. Until a separately accepted packet closes those owners, the planning owner promotes one packet into `docs/NEXT_DECISION.md` and publishes bounded work through the existing Issue task lane.

Package one promoted packet into these waves, retaining one parent packet identity and one rollback point:

| Wave | Cheap-agent work | Write ownership | Gate |
|---|---|---|---|
| `W0 bind` | T0 refreshes main/capsule, predecessor receipt, exact paths, and overlap | none | T2 rejects stale or ambiguous frontier |
| `W1 inspect` | Parallel T0 agents produce owner/call graph, test/negative-case matrix, compatibility/migration matrix, and deletion/rollback inventory | none | T2 reconciles omissions and freezes the exact task split |
| `W2 red` | T1 test owner adds the smallest failing focused/negative tests under an explicit test-path allowlist | tests only | Red failure must match the frozen contract, not an unrelated defect |
| `W3 build` | One or more T1 implementers take disjoint code/generated/docs path sets; at most one writer owns any path | disjoint declared paths | No implementation begins before its relevant red test/contract gate |
| `W4 prove` | T0/T1 run focused and applicable full checks; independent T2 reviews complete diff, authority, recovery, and rollback | no new semantic edits | Findings return as one bounded repair batch while Draft |
| `W5 close` | T2 binds receipts, exact head, costs, unresolved objections, and next disposition | canonical docs only after acceptance | No successor or EFFECT starts automatically |

Every child Issue/task repeats—not merely links—the parent `packet_id`, accepted-main SHA, goal, exact owned `allowed_paths`, accepted prerequisite receipt IDs, forbidden changes, ordered slice, verification commands, rollback, expected artifact, blocker taxonomy, and forbidden next actions. It also states that other agents share the worktree, that unrelated/other-agent edits must not be reverted, and who owns integration. A task missing any field is not dispatchable.

Parallelism is allowed only inside one promoted provider-free packet and only for disjoint read or write ownership. Contract choices, shared schemas, transactions, evaluator rules, migrations, generated-source authority, and final integration remain serialized through T2. `EFFECT` and human-decision packets use W0/W1 preparation, then pause at T3; they are never split into independently runnable effect children. CI/review waiting may advance another safe task within the same packet, never a later packet.

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

## Promotion Contract

Every path below is a **promotion-time candidate**, not present edit authority. Promotion removes exactly one eligible packet from this file, reruns owner/caller inventory on accepted main, and expands it in `docs/NEXT_DECISION.md` with all twelve execution-readiness fields. The promoted packet must replace globs and AC0/contract references with exact files; bind exact evidence, versions, hashes, commands, rollback, and stop/resume owner; and state which profile facts remained valid or changed. A planned seam is a design target only: if no current canonical owner can contain it without a second authority, stop `DECISION_REQUIRED`.

Negative predecessor dispositions are routing inputs, not failures to hide. `NO_GO`, `DECLINE`, `DEFER`, `SATURATED`, `HARM`, `CONTROLLED_FAILURE`, `OUTCOME_UNKNOWN`, and `INSUFFICIENT` require route reconciliation before promotion. Optional memory/skill and R4-R6 branches never block the core route or Dashboard. Dashboard eligibility joins only accepted adoption and fixed-operator Meta dispositions; AC6 Dashboard data migration remains separate from the last presentation refresh.

## Stop and Resume Protocol

On any stop, the worker first prevents new effects, preserves store leases/receipts and restricted evidence, runs safe cleanup/compensation already owned by the active packet, and emits the bounded handoff with `state=DECISION_REQUIRED`, `BLOCKED_PREREQUISITE`, or `OUTCOME_UNKNOWN`. It must state whether an effect may already have occurred; unknown is never rewritten as zero. Resume must refresh main and external state, verify the same packet/evidence/authority identities and rollback state, and continue only from the named `next_permitted_action`. A stale receipt, changed contract, expired authority, missing evidence, or changed effect status requires a new planning/operator decision; never replay the prior command speculatively.

## Execution Profile Field Contract

Each packet combines its existing `Prerequisite`, `Class`, `Outcome`, `Allowed delta`, `Exit`, and `Stop` with ten mandatory fields below. `Execution profile` must equal `{packet_id}.v1`; every narrative field must contain at least 20 characters, and `Ordered work` must expose its sequence with `->`. `Owner/seam` and `Allowed paths at promotion` identify where a planning owner must revalidate and narrow the work. `Ordered work`, `Verification`, and `Rollback/recovery` are the worker's runbook. `Worker tier` and `Human/effect gate` mark the pause boundary. `Consolidation boundary` prevents convenience from erasing review or rollback points. `Negative-result route` makes a non-favorable result directly completable rather than an invitation to improvise.

## Portfolio Inventory Manifest

The checked manifest below binds the complete ordered packet ID list, class/tier counts, dependency graph, and all state/profile/base-contract text. Any addition, removal, reorder, dependency change, profile edit, or generic-content replacement must deliberately refresh this manifest and appear as an independently reviewed planning diff; `scripts/check_agent_handoff.py` rejects silent drift.

<!-- future-route-inventory:v1
{
  "class_counts": {
    "CLOSEOUT": 24,
    "CONTRACT": 35,
    "EFFECT": 18,
    "IMPLEMENT": 39
  },
  "dependency_graph_sha256": "ae19f242436ae44c49e054b2a79ed7b086eb3e33b3595d304f4c6acd9cc37a70",
  "ordered_packet_ids": [
    "PE7-RWE-V2-VIABILITY-RUN-1",
    "PE7-RWE-V2-VIABILITY-CLOSEOUT-1",
    "PE7-RWE-MR-ESTIMANDS-1",
    "PE7-RWE-MR-CORPUS-SAMPLING-1",
    "PE7-RWE-MR-OPERATIONS-EVIDENCE-1",
    "PE7-RWE-MR-PROTOCOL-FREEZE-1",
    "PE7-RWE-DB-SNAPSHOT-CORPUS-1",
    "PE7-RWE-DB-PREFLIGHT-1",
    "PE7-RWE-DB-RUN-1",
    "PE7-RWE-DB-ANALYSIS-1",
    "PE7-AC0-RUNTIME-INVENTORY-1",
    "PE7-AC0-DATA-CONTRACT-INVENTORY-1",
    "PE7-AC0-TRACE-ORDER-FREEZE-1",
    "PE7-AC1-CONTRACT-1",
    "PE7-AC1-SUPERVISOR-CORE-1",
    "PE7-AC1-CALLER-MIGRATION-1",
    "PE7-AC2-CONTRACT-1",
    "PE7-AC2-BOUNDARY-CORE-1",
    "PE7-AC2-CALLER-MIGRATION-1",
    "PE7-AC3-CONTRACT-1",
    "PE7-AC3-ORCHESTRATOR-CORE-1",
    "PE7-AC3-PORT-MIGRATION-1",
    "PE7-AC4-CONTRACT-1",
    "PE7-AC4-VIEWS-CORE-1",
    "PE7-AC4-CALLER-MIGRATION-1",
    "PE7-AC5-CONTRACT-1",
    "PE7-AC5-ROOT-CORE-1",
    "PE7-AC5-MODULE-MIGRATION-1",
    "PE7-AC6-CONTRACT-1",
    "PE7-AC6-RUST-CODEGEN-1",
    "PE7-AC6-SDK-MIGRATION-1",
    "PE7-AC6-DASHBOARD-MIGRATION-1",
    "PE7-AC6-COMPATIBILITY-CLOSEOUT-1",
    "PE7-AC7-REMOVAL-MANIFEST-1",
    "PE7-AC7-CLEANUP-1",
    "PE7-AC7-CLOSEOUT-1",
    "PE7-RWE-CR-RECONSTRUCTION-1",
    "PE7-RWE-CR-PROTOCOL-PREFLIGHT-1",
    "PE7-RWE-CR-RUN-1",
    "PE7-RWE-CR-ANALYSIS-1",
    "PE7-HE-EC1-CONTRACT-1",
    "PE7-HE-EC1-IDENTITY-LINEAGE-1",
    "PE7-HE-EC1-CAUSAL-MANIFEST-1",
    "PE7-HE-EC1-MUTATION-REGISTRY-1",
    "PE7-HE-EC2-CONTRACT-1",
    "PE7-HE-EC2-HOLDOUT-SEAL-1",
    "PE7-HE-EC2-SENTINEL-CONFORMANCE-1",
    "PE7-HE-EC2-PREDICTION-OUTCOME-1",
    "PE7-HE-EC3-CONTRACT-1",
    "PE7-HE-EC3-INSTRUMENTATION-1",
    "PE7-HE-EC3-ENFORCEMENT-1",
    "PE7-HE-EC4-CONTRACT-1",
    "PE7-HE-EC4-ADMISSION-1",
    "PE7-HE-EC4-COVERAGE-CLOSEOUT-1",
    "PE7-HE-EC5-CONTRACT-1",
    "PE7-HE-EC5-SELECTION-ARCHIVE-1",
    "PE7-HE-EC5-STOP-RECOVERY-1",
    "PE7-HE-LEVEL1-PREFLIGHT-1",
    "PE7-HE-LEVEL1-RUN-1",
    "PE7-HE-LEVEL1-CLOSEOUT-1",
    "PE7-HE-LEVEL1-TRANSFER-PROTOCOL-1",
    "PE7-HE-LEVEL1-TRANSFER-RUN-1",
    "PE7-HE-LEVEL1-TRANSFER-ANALYSIS-1",
    "PE7-MEMORY-SKILL-CONTRACT-1",
    "PE7-MEMORY-ADAPTER-1",
    "PE7-SKILL-ADAPTER-1",
    "PE7-MEMORY-SKILL-RUN-1",
    "PE7-MEMORY-SKILL-ANALYSIS-1",
    "PE7-HE-LEVEL2-RULE-AUDIT-1",
    "PE7-HE-LEVEL2-EVIDENCE-ANALYSIS-1",
    "PE7-HE-LEVEL2-DECISION-1",
    "PE7-HE-LEVEL2-CONTROLLER-CONTRACT-1",
    "PE7-HE-LEVEL2-STATE-PERSISTENCE-1",
    "PE7-HE-LEVEL2-GENERATION-ORCHESTRATION-1",
    "PE7-HE-LEVEL2-EVALUATION-SELECTION-1",
    "PE7-HE-LEVEL2-STOP-RECOVERY-1",
    "PE7-HE-LEVEL2-SIMULATION-1",
    "PE7-HE-LEVEL2-PILOT-1",
    "PE7-HE-LEVEL2-CLOSEOUT-1",
    "PE7-HE-FINAL-TRANSFER-PROTOCOL-1",
    "PE7-HE-FINAL-TRANSFER-RUN-1",
    "PE7-HE-FINAL-TRANSFER-ANALYSIS-1",
    "PE7-HE-ADOPTION-READINESS-1",
    "PE7-HE-ADOPTION-DECISION-1",
    "PE7-HE-META-CLAIM-PROTOCOL-1",
    "PE7-HE-META-OPERATOR-CONTRACT-1",
    "PE7-HE-META-CORPUS-EVALUATOR-1",
    "PE7-HE-META-BUDGET-CONTRACT-1",
    "PE7-HE-META-O0-BASELINE-1",
    "PE7-HE-META-O1-CANDIDATE-1",
    "PE7-HE-META-FIXTURE-PILOT-1",
    "PE7-HE-META-PILOT-CLOSEOUT-1",
    "PE7-HE-META-COMPARISON-RUN-1",
    "PE7-HE-META-REPLICATION-RUN-1",
    "PE7-HE-META-ANALYSIS-DECISION-1",
    "PE7-HE-ADVANCED-RECURSION-GATE-1",
    "PE7-HE-R4-METACOGNITIVE-CONTRACT-1",
    "PE7-HE-R4-METACOGNITIVE-ADAPTER-1",
    "PE7-HE-R4-COMPARISON-RUN-1",
    "PE7-HE-R4-REPLICATION-RUN-1",
    "PE7-HE-R4-ANALYSIS-DECISION-1",
    "PE7-HE-R5-WEIGHT-CONTRACT-1",
    "PE7-HE-R5-WEIGHT-ADAPTER-1",
    "PE7-HE-R5-FACTORIAL-RUN-1",
    "PE7-HE-R5-FACTORIAL-ANALYSIS-1",
    "PE7-HE-R5-COEVOLUTION-RUN-1",
    "PE7-HE-R5-TRANSFER-REPLICATION-1",
    "PE7-HE-R5-ANALYSIS-DECISION-1",
    "PE7-HE-R6-OUTER-POLICY-CONTRACT-1",
    "PE7-HE-R6-OUTER-POLICY-ADAPTER-1",
    "PE7-HE-R6-COMPARISON-RUN-1",
    "PE7-HE-R6-REPLICATION-RUN-1",
    "PE7-HE-R6-ANALYSIS-DECISION-1",
    "PE7-DASHBOARD-DISPOSITION-1",
    "PE7-DASHBOARD-REFRESH-1",
    "PE7-DASHBOARD-CLOSEOUT-1"
  ],
  "ordered_packet_ids_sha256": "ff274072ea6faa0872caccd5f6911f05103eaa8d5947c0164f46a6fa91a0eb0a",
  "packet_count": 116,
  "profile_contracts_sha256": "e29606c4e719f7ea90930c684084369c612ce387c29a45b06d3e425bb0b42095",
  "schema_version": "future_route_inventory.v1",
  "worker_tier_counts": {
    "T0": 2,
    "T1": 19,
    "T2": 74,
    "T3": 21
  }
}
-->

## Stage RWE v2 viability

These packets prove lifecycle viability only. They do not authorize Architecture Convergence or an economic-improvement claim.

### Packet PE7-RWE-V2-VIABILITY-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-V2-VIABILITY-PREFLIGHT-1

**Class:** `EFFECT`

**Execution profile:** `PE7-RWE-V2-VIABILITY-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing `operator_corpus`, `execution_schedule`, `live_baseline_coordinator`, `runner`, and LocalProductStore `rwe_authority` owners; revalidate their exact symbols on promotion.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Recompute v2 corpus/protocol/schedule hashes; validate store-owned authorization, four-cell identities, usage/cost, cleanup, and redaction; run focused RWE plus SQLite/PostgreSQL parity checks when code/store paths are touched.

**Rollback/recovery:** Never delete or rewrite consumed run evidence; revert only analysis/status deltas, retain restricted raw evidence, and resume only from the store-owned terminal/lease state.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. A fresh T3 one-use spend/effect authorization is required only for the run packet.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** Classify `CONTROLLED_FAILURE`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT`; do not retry or advance to measurement readiness until an accepted closeout explicitly permits it.

**Outcome:** Issue one new finite one-use authorization and execute exactly the accepted four-cell v2 schedule once.

**Allowed delta:** Only the pre-authorized Provider effects and existing delegated lifecycle may occur. No code, corpus, protocol, schedule, budget, seed, reviewer, verifier, or target-default-branch change.

**Exit:** All four cells reach honest terminal classifications with complete request journal, usage/cost, cleanup, artifact/output, and restricted raw-evidence bindings.

**Stop:** Authority or hash mismatch, duplicate/stale identity, outcome unknown, budget breach, Provider/model drift, evidence-path failure, contamination, or target-default-branch risk.
### Packet PE7-RWE-V2-VIABILITY-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-V2-VIABILITY-RUN-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-RWE-V2-VIABILITY-CLOSEOUT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `operator_corpus`, `execution_schedule`, `live_baseline_coordinator`, `runner`, and LocalProductStore `rwe_authority` owners; revalidate their exact symbols on promotion.

**Allowed paths at promotion:** Read existing `engine/src/rwe/**`, `engine/src/storage/local_product_store/rwe_authority.rs`, and `engine/src/bin/rwe_live_baseline.rs`; an EFFECT changes no source, while closeout may change only accepted evidence/status paths named at promotion.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Recompute v2 corpus/protocol/schedule hashes; validate store-owned authorization, four-cell identities, usage/cost, cleanup, and redaction; run focused RWE plus SQLite/PostgreSQL parity checks when code/store paths are touched.

**Rollback/recovery:** Never delete or rewrite consumed run evidence; revert only analysis/status deltas, retain restricted raw evidence, and resume only from the store-owned terminal/lease state.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. A fresh T3 one-use spend/effect authorization is required only for the run packet.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** Classify `CONTROLLED_FAILURE`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT`; do not retry or advance to measurement readiness until an accepted closeout explicitly permits it.

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

**Execution profile:** `PE7-RWE-MR-ESTIMANDS-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing RWE corpus/protocol/schedule and artifact/evidence owners; the measurement specification is a planned contract artifact, not a second evaluator or runtime owner, and must be revalidated on promotion.

**Allowed paths at promotion:** `docs/NEXT_DECISION.md`, the relevant measurement sections of `docs/ARCHITECTURE_BOOK.md`, and a versioned artifact under the existing `engine/rwe/corpora/**` owner only after the contract packet names the exact file.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Schema/hash validation, deterministic dry analysis on synthetic missing/failure cases, contamination and retention negative cases, handoff/security checks, and independent statistical review before freeze.

**Rollback/recovery:** Revert the unobserved contract artifact and docs together; once outcome data is observed, never mutate the protocol—issue a superseding decision packet instead.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. T2 owns statistical, evaluator, retention, and evidence-policy choices; no Provider or target effect is authorized.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Return `DECISION_REQUIRED` for unresolved estimands, sample method, reviewer/retention owner, margins, or evidence access; do not materialize or run the decision baseline.

**Outcome:** Freeze the decision question, primary estimands, hard-gate outcomes, inferential unit, eligible value bases, minimum meaningful effects, non-inferiority margins, and missing/outcome-unknown rules.

**Allowed delta:** Planning evidence only. Repetitions remain nested measurements, not independent tasks; scalar summaries cannot override hard gates.

**Exit:** An independently reviewed estimand ledger with every threshold source, uncertainty target, and human value judgment explicit.

**Stop:** A threshold is chosen from favorable observed direction, value semantics are incomparable, or an authority-critical value lacks an owner.
### Packet PE7-RWE-MR-CORPUS-SAMPLING-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-MR-ESTIMANDS-1

**Class:** `CONTRACT`

**Execution profile:** `PE7-RWE-MR-CORPUS-SAMPLING-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing RWE corpus/protocol/schedule and artifact/evidence owners; the measurement specification is a planned contract artifact, not a second evaluator or runtime owner, and must be revalidated on promotion.

**Allowed paths at promotion:** `docs/NEXT_DECISION.md`, the relevant measurement sections of `docs/ARCHITECTURE_BOOK.md`, and a versioned artifact under the existing `engine/rwe/corpora/**` owner only after the contract packet names the exact file.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Schema/hash validation, deterministic dry analysis on synthetic missing/failure cases, contamination and retention negative cases, handoff/security checks, and independent statistical review before freeze.

**Rollback/recovery:** Revert the unobserved contract artifact and docs together; once outcome data is observed, never mutate the protocol—issue a superseding decision packet instead.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. T2 owns statistical, evaluator, retention, and evidence-policy choices; no Provider or target effect is authorized.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Return `DECISION_REQUIRED` for unresolved estimands, sample method, reviewer/retention owner, margins, or evidence access; do not materialize or run the decision baseline.

**Outcome:** Freeze task-family strata, repositories/languages/difficulty coverage, inclusion/exclusion, contamination screening, repetition nesting, sample-size method, and maximum experiment envelope.

**Allowed delta:** No task execution. Viability variance may inform precision but cannot tune toward a favorable effect or substitute repeated cells for task coverage.

**Exit:** A versioned corpus-selection and sampling manifest with power/precision assumptions, sensitivity analysis, finite upper bound, and replacement rules fixed before outcomes.

**Stop:** Required coverage, spend ceiling, task availability, contamination control, or statistically defensible precision cannot be accepted.
### Packet PE7-RWE-MR-OPERATIONS-EVIDENCE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-MR-CORPUS-SAMPLING-1

**Class:** `CONTRACT`

**Execution profile:** `PE7-RWE-MR-OPERATIONS-EVIDENCE-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing RWE corpus/protocol/schedule and artifact/evidence owners; the measurement specification is a planned contract artifact, not a second evaluator or runtime owner, and must be revalidated on promotion.

**Allowed paths at promotion:** `docs/NEXT_DECISION.md`, the relevant measurement sections of `docs/ARCHITECTURE_BOOK.md`, and a versioned artifact under the existing `engine/rwe/corpora/**` owner only after the contract packet names the exact file.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Schema/hash validation, deterministic dry analysis on synthetic missing/failure cases, contamination and retention negative cases, handoff/security checks, and independent statistical review before freeze.

**Rollback/recovery:** Revert the unobserved contract artifact and docs together; once outcome data is observed, never mutate the protocol—issue a superseding decision packet instead.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. T2 owns statistical, evaluator, retention, and evidence-policy choices; no Provider or target effect is authorized.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Return `DECISION_REQUIRED` for unresolved estimands, sample method, reviewer/retention owner, margins, or evidence access; do not materialize or run the decision baseline.

**Outcome:** Freeze reviewer identity/blinding/disagreement rules, environment and drift capture, lifecycle-cost completeness, reconstructable Harness artifacts, and restricted raw/redacted retention/deletion/access policy.

**Allowed delta:** No Provider call or persistence schema change. Reuse existing artifact/evidence owners; define unavailable evidence honestly.

**Exit:** An operations/evidence manifest covering toolchain, dependencies, model-return identity, price source, runner, CI, human/review/rework/recovery cost, retention, and old-Harness reconstruction.

**Stop:** Reviewer independence, sensitive-evidence handling, environment reconstruction, cost completeness, or drift observation remains undefined.
### Packet PE7-RWE-MR-PROTOCOL-FREEZE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-MR-OPERATIONS-EVIDENCE-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-RWE-MR-PROTOCOL-FREEZE-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing RWE corpus/protocol/schedule and artifact/evidence owners; the measurement specification is a planned contract artifact, not a second evaluator or runtime owner, and must be revalidated on promotion.

**Allowed paths at promotion:** `docs/NEXT_DECISION.md`, the relevant measurement sections of `docs/ARCHITECTURE_BOOK.md`, and a versioned artifact under the existing `engine/rwe/corpora/**` owner only after the contract packet names the exact file.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Schema/hash validation, deterministic dry analysis on synthetic missing/failure cases, contamination and retention negative cases, handoff/security checks, and independent statistical review before freeze.

**Rollback/recovery:** Revert the unobserved contract artifact and docs together; once outcome data is observed, never mutate the protocol—issue a superseding decision packet instead.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. T2 owns statistical, evaluator, retention, and evidence-policy choices; no Provider or target effect is authorized.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** Return `DECISION_REQUIRED` for unresolved estimands, sample method, reviewer/retention owner, margins, or evidence access; do not materialize or run the decision baseline.

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

**Execution profile:** `PE7-RWE-DB-SNAPSHOT-CORPUS-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing RWE frozen-corpus, schedule, live coordinator, artifact, and LocalProductStore authority owners; revalidate snapshot and evidence seams against accepted main.

**Allowed paths at promotion:** `engine/rwe/corpora/**`, `engine/src/rwe/**`, existing RWE artifact/store tests, and the exact canonical docs/evidence locations selected by the frozen measurement contract; EFFECT packets change no source.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Byte/hash-identical snapshot reconstruction, allocation/preflight simulation, complete lifecycle-cost and missingness checks, focused RWE tests, parity where persistence is touched, and independent frozen-plan analysis.

**Rollback/recovery:** Revert materialization code/artifacts without deleting accepted snapshots; an executed baseline is immutable evidence and may only be closed out or marked outcome unknown.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. Every live run needs a T3 finite authorization; corpus construction and analysis remain provider-free.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** If sample, drift, contamination, authorization, or evidence completeness fails, close with `INSUFFICIENT`/`OUTCOME_UNKNOWN`; AC0 remains blocked.

**Outcome:** Materialize the frozen task artifacts and a reconstructable pre-AC Harness/config/toolchain snapshot under existing artifact owners.

**Allowed delta:** Provider-free artifact production only. Do not change task semantics, evaluator, budget, runtime owner, or accepted Harness behavior.

**Exit:** Hash-verified corpus and rebuildable old-Harness bundle whose provider-free golden traces match accepted main.

**Stop:** A task cannot be legally retained/replayed, snapshot reconstruction is nondeterministic, or artifact storage would create a second owner.
### Packet PE7-RWE-DB-PREFLIGHT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-DB-SNAPSHOT-CORPUS-1

**Class:** `CONTRACT`

**Execution profile:** `PE7-RWE-DB-PREFLIGHT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing RWE frozen-corpus, schedule, live coordinator, artifact, and LocalProductStore authority owners; revalidate snapshot and evidence seams against accepted main.

**Allowed paths at promotion:** `engine/rwe/corpora/**`, `engine/src/rwe/**`, existing RWE artifact/store tests, and the exact canonical docs/evidence locations selected by the frozen measurement contract; EFFECT packets change no source.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Byte/hash-identical snapshot reconstruction, allocation/preflight simulation, complete lifecycle-cost and missingness checks, focused RWE tests, parity where persistence is touched, and independent frozen-plan analysis.

**Rollback/recovery:** Revert materialization code/artifacts without deleting accepted snapshots; an executed baseline is immutable evidence and may only be closed out or marked outcome unknown.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. Every live run needs a T3 finite authorization; corpus construction and analysis remain provider-free.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** If sample, drift, contamination, authorization, or evidence completeness fails, close with `INSUFFICIENT`/`OUTCOME_UNKNOWN`; AC0 remains blocked.

**Outcome:** Run the complete provider-free baseline preflight and prepare finite per-run authorization packages.

**Allowed delta:** No Provider effect or result observation. Validate corpus/snapshot/protocol hashes, capacity, principals, target state, evidence destinations, and drift baseline.

**Exit:** Current zero-mismatch preflight receipts and explicit operator authorization requests bounded by the accepted experiment envelope.

**Stop:** Capacity, price, Provider identity, target safety, reviewer availability, retention, or any binding is stale or unavailable.
### Packet PE7-RWE-DB-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-DB-PREFLIGHT-1

**Class:** `EFFECT`

**Execution profile:** `PE7-RWE-DB-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing RWE frozen-corpus, schedule, live coordinator, artifact, and LocalProductStore authority owners; revalidate snapshot and evidence seams against accepted main.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Byte/hash-identical snapshot reconstruction, allocation/preflight simulation, complete lifecycle-cost and missingness checks, focused RWE tests, parity where persistence is touched, and independent frozen-plan analysis.

**Rollback/recovery:** Revert materialization code/artifacts without deleting accepted snapshots; an executed baseline is immutable evidence and may only be closed out or marked outcome unknown.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. Every live run needs a T3 finite authorization; corpus construction and analysis remain provider-free.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** If sample, drift, contamination, authorization, or evidence completeness fails, close with `INSUFFICIENT`/`OUTCOME_UNKNOWN`; AC0 remains blocked.

**Outcome:** Execute the frozen pre-AC decision baseline under the accepted allocation and finite authorizations.

**Allowed delta:** Only registered task executions and reviews. No selective rerun, hidden failure, task substitution, threshold change, or mid-run protocol repair.

**Exit:** Every scheduled unit is terminal with attempts, failures, usage, lifecycle cost, reviewer evidence, drift covariates, cleanup, and raw/redacted bundle bindings.

**Stop:** A registered global stop rule fires, comparability breaks, authority expires, outcome becomes unknown, contamination occurs, or evidence capture fails.
### Packet PE7-RWE-DB-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-DB-RUN-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-RWE-DB-ANALYSIS-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing RWE frozen-corpus, schedule, live coordinator, artifact, and LocalProductStore authority owners; revalidate snapshot and evidence seams against accepted main.

**Allowed paths at promotion:** `engine/rwe/corpora/**`, `engine/src/rwe/**`, existing RWE artifact/store tests, and the exact canonical docs/evidence locations selected by the frozen measurement contract; EFFECT packets change no source.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Byte/hash-identical snapshot reconstruction, allocation/preflight simulation, complete lifecycle-cost and missingness checks, focused RWE tests, parity where persistence is touched, and independent frozen-plan analysis.

**Rollback/recovery:** Revert materialization code/artifacts without deleting accepted snapshots; an executed baseline is immutable evidence and may only be closed out or marked outcome unknown.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. Every live run needs a T3 finite authorization; corpus construction and analysis remain provider-free.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** If sample, drift, contamination, authorization, or evidence completeness fails, close with `INSUFFICIENT`/`OUTCOME_UNKNOWN`; AC0 remains blocked.

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

**Execution profile:** `PE7-AC0-RUNTIME-INVENTORY-1.v1`

**Worker tier:** `T0`

**Owner/seam:** Current Rust runtime, LocalProductStore, wire/codegen/SDK/Dashboard, and test owners discovered from CodeGraph; AC0 creates manifests only and must not invent a convergence owner.

**Allowed paths at promotion:** Read-only inventory across `engine/src/**`, `engine/tests/**`, `wire_contract/**`, `codegen/**`, `sdk/**`, and `dashboard/src/**`; write only the exact versioned manifest and canonical docs selected on promotion.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** CodeGraph call-path coverage plus negative `rg` reconciliation, duplicate/zero-caller checks, dependency-cycle validation, trace replay, handoff/security checks, and independent omission review.

**Rollback/recovery:** Revert manifests/docs as one unit; no runtime rollback is needed because AC0 changes no behavior.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. T0 may collect evidence; T2 must freeze scope/order and resolve ownership conflicts.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Any unowned spawn, mutation group, schema projection, config source, legacy abstraction, or ordering conflict is `DECISION_REQUIRED`; AC1 cannot start from a partial inventory.

**Outcome:** Enumerate every production subprocess spawn/kill/reap site, executor adapter, environment/config read, timeout/cancellation path, and affected test fixture.

**Allowed delta:** Inventory and call-graph evidence only; no refactor or deletion.

**Exit:** A zero-unknown runtime/executor matrix with exact callers, owners, failure semantics, golden traces, and candidate migration groups.

**Stop:** A spawn/effect path cannot be classified, ownership conflicts, or static search disagrees with executable traces.
### Packet PE7-AC0-DATA-CONTRACT-INVENTORY-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC0-RUNTIME-INVENTORY-1

**Class:** `CONTRACT`

**Execution profile:** `PE7-AC0-DATA-CONTRACT-INVENTORY-1.v1`

**Worker tier:** `T0`

**Owner/seam:** Current Rust runtime, LocalProductStore, wire/codegen/SDK/Dashboard, and test owners discovered from CodeGraph; AC0 creates manifests only and must not invent a convergence owner.

**Allowed paths at promotion:** Read-only inventory across `engine/src/**`, `engine/tests/**`, `wire_contract/**`, `codegen/**`, `sdk/**`, and `dashboard/src/**`; write only the exact versioned manifest and canonical docs selected on promotion.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** CodeGraph call-path coverage plus negative `rg` reconciliation, duplicate/zero-caller checks, dependency-cycle validation, trace replay, handoff/security checks, and independent omission review.

**Rollback/recovery:** Revert manifests/docs as one unit; no runtime rollback is needed because AC0 changes no behavior.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. T0 may collect evidence; T2 must freeze scope/order and resolve ownership conflicts.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Any unowned spawn, mutation group, schema projection, config source, legacy abstraction, or ordering conflict is `DECISION_REQUIRED`; AC1 cannot start from a partial inventory.

**Outcome:** Enumerate Golden Path responsibilities, store transaction entries, schemas/codegen/SDK/Dashboard projections, config construction, and legacy abstractions.

**Allowed delta:** Inventory only. Do not introduce transaction views, schema sources, composition roots, or replacement modules.

**Exit:** One owner/caller/transaction/projection/legacy matrix with compatibility and rollback obligations for AC1-AC7.

**Stop:** A current owner is ambiguous, a legacy surface still has unknown callers, or SQLite/PostgreSQL behavior cannot be mapped.
### Packet PE7-AC0-TRACE-ORDER-FREEZE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC0-DATA-CONTRACT-INVENTORY-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-AC0-TRACE-ORDER-FREEZE-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Current Rust runtime, LocalProductStore, wire/codegen/SDK/Dashboard, and test owners discovered from CodeGraph; AC0 creates manifests only and must not invent a convergence owner.

**Allowed paths at promotion:** Read-only inventory across `engine/src/**`, `engine/tests/**`, `wire_contract/**`, `codegen/**`, `sdk/**`, and `dashboard/src/**`; write only the exact versioned manifest and canonical docs selected on promotion.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** CodeGraph call-path coverage plus negative `rg` reconciliation, duplicate/zero-caller checks, dependency-cycle validation, trace replay, handoff/security checks, and independent omission review.

**Rollback/recovery:** Revert manifests/docs as one unit; no runtime rollback is needed because AC0 changes no behavior.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. T0 may collect evidence; T2 must freeze scope/order and resolve ownership conflicts.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** Any unowned spawn, mutation group, schema projection, config source, legacy abstraction, or ordering conflict is `DECISION_REQUIRED`; AC1 cannot start from a partial inventory.

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

**Execution profile:** `PE7-AC1-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing per-caller subprocess/executor owners from AC0 plus a [planned seam; not current code] shared `ProcessSupervisor`; revalidate module placement and API before promotion.

**Allowed paths at promotion:** Only exact `engine/src/**` and `engine/tests/**` rows named by the accepted AC0 manifest and AC1 contract; no repository-wide migration glob becomes edit authority.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Spawn failure, bounded stdout/stderr, timeout, cancellation, process-tree kill/reap, late completion, crash/restart, environment/cwd, and behavior-compatibility tests plus full Rust/security checks.

**Rollback/recovery:** Keep the old caller path behind the accepted compatibility boundary until each migration batch passes; revert additive core or one enumerated batch without orphaning children.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Stop on an unenumerated caller, unverifiable child cleanup, platform behavior gap, or need for a second scheduler/executor owner; leave AC2 blocked.

**Outcome:** Freeze ProcessSupervisor interfaces, process identity, executable/args/env/cwd, stdio limits, timeout/cancel/kill/reap, child cleanup, outcome taxonomy, adapters, and caller migration order.

**Allowed delta:** Current-main contract expansion only; no process behavior change.

**Exit:** Exact allowed paths, API shape, failure mapping, ownership non-goals, conformance matrix, and rollback sequence.

**Stop:** Any process family requires incompatible authority, sandbox policy, retry policy, or unowned cleanup semantics.
### Packet PE7-AC1-SUPERVISOR-CORE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC1-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC1-SUPERVISOR-CORE-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing per-caller subprocess/executor owners from AC0 plus a [planned seam; not current code] shared `ProcessSupervisor`; revalidate module placement and API before promotion.

**Allowed paths at promotion:** Only exact `engine/src/**` and `engine/tests/**` rows named by the accepted AC0 manifest and AC1 contract; no repository-wide migration glob becomes edit authority.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Spawn failure, bounded stdout/stderr, timeout, cancellation, process-tree kill/reap, late completion, crash/restart, environment/cwd, and behavior-compatibility tests plus full Rust/security checks.

**Rollback/recovery:** Keep the old caller path behind the accepted compatibility boundary until each migration batch passes; revert additive core or one enumerated batch without orphaning children.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Stop on an unenumerated caller, unverifiable child cleanup, platform behavior gap, or need for a second scheduler/executor owner; leave AC2 blocked.

**Outcome:** Add the shared supervisor core and typed process outcome behind existing behavior.

**Allowed delta:** Additive core only; existing callers remain on compatibility adapters and observed behavior stays golden-trace equivalent.

**Exit:** Focused timeout/cancel/kill/reap/output-bound tests prove no orphan child and no authority import.

**Stop:** Core needs scheduler/lease/spend ownership, changes retry semantics, or cannot preserve platform-specific cleanup.
### Packet PE7-AC1-CALLER-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC1-SUPERVISOR-CORE-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC1-CALLER-MIGRATION-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing per-caller subprocess/executor owners from AC0 plus a [planned seam; not current code] shared `ProcessSupervisor`; revalidate module placement and API before promotion.

**Allowed paths at promotion:** Only exact `engine/src/**` and `engine/tests/**` rows named by the accepted AC0 manifest and AC1 contract; no repository-wide migration glob becomes edit authority.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Spawn failure, bounded stdout/stderr, timeout, cancellation, process-tree kill/reap, late completion, crash/restart, environment/cwd, and behavior-compatibility tests plus full Rust/security checks.

**Rollback/recovery:** Keep the old caller path behind the accepted compatibility boundary until each migration batch passes; revert additive core or one enumerated batch without orphaning children.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Stop on an unenumerated caller, unverifiable child cleanup, platform behavior gap, or need for a second scheduler/executor owner; leave AC2 blocked.

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

**Execution profile:** `PE7-AC2-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `executor_adapter`, `node_executor`, provider executor, CLI executor, and scheduler result owners plus a [planned seam; not current code] typed execution boundary; revalidate exact mappings.

**Allowed paths at promotion:** Only AC0/AC1-enumerated files under `engine/src/{provider,cli,scheduler}/**`, `engine/src/*executor*.rs`, and their exact tests, narrowed in the promoted contract.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Exhaustive executor outcome/usage mapping, unknown variant fail-closed, timeout/outcome-unknown compatibility, serialization/tamper tests, and full Rust plus integration checks.

**Rollback/recovery:** Retain accepted adapters until all named callers migrate; revert one mapping batch and preserve durable legacy outcome interpretation.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** An unmappable executor result, schema need, or semantic change outside the frozen table is `DECISION_REQUIRED`; do not coerce unknown states to success.

**Outcome:** Freeze the typed execution state/outcome/usage contract and executor-specific mapping table.

**Allowed delta:** No wire/schema or runtime change until compatibility and failure mappings are accepted.

**Exit:** Exact variants for admission, prepared, effect-not-started, effect-started, known/unknown outcome, cancellation, terminal failure, and evidence completeness.

**Stop:** A state cannot be derived from trustworthy owner evidence or would imply unsafe retry.
### Packet PE7-AC2-BOUNDARY-CORE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC2-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC2-BOUNDARY-CORE-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing `executor_adapter`, `node_executor`, provider executor, CLI executor, and scheduler result owners plus a [planned seam; not current code] typed execution boundary; revalidate exact mappings.

**Allowed paths at promotion:** Only AC0/AC1-enumerated files under `engine/src/{provider,cli,scheduler}/**`, `engine/src/*executor*.rs`, and their exact tests, narrowed in the promoted contract.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Exhaustive executor outcome/usage mapping, unknown variant fail-closed, timeout/outcome-unknown compatibility, serialization/tamper tests, and full Rust plus integration checks.

**Rollback/recovery:** Retain accepted adapters until all named callers migrate; revert one mapping batch and preserve durable legacy outcome interpretation.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** An unmappable executor result, schema need, or semantic change outside the frozen table is `DECISION_REQUIRED`; do not coerce unknown states to success.

**Outcome:** Implement the typed boundary and adapters without migrating all callers.

**Allowed delta:** Additive types/mappers only; no second executor, journal, scheduler, budget, or public behavior owner.

**Exit:** Exhaustive mapping tests, unknown-outcome negative tests, serialization compatibility where applicable, and no caller-visible semantic drift.

**Stop:** Mapping guesses effect status, drops usage uncertainty, or requires evaluator/verifier policy.
### Packet PE7-AC2-CALLER-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC2-BOUNDARY-CORE-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC2-CALLER-MIGRATION-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing `executor_adapter`, `node_executor`, provider executor, CLI executor, and scheduler result owners plus a [planned seam; not current code] typed execution boundary; revalidate exact mappings.

**Allowed paths at promotion:** Only AC0/AC1-enumerated files under `engine/src/{provider,cli,scheduler}/**`, `engine/src/*executor*.rs`, and their exact tests, narrowed in the promoted contract.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Exhaustive executor outcome/usage mapping, unknown variant fail-closed, timeout/outcome-unknown compatibility, serialization/tamper tests, and full Rust plus integration checks.

**Rollback/recovery:** Retain accepted adapters until all named callers migrate; revert one mapping batch and preserve durable legacy outcome interpretation.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** An unmappable executor result, schema need, or semantic change outside the frozen table is `DECISION_REQUIRED`; do not coerce unknown states to success.

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

**Execution profile:** `PE7-AC3-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `engine/src/product_golden_path.rs` and LocalProductStore `product_tasks` owners plus a [planned seam; not current code] pure orchestration decision layer; revalidate effect-port boundaries.

**Allowed paths at promotion:** `engine/src/product_golden_path.rs`, exact product-task/store modules, entrypoints, and focused tests named by the AC3 contract; no parallel workflow, approval, output, or audit owner.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Golden traces, role/authority negatives, exactly-once output, lease/restart/late-write, cleanup/compensation, SQLite/PostgreSQL parity, and full Product Golden Path tests.

**Rollback/recovery:** Migrate behind current entrypoints, preserve durable state compatibility, and revert one port batch to the prior owner without replaying an external effect.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Stop on ownership ambiguity, effect-before-authority, non-replayable transition, or schema change not frozen by the contract; AC4 remains blocked.

**Outcome:** Freeze the Golden Path responsibility matrix, state transitions, audit identities, pure inputs/outputs, effect ports, store commands, and migration sequence.

**Allowed delta:** No endpoint, state, persistence, Provider, approval, output, or terminal behavior change.

**Exit:** A file-level extraction contract with golden-trace equivalence and exact forbidden ownership imports.

**Stop:** Responsibility cannot be separated without changing authority order or creating a second state machine.
### Packet PE7-AC3-ORCHESTRATOR-CORE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC3-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC3-ORCHESTRATOR-CORE-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing `engine/src/product_golden_path.rs` and LocalProductStore `product_tasks` owners plus a [planned seam; not current code] pure orchestration decision layer; revalidate effect-port boundaries.

**Allowed paths at promotion:** `engine/src/product_golden_path.rs`, exact product-task/store modules, entrypoints, and focused tests named by the AC3 contract; no parallel workflow, approval, output, or audit owner.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Golden traces, role/authority negatives, exactly-once output, lease/restart/late-write, cleanup/compensation, SQLite/PostgreSQL parity, and full Product Golden Path tests.

**Rollback/recovery:** Migrate behind current entrypoints, preserve durable state compatibility, and revert one port batch to the prior owner without replaying an external effect.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Stop on ownership ambiguity, effect-before-authority, non-replayable transition, or schema change not frozen by the contract; AC4 remains blocked.

**Outcome:** Extract the pure orchestration decision layer behind current entrypoints.

**Allowed delta:** Pure computation and compatibility façade only; no direct store or external effect in the extracted core.

**Exit:** Deterministic transition-table tests and replayed golden traces match prior behavior.

**Stop:** The core needs ambient environment, credentials, transactions, network, filesystem effects, or mutable global state.
### Packet PE7-AC3-PORT-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC3-ORCHESTRATOR-CORE-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC3-PORT-MIGRATION-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing `engine/src/product_golden_path.rs` and LocalProductStore `product_tasks` owners plus a [planned seam; not current code] pure orchestration decision layer; revalidate effect-port boundaries.

**Allowed paths at promotion:** `engine/src/product_golden_path.rs`, exact product-task/store modules, entrypoints, and focused tests named by the AC3 contract; no parallel workflow, approval, output, or audit owner.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Golden traces, role/authority negatives, exactly-once output, lease/restart/late-write, cleanup/compensation, SQLite/PostgreSQL parity, and full Product Golden Path tests.

**Rollback/recovery:** Migrate behind current entrypoints, preserve durable state compatibility, and revert one port batch to the prior owner without replaying an external effect.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Stop on ownership ambiguity, effect-before-authority, non-replayable transition, or schema change not frozen by the contract; AC4 remains blocked.

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

**Execution profile:** `PE7-AC4-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Sole LocalProductStore owner in `engine/src/storage/local_product_store/**` plus [planned seams; not current code] borrowed transaction views for only AC0-proven mutation groups; revalidate both backends.

**Allowed paths at promotion:** Exact LocalProductStore modules, `schema.rs`, `migrations.rs`, PostgreSQL backend files, and parity tests named by the contract; never create a second store or generic transaction framework.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Atomic commit/rollback, borrow/lifetime boundary, nested-call refusal, deadlock/concurrency, crash/restart, idempotency, migration rollback, and SQLite/PostgreSQL parity tests.

**Rollback/recovery:** Keep old entrypoints until migrated; revert view/core and caller batch with the accepted migration rollback, never manual database surgery.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. T2 review is mandatory for transaction, schema, concurrency, and recovery decisions.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Stop if a mutation group is not truly atomic across owners, parity cannot be proved, or recovery requires broadening the store boundary.

**Outcome:** Freeze only the repeated cross-domain mutation groups that justify transaction views, including borrow/commit/rollback rules and backend parity.

**Allowed delta:** No schema or transaction behavior change.

**Exit:** Exact WorkflowTx/ProductTaskTx/ManagedAcceptanceTx/RweTx method list, call sites, invariants, and forbidden nested commits.

**Stop:** A proposed view owns policy, caching, queuing, independent connection/commit, or cannot map across both backends.
### Packet PE7-AC4-VIEWS-CORE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC4-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC4-VIEWS-CORE-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Sole LocalProductStore owner in `engine/src/storage/local_product_store/**` plus [planned seams; not current code] borrowed transaction views for only AC0-proven mutation groups; revalidate both backends.

**Allowed paths at promotion:** Exact LocalProductStore modules, `schema.rs`, `migrations.rs`, PostgreSQL backend files, and parity tests named by the contract; never create a second store or generic transaction framework.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Atomic commit/rollback, borrow/lifetime boundary, nested-call refusal, deadlock/concurrency, crash/restart, idempotency, migration rollback, and SQLite/PostgreSQL parity tests.

**Rollback/recovery:** Keep old entrypoints until migrated; revert view/core and caller batch with the accepted migration rollback, never manual database surgery.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. T2 review is mandatory for transaction, schema, concurrency, and recovery decisions.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Stop if a mutation group is not truly atomic across owners, parity cannot be proved, or recovery requires broadening the store boundary.

**Outcome:** Implement the accepted borrowed transaction views for SQLite and PostgreSQL.

**Allowed delta:** Additive internal API only; same underlying transaction/connection, locks, audit, and rollback semantics.

**Exit:** Backend-focused atomicity, rollback, failure injection, idempotency, and no-nested-commit tests.

**Stop:** Backend semantics diverge, a view can outlive/commit independently, or migration would require destructive schema change.
### Packet PE7-AC4-CALLER-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC4-VIEWS-CORE-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC4-CALLER-MIGRATION-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Sole LocalProductStore owner in `engine/src/storage/local_product_store/**` plus [planned seams; not current code] borrowed transaction views for only AC0-proven mutation groups; revalidate both backends.

**Allowed paths at promotion:** Exact LocalProductStore modules, `schema.rs`, `migrations.rs`, PostgreSQL backend files, and parity tests named by the contract; never create a second store or generic transaction framework.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Atomic commit/rollback, borrow/lifetime boundary, nested-call refusal, deadlock/concurrency, crash/restart, idempotency, migration rollback, and SQLite/PostgreSQL parity tests.

**Rollback/recovery:** Keep old entrypoints until migrated; revert view/core and caller batch with the accepted migration rollback, never manual database surgery.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. T2 review is mandatory for transaction, schema, concurrency, and recovery decisions.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Stop if a mutation group is not truly atomic across owners, parity cannot be proved, or recovery requires broadening the store boundary.

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

**Execution profile:** `PE7-AC5-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `engine/src/main.rs`, CLI/provider/store config, HTTP server state, and startup owners plus a [planned seam; not current code] validated Rust composition root; revalidate precedence and secret resolution.

**Allowed paths at promotion:** Exact startup/config modules under `engine/src/**` and tests enumerated by the AC5 contract; environment reads outside that manifest remain untouched until separately migrated.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Configuration precedence matrix, missing/invalid/secret-shaped inputs, mode graph, deterministic construction, startup failure, compatibility, and full server/runtime tests.

**Rollback/recovery:** Preserve old constructors behind one rollback point until all named modules migrate; revert a batch without changing stored config or exposing credentials.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Conflicting precedence, runtime-global reads not in AC0, cyclic dependencies, or secret material crossing the resolver boundary is `DECISION_REQUIRED`.

**Outcome:** Freeze configuration sources, precedence, validated types, dependency graph, runtime modes, secret-resolution boundary, and module migration batches.

**Allowed delta:** No configuration behavior change and no new environment variable.

**Exit:** One composition manifest with exact defaults, conflicts, validation errors, owner paths, and staged rollback.

**Stop:** Two accepted sources conflict, a secret would move earlier than the send boundary, or a module requires service-locator/global-registry behavior.
### Packet PE7-AC5-ROOT-CORE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC5-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC5-ROOT-CORE-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing `engine/src/main.rs`, CLI/provider/store config, HTTP server state, and startup owners plus a [planned seam; not current code] validated Rust composition root; revalidate precedence and secret resolution.

**Allowed paths at promotion:** Exact startup/config modules under `engine/src/**` and tests enumerated by the AC5 contract; environment reads outside that manifest remain untouched until separately migrated.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Configuration precedence matrix, missing/invalid/secret-shaped inputs, mode graph, deterministic construction, startup failure, compatibility, and full server/runtime tests.

**Rollback/recovery:** Preserve old constructors behind one rollback point until all named modules migrate; revert a batch without changing stored config or exposing credentials.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Conflicting precedence, runtime-global reads not in AC0, cyclic dependencies, or secret material crossing the resolver boundary is `DECISION_REQUIRED`.

**Outcome:** Implement validated configuration/dependency construction in the existing Rust startup composition surface.

**Allowed delta:** Additive root and compatibility injection only; no module migration or mode-default change beyond the contract.

**Exit:** Deterministic parse/validation/conflict/default-off tests and no credential persistence/logging.

**Stop:** Root takes runtime policy owned elsewhere, requires mutable globals, or cannot reproduce accepted startup behavior.
### Packet PE7-AC5-MODULE-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC5-ROOT-CORE-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC5-MODULE-MIGRATION-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing `engine/src/main.rs`, CLI/provider/store config, HTTP server state, and startup owners plus a [planned seam; not current code] validated Rust composition root; revalidate precedence and secret resolution.

**Allowed paths at promotion:** Exact startup/config modules under `engine/src/**` and tests enumerated by the AC5 contract; environment reads outside that manifest remain untouched until separately migrated.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Configuration precedence matrix, missing/invalid/secret-shaped inputs, mode graph, deterministic construction, startup failure, compatibility, and full server/runtime tests.

**Rollback/recovery:** Preserve old constructors behind one rollback point until all named modules migrate; revert a batch without changing stored config or exposing credentials.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Conflicting precedence, runtime-global reads not in AC0, cyclic dependencies, or secret material crossing the resolver boundary is `DECISION_REQUIRED`.

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

**Execution profile:** `PE7-AC6-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Authoritative Rust types plus existing `wire_contract`, `codegen/generate_wire_types.py`, SDK, and Dashboard data-projection owners; revalidate producer/consumer matrix before edits.

**Allowed paths at promotion:** Exact Rust type modules, `wire_contract/**`, `codegen/**`, `sdk/python/**`, `sdk/typescript/**`, and Dashboard data types/API consumers named by contract; presentation redesign is excluded.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Rust serialization/golden tests, `bash scripts/check_wire_codegen_drift.sh`, `bash scripts/verify_rust_typescript_stack.sh`, Python/TypeScript SDK tests, Dashboard typecheck/tests, compatibility and downgrade fixtures.

**Rollback/recovery:** Use the frozen compatibility/deprecation window and reversible migration order; restore prior generated artifacts and dual-read/write behavior as one reviewed rollback.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. Schema/version choices require T2; AC6 Dashboard work is data migration, not the final presentation refresh.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Unknown consumer, non-deterministic codegen, wire break, migration ambiguity, or presentation request is `DECISION_REQUIRED`; AC7 cannot delete compatibility early.

**Outcome:** Freeze authoritative Rust types, wire/schema projections, compatibility matrix, version/deprecation window, migration ordering, and rollback.

**Allowed delta:** No field/type change until old-reader/new-writer and consumer impact are explicit.

**Exit:** Exact type/field/version manifest and generated-artifact ownership with no consumer-defined authority.

**Stop:** A consumer has incompatible semantics, destructive migration lacks recovery, or two schema owners remain.
### Packet PE7-AC6-RUST-CODEGEN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC6-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC6-RUST-CODEGEN-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Authoritative Rust types plus existing `wire_contract`, `codegen/generate_wire_types.py`, SDK, and Dashboard data-projection owners; revalidate producer/consumer matrix before edits.

**Allowed paths at promotion:** Exact Rust type modules, `wire_contract/**`, `codegen/**`, `sdk/python/**`, `sdk/typescript/**`, and Dashboard data types/API consumers named by contract; presentation redesign is excluded.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Rust serialization/golden tests, `bash scripts/check_wire_codegen_drift.sh`, `bash scripts/verify_rust_typescript_stack.sh`, Python/TypeScript SDK tests, Dashboard typecheck/tests, compatibility and downgrade fixtures.

**Rollback/recovery:** Use the frozen compatibility/deprecation window and reversible migration order; restore prior generated artifacts and dual-read/write behavior as one reviewed rollback.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. Schema/version choices require T2; AC6 Dashboard work is data migration, not the final presentation refresh.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Unknown consumer, non-deterministic codegen, wire break, migration ambiguity, or presentation request is `DECISION_REQUIRED`; AC7 cannot delete compatibility early.

**Outcome:** Implement the Rust source types and deterministic schema/codegen projections.

**Allowed delta:** Only contract-approved additive/versioned type changes and generator updates; consumers remain compatibility-backed.

**Exit:** Drift guard, deterministic regeneration, Rust/wire validation, and old-reader/new-writer tests pass.

**Stop:** Generated output is nondeterministic, hand-edited projection is required, or rollback cannot read persisted/API data.
### Packet PE7-AC6-SDK-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC6-RUST-CODEGEN-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC6-SDK-MIGRATION-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Authoritative Rust types plus existing `wire_contract`, `codegen/generate_wire_types.py`, SDK, and Dashboard data-projection owners; revalidate producer/consumer matrix before edits.

**Allowed paths at promotion:** Exact Rust type modules, `wire_contract/**`, `codegen/**`, `sdk/python/**`, `sdk/typescript/**`, and Dashboard data types/API consumers named by contract; presentation redesign is excluded.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Rust serialization/golden tests, `bash scripts/check_wire_codegen_drift.sh`, `bash scripts/verify_rust_typescript_stack.sh`, Python/TypeScript SDK tests, Dashboard typecheck/tests, compatibility and downgrade fixtures.

**Rollback/recovery:** Use the frozen compatibility/deprecation window and reversible migration order; restore prior generated artifacts and dual-read/write behavior as one reviewed rollback.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. Schema/version choices require T2; AC6 Dashboard work is data migration, not the final presentation refresh.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Unknown consumer, non-deterministic codegen, wire break, migration ambiguity, or presentation request is `DECISION_REQUIRED`; AC7 cannot delete compatibility early.

**Outcome:** Migrate SDK consumers to generated/versioned contracts.

**Allowed delta:** SDK projection/adapters/tests only; no backend authority or Dashboard change.

**Exit:** SDK compatibility and type tests pass with deprecated paths explicitly bounded.

**Stop:** SDK requires a divergent type owner, silent field reinterpretation, or immediate incompatible removal.
### Packet PE7-AC6-DASHBOARD-MIGRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC6-SDK-MIGRATION-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC6-DASHBOARD-MIGRATION-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Authoritative Rust types plus existing `wire_contract`, `codegen/generate_wire_types.py`, SDK, and Dashboard data-projection owners; revalidate producer/consumer matrix before edits.

**Allowed paths at promotion:** Exact Rust type modules, `wire_contract/**`, `codegen/**`, `sdk/python/**`, `sdk/typescript/**`, and Dashboard data types/API consumers named by contract; presentation redesign is excluded.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Rust serialization/golden tests, `bash scripts/check_wire_codegen_drift.sh`, `bash scripts/verify_rust_typescript_stack.sh`, Python/TypeScript SDK tests, Dashboard typecheck/tests, compatibility and downgrade fixtures.

**Rollback/recovery:** Use the frozen compatibility/deprecation window and reversible migration order; restore prior generated artifacts and dual-read/write behavior as one reviewed rollback.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. Schema/version choices require T2; AC6 Dashboard work is data migration, not the final presentation refresh.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Unknown consumer, non-deterministic codegen, wire break, migration ambiguity, or presentation request is `DECISION_REQUIRED`; AC7 cannot delete compatibility early.

**Outcome:** Migrate Dashboard data projections to the accepted generated/versioned contracts without presentation redesign.

**Allowed delta:** Data/type adapters and tests only; no workflow, evaluator, spend, approval, adoption, or output authority.

**Exit:** Typecheck/build/projection tests and representative old/new payload fixtures pass.

**Stop:** UI needs backend policy, schema ownership, or presentation-only PR #225 content to complete the migration.
### Packet PE7-AC6-COMPATIBILITY-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC6-DASHBOARD-MIGRATION-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-AC6-COMPATIBILITY-CLOSEOUT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Authoritative Rust types plus existing `wire_contract`, `codegen/generate_wire_types.py`, SDK, and Dashboard data-projection owners; revalidate producer/consumer matrix before edits.

**Allowed paths at promotion:** Exact Rust type modules, `wire_contract/**`, `codegen/**`, `sdk/python/**`, `sdk/typescript/**`, and Dashboard data types/API consumers named by contract; presentation redesign is excluded.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Rust serialization/golden tests, `bash scripts/check_wire_codegen_drift.sh`, `bash scripts/verify_rust_typescript_stack.sh`, Python/TypeScript SDK tests, Dashboard typecheck/tests, compatibility and downgrade fixtures.

**Rollback/recovery:** Use the frozen compatibility/deprecation window and reversible migration order; restore prior generated artifacts and dual-read/write behavior as one reviewed rollback.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. Schema/version choices require T2; AC6 Dashboard work is data migration, not the final presentation refresh.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** Unknown consumer, non-deterministic codegen, wire break, migration ambiguity, or presentation request is `DECISION_REQUIRED`; AC7 cannot delete compatibility early.

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

**Execution profile:** `PE7-AC7-REMOVAL-MANIFEST-1.v1`

**Worker tier:** `T2`

**Owner/seam:** The accepted AC0 inventory and AC1-AC6 owners; AC7 deletes only zero-caller items in the frozen removal manifest and creates no replacement owner.

**Allowed paths at promotion:** Only exact files/symbols listed in the accepted removal manifest plus mechanically affected tests/docs; no broad directory deletion or guessed legacy cleanup.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** CodeGraph plus negative `rg` zero-caller proof, compatibility fixtures, full Rust/SDK/Dashboard checks as applicable, migration/recovery evidence, security/handoff, and diff review.

**Rollback/recovery:** One removal batch equals one revertable owner group; restore deleted compatibility code and its tests without rolling back unrelated accepted convergence.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Any live/dynamic caller, unresolved deprecation window, missing rollback, or behavior delta stops deletion and routes a new contract decision.

**Outcome:** Freeze a deletion manifest grouped by one canonical owner and rollback point, with zero-caller proof and compatibility disposition per item.

**Allowed delta:** Reference searches and evidence only; no deletion.

**Exit:** Exact files/symbols/tests/docs to delete, replacement owner, negative searches, fixture/script/SDK/Dashboard/replay checks, and batch order.

**Stop:** Any production, recovery, replay, fixture, script, or consumer dependency remains.
### Packet PE7-AC7-CLEANUP-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC7-REMOVAL-MANIFEST-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-AC7-CLEANUP-1.v1`

**Worker tier:** `T1`

**Owner/seam:** The accepted AC0 inventory and AC1-AC6 owners; AC7 deletes only zero-caller items in the frozen removal manifest and creates no replacement owner.

**Allowed paths at promotion:** Only exact files/symbols listed in the accepted removal manifest plus mechanically affected tests/docs; no broad directory deletion or guessed legacy cleanup.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** CodeGraph plus negative `rg` zero-caller proof, compatibility fixtures, full Rust/SDK/Dashboard checks as applicable, migration/recovery evidence, security/handoff, and diff review.

**Rollback/recovery:** One removal batch equals one revertable owner group; restore deleted compatibility code and its tests without rolling back unrelated accepted convergence.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Any live/dynamic caller, unresolved deprecation window, missing rollback, or behavior delta stops deletion and routes a new contract decision.

**Outcome:** Delete the approved obsolete batches and mechanically repair references.

**Allowed delta:** Deletion only; no new feature, owner, schema, abstraction, or behavior.

**Exit:** Every manifest item is removed or explicitly deferred; security/dead-surface, full tests, parity, and golden traces pass.

**Stop:** A hidden caller appears, deletion changes behavior, or one PR would cross owner/rollback groups not authorized for consolidation.
### Packet PE7-AC7-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AC7-CLEANUP-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-AC7-CLOSEOUT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** The accepted AC0 inventory and AC1-AC6 owners; AC7 deletes only zero-caller items in the frozen removal manifest and creates no replacement owner.

**Allowed paths at promotion:** Only exact files/symbols listed in the accepted removal manifest plus mechanically affected tests/docs; no broad directory deletion or guessed legacy cleanup.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** CodeGraph plus negative `rg` zero-caller proof, compatibility fixtures, full Rust/SDK/Dashboard checks as applicable, migration/recovery evidence, security/handoff, and diff review.

**Rollback/recovery:** One removal batch equals one revertable owner group; restore deleted compatibility code and its tests without rolling back unrelated accepted convergence.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** Any live/dynamic caller, unresolved deprecation window, missing rollback, or behavior delta stops deletion and routes a new contract decision.

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

**Execution profile:** `PE7-RWE-CR-RECONSTRUCTION-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing RWE corpus/schedule/coordinator/evidence owners and accepted pre/post Harness artifacts; add no benchmark runtime or evaluator owner, and revalidate both identities.

**Allowed paths at promotion:** Exact existing RWE/artifact paths and isolated reconstruction fixtures named by the protocol; EFFECT changes no source and writes only restricted evidence.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Hash-bound old/new reconstruction, deterministic allocation/interleaving simulation, drift and contamination negatives, full cost/missingness capture, RWE tests, and independent frozen analysis.

**Rollback/recovery:** Revert reconstruction helpers without deleting either Harness snapshot; executed comparisons are immutable and cannot be selectively rerun.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. The replay run requires T3 finite authority; reconstruction and analysis are provider-free and blinded where frozen.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Return `NO_GO`, `HARM`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT` when registered gates fail; do not enter EC1 or claim architecture causality.

**Outcome:** Rebuild and provider-free validate the frozen pre-AC Harness beside the accepted post-AC Harness under isolated identities.

**Allowed delta:** Reconstruction adapters/artifacts only; neither Harness behavior, corpus, evaluator, or Provider route changes.

**Exit:** Both Harnesses pass registered provider-free traces and bind exact binaries/config/toolchains without shared mutable state.

**Stop:** Old Harness cannot be reproduced, isolation fails, or compatibility shims change the measured behavior.
### Packet PE7-RWE-CR-PROTOCOL-PREFLIGHT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-CR-RECONSTRUCTION-1

**Class:** `CONTRACT`

**Execution profile:** `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing RWE corpus/schedule/coordinator/evidence owners and accepted pre/post Harness artifacts; add no benchmark runtime or evaluator owner, and revalidate both identities.

**Allowed paths at promotion:** Exact existing RWE/artifact paths and isolated reconstruction fixtures named by the protocol; EFFECT changes no source and writes only restricted evidence.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Hash-bound old/new reconstruction, deterministic allocation/interleaving simulation, drift and contamination negatives, full cost/missingness capture, RWE tests, and independent frozen analysis.

**Rollback/recovery:** Revert reconstruction helpers without deleting either Harness snapshot; executed comparisons are immutable and cannot be selectively rerun.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. The replay run requires T3 finite authority; reconstruction and analysis are provider-free and blinded where frozen.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Return `NO_GO`, `HARM`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT` when registered gates fail; do not enter EC1 or claim architecture causality.

**Outcome:** Freeze randomization/interleaving, allocation concealment, drift covariates, capacity, finite authorizations, and immediate preflight.

**Allowed delta:** No live execution or post-AC threshold change; reuse the pre-registered measurement protocol.

**Exit:** Zero-mismatch preflight and operator authorization packages for both arms in the same bounded window.

**Stop:** Provider/model/environment identity cannot be kept comparable, capacity causes arm-time confounding, or old/new evidence paths can collide.
### Packet PE7-RWE-CR-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-CR-PROTOCOL-PREFLIGHT-1

**Class:** `EFFECT`

**Execution profile:** `PE7-RWE-CR-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing RWE corpus/schedule/coordinator/evidence owners and accepted pre/post Harness artifacts; add no benchmark runtime or evaluator owner, and revalidate both identities.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Hash-bound old/new reconstruction, deterministic allocation/interleaving simulation, drift and contamination negatives, full cost/missingness capture, RWE tests, and independent frozen analysis.

**Rollback/recovery:** Revert reconstruction helpers without deleting either Harness snapshot; executed comparisons are immutable and cannot be selectively rerun.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. The replay run requires T3 finite authority; reconstruction and analysis are provider-free and blinded where frozen.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** Return `NO_GO`, `HARM`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT` when registered gates fail; do not enter EC1 or claim architecture causality.

**Outcome:** Execute the randomized/interleaved old/new replay exactly once under accepted global stop rules.

**Allowed delta:** Registered effects only; no arm-specific retry, schedule change, or protocol repair.

**Exit:** Complete blinded arm assignments, attempts, lifecycle costs, drift, review, failures, cleanup, and restricted/redacted evidence.

**Stop:** Allocation integrity breaks, drift exceeds registered bounds, one arm loses authority/capacity, outcome unknown occurs, or global stop fires.
### Packet PE7-RWE-CR-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-CR-RUN-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-RWE-CR-ANALYSIS-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing RWE corpus/schedule/coordinator/evidence owners and accepted pre/post Harness artifacts; add no benchmark runtime or evaluator owner, and revalidate both identities.

**Allowed paths at promotion:** Exact existing RWE/artifact paths and isolated reconstruction fixtures named by the protocol; EFFECT changes no source and writes only restricted evidence.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Hash-bound old/new reconstruction, deterministic allocation/interleaving simulation, drift and contamination negatives, full cost/missingness capture, RWE tests, and independent frozen analysis.

**Rollback/recovery:** Revert reconstruction helpers without deleting either Harness snapshot; executed comparisons are immutable and cannot be selectively rerun.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. The replay run requires T3 finite authority; reconstruction and analysis are provider-free and blinded where frozen.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** Return `NO_GO`, `HARM`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT` when registered gates fail; do not enter EC1 or claim architecture causality.

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

**Execution profile:** `PE7-HE-EC1-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `engine/src/harness_evolution.rs` and LocalProductStore `harness_evolution.rs` owners plus [planned seams; not current code] causal evidence and mutation-manifest types; revalidate placement.

**Allowed paths at promotion:** Those two owner files, exact artifact/schema/migration modules, and HE tests named by contract; no second artifact/store owner and no candidate-controlled identity.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Canonical hash/tamper, stale parent, duplicate lineage, invalidation, missing causal source, mutation-family allowlist, restart/parity, and provider-free generation fixture tests.

**Rollback/recovery:** Add types/records compatibly, preserve immutable prior identities, and roll back new admissions without deleting lineage or failure evidence.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. T2 owns identity/schema/causal contracts even when T1 performs mechanical serialization work.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Unaddressable causal evidence, mutable identity, unbounded generator, or store-owner conflict is `DECISION_REQUIRED`; EC2 remains blocked.

**Outcome:** Freeze active-Harness, candidate, parent, generator, lineage, mutation-family, identity-hash, invalidation, budget, `FailurePatternEvidenceV1`, `MutationHypothesisManifestV1`, and `PredictionOutcomeV1` bindings.

**Allowed delta:** No candidate generation, evaluation, or persistence change.

**Exit:** Exact identity/lineage and causal-evidence schemas plus a pre-registered mutation registry with ownership, redaction, counterevidence, addressability, and non-authority rules.

**Stop:** Identity or cause can be caller/model asserted, lineage can be rewritten, uncertainty cannot be represented, or mutation scope can reach evaluator/authority policy.
### Packet PE7-HE-EC1-IDENTITY-LINEAGE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC1-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-EC1-IDENTITY-LINEAGE-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `engine/src/harness_evolution.rs` and LocalProductStore `harness_evolution.rs` owners plus [planned seams; not current code] causal evidence and mutation-manifest types; revalidate placement.

**Allowed paths at promotion:** Those two owner files, exact artifact/schema/migration modules, and HE tests named by contract; no second artifact/store owner and no candidate-controlled identity.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Canonical hash/tamper, stale parent, duplicate lineage, invalidation, missing causal source, mutation-family allowlist, restart/parity, and provider-free generation fixture tests.

**Rollback/recovery:** Add types/records compatibly, preserve immutable prior identities, and roll back new admissions without deleting lineage or failure evidence.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. T2 owns identity/schema/causal contracts even when T1 performs mechanical serialization work.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Unaddressable causal evidence, mutable identity, unbounded generator, or store-owner conflict is `DECISION_REQUIRED`; EC2 remains blocked.

**Outcome:** Implement immutable identity and lineage recording under existing artifact/store owners, including source identities for later causal manifests.

**Allowed delta:** Contract-approved records, hashes, validation, and projections only; no selection or adoption.

**Exit:** Tamper/replay/duplicate/restart/parity tests prove immutable ancestry, exact active-Harness binding, and no orphan causal-evidence reference.

**Stop:** Requires a second store, mutable ancestry, candidate-controlled identity, or destructive migration.
### Packet PE7-HE-EC1-CAUSAL-MANIFEST-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC1-IDENTITY-LINEAGE-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-EC1-CAUSAL-MANIFEST-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `engine/src/harness_evolution.rs` and LocalProductStore `harness_evolution.rs` owners plus [planned seams; not current code] causal evidence and mutation-manifest types; revalidate placement.

**Allowed paths at promotion:** Those two owner files, exact artifact/schema/migration modules, and HE tests named by contract; no second artifact/store owner and no candidate-controlled identity.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Canonical hash/tamper, stale parent, duplicate lineage, invalidation, missing causal source, mutation-family allowlist, restart/parity, and provider-free generation fixture tests.

**Rollback/recovery:** Add types/records compatibly, preserve immutable prior identities, and roll back new admissions without deleting lineage or failure evidence.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. T2 owns identity/schema/causal contracts even when T1 performs mechanical serialization work.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Unaddressable causal evidence, mutable identity, unbounded generator, or store-owner conflict is `DECISION_REQUIRED`; EC2 remains blocked.

**Outcome:** Implement validation and immutable persistence for source-bound failure-pattern evidence and pre-execution mutation hypotheses by extending the existing Harness-Evolution artifact/store owner.

**Allowed delta:** Redacted records, hashes, validation, and feedback-evidence adapters only; no candidate execution, evaluator result, selection, or admission-policy change.

**Exit:** Unknown/disputed cause, counterevidence, addressability, invariant, prediction, tamper, duplicate, restart, SQLite/PostgreSQL parity, forbidden-sensitive-field, and proposal-binding tests pass.

**Stop:** Requires a parallel failure-intelligence store, treats confidence as causal proof, permits post-execution hypothesis edits, or cannot distinguish observation from inference.
### Packet PE7-HE-EC1-MUTATION-REGISTRY-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC1-CAUSAL-MANIFEST-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-EC1-MUTATION-REGISTRY-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `engine/src/harness_evolution.rs` and LocalProductStore `harness_evolution.rs` owners plus [planned seams; not current code] causal evidence and mutation-manifest types; revalidate placement.

**Allowed paths at promotion:** Those two owner files, exact artifact/schema/migration modules, and HE tests named by contract; no second artifact/store owner and no candidate-controlled identity.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Canonical hash/tamper, stale parent, duplicate lineage, invalidation, missing causal source, mutation-family allowlist, restart/parity, and provider-free generation fixture tests.

**Rollback/recovery:** Add types/records compatibly, preserve immutable prior identities, and roll back new admissions without deleting lineage or failure evidence.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. T2 owns identity/schema/causal contracts even when T1 performs mechanical serialization work.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Unaddressable causal evidence, mutable identity, unbounded generator, or store-owner conflict is `DECISION_REQUIRED`; EC2 remains blocked.

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

**Execution profile:** `PE7-HE-EC2-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `engine/src/harness_evolution_eval.rs` fixture path and accepted evaluator/evidence owners; [planned seam; not current acceptance] sealed holdout mediation must extend, not replace, them.

**Allowed paths at promotion:** Exact evaluator, HE, store/artifact, and test paths selected by contract; candidate/generator code cannot edit evaluator rules, labels, holdout, or sentinels.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Seal/tamper/access, contamination/gaming/safety sentinel, blinding, immutable label, missingness, prediction-outcome derivation, restart/parity, and adversarial candidate tests.

**Rollback/recovery:** Keep fixture/default-off behavior until real controls pass; revoke new access and restore prior evaluator adapter while retaining audit/invalidation evidence.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. Independent T2 evaluator/security review is mandatory; no candidate or cheap worker may choose thresholds after outcomes.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Leakage, candidate influence, unverifiable labels, evaluator-owner duplication, or fixture-only evidence blocks EC3 and all improvement claims.

**Outcome:** Freeze evaluator constellation, sealed holdout, reviewer policy, immutable labels, access classes, contamination/gaming/safety sentinels, invalidation, and evaluator-owned `PredictionOutcomeV1` derivation rules.

**Allowed delta:** No evaluator implementation or holdout access.

**Exit:** Threat model and exact evaluator/label/access/outcome manifest reusing existing verification/replay/scorecard/review owners, with prediction accuracy explicitly non-authoritative.

**Stop:** Candidate path can observe or mutate labels/rubric, sentinel independence is unprovable, or a second evaluator owner is proposed.
### Packet PE7-HE-EC2-HOLDOUT-SEAL-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC2-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-EC2-HOLDOUT-SEAL-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `engine/src/harness_evolution_eval.rs` fixture path and accepted evaluator/evidence owners; [planned seam; not current acceptance] sealed holdout mediation must extend, not replace, them.

**Allowed paths at promotion:** Exact evaluator, HE, store/artifact, and test paths selected by contract; candidate/generator code cannot edit evaluator rules, labels, holdout, or sentinels.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Seal/tamper/access, contamination/gaming/safety sentinel, blinding, immutable label, missingness, prediction-outcome derivation, restart/parity, and adversarial candidate tests.

**Rollback/recovery:** Keep fixture/default-off behavior until real controls pass; revoke new access and restore prior evaluator adapter while retaining audit/invalidation evidence.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. Independent T2 evaluator/security review is mandatory; no candidate or cheap worker may choose thresholds after outcomes.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Leakage, candidate influence, unverifiable labels, evaluator-owner duplication, or fixture-only evidence blocks EC3 and all improvement claims.

**Outcome:** Materialize sealed holdout identities, labels, access mediation, audit, and invalidation controls.

**Allowed delta:** Access/seal/audit controls only; no candidate run or evaluator rule change.

**Exit:** Unauthorized-read, label-tamper, leakage, restart, audit, and deletion/rotation tests pass.

**Stop:** Raw sensitive content would be committed, candidate identity gains access, or seal cannot survive restart.
### Packet PE7-HE-EC2-SENTINEL-CONFORMANCE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC2-HOLDOUT-SEAL-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-EC2-SENTINEL-CONFORMANCE-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `engine/src/harness_evolution_eval.rs` fixture path and accepted evaluator/evidence owners; [planned seam; not current acceptance] sealed holdout mediation must extend, not replace, them.

**Allowed paths at promotion:** Exact evaluator, HE, store/artifact, and test paths selected by contract; candidate/generator code cannot edit evaluator rules, labels, holdout, or sentinels.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Seal/tamper/access, contamination/gaming/safety sentinel, blinding, immutable label, missingness, prediction-outcome derivation, restart/parity, and adversarial candidate tests.

**Rollback/recovery:** Keep fixture/default-off behavior until real controls pass; revoke new access and restore prior evaluator adapter while retaining audit/invalidation evidence.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. Independent T2 evaluator/security review is mandatory; no candidate or cheap worker may choose thresholds after outcomes.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Leakage, candidate influence, unverifiable labels, evaluator-owner duplication, or fixture-only evidence blocks EC3 and all improvement claims.

**Outcome:** Wire safety, contamination, and evaluator-gaming sentinels into the existing evaluator path.

**Allowed delta:** Sentinel observation/invalidation only; no scalar override or new evaluator.

**Exit:** Adversarial fixtures prove each sentinel fails closed before Pareto selection and preserves complete rejected-candidate evidence.

**Stop:** A sentinel can be candidate-disabled, mutates labels, or turns uncertainty into pass.
### Packet PE7-HE-EC2-PREDICTION-OUTCOME-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC2-SENTINEL-CONFORMANCE-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-EC2-PREDICTION-OUTCOME-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `engine/src/harness_evolution_eval.rs` fixture path and accepted evaluator/evidence owners; [planned seam; not current acceptance] sealed holdout mediation must extend, not replace, them.

**Allowed paths at promotion:** Exact evaluator, HE, store/artifact, and test paths selected by contract; candidate/generator code cannot edit evaluator rules, labels, holdout, or sentinels.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Seal/tamper/access, contamination/gaming/safety sentinel, blinding, immutable label, missingness, prediction-outcome derivation, restart/parity, and adversarial candidate tests.

**Rollback/recovery:** Keep fixture/default-off behavior until real controls pass; revoke new access and restore prior evaluator adapter while retaining audit/invalidation evidence.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. Independent T2 evaluator/security review is mandatory; no candidate or cheap worker may choose thresholds after outcomes.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Leakage, candidate influence, unverifiable labels, evaluator-owner duplication, or fixture-only evidence blocks EC3 and all improvement claims.

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

**Execution profile:** `PE7-HE-EC3-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing budget/spend, usage, artifact, audit, and HE store owners; [planned seam] HE lifecycle-cost projection must reuse those authorities and be revalidated.

**Allowed paths at promotion:** Exact existing budget/evidence/HE modules, migrations, and tests named by contract; no second budget ledger and no unknown cost coerced to zero.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Reservation/reconciliation, duplicate/missing usage, crash/restart, cancellation, failed-candidate cost, equal-envelope enforcement, parity, and full-cost fixture tests.

**Rollback/recovery:** Disable new HE admission/enforcement, reconcile outstanding reservations through existing owners, and preserve every consumed-cost record.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. Budget ontology and eligibility are T2 decisions; any live spend later remains T3.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Missing trustworthy cost dimensions, unbounded diagnosis/training/review cost, or conflicting spend owner yields `INSUFFICIENT`/`DECISION_REQUIRED`; EC4 remains blocked.

**Outcome:** Freeze lifecycle-cost ontology, trustworthy sources, missingness/eligibility rules, reservation/reconciliation, per-candidate/global envelopes, failure accounting, and the cost of diagnosis, hypothesis construction, prediction, and outcome reconciliation.

**Allowed delta:** No spend or runtime behavior change.

**Exit:** Versioned budget/accounting contract covering generation, evaluation, review, repair, CI, recovery, human effort, and failed attempts.

**Stop:** A material cost class is silently zero, source semantics are ambiguous, or contract creates a second spend owner.
### Packet PE7-HE-EC3-INSTRUMENTATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC3-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-EC3-INSTRUMENTATION-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing budget/spend, usage, artifact, audit, and HE store owners; [planned seam] HE lifecycle-cost projection must reuse those authorities and be revalidated.

**Allowed paths at promotion:** Exact existing budget/evidence/HE modules, migrations, and tests named by contract; no second budget ledger and no unknown cost coerced to zero.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Reservation/reconciliation, duplicate/missing usage, crash/restart, cancellation, failed-candidate cost, equal-envelope enforcement, parity, and full-cost fixture tests.

**Rollback/recovery:** Disable new HE admission/enforcement, reconcile outstanding reservations through existing owners, and preserve every consumed-cost record.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. Budget ontology and eligibility are T2 decisions; any live spend later remains T3.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Missing trustworthy cost dimensions, unbounded diagnosis/training/review cost, or conflicting spend owner yields `INSUFFICIENT`/`DECISION_REQUIRED`; EC4 remains blocked.

**Outcome:** Capture and normalize the accepted lifecycle-cost evidence through existing usage/artifact/store owners.

**Allowed delta:** Observation and immutable evidence only; no admission decision yet.

**Exit:** Source/partial/unavailable semantics, failure-path cost retention, restart, and parity tests pass.

**Stop:** Instrumentation drops rejected/failed cost, guesses unavailable values, or exposes sensitive raw evidence.
### Packet PE7-HE-EC3-ENFORCEMENT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC3-INSTRUMENTATION-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-EC3-ENFORCEMENT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing budget/spend, usage, artifact, audit, and HE store owners; [planned seam] HE lifecycle-cost projection must reuse those authorities and be revalidated.

**Allowed paths at promotion:** Exact existing budget/evidence/HE modules, migrations, and tests named by contract; no second budget ledger and no unknown cost coerced to zero.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Reservation/reconciliation, duplicate/missing usage, crash/restart, cancellation, failed-candidate cost, equal-envelope enforcement, parity, and full-cost fixture tests.

**Rollback/recovery:** Disable new HE admission/enforcement, reconcile outstanding reservations through existing owners, and preserve every consumed-cost record.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. Budget ontology and eligibility are T2 decisions; any live spend later remains T3.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Missing trustworthy cost dimensions, unbounded diagnosis/training/review cost, or conflicting spend owner yields `INSUFFICIENT`/`DECISION_REQUIRED`; EC4 remains blocked.

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

**Execution profile:** `PE7-HE-EC4-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE lineage/artifact/store owners plus a [planned seam; not current code] deterministic diversity admission rule; revalidate feature ownership and bounds.

**Allowed paths at promotion:** Exact HE modules/tests selected by contract; novelty, memory, or embeddings remain evidence only and cannot become authority.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Exact/near-duplicate fixtures, distance determinism, parent/family/seed coverage, collapse thresholds, adversarial gaming, restart/parity, and immutable decision evidence.

**Rollback/recovery:** Disable admission rule and restore prior bounded generation while retaining duplicate/rejection evidence and hashes.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Unstable distance, candidate-controlled features, diversity collapse, or coverage below frozen thresholds closes with `INSUFFICIENT`; EC5 cannot start.

**Outcome:** Freeze exact duplicate and near-duplicate definitions, distance features, family/parent/seed coverage, collapse thresholds, and reporting.

**Allowed delta:** No candidate generation or admission change.

**Exit:** Versioned diversity contract with deterministic thresholds, calibration source, false-positive handling, and no production-authority claim.

**Stop:** Metric depends on sealed outcomes, can be candidate-gamed without sentinel, or lacks deterministic replay.
### Packet PE7-HE-EC4-ADMISSION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC4-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-EC4-ADMISSION-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing HE lineage/artifact/store owners plus a [planned seam; not current code] deterministic diversity admission rule; revalidate feature ownership and bounds.

**Allowed paths at promotion:** Exact HE modules/tests selected by contract; novelty, memory, or embeddings remain evidence only and cannot become authority.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Exact/near-duplicate fixtures, distance determinism, parent/family/seed coverage, collapse thresholds, adversarial gaming, restart/parity, and immutable decision evidence.

**Rollback/recovery:** Disable admission rule and restore prior bounded generation while retaining duplicate/rejection evidence and hashes.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Unstable distance, candidate-controlled features, diversity collapse, or coverage below frozen thresholds closes with `INSUFFICIENT`; EC5 cannot start.

**Outcome:** Implement duplicate/near-duplicate admission and immutable distance evidence.

**Allowed delta:** Diversity admission only; hard safety/quality gates remain separate and prior.

**Exit:** Exact/near duplicate, collision, order, restart, lineage, and rejected-candidate preservation tests pass.

**Stop:** Admissibility becomes a quality score, evidence is nondeterministic, or rejected work disappears.
### Packet PE7-HE-EC4-COVERAGE-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC4-ADMISSION-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-EC4-COVERAGE-CLOSEOUT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE lineage/artifact/store owners plus a [planned seam; not current code] deterministic diversity admission rule; revalidate feature ownership and bounds.

**Allowed paths at promotion:** Exact HE modules/tests selected by contract; novelty, memory, or embeddings remain evidence only and cannot become authority.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Exact/near-duplicate fixtures, distance determinism, parent/family/seed coverage, collapse thresholds, adversarial gaming, restart/parity, and immutable decision evidence.

**Rollback/recovery:** Disable admission rule and restore prior bounded generation while retaining duplicate/rejection evidence and hashes.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** Unstable distance, candidate-controlled features, diversity collapse, or coverage below frozen thresholds closes with `INSUFFICIENT`; EC5 cannot start.

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

**Execution profile:** `PE7-HE-EC5-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/evaluator/store/lease owners plus [planned seams; not current code] immutable Pareto archive and HE stop/recovery state machine; revalidate transaction boundaries.

**Allowed paths at promotion:** Exact HE, evaluator, LocalProductStore, migration, and tests named by contract; never reuse `recursive_execution.rs` as an evolution controller.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Hard-gate order, dominance/tie/disagreement, archive completeness, every stop class, lease loss, crash points, exactly-once, late write, cleanup, parity, and deterministic replay tests.

**Rollback/recovery:** Default off, recover only from durable leases/state, preserve all candidates and counterevidence, and revert controller additions without rewriting terminal records.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. T2 owns selection, state-machine, and recovery semantics.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Ambiguous dominance, hidden reject, non-idempotent recovery, or unverifiable stop leaves Level-1 blocked and records `DECISION_REQUIRED`.

**Outcome:** Freeze hard-gate order, Pareto objectives, dominance/ties/disagreement, archive semantics, saturation/contamination/gaming/regression/budget/diversity stops, and recovery invariants.

**Allowed delta:** No selection engine or generation execution.

**Exit:** Exact selection/stop/recovery state-transition contract and Level-1 experiment envelope.

**Stop:** A scalar can override a hard gate, objective value bases are incomparable, or restart semantics are ambiguous.
### Packet PE7-HE-EC5-SELECTION-ARCHIVE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC5-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-EC5-SELECTION-ARCHIVE-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/evaluator/store/lease owners plus [planned seams; not current code] immutable Pareto archive and HE stop/recovery state machine; revalidate transaction boundaries.

**Allowed paths at promotion:** Exact HE, evaluator, LocalProductStore, migration, and tests named by contract; never reuse `recursive_execution.rs` as an evolution controller.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Hard-gate order, dominance/tie/disagreement, archive completeness, every stop class, lease loss, crash points, exactly-once, late write, cleanup, parity, and deterministic replay tests.

**Rollback/recovery:** Default off, recover only from durable leases/state, preserve all candidates and counterevidence, and revert controller additions without rewriting terminal records.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. T2 owns selection, state-machine, and recovery semantics.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Ambiguous dominance, hidden reject, non-idempotent recovery, or unverifiable stop leaves Level-1 blocked and records `DECISION_REQUIRED`.

**Outcome:** Implement hard-gate filtering, Pareto comparison, tie/disagreement handling, and an immutable candidate archive retaining causal manifests, counterevidence, and prediction outcomes.

**Allowed delta:** Selection evidence only; no active-Harness replacement or production adoption.

**Exit:** Dominance, incomparable basis, tie, rejection, archive tamper, and full-cost fixtures pass.

**Stop:** Best-only reporting, scalar override, candidate-controlled metric, hidden rejection, or prediction accuracy becoming selection authority becomes possible.
### Packet PE7-HE-EC5-STOP-RECOVERY-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-EC5-SELECTION-ARCHIVE-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-EC5-STOP-RECOVERY-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/evaluator/store/lease owners plus [planned seams; not current code] immutable Pareto archive and HE stop/recovery state machine; revalidate transaction boundaries.

**Allowed paths at promotion:** Exact HE, evaluator, LocalProductStore, migration, and tests named by contract; never reuse `recursive_execution.rs` as an evolution controller.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Hard-gate order, dominance/tie/disagreement, archive completeness, every stop class, lease loss, crash points, exactly-once, late write, cleanup, parity, and deterministic replay tests.

**Rollback/recovery:** Default off, recover only from durable leases/state, preserve all candidates and counterevidence, and revert controller additions without rewriting terminal records.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. T2 owns selection, state-machine, and recovery semantics.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Ambiguous dominance, hidden reject, non-idempotent recovery, or unverifiable stop leaves Level-1 blocked and records `DECISION_REQUIRED`.

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

**Execution profile:** `PE7-HE-LEVEL1-PREFLIGHT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing default-off one-generation HE fixture, evaluator, artifact, budget, and store owners; revalidate that every EC1-EC5 accepted control is wired before promotion.

**Allowed paths at promotion:** No source changes in EFFECT; preflight/closeout use exact HE/evidence paths and restricted evidence locations named by the accepted Level-1 contract.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Provider-free conformance for all EC controls, exact identity/budget/holdout checks, one-use run receipt, archive completeness, independent Pareto/prediction recomputation, and cleanup.

**Rollback/recovery:** An executed generation is immutable evidence; stop/recover through EC5, never rerun outcome unknown, and revert only closeout/status projections.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. The run requires T3 finite authority; preflight and independent closeout do not.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Close `SATURATED`, `HARM`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT` without transfer candidate; Level-1 transfer and Level-2 remain blocked.

**Outcome:** Freeze active Harness, parents, mutation families, causal-manifest identities, seeds, candidate limits, full budgets, evaluator/holdout identities, prediction-outcome rules, authorization package, and immediate preflight.

**Allowed delta:** No candidate generation or holdout access.

**Exit:** Zero-mismatch preflight and one finite experiment authorization request; every identity matches EC1-EC5.

**Stop:** Any mutable/unbound experiment field, stale seal, insufficient capacity, or missing rollback/evidence destination.
### Packet PE7-HE-LEVEL1-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL1-PREFLIGHT-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-LEVEL1-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing default-off one-generation HE fixture, evaluator, artifact, budget, and store owners; revalidate that every EC1-EC5 accepted control is wired before promotion.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Provider-free conformance for all EC controls, exact identity/budget/holdout checks, one-use run receipt, archive completeness, independent Pareto/prediction recomputation, and cleanup.

**Rollback/recovery:** An executed generation is immutable evidence; stop/recover through EC5, never rerun outcome unknown, and revert only closeout/status projections.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. The run requires T3 finite authority; preflight and independent closeout do not.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** Close `SATURATED`, `HARM`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT` without transfer candidate; Level-1 transfer and Level-2 remain blocked.

**Outcome:** Execute exactly one bounded generation through candidate creation, diversity admission, full-cost evaluation, hard gates, sealed holdout, and archive.

**Allowed delta:** Registered laboratory effects only; no memory/skill projection, active-Harness adoption, retuning, or second generation.

**Exit:** Every candidate including rejects has terminal lineage, failure-pattern evidence, frozen hypothesis, prediction outcome, cost, evaluator/sentinel, diversity, holdout, archive, cleanup, and restricted evidence.

**Stop:** Any EC stop rule, authority/lease mismatch, contamination, evaluator mutation, budget breach, outcome unknown, or hidden candidate.
### Packet PE7-HE-LEVEL1-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL1-RUN-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-LEVEL1-CLOSEOUT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing default-off one-generation HE fixture, evaluator, artifact, budget, and store owners; revalidate that every EC1-EC5 accepted control is wired before promotion.

**Allowed paths at promotion:** No source changes in EFFECT; preflight/closeout use exact HE/evidence paths and restricted evidence locations named by the accepted Level-1 contract.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Provider-free conformance for all EC controls, exact identity/budget/holdout checks, one-use run receipt, archive completeness, independent Pareto/prediction recomputation, and cleanup.

**Rollback/recovery:** An executed generation is immutable evidence; stop/recover through EC5, never rerun outcome unknown, and revert only closeout/status projections.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. The run requires T3 finite authority; preflight and independent closeout do not.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** Close `SATURATED`, `HARM`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT` without transfer candidate; Level-1 transfer and Level-2 remain blocked.

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

**Execution profile:** `PE7-HE-LEVEL1-TRANSFER-PROTOCOL-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/evaluator/RWE evidence owners with a sealed transfer artifact under the accepted contract; revalidate task-family isolation and model/environment strata.

**Allowed paths at promotion:** Exact transfer corpus/artifact/evaluator paths named at promotion; EFFECT changes no repository source and uses only restricted evidence destinations.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Seal/access/contamination checks, baseline identity, equal-budget allocation, drift/missingness, registered analysis, transfer/non-inferiority, full costs, and independent replication of calculations.

**Rollback/recovery:** Never unseal or retune after outcome; preserve failed runs and revert only provider-free analysis/status code or docs.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. The transfer run requires T3 finite authority and independent evaluator custody.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** A transfer failure, contamination, regression, or insufficient task-family support blocks Level-2; record the negative result as valid completion.

**Outcome:** Seal unseen tasks/task families and, where practical, repository/model/environment strata; freeze baselines, evaluator, budgets, drift, contamination, and decision rules.

**Allowed delta:** No transfer execution or candidate change.

**Exit:** Hash-bound transfer protocol/corpus and zero-mismatch preflight/authorization package.

**Stop:** Candidate or generator influenced the unseen set, strata are not truly unseen, or comparable value semantics are absent.
### Packet PE7-HE-LEVEL1-TRANSFER-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL1-TRANSFER-PROTOCOL-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-LEVEL1-TRANSFER-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/evaluator/RWE evidence owners with a sealed transfer artifact under the accepted contract; revalidate task-family isolation and model/environment strata.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Seal/access/contamination checks, baseline identity, equal-budget allocation, drift/missingness, registered analysis, transfer/non-inferiority, full costs, and independent replication of calculations.

**Rollback/recovery:** Never unseal or retune after outcome; preserve failed runs and revert only provider-free analysis/status code or docs.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. The transfer run requires T3 finite authority and independent evaluator custody.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** A transfer failure, contamination, regression, or insufficient task-family support blocks Level-2; record the negative result as valid completion.

**Outcome:** Execute the selected experimental candidate and frozen baselines on the sealed transfer set.

**Allowed delta:** Registered effects only; no repair or retraining on transfer outcomes.

**Exit:** Complete blinded results, failures, lifecycle cost, drift, cleanup, and evidence for all arms/tasks.

**Stop:** Contamination, evaluator drift, authority failure, outcome unknown, or global transfer stop.
### Packet PE7-HE-LEVEL1-TRANSFER-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL1-TRANSFER-RUN-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-LEVEL1-TRANSFER-ANALYSIS-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/evaluator/RWE evidence owners with a sealed transfer artifact under the accepted contract; revalidate task-family isolation and model/environment strata.

**Allowed paths at promotion:** Exact transfer corpus/artifact/evaluator paths named at promotion; EFFECT changes no repository source and uses only restricted evidence destinations.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Seal/access/contamination checks, baseline identity, equal-budget allocation, drift/missingness, registered analysis, transfer/non-inferiority, full costs, and independent replication of calculations.

**Rollback/recovery:** Never unseal or retune after outcome; preserve failed runs and revert only provider-free analysis/status code or docs.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. The transfer run requires T3 finite authority and independent evaluator custody.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** A transfer failure, contamination, regression, or insufficient task-family support blocks Level-2; record the negative result as valid completion.

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

**Execution profile:** `PE7-MEMORY-SKILL-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE artifact/store/evaluator owners plus [planned experimental seams] memory-only and skill-only projections; product `durable_memory.rs` is not HE projection authority and must not be repurposed.

**Allowed paths at promotion:** Exact HE-owned adapter/artifact/test paths selected by contract; no product-memory authority, global skill installation, or active Harness mutation.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Provenance/expiry/invalidation/delete-rebuild, leakage, arm isolation, equal-budget, contamination, attribution, restart/parity, and no-authority tests.

**Rollback/recovery:** Disable/delete experimental projections through their accepted owner while retaining tombstone/provenance and run evidence; baseline route remains intact.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. The optional run requires T3 finite authority; it is never a Level-2 prerequisite.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Close the optional branch `NO_GO`, `HARM`, or `INSUFFICIENT` without delaying Level-2; never infer a combined-factor effect from separate arms.

**Outcome:** Freeze baseline/no-projection, memory-only, and skill-only arms; projection schema, provenance, expiry, invalidation, deletion/rebuild, leakage, budgets, and attribution.

**Allowed delta:** No projection implementation or experiment. Product durable memory stays a separate domain.

**Exit:** Factorial protocol with identical non-factor conditions and explicit non-authority/sensitive-evidence rules.

**Stop:** Projection can grant routing/spend/evaluator/output/adoption authority, combined arm is introduced post hoc, or raw sensitive evidence lacks approved retention.
### Packet PE7-MEMORY-ADAPTER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-MEMORY-SKILL-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-MEMORY-ADAPTER-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing HE artifact/store/evaluator owners plus [planned experimental seams] memory-only and skill-only projections; product `durable_memory.rs` is not HE projection authority and must not be repurposed.

**Allowed paths at promotion:** Exact HE-owned adapter/artifact/test paths selected by contract; no product-memory authority, global skill installation, or active Harness mutation.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Provenance/expiry/invalidation/delete-rebuild, leakage, arm isolation, equal-budget, contamination, attribution, restart/parity, and no-authority tests.

**Rollback/recovery:** Disable/delete experimental projections through their accepted owner while retaining tombstone/provenance and run evidence; baseline route remains intact.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. The optional run requires T3 finite authority; it is never a Level-2 prerequisite.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Close the optional branch `NO_GO`, `HARM`, or `INSUFFICIENT` without delaying Level-2; never infer a combined-factor effect from separate arms.

**Outcome:** Implement the bounded experimental memory projection adapter.

**Allowed delta:** Derived/deletable/rebuildable source-bound projection only; no product durable-memory mutation or authority.

**Exit:** Provenance/expiry/invalidation/delete/rebuild/leakage and no-authority tests pass.

**Stop:** Adapter becomes authoritative, persists forbidden raw content, or cannot be fully invalidated.
### Packet PE7-SKILL-ADAPTER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-MEMORY-ADAPTER-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-SKILL-ADAPTER-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing HE artifact/store/evaluator owners plus [planned experimental seams] memory-only and skill-only projections; product `durable_memory.rs` is not HE projection authority and must not be repurposed.

**Allowed paths at promotion:** Exact HE-owned adapter/artifact/test paths selected by contract; no product-memory authority, global skill installation, or active Harness mutation.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Provenance/expiry/invalidation/delete-rebuild, leakage, arm isolation, equal-budget, contamination, attribution, restart/parity, and no-authority tests.

**Rollback/recovery:** Disable/delete experimental projections through their accepted owner while retaining tombstone/provenance and run evidence; baseline route remains intact.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. The optional run requires T3 finite authority; it is never a Level-2 prerequisite.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Close the optional branch `NO_GO`, `HARM`, or `INSUFFICIENT` without delaying Level-2; never infer a combined-factor effect from separate arms.

**Outcome:** Implement the bounded experimental skill projection adapter under the same factor contract.

**Allowed delta:** Skill-only derived projection; no registry authority, evaluator mutation, or production installation.

**Exit:** Source/version/scope/expiry/delete/rebuild/leakage and no-authority tests pass.

**Stop:** Skill can alter immutable policy/evaluator, execute outside admitted scope, or cannot be reconstructed.
### Packet PE7-MEMORY-SKILL-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-SKILL-ADAPTER-1

**Class:** `EFFECT`

**Execution profile:** `PE7-MEMORY-SKILL-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE artifact/store/evaluator owners plus [planned experimental seams] memory-only and skill-only projections; product `durable_memory.rs` is not HE projection authority and must not be repurposed.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Provenance/expiry/invalidation/delete-rebuild, leakage, arm isolation, equal-budget, contamination, attribution, restart/parity, and no-authority tests.

**Rollback/recovery:** Disable/delete experimental projections through their accepted owner while retaining tombstone/provenance and run evidence; baseline route remains intact.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. The optional run requires T3 finite authority; it is never a Level-2 prerequisite.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** Close the optional branch `NO_GO`, `HARM`, or `INSUFFICIENT` without delaying Level-2; never infer a combined-factor effect from separate arms.

**Outcome:** Execute the frozen baseline, memory-only, and skill-only arms under equal total lifecycle budget.

**Allowed delta:** Registered factor effects only; no combined arm or mid-run projection change.

**Exit:** Complete arm/task evidence, contamination/leakage sentinels, lifecycle cost, cleanup, and restricted/redacted bundles.

**Stop:** Leakage, imbalance, authority import, cross-arm contamination, or registered stop.
### Packet PE7-MEMORY-SKILL-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-MEMORY-SKILL-RUN-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-MEMORY-SKILL-ANALYSIS-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE artifact/store/evaluator owners plus [planned experimental seams] memory-only and skill-only projections; product `durable_memory.rs` is not HE projection authority and must not be repurposed.

**Allowed paths at promotion:** Exact HE-owned adapter/artifact/test paths selected by contract; no product-memory authority, global skill installation, or active Harness mutation.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Provenance/expiry/invalidation/delete-rebuild, leakage, arm isolation, equal-budget, contamination, attribution, restart/parity, and no-authority tests.

**Rollback/recovery:** Disable/delete experimental projections through their accepted owner while retaining tombstone/provenance and run evidence; baseline route remains intact.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. The optional run requires T3 finite authority; it is never a Level-2 prerequisite.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** Close the optional branch `NO_GO`, `HARM`, or `INSUFFICIENT` without delaying Level-2; never infer a combined-factor effect from separate arms.

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

**Execution profile:** `PE7-HE-LEVEL2-RULE-AUDIT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Canonical evidence/docs owners only; no Level-2 controller exists yet, and neither `recursive_execution.rs` nor model output may act as decision authority.

**Allowed paths at promotion:** Accepted evidence references plus `docs/NEXT_DECISION.md`/`docs/CURRENT_STATUS.md`; no runtime, schema, or controller code before explicit GO.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Digest/identity completeness, frozen-rule timing, hard-gate recomputation, lifecycle cost/diversity/transfer sensitivity, objection ledger, and independent reviewer reproduction.

**Rollback/recovery:** Decision receipts are append-only accepted evidence; correct a mistake with a superseding human decision, never rewrite the bound dossier.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. The final Level-2 decision is T3 human authority bound to the exact dossier and maximum controller envelope.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** `NO_GO`, `DEFER`, `HARM`, or `INSUFFICIENT` closes/rewrites the branch and forbids controller work.

**Outcome:** Verify that the Level-2 decision rule, hard gates, non-inferiority, value basis, uncertainty, lifecycle cost, diversity, contamination, feasibility, and stop thresholds were frozen before relevant outcomes.

**Allowed delta:** Audit only; no post-result threshold selection.

**Exit:** An eligible immutable rule/evidence manifest or DECISION_REQUIRED/NO_GO if preregistration is missing.

**Stop:** Any decisive threshold is post hoc, evidence is incomparable, or implementation feasibility lacks a bounded design.
### Packet PE7-HE-LEVEL2-EVIDENCE-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-RULE-AUDIT-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-LEVEL2-EVIDENCE-ANALYSIS-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Canonical evidence/docs owners only; no Level-2 controller exists yet, and neither `recursive_execution.rs` nor model output may act as decision authority.

**Allowed paths at promotion:** Accepted evidence references plus `docs/NEXT_DECISION.md`/`docs/CURRENT_STATUS.md`; no runtime, schema, or controller code before explicit GO.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Digest/identity completeness, frozen-rule timing, hard-gate recomputation, lifecycle cost/diversity/transfer sensitivity, objection ledger, and independent reviewer reproduction.

**Rollback/recovery:** Decision receipts are append-only accepted evidence; correct a mistake with a superseding human decision, never rewrite the bound dossier.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. The final Level-2 decision is T3 human authority bound to the exact dossier and maximum controller envelope.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** `NO_GO`, `DEFER`, `HARM`, or `INSUFFICIENT` closes/rewrites the branch and forbids controller work.

**Outcome:** Independently apply the frozen rule to Golden Path, RWE, Level-1, transfer, cost, diversity, maintenance, review, recovery, and rollback evidence.

**Allowed delta:** Analysis only; no controller design or candidate adoption.

**Exit:** A complete decision dossier with each gate PASS/FAIL/INSUFFICIENT and no scalar override.

**Stop:** Any required evidence is unavailable, evaluator integrity is uncertain, or sensitivity changes the gate result.
### Packet PE7-HE-LEVEL2-DECISION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-EVIDENCE-ANALYSIS-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-LEVEL2-DECISION-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Canonical evidence/docs owners only; no Level-2 controller exists yet, and neither `recursive_execution.rs` nor model output may act as decision authority.

**Allowed paths at promotion:** Accepted evidence references plus `docs/NEXT_DECISION.md`/`docs/CURRENT_STATUS.md`; no runtime, schema, or controller code before explicit GO.

**Ordered work:** T0/T2 assemble and independently verify the exact dossier -> pause -> T3 reads objections/rollback/cost -> sign one bounded disposition -> planning owner synchronizes route; do not execute the disposition's successor.

**Verification:** Digest/identity completeness, frozen-rule timing, hard-gate recomputation, lifecycle cost/diversity/transfer sensitivity, objection ledger, and independent reviewer reproduction.

**Rollback/recovery:** Decision receipts are append-only accepted evidence; correct a mistake with a superseding human decision, never rewrite the bound dossier.

**Human/effect gate:** T0/T2 may prepare but must pause for an explicit T3 human receipt; model output cannot sign or infer it. The final Level-2 decision is T3 human authority bound to the exact dossier and maximum controller envelope.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** `NO_GO`, `DEFER`, `HARM`, or `INSUFFICIENT` closes/rewrites the branch and forbids controller work.

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

**Execution profile:** `PE7-HE-LEVEL2-CONTROLLER-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE, scheduler/runtime, evaluator, budget, artifact, lease, and sole LocalProductStore owners plus a [planned seam; not current code] bounded Level-2 controller; `recursive_execution.rs` is explicitly not that owner.

**Allowed paths at promotion:** Exact modules/migrations/tests selected by the GO-bound controller contract; additions must live under existing owners and no second scheduler/store/evaluator may appear.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** State/migration parity, generation limits, parent rule, evaluation/admission, every global/local stop, crash/lease/exactly-once/cleanup, deterministic simulation, and rollback tests before any pilot.

**Rollback/recovery:** Default off; preserve durable controller/run/candidate records, recover through existing leases, and use tested migration/config rollback without adopting a candidate.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. T2 owns controller/schema/recovery implementation; the one pilot alone requires a separate T3 finite authorization.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Any owner conflict, mutable evaluator/budget/stop, unsafe restart, simulation gap, pilot outcome unknown, or maintenance excess ends `NO_GO`/`DECISION_REQUIRED`.

**Outcome:** On GO only, freeze generation/candidate limits, state machine, parent rule, APIs, owners, budgets, evaluator separation, stops, restart, cleanup, schema needs, and pilot envelope.

**Allowed delta:** No controller code, schema migration, or Provider effect.

**Exit:** File-level execution-ready contracts for the following controller slices and explicit proof that GO identity/envelope match.

**Stop:** Decision is not GO, any field remains caller/model controlled, or design imports adoption/merge/release authority.
### Packet PE7-HE-LEVEL2-STATE-PERSISTENCE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-CONTROLLER-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-LEVEL2-STATE-PERSISTENCE-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE, scheduler/runtime, evaluator, budget, artifact, lease, and sole LocalProductStore owners plus a [planned seam; not current code] bounded Level-2 controller; `recursive_execution.rs` is explicitly not that owner.

**Allowed paths at promotion:** Exact modules/migrations/tests selected by the GO-bound controller contract; additions must live under existing owners and no second scheduler/store/evaluator may appear.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** State/migration parity, generation limits, parent rule, evaluation/admission, every global/local stop, crash/lease/exactly-once/cleanup, deterministic simulation, and rollback tests before any pilot.

**Rollback/recovery:** Default off; preserve durable controller/run/candidate records, recover through existing leases, and use tested migration/config rollback without adopting a candidate.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. T2 owns controller/schema/recovery implementation; the one pilot alone requires a separate T3 finite authorization.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Any owner conflict, mutable evaluator/budget/stop, unsafe restart, simulation gap, pilot outcome unknown, or maintenance excess ends `NO_GO`/`DECISION_REQUIRED`.

**Outcome:** Implement default-off generation/run/candidate state, leases, lineage links, audit, and migrations under LocalProductStore.

**Allowed delta:** Contract-approved additive persistence only; no scheduling or Provider effect.

**Exit:** Migration/rollback, SQLite/PostgreSQL parity, lease, idempotency, tamper, and restart tests pass.

**Stop:** Creates a second store, destructive migration lacks recovery, or lease identity is ambiguous.
### Packet PE7-HE-LEVEL2-GENERATION-ORCHESTRATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-STATE-PERSISTENCE-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-LEVEL2-GENERATION-ORCHESTRATION-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE, scheduler/runtime, evaluator, budget, artifact, lease, and sole LocalProductStore owners plus a [planned seam; not current code] bounded Level-2 controller; `recursive_execution.rs` is explicitly not that owner.

**Allowed paths at promotion:** Exact modules/migrations/tests selected by the GO-bound controller contract; additions must live under existing owners and no second scheduler/store/evaluator may appear.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** State/migration parity, generation limits, parent rule, evaluation/admission, every global/local stop, crash/lease/exactly-once/cleanup, deterministic simulation, and rollback tests before any pilot.

**Rollback/recovery:** Default off; preserve durable controller/run/candidate records, recover through existing leases, and use tested migration/config rollback without adopting a candidate.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. T2 owns controller/schema/recovery implementation; the one pilot alone requires a separate T3 finite authorization.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Any owner conflict, mutable evaluator/budget/stop, unsafe restart, simulation gap, pilot outcome unknown, or maintenance excess ends `NO_GO`/`DECISION_REQUIRED`.

**Outcome:** Implement the fixed-generation scheduler and candidate lifecycle using existing runtime/executor owners.

**Allowed delta:** Provider-free orchestration with stubbed effects only; one selected laboratory parent per generation.

**Exit:** Deterministic order, candidate limits, exact lineage, cancellation, late-write, and no-extra-generation tests pass.

**Stop:** Controller becomes a second scheduler, can self-extend limits, or changes active production Harness.
### Packet PE7-HE-LEVEL2-EVALUATION-SELECTION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-GENERATION-ORCHESTRATION-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-LEVEL2-EVALUATION-SELECTION-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE, scheduler/runtime, evaluator, budget, artifact, lease, and sole LocalProductStore owners plus a [planned seam; not current code] bounded Level-2 controller; `recursive_execution.rs` is explicitly not that owner.

**Allowed paths at promotion:** Exact modules/migrations/tests selected by the GO-bound controller contract; additions must live under existing owners and no second scheduler/store/evaluator may appear.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** State/migration parity, generation limits, parent rule, evaluation/admission, every global/local stop, crash/lease/exactly-once/cleanup, deterministic simulation, and rollback tests before any pilot.

**Rollback/recovery:** Default off; preserve durable controller/run/candidate records, recover through existing leases, and use tested migration/config rollback without adopting a candidate.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. T2 owns controller/schema/recovery implementation; the one pilot alone requires a separate T3 finite authorization.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Any owner conflict, mutable evaluator/budget/stop, unsafe restart, simulation gap, pilot outcome unknown, or maintenance excess ends `NO_GO`/`DECISION_REQUIRED`.

**Outcome:** Integrate immutable evaluator/sentinels, total lifecycle budgets, diversity admission, hard gates, Pareto archive, and parent selection.

**Allowed delta:** Use EC1-EC5 owners unchanged; integration only.

**Exit:** Adversarial fixtures prove evaluator immutability, full-cost accounting, no scalar override, hidden-reject prevention, and deterministic parent selection.

**Stop:** Controller can alter evaluator/labels, reset budget, select failed candidate, or hide an arm.
### Packet PE7-HE-LEVEL2-STOP-RECOVERY-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-EVALUATION-SELECTION-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-LEVEL2-STOP-RECOVERY-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE, scheduler/runtime, evaluator, budget, artifact, lease, and sole LocalProductStore owners plus a [planned seam; not current code] bounded Level-2 controller; `recursive_execution.rs` is explicitly not that owner.

**Allowed paths at promotion:** Exact modules/migrations/tests selected by the GO-bound controller contract; additions must live under existing owners and no second scheduler/store/evaluator may appear.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** State/migration parity, generation limits, parent rule, evaluation/admission, every global/local stop, crash/lease/exactly-once/cleanup, deterministic simulation, and rollback tests before any pilot.

**Rollback/recovery:** Default off; preserve durable controller/run/candidate records, recover through existing leases, and use tested migration/config rollback without adopting a candidate.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. T2 owns controller/schema/recovery implementation; the one pilot alone requires a separate T3 finite authorization.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** Any owner conflict, mutable evaluator/budget/stop, unsafe restart, simulation gap, pilot outcome unknown, or maintenance excess ends `NO_GO`/`DECISION_REQUIRED`.

**Outcome:** Implement global/local stops, saturation, regression, exploitation, diversity-collapse, maintenance-burden, crash, lease, exactly-once, and cleanup behavior.

**Allowed delta:** Stop/recovery transitions only; no live effects.

**Exit:** Fault injection, concurrency, restart, outcome-unknown, cleanup, parity, and terminal-evidence tests pass.

**Stop:** A stopped run can resume without authority, an effect can repeat, or budget/evaluator state is lost.
### Packet PE7-HE-LEVEL2-SIMULATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-STOP-RECOVERY-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-LEVEL2-SIMULATION-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE, scheduler/runtime, evaluator, budget, artifact, lease, and sole LocalProductStore owners plus a [planned seam; not current code] bounded Level-2 controller; `recursive_execution.rs` is explicitly not that owner.

**Allowed paths at promotion:** Exact modules/migrations/tests selected by the GO-bound controller contract; additions must live under existing owners and no second scheduler/store/evaluator may appear.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** State/migration parity, generation limits, parent rule, evaluation/admission, every global/local stop, crash/lease/exactly-once/cleanup, deterministic simulation, and rollback tests before any pilot.

**Rollback/recovery:** Default off; preserve durable controller/run/candidate records, recover through existing leases, and use tested migration/config rollback without adopting a candidate.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. T2 owns controller/schema/recovery implementation; the one pilot alone requires a separate T3 finite authorization.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** Any owner conflict, mutable evaluator/budget/stop, unsafe restart, simulation gap, pilot outcome unknown, or maintenance excess ends `NO_GO`/`DECISION_REQUIRED`.

**Outcome:** Run provider-free deterministic simulations covering success, every stop class, crash points, contamination, gaming, and rollback.

**Allowed delta:** Fixture/simulation evidence only; no Provider or target effect.

**Exit:** Independent conformance receipt, bounded performance/resource evidence, and zero unresolved pilot blocker.

**Stop:** Simulation cannot reproduce a transition, safety invariant fails, or implementation deviates from the GO envelope.
### Packet PE7-HE-LEVEL2-PILOT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-SIMULATION-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-LEVEL2-PILOT-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE, scheduler/runtime, evaluator, budget, artifact, lease, and sole LocalProductStore owners plus a [planned seam; not current code] bounded Level-2 controller; `recursive_execution.rs` is explicitly not that owner.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** State/migration parity, generation limits, parent rule, evaluation/admission, every global/local stop, crash/lease/exactly-once/cleanup, deterministic simulation, and rollback tests before any pilot.

**Rollback/recovery:** Default off; preserve durable controller/run/candidate records, recover through existing leases, and use tested migration/config rollback without adopting a candidate.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. T2 owns controller/schema/recovery implementation; the one pilot alone requires a separate T3 finite authorization.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** Any owner conflict, mutable evaluator/budget/stop, unsafe restart, simulation gap, pilot outcome unknown, or maintenance excess ends `NO_GO`/`DECISION_REQUIRED`.

**Outcome:** Execute one small fixed Level-2 laboratory pilot under a separate finite authorization.

**Allowed delta:** Only the audited generation/candidate/evaluation envelope; no continuation across runs, production adoption, or limit increase.

**Exit:** Every generation/candidate/effect reaches terminal evidence with complete cost, lineage, evaluator, stop, cleanup, and restricted/redacted bundles.

**Stop:** Any mandatory stop, authority drift, outcome unknown, contamination, evaluator mutation, budget breach, or evidence loss.
### Packet PE7-HE-LEVEL2-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-LEVEL2-PILOT-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-LEVEL2-CLOSEOUT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE, scheduler/runtime, evaluator, budget, artifact, lease, and sole LocalProductStore owners plus a [planned seam; not current code] bounded Level-2 controller; `recursive_execution.rs` is explicitly not that owner.

**Allowed paths at promotion:** Exact modules/migrations/tests selected by the GO-bound controller contract; additions must live under existing owners and no second scheduler/store/evaluator may appear.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** State/migration parity, generation limits, parent rule, evaluation/admission, every global/local stop, crash/lease/exactly-once/cleanup, deterministic simulation, and rollback tests before any pilot.

**Rollback/recovery:** Default off; preserve durable controller/run/candidate records, recover through existing leases, and use tested migration/config rollback without adopting a candidate.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. T2 owns controller/schema/recovery implementation; the one pilot alone requires a separate T3 finite authorization.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** Any owner conflict, mutable evaluator/budget/stop, unsafe restart, simulation gap, pilot outcome unknown, or maintenance excess ends `NO_GO`/`DECISION_REQUIRED`.

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

**Execution profile:** `PE7-HE-FINAL-TRANSFER-PROTOCOL-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/evaluator/RWE artifact and evidence owners; revalidate selected experimental Harness and every sealed baseline identity.

**Allowed paths at promotion:** Exact sealed corpus/evaluator/artifact paths named by protocol; EFFECT changes no source and uses restricted evidence only.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Seal/contamination, allocation, drift, hard gates, transfer/non-inferiority, full lifecycle cost, failure/missingness, independent analysis, and strongest-claim boundary checks.

**Rollback/recovery:** Never retune/unseal/rerun outcome unknown; preserve raw evidence and revert only provider-free analysis/status projections.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. The final run requires T3 finite authority; evaluator custody and analysis remain independent.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** `NOT_SUPPORTED`, `HARM`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT` is valid completion and blocks favorable adoption/Meta claims while still allowing explicit branch disposition.

**Outcome:** Freeze a larger unseen task/family corpus, baselines, evaluator/labels, budgets, seeds, drift, contamination, stops, analysis, preflight, and finite authorizations.

**Allowed delta:** No execution or candidate repair.

**Exit:** Hash-bound final-transfer protocol/corpus and zero-mismatch authorization package.

**Stop:** Unseen status is compromised, value bases are incomparable, or candidate influenced protocol/corpus.
### Packet PE7-HE-FINAL-TRANSFER-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-FINAL-TRANSFER-PROTOCOL-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-FINAL-TRANSFER-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/evaluator/RWE artifact and evidence owners; revalidate selected experimental Harness and every sealed baseline identity.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Seal/contamination, allocation, drift, hard gates, transfer/non-inferiority, full lifecycle cost, failure/missingness, independent analysis, and strongest-claim boundary checks.

**Rollback/recovery:** Never retune/unseal/rerun outcome unknown; preserve raw evidence and revert only provider-free analysis/status projections.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. The final run requires T3 finite authority; evaluator custody and analysis remain independent.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** `NOT_SUPPORTED`, `HARM`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT` is valid completion and blocks favorable adoption/Meta claims while still allowing explicit branch disposition.

**Outcome:** Execute the selected experimental Harness and baselines on the final sealed set.

**Allowed delta:** Registered transfer effects only; no repair, learning, or evaluator change from transfer outcomes.

**Exit:** Complete blinded task/arm results, failures, cost, diversity, review/rework/recovery, drift, cleanup, and evidence.

**Stop:** Contamination, authority/evaluator drift, outcome unknown, or registered global stop.
### Packet PE7-HE-FINAL-TRANSFER-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-FINAL-TRANSFER-RUN-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-FINAL-TRANSFER-ANALYSIS-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/evaluator/RWE artifact and evidence owners; revalidate selected experimental Harness and every sealed baseline identity.

**Allowed paths at promotion:** Exact sealed corpus/evaluator/artifact paths named by protocol; EFFECT changes no source and uses restricted evidence only.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Seal/contamination, allocation, drift, hard gates, transfer/non-inferiority, full lifecycle cost, failure/missingness, independent analysis, and strongest-claim boundary checks.

**Rollback/recovery:** Never retune/unseal/rerun outcome unknown; preserve raw evidence and revert only provider-free analysis/status projections.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. The final run requires T3 finite authority; evaluator custody and analysis remain independent.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** `NOT_SUPPORTED`, `HARM`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT` is valid completion and blocks favorable adoption/Meta claims while still allowing explicit branch disposition.

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

**Execution profile:** `PE7-HE-ADOPTION-READINESS-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing candidate artifact, compatibility, rollout/observability, rollback, CI/review, and canonical decision owners; no runtime may self-adopt.

**Allowed paths at promotion:** Read-only accepted candidate/evidence plus exact docs/rollout artifacts named by the readiness contract; adoption decision itself changes no production code or deployment state.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Artifact/diff identity, compatibility/migration, security/maintenance, staged rollback drill, CI/review/objection completeness, and human-readable risk/cost reconciliation.

**Rollback/recovery:** A decision is superseded only by a new human receipt; `DECLINE`/`DEFER` leaves active Harness unchanged and preserves candidate evidence.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. The adoption decision is T3 human authority; release/deployment remains separately unauthorized.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** Any unresolved objection, untested rollback, identity drift, or unsupported final-transfer gate yields `DECLINE`/`DEFER`; no automatic adoption follows.

**Outcome:** Build the exact candidate artifact/diff, compatibility/migration, maintenance/security, rollout/observability, rollback, CI/review, and unresolved-objection dossier.

**Allowed delta:** Readiness planning/evidence only; no adoption, merge, release, deployment, or installation.

**Exit:** Independent adoption-readiness receipt with bounded canary/rollback proposal and all objections explicit.

**Stop:** Transfer is not PASS, rollback is untested, compatibility/security/maintenance cost is unacceptable, or exact artifact differs.
### Packet PE7-HE-ADOPTION-DECISION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-ADOPTION-READINESS-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-ADOPTION-DECISION-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing candidate artifact, compatibility, rollout/observability, rollback, CI/review, and canonical decision owners; no runtime may self-adopt.

**Allowed paths at promotion:** Read-only accepted candidate/evidence plus exact docs/rollout artifacts named by the readiness contract; adoption decision itself changes no production code or deployment state.

**Ordered work:** T0/T2 assemble and independently verify the exact dossier -> pause -> T3 reads objections/rollback/cost -> sign one bounded disposition -> planning owner synchronizes route; do not execute the disposition's successor.

**Verification:** Artifact/diff identity, compatibility/migration, security/maintenance, staged rollback drill, CI/review/objection completeness, and human-readable risk/cost reconciliation.

**Rollback/recovery:** A decision is superseded only by a new human receipt; `DECLINE`/`DEFER` leaves active Harness unchanged and preserves candidate evidence.

**Human/effect gate:** T0/T2 may prepare but must pause for an explicit T3 human receipt; model output cannot sign or infer it. The adoption decision is T3 human authority; release/deployment remains separately unauthorized.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** Any unresolved objection, untested rollback, identity drift, or unsupported final-transfer gate yields `DECLINE`/`DEFER`; no automatic adoption follows.

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

**Execution profile:** `PE7-HE-META-CLAIM-PROTOCOL-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/evaluator/artifact/store owners plus [planned seams; not current code] fixed O0/O1 operator adapters; no second evaluator, scheduler, or store.

**Allowed paths at promotion:** Exact HE-owned operator adapter, sealed corpus, artifact/store, and test paths selected by contracts; EFFECT packets change no source.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Operator-interface equivalence, treatment isolation, sealed-set access, equal full-cost budgets, stop/recovery, fixture conformance, blinded comparison/replication, and independent claim analysis.

**Rollback/recovery:** Default off; revert adapters while preserving operator/descendant lineage and every failed/rejected run; never adapt O1 after comparative outcomes.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. Every Meta run requires T3 finite authority; operator/claim contracts and evaluator custody remain T2/independent.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** `META_NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` closes Meta without blocking Dashboard; no R4-R6 or recursive claim becomes eligible.

**Outcome:** Decide whether Meta research is justified and, on GO, freeze the bounded second-order claim, estimands, hard gates, effect/error thresholds, domain, stops, and strongest allowed conclusion.

**Allowed delta:** Planning/GO-NO-GO only; no operator implementation or experiment.

**Exit:** Human-approved META_GO or META_NO_GO receipt. NO_GO rewrites the route; GO binds every later Meta packet.

**Stop:** Claim is open-ended, thresholds are post hoc, task/operator sample is infeasible, or authority/retention/spend envelope is unacceptable.
### Packet PE7-HE-META-OPERATOR-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-CLAIM-PROTOCOL-1

**Class:** `CONTRACT`

**Execution profile:** `PE7-HE-META-OPERATOR-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/evaluator/artifact/store owners plus [planned seams; not current code] fixed O0/O1 operator adapters; no second evaluator, scheduler, or store.

**Allowed paths at promotion:** Exact HE-owned operator adapter, sealed corpus, artifact/store, and test paths selected by contracts; EFFECT packets change no source.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Operator-interface equivalence, treatment isolation, sealed-set access, equal full-cost budgets, stop/recovery, fixture conformance, blinded comparison/replication, and independent claim analysis.

**Rollback/recovery:** Default off; revert adapters while preserving operator/descendant lineage and every failed/rejected run; never adapt O1 after comparative outcomes.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. Every Meta run requires T3 finite authority; operator/claim contracts and evaluator custody remain T2/independent.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** `META_NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` closes Meta without blocking Dashboard; no R4-R6 or recursive claim becomes eligible.

**Outcome:** On META_GO, freeze O0/O1 operator interfaces, identities, allowed algorithmic difference, input evidence, outputs, lineage, randomness, failure mapping, and non-authorities.

**Allowed delta:** No operator code. O1 may change only the pre-registered improvement policy, never evaluator/labels/authority.

**Exit:** Exact O0/O1 contract and implementation test vectors with one identifiable treatment difference.

**Stop:** Operators differ in budget/evaluator/access/authority, treatment is not isolatable, or O1 can self-modify its contract.
### Packet PE7-HE-META-CORPUS-EVALUATOR-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-OPERATOR-CONTRACT-1

**Class:** `CONTRACT`

**Execution profile:** `PE7-HE-META-CORPUS-EVALUATOR-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/evaluator/artifact/store owners plus [planned seams; not current code] fixed O0/O1 operator adapters; no second evaluator, scheduler, or store.

**Allowed paths at promotion:** Exact HE-owned operator adapter, sealed corpus, artifact/store, and test paths selected by contracts; EFFECT packets change no source.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Operator-interface equivalence, treatment isolation, sealed-set access, equal full-cost budgets, stop/recovery, fixture conformance, blinded comparison/replication, and independent claim analysis.

**Rollback/recovery:** Default off; revert adapters while preserving operator/descendant lineage and every failed/rejected run; never adapt O1 after comparative outcomes.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. Every Meta run requires T3 finite authority; operator/claim contracts and evaluator custody remain T2/independent.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** `META_NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` closes Meta without blocking Dashboard; no R4-R6 or recursive claim becomes eligible.

**Outcome:** Seal development, fixture-pilot, full-comparison, and replication task families; freeze immutable evaluator/labels, baselines, contamination/gaming sentinels, blinding, and access.

**Allowed delta:** No operator access or experiment.

**Exit:** Disjoint hash-bound corpus/evaluator manifest with unseen-family proof and invalidation rules.

**Stop:** Operator/generator influenced labels or holdout, task families are not independent enough for the claim, or leakage cannot be detected.
### Packet PE7-HE-META-BUDGET-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-CORPUS-EVALUATOR-1

**Class:** `CONTRACT`

**Execution profile:** `PE7-HE-META-BUDGET-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/evaluator/artifact/store owners plus [planned seams; not current code] fixed O0/O1 operator adapters; no second evaluator, scheduler, or store.

**Allowed paths at promotion:** Exact HE-owned operator adapter, sealed corpus, artifact/store, and test paths selected by contracts; EFFECT packets change no source.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Operator-interface equivalence, treatment isolation, sealed-set access, equal full-cost budgets, stop/recovery, fixture conformance, blinded comparison/replication, and independent claim analysis.

**Rollback/recovery:** Default off; revert adapters while preserving operator/descendant lineage and every failed/rejected run; never adapt O1 after comparative outcomes.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. Every Meta run requires T3 finite authority; operator/claim contracts and evaluator custody remain T2/independent.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** `META_NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` closes Meta without blocking Dashboard; no R4-R6 or recursive claim becomes eligible.

**Outcome:** Freeze equal total lifecycle budgets, candidate/generation/task/repetition limits, randomization, missingness, analysis, stop, recovery, and finite authorization envelopes for O0/O1.

**Allowed delta:** No run or operator implementation.

**Exit:** Versioned pre-registration with full-cost eligibility, power/precision sensitivity, seeds, allocation, and no post-pilot tunable claim field.

**Stop:** Equal budget is not enforceable, sample size/spend is unacceptable, or pilot/full/replication boundaries are not disjoint.
### Packet PE7-HE-META-O0-BASELINE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-BUDGET-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-META-O0-BASELINE-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/evaluator/artifact/store owners plus [planned seams; not current code] fixed O0/O1 operator adapters; no second evaluator, scheduler, or store.

**Allowed paths at promotion:** Exact HE-owned operator adapter, sealed corpus, artifact/store, and test paths selected by contracts; EFFECT packets change no source.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Operator-interface equivalence, treatment isolation, sealed-set access, equal full-cost budgets, stop/recovery, fixture conformance, blinded comparison/replication, and independent claim analysis.

**Rollback/recovery:** Default off; revert adapters while preserving operator/descendant lineage and every failed/rejected run; never adapt O1 after comparative outcomes.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. Every Meta run requires T3 finite authority; operator/claim contracts and evaluator custody remain T2/independent.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** `META_NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` closes Meta without blocking Dashboard; no R4-R6 or recursive claim becomes eligible.

**Outcome:** Implement/freeze the baseline improvement operator O0 as a deterministic adapter over existing EC/Level-2 owners.

**Allowed delta:** O0 policy only; no evaluator, budget, authority, adoption, or live effect.

**Exit:** Golden test vectors, lineage, budget requests, failure/stop, replay, and no-authority tests pass.

**Stop:** O0 is not reproducible, imports hidden heuristics/data, or bypasses experiment controls.
### Packet PE7-HE-META-O1-CANDIDATE-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-O0-BASELINE-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-META-O1-CANDIDATE-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/evaluator/artifact/store owners plus [planned seams; not current code] fixed O0/O1 operator adapters; no second evaluator, scheduler, or store.

**Allowed paths at promotion:** Exact HE-owned operator adapter, sealed corpus, artifact/store, and test paths selected by contracts; EFFECT packets change no source.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Operator-interface equivalence, treatment isolation, sealed-set access, equal full-cost budgets, stop/recovery, fixture conformance, blinded comparison/replication, and independent claim analysis.

**Rollback/recovery:** Default off; revert adapters while preserving operator/descendant lineage and every failed/rejected run; never adapt O1 after comparative outcomes.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. Every Meta run requires T3 finite authority; operator/claim contracts and evaluator custody remain T2/independent.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** `META_NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` closes Meta without blocking Dashboard; no R4-R6 or recursive claim becomes eligible.

**Outcome:** Implement the pre-registered candidate improvement operator O1 behind the identical interface.

**Allowed delta:** Only the contract-approved operator-policy treatment difference from O0.

**Exit:** Differential tests prove identical authority/evaluator/budget/access and the exact intended policy delta.

**Stop:** Implementation adds another treatment difference, accesses sealed outcomes, self-modifies, or cannot be replayed.
### Packet PE7-HE-META-FIXTURE-PILOT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-O1-CANDIDATE-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-META-FIXTURE-PILOT-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/EC/Level-2/evaluator/artifact/store owners plus [planned seams; not current code] fixed O0/O1 operator adapters; no second evaluator, scheduler, or store.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Operator-interface equivalence, treatment isolation, sealed-set access, equal full-cost budgets, stop/recovery, fixture conformance, blinded comparison/replication, and independent claim analysis.

**Rollback/recovery:** Default off; revert adapters while preserving operator/descendant lineage and every failed/rejected run; never adapt O1 after comparative outcomes.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. Every Meta run requires T3 finite authority; operator/claim contracts and evaluator custody remain T2/independent.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** `META_NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` closes Meta without blocking Dashboard; no R4-R6 or recursive claim becomes eligible.

**Outcome:** Run O0/O1 only on the disjoint fixture-pilot set to verify mechanics, cost capture, stops, and evidence flow; do not estimate the Meta claim.

**Allowed delta:** Finite pilot effects only; full/replication sets remain sealed and claim thresholds cannot change.

**Exit:** Complete pilot evidence and a mechanical PASS/REPAIR_REQUIRED/NO_GO disposition.

**Stop:** Leakage, treatment imbalance, evaluator mutation, outcome unknown, cost incompleteness, or any claim inference from fixture results.
### Packet PE7-HE-META-PILOT-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-FIXTURE-PILOT-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-META-PILOT-CLOSEOUT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/evaluator/artifact/store owners plus [planned seams; not current code] fixed O0/O1 operator adapters; no second evaluator, scheduler, or store.

**Allowed paths at promotion:** Exact HE-owned operator adapter, sealed corpus, artifact/store, and test paths selected by contracts; EFFECT packets change no source.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Operator-interface equivalence, treatment isolation, sealed-set access, equal full-cost budgets, stop/recovery, fixture conformance, blinded comparison/replication, and independent claim analysis.

**Rollback/recovery:** Default off; revert adapters while preserving operator/descendant lineage and every failed/rejected run; never adapt O1 after comparative outcomes.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. Every Meta run requires T3 finite authority; operator/claim contracts and evaluator custody remain T2/independent.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** `META_NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` closes Meta without blocking Dashboard; no R4-R6 or recursive claim becomes eligible.

**Outcome:** Validate pilot conformance and allow only mechanical repairs that preserve the frozen treatment and claim protocol.

**Allowed delta:** Evidence and explicitly enumerated non-semantic repair decision only; any semantic change requires a new Meta contract/version.

**Exit:** Exact-head conformance receipt and unchanged full-comparison pre-registration, or NO_GO.

**Stop:** Repair changes O0/O1 treatment, thresholds, corpus, evaluator, budgets, allocation, or claim.
### Packet PE7-HE-META-COMPARISON-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-PILOT-CLOSEOUT-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-META-COMPARISON-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/EC/Level-2/evaluator/artifact/store owners plus [planned seams; not current code] fixed O0/O1 operator adapters; no second evaluator, scheduler, or store.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Operator-interface equivalence, treatment isolation, sealed-set access, equal full-cost budgets, stop/recovery, fixture conformance, blinded comparison/replication, and independent claim analysis.

**Rollback/recovery:** Default off; revert adapters while preserving operator/descendant lineage and every failed/rejected run; never adapt O1 after comparative outcomes.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. Every Meta run requires T3 finite authority; operator/claim contracts and evaluator custody remain T2/independent.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** `META_NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` closes Meta without blocking Dashboard; no R4-R6 or recursive claim becomes eligible.

**Outcome:** Execute the preregistered randomized/blinded O0/O1 full comparison on unseen task families.

**Allowed delta:** Registered operator experiments only; no tuning, selective rerun, or hidden descendant.

**Exit:** All operators/tasks/repetitions/candidates including failures and rejects have terminal lineage, full cost, evaluator, transfer, stop, cleanup, and evidence.

**Stop:** Any global stop, contamination, imbalance, authority/evaluator drift, outcome unknown, or evidence loss.
### Packet PE7-HE-META-REPLICATION-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-COMPARISON-RUN-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-META-REPLICATION-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/EC/Level-2/evaluator/artifact/store owners plus [planned seams; not current code] fixed O0/O1 operator adapters; no second evaluator, scheduler, or store.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Operator-interface equivalence, treatment isolation, sealed-set access, equal full-cost budgets, stop/recovery, fixture conformance, blinded comparison/replication, and independent claim analysis.

**Rollback/recovery:** Default off; revert adapters while preserving operator/descendant lineage and every failed/rejected run; never adapt O1 after comparative outcomes.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. Every Meta run requires T3 finite authority; operator/claim contracts and evaluator custody remain T2/independent.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** `META_NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` closes Meta without blocking Dashboard; no R4-R6 or recursive claim becomes eligible.

**Outcome:** Execute the frozen independent replication/transfer set without inspecting or adapting to comparative conclusions beyond registered safety stops.

**Allowed delta:** Registered replication effects only.

**Exit:** Complete replication evidence bound to the same O0/O1 identities, evaluator, budgets, and claim protocol.

**Stop:** Operator/version changes, holdout leakage, drift beyond limits, outcome unknown, or replication authority failure.
### Packet PE7-HE-META-ANALYSIS-DECISION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-META-REPLICATION-RUN-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-META-ANALYSIS-DECISION-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/evaluator/artifact/store owners plus [planned seams; not current code] fixed O0/O1 operator adapters; no second evaluator, scheduler, or store.

**Allowed paths at promotion:** Exact HE-owned operator adapter, sealed corpus, artifact/store, and test paths selected by contracts; EFFECT packets change no source.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Operator-interface equivalence, treatment isolation, sealed-set access, equal full-cost budgets, stop/recovery, fixture conformance, blinded comparison/replication, and independent claim analysis.

**Rollback/recovery:** Default off; revert adapters while preserving operator/descendant lineage and every failed/rejected run; never adapt O1 after comparative outcomes.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. Every Meta run requires T3 finite authority; operator/claim contracts and evaluator custody remain T2/independent.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** `META_NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` closes Meta without blocking Dashboard; no R4-R6 or recursive claim becomes eligible.

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

**Execution profile:** `PE7-HE-ADVANCED-RECURSION-GATE-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** Only exact branch adapter/artifact/test paths selected by its contract; R5 has no accepted trainer/adapter owner until its contract and authority choose one, and model binaries/data stay outside Git.

**Ordered work:** T0/T2 assemble and independently verify the exact dossier -> pause -> T3 reads objections/rollback/cost -> sign one bounded disposition -> planning owner synchronizes route; do not execute the disposition's successor.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** T0/T2 may prepare but must pause for an explicit T3 human receipt; model output cannot sign or infer it. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Decide whether any R4-R6 research is justified and, on human `ADVANCED_GO`, freeze the immutable outer shell, branch-specific claims, maximum depth, mutable surfaces, sandbox, budgets, evidence retention, global stops, and strongest allowed conclusions.

**Allowed delta:** Planning and GO/NO-GO only; no adapter, training, self-modification, Provider request, or target effect.

**Exit:** Hash-bound `ADVANCED_GO` or `ADVANCED_NO_GO` receipt. GO names independently authorized R4 and/or R5 branches and grants neither R6 nor production authority.

**Stop:** Meta evidence is unsupported, recursive depth is unbounded, immutable surfaces are incomplete, oversight/rollback is infeasible, or expected lifecycle cost is unacceptable.

### R4 bounded metacognitive operator (DGM-H-inspired)

### Packet PE7-HE-R4-METACOGNITIVE-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-ADVANCED-RECURSION-GATE-1 with R4 authorized

**Class:** `CONTRACT`

**Execution profile:** `PE7-HE-R4-METACOGNITIVE-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** Only exact branch adapter/artifact/test paths selected by its contract; R5 has no accepted trainer/adapter owner until its contract and authority choose one, and model binaries/data stay outside Git.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Freeze the fixed Meta operator baseline and one self-referential treatment whose internal diagnosis, memory, proposal, and modification procedure may edit only an enumerated meta-operator workspace.

**Allowed delta:** Contract only. Evaluator/labels, parent selection, archive admission, budgets, permissions, stops, sandbox, active Harness, adoption, and release remain byte/value/behavior immutable.

**Exit:** Versioned editable-surface manifest, O0/R4 treatment identity, code/data access map, causal-manifest binding, compile/validation rules, equal-budget comparison and replication protocol, and complete rollback.

**Stop:** The treatment difference is not isolatable, generated code can escape the sandbox, or any outer-shell component must become mutable.

### Packet PE7-HE-R4-METACOGNITIVE-ADAPTER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R4-METACOGNITIVE-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-R4-METACOGNITIVE-ADAPTER-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** Only exact branch adapter/artifact/test paths selected by its contract; R5 has no accepted trainer/adapter owner until its contract and authority choose one, and model binaries/data stay outside Git.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Implement a provider-free, default-off metacognitive operator adapter over existing EC/Level-2 owners, with immutable snapshots of every self-change and no direct external-project dependency.

**Allowed delta:** Contract-approved meta-operator workspace, validation, lineage, archive projection, sandbox mediation, and fixtures only; no live effect or outer-loop change.

**Exit:** Self-edit lineage, stale-parent, tamper, forbidden-surface, compile failure, sandbox escape, rollback, restart, deterministic replay, full-cost, and no-authority tests pass.

**Stop:** Adapter becomes another scheduler/store/evaluator, can rewrite its contract, hides failed descendants, or cannot restore the exact prior operator.

### Packet PE7-HE-R4-COMPARISON-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R4-METACOGNITIVE-ADAPTER-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-R4-COMPARISON-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Execute the frozen fixed-operator versus metacognitive-operator comparison on sealed unseen task families under equal lifecycle budgets and finite authorization.

**Allowed delta:** Registered operator effects only; parent selection/evaluator/thresholds remain fixed and no descendant is adopted.

**Exit:** Every operator, task, descendant, self-edit, reject, failure, cost, stop, cleanup, and causal/prediction record reaches terminal evidence.

**Stop:** Sandbox/authority drift, contamination, evaluator gaming, outcome unknown, selective archive, budget imbalance, or outer-shell change.

### Packet PE7-HE-R4-REPLICATION-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R4-COMPARISON-RUN-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-R4-REPLICATION-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Execute the unchanged R4 protocol on a separately sealed replication domain without adapting to comparative results.

**Allowed delta:** Registered replication effects only.

**Exit:** Complete replication evidence bound to the same operator identities, mutable surface, outer shell, evaluator, budgets, and claim protocol.

**Stop:** Operator/version drift, cross-domain leakage, outcome unknown, or replication requires a post-result repair.

### Packet PE7-HE-R4-ANALYSIS-DECISION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R4-REPLICATION-RUN-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-R4-ANALYSIS-DECISION-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** Only exact branch adapter/artifact/test paths selected by its contract; R5 has no accepted trainer/adapter owner until its contract and authority choose one, and model binaries/data stay outside Git.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Test whether bounded self-referential modification improves eligible-descendant distribution, transfer reliability, improvement cost, and prediction calibration over the fixed operator.

**Allowed delta:** Analysis, claim, and branch disposition only.

**Exit:** Independent `METACOGNITIVE_SUPPORTED`, `NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` receipt with domain limits and full failures/costs.

**Stop:** One descendant carries the result, replication fails, added complexity erases value, or the conclusion implies open-ended/self-accelerating improvement.

### R5 Harness and weight-adapter co-evolution (SIA-inspired)

### Packet PE7-HE-R5-WEIGHT-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-ADVANCED-RECURSION-GATE-1 with R5 authorized

**Class:** `CONTRACT`

**Execution profile:** `PE7-HE-R5-WEIGHT-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** Only exact branch adapter/artifact/test paths selected by its contract; R5 has no accepted trainer/adapter owner until its contract and authority choose one, and model binaries/data stay outside Git.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Freeze a separate training-effect boundary, immutable open-weight base checkpoint, parameter-efficient adapter format, dataset/provenance/license/privacy rules, trainer/optimizer/RNG/compute identities, verifier separation, checkpoint security, budgets, rollback, and four-arm factorial protocol.

**Allowed delta:** Planning only. First-stage weight work is adapter-only (for example LoRA); base or full-model weights, Provider-hosted models, and production model routing remain immutable.

**Exit:** Exact `base`, `harness-only`, `weight-only`, and `harness+weight` arms with matched non-factor conditions, fixed update schedule, disjoint development/transfer sets, finite training authority, artifact retention, and deletion/rollback contract.

**Stop:** Training data rights/provenance are unclear, verifier leakage is possible, compute cannot be bounded, arms are confounded, or a second product store/budget/evaluator is proposed.

### Packet PE7-HE-R5-WEIGHT-ADAPTER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R5-WEIGHT-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-R5-WEIGHT-ADAPTER-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** Only exact branch adapter/artifact/test paths selected by its contract; R5 has no accepted trainer/adapter owner until its contract and authority choose one, and model binaries/data stay outside Git.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Implement a default-off external-training adapter that records immutable base/adapter/data/trainer/config/seed/compute identities and returns hash-bound artifacts through existing artifact/store owners.

**Allowed delta:** Adapter, validation, sandbox/job mediation, artifact references, redacted receipts, and provider-free fixtures only; model binaries remain outside the repository and no training runs in CI.

**Exit:** Wrong-base, poisoned/malformed adapter, data/config drift, duplicate job, crash, cancellation, outcome unknown, checksum, deletion, rollback, restart, parity, and no-production-route tests pass.

**Stop:** Credentials or training data enter durable evidence, adapter can replace the active model, training effects can retry ambiguously, or external infrastructure becomes a core authority.

### Packet PE7-HE-R5-FACTORIAL-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R5-WEIGHT-ADAPTER-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-R5-FACTORIAL-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Execute the preregistered four-arm factorial experiment with fixed update schedules to estimate harness, weight-adapter, and interaction effects before dynamic lever selection.

**Allowed delta:** Registered Harness mutations and adapter-training effects only; no interleaved chooser, full-weight update, production routing, or post-result arm change.

**Exit:** Complete task/arm/checkpoint/candidate/failure/reject/cost/contamination/cleanup evidence with matched budgets and terminal artifact lineage.

**Stop:** Arm imbalance, verifier coupling sentinel, data leakage, catastrophic capability regression, outcome unknown, budget breach, or selective checkpoint reporting.

### Packet PE7-HE-R5-FACTORIAL-ANALYSIS-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R5-FACTORIAL-RUN-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-R5-FACTORIAL-ANALYSIS-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** Only exact branch adapter/artifact/test paths selected by its contract; R5 has no accepted trainer/adapter owner until its contract and authority choose one, and model binaries/data stay outside Git.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Estimate main and interaction effects, transfer/non-inferiority, coupled-Goodhart sensitivity, full lifecycle value, and whether a fixed SIA-like lever chooser is eligible for one co-evolution pilot.

**Allowed delta:** Frozen analysis and `COEVOLUTION_ELIGIBLE`/`NO_GO`/`HARM`/`INSUFFICIENT` disposition only.

**Exit:** Independent factorial receipt with all four arms, uncertainty, multiplicity, sensitivity, adapter/base identities, and strongest allowed claim.

**Stop:** Weight-only attribution is unavailable, interaction is post hoc, transfer regresses, or the chooser would be tuned on the comparison result.

### Packet PE7-HE-R5-COEVOLUTION-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R5-FACTORIAL-ANALYSIS-1 with `COEVOLUTION_ELIGIBLE` and separate human authorization

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-R5-COEVOLUTION-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Execute one bounded SIA-like pilot in which a frozen lever-selection policy interleaves Harness and adapter-weight updates under the same immutable outer evaluator and budget owners.

**Allowed delta:** Registered interleaving only; the lever selector does not learn or self-modify, and full-model weights remain unchanged.

**Exit:** Every lever decision, causal manifest, trajectory identity, Harness delta, adapter checkpoint, cost, stop, and reject reaches terminal evidence.

**Stop:** Selector or evaluator changes, alternating updates amplify verifier gaming, capability regression crosses a hard gate, or evidence cannot attribute each state transition.

### Packet PE7-HE-R5-TRANSFER-REPLICATION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R5-COEVOLUTION-RUN-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-R5-TRANSFER-REPLICATION-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Replicate the frozen co-evolution treatment and factorial baselines on a separately sealed unseen task/model family.

**Allowed delta:** Registered replication effects only; no chooser, Harness, optimizer, data, threshold, or adapter repair.

**Exit:** Complete blinded replication evidence with base/adapter lineage, transfer, drift, regression, cost, and cleanup.

**Stop:** Model/task leakage, checkpoint incompatibility, outcome unknown, treatment drift, or replication hard-gate failure.

### Packet PE7-HE-R5-ANALYSIS-DECISION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R5-TRANSFER-REPLICATION-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-R5-ANALYSIS-DECISION-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** Only exact branch adapter/artifact/test paths selected by its contract; R5 has no accepted trainer/adapter owner until its contract and authority choose one, and model binaries/data stay outside Git.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Decide the bounded Harness+adapter claim and whether full-weight or model-architecture evolution is even planning-eligible.

**Allowed delta:** Analysis and `WEIGHT_COEVOLUTION_SUPPORTED`, `NOT_SUPPORTED`, `HARM`, or `INSUFFICIENT` disposition only. Full-weight work remains unrouted unless a later human decision creates a new contract.

**Exit:** Independent receipt covering factorial attribution, co-evolution increment, replication, catastrophic-forgetting/regression, contamination, compute, storage, and rollback costs.

**Stop:** Gains do not survive replication, base-model capabilities regress, checkpoint provenance is incomplete, or the conclusion generalizes beyond the tested adapter/model/domain.

### R6 bounded outer-policy evolution

### Packet PE7-HE-R6-OUTER-POLICY-CONTRACT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R4-ANALYSIS-DECISION-1 and PE7-HE-R5-ANALYSIS-DECISION-1, with at least one supported result and separate human R6 GO

**Class:** `CONTRACT`

**Execution profile:** `PE7-HE-R6-OUTER-POLICY-CONTRACT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** Only exact branch adapter/artifact/test paths selected by its contract; R5 has no accepted trainer/adapter owner until its contract and authority choose one, and model binaries/data stay outside Git.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Freeze exactly one mutable outer policy family for the first experiment: parent selection, Harness-vs-weight lever selection, or curriculum proposal. Keep evaluator/labels, task acceptance, hard gates, budgets, permissions, stops, archive integrity, and adoption external and immutable.

**Allowed delta:** Contract only; no simultaneous multi-policy evolution, evaluator evolution, self-generated goals, or live effect.

**Exit:** One identifiable fixed-policy baseline/treatment difference, state/action/outcome schema, off-policy evaluation limits, equal-budget comparison, sealed replication, rollback, and strongest allowed R6 claim.

**Stop:** Policy effects cannot be isolated, policy can choose its own evaluator/data/limits, or recursive depth can grow without a new human decision.

### Packet PE7-HE-R6-OUTER-POLICY-ADAPTER-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R6-OUTER-POLICY-CONTRACT-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-HE-R6-OUTER-POLICY-ADAPTER-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** Only exact branch adapter/artifact/test paths selected by its contract; R5 has no accepted trainer/adapter owner until its contract and authority choose one, and model binaries/data stay outside Git.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Implement the single accepted outer-policy treatment behind a deterministic, versioned interface over existing controllers and evidence owners.

**Allowed delta:** Policy adapter, immutable transition evidence, sandbox/fixtures, and rollback only; no live effect or other mutable family.

**Exit:** Action bounds, stale state, tamper, forbidden action, replay, exploration cap, rollback, crash, restart, full-cost, and no-authority tests pass.

**Stop:** Adapter becomes another controller/evaluator, can change its action space, or cannot reproduce the baseline/treatment boundary.

### Packet PE7-HE-R6-COMPARISON-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R6-OUTER-POLICY-ADAPTER-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-R6-COMPARISON-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Execute the fixed versus evolvable outer-policy comparison under finite authority, equal lifecycle budget, sealed tasks, and unchanged R4/R5 components.

**Allowed delta:** Registered policy effects only.

**Exit:** Complete policy-transition, candidate/checkpoint, failure/reject, cost, stop, contamination, and cleanup evidence.

**Stop:** Action-space/evaluator/budget drift, runaway exploration, curriculum leakage, outcome unknown, or selective transition history.

### Packet PE7-HE-R6-REPLICATION-RUN-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R6-COMPARISON-RUN-1

**Class:** `EFFECT`

**Execution profile:** `PE7-HE-R6-REPLICATION-RUN-1.v1`

**Worker tier:** `T3`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** No repository source path is writable; owner paths are read-only, and only the contract-named restricted evidence root plus later closeout/status projection may change.

**Ordered work:** Refresh exact main/evidence -> run immediate provider-free preflight -> pause for T3 finite authority -> execute exactly once -> journal every attempt/cost -> reconcile outcome and cleanup -> seal restricted raw plus redacted digest; never auto-retry unknown effects.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** T0/T1 may prepare and preflight, then must pause; only a fresh T3 finite one-use authority may permit the registered external effect. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Never consolidate with preflight, code repair, analysis, another run, or human decision; one authority, one run packet, one immutable receipt.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

**Outcome:** Execute the unchanged outer-policy comparison on a separately sealed replication family.

**Allowed delta:** Registered replication effects only.

**Exit:** Complete replication evidence bound to the same policy identities, action space, outer shell, evaluator, budgets, and claim protocol.

**Stop:** Policy/version drift, leakage, outcome unknown, or replication requires post-result tuning.

### Packet PE7-HE-R6-ANALYSIS-DECISION-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HE-R6-REPLICATION-RUN-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-HE-R6-ANALYSIS-DECISION-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing HE/EC/Level-2/Meta owners plus one branch-specific [planned seam; not current code]; revalidate the immutable outer shell and never use task-tree `recursive_execution.rs` as research depth.

**Allowed paths at promotion:** Only exact branch adapter/artifact/test paths selected by its contract; R5 has no accepted trainer/adapter owner until its contract and authority choose one, and model binaries/data stay outside Git.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** Immutable-shell/treatment isolation, sandbox/escape, lineage/tamper, full lifecycle cost, equal-budget comparison, sealed replication, catastrophic regression, stop/recovery, and no-authority tests.

**Rollback/recovery:** Default off; restore the exact prior operator/Harness/adapter/policy, retain every self-change/checkpoint/transition and cost record, and never undo an unknown external effect by retry.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. The gate and all runs/training require T3 human/operator authority; contracts/adapters require T2 architecture, security, license, and recovery review.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** A negative/insufficient R4 or R5 closes that sibling only; R6 needs explicit dispositions plus separate GO. Any unbounded depth or evaluator/goal mutation stops the portfolio.

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

**Execution profile:** `PE7-DASHBOARD-DISPOSITION-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `dashboard/src/**` presentation owner and accepted API/data projections; Dashboard never owns runtime, schema, workflow, evaluator, approval, spend, adoption, output, release, or deployment.

**Allowed paths at promotion:** `dashboard/src/**`, dashboard tests/styles/config, and exact presentation docs named by the disposition; `engine/**`, `wire_contract/**`, SDK, routes, and schemas are forbidden for the final refresh.

**Ordered work:** T0 inventory accepted owners/callers -> reconcile predecessor evidence -> T2 freeze values/interfaces/paths/failure and rollback rules -> add deterministic negative fixtures -> independent review -> publish one versioned hash-bound contract.

**Verification:** `bun --cwd dashboard run lint`, `typecheck`, `test`, and `build`; browser evidence for light/dark, desktop/mobile, keyboard, contrast, reduced motion, overflow, console/network errors; handoff/diff and exact-head review.

**Rollback/recovery:** Revert the presentation commit/PR and restore prior static assets; no data migration or backend rollback belongs here.

**Human/effect gate:** No external effect; T2 must accept any architecture, authority, schema, evaluator, statistical, retention, security, or recovery choice. PR disposition/merge remains maintainer-reviewed; presentation implementation is suitable for T1 after the contract is exact.

**Consolidation boundary:** Do not combine with implementation/effect; only adjacent provider-free contract text may share a PR when one owner, path set, rollback, and decision point are proven.

**Negative-result route:** If current schema cannot support the UI, return `DECISION_REQUIRED` to the owning upstream stage; do not add backend behavior. Accessibility failure blocks closeout.

**Outcome:** Decide whether stale PR #225 should close and be recreated, or be refreshed, against the final accepted schema and route dispositions.

**Allowed delta:** PR disposition and presentation contract only; no runtime/schema/business behavior.

**Exit:** Exact presentation scope, accepted data projections, accessibility/visual matrix, branch strategy, and rollback.

**Stop:** An upstream branch ended NO_GO and canonical routing has not been synchronized, schema still moves, or requested UI implies backend authority.
### Packet PE7-DASHBOARD-REFRESH-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-DASHBOARD-DISPOSITION-1

**Class:** `IMPLEMENT`

**Execution profile:** `PE7-DASHBOARD-REFRESH-1.v1`

**Worker tier:** `T1`

**Owner/seam:** Existing `dashboard/src/**` presentation owner and accepted API/data projections; Dashboard never owns runtime, schema, workflow, evaluator, approval, spend, adoption, output, release, or deployment.

**Allowed paths at promotion:** `dashboard/src/**`, dashboard tests/styles/config, and exact presentation docs named by the disposition; `engine/**`, `wire_contract/**`, SDK, routes, and schemas are forbidden for the final refresh.

**Ordered work:** Revalidate accepted contract and exact paths -> add focused failing/negative tests -> implement one additive or enumerated migration slice -> run compatibility/recovery checks -> remove only contract-approved compatibility -> emit cost/rollback receipt.

**Verification:** `bun --cwd dashboard run lint`, `typecheck`, `test`, and `build`; browser evidence for light/dark, desktop/mobile, keyboard, contrast, reduced motion, overflow, console/network errors; handoff/diff and exact-head review.

**Rollback/recovery:** Revert the presentation commit/PR and restore prior static assets; no data migration or backend rollback belongs here.

**Human/effect gate:** No external effect; T1 may implement only a frozen mechanical contract, while T2 accepts any high-risk seam and the complete diff. PR disposition/merge remains maintainer-reviewed; presentation implementation is suitable for T1 after the contract is exact.

**Consolidation boundary:** Apply the global rule: combine only same-owner mechanical slices explicitly permitted by the parent contract; never cross schema/authority/evaluator/rollback boundaries.

**Negative-result route:** If current schema cannot support the UI, return `DECISION_REQUIRED` to the owning upstream stage; do not add backend behavior. Accessibility failure blocks closeout.

**Outcome:** Apply the accepted presentation-only refresh on the current schema.

**Allowed delta:** CSS/layout/presentation and bounded tests only; no API, runtime, persistence, route, permission, evaluator, budget, adoption, output, or deployment behavior.

**Exit:** Lint/typecheck/tests/build and browser evidence across light/dark, desktop/mobile, keyboard, contrast, reduced motion, overflow, console, and network errors.

**Stop:** Any backend or schema change is needed, stale #225 behavior is imported blindly, or accessibility regresses.
### Packet PE7-DASHBOARD-CLOSEOUT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-DASHBOARD-REFRESH-1

**Class:** `CLOSEOUT`

**Execution profile:** `PE7-DASHBOARD-CLOSEOUT-1.v1`

**Worker tier:** `T2`

**Owner/seam:** Existing `dashboard/src/**` presentation owner and accepted API/data projections; Dashboard never owns runtime, schema, workflow, evaluator, approval, spend, adoption, output, release, or deployment.

**Allowed paths at promotion:** `dashboard/src/**`, dashboard tests/styles/config, and exact presentation docs named by the disposition; `engine/**`, `wire_contract/**`, SDK, routes, and schemas are forbidden for the final refresh.

**Ordered work:** Acquire immutable evidence and frozen rule -> independently recompute identities/gates/results -> preserve failures/missingness/cost -> issue explicit disposition -> synchronize status and rewrite only eligible routing; perform no new effect.

**Verification:** `bun --cwd dashboard run lint`, `typecheck`, `test`, and `build`; browser evidence for light/dark, desktop/mobile, keyboard, contrast, reduced motion, overflow, console/network errors; handoff/diff and exact-head review.

**Rollback/recovery:** Revert the presentation commit/PR and restore prior static assets; no data migration or backend rollback belongs here.

**Human/effect gate:** No new external effect; independent T2 closeout is required, and any human decision remains a separately signed receipt. PR disposition/merge remains maintainer-reviewed; presentation implementation is suitable for T1 after the contract is exact.

**Consolidation boundary:** Keep independent from the effect/implementation it judges; it may share no head that changes the frozen evidence or rule.

**Negative-result route:** If current schema cannot support the UI, return `DECISION_REQUIRED` to the owning upstream stage; do not add backend behavior. Accessibility failure blocks closeout.

**Outcome:** Independently verify exact-head presentation scope and close the final deferred surface.

**Allowed delta:** Review/status/merge eligibility evidence only; no deployment authority.

**Exit:** Exact-head independent PASS, canonical CI, visual evidence digests, clean rollback, and explicit merge decision.

**Stop:** Unreviewed visual delta, missing canonical check, backend behavior change, or unresolved accessibility objection.
## Adoption and claim boundary

candidate generation != causal explanation != prediction accuracy != experimental parent selection != active-Harness adoption != improvement-operator research != weight-adapter training

Each authority has its own evidence and decision. A GO authorizes only its named next packet. A NO_GO, DECLINE, DEFER, SATURATED, HARM, or INSUFFICIENT result is valid completion and requires the canonical route to be rewritten before any non-dependent work proceeds.

## Dashboard boundary

Dashboard work stays presentation-only and last. It may project accepted schemas and evidence but cannot become a workflow, evaluator, spend, approval, adoption, output, merge, release, or deployment owner.
