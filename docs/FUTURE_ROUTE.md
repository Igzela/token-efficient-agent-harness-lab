# Future Route

Last updated: 2026-08-27.

This document owns only the blocked successor order for the owner-approved Autonomous Steward migration campaign. The current execution window is `PE7-AUTONOMOUS-STEWARD-PR0` in `docs/NEXT_DECISION.md`. All packets below are routing-only and `BLOCKED_PREREQUISITE`; none authorizes implementation, GitHub mutation, Provider spend, target writes, release, deployment, destructive effects, or automatic merge.

The former Harness-Evolution route is parked, not erased. Its accepted receipts remain in `docs/CURRENT_STATUS.md`, and its exact historical planning remains recoverable from Git. Unaccepted MX1 work is preserved only through the recovery references recorded in `docs/CURRENT_STATUS.md`. It may enter a later Mission only through an explicit symbol-level audit and never by wholesale merge.

## Worker Tiers

- `T0`: read-only inventory and evidence collection.
- `T1`: bounded implementation under an accepted contract.
- `T2`: planning, authority-boundary design, and independent closeout.
- `T3`: owner authority for finite external effects and irreversible decisions; never inferred from model output.

## Known Planned-Seam Gaps

- `MaintenanceMission`, `Stage`, and `WorkCard` have no accepted schema or sole persistence owner yet.
- No continuously running, crash-recoverable Steward service exists.
- GitHub does not yet enforce the canonical exact-head/check contract through a repository ruleset.
- The legacy controller, workflows, packet route, and documentation remain authoritative until their individually verified cutover or removal.
- Natural-language intake, digest-bound owner approval, reconciliation, single-writer recovery, and bounded effect envelopes are planned but not accepted capability.

## Promotion Profile Contract

Before promotion, the planning owner must refresh accepted `main`, live GitHub state, relevant owners, exact allowed paths, risk class, verification family, rollback, stop conditions, and any required external authorization. The inventory profile below is a routing classifier only. A future implementation packet that would perform an external effect must split or pause at the existing authority owner; its `IMPLEMENT` label never authorizes that effect.

## Stop and Resume Protocol

Ordinary implementation, test, review, CI, main-drift, worker, or retry failures stay inside the accepted Mission budget and must be repaired or replanned without escalating to the owner. Enter `DECISION_REQUIRED` only for a material Mission-goal change, authority or budget expansion, unapproved production/destructive action, incompatible product direction, unresolved safety contradiction, or `OUTCOME_UNKNOWN` external mutation. On restart, reconcile GitHub and durable journal facts before replaying any mutation.

### Packet PE7-AUTONOMOUS-STEWARD-PR1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AUTONOMOUS-STEWARD-PR0

**Class:** `CONTRACT`

**Outcome:** Freeze `MaintenanceMission`, `Stage`, and `WorkCard` contracts plus the short-term legacy compatibility boundary without creating a second writer.

**Allowed delta:** Contract schemas, focused positive/negative tests, and the minimum canonical architecture/autonomy owners; legacy and new state may be read together, but only the legacy controller writes.

**Exit:** Schema validation proves digest-bound owner approval, bounded grants, budgets, stop taxonomy, exact identities, rollback, and rejection of unauthorized or stale proposals while all applicable legacy tests remain green.

**Stop:** A second runtime/store/approval owner appears, user comments become executable without authenticated digest binding, or the compatibility layer can write from both controllers.

### Packet PE7-AUTONOMOUS-STEWARD-PR2

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AUTONOMOUS-STEWARD-PR1

**Class:** `IMPLEMENT`

**Outcome:** Implement a read-only Shadow Steward that compiles natural-language proposals, plans/replans Stages and WorkCards, classifies stops, and emits compact status without mutating GitHub or repository state.

**Allowed delta:** Shadow-only planner, policy, state projection, replay fixtures, and focused tests; no GitHub mutation, worker dispatch, merge, Provider call, or service installation.

**Exit:** Historical failure replay proves ordinary failures do not pause the owner, all authority expansion and unknown-outcome cases do pause, and non-owner or digest-mismatched input cannot activate a Mission.

**Stop:** Shadow output is treated as authority, raw prompts/private content are retained, or replay comparison cannot distinguish recommendation from mutation.

### Packet PE7-AUTONOMOUS-STEWARD-PR3

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AUTONOMOUS-STEWARD-PR2

**Class:** `IMPLEMENT`

**Outcome:** Add the provider-free Steward service, durable journal, reconciliation, isolated WorkCard execution, path locking, bounded concurrency, repair/review loop, and Stage PR integration with automatic merge disabled.

**Allowed delta:** Provider-free local service, SQLite journal, worker/reviewer adapters, GitHub reconciliation adapter, systemd packaging, fault tests, and operator runbook; no live Provider call or auto-merge.

**Exit:** One approved provider-free Mission reaches a verified waiting-for-merge state after crash/restart testing without duplicate external mutations, overlapping-path writes, credential leakage, or self-review.

**Stop:** The Steward becomes a second product runtime/store, child sessions receive write credentials, mutation intent cannot be reconciled, or restart safety is unproved.

### Packet PE7-AUTONOMOUS-STEWARD-PR4

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AUTONOMOUS-STEWARD-PR3

**Class:** `IMPLEMENT`

