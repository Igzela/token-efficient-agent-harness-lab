# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-CWS-REPOSITORY-INTEGRATION-1` is complete. `PE7-RWE-CR-RUN-1` remains a retained live-ready blocker. The current window is `PE7-CWS-RUNTIME-INTEGRATION-1`: bind the same residency projection to runtime/provider prompts without a second owner. No Provider POST, store table, or RUN-1.

## Authoritative Forward Order

```text
[window: PE7-CWS-RUNTIME-INTEGRATION-1 — READY_FOR_EXECUTION, provider-free; runtime prompt composition]


```

## Active Routing

1. `PE7-CWS-RUNTIME-INTEGRATION-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-TOOL-RESULT-REDUCTION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #583 exact head `bd793a7ea449e96df9576876bc38003d6f295be1`; squash merge `2af00e19463a10a58c44a52587ceb78114b23538`.

## Completed (PE7-CWS-REPOSITORY-INTEGRATION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #584 exact head `323d479d73f26f280cf28502e3c609d4baf78298`; squash merge `d33d7d04709575d1f6fb9fdbe94169175a261108`; exact-head review comments `5346743411` and `5346743657`; canonical workflow `32290928328`; exact-head check `32290928230`.

## Packet PE7-CWS-RUNTIME-INTEGRATION-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CWS-REPOSITORY-INTEGRATION-1`

**Class:** `IMPLEMENT`

**Outcome:** Production model requests consume the same source-bound residency semantics. The provider remains an executor, not the context owner.

**Allowed delta:** `engine/src/context_working_set.rs`, `engine/src/workflow/context_pack/assembly.rs`, `engine/src/workflow/context_pack/mod.rs`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`.

**Exit:** Stub/fake provider-free tests prove stable task bindings, bounded dynamic context, exact rehydration handles, cancellation/unknown preservation, and unchanged executor mappings.

**Stop:** Provider becomes context truth; schema/credential/retry change; cache as correctness; outcome remapping; RUN-1.

### Twelve-field contract

1. **Outcome and non-goals.** Runtime prompt composition only. No cache partition, Provider POST, or RUN-1.
2. **Prerequisites and evidence.** Repository integration COMPLETE on `d33d7d04`.
3. **Owners and paths.** Derived CWS module; `context_pack` consumer; existing stub provider used only as a hash oracle.
4. **Frozen invariants.** Provider does not own context. Unknown/cancel stay unknown/cancel.
5. **Only semantic delta.** `compose_runtime_prompt` plus owner adapter.
6. **Forbidden changes.** No credential, retry, schema, or Store mutation.
7. **Ordered slices.** Compose PINNED then dynamic then cold handles; stub replay test.
8. **Failure taxonomy.** Empty task binding fails closed.
9. **Verification.** Focused cargo tests including stub invoke, handoff, security, rustfmt.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** Runtime composer in `engine/src/context_working_set.rs`.
12. **Next action.** Promote `PE7-CWS-CACHE-PARTITION-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-CWS-RUNTIME-INTEGRATION-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Compose provider-free runtime prompts from the accepted working-set projection without replacing the provider owner.","allowed_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["engine/src/context_working_set.rs","engine/src/workflow/context_pack/assembly.rs","engine/src/workflow/context_pack/mod.rs","engine/src/provider/stub.rs","engine/src/provider/mod.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md"],"allowed_outputs":["A runtime prompt composer that preserves PINNED/unknown/cancellation and does not make the provider the context owner."],"prerequisites":["PE7-CWS-REPOSITORY-INTEGRATION-1"],"prerequisite_receipts":["PE7-CWS-REPOSITORY-INTEGRATION-1 COMPLETE: PR #584 exact head `323d479d73f26f280cf28502e3c609d4baf78298`; squash merge `d33d7d04709575d1f6fb9fdbe94169175a261108`; exact-head review comments `5346743411` and `5346743657`; canonical workflow `32290928328`; exact-head check `32290928230`"],"forbidden_changes":["Do not change Provider credentials, retry, or budget.","Do not remap unknown or cancelled outcomes.","Do not start PE7-RWE-CR-RUN-1."],"ordered_steps":["Implement compose_runtime_prompt.","Wire context_pack adapter.","Prove stub replay and unknown/cancel preservation."],"verification":["cargo test -p engine --lib context_working_set -- --test-threads=1","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this documentation and composer PR; provider, scheduler, and Store owners remain unchanged and no Provider POST is introduced.","pause_gates":["Stop if the provider would own context.","Stop before Provider POST or RUN-1."],"expected_artifacts":["compose_runtime_prompt"],"forbidden_next_actions":["Do not start PE7-RWE-CR-RUN-1."],"worker_tier":"T1","known_store_mutations":[]}
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
