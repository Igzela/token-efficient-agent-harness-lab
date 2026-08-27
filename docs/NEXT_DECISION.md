# Next Decision

Last updated: 2026-08-28.

This document owns one current execution window. Accepted receipts belong in
`docs/CURRENT_STATUS.md`; blocked successors belong in `docs/FUTURE_ROUTE.md`;
live PR, CI, review, ruleset, Issue, and mergeability facts require fresh
GitHub readback.

## Current Direction

The owner-approved Autonomous Steward campaign has completed PR0 baseline
recovery and PR1 contract freeze. The current window is PR2 Shadow Steward
work: compile natural-language proposals, plan and replan Stages and
WorkCards, classify stops, and emit compact status while remaining strictly
read-only. No GitHub mutation, worker dispatch, Provider call, product effect,
release, deployment, target write, service installation, or automatic merge is
authorized by this window.

## Authoritative Forward Order

```text
[completed: PE7-AUTONOMOUS-STEWARD-PR0 — COMPLETE, accepted baseline and control-plane recovery]
[completed: PE7-AUTONOMOUS-STEWARD-PR1 — COMPLETE, Mission/Stage/WorkCard contract and read-only compatibility boundary]
[window: PE7-AUTONOMOUS-STEWARD-PR2 — READY_FOR_EXECUTION, read-only Shadow Steward]
```

## Active Routing

1. `PE7-AUTONOMOUS-STEWARD-PR2` — `READY_FOR_EXECUTION`

## Completed (PE7-AUTONOMOUS-STEWARD-PR1)

**Historical state:** accepted on `main`; PR1 is complete and its contract is
the prerequisite for the current Shadow Steward window.

**Historical evidence:** PR #628 exact head
`07e9080f176c7176f6c55a7b966fac1ea4fa8c1b`, exact-head `PASS`, canonical
workflow `33112792625`, merged as
`1dda023ace9f2bef7ec54ac3fc316693a145dfa7`. The provider-free
Mission/Stage/WorkCard contract and read-only compatibility boundary are
accepted; the legacy controller remains the sole lifecycle writer.

## Packet PE7-AUTONOMOUS-STEWARD-PR2

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-AUTONOMOUS-STEWARD-PR1` — COMPLETE on accepted main
`1dda023ace9f2bef7ec54ac3fc316693a145dfa7`.

**Class:** `IMPLEMENT`

**Outcome:** Implement a read-only Shadow Steward that compiles natural-language
proposals, plans and replans Stages and WorkCards, classifies stops, and emits
compact status without mutating GitHub or repository state.

**Allowed delta:** `scripts/agent-control/shadow_steward.py`,
`tests/test_agent_shadow_steward.py`, `docs/ARCHITECTURE_BOOK.md`,
`docs/MODULE_MAP.md`, `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, and
`docs/NEXT_DECISION.md`. The implementation must remain a projection and
recommendation boundary; it cannot write lifecycle state or become a second
runtime, scheduler, store, evaluator, budget, approval, output, audit, or
rollback owner.

**Exit:** Historical failure replay proves ordinary failures do not pause the
owner; authority expansion, production or destructive requests, and unknown
outcomes pause; and non-owner or digest-mismatched input cannot activate a
Mission. Shadow output is visibly distinct from authority or mutation.

**Stop:** Shadow output is treated as authority, raw prompts/private content are
retained, replay comparison cannot distinguish recommendation from mutation,
or implementation requires a GitHub mutation, Provider call, worker dispatch,
service installation, or external effect.

### Twelve-field contract

1. **Outcome and non-goals.** Implement only provider-free natural-language
   intake compilation, digest-bound proposal/approval evaluation, stage/card
   planning and replanning, stop classification, compact status, and replay
   comparison. Do not implement the Steward service, worker dispatch, GitHub
   mutation, merge, Provider call, product effect, release, deployment, or
   automatic merge.
2. **Prerequisites and evidence.** Accepted `main` is
   `1dda023ace9f2bef7ec54ac3fc316693a145dfa7`; PR1 is accepted by the receipt
   above; campaign owner approval remains bound to digest
   `4b6eacaa4ff58337a02a6a73f458ffb0e4d3cb4e71f256c1024b3dd6205e1d39`.
