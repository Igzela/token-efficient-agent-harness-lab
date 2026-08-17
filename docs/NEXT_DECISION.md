# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC3 Golden Path responsibility contract was accepted on `main` by PR #486. Active semantic frontier is reset to `PE7-AC3-CONTRACT-1` complete -> `PE7-AC3-ORCHESTRATOR-CORE-1` reopened and ready for execution; the false completion receipts for downstream packets have been invalidated and moved to the audit table in `docs/CURRENT_STATUS.md`.

## Authoritative Forward Order

```text
[window: PE7-AC5-CONTRACT-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-AC5-CONTRACT-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC4-CALLER-MIGRATION-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #542 exact head `b0fb353ac4334ff906b503f1c710c9b8be71d9bb`; merge `658e8ce4619447c64712d2965ba3109d18ee5c7f`; exact-head `PASS`; canonical workflow `31999114312`.
## Packet PE7-AC5-CONTRACT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC4-CALLER-MIGRATION-1 — COMPLETE on accepted main `658e8ce4619447c64712d2965ba3109d18ee5c7f` (PR #542 exact head `b0fb353ac4334ff906b503f1c710c9b8be71d9bb`; merge `658e8ce4619447c64712d2965ba3109d18ee5c7f`; exact-head `PASS`; canonical workflow `31999114312`).

**Class:** `CONTRACT`

**Outcome:** Freeze configuration sources, precedence, validated types, dependency graph, runtime modes, secret-resolution boundary, and module migration batches.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md.

**Exit:** One composition manifest with exact defaults, conflicts, validation errors, owner paths, and staged rollback.

**Stop:** Two accepted sources conflict, a secret would move earlier than the send boundary, or a module requires service-locator/global-registry behavior.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze configuration sources, precedence, validated types, dependency graph, runtime modes, secret-resolution boundary, and module migration batches.
2. **Prerequisites and evidence.** Accepted main `658e8ce4619447c64712d2965ba3109d18ee5c7f`; checked route manifest SHA `fd16641ddb8a9fd4590ab7ec185b27fe8faa7b01e49b04a6f12ca52a68113e9e`; predecessor receipt PR #542 exact head `b0fb353ac4334ff906b503f1c710c9b8be71d9bb`; merge `658e8ce4619447c64712d2965ba3109d18ee5c7f`; exact-head `PASS`; canonical workflow `31999114312`; current-main evidence SHA `c33ea33a11d680594dcae1e577d2c4223495ec6a0c89592c19195224f9f54a91`.
3. **Owners and paths.** Owners: engine/src/main.rs; callers: engine/src/main.rs; tests: engine/src/main.rs.
4. **Frozen invariants.** Packet identity, route manifest SHA `fd16641ddb8a9fd4590ab7ec185b27fe8faa7b01e49b04a6f12ca52a68113e9e`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/ARCHITECTURE_BOOK.md, docs/MODULE_MAP.md: Freeze composition root precedence, validated types, dependency graph, and module migration batches
8. **Failure, recovery, and stop taxonomy.** Cleanup: No temporary resources created (proved by docs/ARCHITECTURE_BOOK.md:cleanup); retention: Retain canonical architecture invariants (proved by docs/ARCHITECTURE_BOOK.md:audit); decisions: authority unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore); evaluator unchanged (docs/ARCHITECTURE_BOOK.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:rollback); schema unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore).
9. **Verification.** python tools/check_security_baseline.py; uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revertable documentation diff with zero schema migrations (proved by docs/ARCHITECTURE_BOOK.md:rollback)
11. **Exit artifact.** Evidence destinations: Accepted closeout of PE7-AC4-CALLER-MIGRATION-1 in Accepted Packet Receipts table (docs/CURRENT_STATUS.md:Accepted), Active window promotion for PE7-AC5-CONTRACT-1 under Active Routing (docs/NEXT_DECISION.md:READY_FOR_EXECUTION).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Accepted closeout of PE7-AC4-CALLER-MIGRATION-1 in Accepted Packet Receipts table (docs/CURRENT_STATUS.md:Accepted)", "Active window promotion for PE7-AC5-CONTRACT-1 under Active Routing (docs/NEXT_DECISION.md:READY_FOR_EXECUTION)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Freeze configuration sources, precedence, validated types, dependency graph, runtime modes, secret-resolution boundary, and module migration batches.", "ordered_steps": ["docs/ARCHITECTURE_BOOK.md, docs/MODULE_MAP.md: Freeze composition root precedence, validated types, dependency graph, and module migration batches"], "packet_id": "PE7-AC5-CONTRACT-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #542 exact head `b0fb353ac4334ff906b503f1c710c9b8be71d9bb`; merge `658e8ce4619447c64712d2965ba3109d18ee5c7f`; exact-head `PASS`; canonical workflow `31999114312`"], "prerequisites": ["PE7-AC4-CALLER-MIGRATION-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "c33ea33a11d680594dcae1e577d2c4223495ec6a0c89592c19195224f9f54a91", "read_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/http_server/mod.rs", "engine/src/http_server/routes.rs", "engine/src/main.rs"], "risk_class": "none", "rollback": "Revertable documentation diff with zero schema migrations (proved by docs/ARCHITECTURE_BOOK.md:rollback)", "route_manifest_sha256": "fd16641ddb8a9fd4590ab7ec185b27fe8faa7b01e49b04a6f12ca52a68113e9e", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["python tools/check_security_baseline.py", "uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "docs_evidence_review", "worker_tier": "T2"}
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
