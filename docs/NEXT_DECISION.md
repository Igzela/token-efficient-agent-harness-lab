# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The current provider-free packet is the AC3 Golden Path responsibility contract; it does not change state semantics, public compatibility, or authority ownership.

## Authoritative Forward Order

```text
[window: PE7-HE-EC2-HOLDOUT-SEAL-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-HE-EC2-HOLDOUT-SEAL-1` — `READY_FOR_EXECUTION`

## Completed (PE7-HE-EC2-CONTRACT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #529 exact head `6a34cff9d667a007397213ccf7d85cc60c0c2675`; merge `e405142c6eca2b55b7edd25329ff0a7ab63767ea`; exact-head `PASS`; canonical workflow `31951242817`.
## Packet PE7-HE-EC2-HOLDOUT-SEAL-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-HE-EC2-CONTRACT-1 — COMPLETE on accepted main `e405142c6eca2b55b7edd25329ff0a7ab63767ea` (PR #529 exact head `6a34cff9d667a007397213ccf7d85cc60c0c2675`; merge `e405142c6eca2b55b7edd25329ff0a7ab63767ea`; exact-head `PASS`; canonical workflow `31951242817`).

**Class:** `IMPLEMENT`

**Outcome:** Materialize sealed holdout identities, labels, access mediation, audit, and invalidation controls.

**Allowed delta:** engine/src/harness_evolution_eval.rs, engine/src/storage/local_product_store/harness_evolution.rs.

**Exit:** Unauthorized-read, label-tamper, leakage, restart, audit, and deletion/rotation tests pass.

**Stop:** Raw sensitive content would be committed, candidate identity gains access, or seal cannot survive restart.

### Twelve-field contract

1. **Outcome and non-goals.** Materialize sealed holdout identities, labels, access mediation, audit, and invalidation controls.
2. **Prerequisites and evidence.** Accepted main `e405142c6eca2b55b7edd25329ff0a7ab63767ea`; checked route manifest SHA `2d5d781226c955acaf43aa4c1fd293b710aadd0ff40a197d83a0a286c2a3b8d0`; predecessor receipt PR #529 exact head `6a34cff9d667a007397213ccf7d85cc60c0c2675`; merge `e405142c6eca2b55b7edd25329ff0a7ab63767ea`; exact-head `PASS`; canonical workflow `31951242817`; current-main evidence SHA `003955585124d9b36606e6e80c5b5226d9c8db46c0f4fb20030aba566af11d52`.
3. **Owners and paths.** Owners: engine/src/harness_evolution_eval.rs, engine/src/storage/local_product_store/harness_evolution.rs; callers: engine/src/storage/local_product_store/harness_evolution.rs; tests: engine/src/storage/local_product_store/harness_evolution.rs.
4. **Frozen invariants.** Packet identity, route manifest SHA `2d5d781226c955acaf43aa4c1fd293b710aadd0ff40a197d83a0a286c2a3b8d0`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** engine/src/harness_evolution_eval.rs, engine/src/storage/local_product_store/harness_evolution.rs: Materialize hash-only sealed holdout identity, access mediation, one-use admission, audit, restart, and invalidation controls without exposing labels or changing evaluator rules.
8. **Failure, recovery, and stop taxonomy.** Cleanup: No external effect or provider state is created; retain rejected evidence and use existing store cleanup/recovery owners.; retention: Keep hash-only vault identity, one-use receipt, audit, and rejected evidence under the existing evaluator and LocalProductStore owners.; decisions: authority unchanged; existing evaluator/store owners remain authoritative.; evaluator unchanged; no second evaluator is introduced.; recovery unchanged; restart and invalidation remain fail-closed.; schema unchanged; existing v1 wire/persistence identifiers remain compatible..
9. **Verification.** cargo fmt --all -- --check; cargo test -p engine; uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revert the holdout seal and audit changes together; retain existing v1 receipts and restore the accepted contract-only behavior.
11. **Exit artifact.** Evidence destinations: Existing evaluator/store audit and replay evidence..
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["engine/src/harness_evolution_eval.rs", "engine/src/storage/local_product_store/harness_evolution.rs"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Existing evaluator/store audit and replay evidence."], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Materialize sealed holdout identities, labels, access mediation, audit, and invalidation controls.", "ordered_steps": ["engine/src/harness_evolution_eval.rs, engine/src/storage/local_product_store/harness_evolution.rs: Materialize hash-only sealed holdout identity, access mediation, one-use admission, audit, restart, and invalidation controls without exposing labels or changing evaluator rules."], "packet_id": "PE7-HE-EC2-HOLDOUT-SEAL-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #529 exact head `6a34cff9d667a007397213ccf7d85cc60c0c2675`; merge `e405142c6eca2b55b7edd25329ff0a7ab63767ea`; exact-head `PASS`; canonical workflow `31951242817`"], "prerequisites": ["PE7-HE-EC2-CONTRACT-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "003955585124d9b36606e6e80c5b5226d9c8db46c0f4fb20030aba566af11d52", "read_paths": ["engine/src/harness_evolution_eval.rs", "engine/src/storage/local_product_store/harness_evolution.rs"], "risk_class": "none", "rollback": "Revert the holdout seal and audit changes together; retain existing v1 receipts and restore the accepted contract-only behavior.", "route_manifest_sha256": "2d5d781226c955acaf43aa4c1fd293b710aadd0ff40a197d83a0a286c2a3b8d0", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["cargo fmt --all -- --check", "cargo test -p engine", "uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "source_focused_full", "worker_tier": "T1"}
-->

## Common Execution Protocol

- `READY_FOR_EXECUTION` packets require a valid dispatch capsule; blocked or decision-required packets carry no executable capsule.
- Refresh accepted `main`, the current packet, exact PR head, CI, and review receipts before each transition.
- Keep a changing PR Draft; run focused/local full checks, then one final stable-head Standards/Spec review, Ready, canonical exact-head CI, manual squash merge, and main refresh.
- No Provider call, credential read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner in this packet.
- Unknown, stale, missing, or conflicting evidence remains fail-closed; never retry an effect whose outcome may be unknown.

## Hard Stops

- `DECISION_REQUIRED` when a caller, owner, path, semantics, compatibility fact, rollback, or evidence cannot be re-proved from accepted `main`.
- No Provider call, credential read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner.
- Unknown or possibly executed effects are never treated as success or retried.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is routing-only. The promoted packet was removed from that index and its manifest was refreshed; no future sketch authorizes code or an effect until promoted into this document from accepted `main`.
