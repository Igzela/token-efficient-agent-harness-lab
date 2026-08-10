# Next Decision

Last updated: 2026-08-10.

This document owns only the current executable window: active routing, the common execution contract, and one current planning/execution window — a single packet block that is either planning-parked (`DECISION_REQUIRED` with a complete, expandable contract) or execution-ready (`READY_FOR_EXECUTION`/`IN_PROGRESS` with its weak-agent dispatch capsule). Accepted truth belongs in `docs/CURRENT_STATUS.md`; long-horizon routing-only packet sketches and promotion profiles belong in `docs/FUTURE_ROUTE.md`; durable invariants belong in `docs/ARCHITECTURE_BOOK.md`; current owners belong in `docs/MODULE_MAP.md`. Live PR heads, CI, and reviews belong only in a fresh context capsule.

## Current Direction

The repository optimizes one outcome:

> Under non-negotiable quality, safety, traceability, compatibility, recovery, and rollback constraints, increase verifiable and reusable task delivery per unit of total lifecycle cost.

Quality, authority, evidence integrity, compatibility, recovery, and rollback are hard gates. Token use, monetary cost, latency, accepted delivery, engineering effort, maintenance surface, and reuse are optimization evidence only after those gates pass.

The accepted route is **bounded recursive Harness optimization**, not open-ended evolution or general recursive self-improvement. Candidate generation, experimental-parent selection, production adoption, and improvement-operator research remain separate authorities. A higher-priority repository-maintenance transition now hardens the control plane that executes these packets; it changes no product authority or external-effect boundary.

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
[window: repository-maintenance route contract — READY_FOR_EXECUTION, provider-free contract synchronization only]
→ activate unified Plan lane
→ execute/recover/review/merge/closeout through existing maintenance owners
→ select and promote exactly one successor at a time
→ return to the blocked PREFLIGHT repair/reconciliation window
→ [window: viability preflight — provider-free S1–S5 only]
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

1. `PE7-CTRL-ROUTE-CONTRACT-1` — `READY_FOR_EXECUTION`. This provider-free packet synchronizes the accepted repository-maintenance route contract and establishes the exact owner boundary for later Plan-lane activation. It creates no worker, PR, merge, Provider, target, or T3 effect.
2. The provider-free viability PREFLIGHT remains blocked until the control-plane transition completes and its B1/B2 enforcement/provenance disposition is reconciled. Its previously authored execution contract is retained below as historical packet material and is not an execution surface while blocked.
3. Every other packet in `docs/FUTURE_ROUTE.md` — `BLOCKED_PREREQUISITE`; every `EFFECT` additionally needs a separate fresh finite T3 operator authority.
4. Dashboard PR #225 — `DEFERRED_LAST`; it is not a shortcut around the route.

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

No packet other than the current one is execution-ready. `PE7-CTRL-ROUTE-CONTRACT-1` is execution-ready below; `PE7-RWE-V2-VIABILITY-PREFLIGHT-1` is blocked pending control-plane completion and B1/B2/provenance reconciliation; every packet in `docs/FUTURE_ROUTE.md` is routing-only until its exact predecessor is accepted and its complete contract is moved here and refreshed against then-current `main`. An implementation agent must stop `DECISION_REQUIRED` rather than fill a missing architecture, authority, statistical, evaluator, retention, spend, recovery, or adoption decision.

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

## Packet PE7-CTRL-ROUTE-CONTRACT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** accepted `main` has the existing Issue-based engineering outer loop, weak-agent session entry, exact-head CI, bounded review convergence, and the Plan-lane parser/ledger scaffolding; no new scheduler, queue, lease, state database, review owner, merge owner, or product authority may be introduced.

**Class:** `CONTRACT`

**Outcome:** Synchronize the repository-maintenance route contract and current-state reconciliation boundary so later weak workers can operate from accepted documents and durable GitHub state rather than chat memory.

