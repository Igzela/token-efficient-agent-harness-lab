# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The current provider-free packet is the AC3 Golden Path responsibility contract; it does not change state semantics, public compatibility, or authority ownership.

## Authoritative Forward Order

```text
[window: PE7-AC6-RUST-CODEGEN-1 — READY_FOR_EXECUTION, provider-free]

```

## Active Routing

1. `PE7-AC6-RUST-CODEGEN-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC6-CONTRACT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #499 exact head `4cc7f48589ff98706cc66f002b017b7cf6e73748`; merge `9a7dbb2ddb7887b62904b13988cd6217d504d31b`; exact-head `PASS`; canonical workflow `31931854403`.
## Packet PE7-AC6-RUST-CODEGEN-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AC6-CONTRACT-1 — COMPLETE on accepted main `9a7dbb2ddb7887b62904b13988cd6217d504d31b` (PR #499 exact head `4cc7f48589ff98706cc66f002b017b7cf6e73748`; merge `9a7dbb2ddb7887b62904b13988cd6217d504d31b`; exact-head `PASS`; canonical workflow `31931854403`).

**Class:** `IMPLEMENT`

**Outcome:** Implement the Rust source types and deterministic schema/codegen projections.

**Allowed delta:** codegen/generate_wire_types.py, docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md, engine/src/storage/local_product_store/product_tasks.rs, engine/src/wire_types.rs, engine/tests/test_product_golden_path_g3.rs, scripts/check_wire_codegen_drift.sh, sdk/python/src/agent_control_plane_sdk/wire_types.py, sdk/typescript/src/generated-wire-types.ts.

**Exit:** Drift guard, deterministic regeneration, Rust/wire validation, and old-reader/new-writer tests pass.

**Stop:** Generated output is nondeterministic, hand-edited projection is required, or rollback cannot read persisted/API data.

### Twelve-field contract

1. **Outcome and non-goals.** Implement the Rust source types and deterministic schema/codegen projections.
2. **Prerequisites and evidence.** Accepted main `9a7dbb2ddb7887b62904b13988cd6217d504d31b`; checked route manifest SHA `5c3b1078d5ded4f5433466ac74e66c3cf634a63816ed91a6bb2fc4747c6ded3c`; predecessor receipt PR #499 exact head `4cc7f48589ff98706cc66f002b017b7cf6e73748`; merge `9a7dbb2ddb7887b62904b13988cd6217d504d31b`; exact-head `PASS`; canonical workflow `31931854403`; current-main evidence SHA `d0d6c12aa2b8f28922f34fbaa8dbe7fb6eef908ab5977cbe1593f5eb89257ad8`.
3. **Owners and paths.** Owners: engine/src/storage/local_product_store/product_tasks.rs; callers: engine/src/storage/local_product_store/managed_acceptance.rs, engine/tests/test_product_golden_path_g3.rs; tests: engine/tests/test_product_golden_path_g3.rs.
4. **Frozen invariants.** Packet identity, route manifest SHA `5c3b1078d5ded4f5433466ac74e66c3cf634a63816ed91a6bb2fc4747c6ded3c`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Bind the accepted AC6 authoritative wire types and codegen execution window.; codegen/generate_wire_types.py, engine/src/storage/local_product_store/product_tasks.rs, engine/src/wire_types.rs, engine/tests/test_product_golden_path_g3.rs, scripts/check_wire_codegen_drift.sh, sdk/python/src/agent_control_plane_sdk/wire_types.py, sdk/typescript/src/generated-wire-types.ts: Implement authoritative Rust source types and deterministic wire codegen with zero drift.
8. **Failure, recovery, and stop taxonomy.** Cleanup: No runtime mutation or cleanup required for wire codegen. (proved by docs/ARCHITECTURE_BOOK.md:recovery); retention: Retain the accepted AC6 contract receipt. (proved by docs/CURRENT_STATUS.md:receipt); decisions: authority unchanged (docs/MODULE_MAP.md:authority); evaluator unchanged (docs/CURRENT_STATUS.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:recovery); schema unchanged (docs/CURRENT_STATUS.md:schema).
9. **Verification.** bash scripts/check_wire_codegen_drift.sh; uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revert the wire types and codegen implementation if generated shapes diverge. (proved by docs/ARCHITECTURE_BOOK.md:recovery)
11. **Exit artifact.** Evidence destinations: Canonical route evidence. (docs/NEXT_DECISION.md:canonical).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["codegen/generate_wire_types.py", "docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/storage/local_product_store/product_tasks.rs", "engine/src/wire_types.rs", "engine/tests/test_product_golden_path_g3.rs", "scripts/check_wire_codegen_drift.sh", "sdk/python/src/agent_control_plane_sdk/wire_types.py", "sdk/typescript/src/generated-wire-types.ts"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Canonical route evidence. (docs/NEXT_DECISION.md:canonical)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Implement the Rust source types and deterministic schema/codegen projections.", "ordered_steps": ["docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Bind the accepted AC6 authoritative wire types and codegen execution window.", "codegen/generate_wire_types.py, engine/src/storage/local_product_store/product_tasks.rs, engine/src/wire_types.rs, engine/tests/test_product_golden_path_g3.rs, scripts/check_wire_codegen_drift.sh, sdk/python/src/agent_control_plane_sdk/wire_types.py, sdk/typescript/src/generated-wire-types.ts: Implement authoritative Rust source types and deterministic wire codegen with zero drift."], "packet_id": "PE7-AC6-RUST-CODEGEN-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #499 exact head `4cc7f48589ff98706cc66f002b017b7cf6e73748`; merge `9a7dbb2ddb7887b62904b13988cd6217d504d31b`; exact-head `PASS`; canonical workflow `31931854403`"], "prerequisites": ["PE7-AC6-CONTRACT-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "d0d6c12aa2b8f28922f34fbaa8dbe7fb6eef908ab5977cbe1593f5eb89257ad8", "read_paths": ["codegen/generate_wire_types.py", "docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/storage/local_product_store/managed_acceptance.rs", "engine/src/storage/local_product_store/product_tasks.rs", "engine/src/wire_types.rs", "engine/tests/test_product_golden_path_g3.rs", "scripts/check_wire_codegen_drift.sh", "sdk/python/src/agent_control_plane_sdk/wire_types.py", "sdk/typescript/src/generated-wire-types.ts"], "risk_class": "none", "rollback": "Revert the wire types and codegen implementation if generated shapes diverge. (proved by docs/ARCHITECTURE_BOOK.md:recovery)", "route_manifest_sha256": "5c3b1078d5ded4f5433466ac74e66c3cf634a63816ed91a6bb2fc4747c6ded3c", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["bash scripts/check_wire_codegen_drift.sh", "uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "source_focused_full", "worker_tier": "T1"}
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
