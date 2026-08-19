# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-CWS-ANALYSIS-1` is complete with `INSUFFICIENT_DEFAULT_OFF`. The current window is `PE7-HE-EC1-CONTRACT-1`: freeze identity, causal-evidence, and mutation-family schemas on the default-off Harness `84b1933b`. No candidate generation, evaluation, persistence, ENABLE, or Level-1.

## Authoritative Forward Order

```text
[window: PE7-HE-EC1-CONTRACT-1 — READY_FOR_EXECUTION, provider-free; freeze EC1 schemas]


```

## Active Routing

1. `PE7-HE-EC1-CONTRACT-1` — `READY_FOR_EXECUTION`

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

## Packet PE7-HE-EC1-CONTRACT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CWS-ANALYSIS-1`

**Class:** `CONTRACT`

**Outcome:** Freeze active-Harness, candidate, parent, generator, lineage, mutation-family, identity-hash, invalidation, budget, `FailurePatternEvidenceV1`, `MutationHypothesisManifestV1`, and `PredictionOutcomeV1` bindings on default-off SHA `84b1933b`.

**Allowed delta:** `engine/src/harness_evolution.rs`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`. No candidate generation, evaluation, or persistence change.

**Exit:** Exact identity/lineage and causal-evidence schemas plus a pre-registered mutation registry with ownership, redaction, counterevidence, addressability, and non-authority rules.

**Stop:** Identity or cause can be caller/model asserted; lineage can be rewritten; uncertainty cannot be represented; mutation scope can reach evaluator/authority policy; Level-1.

### Twelve-field contract

1. **Outcome and non-goals.** Contract freeze only. No generation, evaluation, persistence, ENABLE, or Level-1.
2. **Prerequisites and evidence.** CWS analysis COMPLETE `INSUFFICIENT_DEFAULT_OFF` on PR #590 / `1544c8d0a3f1b196fdb4b560759609662cd5f432`.
3. **Owners and paths.** Existing `engine/src/harness_evolution.rs`.
4. **Frozen invariants.** Default-off Harness `84b1933b`. Causal `unknown`/`disputed` remain representable. Prediction outcomes are not authority.
5. **Only semantic delta.** EC1 schema types and validators; pre-registered mutation families on admitted surfaces.
6. **Forbidden changes.** No second store/evaluator; no generator; no evaluator/holdout implementation.
7. **Ordered slices.** Freeze schemas; bind default-off identity; register families; stop before identity persistence.
8. **Failure taxonomy.** Empty identity, forbidden surface, empty registry.
9. **Verification.** Focused cargo tests, handoff, security, rustfmt.
10. **Compatibility and rollback.** Revert this PR; laboratory remains default-off.
11. **Exit artifact.** Versioned EC1 types and registry validators.
12. **Next action.** Promote `PE7-HE-EC1-IDENTITY-LINEAGE-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-HE-EC1-CONTRACT-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Freeze EC1 identity, causal-evidence, and mutation-family schemas on default-off Harness 84b1933b.","allowed_paths":["engine/src/harness_evolution.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["engine/src/harness_evolution.rs","engine/src/context_working_set.rs","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md"],"allowed_outputs":["EC1 contract types and validators with a pre-registered mutation family registry."],"prerequisites":["PE7-CWS-ANALYSIS-1"],"prerequisite_receipts":["PE7-CWS-ANALYSIS-1 COMPLETE: PR #590 exact head da09ea576154e55e532d2de5477972f2c5c516d5; squash merge 1544c8d0a3f1b196fdb4b560759609662cd5f432; disposition INSUFFICIENT_DEFAULT_OFF; active Harness 84b1933bc3d9e657acae94d9e5f14810c0651917"],"forbidden_changes":["Do not generate candidates.","Do not change evaluation or persistence.","Do not ENABLE the laboratory.","Do not start PE7-HE-LEVEL1-PREFLIGHT-1."],"ordered_steps":["Freeze FailurePatternEvidenceV1, MutationHypothesisManifestV1, and PredictionOutcomeV1.","Bind default-off active Harness SHA.","Pre-register mutation families on admitted surfaces.","Stop before identity-lineage persistence."],"verification":["cargo test -p engine --lib ec1_contract_freezes_default_off_harness_and_registry -- --test-threads=1","git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this PR; Harness Evolution remains default-off with no candidate generation.","pause_gates":["Stop before identity persistence.","Stop before Level-1."],"expected_artifacts":["FailurePatternEvidenceV1","MutationHypothesisManifestV1","PredictionOutcomeV1","MutationFamilyRegistry"],"forbidden_next_actions":["Do not start PE7-HE-LEVEL1-PREFLIGHT-1."],"worker_tier":"T2","known_store_mutations":[]}
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
