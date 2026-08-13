# Next Decision

Last updated: 2026-08-13.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

## Authoritative Forward Order

```text
[window: PE7-RWE-V2-PREFLIGHT-GATE-CONTRACT-1 — READY_FOR_EXECUTION, provider-free B1/B2/provenance contract]

→ [bounded repair → provider-free PREFLIGHT]
→ [viability RUN — typed T3 pause]
→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-V2-PREFLIGHT-GATE-CONTRACT-1` — `READY_FOR_EXECUTION`

## Completed (PE7-ROUTE-AUTOPILOT-SOAK-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #429 exact head `92e9b49c13b51ee9c471a6acc2181c37d8084029`; merge `d40c8ce82101922e7270f30bd6da592d72354ffe`; exact-head `PASS`; canonical workflow `31681024633`; OpenCode worker PR #426 exact head `c54860674fbf5045239469c2a842ec88002bb3df`; merge `f02d58b5d1fb8d74dd1c68349e4075eb7641879e`; ledger #383 trusted CI/review/merge/closeout; canonical workflow `31664342318`.
## Packet PE7-RWE-V2-PREFLIGHT-GATE-CONTRACT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-ROUTE-AUTOPILOT-SOAK-1 — COMPLETE on accepted main `d40c8ce82101922e7270f30bd6da592d72354ffe` (PR #429 exact head `92e9b49c13b51ee9c471a6acc2181c37d8084029`; merge `d40c8ce82101922e7270f30bd6da592d72354ffe`; exact-head `PASS`; canonical workflow `31681024633`).

**Class:** `CONTRACT`

