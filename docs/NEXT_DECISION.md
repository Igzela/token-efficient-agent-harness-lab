# Next Decision

Last updated: 2026-08-11.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, a Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

## Authoritative Forward Order

```text
[window: Route automation — READY_FOR_EXECUTION, provider-free control-plane implementation]
→ [route-autopilot adversarial soak — provider-free]
→ [PREFLIGHT B1/B2/provenance contract → bounded repair → provider-free PREFLIGHT]
→ [viability RUN — typed T3 pause]
→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-ROUTE-AUTOPILOT-SOAK-1` — `READY_FOR_EXECUTION`

## Completed (PE7-ROUTE-AUTOMATION-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #390 exact head `24618e52c969adc93e7bc092c51dde6b2d0ffea9`; merge `5481053c736e7db8481cabd9316741f2a5cd6c7a`; exact-head `PASS`; canonical workflow `31467821768`.
## Packet PE7-ROUTE-AUTOPILOT-SOAK-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-ROUTE-AUTOMATION-1 — COMPLETE on accepted main `83bbdc43507c2731d15375c27e20897de66a1618` (PR #390 exact head `24618e52c969adc93e7bc092c51dde6b2d0ffea9`; merge `5481053c736e7db8481cabd9316741f2a5cd6c7a`; exact-head `PASS`; canonical workflow `31467821768`).

**Class:** `CLOSEOUT`

**Outcome:** Exercise the accepted route controller through clean crossings and adversarial provider-free recovery cases: worker failure, CI repair, review repair, crash/restart, duplicate dispatch/PR prevention, main drift, stale checkpoint, merge-before-closeout crash, promotion crash, DECISION_REQUIRED, NO_GO rewrite, EFFECT/T3 pause/resume, OUTCOME_UNKNOWN, and route exhaustion.

**Allowed delta:** docs/MODULE_MAP.md, docs/NEXT_DECISION.md, docs/FUTURE_ROUTE.md, docs/CURRENT_STATUS.md, scripts/agent-control/route_driver.py, scripts/agent-control/local_run_once.py, tests/test_agent_route_driver.py.

**Exit:** A bounded, independently reviewed soak report proves the controller traverses multiple packet boundaries without manual successor authoring, preserves exactly one current window, recovers ordinary failures through existing owners, pauses at exact authority gates, and validates no-growth traversal of the original 116-packet portfolio plus the canonical route-control entries.

**Stop:** Any second lifecycle owner; an unproved transition; duplicate/non-idempotent PR, dispatch, merge, or promotion; route prose treated as authority; an EFFECT executed or skipped; a missing T3 receipt; an outcome-unknown effect retried; or a recovery condition that cannot be proved from existing owners.

### Twelve-field contract

