# Next Decision

Last updated: 2026-08-13.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. Provider-free viability preflight is accepted. The current window is `PE7-RWE-V2-VIABILITY-RUN-1` `READY_FOR_EXECUTION`: a human-operator T3 GO (2026-08-13) authorizes attempting the exact named four-cell v2 action through existing owners. T3 ≠ EFFECT; this GO is not the run receipt. The weak-agent dispatch lane does not consume authority.

## Authoritative Forward Order

```text
[window: PE7-RWE-V2-VIABILITY-RUN-1 — READY_FOR_EXECUTION, four-cell EFFECT]

→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-V2-VIABILITY-RUN-1` — `READY_FOR_EXECUTION`

## Completed (PE7-RWE-V2-VIABILITY-PREFLIGHT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #437 exact head `4bf6f33c9318369c99a0920eac2048527bea2e83`; merge `97ca257345460e1939662b8ffaf602c0a668028a`; exact-head `PASS`; canonical workflow `31698417170`; unissued request sha256 `015c94e9d65a902f3aba5eae4f3da6cba6d534cc3c57af3a6faf89125663469a`.

## Packet PE7-RWE-V2-VIABILITY-RUN-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-RWE-V2-VIABILITY-PREFLIGHT-1 — COMPLETE on accepted main `97ca257345460e1939662b8ffaf602c0a668028a` (PR #437 exact head `4bf6f33c9318369c99a0920eac2048527bea2e83`; merge `97ca257345460e1939662b8ffaf602c0a668028a`; exact-head `PASS`; canonical workflow `31698417170`).

**Class:** `EFFECT`

**Worker tier:** `T3`

**Risk class:** `external_effect`

**Verification family:** `external_effect_evidence`

**Outcome:** Issue one new finite one-use authorization and execute exactly the accepted four-cell v2 schedule once through existing store and coordinator owners. The human-operator T3 GO binds that named action; it does not invent a freeze TTL or skip this node.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md`, `engine/src/rwe/live_baseline_coordinator.rs`, `engine/src/storage/local_product_store/rwe_authority.rs`. Operator-side redacted evidence and existing delegated lifecycle only. No second store, no default-branch write, no auto-merge.

**Exit:** All four cells reach honest terminal classifications with complete request journal, usage/cost, cleanup, artifact/output, and restricted raw-evidence bindings.

**Stop:** Authority or hash mismatch, duplicate/stale identity, outcome unknown, budget breach, Provider/model drift, evidence-path failure, contamination, or target-default-branch write.

### T3 decision

Disposition: `GO`.
T3 owner: human operator (user) on 2026-08-13.
Recorder: implementer; not the T3 owner and not a delegated T3 source.
Named action: Issue one new finite one-use authorization and execute exactly the accepted four-cell v2 schedule once through existing store and coordinator owners.
Named target: `Igzela/alters-lab` (Draft PR / `acp/*` only; no default-branch write; no auto-merge).
This GO binds the retained request below. It is not the EFFECT receipt and does not rewrite a missing outcome as success. Packet state is `READY_FOR_EXECUTION`, not `T3_REQUIRED` (that token is not a valid packet State). The weak-agent capsule below is the provider-free coding dispatch lane only: `external_effect_limit=0` and `authority_consumption_allowed=false` are required; it does not execute the EFFECT.

<!-- route-t3-request:v1
{"accepted_main_sha": "97ca257345460e1939662b8ffaf602c0a668028a", "action_digest": "ad004bab81ebac0942037a428f41240ff0f570ccacb7b0bfd198093f2a1e38a9", "authority_owner_digest": "f69570458f2445057f92abb09f1f9eb1dbb559b5cd0528b10da244bd8db124a9", "candidate_digest": "876f81bd436bdcf714b061aea7b527735df35a9728eb78688fd33c98923500ae", "packet_id": "PE7-RWE-V2-VIABILITY-RUN-1", "requested_action": "Issue one new finite one-use authorization and execute exactly the accepted four-cell v2 schedule once.", "schema_version": "route_t3_request.v1", "scope_digest": "76a86114a9ab92337297f44c572bc0747dc8b24ee0aa27c7425f4f05ace16b50"}
-->

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["Human T3 GO bound to the exact named four-cell action.", "Redacted request-journal and terminal-classification receipts after the operator EFFECT."], "allowed_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/rwe/live_baseline_coordinator.rs", "engine/src/storage/local_product_store/rwe_authority.rs"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Accepted packet receipt index. (docs/CURRENT_STATUS.md:Accepted)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not write the target default branch.", "Do not auto-merge."], "forbidden_next_actions": ["Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not invent a B2 duration freeze constant.", "Do not consume RWE authority or execute the four-cell EFFECT from this dispatch lane."], "goal": "Record the human T3 GO for the exact named four-cell action and persist redacted evidence after the operator EFFECT. This dispatch lane does not consume authority or execute the EFFECT.", "ordered_steps": ["docs/NEXT_DECISION.md, docs/CURRENT_STATUS.md: Keep the human T3 GO bound to the exact named four-cell action and keep the no-result gap honest.", "docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md: After the operator EFFECT, record redacted terminal evidence without promoting CLOSEOUT."], "packet_id": "PE7-RWE-V2-VIABILITY-RUN-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #437 exact head `4bf6f33c9318369c99a0920eac2048527bea2e83`; merge `97ca257345460e1939662b8ffaf602c0a668028a`; exact-head `PASS`; canonical workflow `31698417170`; unissued request sha256 `015c94e9d65a902f3aba5eae4f3da6cba6d534cc3c57af3a6faf89125663469a`"], "prerequisites": ["PE7-RWE-V2-VIABILITY-PREFLIGHT-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "168070ffb25ff255ce252ea7d58c0dc056e5e92a2079c1578bf05f5abc75a9ad", "read_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/rwe/live_baseline_coordinator.rs", "engine/src/storage/local_product_store/rwe_authority.rs"], "risk_class": "none", "rollback": "Revert the current window and retain detailed lifecycle evidence. (proved by docs/NEXT_DECISION.md:Emergency-stop)", "route_manifest_sha256": "f05c33326baf991c1faf40a64aed95cd5e52e9baec58039978c7313997583247", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["git diff --check", "python scripts/check_agent_handoff.py"], "verification_family": "evidence_review", "worker_tier": "T1"}
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
