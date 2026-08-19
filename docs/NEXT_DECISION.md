# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-CWS-RUNTIME-INTEGRATION-1` is complete. `PE7-RWE-CR-RUN-1` remains a retained live-ready blocker. The current window is `PE7-CWS-CACHE-PARTITION-1`: stable prefix vs dynamic working set. Cache telemetry cannot enter correctness. Disposition is `REIMPLEMENT`. No Provider POST or RUN-1.

## Authoritative Forward Order

```text
[window: PE7-CWS-CACHE-PARTITION-1 — READY_FOR_EXECUTION, provider-free; cache partition metadata]


```

## Active Routing

1. `PE7-CWS-CACHE-PARTITION-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-REPOSITORY-INTEGRATION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #584 exact head `323d479d73f26f280cf28502e3c609d4baf78298`; squash merge `d33d7d04709575d1f6fb9fdbe94169175a261108`.

## Completed (PE7-CWS-RUNTIME-INTEGRATION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #585 exact head `7cbe7a0f3660468862302075f024b627a26a0a2e`; squash merge `1dffbc4271a68aebce93a540e7a5793eacefa546`; exact-head review comments `5346942802` and `5346943054`; canonical workflow `32292746487`; exact-head check `32292746231`.

## Packet PE7-CWS-CACHE-PARTITION-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CWS-RUNTIME-INTEGRATION-1`

**Class:** `IMPLEMENT`

**Outcome:** Deterministic stable-prefix versus dynamic partition and optional cache telemetry. Cache presence, hit rate, and writes are not correctness.

**Allowed delta:** `engine/src/context_working_set.rs`, `engine/src/workflow/context_pack/assembly.rs`, `engine/src/workflow/context_pack/mod.rs`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`.

**Exit:** Prefix-stability, mutation invalidation, missing telemetry, replay, and retry-independence tests pass.

**Stop:** Cache identity authorizes work; missing telemetry coerced to zero; dynamic evidence in the stable prefix; Provider POST; RUN-1.

### Twelve-field contract

1. **Outcome and non-goals.** Partition metadata only. No cache service, benchmark protocol, or RUN-1.
2. **Prerequisites and evidence.** Runtime integration COMPLETE on `1dffbc42`.
3. **Owners and paths.** Derived CWS module; `context_pack` consumer.
4. **Frozen invariants.** Digests ignore telemetry. PINNED-only prefix.
5. **Only semantic delta.** `partition_working_set` plus adapter.
6. **Forbidden changes.** No cache authority, invented usage zeros, harvest TRANSPLANT.
7. **Ordered slices.** Record `REIMPLEMENT`; implement partition; tests.
8. **Failure taxonomy.** `prefix_contaminated` fail closed.
9. **Verification.** Focused cargo tests, handoff, security, rustfmt.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** Cache partition types in `engine/src/context_working_set.rs`.
12. **Next action.** Promote `PE7-CWS-BENCHMARK-PROTOCOL-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-CWS-CACHE-PARTITION-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Expose deterministic stable-prefix vs dynamic partition metadata without making cache state correctness.","allowed_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md"],"allowed_outputs":["A partition whose digests are independent of cache telemetry and that refuses authority in the dynamic set."],"prerequisites":["PE7-CWS-RUNTIME-INTEGRATION-1"],"prerequisite_receipts":["PE7-CWS-RUNTIME-INTEGRATION-1 COMPLETE: PR #585 exact head `7cbe7a0f3660468862302075f024b627a26a0a2e`; squash merge `1dffbc4271a68aebce93a540e7a5793eacefa546`; exact-head review comments `5346942802` and `5346943054`; canonical workflow `32292746487`; exact-head check `32292746231`"],"forbidden_changes":["Do not coerce missing cache telemetry to zero.","Do not let cache hits change partition digests.","Do not start PE7-RWE-CR-RUN-1."],"ordered_steps":["Record REIMPLEMENT.","Implement partition_working_set.","Wire context_pack adapter.","Pass replay and missingness tests."],"verification":["cargo test -p engine --lib context_working_set -- --test-threads=1","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this documentation and partition PR; no cache service, Provider POST, or Store mutation is introduced.","pause_gates":["Stop if cache state would enter correctness.","Stop before Provider POST or RUN-1."],"expected_artifacts":["partition_working_set"],"forbidden_next_actions":["Do not start PE7-RWE-CR-RUN-1."],"worker_tier":"T1","known_store_mutations":[]}
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
