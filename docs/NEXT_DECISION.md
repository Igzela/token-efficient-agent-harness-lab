# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC3 Golden Path responsibility contract was accepted on `main` by PR #486. Active semantic frontier is reset to `PE7-AC3-CONTRACT-1` complete -> `PE7-AC3-ORCHESTRATOR-CORE-1` reopened and ready for execution; the false completion receipts for downstream packets have been invalidated and moved to the audit table in `docs/CURRENT_STATUS.md`.

## Authoritative Forward Order

```text
[window: PE7-AC6-RUST-CODEGEN-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-AC6-RUST-CODEGEN-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC6-CONTRACT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #550 exact head `5e7f0130b8ca518e5d30d198f7209332e77c99bb`; merge `baa21e0495154e6ba00d4f06452679b9a6722e0b`; exact-head `PASS`; canonical workflow `32003759191`.
## Packet PE7-AC6-RUST-CODEGEN-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC6-CONTRACT-1 — COMPLETE on accepted main `baa21e0495154e6ba00d4f06452679b9a6722e0b` (PR #550 exact head `5e7f0130b8ca518e5d30d198f7209332e77c99bb`; merge `baa21e0495154e6ba00d4f06452679b9a6722e0b`; exact-head `PASS`; canonical workflow `32003759191`).

**Class:** `IMPLEMENT`

**Outcome:** Implement the Rust source types and deterministic schema/codegen projections.

**Allowed delta:** codegen/generate_wire_types.py, docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md, engine/src/wire_types.rs, scripts/check_wire_codegen_drift.sh, sdk/python/src/agent_control_plane_sdk/wire_types.py, sdk/typescript/src/generated-wire-types.ts.

**Exit:** Drift guard, deterministic regeneration, Rust/wire validation, and old-reader/new-writer tests pass.

**Stop:** Generated output is nondeterministic, hand-edited projection is required, or rollback cannot read persisted/API data.

### Twelve-field contract

1. **Outcome and non-goals.** Implement the Rust source types and deterministic schema/codegen projections.
2. **Prerequisites and evidence.** Accepted main `baa21e0495154e6ba00d4f06452679b9a6722e0b`; checked route manifest SHA `5c3b1078d5ded4f5433466ac74e66c3cf634a63816ed91a6bb2fc4747c6ded3c`; predecessor receipt PR #550 exact head `5e7f0130b8ca518e5d30d198f7209332e77c99bb`; merge `baa21e0495154e6ba00d4f06452679b9a6722e0b`; exact-head `PASS`; canonical workflow `32003759191`; current-main evidence SHA `79ef227d326a164b15528a56bf4f85730045b227b78dc09663781863338f3926`.
3. **Owners and paths.** Owners: engine/src/main.rs; callers: engine/src/main.rs; tests: engine/src/main.rs.
4. **Frozen invariants.** Packet identity, route manifest SHA `5c3b1078d5ded4f5433466ac74e66c3cf634a63816ed91a6bb2fc4747c6ded3c`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** codegen/generate_wire_types.py, engine/src/wire_types.rs, sdk/python/src/agent_control_plane_sdk/wire_types.py, sdk/typescript/src/generated-wire-types.ts: Implement Rust wire types and deterministic codegen multi-language generator
8. **Failure, recovery, and stop taxonomy.** Cleanup: No temporary resources created (proved by docs/ARCHITECTURE_BOOK.md:cleanup); retention: Retain canonical wire schemas and audit trail invariants (proved by docs/ARCHITECTURE_BOOK.md:audit); decisions: authority unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore); evaluator unchanged (docs/ARCHITECTURE_BOOK.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:rollback); schema unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore).
9. **Verification.** cargo test -p engine; bash scripts/check_wire_codegen_drift.sh; python tools/check_security_baseline.py; uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revertable code diff with zero database migrations (proved by docs/ARCHITECTURE_BOOK.md:rollback)
11. **Exit artifact.** Evidence destinations: Accepted closeout of PE7-AC6-CONTRACT-1 in Accepted Packet Receipts table (docs/CURRENT_STATUS.md:Accepted), Active window promotion for PE7-AC6-RUST-CODEGEN-1 under Active Routing (docs/NEXT_DECISION.md:READY_FOR_EXECUTION).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["codegen/generate_wire_types.py", "docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/wire_types.rs", "scripts/check_wire_codegen_drift.sh", "sdk/python/src/agent_control_plane_sdk/wire_types.py", "sdk/typescript/src/generated-wire-types.ts"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Accepted closeout of PE7-AC6-CONTRACT-1 in Accepted Packet Receipts table (docs/CURRENT_STATUS.md:Accepted)", "Active window promotion for PE7-AC6-RUST-CODEGEN-1 under Active Routing (docs/NEXT_DECISION.md:READY_FOR_EXECUTION)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Implement the Rust source types and deterministic schema/codegen projections.", "ordered_steps": ["codegen/generate_wire_types.py, engine/src/wire_types.rs, sdk/python/src/agent_control_plane_sdk/wire_types.py, sdk/typescript/src/generated-wire-types.ts: Implement Rust wire types and deterministic codegen multi-language generator"], "packet_id": "PE7-AC6-RUST-CODEGEN-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #550 exact head `5e7f0130b8ca518e5d30d198f7209332e77c99bb`; merge `baa21e0495154e6ba00d4f06452679b9a6722e0b`; exact-head `PASS`; canonical workflow `32003759191`"], "prerequisites": ["PE7-AC6-CONTRACT-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "79ef227d326a164b15528a56bf4f85730045b227b78dc09663781863338f3926", "read_paths": ["codegen/generate_wire_types.py", "docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/main.rs", "engine/src/wire_types.rs", "scripts/check_wire_codegen_drift.sh", "sdk/python/src/agent_control_plane_sdk/wire_types.py", "sdk/typescript/src/generated-wire-types.ts", "wire_contract/v1/dispatch_bundle.schema.json", "wire_contract/v1/dispatch_decision.schema.json", "wire_contract/v1/dispatch_request.schema.json", "wire_contract/v1/evaluation_result.schema.json", "wire_contract/v1/execution_result.schema.json", "wire_contract/v1/task_analysis.schema.json"], "risk_class": "none", "rollback": "Revertable code diff with zero database migrations (proved by docs/ARCHITECTURE_BOOK.md:rollback)", "route_manifest_sha256": "5c3b1078d5ded4f5433466ac74e66c3cf634a63816ed91a6bb2fc4747c6ded3c", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["cargo test -p engine", "bash scripts/check_wire_codegen_drift.sh", "python tools/check_security_baseline.py", "uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "source_focused_full", "worker_tier": "T1"}
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
