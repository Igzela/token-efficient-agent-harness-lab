# Next Decision

Last updated: 2026-08-10.

This document owns only the current executable window: active routing, the common execution contract, and one current planning/execution window — a single packet block that is either planning-parked (`DECISION_REQUIRED` with a complete, expandable contract) or execution-ready (`READY_FOR_EXECUTION`/`IN_PROGRESS` with its weak-agent dispatch capsule). Accepted truth belongs in `docs/CURRENT_STATUS.md`; long-horizon routing-only packet sketches and promotion profiles belong in `docs/FUTURE_ROUTE.md`; durable invariants belong in `docs/ARCHITECTURE_BOOK.md`; current owners belong in `docs/MODULE_MAP.md`. Live PR heads, CI, and reviews belong only in a fresh context capsule.

## Current Direction

The repository optimizes one outcome:

> Under non-negotiable quality, safety, traceability, compatibility, recovery, and rollback constraints, increase verifiable and reusable task delivery per unit of total lifecycle cost.

Quality, authority, evidence integrity, compatibility, recovery, and rollback are hard gates. Token use, monetary cost, latency, accepted delivery, engineering effort, maintenance surface, and reuse are optimization evidence only after those gates pass.

The accepted route is **bounded recursive Harness optimization**, not open-ended evolution or general recursive self-improvement. Candidate generation, experimental-parent selection, production adoption, and improvement-operator research remain separate authorities.

The following refinements are accepted as of 2026-08-08:

- the repaired 4-cell RWE run is a **viability baseline**, not decision-grade evidence of architecture improvement;
- task-level measurement design and a larger pre-convergence decision baseline precede Architecture Convergence;
- Architecture Convergence begins with three AC0 inventory/freeze packets; AC1–AC6 then separate current-main contract, additive core, and caller/consumer migration, while AC7 separates removal manifest, deletion, and closeout;
- the causal comparison is a contemporary randomized/interleaved old/new replay, not an unqualified historical before/after comparison;
- Harness-Evolution experiment-control hardening keeps five control families but separates each family's contract from implementation and closeout;
- Level-1 first runs without memory or skill projection; memory-only and skill-only tests are optional factor experiments and do not block the core route;
- production adoption and Meta Improver research fork after final transfer evidence and neither authorizes the other;
- a future route label, issue, chat handoff, or promotion profile is not implementation authority. Only a packet satisfying the execution-ready contract below may enter `READY_FOR_EXECUTION`.

This decision changes routing and acceptance gates. It does not authorize a provider call, live experiment, target effect, merge, release, deployment, production adoption, Level-2 controller, or Meta Improver.

## Authoritative Forward Order

The route is stage-ordered. The current window is declared below; blocked successors are indexed without execution authority in `docs/FUTURE_ROUTE.md`:

```text
[window: viability preflight — DECISION_REQUIRED, planning must expand its contract]
→ separately authorized viability run → evidence closeout
→ measurement estimands → corpus/sample → operations/evidence → protocol freeze
→ decision-baseline snapshot → preflight → run → analysis
→ AC0 runtime inventory → data/contract inventory → trace/order freeze
→ AC1–AC6 contract → bounded implementation → migration/closeout
→ AC7 removal manifest → cleanup/closeout
→ contemporary replay reconstruction → freeze/preflight → run → analysis
→ EC1–EC5 contract → implementation/closeout
→ Level-1 preflight/generation → evaluation/closeout
→ Level-1 transfer protocol → run/analysis
→ Level-2 evidence audit → human GO/NO-GO receipt
→ bounded Level-2 controller slices only on GO
→ final-transfer protocol → run → analysis
→ independent adoption and Meta-Improver branches
→ optional R4 metacognitive and R5 weight-adapter research only after supported Meta + separate human GO
→ optional R6 single outer-policy family only after explicit R4/R5 dispositions + separate human GO
→ Dashboard disposition and presentation refresh last
```

The memory/skill factor experiment is an optional branch after Level-1 evaluation. It is not a Level-2 prerequisite. Adoption and Meta Improver remain independent after final transfer; both must reach an explicit completion disposition before the deferred Dashboard refresh becomes eligible. The optional R4-R6 research portfolio does not block adoption or Dashboard, never starts mechanically, and cannot expand evaluator, goal, safety, authority, budget, adoption, release, or deployment mutability.

