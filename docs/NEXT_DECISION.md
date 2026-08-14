# Next Decision

Last updated: 2026-08-15.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1`, `PE7-RWE-MR-CORPUS-SAMPLING-1`, `PE7-RWE-MR-OPERATIONS-EVIDENCE-1`, and `PE7-RWE-MR-PROTOCOL-FREEZE-1` are accepted on main with their source-bound measurement, corpus, operations, and protocol contracts. The reconstructable snapshot replacement, provider-free DB preflight, and separately authorized DB RUN are now closed; the DB RUN disposition is `INSUFFICIENT`, with no decision-grade baseline or downstream AC authorization.

## Authoritative Forward Order

```text
[window: PE7-RWE-DB-ANALYSIS-1 — READY_FOR_EXECUTION, the DB RUN is closed `INSUFFICIENT` and the bounded T2 analysis contract is accepted]

→ `PE7-RWE-DB-ANALYSIS-1` — execute the bounded evidence analysis only; leave AC0 blocked
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-DB-ANALYSIS-1` — `READY_FOR_EXECUTION`

## Historical V2 Closeout

**State:** `COMPLETE`

**Evidence:** Disposition `CONTROLLED_FAILURE`; run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; four frozen cells; restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`; no seal and no target-default-branch write. Do not rerun this effect.

## Completed snapshot closeout

The accepted `PE7-RWE-DB-SNAPSHOT-CORPUS-1` packet is closed on main by PR #448 exact head `923d9f750c652a268b3d7944be35f34c2a2f9fac`, squash merge `a4472b9a0aa9c78d1616e9d22c88c2f6a6405cb8`, exact-head review receipt `5289908799`, canonical workflow `31773697000`, and final exact-head check `31773696854`.

Its manifest sha256 is `d13834c8ad41376f2884c906b335dce3a397fa0464ba83da0af6310fe2837ce2`; the snapshot disposition is `UNAVAILABLE_NOW`, `reconstructable=false`. No Provider call, authority consumption, target write, or EFFECT occurred. The complete lifecycle receipt and unavailable disposition are owned by `docs/CURRENT_STATUS.md`; this document retains only the closeout binding needed for the current route.

The accepted reconstructable replacement is bound by PR #451 exact head `d48e9853856714a964709956651fc0ac0961315c`, squash merge `e1ff80b7599d8aec8d64909f937f79c948010392`, canonical workflow `31790256137`, and manifest sha256 `a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c`. Its `preflight_promotion=BLOCKED_UNTIL_ACCEPTED` condition is satisfied by that accepted merge; this document now owns the current promotion to provider-free preflight.

## Packet PE7-RWE-DB-ANALYSIS-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-RWE-DB-RUN-1 — COMPLETE with `INSUFFICIENT` disposition; the four-cell controlled-failure receipt is recorded in `docs/CURRENT_STATUS.md`.

**Class:** `CLOSEOUT`

**Outcome:** Apply the frozen decision-baseline analysis to the exact DB RUN evidence and decide whether any pre-AC baseline claim is supported.

**Allowed delta:** Analysis and redacted evidence sealing only. Preserve every failure, missing reviewer/verification field, usage/cost limitation, and cleanup result; do not modify the frozen protocol or rerun the effect.

**Owner/seam:** Reuse the existing `LocalProductStore` evidence/usage owners and the accepted measurement-readiness contracts; write only the canonical status and route documents. Add no evaluator, budget, store, or analysis owner.

**Required bindings:** Snapshot manifest `a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c`, corpus `044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20`, protocol `bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db`, schedule `6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38`, DB RUN evidence `a841e6d092d2946de2ee96bef03409ab8c276111c3ace53aef827bd0c00c277e`, and old-Harness target revision `6240768506320a324d68787b9eaa86971c8c930c`.

**Exit:** A redacted, reproducible receipt with `GO`, `NO_GO`, or `INSUFFICIENT`, explicit hard-gate/missingness/cost limits, and the exact old-Harness identity; `INSUFFICIENT` is valid completion and keeps AC0 blocked.

**Stop:** The store evidence cannot be reproduced, a hard gate is unresolvable, cost provenance is ineligible, or analysis would require post-hoc exclusions or protocol changes.

**Current disposition:** `READY_FOR_EXECUTION`. The latest run is already terminal and must be analyzed from durable evidence; no Provider, authority, target, PR, or external effect is permitted.

**Rollback:** Revert only the redacted status/route-document change; retain the original store receipts and all failure/missingness evidence. Never delete or rewrite the run.

**Next permitted action:** Read the bound durable evidence, produce the bounded uncertainty-aware disposition, update the canonical closeout, and leave downstream AC0 unchanged unless a separate accepted decision permits it.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-RWE-DB-ANALYSIS-1","dispatch_lane":"t2-evidence-analysis","plan_lane_state":"plan_lane_active","goal":"Analyze the exact DB RUN receipts and publish only a bounded redacted sufficiency disposition.","rollback":"Revert only canonical redacted status and route-document edits while retaining all LocalProductStore receipts and failure evidence.","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"allowed_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md"],"read_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md"],"allowed_outputs":["redacted analysis receipt","canonical packet disposition","route state update"],"prerequisites":["PE7-RWE-DB-RUN-1 is COMPLETE with INSUFFICIENT disposition","all four frozen cells are terminal controlled failures"],"prerequisite_receipts":["a841e6d092d2946de2ee96bef03409ab8c276111c3ace53aef827bd0c00c277e","e6ab1a1f5516ad52c0d1b431a5b1d52e990f90d7"],"forbidden_changes":["Provider calls","authority consumption","target writes","protocol edits","evaluator or budget changes","raw prompts or outputs","private paths or credentials"],"ordered_steps":["Read the bound store-owned run and task receipts","Check hard gates, missingness, usage/cost provenance, and old-Harness identity","Publish one redacted GO/NO_GO/INSUFFICIENT disposition","Run the handoff and diff checks"],"verification":["uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"pause_gates":["missing or conflicting durable evidence","unreproducible analysis","ineligible cost provenance","post-hoc protocol change"],"expected_artifacts":["redacted uncertainty-aware analysis receipt","updated canonical packet state","unchanged AC0 blocked state"],"forbidden_next_actions":["Do not rerun the DB effect","Do not issue or consume authority","Do not call a Provider","Do not start AC0","Do not claim a viable baseline"],"known_store_mutations":["No LocalProductStore mutation; read-only evidence analysis only"]}
-->


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