3. **Owners and paths.** The new shadow module owns pure proposal compilation,
   policy classification, replay comparison, and status projection. Existing
   `mission_contract.py` owns Mission identity, digest, grant, budget, stop,
   and rollback validation. `docs/ARCHITECTURE_BOOK.md` and
   `docs/MODULE_MAP.md` remain the durable architecture and ownership owners;
   the legacy controller remains the only lifecycle writer.
4. **Frozen invariants.** Natural-language text is untrusted input; only an
   authenticated owner approval bound to the exact proposal digest can activate
   a Mission recommendation. Shadow outputs never mint authority, widen scope,
   spend budget, write state, or cause an effect. Ordinary failures are
   recoverable recommendations; authority expansion, destructive/production
   requests, and unknown outcomes are owner-stop recommendations.
5. **Only semantic delta.** Add deterministic, provider-free shadow planning,
   replay, stop classification, and compact projection while preserving the
   accepted Mission contract, legacy packet execution, and all existing
   authority boundaries.
6. **Forbidden changes.** No second persistence owner, journal, service,
   workflow, GitHub write, credential access, Provider/effect action,
   auto-merge, release, deployment, product-runtime change, or raw prompt,
   output, transcript, or private-path retention.
7. **Ordered implementation slices.** Define bounded untrusted intake and
   proposal inputs; reuse Mission contract validation; implement deterministic
   planner and replanner projections; classify routine recovery versus owner
   stops; emit redacted compact status; add historical replay fixtures and
   negative tests; document the read-only boundary; run focused and legacy
   verification.
8. **Failure, recovery, and stop taxonomy.** Malformed, stale, unauthorized,
   over-budget, out-of-scope, digest-mismatched, authority-expanding,
   production/destructive, and unknown-outcome proposals fail closed or pause
   as typed recommendations without mutation. Ordinary worker, test, review,
   CI, and main-drift failures remain recoverable shadow outcomes and do not
   become owner pauses by themselves.
9. **Verification.** Run
   `python -m unittest discover -s tests -p test_agent_*.py`,
   `python tools/check_security_baseline.py`,
   `uv run --no-project python scripts/check_agent_handoff.py`, and
   `git diff --check`; also run the applicable full Python control-suite checks
   and verify no GitHub/provider/effect transport was invoked.
10. **Compatibility, rollback, and retention.** Revert the bounded PR2 shadow
    commit to restore the accepted PR1-only route. Retain PR0 and PR1 receipts,
    ruleset recovery evidence, MX1 archive refs, and all accepted legacy
    controller paths. Shadow rollback never activates a second writer and
    never deletes replay or failure evidence.
11. **Exit artifact.** Shadow source, positive/negative contract tests,
    historical replay evidence, compact redacted status projection, documented
    ownership boundary, exact-head review, canonical CI, and refreshed
    accepted-main receipt.
