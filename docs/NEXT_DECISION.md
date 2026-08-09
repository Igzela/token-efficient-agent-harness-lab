# Next Decision

Last updated: 2026-08-09.

This document owns only the current executable window: active routing, the common execution contract, and one fully expanded current packet. Accepted truth belongs in `docs/CURRENT_STATUS.md`; long-horizon routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; durable invariants belong in `docs/ARCHITECTURE_BOOK.md`; current owners belong in `docs/MODULE_MAP.md`. Live PR heads, CI, and reviews belong only in a fresh context capsule.

## Current Direction

The repository optimizes one outcome:

> Under non-negotiable quality, safety, traceability, compatibility, recovery, and rollback constraints, increase verifiable and reusable task delivery per unit of total lifecycle cost.

Quality, authority, evidence integrity, compatibility, recovery, and rollback are hard gates. Token use, monetary cost, latency, accepted delivery, engineering effort, maintenance surface, and reuse are optimization evidence only after those gates pass.

The accepted route is **bounded recursive Harness optimization**, not open-ended evolution or general recursive self-improvement. Candidate generation, experimental-parent selection, production adoption, and improvement-operator research remain separate authorities.

The following refinements are accepted as of 2026-08-09:

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

The core trunk is stage-ordered. The current packet is expanded below; blocked successors are indexed without execution authority in `docs/FUTURE_ROUTE.md`:

```text
current: viability preflight
→ separately authorized viability run → evidence closeout
→ measurement estimands → corpus/sample → operations/evidence → protocol freeze
→ decision-baseline snapshot → preflight → run → analysis
→ AC0 inventory/freeze → AC1-AC6 contract/core/migration → AC7 cleanup/closeout
→ contemporary old/new replay → EC1-EC5 controls
→ Level-1 generation/closeout → sealed Level-1 transfer
→ Level-2 evidence audit → human GO/NO-GO
→ [GO only] bounded Level-2 controller/pilot → final sealed transfer
   ├─→ human adoption readiness → human disposition ─┐
   └─→ fixed-operator Meta protocol/run/replication → disposition ─┤
                                                                  └─→ Dashboard disposition/refresh/closeout

optional after Level-1: memory-only/skill-only factor branch (never a Level-2 prerequisite)
optional after META_SUPPORTED + separate human GO: R4 and/or R5 sibling research
optional after explicit R4+R5 dispositions + separate human GO: one R6 outer-policy family
```

The Dashboard join depends only on explicit adoption and fixed-operator Meta dispositions; R4-R6 never delay it. AC6 Dashboard data-projection migration belongs to schema convergence and is not the final presentation refresh. A negative or insufficient result closes its tested branch and forces route synchronization; it does not silently walk the nominal GO arrows.

No downstream micro-packet starts automatically. Every micro-packet must satisfy its named prerequisite on accepted `main`, its class contract below, and the weak-agent promotion/stop-resume contract in `docs/FUTURE_ROUTE.md`.

## Active Routing

1. `PE7-RWE-V2-VIABILITY-PREFLIGHT-1` — `READY_FOR_EXECUTION`; this is the only packet that may start now, and it is provider-free.
2. Every packet remaining in `docs/FUTURE_ROUTE.md` — `BLOCKED_PREREQUISITE`; every `EFFECT` additionally needs a fresh finite T3 operator authority.
3. Dashboard PR #225 — `DEFERRED_LAST`; it is not a shortcut around either the accepted trunk or the adoption/Meta join.

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

Only `PE7-RWE-V2-VIABILITY-PREFLIGHT-1` is execution-ready now. Every packet in `docs/FUTURE_ROUTE.md` is routing-only until its exact predecessor is accepted and its complete contract is moved here and refreshed against then-current `main`. An implementation agent must stop `DECISION_REQUIRED` rather than fill a missing architecture, authority, statistical, evaluator, retention, spend, recovery, or adoption decision.

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

## Packet PE7-RWE-V2-VIABILITY-PREFLIGHT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-RWE-V2-REFREEZE-1` is `COMPLETE` in `docs/CURRENT_STATUS.md`: PR #370 exact head `36c92b93975366c3f85471f247a3afb128e5351c`, merge `3b4afb3e5ab4254904aa5a63473ab6ae0eac1e82`, exact-head `PASS`, canonical workflow `31312135471`, and bound calibration digests.

**Class:** `CONTRACT`

**Readiness:** execution-ready for provider-free preparation and preflight; T1 may execute deterministic steps, T2 accepts the zero-Provider/target/RWE-authority evidence package with its disclosed authentication-bookkeeping write, and T3 authority is neither required nor permitted inside this packet.

