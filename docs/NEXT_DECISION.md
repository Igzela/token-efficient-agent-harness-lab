# Next Decision

Last updated: 2026-08-13.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The current window is parked: B1 freshness and Golden Path/RWE `created_at` provenance are on accepted main, but store-derived B2 authorization expiry has no accepted freeze duration. Do not invent a TTL, do not treat caller-supplied `expires_at` as a completed repair, and do not promote the viability preflight while that value is unowned.

## Authoritative Forward Order

```text
[window: PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1 — DECISION_REQUIRED, B2 freeze duration unowned]

→ [planning-owned B2 duration, then provider-free PREFLIGHT]
→ [viability RUN — typed T3 pause]
→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1` — `DECISION_REQUIRED`

## Completed (PE7-ROUTE-AUTOPILOT-SOAK-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #429 exact head `92e9b49c13b51ee9c471a6acc2181c37d8084029`; merge `d40c8ce82101922e7270f30bd6da592d72354ffe`; exact-head `PASS`; canonical workflow `31681024633`; OpenCode worker PR #426 exact head `c54860674fbf5045239469c2a842ec88002bb3df`; merge `f02d58b5d1fb8d74dd1c68349e4075eb7641879e`; ledger #383 trusted CI/review/merge/closeout; canonical workflow `31664342318`.
## Completed (PE7-RWE-V2-PREFLIGHT-GATE-CONTRACT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #432 exact head `f31ba002720424deb003728eec52aa9ceae35e33`; merge `710ce06fee68fb75889aa5fa3b9e031b4fdc3a50`; exact-head `PASS`; canonical workflow `31686429471`; contract digest `c8ea4c802e2554b1fa5d0b2f247879ba758d67e4d5df23ed43f1eddadf8aef74`
## Packet PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1

**State:** `DECISION_REQUIRED`

**Prerequisite:** PE7-RWE-V2-PREFLIGHT-GATE-CONTRACT-1 — COMPLETE on accepted main `710ce06fee68fb75889aa5fa3b9e031b4fdc3a50` (PR #432 exact head `f31ba002720424deb003728eec52aa9ceae35e33`; merge `710ce06fee68fb75889aa5fa3b9e031b4fdc3a50`; exact-head `PASS`; canonical workflow `31686429471`; contract digest `c8ea4c802e2554b1fa5d0b2f247879ba758d67e4d5df23ed43f1eddadf8aef74`).

**Class:** `IMPLEMENT`

**Outcome:** B1 and Golden Path/RWE `created_at` provenance are implemented on accepted main `e311db76bf4d2a3a407213b8129a600bc447fd56` (PR #434 exact head `9fdd1045928f862a5b1c1017bc0e9d73e5d50966`; exact-head `PASS`; canonical workflow `31690000442`). Store-derived B2 authorization expiry cannot be completed: no accepted freeze duration exists on current main. The 24-hour value is the existing `DelegationContract` invariant, not an RWE authorization TTL. Planning must name an existing owner and finite duration, or accept that caller-supplied expiry remains until a later contract. Do not invent a TTL here.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md` only while this window stays parked. Do not implement a B2 duration, issue or admit authority, call a Provider, or promote `PE7-RWE-V2-VIABILITY-PREFLIGHT-1`.

**Exit:** A planning-owned freeze duration bound to an existing owner, or an accepted contract that keeps caller-supplied expiry as the durable B2 rule. Either result must be independently reviewed before this packet can close COMPLETE.

**Stop:** Inventing a B2 duration, treating caller-supplied expiry as a completed store-derived repair, promoting viability preflight, minting T3, executing an EFFECT, or writing a target.

### Decision required

No accepted freeze-duration owner exists in `engine/src/rwe/` corpus, protocol, schedule, or economic-protocol freeze artifacts. `LocalProductStore::issue_rwe_run_authorization_v2` therefore still takes caller `expires_at`. That remaining caller-supplied B2 path is not a completed repair and is not authority to invent a TTL.

Options for the planning owner:
1. Name an existing finite duration already owned on main and bind B2 derivation to `store.require_now()` plus that duration.
2. Accept caller-supplied finite expiry as the durable B2 rule in a later contract.
3. Park the route until a human names the duration.

This parked window carries no weak-agent dispatch capsule and is not `READY_FOR_EXECUTION` until planning owns a B2 duration.

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
