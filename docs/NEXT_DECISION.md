# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC3 Golden Path responsibility contract was accepted on `main` by PR #486. Active semantic frontier is reset to `PE7-AC3-CONTRACT-1` complete -> `PE7-AC3-ORCHESTRATOR-CORE-1` reopened and ready for execution; the false completion receipts for downstream packets have been invalidated and moved to the audit table in `docs/CURRENT_STATUS.md`.

## Authoritative Forward Order

```text
[window: PE7-AC4-VIEWS-CORE-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-AC4-VIEWS-CORE-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC4-CONTRACT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #538 exact head `2be275e714bc753eb492f2545d6174a72d7f87e6`; merge `437838289e48907767321363c394cf095b9f41dd`; exact-head `PASS`; canonical workflow `31986463688`.
## Packet PE7-AC4-VIEWS-CORE-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC4-CONTRACT-1 — COMPLETE on accepted main `437838289e48907767321363c394cf095b9f41dd` (PR #538 exact head `2be275e714bc753eb492f2545d6174a72d7f87e6`; merge `437838289e48907767321363c394cf095b9f41dd`; exact-head `PASS`; canonical workflow `31986463688`).

**Class:** `IMPLEMENT`

**Outcome:** Implement the accepted borrowed transaction views for SQLite and PostgreSQL.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md, engine/src/storage/local_product_store/managed_acceptance.rs, engine/src/storage/local_product_store/mod.rs, engine/src/storage/local_product_store/product_tasks.rs, engine/src/storage/local_product_store/rwe_authority.rs, engine/src/storage/local_product_store/workflow_runs.rs, engine/tests/test_product_golden_path_g3.rs.

**Exit:** Backend-focused atomicity, rollback, failure injection, idempotency, and no-nested-commit tests.

**Stop:** Backend semantics diverge, a view can outlive/commit independently, or migration would require destructive schema change.

### Twelve-field contract

1. **Outcome and non-goals.** Implement the accepted borrowed transaction views for SQLite and PostgreSQL.
2. **Prerequisites and evidence.** Accepted main `437838289e48907767321363c394cf095b9f41dd`; checked route manifest SHA `854c09e9146c70bdf29775126db716f5c9449c4b828b5d209ef1906b686751e5`; predecessor receipt PR #538 exact head `2be275e714bc753eb492f2545d6174a72d7f87e6`; merge `437838289e48907767321363c394cf095b9f41dd`; exact-head `PASS`; canonical workflow `31986463688`; current-main evidence SHA `a4185d297d115e2bd46350799ec63313e6c9c9eaf6b1dce89111a7972fd659bc`.
3. **Owners and paths.** Owners: engine/src/storage/local_product_store/product_tasks.rs; callers: engine/src/http_server/handlers/product_tasks.rs; tests: engine/tests/test_product_golden_path_g3.rs.
4. **Frozen invariants.** Packet identity, route manifest SHA `854c09e9146c70bdf29775126db716f5c9449c4b828b5d209ef1906b686751e5`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/NEXT_DECISION.md: Bind the accepted AC4 views core execution window.; engine/src/storage/local_product_store/mod.rs, engine/src/storage/local_product_store/product_tasks.rs: Implement borrowed transaction views for SQLite and PostgreSQL.
8. **Failure, recovery, and stop taxonomy.** Cleanup: No runtime cleanup or schema mutation required. (proved by docs/ARCHITECTURE_BOOK.md:recovery); retention: Retain the accepted contract receipt. (proved by docs/CURRENT_STATUS.md:PE7-AC4-CONTRACT-1); decisions: authority unchanged (docs/MODULE_MAP.md:authority); evaluator unchanged (docs/CURRENT_STATUS.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:recovery); schema unchanged (docs/CURRENT_STATUS.md:schema).
9. **Verification.** cargo test -p engine; uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revert the transaction views core implementation if backend parity diverges. (proved by docs/ARCHITECTURE_BOOK.md:recovery)
11. **Exit artifact.** Evidence destinations: Accepted AC4 views core in engine/src/storage/local_product_store. (docs/NEXT_DECISION.md:canonical).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/storage/local_product_store/managed_acceptance.rs", "engine/src/storage/local_product_store/mod.rs", "engine/src/storage/local_product_store/product_tasks.rs", "engine/src/storage/local_product_store/rwe_authority.rs", "engine/src/storage/local_product_store/workflow_runs.rs", "engine/tests/test_product_golden_path_g3.rs"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Accepted AC4 views core in engine/src/storage/local_product_store. (docs/NEXT_DECISION.md:canonical)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Implement the accepted borrowed transaction views for SQLite and PostgreSQL.", "ordered_steps": ["docs/NEXT_DECISION.md: Bind the accepted AC4 views core execution window.", "engine/src/storage/local_product_store/mod.rs, engine/src/storage/local_product_store/product_tasks.rs: Implement borrowed transaction views for SQLite and PostgreSQL."], "packet_id": "PE7-AC4-VIEWS-CORE-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #538 exact head `2be275e714bc753eb492f2545d6174a72d7f87e6`; merge `437838289e48907767321363c394cf095b9f41dd`; exact-head `PASS`; canonical workflow `31986463688`"], "prerequisites": ["PE7-AC4-CONTRACT-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "a4185d297d115e2bd46350799ec63313e6c9c9eaf6b1dce89111a7972fd659bc", "read_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/http_server/handlers/product_tasks.rs", "engine/src/storage/local_product_store/managed_acceptance.rs", "engine/src/storage/local_product_store/mod.rs", "engine/src/storage/local_product_store/product_tasks.rs", "engine/src/storage/local_product_store/rwe_authority.rs", "engine/src/storage/local_product_store/workflow_runs.rs", "engine/tests/test_product_golden_path_g3.rs"], "risk_class": "none", "rollback": "Revert the transaction views core implementation if backend parity diverges. (proved by docs/ARCHITECTURE_BOOK.md:recovery)", "route_manifest_sha256": "854c09e9146c70bdf29775126db716f5c9449c4b828b5d209ef1906b686751e5", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["cargo test -p engine", "uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "source_focused_full", "worker_tier": "T1"}
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
