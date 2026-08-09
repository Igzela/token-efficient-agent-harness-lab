# Next Decision

Last updated: 2026-08-09.

This document owns only the current executable window: active routing, the common execution contract, and one fully expanded current packet. Accepted truth belongs in `docs/CURRENT_STATUS.md`; long-horizon routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; durable invariants belong in `docs/ARCHITECTURE_BOOK.md`; current owners belong in `docs/MODULE_MAP.md`. Live PR heads, CI, and reviews belong only in a fresh context capsule.

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
- a future route label is not implementation authority. Only a packet satisfying the execution-ready contract below may enter `READY_FOR_EXECUTION`.

This decision changes routing and acceptance gates. It does not authorize a provider call, live experiment, target effect, merge, release, deployment, production adoption, Level-2 controller, or Meta Improver.

## Authoritative Forward Order

The route is stage-ordered. The current packet is expanded below; blocked successors are indexed without execution authority in `docs/FUTURE_ROUTE.md`:

```text
RWE v2 refreeze
→ viability preflight → authorized run → evidence closeout
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
→ Dashboard disposition and presentation refresh last
```

The memory/skill factor experiment is an optional branch after Level-1 evaluation. It is not a Level-2 prerequisite. Adoption and Meta Improver remain independent after final transfer; both must reach an explicit completion disposition before the deferred Dashboard refresh becomes eligible.

No downstream micro-packet starts automatically. Every micro-packet must satisfy its named prerequisite on accepted `main` and its class contract below.

## Active Routing

1. `PE7-RWE-V2-REFREEZE-1` — `READY_FOR_EXECUTION`; this is the only packet that may start now.
2. Every later packet in `docs/FUTURE_ROUTE.md` — `BLOCKED_PREREQUISITE` on its explicit predecessor and, for `EFFECT` packets, separate current operator authority.
3. Dashboard PR #225 — `DEFERRED_LAST`; it is not a shortcut around the route.

## Packet States

- `READY_FOR_EXECUTION` — accepted prerequisites and a complete packet contract permit provider-free implementation.
- `BLOCKED_PREREQUISITE` — a named earlier evidence, implementation, or authority condition is incomplete.
- `DECISION_REQUIRED` — safe direction or authority cannot be derived from accepted owners.
- `IN_PROGRESS` — one current branch/PR owns the packet.
- `COMPLETE` — merged, verified, independently reviewed, and synchronized into accepted documents.

Review `PASS`, PR merge, and packet `COMPLETE` are different states. Exact-head review `PASS` satisfies only the independent-review gate.

## Execution-Readiness Contract

A route label, boundary table, issue, chat handoff, or model-generated implementation plan is not enough to start code. Before a blocked packet becomes `READY_FOR_EXECUTION`, this document must contain, for that exact accepted-main frontier:

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

Only `PE7-RWE-V2-REFREEZE-1` is execution-ready now. Every packet in `docs/FUTURE_ROUTE.md` is routing-only until its exact predecessor is accepted and its complete contract is moved here and refreshed against then-current `main`. An implementation agent must stop `DECISION_REQUIRED` rather than fill a missing architecture, authority, statistical, evaluator, retention, spend, recovery, or adoption decision.

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

