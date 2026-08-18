# Next Decision

Last updated: 2026-08-18.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze, AC2 typed execution, AC3 Golden Path responsibility split, AC4 transaction views, AC5 composition root, and AC6 Rust-authoritative schema convergence are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC7 removal manifest, deletion-only cleanup, and closeout are accepted; `PE7-RWE-CR-RECONSTRUCTION-1` is complete on accepted main `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`. `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` remains parked at `DECISION_REQUIRED`; its bounded read-only owner repair is now the sole provider-free implementation window. No Provider call, target write, authority consumption, or replay effect is authorized.

## Authoritative Forward Order

```text
[window: PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1 — READY_FOR_EXECUTION, provider-free; read-only owner repair]


```

## Active Routing

1. `PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC7-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178`.

## Completed (PE7-RWE-CR-RECONSTRUCTION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #566 exact head `57f4a5ee3a9be48a6ebdc20eddbd5df978c4440f`; squash merge `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`; exact-head review receipt comment `5324095735`; canonical workflow `32103730088`; exact-head check `32103730089`. The explicit Python 3.14.4 verifier and registered provider-free traces passed. No Provider call, target write, authority consumption, or effect occurred.

The reconstruction contract and historical implementation details remain in PR #566 and its merged diff. Its frozen pre-AC inputs are retained unchanged; the provider-free protocol/preflight contract below is the current planning-parked packet and has no execution authority until its decision-required repair is accepted.

## Parked predecessor: `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1`

**State:** `DECISION_REQUIRED`

The predecessor remains parked after the accepted owner audit recorded in PR #570: its legacy CLI/store path can write Store state and read credential values, and its current freeze does not prove the contemporary two-arm comparison. Its detailed contract and stop receipt remain in the accepted documents and merge history; it is not an executable prerequisite for this repair.

## Packet PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-RWE-CR-RECONSTRUCTION-1` — COMPLETE on accepted main `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`.

**Class:** `IMPLEMENT`

