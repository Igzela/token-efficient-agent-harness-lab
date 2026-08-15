# Next Decision

Last updated: 2026-08-15.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1`, `PE7-RWE-MR-CORPUS-SAMPLING-1`, `PE7-RWE-MR-OPERATIONS-EVIDENCE-1`, and `PE7-RWE-MR-PROTOCOL-FREEZE-1` are accepted on main with their source-bound measurement, corpus, operations, and protocol contracts. The reconstructable snapshot replacement and provider-free DB preflight are accepted; the separately authorized DB RUN effect is durably `controlled_failure` and is closed by the planning decision recorded below. There is no decision-grade baseline or downstream AC authorization.

## Authoritative Forward Order

```text
[window: PE7-RWE-DB-ANALYSIS-1 — DECISION_REQUIRED, awaiting evidence-backed current-main promotion]

→ `PE7-RWE-DB-ANALYSIS-1` — promote the analysis-only closeout; do not replay the run or claim a decision-grade baseline
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-DB-ANALYSIS-1` — `DECISION_REQUIRED`

## Historical V2 Closeout

**State:** `COMPLETE`

**Evidence:** Disposition `CONTROLLED_FAILURE`; run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; four frozen cells; restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`; no seal and no target-default-branch write. Do not rerun this effect.

## Completed snapshot closeout

The accepted `PE7-RWE-DB-SNAPSHOT-CORPUS-1` packet is closed on main by PR #448 exact head `923d9f750c652a268b3d7944be35f34c2a2f9fac`, squash merge `a4472b9a0aa9c78d1616e9d22c88c2f6a6405cb8`, exact-head review receipt `5289908799`, canonical workflow `31773697000`, and final exact-head check `31773696854`.

Its manifest sha256 is `d13834c8ad41376f2884c906b335dce3a397fa0464ba83da0af6310fe2837ce2`; the snapshot disposition is `UNAVAILABLE_NOW`, `reconstructable=false`. No Provider call, authority consumption, target write, or EFFECT occurred. The complete lifecycle receipt and unavailable disposition are owned by `docs/CURRENT_STATUS.md`; this document retains only the closeout binding needed for the current route.

The accepted reconstructable replacement is bound by PR #451 exact head `d48e9853856714a964709956651fc0ac0961315c`, squash merge `e1ff80b7599d8aec8d64909f937f79c948010392`, canonical workflow `31790256137`, and manifest sha256 `a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c`. Its `preflight_promotion=BLOCKED_UNTIL_ACCEPTED` condition is satisfied by that accepted merge; this document now owns the current promotion to provider-free preflight.

## Retained DB RUN (historical: PE7-RWE-DB-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** accepted main `7f4c85834c5f2c0d3b5588ad3a7c83d4c9f12848`

**Disposition:** `CONTROLLED_FAILURE`. The already-executed effect is bound to run `run-goal-db-baseline-20260814-2340`, authorization `auth-goal-db-run-20260814-2340`, and run-evidence sha256 `a841e6d092d2946de2ee96bef03409ab8c276111c3ace53aef827bd0c00c277e`. No route-controller T3 request, authorized receipt, or independent owner-outcome receipt was created or inferred. Do not replay the effect or treat it as a decision-grade baseline.

## Packet PE7-RWE-DB-ANALYSIS-1

**State:** `DECISION_REQUIRED`

**Prerequisite:** PE7-RWE-DB-RUN-1 — COMPLETE by the retained terminal `CONTROLLED_FAILURE` decision on accepted main.

**Class:** `CLOSEOUT`

**Outcome:** Promote the frozen analysis-only closeout for the observed DB run and decide whether the pre-AC baseline is sufficient and safe to carry into convergence.

**Allowed delta:** Evidence-backed promotion and analysis planning only. No provider call, effect replay, target write, protocol change, or baseline claim.

**Exit:** A complete current-main contract for analysis with an independent uncertainty-aware GO, NO_GO, or INSUFFICIENT receipt and the exact reconstructable old-Harness identity.

**Stop:** Current-main ownership, evidence, or analysis reproducibility cannot be proved; hard gates fail; or the data do not support the pre-registered estimands.

**Next permitted action:** Use the existing evidence-backed promotion planner to expand and validate this packet on accepted main, then run only the analysis closeout.



## Common Execution Protocol

- Refresh accepted main, the current packet, exact PR heads, CI, review, and ledger receipts before every transition.
- Derive a route action only from the accepted current window, the checked inventory, current-main evidence, and existing durable owners.
- Existing route boundary (quoted for compatibility, not new packet authority): The sole exception is the current packet's dispatch-capsule-authorized, one-per-claim local OpenCode weak-worker Provider invocation; it cannot make the controller read, pass, persist, or report a credential. This packet's external-effect limit is zero and does not use that exception.
- Keep changing PRs Draft; require stable-head independent review and canonical exact-head CI before governed manual merge.
- `READY_FOR_EXECUTION` packets require a valid dispatch capsule; blocked packets carry no executable capsule.
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
