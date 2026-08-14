# Next Decision

Last updated: 2026-08-14.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1` and `PE7-RWE-MR-CORPUS-SAMPLING-1` are accepted on main with their source-bound measurement ledger and finite corpus manifest. The human value owner for the measurement-readiness route is Igzela, the repository owner; this session's implementation agent is explicitly delegated to record and execute the remaining T2 value judgments for this route. The current window is `PE7-RWE-MR-OPERATIONS-EVIDENCE-1` `READY_FOR_EXECUTION` for a provider-free operations and evidence contract only.

## Authoritative Forward Order

```text
[window: PE7-RWE-MR-OPERATIONS-EVIDENCE-1 — READY_FOR_EXECUTION, delegated operations and evidence freeze]

→ `PE7-RWE-MR-PROTOCOL-FREEZE-1` after this packet is merged and closed
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-MR-OPERATIONS-EVIDENCE-1` — `READY_FOR_EXECUTION`

## Historical V2 Closeout

**State:** `COMPLETE`

**Evidence:** Disposition `CONTROLLED_FAILURE`; run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; four frozen cells; restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`; no seal and no target-default-branch write. Do not rerun this effect.

## Packet PE7-RWE-MR-OPERATIONS-EVIDENCE-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-RWE-MR-CORPUS-SAMPLING-1` — COMPLETE on accepted main `3f88d985af3f7701ab9f3c382becb84f73364c9b`; PR #445 exact head `2ddec8d5e2afce104ee718d64eb517219ecdf888`; merge `3f88d985af3f7701ab9f3c382becb84f73364c9b`; exact-head `PASS`; canonical workflow `31766911605`; exact-head check `31766911606`.

**Class:** `CONTRACT`

