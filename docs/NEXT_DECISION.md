# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-CWS-TOOL-RESULT-REDUCTION-1` is complete. `PE7-RWE-CR-RUN-1` remains a retained live-ready blocker. The current window is `PE7-CWS-REPOSITORY-INTEGRATION-1`: wire the projector into repository-maintenance session/prompt paths. No Provider, store table, or RUN-1.

## Authoritative Forward Order

```text
[window: PE7-CWS-REPOSITORY-INTEGRATION-1 — READY_FOR_EXECUTION, provider-free; repository session projection]


```

## Active Routing

1. `PE7-CWS-REPOSITORY-INTEGRATION-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-PROJECTOR-CORE-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #582 exact head `cdcd41655aa098b46cdf7d2ee12031d1860e71c2`; squash merge `07446ffe1cb31e49ace25e36deb6233433a3814e`.

## Completed (PE7-CWS-TOOL-RESULT-REDUCTION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #583 exact head `bd793a7ea449e96df9576876bc38003d6f295be1`; squash merge `2af00e19463a10a58c44a52587ceb78114b23538`; exact-head review comments `5346266783` and `5346267002`; canonical workflow `32286825170`; exact-head check `32286825482`.

## Packet PE7-CWS-REPOSITORY-INTEGRATION-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CWS-TOOL-RESULT-REDUCTION-1`

**Class:** `IMPLEMENT`

**Outcome:** Integrate the projector with repository-maintenance session/prompt construction so canonical context is bound by identity/hash and not repeatedly re-expanded.

**Allowed delta:** `engine/src/context_working_set.rs`, `engine/src/workflow/context_pack/assembly.rs`, `engine/src/workflow/context_pack/mod.rs`, `scripts/agent-control/prompt_builder.py`, `tests/test_cws_repository_projection.py`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`.

**Exit:** Fresh/resume/repair/review/CI-repair fixtures prove exact main/head/packet bindings, no duplicate canonical-document payload, bounded model-visible context, exact rehydration, unchanged fail-closed decisions.

**Stop:** Capsule becomes authority; changed-head is hidden; second session owner; Provider or RUN-1.

### Twelve-field contract

1. **Outcome and non-goals.** Repository session projection only. No runtime/provider integration or RUN-1.
2. **Prerequisites and evidence.** Tool-result reduction COMPLETE on `2af00e19`.
3. **Owners and paths.** Existing `context_pack` and `prompt_builder` owners; derived CWS module.
4. **Frozen invariants.** Capsules remain non-authoritative. PINNED authority stays PINNED.
5. **Only semantic delta.** `project_repository_session` plus claim-bound CWS handles.
6. **Forbidden changes.** No schema, Provider, route-controller, or checkpoint owner move.
7. **Ordered slices.** Record bindings; implement session projector; wire prompt handles; tests.
8. **Failure taxonomy.** `changed_head` and `binding_invalid` fail closed.
9. **Verification.** Focused cargo and python unittests, handoff, security baseline, rustfmt.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** Session projector plus prompt handles.
12. **Next action.** Promote `PE7-CWS-RUNTIME-INTEGRATION-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-CWS-REPOSITORY-INTEGRATION-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Wire working-set projection into repository-maintenance session and claim-bound prompts without a second context owner.","allowed_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","scripts/agent-control/prompt_builder.py","tests/test_cws_repository_projection.py","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","scripts/agent-control/prompt_builder.py","tests/test_cws_repository_projection.py","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md"],"allowed_outputs":["A repository-session projection that binds accepted main, head, and packet without duplicating canonical documents."],"prerequisites":["PE7-CWS-TOOL-RESULT-REDUCTION-1"],"prerequisite_receipts":["PE7-CWS-TOOL-RESULT-REDUCTION-1 COMPLETE: PR #583 exact head `bd793a7ea449e96df9576876bc38003d6f295be1`; squash merge `2af00e19463a10a58c44a52587ceb78114b23538`; exact-head review comments `5346266783` and `5346267002`; canonical workflow `32286825170`; exact-head check `32286825482`"],"forbidden_changes":["Do not make a capsule authoritative.","Do not hide a changed head.","Do not start PE7-RWE-CR-RUN-1."],"ordered_steps":["Implement project_repository_session.","Wire context_pack and prompt_builder.","Pass fresh/repair/review fixtures."],"verification":["cargo test -p engine --lib context_working_set -- --test-threads=1","PYTHONPATH=src uv run --no-project python -m unittest tests.test_cws_repository_projection","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this documentation and adapter PR; session_context and route authority remain unchanged and no Provider or Store mutation is introduced.","pause_gates":["Stop if a capsule would become authority.","Stop before Provider or RUN-1."],"expected_artifacts":["project_repository_session and cws_session_projection_block"],"forbidden_next_actions":["Do not start PE7-RWE-CR-RUN-1."],"worker_tier":"T1","known_store_mutations":[]}
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