### 1. Outcome and non-goals

Produce two hash-bound artifacts for the exact accepted v2 freeze:

1. a fresh `rwe_operator_preflight.v1` receipt from the existing `rwe-live-baseline preflight` command; and
2. an operator-readable **unsigned, not-issued** one-use authorization request package containing only bounded identities, frozen hashes/ceilings, proposed run/authorization IDs and expiry, evidence destinations, and stop rules.

This packet does not change code, corpus, protocol, schedule, schema, RWE authority/run/task/evidence state, evaluator, budget, Provider/model, target repository, or active Harness. The CLI authenticates before preflight and the existing authentication owner persistently updates `api_key_metadata.last_used_at`; that bounded audit-bookkeeping write is expected, must be disclosed in the package, and is the only permitted store mutation. The packet does not call a Provider, issue/admit/consume/revoke an authorization, acquire a run lease, execute a cell, write a target, open a PR in the target, rerun calibration, or authorize the successor.

### 2. Accepted identities and immutable bindings

The worker independently re-reads these from accepted source and requires the preflight receipt to match:

| Binding | Required value |
|---|---|
| v2 frozen Harness main | `ee43eac853644266614da09de764a3bf19f2d281` |
| v2 corpus SHA-256 | `044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20` |
| v2 protocol SHA-256 | `bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db` |
| v2 schedule SHA-256 | `6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38` |
| schedule shape | exactly four cells; 12 Provider requests and 80,768 total tokens at the frozen run ceiling |
| CLI/preflight owner | `engine/src/bin/rwe_live_baseline.rs` → `operator_preflight` in `engine/src/rwe/live_baseline_coordinator.rs` |
| authority owner | LocalProductStore `engine/src/storage/local_product_store/rwe_authority.rs`; no authority is issued in this packet |

The fresh context capsule's accepted repository main may be a documentation descendant of the frozen Harness main. Record both identities; never replace the frozen binding with the current docs head or treat the older frozen SHA as a stale checkout.

### 3. Required operator inputs and pause boundary

The operator supplies these through the current non-committed session without printing their values: existing LocalProductStore path/backend identity, tenant ID, operator key **ID** (not secret), same-tenant completed Golden Path prerequisite ProductTask ID, a restricted receipt destination, logical evidence ID, proposed run/authorization IDs, and expiry. The parent environment must contain the existing `DEEPSEEK_API_KEY` credential symbol because the preflight tests presence, but the worker must never read, echo, hash, persist, or pass its value on a command line.

Before the CLI command, T2/operator evidence must also state that the store is already at the accepted schema, backup/recovery is current, no conflicting RWE run/lease is active, and any extant RWE authorization IDs/statuses are reconciled. The current CLI cannot enumerate unknown active authorities; a weak agent must not compensate with direct SQL. If this store-owner inventory receipt is absent, inconsistent, or requires a new read API, stop `DECISION_REQUIRED` after completing all other preparation.

### 4. Owners, allowed paths, and forbidden changes

Read-only owner paths:

- `engine/src/rwe/operator_corpus.rs`;
- `engine/src/rwe/live_baseline_coordinator.rs`;
- `engine/src/rwe/runner.rs`;
- `engine/src/rwe/execution_schedule.rs`;
- `engine/src/storage/local_product_store/rwe_authority.rs`;
- `engine/src/storage/local_product_store/managed_acceptance.rs` and `engine/src/storage/local_product_store/keys.rs` for the authentication bookkeeping call path;
- `engine/src/bin/rwe_live_baseline.rs`;
- `engine/rwe/corpora/rwe-minimum-first-corpus/v2/**`;
- focused tests for those owners.

Writable output is limited to a new mode-`0600` file at the operator-provided restricted destination, a separately redacted receipt/request package containing only approved fields and digests, the store-owned `api_key_metadata.last_used_at` authentication-bookkeeping update, and—after independent acceptance—`docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md` for one frontier transition. No other store state and no Rust, schema, migration, SDK, Dashboard, workflow, runbook, target, or raw-evidence file is writable. Any code repair, new status/list API, schema migration, or owner change is a separate `DECISION_REQUIRED` packet.

### 5. Ordered weak-agent procedure