**Allowed delta:** Canonical documentation, the handoff checker's historical-packet projection, and focused session-entry/handoff contract tests only. This packet does not activate Plan execution, create a PR, request CI/review/merge, call a Provider, read a credential, write a target, consume T3 authority, or repair the RWE product contract.

**Exit:** Accepted documentation records the single route-controller owner, queue/lease/state/review/merge/closeout owners, exact-head invalidation rule, bounded repair/escalation boundary, current PREFLIGHT blocker truth, and the next admitted Plan-lane packet.

**Stop:** Any second owner, unbound merge path, self-reported acceptance, stale current-main identity, unresolved B1/B2 authority/expiry enforcement, or requirement to choose a schema, evaluator, retention, spend, statistical, or T3 value.

### Twelve-field contract

1. **Outcome and non-goals.** Establish the repository-maintenance route contract only; no product runtime, target repository, Provider, release, deployment, adoption, T3, or protected-branch policy change.
2. **Prerequisites.** Accepted main `c3e58576cbba40dbcad666c39eefb6bbdc372434`; current control-plane owners in `docs/MODULE_MAP.md`; existing exact-head CI/review/merge rules in `docs/REAL_WORLD_TESTING_PLAYBOOK.md`.
3. **Owners and paths.** Navigation/context: `START_HERE.md`, `scripts/session_context.py`, `scripts/project_context.py`; packet owner: `docs/NEXT_DECISION.md`; successor owner: `docs/FUTURE_ROUTE.md`; durable GitHub state/lease: `scripts/agent-control/state_manager.py`; controller/CLI: `scripts/agent-control/local_loop.py` and `loopctl.py`; plan compiler: `plan_lane.py`; lifecycle owners: existing dispatcher, worktree, artifact, CI, review, merge, and closeout workflows.
4. **Frozen invariants.** GitHub remains the durable queue/lease/effect-state owner; local files remain rebuildable projections; accepted main and exact head bind every transition; no child receives credentials or merge authority; no target/release/Provider/T3 effect is implied.
5. **Semantic delta.** Replace the permanent “Plan lane deferred” design direction with a staged admitted-lane transition, beginning with this contract and later replacing the deferred readiness check with real readiness checks.
6. **Forbidden changes.** No second controller, scheduler, queue, lease, state database, CI owner, review owner, merge owner, product runtime, authority, evaluator, schema, migration, release, deploy, target output, Provider call, T3 minting, or branch-protection change.
7. **Ordered slices.** (a) synchronize accepted route/current blocker truth; (b) activate unified weak-agent dispatch consumption; (c) add lifecycle/recovery transition integration; (d) add successor/promotion/escalation/T3 pause/resume; (e) run provider-free soak and current-PREFLIGHT smoke.
8. **Failure/recovery.** Ordinary CI/review/worker/checkpoint/main-drift failures use existing bounded repair/reconcile transitions; duplicate dispatch/PR/merge/promotion and outcome-unknown stop fail closed; unprovable architecture/authority/T3 values emit a bounded decision artifact.
9. **Verification.** Focused session-entry and handoff tests; all applicable Python/control-plane tests; exact-head canonical CI; independent complete-diff exact `PASS`; `scripts/check_agent_handoff.py`; `git diff --check`.
10. **Compatibility/rollback.** Existing Issue lane remains unchanged; Plan lane remains non-admitted until its own packet; docs revert cleanly; no runtime or schema migration occurs in this packet.
11. **Exit artifact.** Accepted contract receipt in this packet, current PREFLIGHT blocker disposition, and a digest-bound `weak-agent-dispatch:v1` capsule whose effect limit is zero.
12. **Next action.** After merge, promote `PE7-CTRL-PLAN-LANE-1` against the refreshed accepted main; do not run PREFLIGHT or any successor effect from this packet.

### 11. Weak-Agent Dispatch Capsule

