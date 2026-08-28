# Next Decision

Last updated: 2026-08-28.

This document owns one current execution window. Accepted receipts belong in
`docs/CURRENT_STATUS.md`; blocked successors belong in `docs/FUTURE_ROUTE.md`;
live PR, CI, review, ruleset, Issue, and mergeability facts require fresh
GitHub readback.

## Current Direction

The owner-approved Autonomous Steward campaign has completed PR0 baseline
recovery, PR1 contract freeze, and PR2 Shadow Steward acceptance. The current
window is PR3 provider-free autonomous execution: coordinate an isolated
Steward service, a rebuildable journal, reconciliation, WorkCard execution,
path locking, bounded concurrency, and repair/review integration until a
verified waiting-for-merge state. Automatic merge, Provider calls, product or
target effects, release, deployment, and destructive operations are not
authorized by this window.

## Authoritative Forward Order

```text
[completed: PE7-AUTONOMOUS-STEWARD-PR0 — COMPLETE, accepted baseline and control-plane recovery]
[completed: PE7-AUTONOMOUS-STEWARD-PR1 — COMPLETE, Mission/Stage/WorkCard contract and read-only compatibility boundary]
[completed: PE7-AUTONOMOUS-STEWARD-PR2 — COMPLETE, provider-free read-only Shadow Steward]
[window: PE7-AUTONOMOUS-STEWARD-PR3 — READY_FOR_EXECUTION, provider-free autonomous executor]
```

## Active Routing

1. `PE7-AUTONOMOUS-STEWARD-PR3` — `READY_FOR_EXECUTION`

## Completed (PE7-AUTONOMOUS-STEWARD-PR2)

**Historical state:** accepted on `main`; PR2 is complete and its Shadow
Steward is the prerequisite for the current autonomous executor window.

**Historical evidence:** PR #631 exact head
`62032137c1127d56d2dcb865a90efc7cbe412b6b`, exact-head `PASS`, canonical
workflow `33136649992`, merged as
`daaf91e824e044683d9c2de6d024e429faf71ff6`. The provider-free Shadow Steward
is a read-only projection and recommendation boundary; it does not write
lifecycle state or perform Provider, product, target, release, deployment, or
destructive effects.

