# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The current provider-free packet is the AC3 Golden Path responsibility contract; it does not change state semantics, public compatibility, or authority ownership.

## Authoritative Forward Order

```text
[window: PE7-CWS-ANALYSIS-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-CWS-ANALYSIS-1` — `READY_FOR_EXECUTION`

## Completed (PE7-CWS-BENCHMARK-RUN-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #522 exact head `ba88f6718f0288863baebcc768ff7c89ed3dfe18`; merge `731936493679aec50c96fa2d595a947f57d584ea`; exact-head `PASS`; canonical workflow `31943063270`.
## Packet PE7-CWS-ANALYSIS-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-CWS-BENCHMARK-RUN-1 — COMPLETE on accepted main `731936493679aec50c96fa2d595a947f57d584ea` (PR #522 exact head `ba88f6718f0288863baebcc768ff7c89ed3dfe18`; merge `731936493679aec50c96fa2d595a947f57d584ea`; exact-head `PASS`; canonical workflow `31943063270`).

**Class:** `CLOSEOUT`

**Outcome:** Apply the frozen analysis and choose `ENABLE`, `DISABLE`, or `INSUFFICIENT_DEFAULT_OFF`, then bind the exact active Harness identity that all later Harness-Evolution packets must use; include implementation-maintenance burden as post-gate decision evidence only.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md, scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/session_context.py, tests/test_agent_route_driver.py, tests/test_session_context.py.

**Exit:** Independent hard-gate-first receipt preserves all failures/missingness, reports repeated/total context and lifecycle effects with uncertainty, proves exact rehydration and rollback, names one exact CWS-enabled or baseline/default-off Harness identity for EC1, and records implementation-maintenance burden after hard gates pass: upstream-derived LOC, adapter LOC, new project-specific LOC, upstream tests retained or adapted, dependency delta, exact upstream source identities, divergence/patch burden, and upgrade/maintenance cost.

**Stop:** Quality/safety/authority/evidence hard gates fail without a proven disable rollback, analysis cannot be reproduced, active-Harness identity is ambiguous, the conclusion depends on unregistered exclusions, or maintenance-burden numbers are used as evaluator or acceptance authority.

### Twelve-field contract

1. **Outcome and non-goals.** Apply the frozen analysis and choose `ENABLE`, `DISABLE`, or `INSUFFICIENT_DEFAULT_OFF`, then bind the exact active Harness identity that all later Harness-Evolution packets must use; include implementation-maintenance burden as post-gate decision evidence only.
2. **Prerequisites and evidence.** Accepted main `731936493679aec50c96fa2d595a947f57d584ea`; checked route manifest SHA `e50016967f84b84070a96ccd8c3fec07650cab177b595e7fe25b4aea6d628b68`; predecessor receipt PR #522 exact head `ba88f6718f0288863baebcc768ff7c89ed3dfe18`; merge `731936493679aec50c96fa2d595a947f57d584ea`; exact-head `PASS`; canonical workflow `31943063270`; current-main evidence SHA `a17feee2f95fc1466f4272a020bcb9522e2138fbf3fe7f31a5713a197f83edee`.
3. **Owners and paths.** Owners: scripts/agent-control/route_driver.py; callers: scripts/agent-control/local_run_once.py; tests: tests/test_agent_route_driver.py.
4. **Frozen invariants.** Packet identity, route manifest SHA `e50016967f84b84070a96ccd8c3fec07650cab177b595e7fe25b4aea6d628b68`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Bind the accepted context working-set analysis closeout execution window.; scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/session_context.py, tests/test_agent_route_driver.py, tests/test_session_context.py: Enforce bounded route-control edit/read/verification scope and prove analysis closeout tests.
8. **Failure, recovery, and stop taxonomy.** Cleanup: No runtime mutation or cleanup required for CWS analysis. (proved by docs/ARCHITECTURE_BOOK.md:recovery); retention: Retain the accepted CWS benchmark run receipt. (proved by docs/CURRENT_STATUS.md:receipt); decisions: authority unchanged (docs/MODULE_MAP.md:authority); evaluator unchanged (docs/CURRENT_STATUS.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:recovery); schema unchanged (docs/CURRENT_STATUS.md:schema).
9. **Verification.** uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revert the docs and route-control repairs together and restore the prior CWS window. (proved by docs/ARCHITECTURE_BOOK.md:recovery)
11. **Exit artifact.** Evidence destinations: Canonical route evidence. (docs/NEXT_DECISION.md:canonical).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "scripts/agent-control/local_verification.py", "scripts/agent-control/route_driver.py", "scripts/check_agent_handoff.py", "scripts/session_context.py", "tests/test_agent_route_driver.py", "tests/test_session_context.py"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Canonical route evidence. (docs/NEXT_DECISION.md:canonical)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Apply the frozen analysis and choose `ENABLE`, `DISABLE`, or `INSUFFICIENT_DEFAULT_OFF`, then bind the exact active Harness identity that all later Harness-Evolution packets must use; include implementation-maintenance burden as post-gate decision evidence only.", "ordered_steps": ["docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Bind the accepted context working-set analysis closeout execution window.", "scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/session_context.py, tests/test_agent_route_driver.py, tests/test_session_context.py: Enforce bounded route-control edit/read/verification scope and prove analysis closeout tests."], "packet_id": "PE7-CWS-ANALYSIS-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #522 exact head `ba88f6718f0288863baebcc768ff7c89ed3dfe18`; merge `731936493679aec50c96fa2d595a947f57d584ea`; exact-head `PASS`; canonical workflow `31943063270`"], "prerequisites": ["PE7-CWS-BENCHMARK-RUN-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "a17feee2f95fc1466f4272a020bcb9522e2138fbf3fe7f31a5713a197f83edee", "read_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "scripts/agent-control/local_run_once.py", "scripts/agent-control/local_verification.py", "scripts/agent-control/route_driver.py", "scripts/check_agent_handoff.py", "scripts/session_context.py", "tests/test_agent_route_driver.py", "tests/test_session_context.py"], "risk_class": "none", "rollback": "Revert the docs and route-control repairs together and restore the prior CWS window. (proved by docs/ARCHITECTURE_BOOK.md:recovery)", "route_manifest_sha256": "e50016967f84b84070a96ccd8c3fec07650cab177b595e7fe25b4aea6d628b68", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "evidence_review", "worker_tier": "T2"}
-->

## Common Execution Protocol

- `READY_FOR_EXECUTION` packets require a valid dispatch capsule; blocked or decision-required packets carry no executable capsule.
- Refresh accepted `main`, the current packet, exact PR head, CI, and review receipts before each transition.
- Keep a changing PR Draft; run focused/local full checks, then one final stable-head Standards/Spec review, Ready, canonical exact-head CI, manual squash merge, and main refresh.
- No Provider call, credential read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner in this packet.
- Unknown, stale, missing, or conflicting evidence remains fail-closed; never retry an effect whose outcome may be unknown.

## Hard Stops

- `DECISION_REQUIRED` when a caller, owner, path, semantics, compatibility fact, rollback, or evidence cannot be re-proved from accepted `main`.
- No Provider call, credential read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner.
- Unknown or possibly executed effects are never treated as success or retried.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is routing-only. The promoted packet was removed from that index and its manifest was refreshed; no future sketch authorizes code or an effect until promoted into this document from accepted `main`.
