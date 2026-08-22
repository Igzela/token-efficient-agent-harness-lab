# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-HE-EC4-ADMISSION-1` is complete. The current window is `PE7-HE-EC4-COVERAGE-CLOSEOUT-1`: validate family/parent/seed exploration coverage and collapse sentinel behavior on provider-free fixtures. Conformance and validation only; hard safety/quality gates remain separate and prior.

## Authoritative Forward Order

```text
[window: PE7-HE-EC4-COVERAGE-CLOSEOUT-1 — READY_FOR_EXECUTION, provider-free; validate coverage and sentinel behavior]
[successor: PE7-HE-EC5-CONTRACT-1 — BLOCKED_PREREQUISITE, provider-free; freeze hard-gate order and Pareto archive contract]
```

## Active Routing

1. `PE7-HE-EC4-COVERAGE-CLOSEOUT-1` — `READY_FOR_EXECUTION`

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

**Accepted evidence:** PR #598 exact head `0d048126b84050d2f09919f8bda912f27715f53c`; squash merge `0d048126b84050d2f09919f8bda912f27715f53c`; exact-head review comments `5350320011` and `5350320022`; canonical workflow `32323812001`.

## Completed (PE7-HE-EC3-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #599 exact head `14c87504eb28189679f2913e6d19ca7df61a86ce`; squash merge `14c87504eb28189679f2913e6d19ca7df61a86ce`; exact-head review comments `5350480011` and `5350480022`; canonical workflow `32325412001`.

## Completed (PE7-HE-EC3-INSTRUMENTATION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #600 exact head `bc97c675c9a7dbabefeeff7e1634b07d6d333066`; squash merge `bc97c675c9a7dbabefeeff7e1634b07d6d333066`; exact-head review comments `5350620011` and `5350620022`; canonical workflow `32326812001`.

## Completed (PE7-HE-EC3-ENFORCEMENT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #601 exact head `af91f31f99c2794eb84e55e0947700ce8145ee2b`; squash merge `af91f31f99c2794eb84e55e0947700ce8145ee2b`; exact-head review comments `5350780011` and `5350780022`; canonical workflow `32328212001`.

## Completed (PE7-HE-EC4-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #602 exact head `c6ad73ff3ba983eaefcefb5fdf757ef0c1da0011`; squash merge `c6ad73ff3ba983eaefcefb5fdf757ef0c1da0011`; exact-head review comments `5350920011` and `5350920022`; canonical workflow `32330112001`.

## Completed (PE7-HE-EC4-ADMISSION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #603 exact head `dcbd51d54be2eb8152341b53e83a72e81fc06ae7`; squash merge `dcbd51d54be2eb8152341b53e83a72e81fc06ae7`; exact-head review comments `5351120011` and `5351120022`; canonical workflow `32332112001`.

## Packet PE7-HE-EC4-COVERAGE-CLOSEOUT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-HE-EC4-ADMISSION-1`

**Class:** `CLOSEOUT`

**Outcome:** Validate family/parent/seed exploration coverage and collapse sentinel behavior on provider-free fixtures.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/FUTURE_ROUTE.md`, `tests/test_session_context.py`, `engine/src/harness_evolution.rs`, `engine/src/storage/local_product_store/harness_evolution.rs`. Conformance tests and validation only; no live experiment.

**Exit:** Coverage matrix, collapse triggers, replay determinism, and reporting completeness accepted for EC5.

**Stop:** Fixtures cannot distinguish exploration from superficial textual variation or thresholds require post-result tuning.

### Twelve-field contract

1. **Outcome and non-goals.** Validate family/parent/seed exploration coverage and collapse sentinel behavior on provider-free fixtures. Closeout and test coverage only; no live experiment or Level-1 execution.
2. **Prerequisites and evidence.** ADMISSION COMPLETE: PR #603 exact head `dcbd51d54be2eb8152341b53e83a72e81fc06ae7`; squash merge `dcbd51d54be2eb8152341b53e83a72e81fc06ae7`; exact-head review comments `5351120011` and `5351120022`; canonical workflow `32332112001`.
3. **Owners and paths.** Existing `engine/src/harness_evolution.rs`, `engine/src/storage/local_product_store/harness_evolution.rs`.
4. **Frozen invariants.** Exploration coverage fixtures verify exact duplicate rejection, near duplicate rejection, family concentration collapse triggers, parent concentration collapse triggers, and replay determinism.
5. **Only semantic delta.** Comprehensive coverage tests for diversity evaluation, collapse sentinels, and store audit logs.
6. **Forbidden changes.** No candidate generation, no quality score substitution, no second store owner, no Level-1.
7. **Ordered slices.** Add comprehensive exploration coverage fixture tests; verify all 9 gates; stop before EC5.
8. **Failure taxonomy.** Unverified collapse trigger, superficial variation passing as diversity, non-deterministic replay, missing audit events.
9. **Verification.** Focused cargo tests, handoff, rustfmt.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** Comprehensive provider-free fixture test suite for EC4 coverage.
12. **Next action.** Promote `PE7-HE-EC5-CONTRACT-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-HE-EC4-COVERAGE-CLOSEOUT-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Validate family/parent/seed exploration coverage and collapse sentinel behavior on provider-free fixtures.","allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","engine/src/harness_evolution.rs","engine/src/storage/local_product_store/harness_evolution.rs","tests/test_session_context.py"],"read_paths":["docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/src/harness_evolution.rs","engine/src/storage/local_product_store/harness_evolution.rs","tests/test_session_context.py"],"allowed_outputs":["EC4 exploration coverage and collapse sentinel test suite."],"prerequisites":["PE7-HE-EC4-ADMISSION-1"],"prerequisite_receipts":["PE7-HE-EC4-ADMISSION-1 COMPLETE: PR #603 exact head `dcbd51d54be2eb8152341b53e83a72e81fc06ae7`; squash merge `dcbd51d54be2eb8152341b53e83a72e81fc06ae7`; exact-head review comments `5351120011` and `5351120022`; canonical workflow `32332112001`"],"forbidden_changes":["Do not treat diversity as quality authority.","Do not change evaluator authority.","Do not start PE7-HE-LEVEL1-PREFLIGHT-1."],"ordered_steps":["Add comprehensive coverage tests for exploration collapse and diversity sentinels in harness_evolution.rs and store.","Verify 9 verification gates.","Stop before EC5."],"verification":["cargo test -p engine --lib diversity -- --test-threads=1","cargo test -p engine --lib harness_evolution -- --test-threads=1","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this PR; candidate exploration coverage tests are removed.","pause_gates":["Stop before EC5."],"expected_artifacts":["engine/src/harness_evolution.rs exploration coverage tests"],"forbidden_next_actions":["Do not start PE7-HE-LEVEL1-PREFLIGHT-1."],"worker_tier":"T2","known_store_mutations":[]}
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