1. **Outcome and non-goals.** Exercise the accepted route controller through clean crossings and adversarial provider-free recovery cases: worker failure, CI repair, review repair, crash/restart, duplicate dispatch/PR prevention, main drift, stale checkpoint, merge-before-closeout crash, promotion crash, DECISION_REQUIRED, NO_GO rewrite, EFFECT/T3 pause/resume, OUTCOME_UNKNOWN, and route exhaustion.
2. **Prerequisites and evidence.** Accepted main `83bbdc43507c2731d15375c27e20897de66a1618`; checked route manifest SHA `e5152d5c51888edde4e1b01a33ade89171c560de6da01121cf14332fb16a242b`; predecessor receipt PR #390 exact head `24618e52c969adc93e7bc092c51dde6b2d0ffea9`; merge `5481053c736e7db8481cabd9316741f2a5cd6c7a`; exact-head `PASS`; canonical workflow `31467821768`; current-main evidence SHA `e805edc919a8a18df9bec04125dd12ce31835de01e3a39d8f71e6783654c29ed`.
3. **Owners and paths.** Owners: scripts/agent-control/route_driver.py; callers: scripts/agent-control/local_run_once.py; tests: tests/test_agent_route_driver.py.
4. **Frozen invariants.** Packet identity, route manifest SHA `e5152d5c51888edde4e1b01a33ade89171c560de6da01121cf14332fb16a242b`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** scripts/agent-control/route_driver.py, scripts/agent-control/local_run_once.py, tests/test_agent_route_driver.py: Exercise the accepted promotion boundary through its exact caller and verifier tests, including rejection, recovery, pause, and bounded-compaction cases.; docs/MODULE_MAP.md, docs/NEXT_DECISION.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md: Synchronize only canonical ownership, current-window, accepted-receipt, and inventory evidence after the provider-free soak.
8. **Failure, recovery, and stop taxonomy.** Cleanup: Use existing lifecycle cleanup only; retain bounded diagnostic hashes and never persist raw prompts, credentials, private paths, or restricted evidence. (proved by docs/NEXT_DECISION.md:cleanup); retention: Retain existing ledger and lifecycle receipts plus bounded diagnostic hashes without changing a persistence or retention-policy owner. (proved by docs/NEXT_DECISION.md:retention); decisions: authority unchanged (docs/NEXT_DECISION.md:authority); evaluator unchanged (docs/MODULE_MAP.md:evaluator); recovery unchanged (docs/NEXT_DECISION.md:recovery); schema unchanged (docs/NEXT_DECISION.md:schema).
9. **Verification.** git diff --check
10. **Compatibility, rollback, and retention.** Revert packet code and canonical document synchronization while retaining existing lifecycle, pause, failure, and outcome-unknown receipts. (proved by docs/NEXT_DECISION.md:Revert)
11. **Exit artifact.** Evidence destinations: Existing controller-owned transition receipts, bounded soak report, checked-inventory traversal proof, implementation-cost receipt, and documented revert path. (docs/NEXT_DECISION.md:Exit), Canonical accepted packet receipt rows after independently reviewed and verified closeout. (docs/CURRENT_STATUS.md:Accepted).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "docs/FUTURE_ROUTE.md", "docs/CURRENT_STATUS.md", "scripts/agent-control/route_driver.py", "scripts/agent-control/local_run_once.py", "tests/test_agent_route_driver.py"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Existing controller-owned transition receipts, bounded soak report, checked-inventory traversal proof, implementation-cost receipt, and documented revert path. (docs/NEXT_DECISION.md:Exit)", "Canonical accepted packet receipt rows after independently reviewed and verified closeout. (docs/CURRENT_STATUS.md:Accepted)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Exercise the accepted route controller through clean crossings and adversarial provider-free recovery cases: worker failure, CI repair, review repair, crash/restart, duplicate dispatch/PR prevention, main drift, stale checkpoint, merge-before-closeout crash, promotion crash, DECISION_REQUIRED, NO_GO rewrite, EFFECT/T3 pause/resume, OUTCOME_UNKNOWN, and route exhaustion.", "ordered_steps": ["scripts/agent-control/route_driver.py, scripts/agent-control/local_run_once.py, tests/test_agent_route_driver.py: Exercise the accepted promotion boundary through its exact caller and verifier tests, including rejection, recovery, pause, and bounded-compaction cases.", "docs/MODULE_MAP.md, docs/NEXT_DECISION.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md: Synchronize only canonical ownership, current-window, accepted-receipt, and inventory evidence after the provider-free soak."], "packet_id": "PE7-ROUTE-AUTOPILOT-SOAK-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #390 exact head `24618e52c969adc93e7bc092c51dde6b2d0ffea9`; merge `5481053c736e7db8481cabd9316741f2a5cd6c7a`; exact-head `PASS`; canonical workflow `31467821768`"], "prerequisites": ["PE7-ROUTE-AUTOMATION-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "e805edc919a8a18df9bec04125dd12ce31835de01e3a39d8f71e6783654c29ed", "read_paths": ["docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "docs/FUTURE_ROUTE.md", "docs/CURRENT_STATUS.md", "scripts/agent-control/route_driver.py", "scripts/agent-control/local_run_once.py", "tests/test_agent_route_driver.py"], "risk_class": "none", "rollback": "Revert packet code and canonical document synchronization while retaining existing lifecycle, pause, failure, and outcome-unknown receipts. (proved by docs/NEXT_DECISION.md:Revert)", "route_manifest_sha256": "e5152d5c51888edde4e1b01a33ade89171c560de6da01121cf14332fb16a242b", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["git diff --check"], "verification_family": "evidence_review", "worker_tier": "T2"}
-->

## Common Execution Protocol

- Refresh accepted main, the current packet, exact PR heads, CI, review, and ledger receipts before every transition.
- Derive a route action only from the accepted current window, the checked inventory, current-main evidence, and existing durable owners.
- Keep changing PRs Draft; require stable-head independent review and canonical exact-head CI before governed manual merge.
- Treat ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures as bounded recovery transitions through their existing owners.
- Preserve exact receipt bindings and failed/unknown evidence; never convert absence, stale evidence, or an unproven external outcome to success.

## Hard Stops

- no Provider call, credential read, target write, release, deployment, automatic merge, EFFECT execution, or T3 action without its separate exact authority;
- no second controller, ledger, queue, lease, workflow owner, store, scheduler, evaluator, authority, or persistence owner;
- no future-route path/prose, model output, local checkpoint, or candidate PR accepted as current-main authority;
- no stale/ambiguous owner, caller, path, verification, rollback, cleanup, retention, evidence, schema, evaluator, authority, or recovery fact treated as proved;
- no retry of an outcome-unknown effect and no deletion or concealment of failure, pause, repair, or recovery evidence.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is the sole routing-only index. Promotion removes exactly one eligible packet, re-derives every `REFRESH_AT_PROMOTION` field from accepted main, validates the resulting candidate, and independently reviews the routing change. No future sketch, static path, or profile alone authorizes code or an effect.
