# Next Decision

Last updated: 2026-08-28.

This document owns one current execution window. Accepted receipts belong in
`docs/CURRENT_STATUS.md`; blocked successors belong in `docs/FUTURE_ROUTE.md`;
live PR, CI, review, ruleset, Issue, and mergeability facts require fresh
GitHub readback.

## Current Direction

The owner-approved Autonomous Steward campaign has completed PR0 baseline
recovery. The current window is PR1 contract work: freeze the
Mission/Stage/WorkCard contract and a read-only compatibility boundary while
the legacy controller remains the sole lifecycle writer. No Provider call,
product effect, release, deployment, production action, target write, or
automatic merge is authorized by this window.

## Authoritative Forward Order

```text
[completed: PE7-AUTONOMOUS-STEWARD-PR0 — COMPLETE, accepted baseline and control-plane recovery]
[window: PE7-AUTONOMOUS-STEWARD-PR1 — READY_FOR_EXECUTION, Mission/Stage/WorkCard contract]
```

## Active Routing

1. `PE7-AUTONOMOUS-STEWARD-PR1` — `READY_FOR_EXECUTION`

## Completed: PR0 baseline receipt

PR #626 exact head `6fd4c05479f3a1512eafb35d374245539df950f9` was independently
reviewed with exact-head `PASS`, passed canonical workflow `33090558938`, and
merged as `3f5a3c305d317e5fa160369ac3965adae4634721`. Refreshed `main` passed
canonical workflow `33091453322` with follow-up monitor `33092381869`. Ruleset
`21661714` is active with the required contexts and no bypass actors; PR #574,
Issue #623, and ledger Issue #383 were reconciled, and both MX1 archive refs
remain at their recorded exact heads. PR0 made no Provider or product effect.

## Packet PE7-AUTONOMOUS-STEWARD-PR1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-AUTONOMOUS-STEWARD-PR0 — COMPLETE.

**Class:** `CONTRACT`

**Outcome:** Freeze `MaintenanceMission`, `Stage`, and `WorkCard` contracts
plus the short-term legacy compatibility boundary without creating a second
writer.

**Allowed delta:** `scripts/agent-control/mission_contract.py`, `scripts/session_context.py`, `scripts/check_agent_handoff.py`, `tests/test_mission_contract.py`, `tests/test_session_context.py`, `tests/test_check_agent_handoff.py`, and `docs/ARCHITECTURE_BOOK.md`. Read-only Mission projection may coexist with the legacy packet route; only the existing legacy controller may write lifecycle state.

**Exit:** Positive and negative contract tests prove digest-bound owner
approval, bounded grants, budgets, stop taxonomy, exact repository identities,
rollback, and rejection of unauthorized or stale proposals; compatibility
readers cannot mutate state and all applicable legacy tests remain green.

**Stop:** A second runtime, scheduler, store, approval, evaluator, budget,
output, audit, rollback, or lifecycle writer appears; user comments become
executable without authenticated digest binding; a Provider/effect is needed;
or the compatibility boundary cannot be proven read-only.

### Twelve-field contract

1. **Outcome and non-goals.** Implement only the provider-free Mission/Stage/
   WorkCard contract and read-only compatibility projection. Do not implement
   the Steward service, worker dispatch, GitHub mutation, merge, Provider
   call, product effect, release, deployment, or automatic merge.
2. **Prerequisites and evidence.** Accepted `main` is
   `3f5a3c305d317e5fa160369ac3965adae4634721`; PR0 is accepted by the exact
   receipt above; campaign owner approval remains bound to digest
   `4b6eacaa4ff58337a02a6a73f458ffb0e4d3cb4e71f256c1024b3dd6205e1d39`.
3. **Owners and paths.** `scripts/agent-control/mission_contract.py` owns the
   new wire contract and validation; `scripts/session_context.py` and
   `scripts/check_agent_handoff.py` own read-only compatibility checks; tests
   own positive/negative coverage; `docs/ARCHITECTURE_BOOK.md` remains the
   single durable architecture and authority owner. The legacy controller
   remains the only lifecycle writer; no parallel documentation owner is
   introduced.
4. **Frozen invariants.** Mission approval is bound to the proposal digest;
   grants never widen the approved scope; budgets are finite; exact repository
   and source identities are required; stop reasons distinguish routine
   recovery from owner decisions; and no untrusted text becomes authority.
5. **Only semantic delta.** Add provider-free schema validation and a
   projection that can read the new contract while preserving legacy packet
   execution and all existing authority boundaries.
6. **Forbidden changes.** No second persistence owner, SQLite journal,
   service, workflow, GitHub write, credential access, Provider/effect action,
   auto-merge, release, deployment, or product-runtime change.
7. **Ordered implementation slices.** Define immutable wire models and
   canonical digest rules; validate owner approval, scope, grants, budgets,
   stops, identities, rollback, and stale proposals; add negative tests;
   expose read-only compatibility checks; document the boundary; run focused
   and legacy verification.
8. **Failure, recovery, and stop taxonomy.** Malformed, stale, unauthorized,
   over-budget, out-of-scope, and incompatible proposals fail closed without
   mutation. Ordinary test or worker failures remain future Steward recovery
   outcomes and do not become owner pauses in this contract. No external
   mutation is attempted, so no mutation replay is introduced.
9. **Verification.**
   `PYTHONPATH=scripts/agent-control uv run --no-project python -m unittest
   tests.test_mission_contract tests.test_session_context`;
   `uv run --no-project python -m unittest tests.test_check_agent_handoff`;
   `uv run --no-project python tools/check_security_baseline.py`;
   `uv run --no-project python scripts/check_agent_handoff.py`; and
   `git diff --check`, plus the applicable full Python control-suite checks.
