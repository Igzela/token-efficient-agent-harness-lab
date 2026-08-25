# Next Decision

Last updated: 2026-08-25.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

The Harness-Evolution C0 loop is closed and the `PE7-HE-MX1-CONTRACT-1` three-axis experiment contract is accepted on `main`, including exact descriptor, admission, comparability, allocation, budget, estimand, and `INCOMPARABLE` boundaries. The current window is `PE7-HE-MX1-CORE-1`: implement the provider-free shared Harness run seam, exact arm manifest, ModelPlan and StrategyPlan adapters, deterministic matrix planning, and read-only evidence projections. No Provider call, live matrix cell, holdout access, target write, or PILOT execution.

## Authoritative Forward Order

```text
[completed: PE7-HE-MX1-CONTRACT-1 — COMPLETE, provider-free three-axis contract freeze]
[window: PE7-HE-MX1-CORE-1 — READY_FOR_EXECUTION, provider-free]
```

## Active Routing

1. `PE7-HE-MX1-CORE-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-HE-MX1-CONTRACT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #617 exact head `23675dbff59d030eeae2e7e6cbfebca81f50e66b`; merge `7caed005a9914e8669a64f6174eab286e160e6d7`; exact-head `PASS`; canonical workflow `32828369869`.

## Packet PE7-HE-MX1-CORE-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-HE-MX1-CONTRACT-1 — COMPLETE on accepted main `7caed005a9914e8669a64f6174eab286e160e6d7` (PR #617 exact head `23675dbff59d030eeae2e7e6cbfebca81f50e66b`; merge `7caed005a9914e8669a64f6174eab286e160e6d7`; exact-head `PASS`; canonical workflow `32828369869`).

**Class:** `IMPLEMENT`

**Outcome:** Deepen the current execution path into one high-level Harness run seam, add exactly one admitted second Harness implementation, and implement or reuse the frozen baseline/no-projection, memory-only, and skill-only Strategy adapters plus ModelPlan variants through the C0 evidence contract.

**Allowed delta:** docs/CURRENT_STATUS.md, docs/NEXT_DECISION.md, docs/FUTURE_ROUTE.md, tests/test_session_context.py, engine/src/harness_evolution.rs, engine/src/product_golden_path.rs, engine/src/storage/local_product_store/harness_evolution.rs, engine/tests/test_local_product_store.rs, engine/tests/test_product_golden_path_authority.rs, engine/tests/test_product_golden_path_evidence.rs, engine/tests/test_product_golden_path_recovery.rs.

**Exit:** Both Harness implementations pass the same interface tests for task/workspace confinement, terminal outcome, verified deliverable, usage/cost, cancellation, cleanup, restart, and failures. Memory/skill projections pass source binding, stale/expiry, deletion/rebuild, leakage, cross-arm isolation, and no-authority tests; unsupported cells deterministically return `INCOMPARABLE`.

**Stop:** The seam merely forwards CLI details, an external harness becomes a second runtime authority, evidence semantics differ silently, a model/strategy changes evaluator or budget, a projection becomes durable truth or leaks across arms, or current binary/confinement restrictions are weakened.

### Twelve-field contract

1. **Outcome and non-goals.** Deepen the current execution path into one high-level Harness run seam, add exactly one admitted second Harness implementation, and implement or reuse the frozen baseline/no-projection, memory-only, and skill-only Strategy adapters plus ModelPlan variants through the C0 evidence contract.
2. **Prerequisites and evidence.** Accepted main `7caed005a9914e8669a64f6174eab286e160e6d7`; checked route manifest SHA `44cd4d0b591c5140a6321a4f180102b09690571c01fe56c74117c29dc0d44842`; predecessor receipt PR #617 exact head `23675dbff59d030eeae2e7e6cbfebca81f50e66b`; merge `7caed005a9914e8669a64f6174eab286e160e6d7`; exact-head `PASS`; canonical workflow `32828369869`; current-main evidence SHA `6c2daab37aa7ef93a2423ece17bae1a8073cc5ea8967a4b89940f9538ae0db62`.
3. **Owners and paths.** Owners: engine/src/harness_evolution.rs, engine/src/product_golden_path.rs, engine/src/storage/local_product_store/harness_evolution.rs; callers: engine/src/storage/local_product_store/harness_evolution.rs, engine/tests/test_product_golden_path_authority.rs; tests: engine/tests/test_local_product_store.rs, engine/tests/test_product_golden_path_authority.rs, engine/tests/test_product_golden_path_evidence.rs, engine/tests/test_product_golden_path_recovery.rs, tests/test_session_context.py.
4. **Frozen invariants.** Packet identity, route manifest SHA `44cd4d0b591c5140a6321a4f180102b09690571c01fe56c74117c29dc0d44842`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** Define the frozen HarnessImplementation, ModelPlan, and StrategyPlan descriptors plus exact manifest hashing and reject unresolved or drifted identities before any run; Deepen the existing Product Golden Path into one provider-free Harness run seam whose normalized result preserves workspace confinement, terminal outcome, verified deliverable, usage and cost, failure, cancellation, cleanup, restart, and recovery evidence; Implement the arm-zero Harness adapter and exactly one independently admitted second Harness adapter without transferring engine scheduling, policy, budget, verification, evidence, output, audit, recovery, or persistence authority; Implement baseline no-projection, memory-only, and skill-only Strategy adapters plus the two frozen ModelPlan identities with stale, expiry, deletion, rebuild, leakage, and cross-arm isolation guards; Build deterministic provider-free matrix planning and read-only projections that return INCOMPARABLE for unsupported cells and never coerce missing or OutcomeUnknown evidence into an outcome; Add focused negative and parity tests, synchronize only the canonical route owners at closeout, and stop before PILOT or any Provider effect
8. **Failure, recovery, and stop taxonomy.** Cleanup: Remove only packet-owned disposable worktrees, subprocesses, projections, and fixture artifacts after tests prove termination and absence; preserve accepted receipts and any evidence needed to diagnose an OutcomeUnknown or failed cleanup.; retention: Retain exact descriptor and manifest digests, admission dispositions, deterministic matrix plans, normalized provider-free fixture evidence, and canonical merge receipts through existing repository and LocalProductStore owners; retain no credential values or raw private content.; decisions: The Rust engine and LocalProductStore remain the sole runtime, scheduler, policy, budget, evaluator boundary, persistence, audit, recovery, and output authorities; The second Harness is admitted only behind the shared seam and cannot become an independent workspace, scheduler, store, evaluator, approval, or rollback owner; Evaluator, budget, task corpus, arm-zero statistical contract, and external-effect authority remain unchanged; No Provider call, target write, holdout access, PILOT execution, schema-owner transfer, release, deployment, or active-Harness adoption is authorized.
9. **Verification.** cargo test --manifest-path engine/Cargo.toml harness_evolution; cargo test --manifest-path engine/Cargo.toml --test test_product_golden_path_authority --test test_product_golden_path_evidence --test test_product_golden_path_recovery --test test_local_product_store; cargo test --manifest-path engine/Cargo.toml; PYTHONPATH=src uv run --no-project python -m unittest discover -s tests; PYTHONPATH=src uv run --no-project --with pyyaml python -m unittest discover -s tools; bash scripts/check_toolchain_drift.sh; git diff --check; uv run --no-project python scripts/check_agent_handoff.py; uv run --no-project python tools/check_security_baseline.py
10. **Compatibility, rollback, and retention.** Revert the bounded CORE commit set and retain the accepted C1 contract and arm-zero evidence; no Provider effect, target write, adoption, or active-Harness change is permitted, so rollback restores the pre-CORE provider-free tree without data replay.
11. **Exit artifact.** Evidence destinations: provider-free Harness run seam interface and adapter tests, exact three-axis descriptor manifest and digest, deterministic matrix and INCOMPARABLE projection fixtures, canonical packet receipt and successor routing projection.
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/CURRENT_STATUS.md", "docs/NEXT_DECISION.md", "docs/FUTURE_ROUTE.md", "tests/test_session_context.py", "engine/src/harness_evolution.rs", "engine/src/product_golden_path.rs", "engine/src/storage/local_product_store/harness_evolution.rs", "engine/tests/test_local_product_store.rs", "engine/tests/test_product_golden_path_authority.rs", "engine/tests/test_product_golden_path_evidence.rs", "engine/tests/test_product_golden_path_recovery.rs"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["provider-free Harness run seam interface and adapter tests", "exact three-axis descriptor manifest and digest", "deterministic matrix and INCOMPARABLE projection fixtures", "canonical packet receipt and successor routing projection"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "goal": "Deepen the current execution path into one high-level Harness run seam, add exactly one admitted second Harness implementation, and implement or reuse the frozen baseline/no-projection, memory-only, and skill-only Strategy adapters plus ModelPlan variants through the C0 evidence contract.", "ordered_steps": ["Define the frozen HarnessImplementation, ModelPlan, and StrategyPlan descriptors plus exact manifest hashing and reject unresolved or drifted identities before any run", "Deepen the existing Product Golden Path into one provider-free Harness run seam whose normalized result preserves workspace confinement, terminal outcome, verified deliverable, usage and cost, failure, cancellation, cleanup, restart, and recovery evidence", "Implement the arm-zero Harness adapter and exactly one independently admitted second Harness adapter without transferring engine scheduling, policy, budget, verification, evidence, output, audit, recovery, or persistence authority", "Implement baseline no-projection, memory-only, and skill-only Strategy adapters plus the two frozen ModelPlan identities with stale, expiry, deletion, rebuild, leakage, and cross-arm isolation guards", "Build deterministic provider-free matrix planning and read-only projections that return INCOMPARABLE for unsupported cells and never coerce missing or OutcomeUnknown evidence into an outcome", "Add focused negative and parity tests, synchronize only the canonical route owners at closeout, and stop before PILOT or any Provider effect"], "packet_id": "PE7-HE-MX1-CORE-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #617 exact head `23675dbff59d030eeae2e7e6cbfebca81f50e66b`; merge `7caed005a9914e8669a64f6174eab286e160e6d7`; exact-head `PASS`; canonical workflow `32828369869`"], "prerequisites": ["PE7-HE-MX1-CONTRACT-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "6c2daab37aa7ef93a2423ece17bae1a8073cc5ea8967a4b89940f9538ae0db62", "read_paths": ["docs/CURRENT_STATUS.md", "docs/NEXT_DECISION.md", "docs/FUTURE_ROUTE.md", "tests/test_session_context.py", "engine/src/harness_evolution.rs", "engine/src/product_golden_path.rs", "engine/src/storage/local_product_store/harness_evolution.rs", "engine/tests/test_local_product_store.rs", "engine/tests/test_product_golden_path_authority.rs", "engine/tests/test_product_golden_path_evidence.rs", "engine/tests/test_product_golden_path_recovery.rs", "docs/MODULE_MAP.md", "docs/ARCHITECTURE_BOOK.md", "docs/REAL_WORLD_TESTING_PLAYBOOK.md"], "risk_class": "none", "rollback": "Revert the bounded CORE commit set and retain the accepted C1 contract and arm-zero evidence; no Provider effect, target write, adoption, or active-Harness change is permitted, so rollback restores the pre-CORE provider-free tree without data replay.", "route_manifest_sha256": "44cd4d0b591c5140a6321a4f180102b09690571c01fe56c74117c29dc0d44842", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["cargo test --manifest-path engine/Cargo.toml harness_evolution", "cargo test --manifest-path engine/Cargo.toml --test test_product_golden_path_authority --test test_product_golden_path_evidence --test test_product_golden_path_recovery --test test_local_product_store", "cargo test --manifest-path engine/Cargo.toml", "PYTHONPATH=src uv run --no-project python -m unittest discover -s tests", "PYTHONPATH=src uv run --no-project --with pyyaml python -m unittest discover -s tools", "bash scripts/check_toolchain_drift.sh", "git diff --check", "uv run --no-project python scripts/check_agent_handoff.py", "uv run --no-project python tools/check_security_baseline.py"], "verification_family": "source_focused_full", "worker_tier": "T1"}
-->

## Common Execution Protocol

- `READY_FOR_EXECUTION` packets require a valid dispatch capsule; blocked or decision-required packets carry no executable capsule.
- Refresh accepted `main`, the current packet, exact PR head, CI, and review receipts before each transition.
- Keep a changing PR Draft; run focused/local full checks, then one final stable-head Standards/Spec review, Ready, canonical exact-head CI, manual squash merge, and main refresh.
- No Provider call, credential-value read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner in this packet.
- Unknown, stale, missing, or conflicting evidence remains fail-closed; never retry an effect whose outcome may be unknown.

## Hard Stops

- `DECISION_REQUIRED` when a caller, owner, path, semantics, compatibility fact, rollback, or evidence cannot be re-proved from accepted `main`.
- No Provider call, credential-value read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner.
- Unknown or possibly executed effects are never treated as success or retried.
## Future Route Boundary

`docs/FUTURE_ROUTE.md` is routing-only. RUN-1 remains a retained live-ready blocker.
