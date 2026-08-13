# Next Decision

Last updated: 2026-08-13.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

## Authoritative Forward Order

```text
[window: PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1 — READY_FOR_EXECUTION, bounded B1/B2/provenance repair]

→ [bounded repair → provider-free PREFLIGHT]
→ [viability RUN — typed T3 pause]
→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1` — `READY_FOR_EXECUTION`

## Completed (PE7-ROUTE-AUTOPILOT-SOAK-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #429 exact head `92e9b49c13b51ee9c471a6acc2181c37d8084029`; merge `d40c8ce82101922e7270f30bd6da592d72354ffe`; exact-head `PASS`; canonical workflow `31681024633`; OpenCode worker PR #426 exact head `c54860674fbf5045239469c2a842ec88002bb3df`; merge `f02d58b5d1fb8d74dd1c68349e4075eb7641879e`; ledger #383 trusted CI/review/merge/closeout; canonical workflow `31664342318`.
## Completed (PE7-RWE-V2-PREFLIGHT-GATE-CONTRACT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #432 exact head `f31ba002720424deb003728eec52aa9ceae35e33`; merge `710ce06fee68fb75889aa5fa3b9e031b4fdc3a50`; exact-head `PASS`; canonical workflow `31686429471`; contract digest `c8ea4c802e2554b1fa5d0b2f247879ba758d67e4d5df23ed43f1eddadf8aef74`
## Packet PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-RWE-V2-PREFLIGHT-GATE-CONTRACT-1 — COMPLETE on accepted main `710ce06fee68fb75889aa5fa3b9e031b4fdc3a50` (PR #432 exact head `f31ba002720424deb003728eec52aa9ceae35e33`; merge `710ce06fee68fb75889aa5fa3b9e031b4fdc3a50`; exact-head `PASS`; canonical workflow `31686429471`; contract digest `c8ea4c802e2554b1fa5d0b2f247879ba758d67e4d5df23ed43f1eddadf8aef74`).

**Class:** `IMPLEMENT`