The active capsule below binds this documentation-only contract packet. It grants
no execution authority beyond the listed repository paths and zero external
effect; later lifecycle packets must replace it with their own exact capsule.

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["Redacted canonical route-contract documentation changes only","Focused session-context and handoff contract test results with no external effect"],"allowed_paths":["docs/ARCHITECTURE_BOOK.md","docs/CURRENT_STATUS.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","scripts/check_agent_handoff.py","tests/test_session_context.py","tools/test_check_agent_handoff.py"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["One reviewed documentation contract for the repository-maintenance route","One focused session-context and handoff test result proving the active packet and historical dependency projection bind consistently"],"external_effect_limit":0,"forbidden_changes":["Do not activate the Plan lane from this contract packet.","Do not create or merge a PR through worker output.","Do not call a Provider, read credentials, write a target, or consume T3 authority.","Do not change branch protection or create a second lifecycle owner.","Do not change product runtime, schema, evaluator, authority, release, deployment, or target behavior."],"forbidden_next_actions":["Do not activate PE7-CTRL-PLAN-LANE-1 from this packet.","Do not run PREFLIGHT S1-S5 or any successor effect.","Do not issue, admit, consume, or mint authority.","Do not call a Provider, read credentials, or write a target repository."],"goal":"Synchronize the repository-maintenance route contract and current blocker truth without external effects.","known_store_mutations":[],"ordered_steps":["Reconcile accepted-main, packet, owner, and blocker facts against the canonical documents.","Update only the listed documentation, handoff checker, and focused contract-test paths.","Run the focused test, handoff checker, and diff hygiene checks.","Stop for exact-head review and canonical CI; do not self-report acceptance."],"packet_id":"PE7-CTRL-ROUTE-CONTRACT-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop on stale accepted-main or conflicting current documents.","Stop on any unprovable authority, schema, evaluator, retention, spend, statistical, or T3 value.","Stop if any requested change requires a second lifecycle owner or an external effect."],"plan_lane_state":"plan_lane_deferred_until_terminal_owners","prerequisite_receipts":["Accepted main c3e58576cbba40dbcad666c39eefb6bbdc372434","Existing exact-head CI, review, merge, and local-loop owner contracts"],"prerequisites":["Accepted main contains the existing Issue-based engineering outer loop and weak-agent session entry","Existing canonical documents identify the reusable CI, review, merge, and closeout owners"],"private_paths_allowed":false,"read_paths":["START_HERE.md","AGENTS.md","docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","docs/MODULE_MAP.md","docs/ARCHITECTURE_BOOK.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","scripts/session_context.py","scripts/project_context.py","scripts/agent-control/"],"rollback":"Revert the documentation and focused test changes on this branch; no GitHub state, provider, credential, target, authority, or product effect is created by this packet.","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["PYTHONPATH=src uv run --no-project python -m unittest tests.test_session_context","uv run --no-project python -m unittest tools.test_check_agent_handoff","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"]}
-->

## Retained PREFLIGHT Contract (historical: PE7-RWE-V2-VIABILITY-PREFLIGHT-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `PE7-RWE-V2-REFREEZE-1` is `COMPLETE`; accepted evidence is in the `## Accepted Packet Receipts` table of `docs/CURRENT_STATUS.md` (PR #370 exact head `36c92b93975366c3f85471f247a3afb128e5351c`, merge `3b4afb3e5ab4254904aa5a63473ab6ae0eac1e82`, exact-head `PASS`, canonical workflow `31312135471`, bound calibration digests).

**Class:** `CONTRACT`

**Why this packet is retained:** this packet was promoted out of `docs/FUTURE_ROUTE.md` when its prerequisite closed, but the current audit found that B1 freshness and B2 issuance-window enforcement are not implemented. The twelve-field contract remains preserved for later re-expansion; it is not executable until the control-plane transition and the minimal canonical repair/provenance disposition are accepted.

