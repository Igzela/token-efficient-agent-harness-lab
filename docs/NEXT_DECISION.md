# Next Decision

Last updated: 2026-08-17.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze, AC2 typed execution, AC3 Golden Path responsibility split, AC4 transaction views, AC5 composition root, and AC6 Rust-authoritative schema convergence are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC7 removal manifest and deletion-only cleanup are accepted; `PE7-AC7-CLOSEOUT-1` is now the sole provider-free evidence/status window. No provider call, target write, authority consumption, or effect is authorized.

## Authoritative Forward Order

```text
[window: PE7-RWE-CR-RECONSTRUCTION-1 — READY_FOR_EXECUTION, provider-free]


```

## Active Routing

1. `PE7-RWE-CR-RECONSTRUCTION-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC7-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178`.
## Packet PE7-RWE-CR-RECONSTRUCTION-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC7-CLOSEOUT-1 — COMPLETE on accepted main `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b` (PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178`).

**Class:** `IMPLEMENT`

**Outcome:** Rebuild and provider-free validate the frozen pre-AC Harness beside the accepted post-AC Harness under isolated identities.

**Allowed delta:** docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md, engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json, engine/src/rwe/corpus.rs, engine/src/rwe/execution_schedule.rs, engine/src/rwe/frozen_rwe_bindings.rs, engine/src/rwe/operator_corpus.rs, scripts/verify_rwe_snapshot.py, tests/test_verify_rwe_snapshot.py.

**Exit:** Both Harnesses pass registered provider-free traces and bind exact binaries/config/toolchains without shared mutable state.

**Stop:** Old Harness cannot be reproduced, isolation fails, or compatibility shims change the measured behavior.

### Twelve-field contract

1. **Outcome and non-goals.** Rebuild and provider-free validate the frozen pre-AC Harness beside the accepted post-AC Harness under isolated identities.
2. **Prerequisites and evidence.** Accepted main `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; checked route manifest SHA `8569f8aad2dd6672e75a44ed6f61c72f852c0a3290744d957e0bc762d9acde7f`; predecessor receipt PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178`; current-main evidence SHA `ae7adfa9c87ed461649dd8f61dcf329e35372f22a1eb1f31f4c84a352587d831`.
3. **Owners and paths.** Owners: engine/src/rwe/operator_corpus.rs; callers: engine/tests/test_pg_integration.rs; tests: engine/src/rwe/operator_corpus.rs, tests/test_verify_rwe_snapshot.py.
4. **Frozen invariants.** Packet identity, route manifest SHA `8569f8aad2dd6672e75a44ed6f61c72f852c0a3290744d957e0bc762d9acde7f`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/CURRENT_STATUS.md, docs/NEXT_DECISION.md, docs/FUTURE_ROUTE.md: promote one provider-free reconstruction window and bind the accepted predecessor.; engine/src/rwe/operator_corpus.rs, engine/src/rwe/corpus.rs, engine/src/rwe/execution_schedule.rs, engine/src/rwe/frozen_rwe_bindings.rs, engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json: materialize only isolated old/new reconstruction identities and hash-bound artifacts.; scripts/verify_rwe_snapshot.py, tests/test_verify_rwe_snapshot.py: verify the source commit, recipe overlay, frozen corpus inputs, and provider-free reconstruction traces.
8. **Failure, recovery, and stop taxonomy.** Cleanup: Remove only disposable reconstruction workspaces and derived provider-free traces after validation; retain hash-bound manifests and redacted evidence.; retention: Retain the pre_ac_harness_snapshot.v2.json manifest, source/recipe/corpus/toolchain hashes, and redacted verification receipts under existing RWE evidence owners.; decisions: schema unchanged (docs/NEXT_DECISION.md:schema); evaluator unchanged (docs/NEXT_DECISION.md:evaluator); authority unchanged (docs/NEXT_DECISION.md:authority); recovery unchanged (docs/NEXT_DECISION.md:recovery).
9. **Verification.** git diff --check; cargo fmt --all -- --check; cargo clippy -p engine --all-targets --all-features -- -D warnings; cargo test -p engine; python -m unittest discover -s tests -p test_agent_*.py
10. **Compatibility, rollback, and retention.** Revert the reconstruction adapter/artifact change and retain the accepted post-AC Harness; do not alter the frozen pre-AC snapshot or accepted RWE corpus identities.
11. **Exit artifact.** Evidence destinations: docs/CURRENT_STATUS.md, docs/NEXT_DECISION.md, engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json.
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/NEXT_DECISION.md", "engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json", "engine/src/rwe/corpus.rs", "engine/src/rwe/execution_schedule.rs", "engine/src/rwe/frozen_rwe_bindings.rs", "engine/src/rwe/operator_corpus.rs", "scripts/verify_rwe_snapshot.py", "tests/test_verify_rwe_snapshot.py"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["docs/CURRENT_STATUS.md", "docs/NEXT_DECISION.md", "engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Rebuild and provider-free validate the frozen pre-AC Harness beside the accepted post-AC Harness under isolated identities.", "ordered_steps": ["docs/CURRENT_STATUS.md, docs/NEXT_DECISION.md, docs/FUTURE_ROUTE.md: promote one provider-free reconstruction window and bind the accepted predecessor.", "engine/src/rwe/operator_corpus.rs, engine/src/rwe/corpus.rs, engine/src/rwe/execution_schedule.rs, engine/src/rwe/frozen_rwe_bindings.rs, engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json: materialize only isolated old/new reconstruction identities and hash-bound artifacts.", "scripts/verify_rwe_snapshot.py, tests/test_verify_rwe_snapshot.py: verify the source commit, recipe overlay, frozen corpus inputs, and provider-free reconstruction traces."], "packet_id": "PE7-RWE-CR-RECONSTRUCTION-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178`"], "prerequisites": ["PE7-AC7-CLOSEOUT-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "ae7adfa9c87ed461649dd8f61dcf329e35372f22a1eb1f31f4c84a352587d831", "read_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/NEXT_DECISION.md", "engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json", "engine/src/rwe/corpus.rs", "engine/src/rwe/execution_schedule.rs", "engine/src/rwe/frozen_rwe_bindings.rs", "engine/src/rwe/operator_corpus.rs", "scripts/verify_rwe_snapshot.py", "tests/test_verify_rwe_snapshot.py", "engine/tests/test_pg_integration.rs"], "risk_class": "none", "rollback": "Revert the reconstruction adapter/artifact change and retain the accepted post-AC Harness; do not alter the frozen pre-AC snapshot or accepted RWE corpus identities.", "route_manifest_sha256": "8569f8aad2dd6672e75a44ed6f61c72f852c0a3290744d957e0bc762d9acde7f", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["git diff --check", "cargo fmt --all -- --check", "cargo clippy -p engine --all-targets --all-features -- -D warnings", "cargo test -p engine", "python -m unittest discover -s tests -p test_agent_*.py"], "verification_family": "source_focused_full", "worker_tier": "T1"}
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