**Outcome:** Implement the smallest existing-owner strict read-only Store/auth/preflight seam and contemporary old/new identity projection. This packet does not run a live preflight, claim credential readiness, alter the frozen protocol/schedule, or authorize a replay.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/FUTURE_ROUTE.md`, `engine/src/storage/local_product_store/mod.rs`, `engine/src/storage/local_product_store/managed_acceptance.rs`, `engine/src/rwe/live_baseline_coordinator.rs`, `engine/src/bin/rwe_live_baseline.rs`, `engine/src/rwe/frozen_rwe_bindings.rs`, `engine/tests/test_operator_evidence.rs`, and `engine/tests/test_pg_integration.rs` only; no schema/migration, live execution, or post-AC measurement change.

**Exit:** Read-only open/auth/preflight and identity-projection focused tests pass; the final review/CI/merge receipt proves no persistent write, credential-value read, authority action, Provider call, target write, or effect; the protocol packet remains parked until its remaining comparison/readiness decisions are separately re-proved.

**Stop:** Any required seam needs a new owner, schema/migration, credential-value read, Store metadata write, protocol/schedule change, fixture-only acceptance, Provider call, authority action, target write, or effect.

### Twelve-field contract

1. **Outcome and non-goals.** Repair only the existing read-only Store/auth/preflight and old/new identity projection owners. Do not run an arm, issue or consume authority, call a Provider, produce an effect, analyze results, or declare the contemporary baseline ready.
2. **Prerequisites and evidence.** Reconstruction is accepted on main `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`; the successor protocol/preflight promotion is accepted as `DECISION_REQUIRED` on `f16e3fc4ffa303b3d93876355b3b1783e988be1c`; the repair starts from accepted main `37a4f752b2fdc3516b7581dffedcebb99c76f6a7`.
3. **Owners and paths.** Reuse only `LocalProductStore`, its managed-authentication owner, `operator_preflight`/CLI, the existing frozen RWE binding owner, and their focused tests; exact writable paths are sealed in the dispatch capsule.
4. **Frozen invariants.** Preserve authority order, store ownership, provider/effect gates, old/new source identities, corpus/protocol/schedule hashes, and all existing fail-closed semantics; no caller-supplied identity or readiness claim becomes authoritative.
5. **Only semantic delta.** Add a non-persistent read-only access mode, a non-touching authentication path, explicit unavailable credential-readiness projection, and existing-owner identity validation.
6. **Forbidden changes.** No Provider call, effect, T3 action, authority consumption, target write, credential-value read/output/persistence, schema/migration, protocol/schedule/threshold change, fixture-based acceptance, new runtime/store/evaluator, or second owner.
7. **Ordered slices.** Add strict read-only Store open; factor non-touching auth; add provider-free preflight projection with explicit unavailable state; validate the existing old/new binding projection; add focused nonmutation and identity-collision tests.
8. **Failure, recovery, and stop taxonomy.** Fail closed on missing DB, attempted write, tenant/scope mismatch, missing redacted readiness, stale/swapped/duplicate identity, or evidence collision; do not retry or consume authority; rollback is a code/docs revert with no data cleanup.
9. **Verification.** Run the exact dispatch-capsule commands; no Provider, credential-value read, authority, target, database creation, or effect command is permitted.
10. **Compatibility, rollback, and retention.** Existing mutable Store/auth/admit/run paths retain their behavior; only the new preflight path is read-only; no schema/data migration; revert the repair PR to roll back.
11. **Exit artifact.** Accepted exact-head review/CI/merge receipt, focused tests proving nonmutation and fail-closed identity checks, and a status handoff that keeps the protocol packet and replay blocked.
12. **Next action.** After this repair is accepted, re-promote or amend the contemporary protocol/preflight contract to resolve its remaining redacted-readiness and comparison-protocol decisions; do not run `PE7-RWE-CR-RUN-1` from this packet.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["A provider-free strict read-only LocalProductStore/RWE owner repair limited to the accepted paths.","Focused read-only seam and identity-projection tests plus exact-head lifecycle evidence.","No live preflight readiness claim, Provider call, authority action, target write, or external effect."],"allowed_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","docs/FUTURE_ROUTE.md","engine/src/storage/local_product_store/mod.rs","engine/src/storage/local_product_store/managed_acceptance.rs","engine/src/rwe/live_baseline_coordinator.rs","engine/src/bin/rwe_live_baseline.rs","engine/src/rwe/frozen_rwe_bindings.rs","engine/tests/test_operator_evidence.rs","engine/tests/test_pg_integration.rs"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["Strict read-only Store open/auth/preflight owner seam with missing-database and metadata-nonmutation behavior.","Existing-owner contemporary old/new identity projection that fails closed on missing, swapped, duplicate, or colliding identities.","Focused tests and accepted status/NEXT receipt showing the repair remains provider-free and does not claim a live-ready baseline."],"external_effect_limit":0,"forbidden_changes":["Do not create a second runtime, scheduler, store, controller, evaluator, authority, or evidence owner.","Do not add schema, migration, default-configuration, WAL, metadata-touch, or other persistent write behavior to the read-only path.","Do not read, output, persist, or validate credential values; use only an existing redacted/readiness owner or explicit unavailable blocker.","Do not modify frozen corpus, protocol, schedule, measurement thresholds, allocation, interleaving, drift, capacity, or authorization semantics.","Do not use fixture/fake results as managed acceptance evidence.","Do not call a Provider, issue or consume authority, execute admit/run, write a target, or perform an effect."],"forbidden_next_actions":["Do not run the legacy constructor-backed preflight against a missing or real Store.","Do not re-promote the contemporary protocol packet or start PE7-RWE-CR-RUN-1 until this repair has an exact-head PASS, canonical CI, and merge-backed closeout.","Do not infer credential readiness, two-arm comparability, interleaving, capacity pairing, or a live-ready baseline from focused tests."],"goal":"Implement the smallest existing-owner strict read-only Store/auth/preflight seam and contemporary identity projection needed to re-open protocol planning without authorizing a live run.","ordered_steps":["Add an existing LocalProductStore read-only open mode that only opens an existing database and cannot create directories, schema, migrations, defaults, WAL writes, or metadata.","Reuse the existing authentication owner through a read-only path that validates tenant/scopes without touching API-key last-used metadata.","Add a provider-free preflight projection that never reads credential values and returns an explicit unavailable blocker when redacted readiness is not supplied.","Project and validate the accepted old/new identities through the existing RWE binding owner, then add focused SQLite/PostgreSQL parity and nonmutation tests where the existing harness permits."],"packet_id":"PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop when an owner, caller, path, operation, destination, or decision cannot be re-proved from accepted main.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.","Stop before any schema/data/metadata write, credential-value read, Provider, authority action, target write, or effect.","Stop when implementing the repair would require changing the frozen protocol/schedule or inventing a new owner."],"plan_lane_state":"plan_lane_active","prerequisite_receipts":["PE7-RWE-CR-RECONSTRUCTION-1 COMPLETE: PR #566 exact head `57f4a5ee3a9be48a6ebdc20eddbd5df978c4440f`; squash merge `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`; exact-head review receipt comment `5324095735`; canonical workflow `32103730088`; exact-head check `32103730089`; explicit Python 3.14.4 verifier and provider-free traces passed; no Provider call, target write, authority consumption, or effect"],"prerequisites":["PE7-RWE-CR-RECONSTRUCTION-1"],"private_paths_allowed":false,"promotion_evidence_sha256":"ff28f69819fecfd7a5442ea4b6291632b59db53c1240cf8655fe58815439a23a","read_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","docs/FUTURE_ROUTE.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","engine/src/storage/local_product_store/mod.rs","engine/src/storage/local_product_store/managed_acceptance.rs","engine/src/rwe/live_baseline_coordinator.rs","engine/src/bin/rwe_live_baseline.rs","engine/src/rwe/frozen_rwe_bindings.rs","engine/src/rwe/operator_corpus.rs","engine/tests/test_operator_evidence.rs","engine/tests/test_pg_integration.rs"],"risk_class":"store_mutation","rollback":"Revert the repair PR and retain the parked protocol packet; the repair must not require schema/data cleanup because its read-only path cannot persist state.","route_manifest_sha256":"2b78045a6d4bb5df5ef7965d78ae8978102c6e80dbc7e5b53b5d57f58e25be75","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["cargo fmt --all -- --check","cargo test -p engine viability_preflight_is_read_only_without_store_creation_or_auth_touch","cargo test -p engine current_comparison_manifest_rejects_identity_collision","cargo test -p engine --features pg-tests --test test_pg_integration","git diff --check","bash scripts/check_wire_codegen_drift.sh","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"verification_family":"source_focused_full","worker_tier":"T2","known_store_mutations":[]}
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

`docs/FUTURE_ROUTE.md` is routing-only. The parked protocol packet and the promoted repair packet remain governed by this document; no future sketch authorizes code or an effect until promoted into this document from accepted `main`.
