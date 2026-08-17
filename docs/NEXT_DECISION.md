# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC3 Golden Path responsibility contract was accepted on `main` by PR #486. Active semantic frontier is reset to `PE7-AC3-CONTRACT-1` complete -> `PE7-AC3-ORCHESTRATOR-CORE-1` reopened and ready for execution; the false completion receipts for downstream packets have been invalidated and moved to the audit table in `docs/CURRENT_STATUS.md`.

## Authoritative Forward Order

```text
[window: PE7-AC3-PORT-MIGRATION-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-AC3-PORT-MIGRATION-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC3-ORCHESTRATOR-CORE-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #533 exact head `7c3f3de61b2ecc29fd3512e693948d52d511f4a3`; merge `77dac008a60cb569d3b9a8eb1a3e013e47743387`; exact-head `PASS`; canonical workflow `31984430115`.
## Packet PE7-AC3-PORT-MIGRATION-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC3-ORCHESTRATOR-CORE-1 — COMPLETE on accepted main `77dac008a60cb569d3b9a8eb1a3e013e47743387` (PR #533 exact head `7c3f3de61b2ecc29fd3512e693948d52d511f4a3`; merge `77dac008a60cb569d3b9a8eb1a3e013e47743387`; exact-head `PASS`; canonical workflow `31984430115`).

**Class:** `IMPLEMENT`

**Outcome:** Route store mutations and external effects through the accepted ports and migrate existing entrypoints.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md, engine/src/http_server/handlers/product_tasks.rs, engine/src/product_golden_path.rs, engine/src/storage/local_product_store/managed_acceptance.rs, engine/src/storage/local_product_store/product_tasks.rs, engine/tests/test_product_golden_path_g3.rs.

**Exit:** End-to-end, restart, idempotency, audit, terminal, cleanup, and output traces remain equivalent with no duplicate path.

**Stop:** An adapter becomes a policy owner, transaction ordering changes, or old and new effect paths can both execute.

### Twelve-field contract

1. **Outcome and non-goals.** Route store mutations and external effects through the accepted ports and migrate existing entrypoints.
2. **Prerequisites and evidence.** Accepted main `77dac008a60cb569d3b9a8eb1a3e013e47743387`; checked route manifest SHA `d6941e8a5d9bb4c041d0f3460e8a922cde341fcae91a62169069605489a7c144`; predecessor receipt PR #533 exact head `7c3f3de61b2ecc29fd3512e693948d52d511f4a3`; merge `77dac008a60cb569d3b9a8eb1a3e013e47743387`; exact-head `PASS`; canonical workflow `31984430115`; current-main evidence SHA `71b4920cda602397dc197e4b7d463ae360434a1ac0898c763d2e2f21c26ad7e2`.
3. **Owners and paths.** Owners: engine/src/product_golden_path.rs, engine/src/storage/local_product_store/product_tasks.rs; callers: engine/src/http_server/handlers/product_tasks.rs; tests: engine/tests/test_product_golden_path_g3.rs.
4. **Frozen invariants.** Packet identity, route manifest SHA `d6941e8a5d9bb4c041d0f3460e8a922cde341fcae91a62169069605489a7c144`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/NEXT_DECISION.md: Bind the accepted AC3 port migration execution window.; engine/src/storage/local_product_store/product_tasks.rs: Route store mutations through accepted ports and preserve traces.
8. **Failure, recovery, and stop taxonomy.** Cleanup: No runtime mutation or cleanup required for port migration adapter. (proved by docs/ARCHITECTURE_BOOK.md:recovery); retention: Retain the accepted pure orchestrator core receipt. (proved by docs/CURRENT_STATUS.md:receipt); decisions: authority unchanged (docs/MODULE_MAP.md:authority); evaluator unchanged (docs/CURRENT_STATUS.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:recovery); schema unchanged (docs/CURRENT_STATUS.md:schema).
9. **Verification.** cargo test -p engine; uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revert the port migration adapter if port execution diverges. (proved by docs/ARCHITECTURE_BOOK.md:recovery)
11. **Exit artifact.** Evidence destinations: LocalProductStore port migration in engine/src/storage/local_product_store/product_tasks.rs and HTTP handlers in engine/src/http_server/handlers/product_tasks.rs. (docs/NEXT_DECISION.md:canonical).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/http_server/handlers/product_tasks.rs", "engine/src/product_golden_path.rs", "engine/src/storage/local_product_store/managed_acceptance.rs", "engine/src/storage/local_product_store/product_tasks.rs", "engine/tests/test_product_golden_path_g3.rs"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["LocalProductStore port migration in engine/src/storage/local_product_store/product_tasks.rs and HTTP handlers in engine/src/http_server/handlers/product_tasks.rs. (docs/NEXT_DECISION.md:canonical)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Route store mutations and external effects through the accepted ports and migrate existing entrypoints.", "ordered_steps": ["docs/NEXT_DECISION.md: Bind the accepted AC3 port migration execution window.", "engine/src/storage/local_product_store/product_tasks.rs: Route store mutations through accepted ports and preserve traces."], "packet_id": "PE7-AC3-PORT-MIGRATION-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #533 exact head `7c3f3de61b2ecc29fd3512e693948d52d511f4a3`; merge `77dac008a60cb569d3b9a8eb1a3e013e47743387`; exact-head `PASS`; canonical workflow `31984430115`"], "prerequisites": ["PE7-AC3-ORCHESTRATOR-CORE-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "71b4920cda602397dc197e4b7d463ae360434a1ac0898c763d2e2f21c26ad7e2", "read_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/http_server/handlers/product_tasks.rs", "engine/src/product_golden_path.rs", "engine/src/storage/local_product_store/managed_acceptance.rs", "engine/src/storage/local_product_store/product_tasks.rs", "engine/tests/test_product_golden_path_g3.rs"], "risk_class": "none", "rollback": "Revert the port migration adapter if port execution diverges. (proved by docs/ARCHITECTURE_BOOK.md:recovery)", "route_manifest_sha256": "d6941e8a5d9bb4c041d0f3460e8a922cde341fcae91a62169069605489a7c144", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["cargo test -p engine", "uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "source_focused_full", "worker_tier": "T1"}
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
