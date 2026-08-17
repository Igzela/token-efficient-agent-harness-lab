# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC3 Golden Path responsibility contract was accepted on `main` by PR #486. Active semantic frontier is reset to `PE7-AC3-CONTRACT-1` complete -> `PE7-AC3-ORCHESTRATOR-CORE-1` reopened and ready for execution; the false completion receipts for downstream packets have been invalidated and moved to the audit table in `docs/CURRENT_STATUS.md`.

## Authoritative Forward Order

```text
[window: PE7-AC6-CONTRACT-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-AC6-CONTRACT-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC5-MODULE-MIGRATION-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #548 exact head `a10175092de7ba7cc06d41a9a60d9d333dbb3bb2`; merge `4455832eabdb46fd17d6a14ec3ee849bfda04868`; exact-head `PASS`; canonical workflow `32002409448`.
## Packet PE7-AC6-CONTRACT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC5-MODULE-MIGRATION-1 — COMPLETE on accepted main `4455832eabdb46fd17d6a14ec3ee849bfda04868` (PR #548 exact head `a10175092de7ba7cc06d41a9a60d9d333dbb3bb2`; merge `4455832eabdb46fd17d6a14ec3ee849bfda04868`; exact-head `PASS`; canonical workflow `32002409448`).

**Class:** `CONTRACT`

**Outcome:** Freeze authoritative Rust types, wire/schema projections, compatibility matrix, version/deprecation window, migration ordering, and rollback.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md.

**Exit:** Exact type/field/version manifest and generated-artifact ownership with no consumer-defined authority.

**Stop:** A consumer has incompatible semantics, destructive migration lacks recovery, or two schema owners remain.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze authoritative Rust types, wire/schema projections, compatibility matrix, version/deprecation window, migration ordering, and rollback.
2. **Prerequisites and evidence.** Accepted main `4455832eabdb46fd17d6a14ec3ee849bfda04868`; checked route manifest SHA `0f41c724c240c982a31c3e62a0e4422bf8bfbf34bdd58f4fc6c6522faa132c5e`; predecessor receipt PR #548 exact head `a10175092de7ba7cc06d41a9a60d9d333dbb3bb2`; merge `4455832eabdb46fd17d6a14ec3ee849bfda04868`; exact-head `PASS`; canonical workflow `32002409448`; current-main evidence SHA `fa3d575e44669529900915cccae7bc2dfa2931481a129e32f3e08a66a83a525a`.
3. **Owners and paths.** Owners: engine/src/main.rs; callers: engine/src/main.rs; tests: engine/src/main.rs.
4. **Frozen invariants.** Packet identity, route manifest SHA `0f41c724c240c982a31c3e62a0e4422bf8bfbf34bdd58f4fc6c6522faa132c5e`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/ARCHITECTURE_BOOK.md, docs/MODULE_MAP.md: Freeze AC6 type and codegen governance contract with wire manifest, compatibility matrix, and drift ownership rules
8. **Failure, recovery, and stop taxonomy.** Cleanup: No temporary resources created (proved by docs/ARCHITECTURE_BOOK.md:cleanup); retention: Retain canonical architecture invariants and audit log trails (proved by docs/ARCHITECTURE_BOOK.md:audit); decisions: authority unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore); evaluator unchanged (docs/ARCHITECTURE_BOOK.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:rollback); schema unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore).
9. **Verification.** bash scripts/check_wire_codegen_drift.sh; python tools/check_security_baseline.py; uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revertable documentation contract diff with zero schema migrations (proved by docs/ARCHITECTURE_BOOK.md:rollback)
11. **Exit artifact.** Evidence destinations: Accepted closeout of PE7-AC5-MODULE-MIGRATION-1 in Accepted Packet Receipts table (docs/CURRENT_STATUS.md:Accepted), Active window promotion for PE7-AC6-CONTRACT-1 under Active Routing (docs/NEXT_DECISION.md:READY_FOR_EXECUTION).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Accepted closeout of PE7-AC5-MODULE-MIGRATION-1 in Accepted Packet Receipts table (docs/CURRENT_STATUS.md:Accepted)", "Active window promotion for PE7-AC6-CONTRACT-1 under Active Routing (docs/NEXT_DECISION.md:READY_FOR_EXECUTION)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Freeze authoritative Rust types, wire/schema projections, compatibility matrix, version/deprecation window, migration ordering, and rollback.", "ordered_steps": ["docs/ARCHITECTURE_BOOK.md, docs/MODULE_MAP.md: Freeze AC6 type and codegen governance contract with wire manifest, compatibility matrix, and drift ownership rules"], "packet_id": "PE7-AC6-CONTRACT-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #548 exact head `a10175092de7ba7cc06d41a9a60d9d333dbb3bb2`; merge `4455832eabdb46fd17d6a14ec3ee849bfda04868`; exact-head `PASS`; canonical workflow `32002409448`"], "prerequisites": ["PE7-AC5-MODULE-MIGRATION-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "fa3d575e44669529900915cccae7bc2dfa2931481a129e32f3e08a66a83a525a", "read_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/main.rs", "engine/src/wire_types.rs", "scripts/check_wire_codegen_drift.sh", "sdk/python/src/agent_control_plane_sdk/wire_types.py", "sdk/typescript/src/generated-wire-types.ts"], "risk_class": "none", "rollback": "Revertable documentation contract diff with zero schema migrations (proved by docs/ARCHITECTURE_BOOK.md:rollback)", "route_manifest_sha256": "0f41c724c240c982a31c3e62a0e4422bf8bfbf34bdd58f4fc6c6522faa132c5e", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["bash scripts/check_wire_codegen_drift.sh", "python tools/check_security_baseline.py", "uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "docs_evidence_review", "worker_tier": "T2"}
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