1. Refresh remote accepted `main`; generate a fresh capsule; verify the accepted refreeze receipt and no overlapping current packet/PR.
2. Recompute/read the four immutable identities above from accepted source/artifacts and verify v1 remains present and distinct.
3. Validate the non-secret operator input manifest by presence and format only. Do not render values into logs, prompts, shell history, Git, or chat.
4. Run focused provider-free tests and build the existing CLI. A failure is evidence; do not patch code inside this packet.
5. Acquire and validate the T2/operator store-inventory receipt. Stop before live-store access if it is absent, the backend/path is ambiguous, schema migration would occur, backup is stale, or an authority/lease is unresolved.
6. Run `preflight` **without** `--authorization-id`; never invoke the adjacent `admit` or `run` subcommands. Capture stdout directly into the new restricted file with shell tracing disabled, `umask 077`, and no-clobber enabled.
7. Validate the single JSON receipt locally, compute its digest from stdin so no private path is printed, and scan the redacted projection for forbidden content. Do not rerun merely to prove determinism: every invocation performs another authentication-bookkeeping update.
8. Build the unsigned request package with `state=NOT_ISSUED`, `authority_consumed=false`, `external_effect_count=0`, and `known_store_mutation=api_key_metadata.last_used_at`; bind only receipt/evidence digests, never the restricted path or credential.
9. T2 independently reviews the package and exact evidence. On acceptance, synchronize the three canonical route documents; do not issue authority or begin the run.

### 6. Command template

The worker first runs:

```bash
cargo test -p engine preflight_fails_closed_without_gp_and_without_consuming
cargo test -p engine operator_corpus
cargo build -p engine --bin rwe_live_baseline
```

After the input and store-owner gates pass, with shell tracing disabled and the named variables already set outside Git:

```bash
(
  umask 077
  set -o noclobber
  preflight_args=(
    --db-path "$ACP_RWE_STORE_PATH"
    --tenant-id "$ACP_RWE_TENANT_ID"
    --operator-key-id "$ACP_RWE_OPERATOR_KEY_ID"
    preflight
    --golden-path-prerequisite-product-task-id "$ACP_RWE_GOLDEN_PATH_PRODUCT_TASK_ID"
  )
  cargo run -p engine --bin rwe_live_baseline -- "${preflight_args[@]}" \
    > "$ACP_RWE_PREFLIGHT_RECEIPT"
)
```

Do not unset or override `CI`; the preflight must fail closed when `CI=true`. Do not add `--authorization-id`. Do not run `admit` or `run`. Never use `set -x`, print the environment, or place a raw credential in a variable whose value is written to evidence.

### 7. Receipt verification

The restricted JSON must parse and prove all of the following: `ready=true`; `schema_version=rwe_operator_preflight.v1`; `provider_call_performed=false`; `target_write_performed=false`; `authority_consumed=false`; `live_baseline_sealed=false`; `golden_path_prerequisite_ready=true`; `credential_symbol_present=true`; `authorization=null`; `blockers=[]`; exact frozen hashes/target/provider/model/binary identities; and `cell_count=4`. The redacted package additionally proves the fresh capsule SHA, store-backend logical identity, inventory-receipt digest, input-manifest digest, receipt SHA-256, evidence retention/access class, proposed expiry/ceilings, all stop rules, and the expected `api_key_metadata.last_used_at` authentication bookkeeping. The CLI receipt does not prove total store immutability; T2 must reconcile the disclosed bookkeeping path against the store-owner inventory receipt and reject any other mutation.

The verifier rejects any credential value, raw prompt/output/transcript, private path, unredacted repository content, mutable caller-supplied freeze value, missing/zeroed cost ceiling, active/consumed authorization claim, or assertion that preflight authorizes the run.

### 8. Failure taxonomy and stop/resume

Use exactly one primary disposition:

- `INPUT_MISSING` — a required non-secret identifier/destination is absent;
- `STALE_FRONTIER` — main, packet, prerequisite receipt, corpus, protocol, or schedule identity changed;
- `STORE_NOT_READY` — backend/schema/backup/inventory/prerequisite task is not accepted-current;
- `AUTHORITY_OR_LEASE_CONFLICT` — an extant authority/run/lease is unresolved;
- `PREFLIGHT_BLOCKED` — the CLI returns `ready=false` with preserved blocker codes;
- `IDENTITY_MISMATCH` — a receipt binding differs from the accepted table;
- `EVIDENCE_INVALID` — write, parse, digest, redaction, retention, or disclosed-store-mutation reconciliation fails;
- `READY_ZERO_EXTERNAL_EFFECT` — and only this disposition is eligible for T2 acceptance.

On every non-ready disposition, preserve the zero-Provider/target/RWE-authority proof, disclose the authentication-bookkeeping update, retain the bounded blocker receipt, perform no speculative retry, and emit the common weak-agent handoff. Resume only after a named owner supplies a fresh replacement receipt/input and main/store identities are revalidated. If any external effect or authority consumption is observed or cannot be disproved, classify `OUTCOME_UNKNOWN`, stop immediately, and do not use this packet's rollback or retry path.

