# Next Decision

Last updated: 2026-08-13.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. Provider-free viability preflight is accepted. The current window is the four-cell viability RUN parked as `DECISION_REQUIRED`: it is not `READY_FOR_EXECUTION` and must not execute an EFFECT until a finite T3 GO and independent receipt exist.

## Authoritative Forward Order

```text
[window: PE7-RWE-V2-VIABILITY-RUN-1 — DECISION_REQUIRED, T3/EFFECT pause]

→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-V2-VIABILITY-RUN-1` — `DECISION_REQUIRED`

## Completed (PE7-RWE-V2-VIABILITY-PREFLIGHT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #437 exact head `4bf6f33c9318369c99a0920eac2048527bea2e83`; merge `97ca257345460e1939662b8ffaf602c0a668028a`; exact-head `PASS`; canonical workflow `31698417170`; unissued request sha256 `015c94e9d65a902f3aba5eae4f3da6cba6d534cc3c57af3a6faf89125663469a`.

## Packet PE7-RWE-V2-VIABILITY-RUN-1

**State:** `DECISION_REQUIRED`

**Prerequisite:** PE7-RWE-V2-VIABILITY-PREFLIGHT-1 — COMPLETE on accepted main `97ca257345460e1939662b8ffaf602c0a668028a` (PR #437 exact head `4bf6f33c9318369c99a0920eac2048527bea2e83`; merge `97ca257345460e1939662b8ffaf602c0a668028a`; exact-head `PASS`; canonical workflow `31698417170`).

**Class:** `EFFECT`

**Outcome:** Park the accepted four-cell v2 run until a finite T3 GO and independent EFFECT receipt exist. Do not issue, admit, spend, call a Provider, or write a target from this window.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md` only while this window stays parked. Do not execute the four-cell EFFECT.

**Exit:** A finite T3 GO bound to the exact request plus an independent verified EFFECT receipt, or an accepted NO-GO/暂停 that keeps the four-cell unrun.

**Stop:** Executing the four-cell run, minting T3 as success without a receipt, skipping this EFFECT node, calling a Provider, writing a target, or inventing a B2 TTL.

### Decision required

This parked window carries no weak-agent dispatch capsule and is not `READY_FOR_EXECUTION`. T3 ≠ EFFECT: the retained request below is not a GO and does not authorize a Provider POST, spend, or four-cell run.

<!-- route-t3-request:v1
{"accepted_main_sha": "97ca257345460e1939662b8ffaf602c0a668028a", "action_digest": "ad004bab81ebac0942037a428f41240ff0f570ccacb7b0bfd198093f2a1e38a9", "authority_owner_digest": "f69570458f2445057f92abb09f1f9eb1dbb559b5cd0528b10da244bd8db124a9", "candidate_digest": "876f81bd436bdcf714b061aea7b527735df35a9728eb78688fd33c98923500ae", "packet_id": "PE7-RWE-V2-VIABILITY-RUN-1", "requested_action": "Issue one new finite one-use authorization and execute exactly the accepted four-cell v2 schedule once.", "schema_version": "route_t3_request.v1", "scope_digest": "76a86114a9ab92337297f44c572bc0747dc8b24ee0aa27c7425f4f05ace16b50"}
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
