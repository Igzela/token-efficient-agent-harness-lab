# Next Decision

Last updated: 2026-08-14.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1`, `PE7-RWE-MR-CORPUS-SAMPLING-1`, `PE7-RWE-MR-OPERATIONS-EVIDENCE-1`, and `PE7-RWE-MR-PROTOCOL-FREEZE-1` are accepted on main with their source-bound measurement, corpus, operations, and protocol contracts. The snapshot packet is merged and closed with an honest `UNAVAILABLE_NOW` reconstruction result; the preflight successor is the current blocked window and no external effect is authorized.

## Authoritative Forward Order

```text
[window: PE7-RWE-DB-PREFLIGHT-1 — BLOCKED_PREREQUISITE, snapshot reconstruction unavailable]

→ `PE7-RWE-DB-RUN-1` only after a reconstructable preflight is accepted
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-DB-PREFLIGHT-1` — `BLOCKED_PREREQUISITE`

No `READY_FOR_EXECUTION` packet is active; the retained snapshot closeout is a completed lifecycle receipt, not a dispatch capsule.

## Historical V2 Closeout

**State:** `COMPLETE`

**Evidence:** Disposition `CONTROLLED_FAILURE`; run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; four frozen cells; restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`; no seal and no target-default-branch write. Do not rerun this effect.

## Completed snapshot closeout

The accepted `PE7-RWE-DB-SNAPSHOT-CORPUS-1` packet is closed on main by PR #448 exact head `923d9f750c652a268b3d7944be35f34c2a2f9fac`, squash merge `a4472b9a0aa9c78d1616e9d22c88c2f6a6405cb8`, exact-head review receipt `5289908799`, canonical workflow `31773697000`, and final exact-head check `31773696854`.

Its manifest sha256 is `d13834c8ad41376f2884c906b335dce3a397fa0464ba83da0af6310fe2837ce2`; the snapshot disposition is `UNAVAILABLE_NOW`, `reconstructable=false`. No Provider call, authority consumption, target write, or EFFECT occurred. The complete lifecycle receipt and unavailable disposition are owned by `docs/CURRENT_STATUS.md`; this document retains only the closeout binding needed for the current route.

## Packet PE7-RWE-DB-PREFLIGHT-1

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-RWE-DB-SNAPSHOT-CORPUS-1 — COMPLETE on accepted main `a4472b9a0aa9c78d1616e9d22c88c2f6a6405cb8`, but its manifest is `UNAVAILABLE_NOW` and `reconstructable=false`.

**Class:** `CONTRACT`

**Outcome:** Validate the frozen corpus, snapshot, protocol, schedule, capacity, principals, target state, evidence destinations, and drift baseline before any external-effect authorization is considered.

**Allowed delta:** Provider-free contract and evidence validation only. No Provider call, task execution, authority issue/admit/consume, target write, EFFECT, T3 action, release, deployment, or runtime/schema/store/evaluator change.

**Owner/seam:** Reuse the existing RWE operator preflight, corpus/protocol/schedule integrity validators, `LocalProductStore` authority/evidence owners, and `live_baseline_coordinator`; add no parallel owner.

**Required bindings:** Snapshot manifest `d13834c8ad41376f2884c906b335dce3a397fa0464ba83da0af6310fe2837ce2`, corpus `044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20`, protocol `bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db`, schedule `6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38`.

**Exit:** A zero-mismatch provider-free preflight receipt with every required binding reconstructable, and only then bounded operator authorization requests under the accepted experiment envelope.

**Stop:** Any required snapshot field, source artifact, lockfile, toolchain pin, capacity, price, Provider identity, target safety, reviewer availability, retention destination, or drift binding is unavailable, stale, conflicting, or unverifiable. Preserve `UNAVAILABLE_NOW`; do not guess or proceed.

**Current disposition:** `BLOCKED_PREREQUISITE`. The accepted snapshot explicitly lacks the exact source active YAML artifacts, a Python dependency lockfile, and a checked-in Rust toolchain pin. No preflight command, Provider call, authority consumption, or target interaction is permitted until a separately accepted reconstructable snapshot replaces this prerequisite.

**Rollback:** Revert only this contract/promotion documentation; retain the snapshot manifest and its unavailable evidence.

**Next permitted action:** Remain provider-free and await a new accepted reconstructable snapshot prerequisite; do not promote `PE7-RWE-DB-RUN-1`.

## Common Execution Protocol

- Refresh accepted main, the current packet, exact PR heads, CI, review, and ledger receipts before every transition.
- Derive a route action only from the accepted current window, the checked inventory, current-main evidence, and existing durable owners.
- Existing route boundary (quoted for compatibility, not new packet authority): The sole exception is the current packet's dispatch-capsule-authorized, one-per-claim local OpenCode weak-worker Provider invocation; it cannot make the controller read, pass, persist, or report a credential. This packet's external-effect limit is zero and does not use that exception.
- Keep changing PRs Draft; require stable-head independent review and canonical exact-head CI before governed manual merge.
- Treat ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures as bounded recovery transitions through their existing owners.
- Preserve exact receipt bindings and failed/unknown evidence; never convert absence, stale evidence, or an unproven external outcome to success.
- Emergency-stop: revert the current window and retain detailed lifecycle evidence. Authority, evaluator, recovery, and schema remain unchanged.

## Hard Stops

- no Provider call; no credential read, target write, release, deployment, automatic merge, EFFECT execution, or T3 action without its separate exact authority;
- no second controller, ledger, queue, lease, workflow owner, store, scheduler, evaluator, authority, or persistence owner;
- no future-route path/prose, model output, local checkpoint, or candidate PR accepted as current-main authority;
- no stale/ambiguous owner, caller, path, verification, rollback, cleanup, retention, evidence, schema, evaluator, authority, or recovery fact treated as proved;
- no retry of an outcome-unknown effect and no deletion or concealment of failure, pause, repair, or recovery evidence.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is the sole routing-only index. Promotion removes exactly one eligible packet, re-derives every `REFRESH_AT_PROMOTION` field from accepted main, validates the resulting candidate, and independently reviews the routing change. No future sketch, static path, or profile alone authorizes code or an effect.