**Outcome:** Re-derive against accepted main the authoritative B1 preflight timestamp source, store-owned B2 expiry derivation, and Golden Path test-tooling provenance disposition required before provider-free RWE v2 preflight can be executed.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md`, `engine/src/product_golden_path.rs`, `engine/src/rwe/live_baseline_coordinator.rs`, `engine/src/storage/local_product_store/rwe_authority.rs`. Inventory and a hash-bound repair contract only. Do not modify a Provider, credential, target, issuance, admission, spend, schedule, evaluator, budget, or result.

**Exit:** One independently reviewed contract names exact existing owners, paths, validation, compatibility, recovery, rollback, evidence destinations, and unresolved decisions for B1/B2/provenance. Any unknown owner or authority value is DECISION_REQUIRED.

**Stop:** Caller-controlled expiry, unauthenticated clock/timestamp, missing provenance, a schema/authority decision without an owner, or a proposal that issues/consumes authority, reads a credential, calls a Provider, or changes the v2 freeze.

### Twelve-field contract

1. **Outcome and non-goals.** Re-derive the B1 timestamp source, store-owned B2 expiry, and Golden Path provenance disposition. No Provider, issuance, admission, spend, schedule, or EFFECT.
2. **Prerequisites and evidence.** Accepted main `d40c8ce82101922e7270f30bd6da592d72354ffe`; checked route manifest SHA `0dcd90ebc78919b3cf3dd985aac67d46498c4a8a20d279c04497600f3a8a7413`; predecessor receipt PR #429 exact head `92e9b49c13b51ee9c471a6acc2181c37d8084029`; merge `d40c8ce82101922e7270f30bd6da592d72354ffe`; exact-head `PASS`; canonical workflow `31681024633`; current-main evidence SHA `7985076a59fc23ff08398f613cea4d9a29b976c82d135c2ccd0531ed6b39034a`.
3. **Owners and paths.** Owners: engine/src/rwe/live_baseline_coordinator.rs, engine/src/storage/local_product_store/rwe_authority.rs, engine/src/product_golden_path.rs; callers: docs/MODULE_MAP.md; tests: docs/NEXT_DECISION.md.
4. **Frozen invariants.** Packet identity, route manifest SHA `0dcd90ebc78919b3cf3dd985aac67d46498c4a8a20d279c04497600f3a8a7413`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, target write, automatic merge, Provider call, or second owner.
7. **Ordered implementation slices.** engine/src/rwe/live_baseline_coordinator.rs, engine/src/storage/local_product_store/rwe_authority.rs, engine/src/product_golden_path.rs, docs/MODULE_MAP.md: Inventory the accepted B1 timestamp, store-owned B2 expiry, and Golden Path provenance owners and name the exact repair contract.; docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md: Keep one current window and record unresolved owner decisions as DECISION_REQUIRED.
8. **Failure, recovery, and stop taxonomy.** Cleanup: Compact routing state to one current window rather than retain transition history. (proved by scripts/agent-control/route_driver.py:compact_next_window); retention: Retain detailed lifecycle evidence in the existing ledger and merged history. (proved by docs/NEXT_DECISION.md:retain); decisions: authority unchanged (docs/NEXT_DECISION.md:authority); evaluator unchanged (docs/NEXT_DECISION.md:evaluator); recovery unchanged (docs/NEXT_DECISION.md:recovery); schema unchanged (docs/NEXT_DECISION.md:schema).
9. **Verification.** git diff --check; python scripts/check_agent_handoff.py
10. **Compatibility, rollback, and retention.** Revert the routing/status documents and restore the soak current window. (proved by docs/NEXT_DECISION.md:Emergency-stop)
11. **Exit artifact.** Evidence destinations: Accepted packet receipt index. (docs/CURRENT_STATUS.md:Accepted).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["Hash-bound B1/B2/provenance repair contract naming existing owners only.","Canonical routing and accepted-status synchronization."],"allowed_paths": ["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/product_golden_path.rs","engine/src/rwe/live_baseline_coordinator.rs","engine/src/storage/local_product_store/rwe_authority.rs"],"authority_consumption_allowed": false,"dispatch_lane": "provider_free_docs_contract","expected_artifacts": ["Accepted packet receipt index. (docs/CURRENT_STATUS.md:Accepted)"],"external_effect_limit": 0,"forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.","Do not create a second controller, ledger, queue, lease, store, or workflow owner.","Do not call a Provider, mint T3 authority, execute an EFFECT, auto-merge, or write a target."],"forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.","Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.","Do not start a successor whose promotion candidate has not been independently accepted.","Do not use FUTURE_ROUTE static paths as current-main authority.","Do not create a second controller, ledger, queue, lease, store, or workflow owner.","Do not mint T3 authority, execute an EFFECT, auto-merge, or write a target."],"goal": "Re-derive against accepted main the authoritative B1 preflight timestamp source, store-owned B2 expiry derivation, and Golden Path test-tooling provenance disposition required before provider-free RWE v2 preflight can be executed.","ordered_steps": ["engine/src/rwe/live_baseline_coordinator.rs, engine/src/storage/local_product_store/rwe_authority.rs, engine/src/product_golden_path.rs, docs/MODULE_MAP.md: Inventory the accepted B1 timestamp, store-owned B2 expiry, and Golden Path provenance owners and name the exact repair contract.","docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md: Keep one current window and record unresolved owner decisions as DECISION_REQUIRED."],"packet_id": "PE7-RWE-V2-PREFLIGHT-GATE-CONTRACT-1","packet_state": "READY_FOR_EXECUTION","pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.","Stop before a target write, automatic merge, authority consumption, or external effect.","Do not retry a possibly executed external effect whose outcome is unknown."],"plan_lane_state": "plan_lane_active","prerequisite_receipts": ["Closeout PR #429 exact head `92e9b49c13b51ee9c471a6acc2181c37d8084029`; merge `d40c8ce82101922e7270f30bd6da592d72354ffe`; exact-head `PASS`; canonical workflow `31681024633`; OpenCode worker PR #426 exact head `c54860674fbf5045239469c2a842ec88002bb3df`; merge `f02d58b5d1fb8d74dd1c68349e4075eb7641879e`; ledger #383 trusted CI/review/merge/closeout; canonical workflow `31664342318`"],"prerequisites": ["PE7-ROUTE-AUTOPILOT-SOAK-1"],"private_paths_allowed": false,"promotion_evidence_sha256": "7985076a59fc23ff08398f613cea4d9a29b976c82d135c2ccd0531ed6b39034a","read_paths": ["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/product_golden_path.rs","engine/src/rwe/live_baseline_coordinator.rs","engine/src/storage/local_product_store/rwe_authority.rs"],"risk_class": "none","rollback": "Revert the routing/status documents and restore the soak current window. (proved by docs/NEXT_DECISION.md:Emergency-stop)","route_manifest_sha256": "0dcd90ebc78919b3cf3dd985aac67d46498c4a8a20d279c04497600f3a8a7413","schema_version": "weak_agent_dispatch.v1","secret_values_allowed": false,"verification": ["git diff --check","python scripts/check_agent_handoff.py"],"verification_family": "docs_evidence_review","worker_tier": "T2"}
-->

### B1/B2/provenance repair contract

Hash-bound inventory against accepted main `787663825327043f77eb0d896b048b7fa043c73f`.

1. **B1 preflight timestamp.** `operator_preflight` in `engine/src/rwe/live_baseline_coordinator.rs` emits `rwe_operator_preflight.v1` with no store-derived freshness field. The accepted clock owner is `LocalProductStore::now` (`engine/src/storage/local_product_store/mod.rs`), which reads the store-owned `clock` closure. Repair: preflight must call `store.now()`, persist that RFC3339 UTC value on the receipt, and fail closed if the clock is missing or unparseable. Caller-supplied timestamps are not B1 evidence.
2. **B2 expiry derivation.** `rwe_authority` `issue` / v2 issue paths take `request.expires_at` and only run `require_finite_rwe_expiry`. That is caller-controlled expiry. Repair: the store must derive `expires_at` from `store.now()` plus one frozen finite duration owned by the existing RWE freeze/authority module; reject caller-supplied expiry. `is_at_or_before` remains the admission comparison.
3. **Golden Path test-tooling provenance.** `product_golden_path::compile_product_executable_graph` takes caller `created_at`. Coordinator live-cell setup uses `store.now()` then a 24h offset, with a hardcoded `2026-08-06T00:00:00Z` parse fallback. Tests/fixtures use hardcoded `2026-07-25T12:00:00Z`. Repair: production GP/RWE created_at must come from `store.now()`; fixture clocks must be labeled non-authoritative and must not satisfy preflight freshness. Transport provenance stays on the existing store-owned `transport_provenance` gate (`external` required to seal).
4. **Unresolved / DECISION_REQUIRED.** The store clock owner file is outside this packet's allowed_paths. The repair implementation packet must add `engine/src/storage/local_product_store/mod.rs` and focused tests to its allowed_paths at promotion time. Duration of store-derived B2 expiry is not frozen here: the IMPLEMENT packet must take it from an existing freeze constant or stop DECISION_REQUIRED rather than invent one.
5. **Compatibility, recovery, rollback.** No schema change in this contract. IMPLEMENT must preserve SQLite/PostgreSQL parity, keep fail-closed missing-clock behavior, and revert by restoring caller-supplied expiry plus preflight-without-timestamp. Evidence destination: this section plus `docs/CURRENT_STATUS.md` Accepted receipts.

Contract digest (SHA-256 of this section body without this line, UTF-8): `c8ea4c802e2554b1fa5d0b2f247879ba758d67e4d5df23ed43f1eddadf8aef74`.

## Common Execution Protocol

- Refresh accepted main, the current packet, exact PR heads, CI, review, and ledger receipts before every transition.
- Derive a route action only from the accepted current window, the checked inventory, current-main evidence, and existing durable owners.
- Keep changing PRs Draft; require stable-head independent review and canonical exact-head CI before governed manual merge.
- Treat ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures as bounded recovery transitions through their existing owners.
- Preserve exact receipt bindings and failed/unknown evidence; never convert absence, stale evidence, or an unproven external outcome to success.
- Emergency-stop: revert the current window and retain detailed lifecycle evidence. authority, evaluator, recovery, and schema remain unchanged.

## Hard Stops

- no Provider call; no credential read, target write, release, deployment, automatic merge, EFFECT execution, or T3 action without its separate exact authority;
- no second controller, ledger, queue, lease, workflow owner, store, scheduler, evaluator, authority, or persistence owner;
- no future-route path/prose, model output, local checkpoint, or candidate PR accepted as current-main authority;
- no stale/ambiguous owner, caller, path, verification, rollback, cleanup, retention, evidence, schema, evaluator, authority, or recovery fact treated as proved;
- no retry of an outcome-unknown effect and no deletion or concealment of failure, pause, repair, or recovery evidence.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is the sole routing-only index. Promotion removes exactly one eligible packet, re-derives every `REFRESH_AT_PROMOTION` field from accepted main, validates the resulting candidate, and independently reviews the routing change. No future sketch, static path, or profile alone authorizes code or an effect.
