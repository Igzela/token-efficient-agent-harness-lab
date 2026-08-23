# Next Decision

Last updated: 2026-08-23.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-HE-EC2-PREDICTION-OUTCOME-1` is complete. The current window is `PE7-HE-EC3-CONTRACT-1`: freeze lifecycle-cost ontology, trustworthy sources, missingness/eligibility rules, reservation/reconciliation, per-candidate/global envelopes, failure accounting, and the cost of diagnosis, hypothesis construction, prediction, and outcome reconciliation. No spend or runtime behavior change.

## Authoritative Forward Order

```text
[window: PE7-HE-EC3-CONTRACT-1 — READY_FOR_EXECUTION, provider-free; freeze lifecycle-cost ontology and budget contract]
[successor: PE7-HE-EC3-INSTRUMENTATION-1 — BLOCKED_PREREQUISITE, provider-free; capture and normalize lifecycle-cost evidence]
```

## Active Routing

1. `PE7-HE-EC3-CONTRACT-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-BENCHMARK-PREFLIGHT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #588 exact head `c806f75c5910b117c3cf7e44ad1c6a6503e48ddd`; squash merge `1569c70e9f2034bb4f7bc5ccbc24d889b66645ab`.

## Completed (PE7-CWS-BENCHMARK-RUN-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #589 exact head `0f9cad12a850a7ed2ffcc823ebd2da29318c5ae6`; squash merge `84b1933bc3d9e657acae94d9e5f14810c0651917`; exact-head review comments `5347630853` and `5347631083`; canonical workflow `32298813456`; exact-head check `32298813444`; `executed=false`; `provider_posts=0`.

## Completed (PE7-CWS-ANALYSIS-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #590 exact head `da09ea576154e55e532d2de5477972f2c5c516d5`; squash merge `1544c8d0a3f1b196fdb4b560759609662cd5f432`; exact-head review comments `5347818781` and `5347818993`; canonical workflow `32301497907`; exact-head check `32301497898`; `INSUFFICIENT_DEFAULT_OFF`; active Harness `84b1933bc3d9e657acae94d9e5f14810c0651917`.

## Completed (PE7-HE-EC1-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #591 exact head `50661a622c19e1f6da1f934a43bcbbaa4b52a003`; squash merge `e116e212ed043d773e215f2ba029e5b2f1763e4d`; exact-head review comments `5348443154` and `5348443354`; canonical workflow `32306087501`.

## Completed (PE7-HE-EC1-IDENTITY-LINEAGE-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #592 exact head `155fa749effdcd790fb954eefcf64d12790d21b6`; squash merge `3dc2d3b12fbb95ec2b26220681cba5ad7547c6d2`; exact-head review comments `5348823217` and `5348823396`; canonical workflow `32309602816`.

## Completed (PE7-HE-EC1-CAUSAL-MANIFEST-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #593 exact head `c00f24dac433d9b3fc23f5b0df746c89442097dd`; squash merge `b2fa400395a0502bf52ea5fd9468af5830766422`; exact-head review comments `5349023567` and `5349023702`; canonical workflow `32311374839`.

## Completed (PE7-HE-EC1-MUTATION-REGISTRY-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #594 exact head `b3199736d85312083c45a3522211ae086f5fe756`; squash merge `b970226181957de98859f26f03db3bf101b1f8a0`; exact-head review comments `5349266593` and `5349266718`; canonical workflow `32313718374`.

## Completed (PE7-HE-EC2-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #595 exact head `e0585701dec206fca5645299d65cbb3341257008`; squash merge `f996ded631f12f74f42528c70e76ccf0f040bdfd`; exact-head review comments `5349652629` and `5349652752`; canonical workflow `32317253205`.

## Completed (PE7-HE-EC2-HOLDOUT-SEAL-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #596 exact head `cffd49edfc36fe602cc311f025367cadb15a425a`; squash merge `5c367b85d79f680b5f76b7aa4f2f1656c0a460ae`; exact-head review comments `5349963606` and `5349963725`; canonical workflow `32320235684`.

## Completed (PE7-HE-EC2-SENTINEL-CONFORMANCE-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #597 exact head `4e39a52a265d4a9e3a6902c68da142b424b15c36`; squash merge `dbe20eccb4980e595958d615cf937ba34cfdaed2`; exact-head review comments `5350149695` and `5350149805`; canonical workflow `32321977265`.

## Completed (PE7-HE-EC2-PREDICTION-OUTCOME-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #600 exact head `0ccdbefa59e18b241cba7cb6f26f3d267608a9a9`; squash merge `ac2b2f640406ca766b0cd567c2782e426d8dad2b`; exact-head review comments `5385499271` and `5385499345`; canonical workflow `32633510108`.

## Packet PE7-HE-EC3-CONTRACT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-HE-EC2-PREDICTION-OUTCOME-1`

**Class:** `CONTRACT`

**Outcome:** Freeze lifecycle-cost ontology, trustworthy sources, missingness/eligibility rules, reservation/reconciliation, per-candidate/global envelopes, failure accounting, and the cost of diagnosis, hypothesis construction, prediction, and outcome reconciliation.

