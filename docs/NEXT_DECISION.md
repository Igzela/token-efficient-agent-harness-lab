# Next Decision

Last updated: 2026-08-25.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

The Harness-Evolution C0 loop is closed: `PE7-HE-CL0-PILOT-1` executed as the authorized finite artifact-only effect and `PE7-HE-CL0-CLOSEOUT-1` independently classified it `READY_FOR_BOUNDED_LOCAL_USE`, freezing the C1 baseline identity (engine-managed harness x deepseek-v4-pro x single-pass plan/implement/review). EC3 lifecycle-cost instrumentation plus v38 exact-once terminal reconciliation are accepted on `main`. The current window is `PE7-HE-MX1-CONTRACT-1`: freeze Harness/Model/Strategy descriptors, admission and comparability rules, and the staged experiment contract. Provider-free; no candidate generation or holdout access.

## Authoritative Forward Order

```text
[completed: PE7-HE-CL0-CLOSEOUT-1 — COMPLETE, provider-free evidence review; READY_FOR_BOUNDED_LOCAL_USE disposition and frozen C1 baseline identity]
[window: PE7-HE-MX1-CONTRACT-1 — READY_FOR_EXECUTION, provider-free; freeze three-axis descriptors, admission/comparability rules, and staged ladder contract]
```

## Active Routing

1. `PE7-HE-MX1-CONTRACT-1` — `READY_FOR_EXECUTION` (provider-free contract freeze)

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-HE-CL0-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #614 exact head `c464235313255b224c481214be16f7a24831e379`; merge `075f995b574fb8a28f08986291751152bf158dd5`; exact-head `PASS`; canonical workflow `32812721310`.

## Packet PE7-HE-MX1-CONTRACT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-HE-CL0-CLOSEOUT-1` — COMPLETE with disposition `READY_FOR_BOUNDED_LOCAL_USE`; dossier bound to reviewed terminal receipt `0b85bac66a61a8565bd7be238471c0551cd607f3c801dd1c785af2c232e51f25`.

**Class:** `CONTRACT`

**Outcome:** Freeze HarnessImplementation, ModelPlan, and StrategyPlan descriptors, admission/comparability rules and `INCOMPARABLE` semantics, and the staged `1x2x1 -> 1x2x3 -> 2x2x3` ladder with hard-gate-first analysis, binding the frozen C0 baseline identity (engine-managed harness x deepseek-v4-pro x single-pass plan/implement/review) as arm zero.