10. **Compatibility, rollback, and retention.** Revert the bounded contract
    commit to restore the PR0-only route. Retain PR0 receipts, ruleset
    recovery evidence, MX1 archive refs, and all accepted legacy controller
    paths. Never activate a second writer during rollback.
11. **Exit artifact.** Contract source, positive/negative tests, read-only
    compatibility evidence, architecture/autonomy documentation, exact-head
    review, canonical CI, and a refreshed accepted-main receipt.
12. **Next action.** After PR1 is accepted, promote only PR2; do not install
    the Steward service or begin provider/effect work in this window.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["Provider-free MaintenanceMission, Stage, and WorkCard wire contract with canonical digest and fail-closed validation.","Read-only legacy compatibility projection and exact positive/negative contract tests.","Stable ARCHITECTURE_BOOK.md contract boundary with no second lifecycle writer."],"allowed_paths":["docs/ARCHITECTURE_BOOK.md","scripts/agent-control/mission_contract.py","scripts/check_agent_handoff.py","scripts/session_context.py","tests/test_check_agent_handoff.py","tests/test_mission_contract.py","tests/test_session_context.py"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["mission_contract.py schema and digest validation source with read-only compatibility API","test_mission_contract.py positive and negative Mission/Stage/WorkCard coverage","updated session and handoff checks proving legacy-only lifecycle writes","ARCHITECTURE_BOOK.md contract documentation update with no parallel owner"],"external_effect_limit":0,"forbidden_changes":["Do not implement the Steward service or create a second lifecycle writer.","Do not call a Provider or execute a product, target, release, deployment, or destructive effect.","Do not mutate GitHub, enable auto-merge, or weaken exact-head, CI, review, credential, or rollback guards."],"forbidden_next_actions":["Do not begin PE7-AUTONOMOUS-STEWARD-PR2 before PR1 is accepted and closed.","Do not dispatch workers, install a service, or create a SQLite Mission journal in PR1.","Do not resume parked MX1 Provider work or consume external-effect authority."],"goal":"Freeze the provider-free Mission, Stage, and WorkCard contract and preserve the legacy controller as the sole lifecycle writer.","ordered_steps":["Define canonical Mission, Stage, and WorkCard wire models.","Add fail-closed digest, scope, grant, budget, stop, identity, and rollback validation.","Add read-only compatibility checks and positive/negative contract tests.","Update the existing ARCHITECTURE_BOOK.md owner with the minimum contract documentation.","Run focused and legacy verification with no Provider or GitHub mutation."],"known_store_mutations":[],"packet_id":"PE7-AUTONOMOUS-STEWARD-PR1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop if a second writer or authority owner appears.","Stop if proposal digest, exact identity, rollback, or read-only compatibility cannot be proved.","Stop before any Provider, product, target, release, deployment, or destructive effect."],"plan_lane_state":"plan_lane_active","prerequisite_receipts":["PE7-AUTONOMOUS-STEWARD-PR0 COMPLETE: PR #626 exact head `6fd4c05479f3a1512eafb35d374245539df950f9`; merge `3f5a3c305d317e5fa160369ac3965adae4634721`; exact-head `PASS`; canonical workflow `33090558938`; refreshed-main workflow `33091453322`"],"prerequisites":["PE7-AUTONOMOUS-STEWARD-PR0"],"private_paths_allowed":false,"promotion_evidence_sha256":"4b6eacaa4ff58337a02a6a73f458ffb0e4d3cb4e71f256c1024b3dd6205e1d39","read_paths":["AGENTS.md","docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","scripts/agent-control/mission_contract.py","scripts/check_agent_handoff.py","scripts/session_context.py","tests/test_check_agent_handoff.py","tests/test_mission_contract.py","tests/test_session_context.py"],"risk_class":"authority","rollback":"Revert the bounded PR1 contract commit, restore the accepted PR0-only routing, and retain all PR0 ruleset, review, CI, merge, issue, and archive evidence.","route_manifest_sha256":"e54f7d13a2f0f1792c40131282ee6af55a9086b68124ca81bbb3e70a87c7c21c","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["PYTHONPATH=scripts/agent-control uv run --no-project python -m unittest tests.test_mission_contract tests.test_session_context","uv run --no-project python -m unittest tests.test_check_agent_handoff","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"verification_family":"docs_evidence_review","worker_tier":"T2"}
-->

## Common Execution Protocol

- Keep the changing PR Draft while iterating; batch repairs before final
  exact-head review and Ready CI.
- A new head invalidates prior review and CI; a new `main` invalidates stale
  baseline conclusions.
- PR1 is provider-free and read-only with respect to lifecycle state. The
  existing controller remains the only writer until a later canary proves a
  single replacement writer.
- GitHub API ambiguity, if encountered in later windows, requires readback;
  `OUTCOME_UNKNOWN` is never treated as success or retried blindly.

## Hard Stops

- `DECISION_REQUIRED` on conflicting owner direction, unprovable contract or
  identity, missing rollback, secret exposure, or unknown external mutation.
- Never weaken exact-head review, canonical CI, expected-head merge,
  credential, effect, target, release, deployment, recovery, or single-writer
  boundaries.
- Never treat the plan, archive refs, branch-local prose, fixture evidence, or
  worker self-report as accepted capability.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` contains only blocked PR2-PR7 routing. Promotion requires
the refreshed accepted PR1 evidence and a new exact dispatch capsule.
