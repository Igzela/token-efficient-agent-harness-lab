# Next Decision

Last updated: 2026-08-16.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze and the AC2 typed contract, boundary repair, and caller migration are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The current provider-free packet is the AC3 Golden Path responsibility contract; it does not change state semantics, public compatibility, or authority ownership.

## Authoritative Forward Order

```text
[window: PE7-RWE-CR-RUN-1 — T3_REQUIRED, external_effect]

```

## Active Routing

1. `PE7-RWE-CR-RUN-1` — `T3_REQUIRED`

## Completed (PE7-RWE-CR-PROTOCOL-PREFLIGHT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #509 exact head `35e6d73e91a797255ee077481f19f63b0be82a35`; merge `cce8f91caaf1bc1a90259b717ef586820ff47293`; exact-head `PASS`; canonical workflow `31934003226`.
## Packet PE7-RWE-CR-RUN-1

**State:** `T3_REQUIRED`

**Prerequisite:** PE7-RWE-CR-PROTOCOL-PREFLIGHT-1 — COMPLETE on accepted main `cce8f91caaf1bc1a90259b717ef586820ff47293` (PR #509 exact head `35e6d73e91a797255ee077481f19f63b0be82a35`; merge `cce8f91caaf1bc1a90259b717ef586820ff47293`; exact-head `PASS`; canonical workflow `31934003226`).

**Class:** `EFFECT`

**Outcome:** Execute the randomized/interleaved old/new replay exactly once under accepted global stop rules.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md, engine/src/rwe/corpus.rs, engine/src/rwe/operator_corpus.rs, engine/src/rwe/runner.rs, engine/src/storage/local_product_store/rwe_authority.rs, engine/tests/test_pg_integration.rs, scripts/agent-control/plan_lane.py, scripts/check_agent_handoff.py, scripts/check_wire_codegen_drift.sh, scripts/project_context.py, scripts/session_context.py, tests/test_session_context.py.

**Exit:** Complete blinded arm assignments, attempts, lifecycle costs, drift, review, failures, cleanup, and restricted/redacted evidence.

**Stop:** Allocation integrity breaks, drift exceeds registered bounds, one arm loses authority/capacity, outcome unknown occurs, or global stop fires.

### Twelve-field contract

1. **Outcome and non-goals.** Execute the randomized/interleaved old/new replay exactly once under accepted global stop rules.
2. **Prerequisites and evidence.** Accepted main `cce8f91caaf1bc1a90259b717ef586820ff47293`; checked route manifest SHA `eb00c91c2433ae40cc2b69f50e0578931c0df18a4c9735f2c1784d2b48338494`; predecessor receipt PR #509 exact head `35e6d73e91a797255ee077481f19f63b0be82a35`; merge `cce8f91caaf1bc1a90259b717ef586820ff47293`; exact-head `PASS`; canonical workflow `31934003226`; current-main evidence SHA `37dd998770418e4d9adad73d061b96b033d5879ed9b9ef5930fc96cef3fd3484`.
3. **Owners and paths.** Owners: engine/src/storage/local_product_store/rwe_authority.rs; callers: engine/src/rwe/runner.rs, engine/tests/test_pg_integration.rs; tests: engine/tests/test_pg_integration.rs.
4. **Frozen invariants.** Packet identity, route manifest SHA `eb00c91c2433ae40cc2b69f50e0578931c0df18a4c9735f2c1784d2b48338494`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Bind the accepted RWE contemporary run execution window.; engine/src/rwe/corpus.rs, engine/src/rwe/operator_corpus.rs, engine/src/rwe/runner.rs, engine/src/storage/local_product_store/rwe_authority.rs, engine/tests/test_pg_integration.rs, scripts/agent-control/plan_lane.py, scripts/check_agent_handoff.py, scripts/check_wire_codegen_drift.sh, scripts/project_context.py, scripts/session_context.py, tests/test_session_context.py: Execute the randomized/interleaved old/new replay under accepted global stop rules.
8. **Failure, recovery, and stop taxonomy.** Cleanup: No runtime mutation or cleanup required for RWE run. (proved by docs/ARCHITECTURE_BOOK.md:recovery); retention: Retain the accepted RWE protocol preflight receipt. (proved by docs/CURRENT_STATUS.md:receipt); decisions: authority unchanged (docs/MODULE_MAP.md:authority); evaluator unchanged (docs/CURRENT_STATUS.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:recovery); schema unchanged (docs/CURRENT_STATUS.md:schema).
9. **Verification.** bash scripts/check_wire_codegen_drift.sh; uv run --no-project python scripts/check_agent_handoff.py; git diff --check
10. **Compatibility, rollback, and retention.** Revert run receipts and restore pre-run RWE state if execution fails or halts. (proved by docs/ARCHITECTURE_BOOK.md:recovery)
11. **Exit artifact.** Evidence destinations: Canonical route evidence. (docs/NEXT_DECISION.md:canonical).
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/rwe/corpus.rs", "engine/src/rwe/operator_corpus.rs", "engine/src/rwe/runner.rs", "engine/src/storage/local_product_store/rwe_authority.rs", "engine/tests/test_pg_integration.rs", "scripts/agent-control/plan_lane.py", "scripts/check_agent_handoff.py", "scripts/check_wire_codegen_drift.sh", "scripts/project_context.py", "scripts/session_context.py", "tests/test_session_context.py"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Canonical route evidence. (docs/NEXT_DECISION.md:canonical)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Execute the randomized/interleaved old/new replay exactly once under accepted global stop rules.", "ordered_steps": ["docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md: Bind the accepted RWE contemporary run execution window.", "engine/src/rwe/corpus.rs, engine/src/rwe/operator_corpus.rs, engine/src/rwe/runner.rs, engine/src/storage/local_product_store/rwe_authority.rs, engine/tests/test_pg_integration.rs, scripts/agent-control/plan_lane.py, scripts/check_agent_handoff.py, scripts/check_wire_codegen_drift.sh, scripts/project_context.py, scripts/session_context.py, tests/test_session_context.py: Execute the randomized/interleaved old/new replay under accepted global stop rules."], "packet_id": "PE7-RWE-CR-RUN-1", "packet_state": "T3_REQUIRED", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #509 exact head `35e6d73e91a797255ee077481f19f63b0be82a35`; merge `cce8f91caaf1bc1a90259b717ef586820ff47293`; exact-head `PASS`; canonical workflow `31934003226`"], "prerequisites": ["PE7-RWE-CR-PROTOCOL-PREFLIGHT-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "37dd998770418e4d9adad73d061b96b033d5879ed9b9ef5930fc96cef3fd3484", "read_paths": ["docs/ARCHITECTURE_BOOK.md", "docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md", "engine/src/rwe/corpus.rs", "engine/src/rwe/operator_corpus.rs", "engine/src/rwe/runner.rs", "engine/src/storage/local_product_store/rwe_authority.rs", "engine/tests/test_pg_integration.rs", "scripts/agent-control/plan_lane.py", "scripts/check_agent_handoff.py", "scripts/check_wire_codegen_drift.sh", "scripts/project_context.py", "scripts/session_context.py", "tests/test_session_context.py"], "risk_class": "external_effect", "rollback": "Revert run receipts and restore pre-run RWE state if execution fails or halts. (proved by docs/ARCHITECTURE_BOOK.md:recovery)", "route_manifest_sha256": "eb00c91c2433ae40cc2b69f50e0578931c0df18a4c9735f2c1784d2b48338494", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "t3_request_digest": "62952294e931ffbb117867948621acfd5ff5c935226afecb6f923090fb73ae88", "verification": ["bash scripts/check_wire_codegen_drift.sh", "uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "external_effect_evidence", "worker_tier": "T3"}
-->
<!-- route-t3-request:v1
{"accepted_main_sha": "cce8f91caaf1bc1a90259b717ef586820ff47293", "action_digest": "164ddf46c1f275b98f11e610948429fee9428a398eab3ad317ff8a2f202f8320", "authority_owner_digest": "18c86699746978b0b321814ba861afc86cba68085ce9dbd252bab6b5ca493350", "candidate_digest": "62952294e931ffbb117867948621acfd5ff5c935226afecb6f923090fb73ae88", "packet_id": "PE7-RWE-CR-RUN-1", "requested_action": "Execute the randomized/interleaved old/new replay exactly once under accepted global stop rules.", "schema_version": "route_t3_request.v1", "scope_digest": "5773e6f70ddd8d5af7f2031c6579d0eaf180a2adbd5ff255ddff02b1a1e02183"}
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
