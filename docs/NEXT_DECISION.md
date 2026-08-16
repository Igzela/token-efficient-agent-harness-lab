# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The current provider-free packet is the AC3 Golden Path responsibility contract; it does not change state semantics, public compatibility, or authority ownership.

## Authoritative Forward Order

```text
[window: PE7-CWS-PROJECTOR-CORE-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-CWS-PROJECTOR-CORE-1` — `READY_FOR_EXECUTION`

## Completed (PE7-CWS-REHYDRATION-CONTRACT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #514 exact head `2a67606fb56e56ab71dc818a2830f6f485ffadc7`; merge `284b7f3eaecc10c9ece7e92b23ce6f83dde446e5`; exact-head `PASS`; canonical workflow `31935808699`.
## Packet PE7-CWS-PROJECTOR-CORE-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-CWS-REHYDRATION-CONTRACT-1 — COMPLETE on accepted main `284b7f3eaecc10c9ece7e92b23ce6f83dde446e5` (PR #514 exact head `2a67606fb56e56ab71dc818a2830f6f485ffadc7`; merge `284b7f3eaecc10c9ece7e92b23ce6f83dde446e5`; exact-head `PASS`; canonical workflow `31935808699`).

**Class:** `IMPLEMENT`

**Outcome:** Implement the pure deterministic working-set projector and residency transitions behind existing context owners, producing a stable prefix plus bounded dynamic working set from already-authorized source material, preferring transplant, port, or adapt of a qualified upstream mechanism core.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md, scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/project_context.py, scripts/session_context.py, tests/test_agent_route_driver.py, tests/test_session_context.py.

**Exit:** Ordering, pinning, supersession, duplicate, token/byte bound, deterministic replay, delete/rebuild, stale-source, and forbidden-eviction tests pass with identical canonical source identities; retained or adapted upstream tests plus adapter tests prove contract fidelity without a second owner; the exact implementation-selection disposition is bound to the frozen contracts rather than to INGRESS candidate status.

**Stop:** Projector requires ambient credentials/network, persisted truth, nondeterministic scoring for a hard decision, a source owner must move into the projection layer, a transplant would vendor or fork another harness or create a parallel owner, license/provenance/attribution cannot be satisfied through the then-current accepted owner, upstream semantics cannot map losslessly onto the accepted contract, or an INGRESS `candidate_status` is treated as the implementation disposition.

### Twelve-field contract

1. **Outcome and non-goals.** Implement the pure deterministic working-set projector and residency transitions behind existing context owners, producing a stable prefix plus bounded dynamic working set from already-authorized source material, preferring transplant, port, or adapt of a qualified upstream mechanism core.
2. **Prerequisites and evidence.** Accepted main `284b7f3eaecc10c9ece7e92b23ce6f83dde446e5`; checked route manifest SHA `4e0e0295b7d93c6e37ace49ff201c2c66eabafe5a19f66e2974a555d0e1e6748`; predecessor receipt PR #514 exact head `2a67606fb56e56ab71dc818a2830f6f485ffadc7`; merge `284b7f3eaecc10c9ece7e92b23ce6f83dde446e5`; exact-head `PASS`; canonical workflow `31935808699`; current-main evidence SHA `91fdee6aea8d447e4f1cd403d99e246b36bf4041af4b58bef788fcad1d6aeeaf`.
3. **Owners and paths.** Owners: scripts/agent-control/route_driver.py; callers: scripts/agent-control/local_run_once.py, tests/test_agent_route_driver.py; tests: tests/test_agent_route_driver.py.
4. **Frozen invariants.** Packet identity, route manifest SHA `4e0e0295b7d93c6e37ace49ff201c2c66eabafe5a19f66e2974a555d0e1e6748`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Bind the accepted context working-set projector core implementation execution window.; scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/project_context.py, scripts/session_context.py, tests/test_agent_route_driver.py, tests/test_session_context.py: Implement the pure deterministic working-set projector and prove contract tests.
8. **Failure, recovery, and stop taxonomy.** Cleanup: No runtime mutation or cleanup required for projector core. (proved by docs/ARCHITECTURE_BOOK.md:recovery); retention: Retain the accepted CWS rehydration contract receipt. (proved by docs/CURRENT_STATUS.md:receipt); decisions: authority unchanged (docs/MODULE_MAP.md:authority); evaluator unchanged (docs/CURRENT_STATUS.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:recovery); schema unchanged (docs/CURRENT_STATUS.md:schema).
9. **Verification.** uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revert the docs and route-control repairs together and restore the prior CWS window. (proved by docs/ARCHITECTURE_BOOK.md:recovery)
11. **Exit artifact.** Evidence destinations: Canonical route evidence. (docs/NEXT_DECISION.md:canonical).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "scripts/agent-control/local_verification.py", "scripts/agent-control/route_driver.py", "scripts/check_agent_handoff.py", "scripts/project_context.py", "scripts/session_context.py", "tests/test_agent_route_driver.py", "tests/test_session_context.py"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Canonical route evidence. (docs/NEXT_DECISION.md:canonical)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Implement the pure deterministic working-set projector and residency transitions behind existing context owners, producing a stable prefix plus bounded dynamic working set from already-authorized source material, preferring transplant, port, or adapt of a qualified upstream mechanism core.", "ordered_steps": ["docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Bind the accepted context working-set projector core implementation execution window.", "scripts/agent-control/local_verification.py, scripts/agent-control/route_driver.py, scripts/check_agent_handoff.py, scripts/project_context.py, scripts/session_context.py, tests/test_agent_route_driver.py, tests/test_session_context.py: Implement the pure deterministic working-set projector and prove contract tests."], "packet_id": "PE7-CWS-PROJECTOR-CORE-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #514 exact head `2a67606fb56e56ab71dc818a2830f6f485ffadc7`; merge `284b7f3eaecc10c9ece7e92b23ce6f83dde446e5`; exact-head `PASS`; canonical workflow `31935808699`"], "prerequisites": ["PE7-CWS-REHYDRATION-CONTRACT-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "91fdee6aea8d447e4f1cd403d99e246b36bf4041af4b58bef788fcad1d6aeeaf", "read_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "scripts/agent-control/local_run_once.py", "scripts/agent-control/local_verification.py", "scripts/agent-control/route_driver.py", "scripts/check_agent_handoff.py", "scripts/project_context.py", "scripts/session_context.py", "tests/test_agent_route_driver.py", "tests/test_session_context.py"], "risk_class": "none", "rollback": "Revert the docs and route-control repairs together and restore the prior CWS window. (proved by docs/ARCHITECTURE_BOOK.md:recovery)", "route_manifest_sha256": "4e0e0295b7d93c6e37ace49ff201c2c66eabafe5a19f66e2974a555d0e1e6748", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "source_focused_full", "worker_tier": "T1"}
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
