# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The current provider-free packet is the AC3 Golden Path responsibility contract; it does not change state semantics, public compatibility, or authority ownership.

## Authoritative Forward Order

```text
[window: PE7-CWS-BENCHMARK-RUN-1 — T3_REQUIRED, external_effect]

```

## Active Routing

1. `PE7-CWS-BENCHMARK-RUN-1` — `T3_REQUIRED`

## Completed (PE7-CWS-BENCHMARK-PREFLIGHT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #521 exact head `8ceee8b411e7757cacea14c34f3a0792d60c1bb2`; merge `189ec4b4d7d82f254b05213aa500e00541edd9fd`; exact-head `PASS`; canonical workflow `31942335704`.
## Packet PE7-CWS-BENCHMARK-RUN-1

**State:** `T3_REQUIRED`

**Prerequisite:** PE7-CWS-BENCHMARK-PREFLIGHT-1 — COMPLETE on accepted main `189ec4b4d7d82f254b05213aa500e00541edd9fd` (PR #521 exact head `8ceee8b411e7757cacea14c34f3a0792d60c1bb2`; merge `189ec4b4d7d82f254b05213aa500e00541edd9fd`; exact-head `PASS`; canonical workflow `31942335704`).

**Class:** `EFFECT`

**Outcome:** Execute the frozen baseline-versus-CWS comparison once under finite authorization, preferably randomized/interleaved when the registered protocol requires it.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md, scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/project_context.py, scripts/session_context.py, tests/test_agent_route_driver.py, tests/test_session_context.py.

**Exit:** Every scheduled arm/task reaches terminal evidence with exact request/model/context identities, provider usage when reported, tool/rehydration/retry events, quality, latency, cost, failures/unknowns, cleanup, and raw/redacted bindings.

**Stop:** Authority expires, outcome becomes unknown, arm comparability breaks, cache/provider semantics drift, evidence capture fails, a hard global stop fires, or any selective treatment repair would be required.

### Twelve-field contract

1. **Outcome and non-goals.** Execute the frozen baseline-versus-CWS comparison once under finite authorization, preferably randomized/interleaved when the registered protocol requires it.
2. **Prerequisites and evidence.** Accepted main `189ec4b4d7d82f254b05213aa500e00541edd9fd`; checked route manifest SHA `5d6dc20896f8db508f8ab011b7f7ccac2c73319ff56758030b20dead6ecba3cd`; predecessor receipt PR #521 exact head `8ceee8b411e7757cacea14c34f3a0792d60c1bb2`; merge `189ec4b4d7d82f254b05213aa500e00541edd9fd`; exact-head `PASS`; canonical workflow `31942335704`; current-main evidence SHA `abcf55335a1e91902bd24734e200af274a7d4db2dbd3cfed37701d62d59dc483`.
3. **Owners and paths.** Owners: scripts/agent-control/route_driver.py; callers: scripts/agent-control/local_run_once.py; tests: tests/test_agent_route_driver.py.
4. **Frozen invariants.** Packet identity, route manifest SHA `5d6dc20896f8db508f8ab011b7f7ccac2c73319ff56758030b20dead6ecba3cd`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Bind the accepted context working-set benchmark run execution window.; scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/project_context.py, scripts/session_context.py, tests/test_agent_route_driver.py, tests/test_session_context.py: Enforce bounded route-control edit/read/verification scope and prove benchmark run tests.
8. **Failure, recovery, and stop taxonomy.** Cleanup: No runtime mutation or cleanup required for benchmark run. (proved by docs/ARCHITECTURE_BOOK.md:recovery); retention: Retain the accepted CWS benchmark preflight receipt. (proved by docs/CURRENT_STATUS.md:receipt); decisions: authority unchanged (docs/MODULE_MAP.md:authority); evaluator unchanged (docs/CURRENT_STATUS.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:recovery); schema unchanged (docs/CURRENT_STATUS.md:schema).
9. **Verification.** uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revert the docs and route-control repairs together and restore the prior CWS window. (proved by docs/ARCHITECTURE_BOOK.md:recovery)
11. **Exit artifact.** Evidence destinations: Canonical route evidence. (docs/NEXT_DECISION.md:canonical).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "scripts/agent-control/local_verification.py", "scripts/agent-control/route_driver.py", "scripts/check_agent_handoff.py", "scripts/project_context.py", "scripts/session_context.py", "tests/test_agent_route_driver.py", "tests/test_session_context.py"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Canonical route evidence. (docs/NEXT_DECISION.md:canonical)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Execute the frozen baseline-versus-CWS comparison once under finite authorization, preferably randomized/interleaved when the registered protocol requires it.", "ordered_steps": ["docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Bind the accepted context working-set benchmark run execution window.", "scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/project_context.py, scripts/session_context.py, tests/test_agent_route_driver.py, tests/test_session_context.py: Enforce bounded route-control edit/read/verification scope and prove benchmark run tests."], "packet_id": "PE7-CWS-BENCHMARK-RUN-1", "packet_state": "T3_REQUIRED", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #521 exact head `8ceee8b411e7757cacea14c34f3a0792d60c1bb2`; merge `189ec4b4d7d82f254b05213aa500e00541edd9fd`; exact-head `PASS`; canonical workflow `31942335704`"], "prerequisites": ["PE7-CWS-BENCHMARK-PREFLIGHT-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "abcf55335a1e91902bd24734e200af274a7d4db2dbd3cfed37701d62d59dc483", "read_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "scripts/agent-control/local_run_once.py", "scripts/agent-control/local_verification.py", "scripts/agent-control/route_driver.py", "scripts/check_agent_handoff.py", "scripts/project_context.py", "scripts/session_context.py", "tests/test_agent_route_driver.py", "tests/test_session_context.py"], "risk_class": "external_effect", "rollback": "Revert the docs and route-control repairs together and restore the prior CWS window. (proved by docs/ARCHITECTURE_BOOK.md:recovery)", "route_manifest_sha256": "5d6dc20896f8db508f8ab011b7f7ccac2c73319ff56758030b20dead6ecba3cd", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "t3_request_digest": "d6b202235c41752225e58ca14b3bf235ae12e50f7fbd1eb1f14233073c0e35f1", "verification": ["uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "external_effect_evidence", "worker_tier": "T3"}
-->
<!-- route-t3-request:v1
{"accepted_main_sha": "189ec4b4d7d82f254b05213aa500e00541edd9fd", "action_digest": "9c4184f99df2fb24de99bed6ef251a708579928b6af6839d9d942a1bd379d44e", "authority_owner_digest": "38716a2b73acf1399eecd15cbe11ea3207af23b2416c692fbcd2b0cc4290f395", "candidate_digest": "d6b202235c41752225e58ca14b3bf235ae12e50f7fbd1eb1f14233073c0e35f1", "packet_id": "PE7-CWS-BENCHMARK-RUN-1", "requested_action": "Execute the frozen baseline-versus-CWS comparison once under finite authorization, preferably randomized/interleaved when the registered protocol requires it.", "schema_version": "route_t3_request.v1", "scope_digest": "2746fce50d86261893378cdf6eb66dbb80338f1df99d72a7d1a8e7490cd09168"}
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