**Allowed delta:** `docs/ARCHITECTURE_BOOK.md`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/FUTURE_ROUTE.md`, `engine/src/harness_evolution.rs`, and `tests/test_session_context.py`. Lifecycle-budget contract and ontology types only; no spend, persistence, admission, or runtime behavior change.

**Exit:** A versioned budget/accounting contract covers generation, evaluation, review, repair, CI, recovery, human effort, and failed attempts with explicit source and missingness semantics.

**Stop:** A material cost class is silently zero, source semantics are ambiguous, failed attempts can escape accounting, or the contract creates a second spend owner.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze lifecycle-cost ontology, trustworthy sources, missingness/eligibility rules, reservation/reconciliation, per-candidate/global envelopes, failure accounting, and diagnosis/hypothesis/prediction/outcome-reconciliation cost. No spend or runtime behavior change.
2. **Prerequisites and evidence.** PREDICTION-OUTCOME COMPLETE: PR #600 exact head `0ccdbefa59e18b241cba7cb6f26f3d267608a9a9`; squash merge `ac2b2f640406ca766b0cd567c2782e426d8dad2b`; exact-head review comments `5385499271` and `5385499345`; canonical workflow `32633510108`.
3. **Owners and paths.** Existing Harness-Evolution contract owner in `engine/src/harness_evolution.rs`; architecture/status/routing owners remain their existing documents.
4. **Frozen invariants.** Material costs cannot be silently zero. Sources are measured-direct, deterministic-derived, or explicitly unavailable. Missing required cost is ineligible, failed attempts remain charged, and the contract grants no spend authority.
5. **Only semantic delta.** Versioned lifecycle-cost phases, trust-source and missingness semantics, candidate/global envelopes, reservation/reconciliation vocabulary, and validation/sealing helpers in `harness_evolution.rs`.
6. **Forbidden changes.** No spend, persistence, admission, candidate execution, Provider call, second budget/spend owner, ENABLE, or Level-1.
7. **Ordered slices.** Define cost phases and source semantics; define candidate/global envelopes and failure accounting; validate and seal the contract; add positive and adversarial unit tests; synchronize the smallest canonical documents; stop before instrumentation.
8. **Failure taxonomy.** Silently zero material cost, ambiguous source, unavailable required phase, failed-attempt omission, envelope overflow ambiguity, spend-authority delegation.
9. **Verification.** `cargo fmt --all -- --check`; `cargo test -p engine --lib ec3_lifecycle_budget -- --test-threads=1`; `cargo clippy -p engine --all-targets --all-features -- -D warnings`; `git diff --check`; security baseline; and handoff.
10. **Compatibility and rollback.** Additive contract types only. Revert the contract PR; no durable data or external effect exists to undo.
11. **Exit artifact.** Versioned lifecycle-budget contract types and fail-closed validation in `engine/src/harness_evolution.rs`.
12. **Next action.** Promote `PE7-HE-EC3-INSTRUMENTATION-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-HE-EC3-CONTRACT-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Freeze lifecycle-cost ontology, trustworthy sources, missingness and eligibility rules, reservation and reconciliation, per-candidate and global envelopes, failure accounting, and diagnosis, hypothesis, prediction, and outcome-reconciliation cost.","allowed_paths":["docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","engine/src/harness_evolution.rs","tests/test_session_context.py"],"read_paths":["docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/harness_evolution.rs","tests/test_session_context.py"],"allowed_outputs":["Versioned lifecycle-budget contract types and fail-closed validation in engine/src/harness_evolution.rs."],"prerequisites":["PE7-HE-EC2-PREDICTION-OUTCOME-1"],"prerequisite_receipts":["PE7-HE-EC2-PREDICTION-OUTCOME-1 COMPLETE: PR #600 exact head `0ccdbefa59e18b241cba7cb6f26f3d267608a9a9`; squash merge `ac2b2f640406ca766b0cd567c2782e426d8dad2b`; exact-head review comments `5385499271` and `5385499345`; canonical workflow `32633510108`"],"forbidden_changes":["Do not change spend, persistence, admission, or runtime behavior.","Do not create a second budget or spend owner.","Do not execute candidates or call a Provider.","Do not ENABLE the laboratory.","Do not start PE7-HE-LEVEL1-PREFLIGHT-1."],"ordered_steps":["Define lifecycle cost phases and source semantics.","Define candidate and global envelopes, reservation and reconciliation vocabulary, and failure accounting.","Implement versioned validation and sealing helpers.","Add positive and adversarial unit tests.","Stop before instrumentation."],"verification":["cargo fmt --all -- --check","cargo test -p engine --lib ec3_lifecycle_budget -- --test-threads=1","cargo clippy -p engine --all-targets --all-features -- -D warnings","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert the contract PR; no durable data or external effect exists to undo.","pause_gates":["Stop before instrumentation."],"expected_artifacts":["engine/src/harness_evolution.rs lifecycle-budget contract types and validation"],"forbidden_next_actions":["Do not start PE7-HE-EC3-INSTRUMENTATION-1.","Do not start PE7-HE-LEVEL1-PREFLIGHT-1."],"worker_tier":"T2","known_store_mutations":[]}
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
