# Next Decision

Last updated: 2026-08-13.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority. The sole exception is the current packet's dispatch-capsule-authorized, one-per-claim local OpenCode weak-worker Provider invocation; it cannot make the controller read, pass, persist, or report a credential, or grant target, EFFECT, T3, release, deployment, or merge authority.

## Authoritative Forward Order

```text
[window: PE7-ROUTE-AUTOPILOT-SOAK-1 — READY_FOR_EXECUTION, bounded local OpenCode worker]

→ [PREFLIGHT B1/B2/provenance contract → bounded repair → provider-free PREFLIGHT]
→ [viability RUN — typed T3 pause]
→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-ROUTE-AUTOPILOT-SOAK-1` — `READY_FOR_EXECUTION`

## Completed (PE7-ROUTE-AUTONOMY-STABILIZATION-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #416 exact head `9ce548f620314303b37753a18539c17b5daa6698`; merge `306b500c43270ca83d7cb9defd365140b525187c`; exact-head `PASS`; canonical workflow `31630036965`.
## Packet PE7-ROUTE-AUTOPILOT-SOAK-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-ROUTE-AUTONOMY-STABILIZATION-1 — COMPLETE on accepted main `c7335f0c04f8af80bc7b216af5a9a2cc5840ae9c` (PR #416 exact head `9ce548f620314303b37753a18539c17b5daa6698`; merge `306b500c43270ca83d7cb9defd365140b525187c`; exact-head `PASS`; canonical workflow `31630036965`).

**Class:** `CLOSEOUT`

**Outcome:** Exercise the accepted route controller through clean crossings and adversarial recovery cases, including one bounded local OpenCode weak-worker Provider call for an exact ledger-bound implementation attempt: worker failure, CI repair, review repair, crash/restart, duplicate dispatch/PR prevention, main drift, stale checkpoint, merge-before-closeout crash, promotion crash, DECISION_REQUIRED, NO_GO rewrite, EFFECT/T3 pause/resume, OUTCOME_UNKNOWN, and route exhaustion.

**Allowed delta:** AGENTS.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md, scripts/agent-control/codex_wrapper.sh, scripts/agent-control/local_run_once.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, tests/test_agent_local_loop.py, tests/test_agent_route_driver.py, tests/test_session_context.py. Do not modify the worker to make more than one local OpenCode run invocation per exact durable claim; it receives only the claim-bound prompt and isolated worktree, never a controller credential, GitHub capability, target authority, or effect authority.

**Exit:** A bounded, independently reviewed soak report proves the controller traverses multiple packet boundaries without manual successor authoring, preserves exactly one current window, completes at least one real OpenCode-backed code-and-document packet through existing PR/CI/review/merge/closeout owners, recovers ordinary failures through existing owners, pauses at exact authority gates, and validates no-growth traversal of the original 116-packet portfolio plus the canonical route-control entries.

**Stop:** Any second lifecycle owner; an unproved transition; a second OpenCode invocation for one claim; OpenCode with `--auto`, attach, session-resume, controller credential, or target/effect capability; duplicate/non-idempotent PR, dispatch, merge, or promotion; route prose treated as authority; an EFFECT executed or skipped; a missing T3 receipt; an outcome-unknown effect retried; or a recovery condition that cannot be proved from existing owners.

### Twelve-field contract

