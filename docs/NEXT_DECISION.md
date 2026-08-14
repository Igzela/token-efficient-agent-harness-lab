# Next Decision

Last updated: 2026-08-13.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. The current window is `PE7-RWE-MR-ESTIMANDS-1` `DECISION_REQUIRED`: minimum meaningful effects and other authority-critical value judgments still lack an explicit T2/human owner. This window is not `READY_FOR_EXECUTION`.

## Authoritative Forward Order

```text
[window: PE7-RWE-MR-ESTIMANDS-1 — DECISION_REQUIRED, human value judgments]

→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-MR-ESTIMANDS-1` — `DECISION_REQUIRED`

## Completed (PE7-RWE-V2-VIABILITY-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** Disposition `CONTROLLED_FAILURE`. Run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; cells `cell-rwe-minimum-t1-fix_flow_linkage-r1-bp-standard-s2026080601`, `cell-rwe-minimum-t1-fix_flow_linkage-r2-bp-standard-s2026080602`, `cell-rwe-minimum-t2-draft_contract_tests-r1-bp-standard-s2026080601`, `cell-rwe-minimum-t2-draft_contract_tests-r2-bp-standard-s2026080602`. Restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`. No seal; no target-default-branch write. Promotion PR #442 exact head `50e18540f40a8d47c384f2cac74683618f93c273`; merge `8c5c2f85bc5d66c08d730b7d0c69d914af19540c`; canonical workflow `31710478692`.

## Packet PE7-RWE-MR-ESTIMANDS-1

**State:** `DECISION_REQUIRED`

**Prerequisite:** PE7-RWE-V2-VIABILITY-CLOSEOUT-1

**Class:** `CONTRACT`

**Outcome:** Park the measurement-readiness estimand freeze until a T2/human owner supplies every authority-critical value. Accepted protocol non-inferiority margins are not silently adopted as the estimand ledger.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md` only while parked.

**Exit:** An independently reviewed estimand ledger with every threshold source, uncertainty target, and human value judgment explicit, or an accepted pause that keeps measurement readiness unfrozen.

**Stop:** Inventing a minimum meaningful effect, choosing a threshold from the observed `controlled_failure` direction, or treating protocol margins as a complete estimand ledger without an owner.

### Decision required

This parked window carries no weak-agent dispatch capsule and is not `READY_FOR_EXECUTION`. Needed before unpark: decision question, inferential unit, eligible value bases, minimum meaningful effects, hard-gate outcomes, and missing/outcome-unknown rules, each with a named T2/human owner. The accepted v2 protocol non-inferiority numbers remain protocol facts, not this packet's freeze.

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
