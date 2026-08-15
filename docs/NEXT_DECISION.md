# Next Decision

Last updated: 2026-08-15.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1`, `PE7-RWE-MR-CORPUS-SAMPLING-1`, `PE7-RWE-MR-OPERATIONS-EVIDENCE-1`, and `PE7-RWE-MR-PROTOCOL-FREEZE-1` are accepted on main with their source-bound measurement, corpus, operations, and protocol contracts. The reconstructable snapshot replacement and provider-free DB preflight are accepted. The later DB RUN is retained as a non-baseline controlled failure and removed from the forward AC prerequisite chain; this planning decision does not claim an EFFECT receipt, T3 closeout, or decision-grade baseline.

## Authoritative Forward Order

```text
[window: PE7-AC0-RUNTIME-INVENTORY-1 — READY_FOR_EXECUTION, provider-free architecture inventory can proceed without the parked DB baseline]

→ `PE7-AC0-RUNTIME-INVENTORY-1` — execute the bounded provider-free inventory; do not execute an external effect
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-AC0-RUNTIME-INVENTORY-1` — `READY_FOR_EXECUTION`

## Historical V2 Closeout

**State:** `COMPLETE`

**Evidence:** Disposition `CONTROLLED_FAILURE`; run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; four frozen cells; restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`; no seal and no target-default-branch write. Do not rerun this effect.

## Completed snapshot closeout

The accepted `PE7-RWE-DB-SNAPSHOT-CORPUS-1` packet is closed on main by PR #448 exact head `923d9f750c652a268b3d7944be35f34c2a2f9fac`, squash merge `a4472b9a0aa9c78d1616e9d22c88c2f6a6405cb8`, exact-head review receipt `5289908799`, canonical workflow `31773697000`, and final exact-head check `31773696854`.

Its manifest sha256 is `d13834c8ad41376f2884c906b335dce3a397fa0464ba83da0af6310fe2837ce2`; the snapshot disposition is `UNAVAILABLE_NOW`, `reconstructable=false`. No Provider call, authority consumption, target write, or EFFECT occurred. The complete lifecycle receipt and unavailable disposition are owned by `docs/CURRENT_STATUS.md`; this document retains only the closeout binding needed for the current route.

The accepted reconstructable replacement is bound by PR #451 exact head `d48e9853856714a964709956651fc0ac0961315c`, squash merge `e1ff80b7599d8aec8d64909f937f79c948010392`, canonical workflow `31790256137`, and manifest sha256 `a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c`. Its `preflight_promotion=BLOCKED_UNTIL_ACCEPTED` condition is satisfied by that accepted merge; this document now owns the current promotion to provider-free preflight.

## Packet PE7-AC0-RUNTIME-INVENTORY-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-RWE-DB-PREFLIGHT-1 — COMPLETE; the provider-free preflight returned `ready=true` with zero blockers and no authority/provider/target effect.

**Class:** `CONTRACT`

**Outcome:** Enumerate every production subprocess spawn/kill/reap site, executor adapter, environment/config read, timeout/cancellation path, and affected test fixture before any AC ownership move.

**Allowed delta:** Provider-free inventory and call-graph evidence only; update only `docs/CURRENT_STATUS.md` and `docs/NEXT_DECISION.md`; no refactor, deletion, provider call, target write, or external effect.

**Owner/seam:** Reuse the existing Rust runtime, scheduler, executor, and test owners; add no second runtime, store, scheduler, evaluator, or persistence owner.

**Exit:** A zero-unknown runtime/executor matrix with exact callers, owners, failure semantics, golden traces, and candidate migration groups.

**Stop:** A spawn/effect path cannot be classified, ownership conflicts, or static search disagrees with executable traces. Return `DECISION_REQUIRED` and preserve the partial inventory; do not guess or widen scope.

**Rollback:** Revert this planning-only route change and restore the parked route pointers; retain all DB failure evidence and do not replay the run.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-AC0-RUNTIME-INVENTORY-1","dispatch_lane":"t2-contract-inventory","plan_lane_state":"plan_lane_active","goal":"Build a provider-free, exact runtime and executor inventory for AC0 without changing ownership or source behavior.","rollback":"Revert only the canonical inventory and packet-status document edits while retaining the evidence that was already recorded; do not delete or rewrite DB failure evidence.","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"allowed_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md"],"read_paths":["START_HERE.md","AGENTS.md","docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","docs/MODULE_MAP.md","docs/ARCHITECTURE_BOOK.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","engine","src","tests","tools","scripts"],"allowed_outputs":["runtime and executor inventory matrix","exact caller and owner map","failure and rollback classification","bounded AC1 migration candidate list","canonical status and route updates"],"prerequisites":["PE7-RWE-DB-PREFLIGHT-1"],"prerequisite_receipts":["Real same-tenant Store-owned Golden Path prerequisite completed by ProductTask `ptask-20260814135947-18cbb0b9731e62bf`, run `run-0007`, terminal evidence `product-terminal-ptask-20260814135947-18cbb0b9731e62bf-9-44d49301c781`; Draft PR-only output `Igzela/alters-lab#47`, target main `6240768506320a324d68787b9eaa86971c8c930c`; provider-free `rwe_operator_preflight.v1` `ready=true`, zero blockers, no authority/provider/target effect; projection sha256 `b8c35d4060d98598ce3e3bc3977a84d125b1df09ff66b2b9f6d9aa4303c03954`"],"forbidden_changes":["Provider calls","authority consumption","target writes","source refactors or deletions","schema or migration changes","new runtime, scheduler, store, evaluator, or persistence owner","raw prompts, outputs, credentials, or private paths"],"ordered_steps":["Read the accepted owner and architecture contracts","Enumerate subprocess spawn, kill, reap, executor, environment, timeout, cancellation, and fixture paths","Bind every path to its current caller, owner, failure behavior, rollback point, and golden trace","Record only the bounded inventory and candidate migration groups","Run the handoff and diff checks"],"verification":["uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"pause_gates":["unknown or conflicting runtime path","owner or caller ambiguity","static inventory disagrees with executable evidence","required evidence would expose secrets or private paths","inventory requires source behavior or authority changes"],"expected_artifacts":["provider-free AC0 runtime/executor matrix","exact caller and owner evidence","updated accepted status and next-packet handoff"],"forbidden_next_actions":["Do not call a Provider","Do not consume authority","Do not write a target branch","Do not refactor or delete source","Do not start AC1","Do not replay the parked DB RUN","Do not claim a decision-grade baseline"],"known_store_mutations":["No LocalProductStore mutation; inventory is read-only"]}
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