### 9. Verification and exit gate

Required evidence:

- the three focused commands above pass on the accepted checkout;
- exact v2 hash/cell/budget assertions and v1/v2 distinctness pass;
- store-owner inventory and recovery receipts are fresh and hash-bound;
- one provider-free preflight receipt proves zero Provider/target effects and zero RWE authority consumption, while T2 evidence discloses and reconciles only the expected authentication-bookkeeping write;
- secret/sensitive-content scan and redacted/restricted digest reconciliation pass;
- `uv run --no-project python tools/check_security_baseline.py`;
- `uv run --no-project python scripts/check_agent_handoff.py`;
- `git diff --check` for any canonical-doc closeout diff;
- stable-head independent `PASS` and applicable canonical exact-head CI for the closeout PR.

Exit is one accepted `READY_ZERO_EXTERNAL_EFFECT` receipt plus unsigned `NOT_ISSUED` request package, exact evidence digests, the disclosed authentication-bookkeeping mutation, a rollback statement, and canonical synchronization. A merely runnable CLI, `ready=true` without the inventory/recovery/redaction evidence, or a Draft/fast-check head is not completion.

### 10. Compatibility, rollback, and next actions

Compatibility preserves v1 byte-identically and selected v2 as the active frozen operator contract. Normal rollback deletes or archives the unaccepted unsigned request package according to its retention class, reverts only canonical-doc closeout changes, and leaves authority, run, Provider, target, and frozen artifacts unchanged. Retain the expected `api_key_metadata.last_used_at` update as audit bookkeeping; do not falsify history by rolling it back. Any other store mutation is a hard incident, not a normal rollback success.

After accepted completion, the planning owner may promote `PE7-RWE-V2-VIABILITY-RUN-1` from `docs/FUTURE_ROUTE.md` into this window. The next packet remains blocked until a **new** immediate preflight and a separate finite T3 one-use authorization bind the exact run. Forbidden next actions are issuing/admitting authority inside this packet, reusing the preflight as spend permission, calling the Provider, executing any cell, repairing code, rerunning calibration, starting measurement readiness/AC0, or changing Dashboard/adoption/Meta state.

### 11. Weak-Agent Dispatch Capsule

This machine-readable capsule is the deterministic input for the existing Issue lane or a directly supervised T0/T1 worker. It is not an `agent-orchestrator-plan:v1` claim, does not activate the deferred plan lane, and grants no authority. `private_paths_allowed=false` means private paths may not enter the capsule, logs, chat, Git, or redacted evidence; the separately supplied restricted destination remains usable under the packet procedure.

