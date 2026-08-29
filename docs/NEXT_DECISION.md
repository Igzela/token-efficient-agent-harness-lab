# Next Decision

Last updated: 2026-08-29.

This document owns one current execution window. Accepted receipts belong in
`docs/CURRENT_STATUS.md`; blocked successors belong in `docs/FUTURE_ROUTE.md`;
live PR, CI, review, ruleset, Issue, and mergeability facts require fresh
GitHub readback.

## Current Direction

The owner-approved Autonomous Steward campaign has completed PR0 baseline
recovery, PR1 contract freeze, PR2 Shadow Steward acceptance, PR3
provider-free executor acceptance, PR4A integration readiness, and the PR4B
bounded provider-free repository-maintenance cutover. PR4B's accepted
receipts, authenticated owner approval, Vader service identity, single-writer
readback, canary journal, guarded-merge readback, emergency-stop compensation,
and rollback evidence are owned by `docs/CURRENT_STATUS.md` and Git history.
The current routed window is PR5 in `READY_FOR_EXECUTION`; it is provider-free
implementation under the existing managed-acceptance and LocalProductStore
owners. It must not perform a Provider call, target write, automatic merge,
T3 action, release, deployment, or credential operation.

## Active Routing

1. `PE7-AUTONOMOUS-STEWARD-PR5` — `READY_FOR_EXECUTION`

## Completed (PE7-AUTONOMOUS-STEWARD-PR4B)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #646 exact head `0e25bca160e9d348111598ef51dc80cea557addf`; merge `f46e118951b06e12e7a1286074a18713f03186a3`; exact-head `PASS`; canonical workflow `33237909936`.