12. **Next action.** After PR2 is accepted, promote only PR3; do not install
    the Steward service or begin Provider/effect work in this window.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["Provider-free Shadow Steward proposal compiler, deterministic Stage/WorkCard planner and replanner, stop classifier, compact redacted status projection, and historical replay evidence.","Read-only recommendation output with no GitHub, repository, Provider, worker, service, or product mutation."],"allowed_paths":["docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","scripts/agent-control/shadow_steward.py","tests/test_agent_shadow_steward.py","tests/test_check_agent_handoff.py"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_shadow_planning","expected_artifacts":["shadow_steward.py deterministic intake, planning, replanning, stop, replay, and compact status APIs","test_agent_shadow_steward.py positive, negative, and historical failure replay coverage","architecture and module ownership documentation preserving the legacy controller as sole lifecycle writer"],"external_effect_limit":0,"forbidden_changes":["Do not implement the Steward service, worker dispatch, journal, or a second lifecycle writer.","Do not call a Provider or execute a GitHub, repository, product, target, release, deployment, or destructive effect.","Do not retain raw prompts, raw outputs, transcripts, private paths, credentials, or unredacted repository content.","Do not weaken exact-head, CI, review, credential, effect, target, release, deployment, recovery, or single-writer guards."],"forbidden_next_actions":["Do not begin PE7-AUTONOMOUS-STEWARD-PR3 before PR2 is accepted and closed.","Do not install a service, create a SQLite Mission journal, dispatch workers, or enable auto-merge in PR2.","Do not resume parked MX1 Provider work or consume external-effect authority."],"goal":"Implement a provider-free read-only Shadow Steward for natural-language intake, proposal planning, stop classification, compact status, and historical failure replay.","ordered_steps":["Define bounded untrusted intake and digest-bound proposal/approval inputs.","Reuse the accepted Mission contract and implement deterministic Stage/WorkCard planning and replanning projections.","Classify routine failures separately from authority expansion, destructive/production requests, and unknown outcomes.","Emit compact redacted status and replay comparison without treating shadow output as authority.","Run positive/negative replay tests, security baseline, handoff, diff, and full agent-control verification."],"known_store_mutations":[],"packet_id":"PE7-AUTONOMOUS-STEWARD-PR2","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop if shadow output can activate a Mission, mint authority, widen scope, spend budget, or cause mutation.","Stop if raw prompts, outputs, transcripts, private paths, or credentials would be retained.","Stop before any GitHub, Provider, worker, service, product, target, release, deployment, or destructive effect."],"plan_lane_state":"plan_lane_active","prerequisite_receipts":["PE7-AUTONOMOUS-STEWARD-PR1 COMPLETE: PR #628 exact head `07e9080f176c7176f6c55a7b966fac1ea4fa8c1b`; merge `1dda023ace9f2bef7ec54ac3fc316693a145dfa7`; exact-head `PASS`; canonical workflow `33112792625`"],"prerequisites":["PE7-AUTONOMOUS-STEWARD-PR1"],"private_paths_allowed":false,"promotion_evidence_sha256":"4b6eacaa4ff58337a02a6a73f458ffb0e4d3cb4e71f256c1024b3dd6205e1d39","read_paths":["AGENTS.md","docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","scripts/agent-control/mission_contract.py","scripts/agent-control/route_driver.py","scripts/agent-control/local_loop.py","scripts/session_context.py","scripts/check_agent_handoff.py","tests/test_mission_contract.py","tests/test_agent_route_driver.py","scripts/agent-control/shadow_steward.py","tests/test_agent_shadow_steward.py","tests/test_check_agent_handoff.py"],"risk_class":"none","rollback":"Revert the bounded PR2 shadow commit, restore the accepted PR1-only route, and retain all PR0/PR1 ruleset, review, CI, merge, issue, and archive evidence.","route_manifest_sha256":"eb0a81340afb459920c418436fba352263c3977ae65af23988a8605463d4a2d9","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["python -m unittest discover -s tests -p test_agent_*.py","python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"verification_family":"source_focused_full","worker_tier":"T1"}
-->

## Common Execution Protocol

- Keep the changing PR Draft while iterating; batch repairs before final
  exact-head review and Ready CI.
- A new head invalidates prior review and CI; a new `main` invalidates stale
  baseline conclusions.
- PR2 is provider-free and read-only. The existing controller remains the sole
  lifecycle writer until a later canary proves a single replacement writer.
- GitHub API ambiguity, if encountered in later windows, requires readback;
  `OUTCOME_UNKNOWN` is never treated as success or retried blindly.

## Hard Stops

- `DECISION_REQUIRED` on conflicting owner direction, unprovable contract or
  identity, missing rollback, secret exposure, unknown external mutation, or
  shadow output crossing into authority.
- Never weaken exact-head review, canonical CI, expected-head merge,
  credential, effect, target, release, deployment, recovery, or single-writer
  boundaries.
- Never treat the plan, archive refs, branch-local prose, fixture evidence, or
  worker self-report as accepted capability.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` contains only blocked PR3-PR7 routing. Promotion requires
the refreshed accepted PR2 evidence and a new exact dispatch capsule.