## Packet PE7-AUTONOMOUS-STEWARD-PR3

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-AUTONOMOUS-STEWARD-PR2` — COMPLETE on accepted main
`daaf91e824e044683d9c2de6d024e429faf71ff6`.

**Class:** `IMPLEMENT`

**Outcome:** Add the provider-free Steward service, rebuildable durable
journal, heartbeat and reconciliation, isolated WorkCard execution, path
locking, bounded concurrency, repair/review loop, and Stage PR integration
with automatic merge disabled.

**Allowed delta:** `scripts/agent-control/steward.py`, `scripts/agent-control/steward_journal.py`, `scripts/agent-control/steward_workers.py`, `scripts/agent-control/steward_github.py`, `scripts/agent-control/steward_service.py`, `scripts/agent-control/steward.service`, `scripts/agent-control/local_verification.py`, `scripts/agent-control/worktree_manager.py`, `scripts/agent-control/review_loop/journal.py`, `scripts/agent-control/review_loop/locking.py`, `scripts/agent-control/review_loop/github_adapter.py`, `scripts/agent-control/review_loop/state_machine.py`, `tests/test_agent_steward.py`, `tests/test_agent_steward_journal.py`, `tests/test_agent_steward_faults.py`, `docs/ARCHITECTURE_BOOK.md`, `docs/MODULE_MAP.md`, and `docs/RUNBOOK.md`.
The service journal is a rebuildable operator projection only; it must not
become a second product store, scheduler, evaluator, budget, approval,
output, audit, or rollback owner.

**Exit:** One approved provider-free Mission reaches a verified
waiting-for-merge state after crash/restart testing without duplicate
repository or PR mutations, overlapping-path writes, credential leakage,
unreconciled intent, or self-review.

**Stop:** The Steward becomes a second product runtime/store or lifecycle
authority, a child session receives reusable write credentials, mutation
intent cannot be reconciled, restart safety is unproved, or any forbidden
Provider/product/target/release/deployment/destructive effect is required.

### Twelve-field contract

1. **Outcome and non-goals.** Implement only provider-free repository
   maintenance orchestration, journaled recovery, reconciliation, isolated
   WorkCard execution, path locking, bounded K=2 concurrency, bounded retry
   and model-tier selection, Stage PR integration, and repair/review routing.
   Do not enable automatic merge, call a Provider, or implement product,
   target, release, deployment, or destructive effects.
2. **Prerequisites and evidence.** Accepted `main` is
   `daaf91e824e044683d9c2de6d024e429faf71ff6`; PR2 is accepted by the receipt
   above; campaign owner approval remains bound to digest
   `4b6eacaa4ff58337a02a6a73f458ffb0e4d3cb4e71f256c1024b3dd6205e1d39`.
3. **Owners and paths.** `steward.py` owns only the bounded service
   coordinator. The journal is a rebuildable local projection, while existing
   `state_manager.py`, `review_loop`, `worktree_manager.py`, and
   `local_verification.py` retain their state, review, locking, workspace,
   and verification ownership. Rust `engine/` and `LocalProductStore` remain
   the application runtime and persistence authorities; GitHub remains the
   accepted repository queue/receipt authority until a later cutover.
4. **Frozen invariants.** Every WorkCard is bound to exact Mission/Stage,
   base, paths, worktree, and attempt identities. Overlapping paths serialize;
   disjoint paths may use at most K=2. Child sessions receive only an
   allowlisted environment and no reusable GitHub or Provider credential.
   Every state transition is idempotent and reconciled from facts after a
   restart; automatic merge remains disabled.
5. **Only semantic delta.** Add one bounded provider-free executor and its
   recovery/reconciliation projection while reusing the accepted Mission,
   Shadow Steward, worktree, verification, review, CI, and GitHub receipt
   owners.
6. **Forbidden changes.** No second product runtime, scheduler, store,
   evaluator, budget, approval, output, audit, rollback, or lifecycle writer;
   no Provider call, production or target effect, release, deployment,
   destructive operation, credential propagation, or automatic merge.
7. **Ordered implementation slices.** Define the journal schema and
   transition/idempotency keys; implement heartbeat and read-only
   reconciliation; reuse path and worktree locks; dispatch bounded isolated
   WorkCards; add checkpoint/retry/tier routing; integrate Stage PR and
   exact-head CI/review/repair observations; add crash, timeout, path,
   stale-head, and duplicate-intent fault tests; document operator recovery.
8. **Failure, recovery, and stop taxonomy.** Ordinary no-change, timeout,
   bad-output, focused-test, CI, review, and main-drift outcomes remain
   bounded repair or replan results. Path conflict serializes; stale identity
   rehydrates or replans; malformed or unreconciled journal state fails closed.
   GitHub ambiguity, unknown mutation outcome, authority expansion, and any
   forbidden effect pause without blind replay or automatic merge.
9. **Verification.** Run focused Steward/journal/fault tests,
   `python -m unittest discover -s tests -p 'test_agent_*.py'`,
   `python tools/check_security_baseline.py`,
   `uv run --no-project python scripts/check_agent_handoff.py`,
   `git diff --check`, the applicable full Python control-suite checks, and
   the canonical exact-head/CI contract checks. Verify no Provider call or
   forbidden effect transport was invoked.
10. **Compatibility, rollback, and retention.** Revert the bounded PR3
    executor commit to restore the accepted PR2-only route. Retain the PR0,
    PR1, and PR2 receipts, accepted legacy controller paths, GitHub receipts,
    worktrees, and rebuildable journal evidence; never run two lifecycle
    writers and never delete unreconciled evidence.
11. **Exit artifact.** Provider-free service and journal source, isolated
    worker/reviewer adapters, reconciliation and fault tests, operator runbook
    procedure, a verified waiting-for-merge receipt, exact-head independent
    review, canonical CI, and refreshed accepted-main evidence.
12. **Next action.** After PR3 is accepted and closed, promote only PR4 for
    the explicit canary and single-writer cutover; do not enable automatic
    merge or begin Provider/effect work in this window.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["Provider-free autonomous repository-maintenance execution with durable journal, isolated WorkCards, reconciliation, and review/CI evidence.","Read-only status and bounded Draft PR delivery with automatic merge disabled."],"allowed_paths":["scripts/agent-control/steward.py","scripts/agent-control/steward_journal.py","scripts/agent-control/steward_workers.py","scripts/agent-control/steward_github.py","scripts/agent-control/steward_service.py","scripts/agent-control/steward.service","scripts/agent-control/local_verification.py","scripts/agent-control/worktree_manager.py","scripts/agent-control/review_loop/journal.py","scripts/agent-control/review_loop/locking.py","scripts/agent-control/review_loop/github_adapter.py","scripts/agent-control/review_loop/state_machine.py","tests/test_agent_steward.py","tests/test_agent_steward_journal.py","tests/test_agent_steward_faults.py","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md","docs/RUNBOOK.md"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_autonomous_execution","expected_artifacts":["steward.py bounded coordinator with heartbeat and reconciliation entrypoints","steward_journal.py rebuildable transition and idempotency journal with corruption refusal","isolated WorkCard worker/reviewer adapters with K=2 path-lock enforcement","fault tests for crash restart, duplicate intent, path conflict, stale head, timeout, CI/review blocker, and unknown GitHub outcome","RUNBOOK.md operator recovery and automatic-merge-disabled procedure"],"external_effect_limit":0,"forbidden_changes":["Do not create a second product runtime, scheduler, store, evaluator, budget, approval, output, audit, rollback, or lifecycle writer.","Do not call a Provider or propagate GitHub or Provider credentials into child sessions.","Do not enable automatic merge or execute product, target, release, deployment, production, or destructive operations.","Do not bypass exact-head, canonical CI, independent review, reconciliation, path-lock, recovery, or rollback guards."],"forbidden_next_actions":["Do not begin PE7-AUTONOMOUS-STEWARD-PR4 before PR3 is accepted and closed.","Do not enable automatic merge, call a Provider, or perform product, target, release, deployment, or destructive operations in PR3.","Do not introduce a second runtime, scheduler, store, evaluator, budget, approval, output, audit, rollback, or lifecycle writer."],"goal":"Implement one provider-free autonomous executor that safely drives approved repository-maintenance WorkCards to a verified waiting-for-merge state.","ordered_steps":["Define journal schema, transition validation, idempotency keys, and corruption refusal.","Implement heartbeat, read-only reconciliation, and restart recovery from journal plus GitHub facts.","Reuse worktree, path-lock, verification, review, and GitHub receipt owners for isolated WorkCards.","Add bounded K=2 dispatch, checkpoint, retry, model-tier selection, and Stage PR integration with automatic merge disabled.","Run focused positive and negative fault tests, security baseline, handoff, diff, and canonical CI/review verification."],"known_store_mutations":[],"packet_id":"PE7-AUTONOMOUS-STEWARD-PR3","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop if the service journal becomes product or lifecycle authority.","Stop if a child session receives a reusable credential or a WorkCard can bypass exact path, head, review, CI, or rollback binding.","Stop on unreconciled mutation intent, unknown GitHub outcome, automatic merge, Provider call, or forbidden product/target/release/deployment/destructive effect."],"plan_lane_state":"plan_lane_active","prerequisite_receipts":["PE7-AUTONOMOUS-STEWARD-PR2 COMPLETE: PR #631 exact head `62032137c1127d56d2dcb865a90efc7cbe412b6b`; merge `daaf91e824e044683d9c2de6d024e429faf71ff6`; exact-head `PASS`; canonical workflow `33136649992`"],"prerequisites":["PE7-AUTONOMOUS-STEWARD-PR2"],"private_paths_allowed":false,"promotion_evidence_sha256":"4b6eacaa4ff58337a02a6a73f458ffb0e4d3cb4e71f256c1024b3dd6205e1d39","read_paths":["AGENTS.md","START_HERE.md","docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","docs/RUNBOOK.md","scripts/agent-control/mission_contract.py","scripts/agent-control/shadow_steward.py","scripts/agent-control/steward.py","scripts/agent-control/steward_journal.py","scripts/agent-control/steward_workers.py","scripts/agent-control/steward_github.py","scripts/agent-control/steward_service.py","scripts/agent-control/steward.service","scripts/agent-control/local_verification.py","scripts/agent-control/worktree_manager.py","scripts/agent-control/review_loop/journal.py","scripts/agent-control/review_loop/locking.py","scripts/agent-control/review_loop/github_adapter.py","scripts/agent-control/review_loop/state_machine.py","tests/test_agent_steward.py","tests/test_agent_steward_journal.py","tests/test_agent_steward_faults.py","tests/test_agent_shadow_steward.py","tests/test_mission_contract.py","tests/test_agent_route_driver.py","tests/test_agent_local_loop.py","tests/test_agent_control_worktree.py","tests/test_review_loop.py","tests/test_check_agent_handoff.py"],"risk_class":"none","rollback":"Revert the bounded PR3 executor commit, restore the accepted PR2-only route, and retain all PR0/PR1/PR2 receipts, legacy controller paths, GitHub receipts, worktrees, and rebuildable journal evidence.","route_manifest_sha256":"87854724c765d8977ca8cafadfeb260b02e294f74b7b95a74015f1738fd0aa24","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["uv run --no-project python -m unittest tests.test_agent_steward tests.test_agent_steward_journal tests.test_agent_steward_faults","python -m unittest discover -s tests -p 'test_agent_*.py'","python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"verification_family":"source_focused_full","worker_tier":"T1"}
-->

## Common Execution Protocol

- Keep the changing PR Draft while iterating; batch repairs before final
  exact-head review and Ready CI.
- A new head invalidates prior review and CI; a new `main` invalidates stale
  baseline conclusions.
- PR3 is provider-free and bounded to repository maintenance. The service
  journal is a rebuildable projection, existing state/review/worktree/
  verification owners remain canonical, and automatic merge stays disabled.
- GitHub API ambiguity or a mutation with unknown outcome requires readback;
  `OUTCOME_UNKNOWN` is never treated as success or retried blindly.

## Hard Stops

- `DECISION_REQUIRED` on conflicting owner direction, unprovable contract or
  identity, missing rollback, secret exposure, unknown external mutation,
  second-writer activation, or any service journal crossing into authority.
- Never weaken exact-head review, canonical CI, expected-head merge,
  credential, effect, target, release, deployment, recovery, or single-writer
  boundaries.
- Never treat the plan, archive refs, branch-local prose, fixture evidence, or
  worker self-report as accepted capability.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` contains only blocked PR4-PR7 routing. Promotion requires
the refreshed accepted PR3 evidence and a new exact dispatch capsule.