**Outcome:** Run the provider-free canary and perform the explicit single-writer cutover from the legacy controller to the Steward, enabling guarded merge only after ruleset and exact-head gates are proved.

**Allowed delta:** Fault injection, canary fixtures, emergency-stop/cutover wiring, guarded merge integration, and bounded operator evidence; no Provider, production, deployment, or destructive effect.

**Exit:** Crash, timeout, bad output, path conflict, stale head, CI/review failure, GitHub ambiguity, and restart cases pass; one real provider-free Mission reaches merge with zero routine owner questions and exactly one active writer.

**Stop:** Both controllers can write, emergency stop or rollback is unavailable, review/CI can be bypassed, API ambiguity is replayed blindly, or auto-merge is enabled before all gates are proved.

### Packet PE7-AUTONOMOUS-STEWARD-PR5

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AUTONOMOUS-STEWARD-PR4

**Class:** `IMPLEMENT`

**Outcome:** Implement bounded parent effect envelopes and one-use child authorization derivation under the existing managed-acceptance and store owners.

**Allowed delta:** Provider-free envelope schema, policy, persistence, revocation/expiry/budget/target enforcement, audit evidence, and fault tests. Any live canary requires a separately promoted finite authority action and is not authorized by this packet.

**Exit:** Provider-free tests prove traceability to an owner-approved parent, total-budget accounting, exact target binding, expiry/revocation, fail-closed mismatch, and zero retry for `OUTCOME_UNKNOWN`; any later live canary has its own exact external-effect receipt.

**Stop:** The Steward can mint or widen authority, a child outlives or exceeds its parent, unknown outcomes retry, or existing managed-acceptance/store ownership moves.

### Packet PE7-AUTONOMOUS-STEWARD-PR6

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AUTONOMOUS-STEWARD-PR5

**Class:** `IMPLEMENT`

**Outcome:** Remove the inactive legacy lifecycle control plane, migrate stable documentation and external references, and retain exactly one lifecycle state machine, writer, and user entry.

**Allowed delta:** Legacy controller/workflow/label/template removal, documentation consolidation, compatibility cleanup, residual scans, rollback tag/reference, and equivalent safety tests after the Steward cutover is accepted.

**Exit:** Active governance documents are seven or fewer, old authority terms are zero or time-bounded allowlisted, one writer and one state machine remain, and rollback/recovery evidence is retained without reactivating both controllers.

**Stop:** Any stable invariant, accepted research evidence, safety test, rollback route, public link, or product effect boundary would be deleted without an accepted replacement.

### Packet PE7-AUTONOMOUS-STEWARD-PR7

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AUTONOMOUS-STEWARD-PR6

**Class:** `CLOSEOUT`

**Outcome:** Complete fixed-task non-regression, fault, recovery, emergency-stop, rollback, autonomy, and simplification acceptance, then remove the temporary dual-read compatibility layer.

**Allowed delta:** Acceptance fixtures, evidence manifests, final architecture/runbook/status synchronization, and compatibility-layer removal only; no new feature, effect, release, or deployment.

**Exit:** All hard safety gates pass; delivery quality does not regress; ordinary provider-free Missions need one initial approval and zero routine prompts; owner interruptions, cycle time, documents, workflows, state machines, and writers meet the campaign thresholds.

**Stop:** Before/after conditions are not comparable, any hard safety gate fails, rollback cannot be demonstrated, or a quality/autonomy claim lacks exact reproducible evidence.

## Portfolio Inventory Manifest

The seven successor packets above replace the 54-packet routing horizon as repository-maintenance migration work. Accepted runtime capability and historical evidence remain owned by `docs/CURRENT_STATUS.md` and Git; this compression grants no product, research, Provider, release, deployment, or adoption authority.

<!-- future-route-inventory:v1
{"dependency_graph_sha256": "51f67d42dd635e799d65ed4bcb9413ae551094f23ef8f06991cb4c9fc86f615b", "ordered_packet_ids": ["PE7-AUTONOMOUS-STEWARD-PR1", "PE7-AUTONOMOUS-STEWARD-PR2", "PE7-AUTONOMOUS-STEWARD-PR3", "PE7-AUTONOMOUS-STEWARD-PR4", "PE7-AUTONOMOUS-STEWARD-PR5", "PE7-AUTONOMOUS-STEWARD-PR6", "PE7-AUTONOMOUS-STEWARD-PR7"], "ordered_packet_ids_sha256": "e15c38acd1de7d0f4664d23e2e67a44365e410431836bee530df02f7ea85d1b5", "packet_count": 7, "profiles": [["PE7-AUTONOMOUS-STEWARD-PR1", "CONTRACT", "T2", "none", "docs_evidence_review"], ["PE7-AUTONOMOUS-STEWARD-PR2", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AUTONOMOUS-STEWARD-PR3", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AUTONOMOUS-STEWARD-PR4", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AUTONOMOUS-STEWARD-PR5", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AUTONOMOUS-STEWARD-PR6", "IMPLEMENT", "T1", "none", "source_focused_full"], ["PE7-AUTONOMOUS-STEWARD-PR7", "CLOSEOUT", "T2", "none", "evidence_review"]], "profiles_sha256": "0be3b5eea8fd4f8363c707ecc6b59d17b52b7cc73effdc368ad78beeb2f43d72", "schema_version": "future_route_inventory.v1"}
-->
