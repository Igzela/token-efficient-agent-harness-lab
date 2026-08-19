# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-CWS-BENCHMARK-RUN-1` is complete and fail-closed. `PE7-RWE-CR-RUN-1` remains a retained live-ready blocker. The current window is `PE7-CWS-ANALYSIS-1`: `INSUFFICIENT_DEFAULT_OFF`. No invented live arms. No Provider POST.

## Authoritative Forward Order

```text
[window: PE7-CWS-ANALYSIS-1 — READY_FOR_EXECUTION, provider-free; INSUFFICIENT_DEFAULT_OFF]


```

## Active Routing

1. `PE7-CWS-ANALYSIS-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-BENCHMARK-PREFLIGHT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #588 exact head `c806f75c5910b117c3cf7e44ad1c6a6503e48ddd`; squash merge `1569c70e9f2034bb4f7bc5ccbc24d889b66645ab`.

## Completed (PE7-CWS-BENCHMARK-RUN-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #589 exact head `0f9cad12a850a7ed2ffcc823ebd2da29318c5ae6`; squash merge `84b1933bc3d9e657acae94d9e5f14810c0651917`; exact-head review comments `5347630853` and `5347631083`; canonical workflow `32298813456`; exact-head check `32298813444`; `executed=false`; `provider_posts=0`.

## Packet PE7-CWS-ANALYSIS-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CWS-BENCHMARK-RUN-1`

**Class:** `CLOSEOUT`

**Outcome:** Disposition `INSUFFICIENT_DEFAULT_OFF`. Active Harness identity for later HE packets is accepted main `84b1933b` with CWS default-off. No ENABLE.

**Allowed delta:** `engine/src/context_working_set.rs`, `engine/src/workflow/context_pack/assembly.rs`, `engine/src/workflow/context_pack/mod.rs`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`.

**Exit:** Hard gates do not pass without live arms. Identity for EC1 is the default-off baseline. Maintenance burden is not evaluator authority.

**Stop:** ENABLE without live arms; invented usage; Provider POST; Level-1.

### Twelve-field contract

1. **Outcome and non-goals.** Analysis only. No ENABLE, no Provider, no EC1 implementation in this packet.
2. **Prerequisites and evidence.** RUN COMPLETE fail-closed on `84b1933b`.
3. **Owners and paths.** Derived CWS module; `context_pack` consumer.
4. **Frozen invariants.** Missing live arms cannot ENABLE. Default-off identity is `84b1933b`.
5. **Only semantic delta.** `cws_benchmark_analyze`.
6. **Forbidden changes.** No post-hoc reducer, new evaluator, live arm invention.
7. **Ordered slices.** Analyze fail-closed run; bind default-off identity; stop before EC1.
8. **Failure taxonomy.** Missing arms → INSUFFICIENT_DEFAULT_OFF.
9. **Verification.** Focused cargo tests, handoff, security, rustfmt.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** Analysis receipt + default-off Harness SHA.
12. **Next action.** Promote `PE7-HE-EC1-CONTRACT-1` using default-off identity `84b1933b`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-CWS-ANALYSIS-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Record INSUFFICIENT_DEFAULT_OFF from the fail-closed CWS run and bind the default-off Harness identity for EC1.","allowed_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md"],"allowed_outputs":["An analysis receipt with INSUFFICIENT_DEFAULT_OFF and active harness 84b1933b."],"prerequisites":["PE7-CWS-BENCHMARK-RUN-1"],"prerequisite_receipts":["PE7-CWS-BENCHMARK-RUN-1 COMPLETE: PR #589 exact head `0f9cad12a850a7ed2ffcc823ebd2da29318c5ae6`; squash merge `84b1933bc3d9e657acae94d9e5f14810c0651917`; exact-head review comments `5347630853` and `5347631083`; canonical workflow `32298813456`; exact-head check `32298813444`; executed=false; provider_posts=0"],"forbidden_changes":["Do not ENABLE CWS without live arms.","Do not invent arm terminals.","Do not start PE7-HE-LEVEL1-PREFLIGHT-1."],"ordered_steps":["Analyze the fail-closed run.","Bind default-off identity.","Record burden as non-evaluator evidence.","Stop before EC1 implementation."],"verification":["cargo test -p engine --lib fail_closed_run_analyzes -- --test-threads=1","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this PR; CWS remains default-off and no Provider request is issued.","pause_gates":["Stop before ENABLE.","Stop before Level-1."],"expected_artifacts":["cws_benchmark_analyze"],"forbidden_next_actions":["Do not start PE7-HE-LEVEL1-PREFLIGHT-1."],"worker_tier":"T2","known_store_mutations":[]}
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
