# Future Route

Last updated: 2026-08-29.

This document owns the blocked successor order for the owner-approved Autonomous Steward migration campaign. The current execution window is the `T3_REQUIRED` PR4B contract in `docs/NEXT_DECISION.md`; PR4A is accepted on main and recorded in `docs/CURRENT_STATUS.md`. The packets below are PR5–PR7 routing-only and `BLOCKED_PREREQUISITE`; none authorizes implementation, GitHub mutation, Provider spend, target writes, release, deployment, destructive effects, or automatic merge.

The former Harness-Evolution route is parked, not erased. Its accepted receipts remain in `docs/CURRENT_STATUS.md`, and its exact historical planning remains recoverable from Git. Unaccepted MX1 work is preserved only through the recovery references recorded in `docs/CURRENT_STATUS.md`. It may enter a later Mission only through an explicit symbol-level audit and never by wholesale merge.

## Worker Tiers

- `T0`: read-only inventory and evidence collection.
- `T1`: bounded implementation under an accepted contract.
- `T2`: planning, authority-boundary design, and independent closeout.
- `T3`: owner authority for finite external effects and irreversible decisions; never inferred from model output.

## Known Planned-Seam Gaps

- The Shadow Steward planner, policy, replay, and status projection, plus the provider-free crash-recoverable executor, journal, reconciliation loop, isolated WorkCard dispatch, path-lock coordinator, and bounded review/repair loop are accepted.
- Every successor must preserve the accepted PR0 exact-head/check contract and its guarded merge path; the acceptance receipt belongs to `docs/CURRENT_STATUS.md`.
- The legacy controller, workflows, packet route, and documentation remain authoritative until their individually verified cutover or removal.
- PR4A is accepted on main with current-main Mission integration readiness: authenticated approval, non-test activation/call flow, real multi-WorkCard dependency/path locking with K=2, per-Stage Draft PR creation/update, CI/review/repair, and one provider-free Mission to `WAITING_FOR_MERGE`.
- The PR4B canary, single-writer cutover, guarded merge activation, and bounded effect envelopes are planned but not accepted capability. PR4B is an external-effect/authority operation and cannot use a `risk_class: none` and `external_effect_limit: 0` profile.

## Promotion Profile Contract

Before promotion, the planning owner must refresh accepted `main`, live GitHub state, relevant owners, exact allowed paths, risk class, verification family, rollback, stop conditions, and any required external authorization. The inventory profile below is a routing classifier only. A future implementation packet that would perform an external effect must split or pause at the existing authority owner; its `IMPLEMENT` label never authorizes that effect.

## Stop and Resume Protocol

Ordinary implementation, test, review, CI, main-drift, worker, or retry failures stay inside the accepted Mission budget and must be repaired or replanned without escalating to the owner. Enter `DECISION_REQUIRED` only for a material Mission-goal change, authority or budget expansion, unapproved production/destructive action, incompatible product direction, unresolved safety contradiction, or `OUTCOME_UNKNOWN` external mutation. On restart, reconcile GitHub and durable journal facts before replaying any mutation.

### Packet PE7-AUTONOMOUS-STEWARD-PR5

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-AUTONOMOUS-STEWARD-PR4B

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

The three successor packets above replace the 54-packet routing horizon as repository-maintenance migration work. PR4B is owned by `docs/NEXT_DECISION.md` until its effect closeout is accepted. Accepted runtime capability and historical evidence remain owned by `docs/CURRENT_STATUS.md` and Git; this compression grants no product, research, Provider, release, deployment, or adoption authority.

<!-- future-route-inventory:v1
{"dependency_graph_sha256":"64b96b9e4c4e5788fecff7ccfa476b6d3d87a464d432ec8c60f99c91ee5a0b7e","ordered_packet_ids":["PE7-AUTONOMOUS-STEWARD-PR5","PE7-AUTONOMOUS-STEWARD-PR6","PE7-AUTONOMOUS-STEWARD-PR7"],"ordered_packet_ids_sha256":"e6534ef18a3c9ffc8f059d0510a571e109802571b137740916feb7db0dacfc48","packet_count":3,"profiles":[["PE7-AUTONOMOUS-STEWARD-PR5","IMPLEMENT","T1","none","source_focused_full"],["PE7-AUTONOMOUS-STEWARD-PR6","IMPLEMENT","T1","none","source_focused_full"],["PE7-AUTONOMOUS-STEWARD-PR7","CLOSEOUT","T2","none","evidence_review"]],"profiles_sha256":"a3a1c62eeaf6aae1f1af67383b1fd9cac792686dc8768eb02be54bc2827ab790","schema_version":"future_route_inventory.v1"}
-->
