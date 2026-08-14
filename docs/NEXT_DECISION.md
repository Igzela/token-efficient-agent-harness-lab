# Next Decision

Last updated: 2026-08-15.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1`, `PE7-RWE-MR-CORPUS-SAMPLING-1`, `PE7-RWE-MR-OPERATIONS-EVIDENCE-1`, and `PE7-RWE-MR-PROTOCOL-FREEZE-1` are accepted on main with their source-bound measurement, corpus, operations, and protocol contracts. The reconstructable snapshot replacement and provider-free DB preflight are accepted; the separately authorized DB RUN effect is durably `controlled_failure`, but its packet closeout remains `DECISION_REQUIRED` because the route T3/owner-outcome binding is not proved. There is no decision-grade baseline or downstream AC authorization.

## Authoritative Forward Order

```text
[window: PE7-RWE-DB-RUN-1 — DECISION_REQUIRED, the effect is durably failed but its route T3/owner-outcome binding is not proved]

→ `PE7-RWE-DB-RUN-1` — reconcile the existing effect binding; do not replay the run or promote analysis
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-DB-RUN-1` — `DECISION_REQUIRED`

## Historical V2 Closeout

**State:** `COMPLETE`

**Evidence:** Disposition `CONTROLLED_FAILURE`; run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; four frozen cells; restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`; no seal and no target-default-branch write. Do not rerun this effect.

## Completed snapshot closeout

The accepted `PE7-RWE-DB-SNAPSHOT-CORPUS-1` packet is closed on main by PR #448 exact head `923d9f750c652a268b3d7944be35f34c2a2f9fac`, squash merge `a4472b9a0aa9c78d1616e9d22c88c2f6a6405cb8`, exact-head review receipt `5289908799`, canonical workflow `31773697000`, and final exact-head check `31773696854`.

Its manifest sha256 is `d13834c8ad41376f2884c906b335dce3a397fa0464ba83da0af6310fe2837ce2`; the snapshot disposition is `UNAVAILABLE_NOW`, `reconstructable=false`. No Provider call, authority consumption, target write, or EFFECT occurred. The complete lifecycle receipt and unavailable disposition are owned by `docs/CURRENT_STATUS.md`; this document retains only the closeout binding needed for the current route.

The accepted reconstructable replacement is bound by PR #451 exact head `d48e9853856714a964709956651fc0ac0961315c`, squash merge `e1ff80b7599d8aec8d64909f937f79c948010392`, canonical workflow `31790256137`, and manifest sha256 `a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c`. Its `preflight_promotion=BLOCKED_UNTIL_ACCEPTED` condition is satisfied by that accepted merge; this document now owns the current promotion to provider-free preflight.

## Packet PE7-RWE-DB-RUN-1

**State:** `DECISION_REQUIRED`

**Prerequisite:** PE7-RWE-DB-PREFLIGHT-1 — COMPLETE after provider-free `rwe_operator_preflight.v1` returned `ready=true` with zero blockers and no authority/provider/target effect; the receipt is recorded in `docs/CURRENT_STATUS.md`.

**Class:** `EFFECT`

**Outcome:** Execute the frozen pre-AC decision baseline under the accepted allocation and finite authorizations.

**Allowed delta:** Only the registered baseline task executions, their reviews, and store-owned evidence receipts. No selective rerun, hidden failure, task substitution, threshold change, mid-run protocol repair, release, deployment, or default-branch write.

**Owner/seam:** Reuse the existing `live_baseline_coordinator`, `LocalProductStore` authority/evidence owners, frozen corpus/protocol/schedule validators, managed worker, and target-output owners; add no parallel owner.

**Required bindings:** Snapshot manifest `a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c`, corpus `044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20`, protocol `bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db`, schedule `6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38`, and preflight projection sha256 `b8c35d4060d98598ce3e3bc3977a84d125b1df09ff66b2b9f6d9aa4303c03954`.

**Exit:** Every scheduled unit is terminal with attempts, failures, usage, lifecycle cost, reviewer evidence, drift covariates, cleanup, and raw/redacted bundle bindings.

**Stop:** A registered global stop rule fires, comparability breaks, finite authority expires, an outcome becomes unknown, contamination occurs, or evidence capture fails. Preserve the exact failure and do not rerun selectively.

**Current disposition:** `DECISION_REQUIRED`. The observed effect is uniquely bound to run `run-goal-db-baseline-20260814-2340`, authorization `auth-goal-db-run-20260814`, and run-evidence sha256 `a841e6d092d2946de2ee96bef03409ab8c276111c3ace53aef827bd0c00c277e`; it covered four frozen cells and ProductTasks `ptask-20260814154014-18cbb634bffe42c2`, `ptask-20260814154055-18cbb63e1d04419d`, `ptask-20260814154136-18cbb647bf8562c7`, and `ptask-20260814154219-18cbb651c35d869f`, with all four workspaces reconciled to `cleaned`. No route-controller T3 request, authorized receipt, or independent owner-outcome receipt binds that effect. Preserve the evidence, do not replay it, and do not promote the analysis packet yet.

**Rollback:** Stop before authority issue; if a registered unit reaches an unknown external outcome, enter the existing reconciliation path and preserve the outcome-unknown receipt. Do not delete evidence or retry speculatively.

**Next permitted action:** Reconcile the already-executed effect through the existing route T3/owner-outcome bridge or record the smallest planning decision that closes this discrepancy. Do not replay the effect, infer a route receipt from RWE rows, or promote the analysis packet.

### 11. Weak-Agent Dispatch Capsule

No executable dispatch capsule is present while this packet is `DECISION_REQUIRED`; generate one only after the T3 contract is expanded and revalidated on accepted main.



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
