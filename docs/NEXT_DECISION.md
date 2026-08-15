# Next Decision

Last updated: 2026-08-15.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The primary-route scope decision is owned by `docs/CURRENT_STATUS.md`. The minimal AC0 data/trace freeze is accepted; the active window is the provider-free AC2 typed-execution contract, now being closed out after its state/outcome/usage mapping was recorded. Deferred runtime-inventory and shared-`ProcessSupervisor` hardening is not part of the active packet.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1`, `PE7-RWE-MR-CORPUS-SAMPLING-1`, `PE7-RWE-MR-OPERATIONS-EVIDENCE-1`, and `PE7-RWE-MR-PROTOCOL-FREEZE-1` are accepted on main with their source-bound measurement, corpus, operations, and protocol contracts. The reconstructable snapshot replacement and provider-free DB preflight are accepted. The later DB RUN is retained as a non-baseline controlled failure and removed from the forward AC prerequisite chain; this planning decision does not claim an EFFECT receipt, T3 closeout, or decision-grade baseline.

## Authoritative Forward Order

```text
[window: PE7-AC2-CONTRACT-1 — IN_PROGRESS, provider-free]

```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-AC2-CONTRACT-1` — `IN_PROGRESS`

## Packet PE7-AC2-CONTRACT-1

**State:** `IN_PROGRESS`

**Prerequisite:** PE7-AC0-TRACE-ORDER-FREEZE-1 — COMPLETE on accepted main `f98882c9cdab2b9e9e11a44d2ce3045b52c8d65e` (PR #467 exact head `19cc238fec27236873262c12998eabe2eda26ac4`; merge `a4879fc60f1c080579df7ba942793a4c94367ff5`; exact-head `PASS`; canonical workflow `31869014363`).

**Class:** `CONTRACT`

**Outcome:** Freeze the typed execution state/outcome/usage contract and executor-specific mapping table.

**Allowed delta:** Documentation-only edits to `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, and `docs/NEXT_DECISION.md`; the route-controller and test paths listed below are read-only proof inputs, not edit targets.

**Exit:** Exact variants for admission, prepared, effect-not-started, effect-started, known/unknown outcome, cancellation, terminal failure, and evidence completeness.

**Stop:** A state cannot be derived from trustworthy owner evidence or would imply unsafe retry.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze the typed execution state/outcome/usage contract and executor-specific mapping table.
2. **Prerequisites and evidence.** Accepted main `f98882c9cdab2b9e9e11a44d2ce3045b52c8d65e`; checked route manifest SHA `fb92c84128b6a27cc51dd24608d5521483cc003f6de66a5ad687c6c3ae231fe1`; predecessor receipt PR #467 exact head `19cc238fec27236873262c12998eabe2eda26ac4`; merge `a4879fc60f1c080579df7ba942793a4c94367ff5`; exact-head `PASS`; canonical workflow `31869014363`; current-main evidence SHA `bb23eac8e3825a2309e8da8bfd8774cf4c30ac25c6dde66fa669ef0eb89e0e44`.
3. **Owners and paths.** Owners: scripts/agent-control/route_driver.py; callers: scripts/agent-control/local_run_once.py; tests: tests/test_agent_route_driver.py.
4. **Frozen invariants.** Packet identity, route manifest SHA `fb92c84128b6a27cc51dd24608d5521483cc003f6de66a5ad687c6c3ae231fe1`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/CURRENT_STATUS.md: record the AC2 typed state/outcome/usage contract and existing owner boundaries; docs/NEXT_DECISION.md: promote the AC2 contract with its exact provider-free contract scope; docs/FUTURE_ROUTE.md: verify the promoted AC2 contract sketch removal and checked manifest accepted by PR #468 (`f98882c9cdab2b9e9e11a44d2ce3045b52c8d65e`, manifest `fb92c84128b6a27cc51dd24608d5521483cc003f6de66a5ad687c6c3ae231fe1`), without duplicating or reopening that route change
8. **Failure, recovery, and stop taxonomy.** Cleanup: No runtime or external cleanup is required; retain existing executor, store, audit, and recovery owners.; retention: Keep the merged PR, exact-head review, canonical CI, trace/order matrix, and redacted route evidence in the existing canonical documents.; decisions: schema unchanged (docs/CURRENT_STATUS.md: No wire/schema migration in AC0); evaluator unchanged (docs/CURRENT_STATUS.md: not a claim of a successful live DeepSeek run); authority unchanged (docs/CURRENT_STATUS.md: This packet makes no Provider call and consumes no authority); recovery unchanged (docs/CURRENT_STATUS.md: outcome_unknown never enters a speculative retry).
9. **Verification.** uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revert the three-document AC0 closeout and AC2 promotion while retaining the accepted trace/order evidence and prior failure evidence.
11. **Exit artifact.** Evidence destinations: docs/CURRENT_STATUS.md: AC2 typed execution contract and existing owner boundaries.
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/NEXT_DECISION.md"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["docs/CURRENT_STATUS.md: AC2 typed execution contract and existing owner boundaries"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Freeze the typed execution state/outcome/usage contract and executor-specific mapping table.", "ordered_steps": ["docs/CURRENT_STATUS.md: record the AC2 typed state/outcome/usage contract and existing owner boundaries", "docs/NEXT_DECISION.md: promote the AC2 contract with its exact provider-free contract scope", "docs/FUTURE_ROUTE.md: verify the promoted AC2 contract sketch removal and checked manifest accepted by PR #468 (`f98882c9cdab2b9e9e11a44d2ce3045b52c8d65e`, manifest `fb92c84128b6a27cc51dd24608d5521483cc003f6de66a5ad687c6c3ae231fe1`), without duplicating or reopening that route change"], "packet_id": "PE7-AC2-CONTRACT-1", "packet_state": "IN_PROGRESS", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #467 exact head `19cc238fec27236873262c12998eabe2eda26ac4`; merge `a4879fc60f1c080579df7ba942793a4c94367ff5`; exact-head `PASS`; canonical workflow `31869014363`"], "prerequisites": ["PE7-AC0-TRACE-ORDER-FREEZE-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "bb23eac8e3825a2309e8da8bfd8774cf4c30ac25c6dde66fa669ef0eb89e0e44", "read_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "scripts/agent-control/local_run_once.py", "scripts/agent-control/route_driver.py", "tests/test_agent_route_driver.py"], "risk_class": "none", "rollback": "Revert the three-document AC0 closeout and AC2 promotion while retaining the accepted trace/order evidence and prior failure evidence.", "route_manifest_sha256": "fb92c84128b6a27cc51dd24608d5521483cc003f6de66a5ad687c6c3ae231fe1", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "docs_evidence_review", "worker_tier": "T2"}
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