### Accepted Routing Facts (preserved from `9d2e9458:docs/FUTURE_ROUTE.md`, the last accepted main containing the sketch)

- **Outcome:** Produce a fresh provider-free preflight and an operator-readable one-use authorization request package for the exact accepted v2 freeze.
- **Allowed delta:** No code, Provider request, spend consumption, target effect, or authorization issuance. Bind main SHA, all freeze hashes, target, principal/scopes, executor, Provider/model, ceilings, expiry, run ID, evidence locations, and stop rules.
- **Exit:** A time-bounded preflight receipt with zero mismatch and a separately reviewable authorization envelope; rerun preflight if its accepted maximum age expires.
- **Stop:** Any stale/missing binding, live lease, non-disposable target state, unknown evidence destination, or unresolved Provider/model drift.

### Twelve-Field Execution-Readiness Contract (derived from accepted `main` `2ee6571e`, re-verified against current `main` `0a7fde68`)

1. **Outcome and explicit non-goals.** Outcome: produce one fresh `rwe_operator_preflight.v1` receipt with `ready=true`, zero blockers, and `provider_call_performed=false` / `target_write_performed=false` / `authority_consumed=false`, plus one operator-readable one-use authorization *request* package (envelope shaped after the store-owned `rwe_run_authorization.v2` body). Non-goals: no code change to `engine/`, `scripts/`, or tooling; no Provider request; no spend; no target effect; no authorization issuance or consumption; no schedule run; no re-freeze or calibration rerun; no measurement/AC/adoption/Meta/Dashboard routing change.

2. **Accepted prerequisites and exact evidence identities.** (a) `PE7-RWE-V2-REFREEZE-1` remains COMPLETE as in the `Prerequisite` line above; freeze-point main `ee43eac853644266614da09de764a3bf19f2d281` remains the code-bound freeze identity. (b) Binding main at this packet's contract refresh was `0a7fde6888e6533f180f9d2dbf76eebdb0899d9d`; the current accepted main for any future re-expansion is `c3e58576cbba40dbcad666c39eefb6bbdc372434`. (c) Frozen target bindings (`engine/src/rwe/frozen_rwe_bindings.rs`): target main `6240768506320a324d68787b9eaa86971c8c930c`, tree `137e912f416a3a8d5be307e91bb2580154fc8fc34c6de52c2441ef3e3f93a064`, verifier `PYTHONPATH=apps/api/src python3 -m pytest apps/api/tests/ -q`, verifier identity `deterministic_rwe_pytest_v1`, risk class `rwe`. (d) Operator admitted consts (`engine/src/rwe/operator_corpus.rs`): target repo `Igzela/alters-lab`, executor `managed_deepseek`, model `deepseek-v4-flash`, binary version `0.1.0`, binary path `in-process:managed_deepseek`. (e) Provider consts (`engine/src/provider/managed_deepseek.rs`): kind `deepseek`, base URL `https://api.deepseek.com`, path `/chat/completions`, credential symbol `DEEPSEEK_API_KEY` (symbol presence only, never read). (f) Operator-input evidence identities to be bound only after the repair/provenance gate: same-tenant completed Golden Path `product_task_id` with terminal evidence (`task_status=completed`), optional existing `authorization_id` for drift re-check, run ID, and operator key id/tenant.