A predecessor becoming `COMPLETE` does not mechanically make its successor executable. Before changing a successor to `READY_FOR_EXECUTION`, the planning owner must refresh that block against accepted current `main` and replace every routing-level abstraction with exact evidence identities, owner/allowed paths, frozen interfaces/fields, tests, rollback, and any required human or operator gate. If the accepted predecessor ended `NO_GO`, `DECLINE`, `DEFER`, `SATURATED`, `HARM`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT`, synchronize and rewrite the route before selecting any successor. Do not walk the nominal GO path merely because a prerequisite packet closed.

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
- beginning a routing-only packet from its summary boundary without an accepted execution-ready expansion;
- claiming learning, open-ended evolution, or recursive self-improvement without the separately required evidence.

## Packet PE7-RWE-V2-REFREEZE-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** active runtime/code baseline `ee43eac853644266614da09de764a3bf19f2d281` from accepted PR #369, plus the current accepted documentation head descending from it without relevant code changes; accepted Decision A timeout repair (#368); accepted Decision C calibration mechanism (#369); operator-reported 8192 calibration and local candidate must be independently reconciled before acceptance.

**Execution class:** bounded mechanical contract migration plus deterministic test-race repair.

### Outcome

Land a versioned v2 frozen RWE contract whose only experimental change is the compatibility-required output envelope, and eliminate the independently reproduced HTTP-server test environment race by making every writer of the same environment families serialize on the existing canonical locks.

This packet authorizes no provider request, live 4-cell run, target effect, schema migration, or new authority owner.

### Evidence-reconciliation slice

Treat the current handoff as a claim until bound evidence is observed:

- calibration reportedly selected 8192 as the first viable parseable envelope;
- the reported redacted bundle is under `/tmp/opencode/rwe-calibration-evidence/`;
- a reported local candidate contains the v2 refreeze and canonical-lock repair;
- reported checks are `test_http_server` 206 passed ×4, full engine 1692 passed/1 ignored, fmt and clippy clean.

Before accepting the candidate:

1. verify the redacted calibration record binds accepted main, `deepseek-v4-flash`, candidates 8192 then 16384, first-viable semantics, no more than two requests, authorized timeout, token/latency/cost/finish-reason fields, request IDs, parseable content, and canonical external provenance;
2. verify the record contains no prompt, output text, credential, private path, or unredacted repository content;
3. record a SHA-256 for the restricted raw bundle and a redacted summary in the PR evidence; do not commit sensitive raw evidence;
4. inspect the actual candidate diff. If it is unavailable, reconstruct from accepted main; never infer file contents from the handoff;
5. if 8192 is not independently supported, stop `DECISION_REQUIRED`. Do not silently choose 16384 or run another paid calibration.

### Allowed semantic delta

The v2 contract changes exactly:

| Field | v1 | v2 |
|---|---:|---:|
| `per_task_max_output_tokens` | 4,000 | 8,192 |
| `per_task_max_total_tokens` | 16,000 | 20,192 |
| four-cell run token ceiling | 64,000 | 80,768 |

Version identifiers, dependent per-cell/run totals, corpus/protocol/schedule hashes, freeze-point binding, and authorization bindings must change only where mechanically implied by that delta.

The following remain identical to v1: target repository and source commit/tree, two task objectives and task bodies, allowed paths, verifier, reviewer policy, repetitions, seeds, budget-point count, request count, task wall time, implementer model, planner/reviewer model, stop rules, statistical method, non-inferiority margins, monetary ceilings, Draft-PR-only output, and no-auto-merge policy.

### Allowed paths

- new `engine/rwe/corpora/rwe-minimum-first-corpus/v2/**` artifacts;
- `engine/src/rwe/operator_corpus.rs` for explicit version selection, v1/v2 constants, freeze-point and expected hash locks;
- `engine/tests/test_http_server.rs` for delegating the four duplicated environment-lock helpers to `http_server/common.rs`;
- `engine/tests/http_server/auth.rs` for the two baseline auth tests to hold the canonical provider environment lock;
- `tools/test_run_rust_tests.py` for the runner contract test that verifies the complete lock set enables parallel execution while partial or missing locks remain serial;
- `docs/CURRENT_STATUS.md` and `docs/NEXT_DECISION.md` for exact-head status synchronization after acceptance.

`docs/MODULE_MAP.md` changes only if the real owner path changes; a new version under the same owner is not an owner change. Any need to edit production scheduler, store, authorization, provider, evaluator, schema, migration, SDK, Dashboard, or target-output code is `DECISION_REQUIRED`.

### Test-race repair boundary

- `provider_cli_env_lock`, `auto_adjustment_env_lock`, `adaptive_operator_env_lock`, and `target_repo_output_env_lock` in `test_http_server.rs` delegate to the existing `http_server/common.rs` statics; they must not retain independent `OnceLock<Mutex<()>>` instances.
- `product_golden_path_env_lock` remains local because it has no canonical twin.
- The two auth baseline tests hold the same canonical provider lock before reading or writing related environment state.
- Remove diagnostics added solely for root-cause discovery and restore the original dispatch behavior.
- `engine/tests/http_server/tick.rs` is currently unwired dead code. Do not edit or delete it in this packet.
- Do not introduce a process-global test-serialization mechanism or force all tests single-threaded.

### Ordered implementation slices

1. Reconcile the calibration receipt and candidate against accepted main.
2. Copy v1 artifacts to a distinct v2 root; prove v1 bytes are unchanged.
3. Apply the three numeric budget changes and only mechanically dependent version/hash/binding changes.
4. Add deterministic v1 and v2 load/freeze/hash tests and a whitelist test for the semantic delta.
5. Canonicalize the four duplicated HTTP-server test locks and retain the two auth lock acquisitions.
6. Remove diagnostics, run the verification matrix, then synchronize only the two canonical documents.

### Verification and exit gate

Required local evidence:

- byte-for-byte v1 tree equality against accepted main;
- recomputation of v2 corpus, protocol, and schedule hashes from canonical bodies;
- a machine-readable whitelist diff proving no non-compatibility experiment field changed;
- focused v1/v2 freeze, tamper, schedule-total, authorization-binding, and replay-determinism tests;
- `cargo test -p engine --test test_http_server` green in four consecutive default-parallel runs;
- `cargo test -p engine`;
- `cargo fmt --all -- --check`;
- `cargo clippy -p engine --all-targets --all-features -- -D warnings`;
- applicable PostgreSQL parity tests if any store/authorization path changed; otherwise the diff must prove those owners were untouched;
- `uv run --no-project python tools/check_security_baseline.py`;
- `uv run --no-project python scripts/check_agent_handoff.py`;
- `git diff --check`.

Exit requires one stable exact head, complete-diff independent `PASS`, canonical exact-head CI success, no open blockers, v1 unchanged, verified redacted calibration evidence, and a rollback statement. Rollback reverts the packet and returns the active freeze to v1; prior failed v1 evidence remains valid.

After merge, the planning owner may refresh `PE7-RWE-V2-VIABILITY-PREFLIGHT-1` from `docs/FUTURE_ROUTE.md` into this current window. Merge does not mechanically activate it and does not authorize a live run; `PE7-RWE-V2-VIABILITY-RUN-1` remains separately blocked.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` preserves the accepted long-horizon order and routing-only packet sketches. It cannot authorize implementation, Provider effects, promotion, merge, release, or deployment. Promotion requires removing exactly one eligible packet from that document, expanding it here against accepted current `main`, and independently reviewing the resulting routing change.