1. **Outcome and non-goals.** Exercise the accepted route controller through clean crossings and adversarial recovery cases, including one bounded local OpenCode weak-worker Provider call for an exact ledger-bound implementation attempt. OpenCode is not a product runtime and has no target, EFFECT, T3, GitHub, release, deployment, or auto-merge authority.
2. **Prerequisites and evidence.** Accepted main `c7335f0c04f8af80bc7b216af5a9a2cc5840ae9c`; checked route manifest SHA `ee2d16649b35581e7f4e9e498eb1b32d89dcb2cdfdda937d2b9b66dbe3cd11c7`; predecessor receipt PR #416 exact head `9ce548f620314303b37753a18539c17b5daa6698`; merge `306b500c43270ca83d7cb9defd365140b525187c`; exact-head `PASS`; canonical workflow `31630036965`; current-main evidence SHA `16191c2905cb03ef28ef614c9c2f6deab11dc4ed94b5fc8a6cc3c6679ed6938d`.
3. **Owners and paths.** Owners: scripts/agent-control/codex_wrapper.sh, scripts/agent-control/local_run_once.py, scripts/agent-control/route_driver.py; callers: scripts/agent-control/local_run_once.py, scripts/agent-control/route_driver.py; tests: tests/test_agent_local_loop.py, tests/test_agent_route_driver.py, tests/test_session_context.py.
4. **Frozen invariants.** Packet identity, route manifest SHA `ee2d16649b35581e7f4e9e498eb1b32d89dcb2cdfdda937d2b9b66dbe3cd11c7`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, target write, automatic merge, or second owner. The sole Provider exception is one local OpenCode run from the existing wrapper for an exact claim-bound implementation attempt; no `--auto`, attach, session resume, controller credential, raw transcript retention, or fallback worker is permitted.
7. **Ordered implementation slices.** scripts/agent-control/codex_wrapper.sh, scripts/agent-control/local_run_once.py, scripts/agent-control/route_driver.py, tests/test_agent_local_loop.py, tests/test_agent_route_driver.py, tests/test_session_context.py: Replace the single weak-worker transport with bounded OpenCode execution and prove success/failure, no second call, no raw transcript retention, exact dispatch-lane binding, and existing controller recovery/promotion crossings.; docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md: Compile one replace-only current window, remove one promoted future entry, and synchronize the accepted closeout receipt.
8. **Failure, recovery, and stop taxonomy.** Cleanup: Compact routing state to one current window rather than retain transition history. (proved by scripts/agent-control/route_driver.py:compact_next_window); retention: Retain detailed lifecycle evidence in the existing ledger and merged history. (proved by docs/NEXT_DECISION.md:retain); decisions: authority unchanged (docs/NEXT_DECISION.md:authority); evaluator unchanged (docs/NEXT_DECISION.md:evaluator); recovery unchanged (docs/NEXT_DECISION.md:recovery); schema unchanged (docs/NEXT_DECISION.md:schema).
9. **Verification.** git diff --check; python -m unittest discover -s tests -p test_agent_*.py; python scripts/check_agent_handoff.py
10. **Compatibility, rollback, and retention.** Stop orchestration before reverting a faulty bounded soak change. (proved by docs/NEXT_DECISION.md:Emergency-stop)
11. **Exit artifact.** Evidence destinations: Accepted packet receipt index. (docs/CURRENT_STATUS.md:Accepted).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["One claim-bound local OpenCode weak-worker Provider invocation limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["AGENTS.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/NEXT_DECISION.md", "scripts/agent-control/codex_wrapper.sh", "scripts/agent-control/local_run_once.py", "scripts/agent-control/route_driver.py", "scripts/check_agent_handoff.py", "tests/test_agent_local_loop.py", "tests/test_agent_route_driver.py", "tests/test_session_context.py"], "authority_consumption_allowed": false, "dispatch_lane": "opencode_local_repository_maintenance", "expected_artifacts": ["Accepted packet receipt index. (docs/CURRENT_STATUS.md:Accepted)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not call a Provider except one claim-bound local OpenCode invocation through the existing wrapper; do not mint T3 authority, execute an EFFECT, auto-merge, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not use OpenCode with --auto, attach, session resume, a controller credential, target authority, or effect authority.", "Do not mint T3 authority, execute an EFFECT, auto-merge, or write a target."], "goal": "Exercise the accepted route controller through clean crossings and adversarial recovery cases, including one bounded local OpenCode weak-worker Provider call for an exact ledger-bound implementation attempt: worker failure, CI repair, review repair, crash/restart, duplicate dispatch/PR prevention, main drift, stale checkpoint, merge-before-closeout crash, promotion crash, DECISION_REQUIRED, NO_GO rewrite, EFFECT/T3 pause/resume, OUTCOME_UNKNOWN, and route exhaustion.", "ordered_steps": ["scripts/agent-control/codex_wrapper.sh, scripts/agent-control/local_run_once.py, scripts/agent-control/route_driver.py, tests/test_agent_local_loop.py, tests/test_agent_route_driver.py, tests/test_session_context.py: Replace the single weak-worker transport with bounded OpenCode execution and prove success/failure, no second call, no raw transcript retention, exact dispatch-lane binding, and existing controller recovery/promotion crossings.", "docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md: Compile one replace-only current window, remove one promoted future entry, and synchronize the accepted closeout receipt."], "packet_id": "PE7-ROUTE-AUTOPILOT-SOAK-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a target write, automatic merge, authority consumption, or external effect.", "Stop before a second OpenCode invocation for one claim or any OpenCode invocation outside the existing wrapper and isolated worktree.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #416 exact head `9ce548f620314303b37753a18539c17b5daa6698`; merge `306b500c43270ca83d7cb9defd365140b525187c`; exact-head `PASS`; canonical workflow `31630036965`"], "prerequisites": ["PE7-ROUTE-AUTONOMY-STABILIZATION-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "16191c2905cb03ef28ef614c9c2f6deab11dc4ed94b5fc8a6cc3c6679ed6938d", "read_paths": ["AGENTS.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/NEXT_DECISION.md", "scripts/agent-control/codex_wrapper.sh", "scripts/agent-control/local_run_once.py", "scripts/agent-control/route_driver.py", "scripts/check_agent_handoff.py", "tests/test_agent_local_loop.py", "tests/test_agent_route_driver.py", "tests/test_session_context.py"], "risk_class": "none", "rollback": "Stop orchestration before reverting a faulty bounded soak change. (proved by docs/NEXT_DECISION.md:Emergency-stop)", "route_manifest_sha256": "ee2d16649b35581e7f4e9e498eb1b32d89dcb2cdfdda937d2b9b66dbe3cd11c7", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["git diff --check", "python -m unittest discover -s tests -p test_agent_*.py", "python scripts/check_agent_handoff.py"], "verification_family": "evidence_review", "worker_tier": "T2"}
-->

## Common Execution Protocol

- Refresh accepted main, the current packet, exact PR heads, CI, review, and ledger receipts before every transition.
- Derive a route action only from the accepted current window, the checked inventory, current-main evidence, and existing durable owners.
- Keep changing PRs Draft; require stable-head independent review and canonical exact-head CI before governed manual merge.
- Treat ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures as bounded recovery transitions through their existing owners.
- Preserve exact receipt bindings and failed/unknown evidence; never convert absence, stale evidence, or an unproven external outcome to success.

## Hard Stops

- no Provider call except the dispatch-capsule-authorized, one-per-claim local OpenCode weak-worker invocation; no credential read, target write, release, deployment, automatic merge, EFFECT execution, or T3 action without its separate exact authority;
- no second controller, ledger, queue, lease, workflow owner, store, scheduler, evaluator, authority, or persistence owner;
- no future-route path/prose, model output, local checkpoint, or candidate PR accepted as current-main authority;
- no stale/ambiguous owner, caller, path, verification, rollback, cleanup, retention, evidence, schema, evaluator, authority, or recovery fact treated as proved;
- no retry of an outcome-unknown effect and no deletion or concealment of failure, pause, repair, or recovery evidence.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is the sole routing-only index. Promotion removes exactly one eligible packet, re-derives every `REFRESH_AT_PROMOTION` field from accepted main, validates the resulting candidate, and independently reviews the routing change. No future sketch, static path, or profile alone authorizes code or an effect.
