from pathlib import Path


def replace(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text()
    if old not in text:
        if new in text:
            return
        raise SystemExit(f"missing anchor in {path}: {old[:80]!r}")
    target.write_text(text.replace(old, new, 1))


replace(
    "docs/CURRENT_STATUS.md",
    "| Post-LGB Product Evolution Plan | PE-1 and PE-2 are acceptance-sealed; PE3-CONTRACT-1, PE3-QUEUE-1, and PE3-READ-1 are complete; PE3-ACTIONS-1 is next |",
    "| Post-LGB Product Evolution Plan | PE-1 and PE-2 are acceptance-sealed; PE3-REPAIR-1 is the only active packet after independent review found defects in the merged PE-3 chain; PE-4 contract repair remains blocked on a truthful PE-3 closeout |",
)
replace(
    "docs/CURRENT_STATUS.md",
    "| PE-3 | P1 | Operator Decision Center | Complete and acceptance-sealed: versioned contracts, derived queue, read-only API/OpenAPI/SDK/Dashboard, hash-bound existing-control actions, and independent authorization closeout are merged |",
    "| PE-3 | P1 | Operator Decision Center | Independent repair in progress; the prior closeout is not accepted as evidence until PE3-REPAIR-1 and a separate PE3-CLOSE-1 pass exact-head and post-merge CI |",
)
replace(
    "docs/CURRENT_STATUS.md",
    "| PE-4 | P1/P2 | Trace-backed Policy Replay | Contract in progress: trace coverage, calibration, comparability, freshness, and out-of-distribution refusal gates are being defined before replay implementation |",
    "| PE-4 | P1/P2 | Trace-backed Policy Replay | Contract text is merged, but PR #193 is only an initial caller-asserted eligibility prototype and is superseded pending PE4-CONTRACT-REPAIR-1 after PE-3 closeout |",
)
replace(
    "docs/CURRENT_STATUS.md",
    "| PE-5 | P1.5 | Release Provenance | Detailed packets defined; eligible after PE-1 closeout only by explicit independent-lane activation |",
    "| PE-5 | P1.5 | Release Provenance | Not started; inactive in the current bounded objective |",
)
replace(
    "docs/CURRENT_STATUS.md",
    "| PE-6 | P2 | Fault Injection and Recovery Drills | Detailed packets defined; blocked on recovery invariants and affected stage prerequisites |",
    "| PE-6 | P2 | Fault Injection and Recovery Drills | Not started; blocked on explicit recovery invariants and affected stage prerequisites |",
)
replace(
    "docs/CURRENT_STATUS.md",
    "- PE-3 has not yet completed its independent acceptance closeout.",
    "- PE-3 is under PE3-REPAIR-1 because independent review found historical mutation replay, source-identity, evidence-chain, Retry, and action-owner defects; the separate PE3-CLOSE-1 has not started.",
)
replace(
    "docs/CURRENT_STATUS.md",
    "## Handoff Guard Anchors",
    """## PE-3 Independent Repair Evidence

- mutation actions are being changed to validate the client read time against the store clock and re-bind the exact current page, decision, resource, action, source kind, source ID, and source hash before invoking an owner;
- derived decision sources are being bound to bounded original evidence references, with an absent hash retained when no trustworthy owner hash exists;
- Retry is limited to blocked runs with a genuinely ready node, terminal failed runs do not produce ready actions, and pending approvals expose separate exact approve/reject decisions;
- approval resolution is being made atomic in the existing workflow owner for SQLite and PostgreSQL, while unsupported rollback/inspect/acknowledge remain explicit fail-closed outcomes;
- this is work in progress, not acceptance. PE-3 becomes complete only after the repair merges green and a separate independent closeout verifies the whole chain.

## Handoff Guard Anchors""",
)

replace(
    "docs/NEXT_DECISION.md",
    "The active direction is the Post-LGB Product Evolution plan. PE-2 is complete and acceptance-sealed; PE-3 is active with its contract and derived-queue packets complete. This is not AR-7, another LGB ladder, or a second control plane.",
    "The active direction is the Post-LGB Product Evolution plan. PE-2 is complete and acceptance-sealed. PE-3 is under PE3-REPAIR-1 after independent review found defects in the merged chain; PE-4 contract repair remains blocked until a separate truthful PE3-CLOSE-1. This is not AR-7, another LGB ladder, or a second control plane.",
)
replace(
    "docs/NEXT_DECISION.md",
    "| PE-3 | P1 | Operator Decision Center | In progress; contract and derived queue complete, read surfaces next |",
    "| PE-3 | P1 | Operator Decision Center | In progress; PE3-REPAIR-1 owns the merged-chain defects and PE3-CLOSE-1 is blocked on that repair |",
)
replace(
    "docs/NEXT_DECISION.md",
    "| PE-4 | P1/P2 | Trace-backed Policy Replay | Packetized; blocked on PE-3 closeout and trace coverage |",
    "| PE-4 | P1/P2 | Trace-backed Policy Replay | Not started beyond merged contract text and the superseded #193 prototype; PE4-CONTRACT-REPAIR-1 is blocked on PE3-CLOSE-1 |",
)
replace(
    "docs/NEXT_DECISION.md",
    "PE-3 is active. Its versioned contract and mutation-free derived queue are complete; the read-surface packet is next.",
    "PE-3 is active only through PE3-REPAIR-1. The contract, queue, read surface, actions, and prior closeout are merged, but the prior closeout is not accepted because independent review found mutation freshness, source identity, evidence-chain, Retry, and action-owner defects.",
)
replace(
    "docs/NEXT_DECISION.md",
    "### Packet PE3-CLOSE-1 — PE-3 acceptance seal\n\n**State:** `IN_PROGRESS`\n\n**Prerequisite:** PE3-ACTIONS-1 complete.",
    """### Packet PE3-REPAIR-1 — Independent merged-chain repair

**State:** `IN_PROGRESS`

**Prerequisite:** PE3-ACTIONS-1 complete.

**Goal:** Repair independently demonstrated PE-3 defects without creating a generic action executor or a second approval, pause, workflow, scheduler, audit, or rollback authority.

**Contract:** Read-only deterministic replay may retain a caller-supplied time. Mutation validates that time against the store clock, rejects stale/future reads, re-derives the exact bound page and exact current page, and binds decision ID, conflict key, resource, action, source kind, source ID, source hash, page, and freshness before owner invocation. Derived sources preserve bounded original evidence IDs and trustworthy hashes without fabricating absent hashes. Retry is ready only for blocked runs with a ready node. Approval resolution is atomic in the existing workflow owner across SQLite/PostgreSQL. Unsupported rollback, inspect, and acknowledge remain explicit fail-closed actions.

**Acceptance:** Focused freshness, tamper, source-change/resolution, page/order, hash/decision replay, cross-kind identity, approve/reject, retry terminal/no-ready/repeat/concurrency, resume compensation, permission, audit, restart, SQLite/PostgreSQL, and unsupported-action tests; full exact-head CI; no temporary workflow or repair file in the final diff.

**Rollback:** Revert the repair PR. No migration or queue cleanup; existing owner audit records remain authoritative.

### Packet PE3-CLOSE-1 — PE-3 acceptance seal

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE3-REPAIR-1 complete.""",
)
replace(
    "docs/NEXT_DECISION.md",
    "Later PE-3 packets remain: deterministic derived queue, read-only API/SDK/Dashboard surface, existing-control action adapters, and closeout.\n\n",
    "",
)
replace(
    "docs/NEXT_DECISION.md",
    "### Packet PE4-CONTRACT-1 — Calibration and coverage contract\n\n**State:** `IN_PROGRESS`",
    "### Packet PE4-CONTRACT-1 — Calibration and coverage contract\n\n**State:** `COMPLETE`",
)
replace(
    "docs/NEXT_DECISION.md",
    "**Acceptance and rollback:** Add deterministic contract tests for complete, sparse, stale, duplicate, incompatible, uncovered, and OOD cohorts; prove no provider, routing, policy, audit, or target-repository mutation on either recommendation or refusal. The contract packet adds only code/docs/tests; rollback is a revert with no migration or cleanup.\n\n## PE-5 — Release Provenance",
    """**Acceptance and rollback:** The durable contract text merged in PR #192. PR #193 added only an initial caller-asserted eligibility prototype; its booleans and manually supplied candidate data are not accepted as trace, coverage, calibration, comparability, or OOD evidence and are superseded by PE4-CONTRACT-REPAIR-1. Rollback is a revert with no migration or cleanup.

### Packet PE4-CONTRACT-REPAIR-1 — Real trace-backed replay evidence

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE3-CLOSE-1 complete.

**Goal:** Replace or subordinate the #193 prototype with deterministic normalized replay observations derived from existing RunTrace, persisted feedback/attribution evidence, offline evaluation, policy simulation, and compatible quality evidence.

**Contract:** Derive trace/dispatch identity, observation time, task/objective/candidate definition, endpoint/member set, complexity, terminal outcome, latency, tokens, posted or measured cost, retries, quality meaning, judge/reference pairing, schema version, and source hashes. Compute actual accepted/rejected coverage, paired calibration when judge measurements are used, full comparability, and bounded deterministic OOD envelopes. Missing, contradictory, unpriced, unmeasured, stale, incomparable, malformed, uncovered, uncalibrated, OOD, or tampered evidence produces explicit sorted hash-bound refusal codes and no recommendation.

**Forbidden:** No live policy mutation, provider call, hidden threshold, opaque authoritative score, automatic substitution, budget mutation, new experiment/pause/promotion/rollback owner, target-repository write, or PE-5/PE-6 work.

**Acceptance:** Versioned normalized-observation, cohort, coverage, calibration, eligibility, and refusal contracts; deterministic boundary/tamper tests; real adapters; no silent serialization fallback; SQLite/PostgreSQL-compatible representation; exact-head full CI.

## PE-5 — Release Provenance""",
)
replace(
    "docs/NEXT_DECISION.md",
    "1. Merge PE3-CONTRACT-1 after focused/full verification and green CI.\n2. Refresh `main`, then execute PE3-QUEUE-1.\n3. Continue PE3-READ-1, PE3-ACTIONS-1, and PE3-CLOSE-1 in order; do not begin PE-4 implementation before closeout.",
    "1. Complete PE3-REPAIR-1 on its exact reviewed head and merge only after all required CI jobs are green.\n2. Refresh `main`, verify post-merge CI, then execute PE3-CLOSE-1 as a separate independent closeout PR.\n3. After PE3-CLOSE-1 is complete, execute PE4-CONTRACT-REPAIR-1; leave PE-5 and PE-6 unstarted.",
)

replace(
    "docs/ARCHITECTURE_BOOK.md",
    "PE-3 uses additive `operator_decision_source.v1`, `operator_decision_item.v1`, and `operator_decision_queue.v1` Rust contracts in `engine/src/operator_decision.rs`.",
    "PE-3 uses additive `operator_decision_source.v1`, `operator_decision_item.v1`, and `operator_decision_queue.v1` Rust contracts in `engine/src/operator_decision/mod.rs`.",
)
replace(
    "docs/ARCHITECTURE_BOOK.md",
    "PE3-ACTIONS adds a narrowly allowlisted, hash-bound adapter route and explicit Python/TypeScript SDK request shapes. It re-derives the exact read page from the supplied timestamp, freshness, limit, and offset contract, rejects stale hash, missing confirmation, source/action mismatch, and unavailable owners, then invokes only a compatible existing owner. It does not grant generic execution authority: approval records preserve their metadata-only semantics; budget pause still requires `dispatch:execute`, confirmation, an enabled existing policy, and the original transaction/audit/recovery path; unsupported rollback/inspect/acknowledge requests fail closed. Rollback is a route/module revert with no data cleanup.",
    "PE3-ACTIONS remains a narrowly allowlisted adapter, not an execution authority. Read-only replay may use a deterministic caller time, but mutation compares that time with the store clock, rejects stale or future reads, re-derives the exact bound and current pages, and binds decision, conflict key, resource, action, source kind/ID/hash, pagination, and freshness before invoking an owner. Derived sources include bounded original evidence references; absent trustworthy hashes remain absent. Approval resolution is atomic inside the existing workflow owner for SQLite and PostgreSQL. Retry is exposed only for blocked runs with a ready node; terminal failed/completed/cancelled runs are not ready recommendations. Budget pause/recovery retains `dispatch:execute`, policy, audit, idempotency, and recovery gates. Unsupported rollback/inspect/acknowledge requests fail closed. Rollback is a code revert with no migration or queue cleanup.",
)
replace(
    "docs/ARCHITECTURE_BOOK.md",
    "## Execution Modes",
    """## Trace-backed Policy Replay Contract

PE4-CONTRACT-1 records the durable replay safety boundary, but PR #193 is only a prototype eligibility gate. Caller-asserted completeness booleans, reference-score flags, candidate definitions, or coverage claims are not evidence. PE4-CONTRACT-REPAIR-1 must derive normalized observations from existing `RunTrace`/`RunTraceRecorder`, persisted feedback and attribution evidence, `OfflineEvaluationEngine`, `PolicySimulator`, and compatible quality owners. It computes comparability, accepted/rejected coverage, paired judge/reference calibration when judge-based quality is used, and explicit bounded OOD envelopes. Missing, malformed, stale, incompatible, uncovered, uncalibrated, unpriced, unmeasured, OOD, or tampered evidence fails closed with deterministic versioned reason codes.

Offline replay and shadow comparison remain derived, read-only evidence and cannot mutate live routing or policy. Canary, promotion, pause, resume, and rollback must reuse existing owners and retain confirmation, permission, audit, idempotency, scope, duration, recovery, and rollback gates. No offline or shadow result alone may authorize promotion, and no replacement owner is permitted without an explicit replacement decision, compatibility/migration evidence, and rollback.

## Execution Modes""",
)

replace(
    "docs/MODULE_MAP.md",
    "| PE-3 operator decision contracts, queue, read surface, and bounded action adapter | `engine/src/operator_decision.rs`; `engine/src/storage/local_product_store/operator_decision_queue.rs`;",
    "| PE-3 operator decision contracts, queue, read surface, and bounded action adapter | `engine/src/operator_decision/mod.rs`; `engine/src/storage/local_product_store/operator_decision_queue/mod.rs`;",
)
replace(
    "docs/MODULE_MAP.md",
    "| PE-4 Trace-backed Policy Replay | `engine/src/feedback/run_trace_recorder.rs`; `engine/src/feedback/policy_simulator.rs`; adaptive experiment/canary modules; operator evidence | shadow-first, versioned evidence, coverage/OOD checks; reuse canary/promotion/rollback |",
    "| PE-4 Trace-backed Policy Replay | `engine/src/feedback/run_trace_recorder.rs`; persisted feedback/attribution owners; `engine/src/feedback/offline_evaluation.rs`; `engine/src/adaptive/policy_simulator.rs`; `engine/src/adaptive/shadow_routing.rs`; `engine/src/adaptive/contextual_policy.rs`; existing experiment/canary, pause/resume, rollback, operator-evidence, API/SDK/Dashboard owners | normalized trace evidence first; offline/shadow are non-mutating; reuse experiment, canary, promotion, pause, and rollback authority |",
)
replace(
    "docs/MODULE_MAP.md",
    "| PE-3 Operator Decision Center | `operator_decision.rs`; operator-evidence handlers; approvals; workflow/scheduler read models; Dashboard | derived action queue first; no hidden mutation path or duplicate authority source |",
    "| PE-3 Operator Decision Center | `operator_decision/mod.rs`; `operator_decision_queue/mod.rs`; operator decision HTTP handlers; existing approvals/workflow/scheduler/budget/benchmark/policy/rollback/recovery owners; SDKs; read-only Dashboard | derived queue with mutation-time current binding; no hidden generic executor or duplicate authority source |",
)
replace(
    "docs/MODULE_MAP.md",
    "3. PE-1 and PE-2 are acceptance-sealed; PE3-CONTRACT-1, PE3-QUEUE-1, and PE3-READ-1 are complete; PE3-ACTIONS-1 is next.\n4. Build PE-3 as a derived read model before connecting existing mutation endpoints.\n5. Progress PE-4 from calibration to offline replay, shadow, and bounded canary.",
    "3. PE-1 and PE-2 are acceptance-sealed; PE3-REPAIR-1 is the only active packet and PE3-CLOSE-1 remains blocked on it.\n4. Treat PE-3 as a derived read model plus allowlisted existing-owner adapter; mutation binds current evidence and never becomes a generic executor.\n5. After PE3-CLOSE-1, repair PE-4 from real trace normalization and coverage/calibration/OOD evidence before offline replay, read surfaces, shadow, canary, or promotion.",
)