**Outcome:** Implement only the accepted B1/B2/provenance repair so the preflight receipt has authoritative freshness evidence, any authorization expiry is derived by the existing store authority, and Golden Path test-tooling provenance is validated by its accepted owner.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md`, `engine/src/product_golden_path.rs`, `engine/src/rwe/live_baseline_coordinator.rs`, `engine/src/storage/local_product_store/mod.rs`, `engine/src/storage/local_product_store/rwe_authority.rs`. Focused negative/recovery/parity tests only if they already live in these owners. Do not modify a Provider, credential, target, issuance, admission, spend, schedule, evaluator, budget, or result.

**Exit:** Exact-head reviewed, canonical-CI-green implementation proves store-derived B1/B2/provenance behavior and preserves existing RWE authority, budget, evaluator, and freeze owners with a documented revert path.

**Stop:** Any new store/authority/evaluator owner, incompatible schema/recovery result, ambiguous provenance, caller-supplied B2 expiry, invented B2 duration, stale B1 evidence treated as valid, or any external effect.

### Twelve-field contract

1. **Outcome and non-goals.** Implement the accepted B1/B2/provenance repair only. No Provider, issuance, admission, spend, schedule, or EFFECT.
2. **Prerequisites and evidence.** Accepted main `710ce06fee68fb75889aa5fa3b9e031b4fdc3a50`; checked route manifest SHA `a504c1d9c2385a6887c9e0ca99d345b510ed5aec977f8cd76323e8160cf303ff`; predecessor receipt PR #432 exact head `f31ba002720424deb003728eec52aa9ceae35e33`; merge `710ce06fee68fb75889aa5fa3b9e031b4fdc3a50`; exact-head `PASS`; canonical workflow `31686429471`; contract digest `c8ea4c802e2554b1fa5d0b2f247879ba758d67e4d5df23ed43f1eddadf8aef74`; current-main evidence SHA `66f30e38a278d2f2f87dc76d9cb48044ccdf0d6aed578b4e06a80056ab5429cf`.
3. **Owners and paths.** Owners: engine/src/rwe/live_baseline_coordinator.rs, engine/src/storage/local_product_store/rwe_authority.rs, engine/src/storage/local_product_store/mod.rs, engine/src/product_golden_path.rs; callers: docs/MODULE_MAP.md; tests: docs/NEXT_DECISION.md.
4. **Frozen invariants.** Packet identity, route manifest SHA `a504c1d9c2385a6887c9e0ca99d345b510ed5aec977f8cd76323e8160cf303ff`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no invented freeze duration, effect, T3 action, target write, automatic merge, Provider call, or second owner.
7. **Ordered implementation slices.** engine/src/rwe/live_baseline_coordinator.rs, engine/src/storage/local_product_store/mod.rs: Persist store.now() on operator_preflight and fail closed without a parseable clock.; engine/src/storage/local_product_store/rwe_authority.rs: Derive expires_at from store.now() plus an existing freeze duration, or stop DECISION_REQUIRED.; engine/src/product_golden_path.rs, engine/src/rwe/live_baseline_coordinator.rs: Require store.now() for production created_at and keep fixture clocks non-authoritative.; docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md: Synchronize the accepted closeout receipt after exact-head review and canonical CI.
8. **Failure, recovery, and stop taxonomy.** Cleanup: Compact routing state to one current window rather than retain transition history. (proved by scripts/agent-control/route_driver.py:compact_next_window); retention: Retain detailed lifecycle evidence in the existing ledger and merged history. (proved by docs/NEXT_DECISION.md:retain); decisions: authority unchanged (docs/NEXT_DECISION.md:authority); evaluator unchanged (docs/NEXT_DECISION.md:evaluator); recovery unchanged (docs/NEXT_DECISION.md:recovery); schema unchanged (docs/NEXT_DECISION.md:schema).
9. **Verification.** git diff --check; python scripts/check_agent_handoff.py
10. **Compatibility, rollback, and retention.** Revert the B1/B2/provenance repair and restore caller-supplied expiry plus preflight-without-timestamp. (proved by docs/NEXT_DECISION.md:Emergency-stop)
11. **Exit artifact.** Evidence destinations: Accepted packet receipt index. (docs/CURRENT_STATUS.md:Accepted).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["Store-derived B1 timestamp on preflight receipt.","Store-derived B2 expiry or typed DECISION_REQUIRED if no freeze duration exists.","Fail-closed GP/RWE created_at provenance."],"allowed_paths": ["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/product_golden_path.rs","engine/src/rwe/live_baseline_coordinator.rs","engine/src/storage/local_product_store/mod.rs","engine/src/storage/local_product_store/rwe_authority.rs"],"authority_consumption_allowed": false,"dispatch_lane": "provider_free_source_repair","expected_artifacts": ["Accepted packet receipt index. (docs/CURRENT_STATUS.md:Accepted)"],"external_effect_limit": 0,"forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.","Do not create a second controller, ledger, queue, lease, store, or workflow owner.","Do not call a Provider, mint T3 authority, execute an EFFECT, auto-merge, or write a target."],"forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.","Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.","Do not invent a B2 duration freeze constant.","Do not mint T3 authority, execute an EFFECT, auto-merge, or write a target."],"goal": "Implement only the accepted B1/B2/provenance repair so the preflight receipt has authoritative freshness evidence, any authorization expiry is derived by the existing store authority, and Golden Path test-tooling provenance is validated by its accepted owner.","ordered_steps": ["engine/src/rwe/live_baseline_coordinator.rs, engine/src/storage/local_product_store/mod.rs: Persist store.now() on operator_preflight and fail closed without a parseable clock.","engine/src/storage/local_product_store/rwe_authority.rs: Derive expires_at from store.now() plus an existing freeze duration, or stop DECISION_REQUIRED.","engine/src/product_golden_path.rs, engine/src/rwe/live_baseline_coordinator.rs: Require store.now() for production created_at and keep fixture clocks non-authoritative.","docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md: Synchronize the accepted closeout receipt after exact-head review and canonical CI."],"packet_id": "PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1","packet_state": "READY_FOR_EXECUTION","pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.","Stop before inventing a freeze duration, target write, automatic merge, or external effect."],"plan_lane_state": "plan_lane_active","prerequisite_receipts": ["PR #432 exact head `f31ba002720424deb003728eec52aa9ceae35e33`; merge `710ce06fee68fb75889aa5fa3b9e031b4fdc3a50`; exact-head `PASS`; canonical workflow `31686429471`; contract digest `c8ea4c802e2554b1fa5d0b2f247879ba758d67e4d5df23ed43f1eddadf8aef74`"],"prerequisites": ["PE7-RWE-V2-PREFLIGHT-GATE-CONTRACT-1"],"private_paths_allowed": false,"promotion_evidence_sha256": "66f30e38a278d2f2f87dc76d9cb48044ccdf0d6aed578b4e06a80056ab5429cf","read_paths": ["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/product_golden_path.rs","engine/src/rwe/live_baseline_coordinator.rs","engine/src/storage/local_product_store/mod.rs","engine/src/storage/local_product_store/rwe_authority.rs"],"risk_class": "none","rollback": "Revert the B1/B2/provenance repair and restore caller-supplied expiry plus preflight-without-timestamp. (proved by docs/NEXT_DECISION.md:Emergency-stop)","route_manifest_sha256": "a504c1d9c2385a6887c9e0ca99d345b510ed5aec977f8cd76323e8160cf303ff","schema_version": "weak_agent_dispatch.v1","secret_values_allowed": false,"verification": ["git diff --check","python scripts/check_agent_handoff.py"],"verification_family": "source_focused_full","worker_tier": "T1"}
-->

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
