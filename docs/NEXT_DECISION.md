# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-CWS-PROJECTOR-CORE-1` is complete. `PE7-RWE-CR-RUN-1` remains a retained live-ready blocker. The current window is `PE7-CWS-TOOL-RESULT-REDUCTION-1`: deterministic admission reduction for large tool results. Disposition is `REIMPLEMENT`. No Provider, store table, or RUN-1.

## Authoritative Forward Order

```text
[window: PE7-CWS-TOOL-RESULT-REDUCTION-1 — READY_FOR_EXECUTION, provider-free; tool-result reducer]


```

## Active Routing

1. `PE7-CWS-TOOL-RESULT-REDUCTION-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-REHYDRATION-CONTRACT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #581 exact head `b7b4037bd31731e1ba0f16904006d38bf4c78b82`; squash merge `1b6d73fce72cb195578ae5af784203f7de274e9f`.

## Completed (PE7-CWS-PROJECTOR-CORE-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #582 exact head `cdcd41655aa098b46cdf7d2ee12031d1860e71c2`; squash merge `07446ffe1cb31e49ace25e36deb6233433a3814e`; exact-head review comments `5345989055` and `5345989325`; canonical workflow `32284433657`; exact-head check `32284433705`.

## Packet PE7-CWS-TOOL-RESULT-REDUCTION-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CWS-PROJECTOR-CORE-1`

**Class:** `IMPLEMENT`

**Outcome:** Deterministic admission reduction so raw tool evidence stays with existing artifact owners while the model sees bounded status, diagnostics, and a rehydration handle. Disposition: `REIMPLEMENT`.

**Allowed delta:** `engine/src/context_working_set.rs`, `engine/src/workflow/context_pack/assembly.rs`, `engine/src/workflow/context_pack/mod.rs`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`.

**Exit:** Failure/unknown cannot become success; failure diagnostics survive truncation; secrets are redacted; handles rehydrate the raw artifact hash.

**Stop:** Hidden failure, unbound evidence, second tool runtime, or treating INGRESS `candidate_status` as TRANSPLANT.

### Twelve-field contract

1. **Outcome and non-goals.** Reducer only. No repository/runtime integration, Provider, or RUN-1.
2. **Prerequisites and evidence.** Projector-core COMPLETE on `07446ffe`.
3. **Owners and paths.** Derived CWS module; `context_pack` remains the consumer; redaction reuses the existing provider redaction owner.
4. **Frozen invariants.** Raw bytes are not a new store. Failure/unknown stay failure/unknown.
5. **Only semantic delta.** `reduce_tool_result` plus owner adapter.
6. **Forbidden changes.** No schema, Provider POST, tool retry policy, or harvest TRANSPLANT.
7. **Ordered slices.** Record `REIMPLEMENT`; implement reducer; wire adapter; tests.
8. **Failure taxonomy.** Stale/unbound/`blocker_dropped` fail closed.
9. **Verification.** `cargo test -p engine --lib context_working_set`, handoff, security baseline, rustfmt, diff check.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** Reducer in `engine/src/context_working_set.rs`.
12. **Next action.** Promote `PE7-CWS-REPOSITORY-INTEGRATION-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-CWS-TOOL-RESULT-REDUCTION-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Implement deterministic tool-result admission reduction with REIMPLEMENT disposition.","allowed_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md","engine/src/provider/redaction.rs"],"allowed_outputs":["A reducer that never promotes failure/unknown to success and always binds a raw-artifact rehydration handle."],"prerequisites":["PE7-CWS-PROJECTOR-CORE-1"],"prerequisite_receipts":["PE7-CWS-PROJECTOR-CORE-1 COMPLETE: PR #582 exact head `cdcd41655aa098b46cdf7d2ee12031d1860e71c2`; squash merge `07446ffe1cb31e49ace25e36deb6233433a3814e`; exact-head review comments `5345989055` and `5345989325`; canonical workflow `32284433657`; exact-head check `32284433705`"],"forbidden_changes":["Do not treat INGRESS candidate_status as TRANSPLANT.","Do not convert failure or unknown to success.","Do not add a store table or Provider call.","Do not start PE7-RWE-CR-RUN-1."],"ordered_steps":["Record REIMPLEMENT.","Implement reduce_tool_result.","Wire context_pack adapter.","Pass focused tests."],"verification":["cargo test -p engine --lib context_working_set -- --test-threads=1","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this documentation and engine PR; raw artifacts remain with existing owners and no Provider or Store mutation is introduced.","pause_gates":["Stop if reduction would drop failure diagnostics.","Stop before Provider or RUN-1."],"expected_artifacts":["engine/src/context_working_set.rs reducer"],"forbidden_next_actions":["Do not start PE7-RWE-CR-RUN-1."],"worker_tier":"T1","known_store_mutations":[]}
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
