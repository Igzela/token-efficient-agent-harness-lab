# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-HE-EC2-CONTRACT-1` is complete. The current window is `PE7-HE-EC2-HOLDOUT-SEAL-1`: materialize sealed holdout identities, hash-only labels, access mediation, audit, and invalidation/rotation. No candidate run, ENABLE, or Level-1.

## Authoritative Forward Order

```text
[window: PE7-HE-EC2-HOLDOUT-SEAL-1 — READY_FOR_EXECUTION, provider-free; seal holdout identities and mediate access]


```

## Active Routing

1. `PE7-HE-EC2-HOLDOUT-SEAL-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-BENCHMARK-PREFLIGHT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #588 exact head `c806f75c5910b117c3cf7e44ad1c6a6503e48ddd`; squash merge `1569c70e9f2034bb4f7bc5ccbc24d889b66645ab`.

## Completed (PE7-CWS-BENCHMARK-RUN-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #589 exact head `0f9cad12a850a7ed2ffcc823ebd2da29318c5ae6`; squash merge `84b1933bc3d9e657acae94d9e5f14810c0651917`; exact-head review comments `5347630853` and `5347631083`; canonical workflow `32298813456`; exact-head check `32298813444`; `executed=false`; `provider_posts=0`.

## Completed (PE7-CWS-ANALYSIS-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #590 exact head `da09ea576154e55e532d2de5477972f2c5c516d5`; squash merge `1544c8d0a3f1b196fdb4b560759609662cd5f432`; exact-head review comments `5347818781` and `5347818993`; canonical workflow `32301497907`; exact-head check `32301497898`; `INSUFFICIENT_DEFAULT_OFF`; active Harness `84b1933bc3d9e657acae94d9e5f14810c0651917`.

## Completed (PE7-HE-EC1-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #591 exact head `50661a622c19e1f6da1f934a43bcbbaa4b52a003`; squash merge `e116e212ed043d773e215f2ba029e5b2f1763e4d`; exact-head review comments `5348443154` and `5348443354`; canonical workflow `32306087501`.

## Completed (PE7-HE-EC1-IDENTITY-LINEAGE-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #592 exact head `155fa749effdcd790fb954eefcf64d12790d21b6`; squash merge `3dc2d3b12fbb95ec2b26220681cba5ad7547c6d2`; exact-head review comments `5348823217` and `5348823396`; canonical workflow `32309602816`.

## Completed (PE7-HE-EC1-CAUSAL-MANIFEST-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #593 exact head `c00f24dac433d9b3fc23f5b0df746c89442097dd`; squash merge `b2fa400395a0502bf52ea5fd9468af5830766422`; exact-head review comments `5349023567` and `5349023702`; canonical workflow `32311374839`.

## Completed (PE7-HE-EC1-MUTATION-REGISTRY-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #594 exact head `b3199736d85312083c45a3522211ae086f5fe756`; squash merge `b970226181957de98859f26f03db3bf101b1f8a0`; exact-head review comments `5349266593` and `5349266718`; canonical workflow `32313718374`.

## Completed (PE7-HE-EC2-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #595 exact head `e0585701dec206fca5645299d65cbb3341257008`; squash merge `f996ded631f12f74f42528c70e76ccf0f040bdfd`; exact-head review comments `5349652629` and `5349652752`; canonical workflow `32317253205`.

## Packet PE7-HE-EC2-HOLDOUT-SEAL-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-HE-EC2-CONTRACT-1`

**Class:** `IMPLEMENT`

**Outcome:** Materialize sealed holdout identities, hash-only labels, access mediation, audit, and invalidation/rotation under the existing evaluator and LocalProductStore owners.

