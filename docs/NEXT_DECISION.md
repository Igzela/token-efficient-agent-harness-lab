# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC3 Golden Path responsibility contract was accepted on `main` by PR #486. Active semantic frontier is reset to `PE7-AC3-CONTRACT-1` complete -> `PE7-AC3-ORCHESTRATOR-CORE-1` reopened and ready for execution; the false completion receipts for downstream packets have been invalidated and moved to the audit table in `docs/CURRENT_STATUS.md`.

## Authoritative Forward Order

```text
[window: PE7-AC6-DASHBOARD-MIGRATION-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-AC6-DASHBOARD-MIGRATION-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC6-SDK-MIGRATION-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #554 exact head `f7f18e1b22c6d16f325bdcb153726d611f9b5761`; merge `77f41084ebd5076e01b1a73fbea821dbc44a98d5`; exact-head `PASS`; canonical workflow `32005606558`.
## Packet PE7-AC6-DASHBOARD-MIGRATION-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC6-SDK-MIGRATION-1 — COMPLETE on accepted main `77f41084ebd5076e01b1a73fbea821dbc44a98d5` (PR #554 exact head `f7f18e1b22c6d16f325bdcb153726d611f9b5761`; merge `77f41084ebd5076e01b1a73fbea821dbc44a98d5`; exact-head `PASS`; canonical workflow `32005606558`).

**Class:** `IMPLEMENT`

**Outcome:** Migrate Dashboard data projections to the accepted generated/versioned contracts without presentation redesign.

**Allowed delta:** dashboard/src/lib/api-client.ts, dashboard/src/lib/provider-embedding-receipt-readonly.typecheck.ts, dashboard/src/lib/regression-evidence.test.ts, dashboard/src/lib/regression-evidence.ts, dashboard/src/lib/scorecard-evidence.test.ts, dashboard/src/lib/scorecard-evidence.ts, dashboard/src/lib/types.ts, docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md, engine/src/wire_types.rs.

**Exit:** Typecheck/build/projection tests and representative old/new payload fixtures pass.

**Stop:** UI needs backend policy, schema ownership, or presentation-only PR #225 content to complete the migration.

### Twelve-field contract

1. **Outcome and non-goals.** Migrate Dashboard data projections to the accepted generated/versioned contracts without presentation redesign.
2. **Prerequisites and evidence.** Accepted main `77f41084ebd5076e01b1a73fbea821dbc44a98d5`; checked route manifest SHA `774ae58a23b11e7920dd6079c65be97fa5636cfe9cd470b63d72f71b16583321`; predecessor receipt PR #554 exact head `f7f18e1b22c6d16f325bdcb153726d611f9b5761`; merge `77f41084ebd5076e01b1a73fbea821dbc44a98d5`; exact-head `PASS`; canonical workflow `32005606558`; current-main evidence SHA `9442c3445b45155d0ce5e5ac5d02a8c1cf6db4e0fa78ff81ffab51cf33336365`.
3. **Owners and paths.** Owners: engine/src/main.rs; callers: engine/src/main.rs; tests: engine/src/main.rs.
4. **Frozen invariants.** Packet identity, route manifest SHA `774ae58a23b11e7920dd6079c65be97fa5636cfe9cd470b63d72f71b16583321`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** dashboard/src/lib/types.ts, dashboard/src/lib/api-client.ts: Migrate Dashboard data projections to generated wire types without presentation redesign
8. **Failure, recovery, and stop taxonomy.** Cleanup: No temporary resources created (proved by docs/ARCHITECTURE_BOOK.md:cleanup); retention: Retain canonical wire schemas and dashboard audit trail invariants (proved by docs/ARCHITECTURE_BOOK.md:audit); decisions: authority unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore); evaluator unchanged (docs/ARCHITECTURE_BOOK.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:rollback); schema unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore).
9. **Verification.** cargo test -p engine; bash scripts/check_wire_codegen_drift.sh; python tools/check_security_baseline.py; uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revertable Dashboard code diff with zero database migrations (proved by docs/ARCHITECTURE_BOOK.md:rollback)
11. **Exit artifact.** Evidence destinations: Accepted closeout of PE7-AC6-SDK-MIGRATION-1 in Accepted Packet Receipts table (docs/CURRENT_STATUS.md:Accepted), Active window promotion for PE7-AC6-DASHBOARD-MIGRATION-1 under Active Routing (docs/NEXT_DECISION.md:READY_FOR_EXECUTION).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["dashboard/src/lib/api-client.ts", "dashboard/src/lib/provider-embedding-receipt-readonly.typecheck.ts", "dashboard/src/lib/regression-evidence.test.ts", "dashboard/src/lib/regression-evidence.ts", "dashboard/src/lib/scorecard-evidence.test.ts", "dashboard/src/lib/scorecard-evidence.ts", "dashboard/src/lib/types.ts", "docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/wire_types.rs"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Accepted closeout of PE7-AC6-SDK-MIGRATION-1 in Accepted Packet Receipts table (docs/CURRENT_STATUS.md:Accepted)", "Active window promotion for PE7-AC6-DASHBOARD-MIGRATION-1 under Active Routing (docs/NEXT_DECISION.md:READY_FOR_EXECUTION)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Migrate Dashboard data projections to the accepted generated/versioned contracts without presentation redesign.", "ordered_steps": ["dashboard/src/lib/types.ts, dashboard/src/lib/api-client.ts: Migrate Dashboard data projections to generated wire types without presentation redesign"], "packet_id": "PE7-AC6-DASHBOARD-MIGRATION-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #554 exact head `f7f18e1b22c6d16f325bdcb153726d611f9b5761`; merge `77f41084ebd5076e01b1a73fbea821dbc44a98d5`; exact-head `PASS`; canonical workflow `32005606558`"], "prerequisites": ["PE7-AC6-SDK-MIGRATION-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "9442c3445b45155d0ce5e5ac5d02a8c1cf6db4e0fa78ff81ffab51cf33336365", "read_paths": ["dashboard/src/lib/api-client.ts", "dashboard/src/lib/provider-embedding-receipt-readonly.typecheck.ts", "dashboard/src/lib/regression-evidence.test.ts", "dashboard/src/lib/regression-evidence.ts", "dashboard/src/lib/scorecard-evidence.test.ts", "dashboard/src/lib/scorecard-evidence.ts", "dashboard/src/lib/types.ts", "docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/main.rs", "engine/src/wire_types.rs"], "risk_class": "none", "rollback": "Revertable Dashboard code diff with zero database migrations (proved by docs/ARCHITECTURE_BOOK.md:rollback)", "route_manifest_sha256": "774ae58a23b11e7920dd6079c65be97fa5636cfe9cd470b63d72f71b16583321", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["cargo test -p engine", "bash scripts/check_wire_codegen_drift.sh", "python tools/check_security_baseline.py", "uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "source_focused_full", "worker_tier": "T1"}
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