No downstream micro-packet starts automatically. Every micro-packet must satisfy its named prerequisite on accepted `main` and its class contract below.

## Active Routing

1. `PE7-RWE-V2-VIABILITY-PREFLIGHT-1` — `DECISION_REQUIRED`. Its prerequisite is accepted (PR #370; see the accepted receipts in `docs/CURRENT_STATUS.md`), but its complete execution-ready contract has not been expanded here by the planning owner, so **no packet may start coding now**. Coding session entry fails closed until this window is filled.
2. Every other packet in `docs/FUTURE_ROUTE.md` — `BLOCKED_PREREQUISITE`; every `EFFECT` additionally needs a separate fresh finite T3 operator authority.
3. Dashboard PR #225 — `DEFERRED_LAST`; it is not a shortcut around the route.

## Packet States

- `READY_FOR_EXECUTION` — accepted prerequisites and a complete packet contract permit provider-free implementation.
- `BLOCKED_PREREQUISITE` — a named earlier evidence, implementation, or authority condition is incomplete.
- `DECISION_REQUIRED` — safe direction or authority cannot be derived from accepted owners; no coding entry may consume the window.
- `IN_PROGRESS` — one current branch/PR owns the packet.
- `COMPLETE` — merged, verified, independently reviewed, and synchronized into accepted documents.

Review `PASS`, PR merge, and packet `COMPLETE` are different states. Exact-head review `PASS` satisfies only the independent-review gate.

## Execution-Readiness Contract

A route label, boundary table, issue, chat handoff, promotion profile, or model-generated implementation plan is not enough to start code. Before a blocked packet becomes `READY_FOR_EXECUTION`, this document must contain, for that exact accepted-main frontier:

1. one outcome and explicit non-goals;
2. accepted prerequisites and exact evidence identities;
3. current canonical owners and a bounded allowed-path set;
4. invariants and fields that must remain byte-, value-, or behavior-identical;
5. the only allowed semantic delta;
6. forbidden authority, schema, persistence, evaluator, provider, target, release, and adoption changes;
7. ordered implementation slices small enough for independent review;
8. failure taxonomy, restart/idempotency/concurrency obligations, and stop triggers;
9. focused tests, applicable full tests, exact-head canonical CI, and independent-review requirements;
10. compatibility, migration, rollback, cleanup, and evidence-retention behavior;
11. the exact exit artifact or decision receipt;
12. a next permitted action and forbidden next actions.

Execution readiness is progressive:

- **execution-ready** — all twelve fields are concrete; an implementation agent may work within them;
- **planning-ready** — the goal and boundary are accepted, but current-main inventory or a value decision is still required;
- **routing-only** — ordering is accepted, but implementation details would be premature.

No packet is execution-ready now. `PE7-RWE-V2-VIABILITY-PREFLIGHT-1` is planning-ready below; every packet in `docs/FUTURE_ROUTE.md` is routing-only until its exact predecessor is accepted and its complete contract is moved here and refreshed against then-current `main`. An implementation agent must stop `DECISION_REQUIRED` rather than fill a missing architecture, authority, statistical, evaluator, retention, spend, recovery, or adoption decision.

## Micro-Packet Classes and Consolidation Rule

Every later packet declares one class. The class supplies the repeated contract; the packet supplies only its unique delta, prerequisite, exit, and stop conditions.

### `CONTRACT`

- Provider-free planning/inventory only; production behavior, schema, persistence, authority, evaluator, Provider, target, and output remain unchanged.
- Inspect current owners and callers, freeze exact allowed/forbidden paths, interfaces, invariants, compatibility, migration order, tests, rollback, evidence retention, and unresolved decisions.
- Exit with a versioned, hash-bound contract or manifest accepted by independent review. Do not implement the design in the same packet unless this document explicitly says the delta is purely mechanical.
- Any unresolved value that changes authority, risk, spend, inference, retention, schema, recovery, or adoption exits `DECISION_REQUIRED`.

### `IMPLEMENT`

- Implement exactly one accepted contract and one coherent semantic delta. No new planning choice, owner, authority, external effect, experiment result, or adoption claim.
- Preserve compatibility first. Additive core work precedes caller migration; deletion waits for an explicit cleanup packet.
- Exit with focused negative tests, applicable full tests, parity/restart/concurrency evidence where touched, exact-head CI, independent `PASS`, implementation-cost receipt, and a revert path.
- If the accepted contract does not identify the required file, owner, API, failure mapping, or rollback behavior, stop `DECISION_REQUIRED` instead of guessing.

### `EFFECT`

- Execute one pre-registered external experiment or paid Provider run. It changes no code, contract, evaluator, corpus, budget, seed, reviewer rule, or statistical method.
- Requires an immediately current provider-free preflight plus a distinct finite one-use operator authorization bound to exact accepted-main, artifacts, principal, Provider/model, budgets, expiry, run identity, evidence destinations, and stop rules.
- Record every attempt and consumed lifecycle cost, including failure and outcome unknown. No retuning, selective rerun, hidden rejection, or protocol repair occurs inside the run.
- Exit with restricted raw evidence, a redacted hash-bound receipt, terminal cleanup, and no claim beyond the packet's registered estimand.

### `CLOSEOUT`

- Validate, reconcile, analyze, migrate a mechanically enumerated caller set, or issue a decision receipt from already frozen evidence. No new external effect and no post-result protocol change.
- Recompute identities and statistics independently; preserve missingness, failures, rejected candidates, drift, and unavailable evidence.
- Exit with exact evidence bindings, independent review, explicit PASS/NO-GO/INSUFFICIENT disposition, rollback or next decision, and canonical status synchronization.
- A favorable result cannot waive a failed hard gate; an unfavorable or insufficient result is valid completion.

### Consolidation rule

The default is one focused branch/PR per packet. Adjacent provider-free packets may share one PR only when their accepted parent contract explicitly proves all of the following: same canonical owner, same allowed paths, no intermediate schema/authority/evaluator decision, one rollback point, one reviewable semantic delta, and no loss of an independently useful stop point. `EFFECT` packets, human decision receipts, schema/authority changes, and packets spanning different owners never consolidate. Difficulty or CI cost alone is not justification for consolidation.

### Packet-local reading and activation

An execution session should not load `docs/FUTURE_ROUTE.md` unless it is selecting or refreshing the next packet. It reads `START_HERE.md`, the current status, the common contracts/hard stops in this document, the one active packet block, its accepted predecessor receipt, the relevant owner map/architecture sections, and the exact code/tests.

A predecessor becoming `COMPLETE` does not mechanically make its successor executable. Before changing a successor to `READY_FOR_EXECUTION`, the planning owner must refresh that block against accepted current `main` and replace every routing-level abstraction with exact evidence identities, owner/allowed paths, frozen interfaces/fields, tests, rollback, and any required human or operator gate. The `docs/FUTURE_ROUTE.md` promotion profile supplies bounded promotion-time candidates; facts marked `REFRESH_AT_PROMOTION` must be re-derived from then-current `main`, not guessed. If the accepted predecessor ended `NO_GO`, `DECLINE`, `DEFER`, `SATURATED`, `HARM`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT`, synchronize and rewrite the route before selecting any successor. Do not walk the nominal GO path merely because a prerequisite packet closed.

## Common Evidence and Cost Contract

Every engineering or experimental packet returns a bounded `implementation_cost_receipt` with available realized evidence:

```text
agent_sessions
review_cycles
repair_iterations
ci_runs
ci_compute_minutes
files_changed
schema_migrations
compatibility_adapters_added
authority_boundaries_touched
external_dependencies_added
rollback_complexity
known_maintenance_surface
human_preparation_minutes
review_minutes
material_rework_minutes
recovery_minutes
observed_reuse_count
cost_or_measurement_unavailable_fields
```

Keep realized facts separate from forecasts. Failed, rejected, cancelled, timed-out, killed, recovered, and outcome-unknown attempts retain their consumed cost. Successful-run-only costing is prohibited.

## Comparison and Claim Discipline

```text
chain viability
!= decision-grade baseline
!= architecture-caused improvement
!= Harness improvement on a frozen comparison
!= transfer to unseen tasks
!= improvement-operator improvement
!= production adoption
!= open-ended evolution
```

The inferential unit for cross-task claims is the task or pre-registered task family. Repetitions estimate within-task variability; they do not turn two tasks into a larger independent sample.

Architecture effects require a reconstructable pre-AC Harness and a contemporary randomized/interleaved old/new replay in the same controlled time window. Historical pre/post evidence is compatibility and incident evidence only unless drift is independently ruled out.

## Common Execution Protocol

- Refresh remote `main`, open PR exact heads, dependencies, CI, reviews, canonical documents, and overlapping ownership before work.
- Generate a fresh context capsule and treat it as stale when `main`, a PR head, CI, review, or a canonical document changes.
- Select only the earliest eligible packet. One focused branch/PR owns it.
- Reuse the existing scheduler, executor, ProductTask, worktree, verification, artifact, approval, output, replay, scorecard, audit, cleanup, terminal-evidence, and `LocalProductStore` owners.
- Bind authority from persisted current owners, never caller assertions, model text, branch-local summaries, or memory projections.
- Preserve SQLite/PostgreSQL parity, atomicity, restart, concurrency, idempotency, cancellation, lease ownership, late-write refusal, compensation, and rollback wherever the touched owner requires them.
- Keep provider execution off in CI, target `main` unchanged, Draft-PR-only output, and auto-merge disabled.
- Keep the PR Draft while the diff changes. Fast checks are feedback only.
- Complete focused checks, applicable full checks, handoff/security checks, stable-head complete-diff independent review, Ready transition, canonical exact-head CI, and rollback review before merge.
- A new head invalidates prior CI and review evidence.
- A coding session enters through `uv run --no-project python scripts/session_context.py enter --role coding` and receives one digest-bound compiled context; it does not re-read the whole planning universe.

## Hard Stops

Stop before any of the following:

- secret, credential, raw prompt/output/transcript, private path, or unredacted repository-content exposure;
- a second runtime, scheduler, store, evaluator, budget, approval, output, audit, rollback, VDE, memory-authority, or context-authority owner;
- caller-asserted authority, stale identity, duplicate effect, missing lease, late write, or outcome-unknown treated as success;
- provider call in CI or a paid-provider call without separate current authorization;
- runtime-, candidate-, or experiment-controlled target-default-branch write, auto-merge, repository merge, release, deployment, installation, or automatic production adoption; normal repository-maintainer merge remains governed only by `docs/REAL_WORLD_TESTING_PLAYBOOK.md`;
- candidate modification of evaluator rules, scanner scope, ignore/baseline, sealed holdout, budget accounting, statistical method, reviewer rubric, or immutable safety policy;
- reporting only the best candidate while hiding rejected candidates, diversity collapse, contamination, evaluator gaming, or full consumed cost;
- changing corpus, reviewer policy, budget, verifier, seeds, stop rules, margins, or statistical method after observing comparison results;
- using memory, skills, summaries, novelty scores, forecasts, or scalar VDE indices as authority;
- beginning a routing-only packet from its summary boundary or promotion profile without an accepted execution-ready expansion;
- executing a packet whose dispatch capsule, verification contract, or checkpoint evidence was changed, rehashed, or substituted after acceptance;
- claiming learning, open-ended evolution, or recursive self-improvement without the separately required evidence.

## Packet PE7-RWE-V2-VIABILITY-PREFLIGHT-1

**State:** `DECISION_REQUIRED`

**Prerequisite:** `PE7-RWE-V2-REFREEZE-1` is `COMPLETE`; accepted evidence is in the `## Accepted Packet Receipts` table of `docs/CURRENT_STATUS.md` (PR #370 exact head `36c92b93975366c3f85471f247a3afb128e5351c`, merge `3b4afb3e5ab4254904aa5a63473ab6ae0eac1e82`, exact-head `PASS`, canonical workflow `31312135471`, bound calibration digests).

**Class:** `CONTRACT`

**Why this window is open:** this packet was promoted out of `docs/FUTURE_ROUTE.md` when its prerequisite closed (removed in PR #375; the last accepted `main` still containing the routing sketch is `9d2e9458`, and the sketch is preserved verbatim below so no agent must mine Git history for accepted routing facts). The twelve-field contract below is expanded to the maximum derivable from current accepted `main` (`2ee6571e`); two required values have no accepted owner on current `main` and block execution readiness (blockers B1 and B2 below). No weak-agent dispatch capsule is authored while the window is planning-parked; `check_agent_handoff.py` requires a capsule only for an execution-ready packet, and authoring one here would force the unresolved values to be invented.

### Accepted Routing Facts (preserved from `9d2e9458:docs/FUTURE_ROUTE.md`, the last accepted main containing the sketch)

- **Outcome:** Produce a fresh provider-free preflight and an operator-readable one-use authorization request package for the exact accepted v2 freeze.
- **Allowed delta:** No code, Provider request, spend consumption, target effect, or authorization issuance. Bind main SHA, all freeze hashes, target, principal/scopes, executor, Provider/model, ceilings, expiry, run ID, evidence locations, and stop rules.
- **Exit:** A time-bounded preflight receipt with zero mismatch and a separately reviewable authorization envelope; rerun preflight if its accepted maximum age expires.
- **Stop:** Any stale/missing binding, live lease, non-disposable target state, unknown evidence destination, or unresolved Provider/model drift.

### Twelve-Field Execution-Readiness Contract (derived from accepted `main` `2ee6571e`)

1. **Outcome and explicit non-goals.** Outcome: produce one fresh `rwe_operator_preflight.v1` receipt with `ready=true`, zero blockers, and `provider_call_performed=false` / `target_write_performed=false` / `authority_consumed=false`, plus one operator-readable one-use authorization *request* package (envelope shaped after the store-owned `rwe_run_authorization.v2` body). Non-goals: no code change to `engine/`, `scripts/`, or tooling; no Provider request; no spend; no target effect; no authorization issuance or consumption; no schedule run; no re-freeze or calibration rerun; no measurement/AC/adoption/Meta/Dashboard routing change.

2. **Accepted prerequisites and exact evidence identities.** (a) `PE7-RWE-V2-REFREEZE-1` COMPLETE as in the `Prerequisite` line above; freeze-point main `ee43eac853644266614da09de764a3bf19f2d281` (`OPERATOR_V2_ARTIFACTS_FROZEN_AT_MAIN_SHA` in `engine/src/rwe/operator_corpus.rs`). (b) Current binding main for this packet: `2ee6571ecc3eb80b78bd20ecd2ab359cd734e371`. (c) Frozen target bindings (`engine/src/rwe/frozen_rwe_bindings.rs`): target main `6240768506320a324d68787b9eaa86971c8c930c`, tree `137e912f416a3a8d5be307e91bb2580154fc8fc34c6de52c2441ef3e3f93a064`, verifier `PYTHONPATH=apps/api/src python3 -m pytest apps/api/tests/ -q`, verifier identity `deterministic_rwe_pytest_v1`, risk class `rwe`. (d) Operator admitted consts (`engine/src/rwe/operator_corpus.rs`): target repo `Igzela/alters-lab`, executor `managed_deepseek`, model `deepseek-v4-flash`, binary version `0.1.0`, binary path `in-process:managed_deepseek`. (e) Provider consts (`engine/src/provider/managed_deepseek.rs`): kind `deepseek`, base URL `https://api.deepseek.com`, path `/chat/completions`, credential symbol `DEEPSEEK_API_KEY` (symbol presence only, never read). (f) Operator-input evidence identities to be bound at execution: same-tenant completed Golden Path `product_task_id` with terminal evidence (`task_status=completed`), optional existing `authorization_id` for drift re-check, run ID, and operator key id/tenant.

3. **Canonical owners and bounded allowed/read paths.** Owners (read-only use, no modification): `engine/src/rwe/live_baseline_coordinator.rs::operator_preflight` (sole preflight-evidence producer, schema `rwe_operator_preflight.v1`); `engine/src/rwe/operator_corpus.rs` and `engine/src/rwe/frozen_rwe_bindings.rs` (sole freeze/hash verification); `engine/src/rwe/runner.rs` and `engine/src/storage/local_product_store/rwe_authority.rs` (envelope shapes and store authority tables, read-only); CLI `engine/src/bin/rwe_live_baseline.rs` (`preflight` subcommand). Read paths: `engine/rwe/corpora/rwe-minimum-first-corpus/v2/{protocol,schedule,tasks}` frozen artifacts; `docs/CURRENT_STATUS.md` accepted receipts; this block. Allowed write paths at execution time: none in the repository — the preflight and request package are operator-side evidence bound by digests (PR #370 precedent); the packet closeout syncs only `docs/CURRENT_STATUS.md` gap truth. A future dispatch capsule may not widen these paths.

4. **Invariants that must remain byte-, value-, or behavior-identical.** Freeze hashes `044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20` (corpus), `bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db` (protocol), `6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38` (schedule); all frozen artifacts under `engine/rwe/corpora/rwe-minimum-first-corpus/v2/`; the four-cell schedule (2 tasks × 2 repetitions) with its per-cell ceilings (max cost 0.2, max output 8,192 tokens, 3 provider requests) and run-level budget (max total cost 0.8, 12 requests, 80,768 tokens, 3,600,000 ms); protocol stop rules `["stop_on_authority_failure","stop_on_budget_exhaustion"]`; the `FROZEN_RWE_*` and `OPERATOR_ADMITTED_*` consts; store-owned `rwe_run_authorization.v2` body shape and evidence schemas; the accepted `main` SHA bound into the receipt.

5. **Only allowed semantic delta.** Produce the preflight receipt and the one-use authorization *request* package as described in field 1; nothing else. Both artifacts are documents; neither issues nor consumes authority.

6. **Forbidden authority, schema, persistence, evaluator, provider, target, release, and adoption changes.** Issuing or consuming RWE authority (`issue_and_admit_v2`, admit, spend); any store schema/table/migration change; any new persistence owner or evidence destination outside the established store + digest-bound operator evidence pattern; any Provider call or credential read; any read/write against target repo `Igzela/alters-lab`; any change to `engine/`, `scripts/`, workflow, or tooling code; any evaluator/reviewer/budget/verifier/seed/stop-rule change; release, deployment, installation, or adoption; repairing or rerunning the v2 freeze or calibration; activating `PE7-RWE-V2-VIABILITY-RUN-1` or any successor; changing measurement/AC/adoption/Meta/Dashboard routing.

7. **Ordered implementation slices.** S1 freeze verification: confirm the composition seam and freeze path (`freeze_current_operator_contract_set`) still bind the evidence identities of field 2 with zero mismatch. S2 operator-environment preflight: outside CI, with `DEEPSEEK_API_KEY` symbol present (never read), run `rwe-live-baseline preflight` against the store with the accepted principal, the accepted completed Golden Path `product_task_id`, and (for drift re-check) the prior `authorization_id`; require `ready=true`, zero blockers, and all three negative-effect flags `false`. S3 authorization request package: assemble the one-use envelope from the preflight frozen block, schedule/protocol ceilings and stop rules, and the operator-supplied expiry/run ID/evidence locations, mirroring the `rwe_run_authorization.v2` body shape; no issue/admit call. S4 receipt validation: re-run the preflight within the accepted maximum age (blocker B1) and confirm zero mismatch; hash-bind receipt and package. S5 closeout: record redacted digest bindings and gap truth in `docs/CURRENT_STATUS.md`; independent review; canonical CI.

8. **Failure taxonomy, restart/idempotency/concurrency obligations, and stop triggers.** Failure taxonomy (each is a blocker, matching `operator_preflight` codes): `ci_environment`; `missing_credential_symbol`; `composition_seam_not_ready`; `missing_golden_path_prerequisite_id`, `golden_path_prerequisite_not_ready/not_found/tenant_mismatch/missing`; `authorization_tenant_mismatch/not_active/not_v2`; `frozen_target_sha_mismatch`; `frozen_target_repo_mismatch`; plus zero-mismatch verification failure and evidence-binding failure. Restart/idempotency: the preflight is read-only and idempotent — re-running yields identical bindings and no state to recover; the receipt's freshness is governed by blocker B1's accepted maximum age. Concurrency: exactly one preflight per run identity; no concurrent preflight, issue, admit, or run. Stop triggers: the preserved Stop list above verbatim; any blocker preventing `ready=true`; any unresolved Provider/model drift; expiry-policy violation under blocker B2.

9. **Focused tests, applicable full tests, exact-head canonical CI, and independent-review requirements.** Focused: engine RWE preflight/freeze tests (`preflight_fails_closed_without_gp_and_without_consuming`, `frozen_bindings_load_and_match_pytest`, `four_cell_injected_orchestration_maps_identities_and_receipts` in `engine/src/rwe/live_baseline_coordinator.rs` / `frozen_rwe_bindings.rs`) and `tests/test_session_context.py::test_current_repository_planning_parked_window_fails_closed`. Applicable full: `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets --all-features -- -D warnings`, `scripts/ci/run_rust_tests.py`, the Python unittest suite, `scripts/verify_rust_typescript_stack.sh`, `scripts/check_wire_codegen_drift.sh`, `tools/check_security_baseline.py`, `scripts/check_agent_handoff.py`. Canonical CI: the canonical `tests` workflow on the exact PR head (this routing change is documentation-only and runs the strict documentation-only lane; the classifier is computed from the accepted base checkout). Independent review: complete `base...head` diff with an exact-head receipt on the final head; a new head invalidates prior CI and review.

10. **Compatibility, migration, rollback, cleanup, and evidence-retention behavior.** Compatibility: no code, schema, or behavior change — zero compatibility surface. Migration: none. Rollback: no external effect exists to roll back (provider-free); a failing or stale preflight is discarded and re-run; the request package is a non-authoritative document whose disposal is deletion. Cleanup: evidence hygiene only — never commit restricted raw content; commit redacted digests. Retention: restricted raw evidence stays operator-side with SHA-256 digest bindings in the closeout (PR #370 precedent); the redacted receipt is durable; no retention-policy change.

11. **Weak-Agent Dispatch Capsule.** Not authored in this planning-parked window. At promotion (after B1/B2 resolution) the capsule inside this block must match this contract exactly and bind at least: packet identity/state; the provider-free dispatch lane; goal; prerequisites; allowed paths; read paths; ordered steps; forbidden changes and forbidden next actions; verification; expected artifacts; rollback; pause gates; `external_effect_limit=0`; `authority_consumption_allowed=false`; `secret_values_allowed=false`; `private_paths_allowed=false`; `plan_lane_state="plan_lane_deferred_until_terminal_owners"`. The capsule and the entry it feeds create no authority.

12. **Next permitted action and forbidden next actions.** Next permitted: the planning owner or a human operator accepts the two blocker values below (safety-relevant retention/spend decisions), re-derives nothing else, and promotes this block to `READY_FOR_EXECUTION` with the field-11 capsule; only then may a coding entry run the provider-free preflight procedure (S1–S5). Forbidden next actions: issuing or consuming a one-use authorization; any Provider call; any target write; running the four-cell schedule; promoting or activating `PE7-RWE-V2-VIABILITY-RUN-1`; repairing or rerunning the v2 freeze or calibration; any coding entry before the blockers are resolved.

### Blockers (values with no accepted owner on current `main`)

- **B1 — accepted maximum age of the preflight receipt.** The preserved Exit requires "a time-bounded preflight receipt" and "rerun preflight if its accepted maximum age expires", but no maximum-age value or owner exists on current `main` (no code constant, no accepted document value). Without it, receipt freshness cannot fail closed. Decision needed: the bounded age (e.g. a concrete hours/days window) and its owner.
- **B2 — one-use authorization request-package expiry policy.** The preserved Allowed delta requires binding "expiry", and the store enforces a finite `expires_at` on the one-use authorization without any accepted validity-window policy on `main`. Decision needed: the request package's proposed expiry/validity window and its owner.

**Who owns the next step:** the planning owner or operator accepts B1 and B2 (bounded, safety-relevant retention/spend decisions), then promotes this block to `READY_FOR_EXECUTION`, authors the field-11 `weak-agent-dispatch:v1` capsule inside this block, and has the promotion independently reviewed. Until then, `session_context.py enter --role coding` reports `DECISION_REQUIRED` (`packet_not_executable`) and issues no checkpoint commands.

**Forbidden while the window stays open:** promoting or executing from the preserved routing facts or a future-route profile alone; inventing the B1/B2 values, owners, allowed paths, ordered steps, or verification commands; issuing or admitting RWE authority; calling a Provider; running a schedule cell; writing a target repository; repairing the accepted v2 freeze; rerunning calibration; activating any successor; or changing measurement/AC/adoption/Meta/Dashboard routing.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` preserves the accepted long-horizon order, routing-only packet sketches, and bounded promotion profiles. It cannot authorize implementation, Provider effects, promotion, merge, release, or deployment. Promotion requires removing exactly one eligible packet from that document, expanding it here against accepted current `main`, and independently reviewing the resulting routing change; the profile facts marked `REFRESH_AT_PROMOTION` are candidates, not accepted contract.