3. **Canonical owners and bounded allowed/read paths.** Owners (read-only use, no modification): `engine/src/rwe/live_baseline_coordinator.rs::operator_preflight` (sole preflight-evidence producer, schema `rwe_operator_preflight.v1`); `engine/src/rwe/operator_corpus.rs` and `engine/src/rwe/frozen_rwe_bindings.rs` (sole freeze/hash verification); `engine/src/rwe/runner.rs` and `engine/src/storage/local_product_store/rwe_authority.rs` (envelope shapes and store authority tables, read-only); CLI `engine/src/bin/rwe_live_baseline.rs` (`preflight` subcommand). Read paths: `engine/rwe/corpora/rwe-minimum-first-corpus/v2/{protocol,schedule,tasks}` frozen artifacts; `docs/CURRENT_STATUS.md` accepted receipts; this block. Allowed write paths at execution time: none in the repository — the preflight and request package are operator-side evidence bound by digests (PR #370 precedent); the packet closeout syncs only `docs/CURRENT_STATUS.md` gap truth. A future dispatch capsule may not widen these paths.

4. **Invariants that must remain byte-, value-, or behavior-identical.** Freeze hashes `044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20` (corpus), `bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db` (protocol), `6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38` (schedule); all frozen artifacts under `engine/rwe/corpora/rwe-minimum-first-corpus/v2/`; the four-cell schedule (2 tasks × 2 repetitions) with its per-cell ceilings (max cost 0.2, max output 8,192 tokens, 3 provider requests) and run-level budget (max total cost 0.8, 12 requests, 80,768 tokens, 3,600,000 ms); protocol stop rules `["stop_on_authority_failure","stop_on_budget_exhaustion"]`; the `FROZEN_RWE_*` and `OPERATOR_ADMITTED_*` consts; store-owned `rwe_run_authorization.v2` body shape and evidence schemas; the accepted `main` SHA bound into the receipt.

5. **Only allowed semantic delta.** Produce the preflight receipt and the one-use authorization *request* package as described in field 1; nothing else. Both artifacts are documents; neither issues nor consumes authority.

6. **Forbidden authority, schema, persistence, evaluator, provider, target, release, and adoption changes.** Issuing or consuming RWE authority (`issue_and_admit_v2`, admit, spend); any store schema/table/migration change; any new persistence owner or evidence destination outside the established store + digest-bound operator evidence pattern; any Provider call or credential read; any read/write against target repo `Igzela/alters-lab`; any change to `engine/`, `scripts/`, workflow, or tooling code; any evaluator/reviewer/budget/verifier/seed/stop-rule change; release, deployment, installation, or adoption; repairing or rerunning the v2 freeze or calibration; activating `PE7-RWE-V2-VIABILITY-RUN-1` or any successor; changing measurement/AC/adoption/Meta/Dashboard routing.

7. **Ordered implementation slices.** S1 freeze verification: confirm the composition seam and freeze path (`freeze_current_operator_contract_set`) still bind the evidence identities of field 2 with zero mismatch. S2 operator-environment preflight: outside CI, with `DEEPSEEK_API_KEY` symbol present (never read), run `rwe-live-baseline preflight` against the store with the accepted principal, the accepted completed Golden Path `product_task_id`, and (for drift re-check) the prior `authorization_id`; require `ready=true`, zero blockers, and all three negative-effect flags `false`. S3 authorization request package: assemble the one-use envelope from the preflight frozen block, schedule/protocol ceilings and stop rules, and the operator-supplied run ID/evidence locations, with `expires_at` exactly derived from the authenticated issuance time plus the accepted B2 window (2 hours), mirroring the `rwe_run_authorization.v2` body shape; no issue/admit call. S4 receipt validation: re-run the preflight within the accepted B1 maximum age (15 minutes) and confirm zero mismatch; hash-bind receipt and package. S5 closeout: record redacted digest bindings and gap truth in `docs/CURRENT_STATUS.md`; independent review; canonical CI.

8. **Failure taxonomy, restart/idempotency/concurrency obligations, and stop triggers.** Failure taxonomy (each is a blocker, matching `operator_preflight` codes): `ci_environment`; `missing_credential_symbol`; `composition_seam_not_ready`; `missing_golden_path_prerequisite_id`, `golden_path_prerequisite_not_ready/not_found/tenant_mismatch/missing`; `authorization_tenant_mismatch/not_active/not_v2`; `frozen_target_sha_mismatch`; `frozen_target_repo_mismatch`; plus zero-mismatch verification failure and evidence-binding failure. Restart/idempotency: the preflight is read-only and idempotent — re-running yields identical bindings and no state to recover; the receipt's freshness is governed by the accepted B1 maximum age (15 minutes, no grace period: a receipt older than 15 minutes is stale, MUST be discarded and re-run, and the re-run must again reach `ready=true`, zero blockers, exact frozen bindings, and all negative-effect flags `false`). Concurrency: exactly one preflight per run identity; no concurrent preflight, issue, admit, or run. Stop triggers: the preserved Stop list above verbatim; any blocker preventing `ready=true`; any unresolved Provider/model drift; any expiry-policy violation under the accepted B2 validity window (2 hours from issuance, no renewal/extension).

9. **Focused tests, applicable full tests, exact-head canonical CI, and independent-review requirements.** Focused: engine RWE preflight/freeze tests (`preflight_fails_closed_without_gp_and_without_consuming`, `frozen_bindings_load_and_match_pytest`, `four_cell_injected_orchestration_maps_identities_and_receipts` in `engine/src/rwe/live_baseline_coordinator.rs` / `frozen_rwe_bindings.rs`) and `tests/test_session_context.py::test_current_repository_execution_ready_window_binds_dispatch_capsule`. Applicable full: `cargo fmt --all -- --check`, `cargo clippy -p engine --all-targets --all-features -- -D warnings`, `scripts/ci/run_rust_tests.py`, the Python unittest suite, `scripts/verify_rust_typescript_stack.sh`, `scripts/check_wire_codegen_drift.sh`, `tools/check_security_baseline.py`, `scripts/check_agent_handoff.py`. Canonical CI: the canonical `tests` workflow on the exact PR head. The promotion PR that accepts B1/B2 and promotes this packet carries the live-window test sync (`tests/test_session_context.py`) and therefore runs the complete matrix, not the documentation-only lane; the later S5 closeout sync to `docs/CURRENT_STATUS.md` is documentation-only prose governed by the accepted-base classifier. Independent review: complete `base...head` diff with an exact-head receipt on the final head; a new head invalidates prior CI and review.

10. **Compatibility, migration, rollback, cleanup, and evidence-retention behavior.** Compatibility: no code, schema, or behavior change — zero compatibility surface. Migration: none. Rollback: no external effect exists to roll back (provider-free); a failing or stale preflight is discarded and re-run; the request package is a non-authoritative document whose disposal is deletion. Cleanup: evidence hygiene only — never commit restricted raw content; commit redacted digests. Retention: restricted raw evidence stays operator-side with SHA-256 digest bindings in the closeout (PR #370 precedent); the redacted receipt is durable; no retention-policy change.

### 11. Historical Weak-Agent Dispatch Capsule

The retired `weak-agent-dispatch:v1` capsule below records the former PREFLIGHT execution projection for audit only. It is not an execution surface while this packet is blocked; the active capsule belongs to `PE7-CTRL-ROUTE-CONTRACT-1` above.

<!-- retired-weak-agent-dispatch:v1
{"allowed_outputs":["Operator-side rwe_operator_preflight.v1 receipt bound by SHA-256 (never committed)","Operator-side one-use authorization request package bound by SHA-256 (never committed)","docs/CURRENT_STATUS.md gap-truth sync at S5 closeout (committed)"],"allowed_paths":["docs/CURRENT_STATUS.md"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_local","expected_artifacts":["One rwe_operator_preflight.v1 receipt with ready=true, zero blockers, and provider_call_performed=false, target_write_performed=false, authority_consumed=false","One operator-readable one-use authorization request package mirroring the store-owned rwe_run_authorization.v2 body shape with expires_at = authenticated issuance time + 2 hours (canonical UTC/RFC3339)","docs/CURRENT_STATUS.md closeout recording redacted digest bindings and confirmed gap truth"],"external_effect_limit":0,"forbidden_changes":["No code change to engine/, scripts/, workflow, or tooling","No store schema, table, migration, persistence, or evidence-destination change","No authority, evaluator, reviewer, budget, verifier, seed, or stop-rule change","No Provider call or credential read; DEEPSEEK_API_KEY symbol presence check only","No read or write against target repository Igzela/alters-lab","No repair or rerun of the v2 freeze or calibration","No change to v2 corpus, protocol, schedule, budgets, model, verifier, seeds, or stop rules","No measurement, AC, adoption, Meta, or Dashboard routing change","No activation of PE7-RWE-V2-VIABILITY-RUN-1 or any successor"],"forbidden_next_actions":["Do not issue, admit, or consume RWE authority (issue_and_admit_v2, admit, spend)","Do not call a Provider or read credentials","Do not write the target repository Igzela/alters-lab","Do not run the four-cell schedule or activate PE7-RWE-V2-VIABILITY-RUN-1 or any successor","Do not repair or rerun the v2 freeze or calibration","Do not change v2 corpus, protocol, schedule, budgets, model, verifier, seeds, or stop rules","Do not change production code, schema, store, or persistence authority","Do not start a later packet or broaden scope"],"goal":"Produce one fresh provider-free rwe_operator_preflight.v1 receipt with ready=true, zero blockers, and all negative-effect flags false, plus one operator-readable one-use authorization request package bound to the exact accepted v2 freeze, without issuing or consuming authority.","known_store_mutations":[],"ordered_steps":["S1 freeze verification: confirm the composition seam and freeze path (freeze_current_operator_contract_set) still bind the field-2 evidence identities with zero mismatch","S2 operator-environment preflight: outside CI, with DEEPSEEK_API_KEY symbol present but never read, run rwe-live-baseline preflight against the store with the accepted principal, the accepted completed Golden Path product_task_id, and the prior authorization_id for drift re-check; require ready=true, zero blockers, and all three negative-effect flags false","S3 authorization request package: assemble the one-use envelope from the preflight frozen block, schedule/protocol ceilings and stop rules, and the operator-supplied run ID and evidence locations, with expires_at exactly derived from the authenticated issuance time plus the accepted 2-hour B2 window; no issue/admit call","S4 receipt validation: re-run the preflight within the accepted 15-minute B1 maximum age and confirm zero mismatch; discard and re-run any receipt older than 15 minutes (no grace period); hash-bind receipt and package","S5 closeout: record redacted digest bindings and gap truth in docs/CURRENT_STATUS.md; independent review; canonical CI"],"packet_id":"PE7-RWE-V2-VIABILITY-PREFLIGHT-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop on any blocker preventing ready=true or on any negative-effect flag true","Stop on any stale or missing binding, live lease, non-disposable target state, unknown evidence destination, or unresolved Provider/model drift","Stop if the receipt is older than the accepted 15-minute B1 maximum age before consumption; discard and rerun","Stop before issuing or consuming RWE authority, calling a Provider, reading credentials, writing the target, or running the four-cell schedule","Stop on any expiry-policy violation under the accepted 2-hour B2 validity window"],"plan_lane_state":"plan_lane_deferred_until_terminal_owners","prerequisite_receipts":["PR #370 exact head 36c92b93975366c3f85471f247a3afb128e5351c, merge 3b4afb3e5ab4254904aa5a63473ab6ae0eac1e82, exact-head PASS, canonical workflow 31312135471","Frozen v2 hashes: corpus 044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20, protocol bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db, schedule 6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38"],"prerequisites":["PE7-RWE-V2-REFREEZE-1 is COMPLETE"],"private_paths_allowed":false,"read_paths":["engine/src/rwe/live_baseline_coordinator.rs","engine/src/rwe/operator_corpus.rs","engine/src/rwe/frozen_rwe_bindings.rs","engine/src/rwe/runner.rs","engine/src/storage/local_product_store/rwe_authority.rs","engine/src/bin/rwe_live_baseline.rs","engine/rwe/corpora/rwe-minimum-first-corpus/v2/","docs/CURRENT_STATUS.md"],"rollback":"Provider-free execution has no external effect to roll back: a failing or stale receipt is discarded and re-run; the request package is a non-authoritative document whose disposal is deletion; the S5 closeout is a prose-only docs/CURRENT_STATUS.md sync reverted by git; restricted raw evidence stays operator-side and is never committed.","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["cargo fmt --all -- --check","cargo clippy -p engine --all-targets --all-features -- -D warnings","scripts/ci/run_rust_tests.py","uv run --no-project python -m unittest discover -s tests","bash scripts/verify_rust_typescript_stack.sh","bash scripts/check_wire_codegen_drift.sh","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"]}
-->


12. **Retained next-action note.** No coding entry may run S1–S5 from this historical block. After the control-plane transition, a new planning/repair packet must re-expand this contract against accepted `main` and bind authoritative B1 freshness, store-owned B2 expiry, and the provenance disposition before any PREFLIGHT execution. Until then, issuing or consuming authority, calling a Provider, reading credentials, writing a target, running the four-cell schedule, or activating any successor remains forbidden.

### Retained Policy Values (enforcement not accepted)

Both values below remain **packet-local** operator policy for `PE7-RWE-V2-VIABILITY-PREFLIGHT-1` (operator decision accepted 2026-08-10). Their policy owner is this packet block plus the confirmed-gap truth in `docs/CURRENT_STATUS.md`, but enforcement is not accepted because the current receipt has no authoritative B1 timestamp and production v2 issuance still accepts caller-controlled B2 expiry. They must not be generalized to unrelated authorization classes.

- **B1 — accepted maximum age of the preflight receipt: 15 minutes.** Policy: a preflight receipt may be consumed only within 15 minutes of its recorded creation/completion time; once older than 15 minutes it is stale and MUST be discarded and re-run; no grace period; a re-run must again reach `ready=true`, zero blockers, exact frozen bindings, and all required negative-effect flags `false`; the preflight remains provider-free and read-only and creates no authority.
- **B2 — one-use authorization request-package validity window: 2 hours from authorization issuance.** Policy: `expires_at` must be exactly derived from the authenticated issuance time + 2 hours; it must be canonical UTC/RFC3339; no caller-supplied arbitrary longer expiry; no renewal or extension of an issued authorization; once expired, discard it and require a fresh preflight plus a newly reviewed/issued one-use authorization; this is intentionally bounded relative to the frozen v2 schedule, whose maximum run wall-time is 1 hour; one-use semantics, exact run identity, spend ceilings, and target/freeze/provider/model bindings and all existing stop rules remain unchanged.

**Blocked next step:** after the control-plane transition completes, a new planning/repair packet must bind authoritative B1 timestamp evidence, store-owned B2 issuance expiry, and the Golden Path test-tooling provenance disposition before a coding entry may run S1–S5 provider-free.

**Forbidden while the window stays open:** executing from the preserved routing facts or a future-route profile alone (the capsule is the only execution surface); inventing, re-deriving, or widening the B1/B2 values, owners, allowed paths, ordered steps, or verification commands; issuing or admitting RWE authority; calling a Provider; reading credentials; running a schedule cell; writing a target repository; repairing the accepted v2 freeze; rerunning calibration; activating any successor; or changing measurement/AC/adoption/Meta/Dashboard routing.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` preserves the accepted long-horizon order, routing-only packet sketches, and bounded promotion profiles. It cannot authorize implementation, Provider effects, promotion, merge, release, or deployment. Promotion requires removing exactly one eligible packet from that document, expanding it here against accepted current `main`, and independently reviewing the resulting routing change; the profile facts marked `REFRESH_AT_PROMOTION` are candidates, not accepted contract.