**Outcome:** Freeze the minimum operations/evidence manifest needed to observe reviewer identity, blinding and disagreement, environment and drift, model/price/runner identity, lifecycle cost, reconstructable pre-AC Harness artifacts, and restricted/redacted evidence handling.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md` only. No Provider call, credential access, task execution, persistence schema, runtime, evaluator, scheduler, store, target, or external-effect change.

**Exit:** An independently reviewed `rwe_operations_evidence.v1` manifest that maps every required field to an existing owner/receipt or to an explicit `unavailable` state with a fail-closed stop.

**Stop:** A required field is neither proved by an existing owner/receipt nor explicitly classified `UNAVAILABLE_NOW` with its fail-closed consequence. `UNAVAILABLE_NOW` is a deliberate evidence disposition, not permission to impute, report success, or continue a baseline.

### Operations/evidence manifest — rwe_operations_evidence.v1

1. **Scope and authority.** This is a provider-free contract. The finite population, two repetitions per task, four cells, source commit `6240768506320a324d68787b9eaa86971c8c930c`, source tree `137e912f416a3a8d5be307e91bb2580154fc8fc34c6de52c2441ef3e3f93a064`, corpus binding `044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20`, protocol binding `bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db`, schedule binding `6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38`, and `verified_delivery_points` value basis are inherited unchanged. This manifest grants no spend, execution, output, merge, release, deployment, or adoption authority.
2. **Existing owners.** Frozen RWE identity and schedule remain owned by `engine/src/rwe/operator_corpus.rs`, `engine/src/rwe/corpus.rs`, `engine/src/rwe/economic_protocol.rs`, `engine/src/rwe/execution_schedule.rs`, and versioned `engine/rwe/corpora/` artifacts. Runtime usage, terminal evidence, recovery, and lifecycle receipts remain under the existing RWE coordinator/runner and `LocalProductStore`. Artifact/redaction evidence remains under existing artifact and terminal-evidence owners. PR/CI/review/merge evidence remains under GitHub and `docs/REAL_WORLD_TESTING_PLAYBOOK.md`. No second ledger, retention store, evaluator, reviewer, or controller is introduced.

| Required field(s) | Existing owner or receipt | Missing/ambiguous rule |
|---|---|---|
| accepted main, source repo/commit/tree, task-definition, corpus/protocol/schedule, task/cell/repetition/seed, mutable paths, verifier | `engine/src/rwe/operator_corpus.rs`, `corpus.rs`, `execution_schedule.rs`, versioned `engine/rwe/corpora/` artifacts, accepted-main receipt | mismatch or absent means the cell is unavailable and no replacement is allowed |
| executor, binary version/path, requested model | `engine/src/rwe/operator_corpus.rs` admitted constants; `engine/src/rwe/runner.rs`; `engine/src/rwe/live_baseline_coordinator.rs` | caller assertion cannot fill it; absent or mismatched identity stops preflight |
| runner identity | **`UNAVAILABLE_NOW`:** `engine/src/rwe/runner.rs` is the execution owner but does not provide an accepted per-run identity receipt | the snapshot packet must materialize the exact runner identity; until then `reconstructable=false` and preflight stops |
| provider requests, input/output tokens, latency, resolved model, provider/protocol/endpoint, monetary cost, price source/version | `engine/src/provider/managed_deepseek.rs`, `managed_deepseek_executor.rs`, `engine/src/execution_usage/mod.rs`, and the store-owned journal in `engine/src/storage/local_product_store/managed_acceptance.rs` | missing, conflicting, or untrusted per-cell usage/cost/model/endpoint is `UNAVAILABLE_NOW`; known consumed cost is retained, and unknown is never zero or requested-model inference |
| reviewer policy: `reviewer_identity_class`, `minimum_reviewers`, `blinded`, `permitted_repair`, `measure_review_time`, `disagreement_resolution`, `rubric_sha256` | `engine/rwe/corpora/rwe-minimum-first-corpus/v2/protocol/rwe_economic_protocol.v1.json`, top-level `reviewer_policy` object | mismatch with the frozen protocol binding stops preflight; this maps the accepted policy only and does not prove an actual task/cell review |
| task/cell reviewer acceptance, actual reviewer identity, objection/disagreement, `review_evidence_ref` | **`UNAVAILABLE_NOW`: no accepted task/cell reviewer-evidence owner exists.** `managed_deepseek_review_receipt.v1` at `engine/src/provider/managed_deepseek_executor.rs` and its `managed_acceptance.rs` stage-reader prove only status, objection count/hash, and resolved model; terminal refs and PR review are not task/cell reviewer evidence | `reviewer_acceptance_rate`, actual reviewer identity, explicit disagreement receipt, and `review_evidence_ref` remain unavailable; RWE preflight and observation completeness stop |
| human review duration | **`UNAVAILABLE_NOW`:** `workflow_run_nodes.started_at/completed_at` in `engine/src/storage/local_product_store/managed_acceptance.rs` are execution timestamps, not a human review-time receipt | `review_minutes` is unavailable; do not derive it from node elapsed time |
| `agent_sessions`, `review_cycles`, `repair_iterations`, `ci_runs`, `ci_compute_minutes`, `human_preparation_minutes`, `material_rework_minutes`, `recovery_minutes` | **`UNAVAILABLE_NOW` per field unless an existing per-cell receipt is present.** `implementation_cost_receipt.v1` in `engine/src/rwe/economic_protocol.rs` validates shape but does not create evidence; GitHub/route records are not silently promoted to an RWE lifecycle ledger | lifecycle-cost completeness is unavailable and the baseline cannot pass its cost gate; no second ledger is created |
| terminal, artifact, approval/output, cleanup and recovery refs | `engine/src/storage/local_product_store/product_tasks.rs` `product_task_terminal_evidence.v2` and `engine/src/rwe/live_baseline_coordinator.rs` | missing or untrustworthy binding blocks delivered-success classification |
| runner toolchain and dependency-lockfile digests | **`UNAVAILABLE_NOW`:** accepted checkout lockfiles and `engine/src/rwe/operator_corpus.rs` identity constants do not constitute a per-run reconstruction receipt; the snapshot packet must materialize it | no exact values means `reconstructable=false` and snapshot/preflight stops |
| redacted public evidence | content exclusion/redaction paths in `engine/src/storage/local_product_store/product_tasks.rs`, including `product_task_terminal_evidence.v2` | public projection may expose only the bounded redacted/digest form; raw content is never public |
| restricted raw access, retention, and deletion receipts | **`UNAVAILABLE_NOW`: no accepted RWE raw-bundle access/retention/deletion owner or receipt exists on current main.** Existing supervised-patch retention fields are not RWE policy | no new baseline run, raw copy, or deletion is permitted; historical bundle digests remain preserved |
3. **Identity and drift fields.** Each run/cell must bind the accepted Harness main SHA, task source repository/commit/tree, task-definition SHA, corpus/protocol/schedule artifact bindings, task/cell/repetition/seed, mutable paths, verification command, executor identity, requested model, resolved/returned model, protocol/endpoint identity, runner identity, toolchain, dependency-lockfile digests, and price-source/version. Existing store/usage/terminal evidence is authoritative; caller assertions or worker-recomputed hashes are not. Any preflight or run-time mismatch is `unavailable` and fail-closed, not repaired by substitution.
4. **Reviewer policy.** The accepted task/cell policy is `reviewer_identity_class=operator`, `minimum_reviewers=1`, `blinded=false`, `permitted_repair=one bounded repair cycle`, `measure_review_time=true`, and `disagreement_resolution=record disagreement and fail closed`, bound to rubric sha256 `0e3c4275aacae5ae1eec563ea348135fa05b6719391c526490a7503b497c4e7b`. The current managed receipt is only partial evidence and the missing task/cell fields remain `UNAVAILABLE_NOW`; the PR exact-head receipt is only repository merge evidence and never substitutes for task/cell review evidence. Review transcripts and raw prompts/outputs are not retained in this manifest.
5. **Runtime, model, runner, and price evidence.** Runtime usage and terminal evidence must report provider requests, input/output tokens, latency, recovery, approval/output, and terminal bindings through existing owners. For the accepted v2 schedule, cost authority is a local estimate with operator ceiling, not a provider quote: provider `deepseek`, model `deepseek-v4-flash`, pricing table `deepseek-opencode-2026-08`. A future or changed price source must be bound to its own existing manifest. Requested model identity never substitutes for an ambiguous or missing returned model identity; the latter remains `unavailable`.
6. **Lifecycle-cost completeness.** The required fields are `provider_requests`, `input_tokens`, `output_tokens`, `latency_ms`, `monetary_cost`, `agent_sessions`, `review_cycles`, `repair_iterations`, `ci_runs`, `ci_compute_minutes`, `human_preparation_minutes`, `review_minutes`, `material_rework_minutes`, and `recovery_minutes`. Provider usage/cost fields may be observed through the mapped journal; the other fields are `UNAVAILABLE_NOW` unless an existing per-cell receipt is present. Failed, cancelled, timed-out, killed, recovered, and outcome-unknown attempts retain known consumed cost; unknown fields remain unavailable rather than zero. This packet creates no cost ledger.
7. **Harness reconstruction.** The exact pre-convergence Harness main SHA, versioned RWE artifacts, task source/tree and definitions, and admitted executor/model identity are bound. Toolchain, dependency lockfiles, tracked configuration, runner environment, workflow/CI identity, store schema marker, and complete artifact/terminal reconstruction are `UNAVAILABLE_NOW` until the snapshot packet emits exact receipts. Therefore `reconstructable=false` and no guessed environment or second artifact owner is allowed.
8. **Restricted and redacted evidence.** Never commit, print, or put in public comments raw prompts, model outputs, transcripts, credentials, private paths, or unredacted repository content. The public evidence surface is the bounded redacted/digest-only projection. No accepted current owner/receipt proves restricted raw access, retention, or deletion, so those fields are `UNAVAILABLE_NOW`; this route performs no raw copy or deletion, and historical bundle digests remain preserved.
9. **Failure, recovery, rollback, and verification.** Preserve failed and outcome-unknown evidence, never retry an unknown effect, and use existing cleanup/recovery receipts. Rollback is one documentation revert with evidence retained. Verify the accepted capsule, `scripts/check_agent_handoff.py`, `git diff --check`, the source owner bindings, and the manifest's required-field/unavailable rules before promotion; a provider-free document check is not evidence that a live field exists.
10. **Next action.** Keep the PR Draft while changing; obtain stable-head two-axis PASS, mark Ready once, wait for exact-head canonical CI, manually squash-merge, record the closeout receipt, refresh main, and only then promote `PE7-RWE-MR-PROTOCOL-FREEZE-1`.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-RWE-MR-OPERATIONS-EVIDENCE-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"opencode_local_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Freeze the provider-free RWE operations and evidence manifest without executing a task or effect.","rollback":"Revert the single documentation commit to restore the prior current window while retaining all existing evidence.","allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md"],"allowed_outputs":["The versioned rwe_operations_evidence.v1 manifest and bounded closeout receipt in docs/NEXT_DECISION.md."],"prerequisites":["PE7-RWE-MR-CORPUS-SAMPLING-1"],"prerequisite_receipts":["PR #445 exact head `2ddec8d5e2afce104ee718d64eb517219ecdf888`; merge `3f88d985af3f7701ab9f3c382becb84f73364c9b`; exact-head `PASS`; canonical workflow `31766911605`; exact-head check `31766911606`"],"forbidden_changes":["Any runtime, schema, migration, fixture, Provider, credential, target, evaluator, scheduler, store, or external-effect change.","Do not invent retention, access, cost, model-return, runner, environment, or reconstruction evidence; unavailable remains unavailable."],"forbidden_next_actions":["Do not call a Provider, read credentials, execute a task, or rerun run-live-20260813-v2c.","Do not write a target default branch, execute EFFECT/T3, release, deploy, or merge automatically.","Do not start PE7-RWE-MR-PROTOCOL-FREEZE-1 before this packet is merged and closed."],"ordered_steps":["Read the accepted corpus/estimand bindings and existing RWE, store, artifact, review, CI, and architecture owners.","Record rwe_operations_evidence.v1 with explicit owners and unavailable rules.","Run provider-free handoff checks and prepare the governed Draft PR."],"read_paths":["docs/NEXT_DECISION.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/ARCHITECTURE_BOOK.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","engine/src/rwe/economic_protocol.rs","engine/src/rwe/execution_schedule.rs","engine/src/rwe/live_baseline_coordinator.rs","engine/src/rwe/runner.rs","engine/rwe/corpora/rwe-minimum-first-corpus/v2/protocol/rwe_economic_protocol.v1.json","engine/rwe/corpora/rwe-minimum-first-corpus/v2/schedule/execution_schedule.v1.json"],"verification":["PYTHONPATH=src uv run --no-project python -m unittest tests.test_session_context.CheckpointTests.test_current_repository_packet_binds_safe_live_capsule","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"expected_artifacts":["Versioned rwe_operations_evidence.v1 manifest in docs/NEXT_DECISION.md.","Existing-owner bindings and explicit unavailable/stop rules."],"pause_gates":["Stop before any Provider call, credential access, task execution, target write, EFFECT, T3 action, release, deployment, or automatic merge.","Stop if a required operations/evidence field is neither proved by its existing owner nor explicitly classified UNAVAILABLE_NOW with its fail-closed consequence."]}
-->

## Common Execution Protocol

- Refresh accepted main, the current packet, exact PR heads, CI, review, and ledger receipts before every transition.
- Derive a route action only from the accepted current window, the checked inventory, current-main evidence, and existing durable owners.
- Existing route boundary (quoted for compatibility, not new packet authority): The sole exception is the current packet's dispatch-capsule-authorized, one-per-claim local OpenCode weak-worker Provider invocation; it cannot make the controller read, pass, persist, or report a credential. This packet's external-effect limit is zero and does not use that exception.
- Keep changing PRs Draft; require stable-head independent review and canonical exact-head CI before governed manual merge.
- Treat ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures as bounded recovery transitions through their existing owners.
- Preserve exact receipt bindings and failed/unknown evidence; never convert absence, stale evidence, or an unproven external outcome to success.
- Emergency-stop: revert the current window and retain detailed lifecycle evidence. Authority, evaluator, recovery, and schema remain unchanged.

## Hard Stops

- no Provider call; no credential read, target write, release, deployment, automatic merge, EFFECT execution, or T3 action without its separate exact authority;
- no second controller, ledger, queue, lease, workflow owner, store, scheduler, evaluator, authority, or persistence owner;
- no future-route path/prose, model output, local checkpoint, or candidate PR accepted as current-main authority;
- no stale/ambiguous owner, caller, path, verification, rollback, cleanup, retention, evidence, schema, evaluator, authority, or recovery fact treated as proved;
- no retry of an outcome-unknown effect and no deletion or concealment of failure, pause, repair, or recovery evidence.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is the sole routing-only index. Promotion removes exactly one eligible packet, re-derives every `REFRESH_AT_PROMOTION` field from accepted main, validates the resulting candidate, and independently reviews the routing change. No future sketch, static path, or profile alone authorizes code or an effect.