**Allowed delta:** `docs/NEXT_DECISION.md`, `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, and `tests/test_session_context.py`; contract freeze and route projection through existing documentation and route owners; no second Harness implementation, variant run, provider effect, or policy exception.

**Exit:** Candidate Harness exact commit/version, license/SBOM/provenance audit slots, exact arm identities, common task/evaluator/budget/value basis, randomization/seeds, drift/missingness rules, early-stop rules, main/interaction estimands, `INCOMPARABLE` semantics, and the frozen C0 baseline binding are frozen before any outcome exists.

**Stop:** A CLI name substitutes for full Harness identity, cross-product support is assumed, variants differ on unregistered factors, current admission guardrails are bypassed, or scalar efficiency overrides delivery/safety gates.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze the three-axis descriptor/admission/comparability contract and staged ladder; no candidate generation, holdout access, provider call, target write, live effect, or successor implementation.
2. **Prerequisites and evidence.** CL0 pilot executed and closed 2026-08-24 (task `ptask-20260824115348-18cebba70936f600`, terminal receipt `0b85bac66a61a8565bd7be238471c0551cd607f3c801dd1c785af2c232e51f25`, cost 0.0023360868 USD); closeout disposition `READY_FOR_BOUNDED_LOCAL_USE` recorded in `docs/NEXT_DECISION.md`.
3. **Owners and paths.** Existing documentation, route, session-context, and handoff-check owners remain authoritative; paths are capsule-bound; no new owner.
4. **Frozen invariants.** The frozen C0 baseline identity binds as arm zero; descriptors carry exact commit/version/provenance; unsupported cells stay `INCOMPARABLE`; projections never become truth, persistence, evaluator, or authority.
5. **Only semantic delta.** One contract document set freezing descriptors, admission inventory, ladder design, and analysis rules; aligned route tests.
6. **Forbidden changes.** No CLI-first identity, unregistered-factor variance, admission bypass, scalar-efficiency override, second runtime/store/evaluator owner, or start of `PE7-HE-MX1-CORE-1`.
7. **Ordered work cards.** Bind C0 baseline as arm zero; freeze descriptors and admission rules; freeze ladder and stop rules; freeze analysis estimands and `INCOMPARABLE` semantics; update route projection and tests; run checks; stop.
8. **Failure taxonomy.** Descriptor ambiguity, identity drift, missing provenance slot, guardrail weakening, route-manifest drift, handoff or security check failure, rollback refusal.
9. **Verification.** `git diff --check`, `scripts/check_agent_handoff.py`, `tools/check_security_baseline.py`, aligned unit tests, exact-head review, canonical CI.
10. **Compatibility and rollback.** Documentation-only delta over existing owners; revert commits; no durable store or runtime state is created.
11. **Exit artifact.** Frozen three-axis contract joined to exact identities, updated canonical documents, and aligned route tests.
12. **Next action.** Governed PR, exact-head review/CI, manual merge, receipts sync, then promote `PE7-HE-MX1-CORE-1`; do not start any variant effect here.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-HE-MX1-CONTRACT-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Freeze three-axis descriptors, admission/comparability rules, INCOMPARABLE semantics, and the staged 1x2x1 -> 1x2x3 -> 2x2x3 ladder contract, binding the frozen C0 baseline identity as arm zero.","allowed_paths":["docs/NEXT_DECISION.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","tests/test_session_context.py"],"read_paths":["docs/NEXT_DECISION.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/ARCHITECTURE_BOOK.md","docs/RUNBOOK.md","tests/test_session_context.py"],"allowed_outputs":["Frozen three-axis descriptor/admission/comparability contract joined to exact identities.","Staged ladder design with hard-gate-first analysis and INCOMPARABLE semantics.","Route projection updates and aligned route tests."],"prerequisites":["PE7-HE-CL0-CLOSEOUT-1"],"prerequisite_receipts":["PE7-HE-CL0-CLOSEOUT-1 COMPLETE: PR #614 exact head `c464235313255b224c481214be16f7a24831e379`; merge `075f995b574fb8a28f08986291751152bf158dd5`; exact-head `PASS`; canonical workflow `32812721310`"],"forbidden_changes":["No Provider call, target write, release, deployment, or live effect.","No bypass or second admission, spend, scheduler, runtime, evaluator, Store, audit, or rollback owner.","Do not start PE7-HE-MX1-CORE-1 or any variant/effect work."],"ordered_steps":["Bind the frozen C0 baseline identity as arm zero.","Freeze descriptors and admission/comparability rules.","Freeze the staged ladder with hard-gate-first analysis and stop rules.","Update route projection and aligned tests, run handoff checks, and stop."],"verification":["git diff --check","uv run --no-project python scripts/check_agent_handoff.py","uv run --no-project python tools/check_security_baseline.py"],"rollback":"Revert documentation commits; no durable store or runtime state is created.","pause_gates":["Stop on authority or identity drift, descriptor ambiguity, or admission guardrail weakening.","DECISION_REQUIRED if descriptors cannot be frozen without a second owner."],"expected_artifacts":["Frozen three-axis contract joined to exact identities.","Updated canonical documents and aligned route tests."],"forbidden_next_actions":["Do not start PE7-HE-MX1-CORE-1.","Do not start any provider-backed effect or successor stage."],"worker_tier":"T2"}
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