**Allowed delta:** `engine/src/harness_evolution_eval.rs`, `engine/src/storage/local_product_store/harness_evolution.rs`, `engine/src/storage/local_product_store/schema.rs`, `engine/src/storage/local_product_store/migrations.rs`, `engine/src/storage/local_product_store/pg_backend/migrations.rs`, `engine/src/storage/local_product_store/integrity.rs`, `engine/tests/test_data_operations.rs`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`. No candidate run or evaluator rule change.

**Exit:** Unauthorized-read, label-tamper, leakage, restart, audit, and deletion/rotation tests pass.

**Stop:** Raw sensitive content would be committed, candidate identity gains access, or seal cannot survive restart.

### Twelve-field contract

1. **Outcome and non-goals.** Seal identities and mediate hash-only membership reads. No candidate run, ENABLE, or Level-1.
2. **Prerequisites and evidence.** EC2 CONTRACT COMPLETE on PR #595 / `f996ded631f12f74f42528c70e76ccf0f040bdfd`.
3. **Owners and paths.** Existing `harness_evolution_eval.rs` and LocalProductStore HE tables.
4. **Frozen invariants.** Candidates cannot observe plaintext labels. Operator controller cannot read membership. Evaluator/reviewer see hashes only. Sensitive keys refused. Additive v36 `CREATE TABLE IF NOT EXISTS`.
5. **Only semantic delta.** `Ec2HoldoutSeal` persist/read/rotate plus access mediation.
6. **Forbidden changes.** No plaintext labels, no candidate run, no second store/evaluator, no schema_version bump, no Level-1.
7. **Ordered slices.** Seal vault; persist insert-only; mediate reads; rotate/invalidate; stop before sentinel wiring.
8. **Failure taxonomy.** Unauthorized read, label tamper, leakage, missing actor, immutable vault collision.
9. **Verification.** Focused cargo tests, integrity census 72, handoff, security, rustfmt.
10. **Compatibility and rollback.** Revert this PR; table is additive IF NOT EXISTS.
11. **Exit artifact.** Digest-bound `Ec2HoldoutSeal` rows and audit events.
12. **Next action.** Promote `PE7-HE-EC2-SENTINEL-CONFORMANCE-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-HE-EC2-HOLDOUT-SEAL-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Materialize sealed holdout identities, hash-only labels, access mediation, audit, and rotation without a candidate run.","allowed_paths":["engine/src/harness_evolution_eval.rs","engine/src/storage/local_product_store/harness_evolution.rs","engine/src/storage/local_product_store/schema.rs","engine/src/storage/local_product_store/migrations.rs","engine/src/storage/local_product_store/pg_backend/migrations.rs","engine/src/storage/local_product_store/integrity.rs","engine/tests/test_data_operations.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["engine/src/harness_evolution_eval.rs","engine/src/harness_evolution.rs","engine/src/storage/local_product_store/harness_evolution.rs","engine/src/storage/local_product_store/schema.rs","engine/src/storage/local_product_store/migrations.rs","engine/src/storage/local_product_store/pg_backend/migrations.rs","engine/src/storage/local_product_store/integrity.rs","engine/tests/test_data_operations.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md"],"allowed_outputs":["Digest-bound Ec2HoldoutSeal rows and audit events."],"prerequisites":["PE7-HE-EC2-CONTRACT-1"],"prerequisite_receipts":["PE7-HE-EC2-CONTRACT-1 COMPLETE: PR #595 exact head e0585701dec206fca5645299d65cbb3341257008; squash merge f996ded631f12f74f42528c70e76ccf0f040bdfd"],"forbidden_changes":["Do not persist plaintext labels or secrets.","Do not run a candidate evaluation.","Do not create a second evaluator or store.","Do not ENABLE the laboratory.","Do not start PE7-HE-LEVEL1-PREFLIGHT-1."],"ordered_steps":["Seal holdout vault from family label hashes.","Persist insert-only rows with audit.","Mediate evaluator/reviewer hash-only reads.","Rotate and invalidate prior vaults.","Stop before sentinel conformance."],"verification":["cargo test -p engine --lib holdout_seal_denies_candidate_and_detects_label_tamper -- --test-threads=1","cargo test -p engine --lib persists_ec2_holdout_seal_with_access_mediation_and_rotation -- --test-threads=1","cargo test -p engine --test test_data_operations check_integrity_on_clean_database -- --test-threads=1","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this PR; additive harness_evolution_ec2_holdout_seals is unused and the laboratory stays default-off.","pause_gates":["Stop before sentinel conformance.","Stop before Level-1."],"expected_artifacts":["engine/src/harness_evolution_eval.rs Ec2HoldoutSeal seal_ec2_holdout","engine/src/storage/local_product_store/harness_evolution.rs persist_ec2_holdout_seal"],"forbidden_next_actions":["Do not start PE7-HE-LEVEL1-PREFLIGHT-1."],"worker_tier":"T1","known_store_mutations":["harness_evolution_ec2_holdout_seals"]}
-->

## Common Execution Protocol

- `READY_FOR_EXECUTION` packets require a valid dispatch capsule; blocked or decision-required packets carry no executable capsule.
- Refresh accepted `main`, the current packet, exact PR head, CI, and review receipts before each transition.
- Keep a changing PR Draft; run focused/local full checks, then one final stable-head Standards/Spec review, Ready, canonical exact-head CI, manual squash merge, and main refresh.
- No Provider call, credential-value read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner in this packet.
- Unknown, stale, missing, or conflicting evidence remains fail-closed; never retry an effect whose outcome may be unknown.

## Hard Stops

- `DECISION_REQUIRED` when a caller, owner, path, semantics, compatibility fact, rollback, or evidence cannot be re-proved from accepted `main`.
- No Provider call, credential-value read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner.
- Unknown or possibly executed effects are never treated as success or retried.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is routing-only. RUN-1 remains a retained live-ready blocker.
