# Next Decision

Last updated: 2026-08-13.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. No freeze-duration TTL was invented. B1 `observed_at` and fail-closed `created_at` provenance remain the PR #434 repair. The current window is the provider-free viability preflight.

## Authoritative Forward Order

```text
[window: PE7-RWE-V2-VIABILITY-PREFLIGHT-1 — READY_FOR_EXECUTION, provider-free]

→ [viability RUN — typed T3 pause]
→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-V2-VIABILITY-PREFLIGHT-1` — `READY_FOR_EXECUTION`

## Completed (PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #434 exact head `9fdd1045928f862a5b1c1017bc0e9d73e5d50966`; merge `e311db76bf4d2a3a407213b8129a600bc447fd56`; exact-head `PASS`; canonical workflow `31690000442`; durable B2 rule caller-supplied finite expires_at.
## Packet PE7-RWE-V2-VIABILITY-PREFLIGHT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1 — COMPLETE on accepted main `bfc1dc4b1da53548f89b6c3b507d767d47fdc074` (PR #434 exact head `9fdd1045928f862a5b1c1017bc0e9d73e5d50966`; merge `e311db76bf4d2a3a407213b8129a600bc447fd56`; exact-head `PASS`; canonical workflow `31690000442`; durable B2 rule caller-supplied finite expires_at).

**Class:** `CLOSEOUT`

**Outcome:** Run the accepted provider-free v2 preflight against the repaired accepted main, verify exact freeze and Golden Path bindings, and construct a redacted, hash-bound one-use authorization request package without issuing, admitting, consuming, or executing authority.

**Allowed delta:** docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md, engine/src/rwe/live_baseline_coordinator.rs, engine/src/storage/local_product_store/rwe_authority.rs, scripts/agent-control/local_run_once.py, scripts/agent-control/route_driver.py, tests/test_agent_route_driver.py.

**Exit:** One fresh, authoritative-timestamp preflight receipt with ready=true, zero blockers, and all negative-effect flags false, plus one bounded T3 authorization request package. The route then promotes the viability RUN as `T3_REQUIRED`; it must not execute it.

**Stop:** Any stale/missing binding, failed preflight, live lease, non-disposable target state, unresolved Provider/model drift, missing evidence destination, invalid B1/B2/provenance, invented B2 freeze duration, or request for an authority/effect outside the exact future RUN packet.

### Twelve-field contract

1. **Outcome and non-goals.** Run the accepted provider-free v2 preflight against the repaired accepted main, verify exact freeze and Golden Path bindings, and construct a redacted, hash-bound one-use authorization request package without issuing, admitting, consuming, or executing authority.
2. **Prerequisites and evidence.** Accepted main `bfc1dc4b1da53548f89b6c3b507d767d47fdc074`; checked route manifest SHA `8afa873f6ab19be5145bfb89a1e118e217a7306a0cfe56d2ea63662d83e9695c`; predecessor receipt PR #434 exact head `9fdd1045928f862a5b1c1017bc0e9d73e5d50966`; merge `e311db76bf4d2a3a407213b8129a600bc447fd56`; exact-head `PASS`; canonical workflow `31690000442`; durable B2 rule caller-supplied finite expires_at; current-main evidence SHA `c3dc0b8c2be0cb8a2e0082fc6ef63fbea9ffc83cd6052bb5beba7f2155b137b0`.
3. **Owners and paths.** Owners: engine/src/rwe/live_baseline_coordinator.rs, engine/src/storage/local_product_store/rwe_authority.rs, scripts/agent-control/route_driver.py; callers: scripts/agent-control/local_run_once.py; tests: tests/test_agent_route_driver.py.
4. **Frozen invariants.** Packet identity, route manifest SHA `8afa873f6ab19be5145bfb89a1e118e217a7306a0cfe56d2ea63662d83e9695c`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** engine/src/rwe/live_baseline_coordinator.rs, engine/src/storage/local_product_store/rwe_authority.rs: Run store-owned operator_preflight and bind the redacted v2 request envelope without issuing authority.; docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md: Record the authoritative-timestamp preflight receipt and bounded T3 request package.
8. **Failure, recovery, and stop taxonomy.** Cleanup: Compact routing state to one current window rather than retain transition history. (proved by scripts/agent-control/route_driver.py:compact_next_window); retention: Retain detailed lifecycle evidence in the existing ledger and merged history. (proved by docs/NEXT_DECISION.md:retain); decisions: authority unchanged (docs/NEXT_DECISION.md:authority); evaluator unchanged (docs/NEXT_DECISION.md:evaluator); recovery unchanged (docs/NEXT_DECISION.md:recovery); schema unchanged (docs/NEXT_DECISION.md:schema).
9. **Verification.** git diff --check; python scripts/check_agent_handoff.py
10. **Compatibility, rollback, and retention.** Revert the current window and retain detailed lifecycle evidence. (proved by docs/NEXT_DECISION.md:Emergency-stop)
11. **Exit artifact.** Evidence destinations: Accepted packet receipt index. (docs/CURRENT_STATUS.md:Accepted).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/rwe/live_baseline_coordinator.rs", "engine/src/storage/local_product_store/rwe_authority.rs", "scripts/agent-control/local_run_once.py", "scripts/agent-control/route_driver.py", "tests/test_agent_route_driver.py"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Accepted packet receipt index. (docs/CURRENT_STATUS.md:Accepted)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Run the accepted provider-free v2 preflight against the repaired accepted main, verify exact freeze and Golden Path bindings, and construct a redacted, hash-bound one-use authorization request package without issuing, admitting, consuming, or executing authority.", "ordered_steps": ["engine/src/rwe/live_baseline_coordinator.rs, engine/src/storage/local_product_store/rwe_authority.rs: Run store-owned operator_preflight and bind the redacted v2 request envelope without issuing authority.", "docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md: Record the authoritative-timestamp preflight receipt and bounded T3 request package."], "packet_id": "PE7-RWE-V2-VIABILITY-PREFLIGHT-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #434 exact head `9fdd1045928f862a5b1c1017bc0e9d73e5d50966`; merge `e311db76bf4d2a3a407213b8129a600bc447fd56`; exact-head `PASS`; canonical workflow `31690000442`; durable B2 rule caller-supplied finite expires_at"], "prerequisites": ["PE7-RWE-V2-PREFLIGHT-GATE-REPAIR-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "c3dc0b8c2be0cb8a2e0082fc6ef63fbea9ffc83cd6052bb5beba7f2155b137b0", "read_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/rwe/live_baseline_coordinator.rs", "engine/src/storage/local_product_store/rwe_authority.rs", "scripts/agent-control/local_run_once.py", "scripts/agent-control/route_driver.py", "tests/test_agent_route_driver.py"], "risk_class": "none", "rollback": "Revert the current window and retain detailed lifecycle evidence. (proved by docs/NEXT_DECISION.md:Emergency-stop)", "route_manifest_sha256": "8afa873f6ab19be5145bfb89a1e118e217a7306a0cfe56d2ea63662d83e9695c", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["git diff --check", "python scripts/check_agent_handoff.py"], "verification_family": "evidence_review", "worker_tier": "T2"}
-->

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
