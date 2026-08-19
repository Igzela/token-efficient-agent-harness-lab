# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-HE-EC1-CONTRACT-1` is complete. The current window is `PE7-HE-EC1-IDENTITY-LINEAGE-1`: immutable identity/lineage recording bound to CWS default-off Harness `84b1933b`. No selection, adoption, ENABLE, or Level-1.

## Authoritative Forward Order

```text
[window: PE7-HE-EC1-IDENTITY-LINEAGE-1 — READY_FOR_EXECUTION, provider-free; immutable lineage]


```

## Active Routing

1. `PE7-HE-EC1-IDENTITY-LINEAGE-1` — `READY_FOR_EXECUTION`

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

## Packet PE7-HE-EC1-IDENTITY-LINEAGE-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-HE-EC1-CONTRACT-1`

**Class:** `IMPLEMENT`

**Outcome:** Implement immutable identity and lineage recording under existing artifact/store owners, including source identities for later causal manifests, bound to CWS default-off SHA `84b1933b`.

**Allowed delta:** `engine/src/harness_evolution.rs`, `engine/src/storage/local_product_store/harness_evolution.rs`, `engine/src/storage/local_product_store/schema.rs`, `engine/src/storage/local_product_store/migrations.rs`, `engine/src/storage/local_product_store/pg_backend/migrations.rs`, `engine/src/storage/local_product_store/integrity.rs`, `engine/tests/test_data_operations.rs`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`. Contract-approved records, hashes, validation, and projections only; no selection or adoption.

**Exit:** Tamper/replay/duplicate/restart/parity tests prove immutable ancestry, exact active-Harness binding, and no orphan causal-evidence reference.

**Stop:** Requires a second store, mutable ancestry, candidate-controlled identity, destructive migration, or Level-1.

### Twelve-field contract

1. **Outcome and non-goals.** Immutable identity/lineage persistence. No selection, adoption, generation, ENABLE, or Level-1.
2. **Prerequisites and evidence.** EC1 contract COMPLETE on PR #591 / `e116e212ed043d773e215f2ba029e5b2f1763e4d`.
3. **Owners and paths.** `engine/src/harness_evolution.rs` plus existing `LocalProductStore` HE owner.
4. **Frozen invariants.** Active Harness SHA `84b1933bc3d9e657acae94d9e5f14810c0651917`. Lineage IDs derived. Ancestry insert-only.
5. **Only semantic delta.** `Ec1IdentityLineageRecord` plus store insert/get and additive table.
6. **Forbidden changes.** No second store, evaluator, selection, or destructive schema rollback.
7. **Ordered slices.** Derive/seal records; persist; reject orphan causal sources; restart-load.
8. **Failure taxonomy.** Wrong Harness, asserted identity, missing parent, orphan causal, immutable conflict.
9. **Verification.** Focused cargo tests, handoff, security, rustfmt.
10. **Compatibility and rollback.** Revert this PR; table is additive `CREATE IF NOT EXISTS`.
11. **Exit artifact.** Durable lineage rows bound to default-off SHA.
12. **Next action.** Promote `PE7-HE-EC1-CAUSAL-MANIFEST-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-HE-EC1-IDENTITY-LINEAGE-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Record immutable EC1 identity/lineage bound to CWS default-off Harness 84b1933b.","allowed_paths":["engine/src/harness_evolution.rs","engine/src/storage/local_product_store/harness_evolution.rs","engine/src/storage/local_product_store/schema.rs","engine/src/storage/local_product_store/migrations.rs","engine/src/storage/local_product_store/pg_backend/migrations.rs","engine/src/storage/local_product_store/integrity.rs","engine/tests/test_data_operations.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["engine/src/harness_evolution.rs","engine/src/storage/local_product_store/harness_evolution.rs","engine/src/storage/local_product_store/schema.rs","engine/src/storage/local_product_store/migrations.rs","engine/src/storage/local_product_store/pg_backend/migrations.rs","engine/src/storage/local_product_store/integrity.rs","engine/tests/test_data_operations.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md"],"allowed_outputs":["Immutable EC1 identity lineage records persisted by LocalProductStore."],"prerequisites":["PE7-HE-EC1-CONTRACT-1"],"prerequisite_receipts":["PE7-HE-EC1-CONTRACT-1 COMPLETE: PR #591 exact head 50661a622c19e1f6da1f934a43bcbbaa4b52a003; squash merge e116e212ed043d773e215f2ba029e5b2f1763e4d"],"forbidden_changes":["Do not create a second store.","Do not mutate ancestry.","Do not ENABLE the laboratory.","Do not start PE7-HE-LEVEL1-PREFLIGHT-1."],"ordered_steps":["Seal derived identity/lineage records.","Persist insert-only under LocalProductStore.","Reject orphan causal sources and missing parents.","Prove replay, tamper, and restart."],"verification":["cargo test -p engine --lib ec1_identity_lineage_binds_default_off_sha_and_rejects_orphans -- --test-threads=1","cargo test -p engine --lib records_immutable_ec1_identity_lineage -- --test-threads=1","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this PR; identity lineage table is additive CREATE IF NOT EXISTS and the laboratory stays default-off.","pause_gates":["Stop before causal-manifest persistence of hypotheses.","Stop before Level-1."],"expected_artifacts":["Ec1IdentityLineageRecord","harness_evolution_ec1_identity_lineage"],"forbidden_next_actions":["Do not start PE7-HE-LEVEL1-PREFLIGHT-1."],"worker_tier":"T1","known_store_mutations":["harness_evolution_ec1_identity_lineage"]}
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