<!-- weak-agent-dispatch:v1
{
  "accepted_binding_source": "Fresh project_context capsule plus CURRENT_STATUS accepted receipt for PE7-RWE-V2-REFREEZE-1",
  "allowed_paths": [
    "docs/CURRENT_STATUS.md",
    "docs/FUTURE_ROUTE.md",
    "docs/NEXT_DECISION.md"
  ],
  "allowed_outputs": [
    "One new mode-0600 raw preflight receipt at the operator-supplied restricted destination",
    "One redacted unsigned NOT_ISSUED request package containing only approved identities and digests",
    "CURRENT_STATUS, NEXT_DECISION, and FUTURE_ROUTE only after T2 accepts the complete evidence"
  ],
  "authority_consumption_allowed": false,
  "dispatch_lane": "issue_or_direct_agent_only",
  "expected_artifacts": [
    "rwe_operator_preflight.v1 restricted receipt and SHA-256",
    "Redacted unsigned authorization request with READY_ZERO_EXTERNAL_EFFECT disposition",
    "Bounded weak-agent handoff with verification, rollback, and forbidden-next-action fields"
  ],
  "external_effect_limit": 0,
  "forbidden_changes": [
    "No Rust, schema, migration, corpus, protocol, schedule, evaluator, budget, Provider, model, target, workflow, runbook, Dashboard, adoption, or Meta change",
    "No authority issue, admit, consume, revoke, run lease, cell execution, Provider call, target write, or target PR",
    "No direct SQL, invented store inventory, second preflight for determinism, code repair, calibration rerun, or successor activation"
  ],
  "forbidden_next_actions": [
    "Do not issue or admit RWE authority",
    "Do not call the Provider or execute a schedule cell",
    "Do not promote the run packet until this packet is independently accepted and a new immediate preflight plus separate T3 authority exist"
  ],
  "goal": "Produce one exact-freeze provider-free preflight receipt and one unsigned, not-issued authorization request while exposing the sole authentication-bookkeeping mutation.",
  "known_store_mutations": [
    "Existing authentication owner updates api_key_metadata.last_used_at exactly once for the single preflight invocation; retain it as audit bookkeeping and reject every other store mutation"
  ],
  "ordered_steps": [
    "Refresh accepted main and capsule; prove the PE7-RWE-V2-REFREEZE-1 accepted receipt",
    "Recompute frozen main, corpus, protocol, schedule, four-cell, request, and token-ceiling identities",
    "Validate non-secret input presence and format without rendering values",
    "Run the three focused provider-free commands and stop rather than repair any failure",
    "Acquire the T2 store inventory, schema, recovery, prerequisite, authority, and lease receipt",
    "Invoke preflight exactly once without authorization-id using no-clobber mode-0600 capture",
    "Validate and digest the restricted JSON from stdin; redact and scan the approved projection",
    "Build the unsigned request with zero external effects, zero authority consumption, and the disclosed last-used bookkeeping write",
    "Pause for independent T2 acceptance; only then synchronize the three canonical route documents"
  ],
  "packet_id": "PE7-RWE-V2-VIABILITY-PREFLIGHT-1",
  "pause_gates": [
    "Pause before live-store access when operator input or T2 inventory and recovery evidence is missing or ambiguous",
    "Stop DECISION_REQUIRED for code repair, a new read API, schema migration, ownership conflict, or any mutation beyond last-used bookkeeping",
    "Stop OUTCOME_UNKNOWN when Provider, target, authority, or other external effect cannot be disproved"
  ],
  "plan_lane_state": "plan_lane_deferred_until_terminal_owners",
  "prerequisites": [
    "PE7-RWE-V2-REFREEZE-1 has the exact accepted receipt bound below",
    "Fresh accepted-main context capsule has no overlapping current packet or owned PR",
    "T2 supplies a fresh store schema, backup, inventory, Golden Path prerequisite, authority, and lease receipt before live-store access"
  ],
  "prerequisite_receipts": [
    "PE7-RWE-V2-REFREEZE-1 COMPLETE: exact head 36c92b93975366c3f85471f247a3afb128e5351c, merge 3b4afb3e5ab4254904aa5a63473ab6ae0eac1e82, canonical workflow 31312135471, exact-head PASS, and bound calibration digests"
  ],
  "private_paths_allowed": false,
  "read_paths": [
    "START_HERE.md and the current canonical status, decision, module-map, architecture, and RWE playbook sections",
    "engine/src/rwe/operator_corpus.rs",
    "engine/src/rwe/live_baseline_coordinator.rs",
    "engine/src/rwe/runner.rs",
    "engine/src/rwe/execution_schedule.rs",
    "engine/src/storage/local_product_store/rwe_authority.rs",
    "engine/src/storage/local_product_store/managed_acceptance.rs",
    "engine/src/storage/local_product_store/keys.rs",
    "engine/src/bin/rwe_live_baseline.rs",
    "engine/rwe/corpora/rwe-minimum-first-corpus/v2 and focused owner tests"
  ],
  "rollback": "Archive or delete only the unaccepted unsigned package under its retention rule, revert only canonical closeout prose, retain the last-used audit update, and treat any other store mutation as an incident.",
  "schema_version": "weak_agent_dispatch.v1",
  "secret_values_allowed": false,
  "verification": [
    "cargo test -p engine preflight_fails_closed_without_gp_and_without_consuming",
    "cargo test -p engine operator_corpus",
    "cargo build -p engine --bin rwe_live_baseline",
    "Validate exact v2 hashes, four cells, 12 requests, 80768 tokens, and v1/v2 distinctness",
    "Reconcile one restricted receipt, redacted digest, inventory digest, and the disclosed authentication bookkeeping",
    "uv run --no-project python tools/check_security_baseline.py",
    "uv run --no-project python scripts/check_agent_handoff.py",
    "git diff --check plus stable-head independent PASS and applicable canonical exact-head CI"
  ],
  "worker_tier": "T1 deterministic preparation and execution; T2 store gate and evidence acceptance"
}
-->

## Future Route Boundary

`docs/FUTURE_ROUTE.md` preserves the accepted long-horizon order and routing-only packet sketches. It cannot authorize implementation, Provider effects, promotion, merge, release, or deployment. Promotion requires removing exactly one eligible packet from that document, expanding it here against accepted current `main`, and independently reviewing the resulting routing change.
