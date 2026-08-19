# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-CWS-BENCHMARK-PROTOCOL-1` is complete. `PE7-RWE-CR-RUN-1` remains a retained live-ready blocker. The current window is `PE7-CWS-BENCHMARK-PREFLIGHT-1`: bind reconstructable identities and one unissued T3 authorization package. No Provider POST, no comparison run.

## Authoritative Forward Order

```text
[window: PE7-CWS-BENCHMARK-PREFLIGHT-1 — READY_FOR_EXECUTION, provider-free; unissued CWS authorization]


```

## Active Routing

1. `PE7-CWS-BENCHMARK-PREFLIGHT-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-CACHE-PARTITION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #586 exact head `ecb1367a26d56a633902f0685b3d13d02efff9b4`; squash merge `5a3929dc97b0a94bcec0a95b6e77450238d437da`.

## Completed (PE7-CWS-BENCHMARK-PROTOCOL-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #587 exact head `fe9372732559ffab61b7e98fb81c578cd61bd3fc`; squash merge `f561089103a4a6e51b47f38d6640054ec8a660d0`; exact-head review comments `5347317462` and `5347317666`; canonical workflow `32296178643`; exact-head check `32296178560`.

## Packet PE7-CWS-BENCHMARK-PREFLIGHT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CWS-BENCHMARK-PROTOCOL-1`

**Class:** `CONTRACT`

**Outcome:** Bind baseline/CWS identities, cache observation semantics, evidence destinations, and one unissued finite T3 authorization package without executing the comparison.

**Allowed delta:** `engine/src/context_working_set.rs`, `engine/src/workflow/context_pack/assembly.rs`, `engine/src/workflow/context_pack/mod.rs`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`.

**Exit:** Reconstructable protocol-main identity, unissued package count 1, `authorizations_issued=false`, fail-closed when capability or evidence paths are unverified.

**Stop:** Provider POST; issuing the package; comparison run; RUN-1.

### Twelve-field contract

1. **Outcome and non-goals.** Preflight bind only. No comparison execution or Provider request.
2. **Prerequisites and evidence.** Protocol COMPLETE on `f5610891`.
3. **Owners and paths.** Derived CWS module plus `context_pack` consumer.
4. **Frozen invariants.** Protocol main SHA; toggles off/on; cache telemetry not required.
5. **Only semantic delta.** `cws_benchmark_preflight`.
6. **Forbidden changes.** No Store mutation, issued authorization, or live Provider.
7. **Ordered slices.** Bind identities; keep package unissued; tests.
8. **Failure taxonomy.** Invalid head `binding_invalid`; unverified capability/evidence keep `ready=false`.
9. **Verification.** Focused cargo tests, handoff, security, rustfmt.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** `cws_benchmark_preflight` plus protocol-main binding.
12. **Next action.** Promote `PE7-CWS-BENCHMARK-RUN-1` only when a ready preflight exists and T3 is due.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-CWS-BENCHMARK-PREFLIGHT-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Bind reconstructable CWS benchmark identities and one unissued T3 authorization package without executing the comparison.","allowed_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md"],"allowed_outputs":["A preflight report with authorizations_issued=false and one unissued package."],"prerequisites":["PE7-CWS-BENCHMARK-PROTOCOL-1"],"prerequisite_receipts":["PE7-CWS-BENCHMARK-PROTOCOL-1 COMPLETE: PR #587 exact head `fe9372732559ffab61b7e98fb81c578cd61bd3fc`; squash merge `f561089103a4a6e51b47f38d6640054ec8a660d0`; exact-head review comments `5347317462` and `5347317666`; canonical workflow `32296178643`; exact-head check `32296178560`"],"forbidden_changes":["Do not POST to a Provider.","Do not set authorizations_issued true.","Do not start PE7-CWS-BENCHMARK-RUN-1 in this packet.","Do not start PE7-RWE-CR-RUN-1."],"ordered_steps":["Bind protocol-main identity.","Keep one unissued T3 package.","Fail closed on unverified capability or evidence paths.","Stop before the comparison run."],"verification":["cargo test -p engine --lib context_working_set -- --test-threads=1","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this documentation and preflight PR; no Provider request is issued and no Store mutation is introduced.","pause_gates":["Stop before issuing authorization.","Stop before a live Provider call."],"expected_artifacts":["cws_benchmark_preflight"],"forbidden_next_actions":["Do not start PE7-RWE-CR-RUN-1."],"worker_tier":"T2","known_store_mutations":[]}
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
