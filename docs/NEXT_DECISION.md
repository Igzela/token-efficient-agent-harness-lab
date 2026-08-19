# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-CWS-BENCHMARK-PREFLIGHT-1` is complete. `PE7-RWE-CR-RUN-1` remains a retained live-ready blocker. The current window is `PE7-CWS-BENCHMARK-RUN-1`: fail-closed T3 comparison runner. Authorization is unissued; no Provider POST.

## Authoritative Forward Order

```text
[window: PE7-CWS-BENCHMARK-RUN-1 — READY_FOR_EXECUTION, fail-closed EFFECT; unissued authorization]


```

## Active Routing

1. `PE7-CWS-BENCHMARK-RUN-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-BENCHMARK-PROTOCOL-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #587 exact head `fe9372732559ffab61b7e98fb81c578cd61bd3fc`; squash merge `f561089103a4a6e51b47f38d6640054ec8a660d0`.

## Completed (PE7-CWS-BENCHMARK-PREFLIGHT-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #588 exact head `c806f75c5910b117c3cf7e44ad1c6a6503e48ddd`; squash merge `1569c70e9f2034bb4f7bc5ccbc24d889b66645ab`; exact-head review comments `5347437430` and `5347437722`; canonical workflow `32297108984`; exact-head check `32297109030`.

## Packet PE7-CWS-BENCHMARK-RUN-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CWS-BENCHMARK-PREFLIGHT-1`

**Class:** `EFFECT`

**Outcome:** Execute the frozen comparison only under issued finite authorization. This environment has an unissued package and no Provider credential: the runner fail-closes with zero POSTs.

**Allowed delta:** `engine/src/context_working_set.rs`, `engine/src/workflow/context_pack/assembly.rs`, `engine/src/workflow/context_pack/mod.rs`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`.

**Exit:** Fail-closed report with `executed=false`, `provider_posts=0`, reason `authorization_unissued` or `preflight_not_ready`. No invented arm terminals.

**Stop:** Any Provider POST without issued authorization; fabricated usage; RUN-1 RWE.

### Twelve-field contract

1. **Outcome and non-goals.** Fail-closed runner. No live comparison, no credential creation.
2. **Prerequisites and evidence.** Preflight COMPLETE on `1569c70e`; package unissued; no Provider env credential.
3. **Owners and paths.** Derived CWS module; `context_pack` consumer.
4. **Frozen invariants.** `authorizations_issued` stays false; posts stay 0.
5. **Only semantic delta.** `cws_benchmark_run`.
6. **Forbidden changes.** No HTTP Provider, Store seed/migrate, fake terminals.
7. **Ordered slices.** Implement fail-closed run; tests; stop before ANALYSIS.
8. **Failure taxonomy.** Not-ready / unissued / absent credential / unregistered transport.
9. **Verification.** Focused cargo tests, handoff, security, rustfmt.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** `cws_benchmark_run` report.
12. **Next action.** Promote `PE7-CWS-ANALYSIS-1` with INSUFFICIENT_DEFAULT_OFF from missing live arms.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-CWS-BENCHMARK-RUN-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Fail-close the CWS comparison runner while authorization is unissued and no Provider credential is present.","allowed_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md"],"allowed_outputs":["A run report with executed=false and provider_posts=0."],"prerequisites":["PE7-CWS-BENCHMARK-PREFLIGHT-1"],"prerequisite_receipts":["PE7-CWS-BENCHMARK-PREFLIGHT-1 COMPLETE: PR #588 exact head `c806f75c5910b117c3cf7e44ad1c6a6503e48ddd`; squash merge `1569c70e9f2034bb4f7bc5ccbc24d889b66645ab`; exact-head review comments `5347437430` and `5347437722`; canonical workflow `32297108984`; exact-head check `32297109030`"],"forbidden_changes":["Do not POST to a Provider.","Do not invent arm terminals.","Do not start PE7-RWE-CR-RUN-1."],"ordered_steps":["Call preflight.","Refuse unissued authorization.","Record zero provider posts.","Stop before ANALYSIS."],"verification":["cargo test -p engine --lib cws_run -- --test-threads=1","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this PR; no Provider request is issued and no Store mutation is introduced.","pause_gates":["Stop before any Provider POST.","Stop if authorization would be invented."],"expected_artifacts":["cws_benchmark_run"],"forbidden_next_actions":["Do not start PE7-RWE-CR-RUN-1."],"worker_tier":"T3","known_store_mutations":[]}
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