## Packet PE7-AUTONOMOUS-STEWARD-PR5

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AUTONOMOUS-STEWARD-PR4B — COMPLETE on accepted main `f46e118951b06e12e7a1286074a18713f03186a3` (PR #646 exact head `0e25bca160e9d348111598ef51dc80cea557addf`; merge `f46e118951b06e12e7a1286074a18713f03186a3`; exact-head `PASS`; canonical workflow `33237909936`).

**Class:** `IMPLEMENT`

**Outcome:** Implement bounded parent effect envelopes and one-use child authorization derivation under the existing managed-acceptance and store owners.

**Allowed delta:** docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md, docs/ARCHITECTURE_BOOK.md, docs/MODULE_MAP.md, engine/src/product_golden_path.rs, engine/src/storage/local_product_store/audit.rs, engine/src/storage/local_product_store/managed_acceptance.rs, engine/src/storage/local_product_store/mod.rs, engine/src/storage/local_product_store/workflow_runs/queue_lease.rs, engine/tests/test_managed_acceptance_delegation.rs, engine/tests/test_product_golden_path_authority.rs.

**Exit:** Provider-free tests prove traceability to an owner-approved parent, total-budget accounting, exact target binding, expiry/revocation, fail-closed mismatch, and zero retry for `OUTCOME_UNKNOWN`; any later live canary has its own exact external-effect receipt.

**Stop:** The Steward can mint or widen authority, a child outlives or exceeds its parent, unknown outcomes retry, or existing managed-acceptance/store ownership moves.

### Twelve-field contract

1. **Outcome and non-goals.** Implement bounded parent effect envelopes and one-use child authorization derivation under the existing managed-acceptance and store owners.
2. **Prerequisites and evidence.** Accepted main `f46e118951b06e12e7a1286074a18713f03186a3`; checked route manifest SHA `cc38e18c3b96813b33adaabbe8662b68b545d562ab320cdb47bcade87d9c0530`; predecessor receipt PR #646 exact head `0e25bca160e9d348111598ef51dc80cea557addf`; merge `f46e118951b06e12e7a1286074a18713f03186a3`; exact-head `PASS`; canonical workflow `33237909936`; current-main evidence SHA `2f5e05a6552d88f3addb1675c013d94e7d376e2696a0ce3627b3977cefaef213`.
3. **Owners and paths.** Owners: engine/src/storage/local_product_store/managed_acceptance.rs, engine/src/storage/local_product_store/workflow_runs/queue_lease.rs, engine/src/storage/local_product_store/audit.rs; callers: engine/src/product_golden_path.rs, engine/src/storage/local_product_store/mod.rs; tests: engine/tests/test_managed_acceptance_delegation.rs, engine/tests/test_product_golden_path_authority.rs.
4. **Frozen invariants.** Packet identity, route manifest SHA `cc38e18c3b96813b33adaabbe8662b68b545d562ab320cdb47bcade87d9c0530`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** Add the parent effect-envelope identity and immutable binding under the existing LocalProductStore managed-acceptance owner.; Derive one-use child authorizations without widening scope, budget, target, expiry, or revocation authority.; Add provider-free SQLite/PostgreSQL parity and fault coverage for mismatch, expiry, revocation, and OUTCOME_UNKNOWN.
8. **Failure, recovery, and stop taxonomy.** Cleanup: Remove only superseded provider-free envelope fixtures after replacement tests prove the same owner and no unknown-outcome retry.; retention: Retain parent/child binding, expiry, revocation, budget, target, and outcome-unknown receipts as redacted provider-free evidence.; decisions: schema, evaluator, authority, and recovery ownership remain unchanged under existing owners..
9. **Verification.** python -m unittest discover -s tests -p test_agent_*.py; cargo test -p engine; cargo fmt --all -- --check; cargo clippy -p engine --all-targets --all-features -- -D warnings; python tools/check_security_baseline.py; bash scripts/check_wire_codegen_drift.sh; python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revert the single PR merge; retain existing LocalProductStore delegation, lease, audit, and rollback receipts, and never delete accepted recovery evidence.
11. **Exit artifact.** Evidence destinations: The PR exact-head review receipt and canonical CI run bound to the final head., Focused and full provider-free tests plus SQLite/PostgreSQL parity evidence., The updated CURRENT_STATUS/NEXT_DECISION/FUTURE_ROUTE canonical receipts..
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/NEXT_DECISION.md", "docs/ARCHITECTURE_BOOK.md", "docs/MODULE_MAP.md", "engine/src/product_golden_path.rs", "engine/src/storage/local_product_store/audit.rs", "engine/src/storage/local_product_store/managed_acceptance.rs", "engine/src/storage/local_product_store/mod.rs", "engine/src/storage/local_product_store/workflow_runs/queue_lease.rs", "engine/tests/test_managed_acceptance_delegation.rs", "engine/tests/test_product_golden_path_authority.rs"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["The PR exact-head review receipt and canonical CI run bound to the final head.", "Focused and full provider-free tests plus SQLite/PostgreSQL parity evidence.", "The updated CURRENT_STATUS/NEXT_DECISION/FUTURE_ROUTE canonical receipts."], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Implement bounded parent effect envelopes and one-use child authorization derivation under the existing managed-acceptance and store owners.", "ordered_steps": ["Add the parent effect-envelope identity and immutable binding under the existing LocalProductStore managed-acceptance owner.", "Derive one-use child authorizations without widening scope, budget, target, expiry, or revocation authority.", "Add provider-free SQLite/PostgreSQL parity and fault coverage for mismatch, expiry, revocation, and OUTCOME_UNKNOWN."], "packet_id": "PE7-AUTONOMOUS-STEWARD-PR5", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #646 exact head `0e25bca160e9d348111598ef51dc80cea557addf`; merge `f46e118951b06e12e7a1286074a18713f03186a3`; exact-head `PASS`; canonical workflow `33237909936`"], "prerequisites": ["PE7-AUTONOMOUS-STEWARD-PR4B"], "private_paths_allowed": false, "promotion_evidence_sha256": "2f5e05a6552d88f3addb1675c013d94e7d376e2696a0ce3627b3977cefaef213", "read_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/NEXT_DECISION.md", "docs/ARCHITECTURE_BOOK.md", "docs/MODULE_MAP.md", "engine/src/product_golden_path.rs", "engine/src/storage/local_product_store/audit.rs", "engine/src/storage/local_product_store/managed_acceptance.rs", "engine/src/storage/local_product_store/mod.rs", "engine/src/storage/local_product_store/workflow_runs/queue_lease.rs", "engine/tests/test_managed_acceptance_delegation.rs", "engine/tests/test_product_golden_path_authority.rs"], "risk_class": "none", "rollback": "Revert the single PR merge; retain existing LocalProductStore delegation, lease, audit, and rollback receipts, and never delete accepted recovery evidence.", "route_manifest_sha256": "cc38e18c3b96813b33adaabbe8662b68b545d562ab320cdb47bcade87d9c0530", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["python -m unittest discover -s tests -p test_agent_*.py", "cargo test -p engine", "cargo fmt --all -- --check", "cargo clippy -p engine --all-targets --all-features -- -D warnings", "python tools/check_security_baseline.py", "bash scripts/check_wire_codegen_drift.sh", "python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "source_focused_full", "worker_tier": "T1"}
-->

## Hard Stops

Enter `DECISION_REQUIRED` only for a real authority, scope, safety, identity,
recovery, or `OUTCOME_UNKNOWN` contradiction. Ordinary implementation, test,
review, CI, main-drift, tool, and recoverable-conflict failures remain inside
the accepted PR5 contract and must be repaired without widening authority.

## Common Execution Protocol

- `READY_FOR_EXECUTION` and `IN_PROGRESS` are executable packet states only
  when their prerequisites, authority, scope, rollback, and verification are
  current and proved from accepted main; PR5 remains provider-free and has
  `external_effect_limit=0`.
- Ordinary implementation, test, review, CI, main-drift, tool, and recoverable
  conflict failures remain repairable inside an accepted packet. They do not
  authorize a Provider call, target write, T3 action, automatic merge, release,
  deployment, or credential operation.
- A new main, PR head, review receipt, CI result, or canonical-document change
  invalidates stale evidence. GitHub mutations require exact readback, and
  `OUTCOME_UNKNOWN` is never treated as success or retried blindly.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` retains PR6 and PR7 as blocked successors behind the
current PR5 window. PR4B is accepted and its route was refreshed from
accepted main; PR6 may not start until PR5 is independently accepted.
