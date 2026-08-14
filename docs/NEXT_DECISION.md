# Next Decision

Last updated: 2026-08-14.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. The human value owner for `PE7-RWE-MR-ESTIMANDS-1` is Igzela, the repository owner; this session's implementation agent is explicitly delegated to record and execute that decision. The current window is `PE7-RWE-MR-ESTIMANDS-1` `READY_FOR_EXECUTION` for a provider-free estimand contract only.

## Authoritative Forward Order

```text
[window: PE7-RWE-MR-ESTIMANDS-1 — READY_FOR_EXECUTION, delegated estimand freeze]

→ `PE7-RWE-MR-CORPUS-SAMPLING-1` after this packet is merged and closed
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-MR-ESTIMANDS-1` — `READY_FOR_EXECUTION`

## Completed (PE7-RWE-V2-VIABILITY-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** Disposition `CONTROLLED_FAILURE`. Run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; cells `cell-rwe-minimum-t1-fix_flow_linkage-r1-bp-standard-s2026080601`, `cell-rwe-minimum-t1-fix_flow_linkage-r2-bp-standard-s2026080602`, `cell-rwe-minimum-t2-draft_contract_tests-r1-bp-standard-s2026080601`, `cell-rwe-minimum-t2-draft_contract_tests-r2-bp-standard-s2026080602`. Restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`. No seal; no target-default-branch write. Promotion PR #442 exact head `50e18540f40a8d47c384f2cac74683618f93c273`; merge `8c5c2f85bc5d66c08d730b7d0c69d914af19540c`; canonical workflow `31710478692`.

## Packet PE7-RWE-MR-ESTIMANDS-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-RWE-V2-VIABILITY-CLOSEOUT-1

**Class:** `CONTRACT`

**Outcome:** Freeze the measurement-readiness estimand ledger using the accepted v2 economic protocol and the explicitly delegated human value decision below. Do not infer a threshold from the controlled failure.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md` only. No runtime, schema, test-fixture, Provider, target, or external-effect change.

**Exit:** An independently reviewed versioned estimand ledger with every threshold source, uncertainty target, human value judgment, missingness rule, owner, and rollback recorded; then promote exactly `PE7-RWE-MR-CORPUS-SAMPLING-1` only after this packet is merged and closed.

**Stop:** Inventing an additional minimum meaningful effect, choosing a threshold from the observed `controlled_failure` direction, treating a protocol margin as a complete ledger without its source, or changing a hard gate into a soft score.

### Decision record — `measurement_estimands.v1`

**Decision owner:** Igzela (human repository owner). The implementation agent is delegated to write this record, perform the provider-free verification, and prepare the governed Draft PR. This delegation does not grant Provider spend, target-write, merge, release, deployment, EFFECT, or T3 authority.

**Decision question:** Is the candidate no worse than the baseline on verified delivery and acceptance outcomes, within the accepted non-inferiority margins, while preserving the hard safety gates and recovery behavior?

**Inferential unit:** task. Repetitions are nested measurements within task, with `minimum_repetitions_per_task=2`. The controlled-failure run `run-live-20260813-v2c` is not used to infer or tune a threshold.

**Primary value basis:** `verified_delivery_points`, using the existing protocol scale. No new conversion, normalization, or scalar value basis is introduced.

**Threshold and uncertainty source:** accepted `PE7-RWE-V2-REFREEZE-1` (PR #370 merge `3b4afb3e5ab4254904aa5a63473ab6ae0eac1e82`) and `engine/rwe/corpora/rwe-minimum-first-corpus/v2/protocol/rwe_economic_protocol.v1.json`, `protocol_id=rwe-minimum-first-protocol-v2`. The binding is protocol hash `bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db`, corpus hash `044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20`, schedule hash `6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38`, and frozen artifact main `ee43eac853644266614da09de764a3bf19f2d281`. The paired-bootstrap method is `paired-bootstrap-95` with method hash `0942b62fb4b864332bef8fa95d149cc59718d13428a120f3559672f8f00b6c63`. These are source bindings, not new thresholds.

**Nested-repetition aggregation:** for each condition, the task-level value is the arithmetic mean of that task's registered terminal repetitions. Fewer than the registered minimum of two repetitions makes that task-level estimand unavailable; no partial repetition is promoted to a task success.

| Estimand | Task-level definition | Non-inferiority direction |
|---|---|---|
| Verified delivery | Per-task repeated-measure value on `verified_delivery_points` | Candidate minus baseline lower 95% bound must be at least `-0.10` |
| Machine verification acceptance | Existing machine-verification pass outcome for the task | Candidate minus baseline lower 95% bound must be at least `-0.10` |
| Reviewer acceptance | Existing independent reviewer acceptance outcome for the task | Candidate minus baseline lower 95% bound must be at least `-0.10` |
| Recovery failure | Existing terminal recovery-failure outcome for the task | Candidate minus baseline upper 95% bound must be at most `+0.05` |

The uncertainty interval is a paired bootstrap 95% interval. Resampling is at the task level and retains all nested repetitions and the candidate/baseline pairing within each task. No additional minimum meaningful effect is defined.

**Hard gates:** machine verification must pass; output must remain Draft-only; no target default branch may be written; and unknown or outcome-unknown must never be counted as success. A hard-gate violation makes the unit or run ineligible rather than lowering its score.

**Missing and unknown:** missing, unavailable, and outcome-unknown values remain unavailable. They are not imputed, converted to zero, counted as success, silently dropped from the registered task set, or retried after an unknown effect. An estimand with unavailable required evidence cannot produce a passing decision.

### Execution contract

- **Owner and scope:** extend only the existing RWE corpus/protocol/artifact/evidence/review document owners. This packet records the contract; it does not add a store, evaluator, scheduler, schema, or runtime owner.
- **Authority and budget:** provider-free documentation and verification only; external-effect limit is zero. The accepted v2 closeout is prerequisite evidence, not permission to rerun it.
- **Ordered steps:** bind the accepted prerequisite; record `measurement_estimands.v1`; check that every value and hard gate above is represented; run the declared checks; close this packet in its Draft PR; and only then promote the single corpus/sampling successor.
- **Compatibility:** no Rust, TypeScript, Python runtime, wire, database, migration, or fixture behavior changes. Existing protocol margins remain the source for the four stated margins.
- **Rollback:** revert the packet's documentation commit to restore the parked state; retain all prior controlled-failure and closeout evidence; do not delete or rewrite evidence.
- **Evidence:** the PR exact head, stable-head two-axis review receipt, canonical exact-head CI, merge commit, and closeout receipt are required before successor promotion.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-RWE-MR-ESTIMANDS-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"opencode_local_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Freeze the delegated provider-free RWE measurement estimand contract.","rollback":"Revert the single documentation commit to restore the parked estimand window while retaining all prior failure and closeout evidence.","allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md"],"allowed_outputs":["The versioned measurement_estimands.v1 ledger and its bounded closeout receipt in docs/NEXT_DECISION.md."],"prerequisites":["PE7-RWE-V2-VIABILITY-CLOSEOUT-1"],"prerequisite_receipts":["PE7-RWE-V2-VIABILITY-CLOSEOUT-1 COMPLETE: Disposition `CONTROLLED_FAILURE`; run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; cells `cell-rwe-minimum-t1-fix_flow_linkage-r1-bp-standard-s2026080601`, `cell-rwe-minimum-t1-fix_flow_linkage-r2-bp-standard-s2026080602`, `cell-rwe-minimum-t2-draft_contract_tests-r1-bp-standard-s2026080601`, `cell-rwe-minimum-t2-draft_contract_tests-r2-bp-standard-s2026080602`; restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`; promotion PR #442 exact head `50e18540f40a8d47c384f2cac74683618f93c273`; merge `8c5c2f85bc5d66c08d730b7d0c69d914af19540c`; canonical workflow `31710478692`"],"forbidden_changes":["Any runtime, schema, migration, test-fixture, Provider, target, evaluator, scheduler, store, or external-effect change."],"forbidden_next_actions":["Do not call a Provider or read credentials for this CONTRACT packet.","Do not write a target default branch, execute EFFECT/T3, release, deploy, or merge automatically.","Do not start PE7-RWE-MR-CORPUS-SAMPLING-1 before this packet is merged and closed."],"ordered_steps":["Read the focused canonical RWE owner documents.","Record the delegated measurement_estimands.v1 contract.","Run the declared provider-free checks and prepare the Draft PR."],"read_paths":["docs/NEXT_DECISION.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md"],"verification":["PYTHONPATH=src uv run --no-project python -m unittest tests.test_session_context.CheckpointTests.test_current_repository_packet_binds_safe_live_capsule","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"expected_artifacts":["Versioned measurement_estimands.v1 ledger in docs/NEXT_DECISION.md."],"pause_gates":["Stop before any Provider call, credential access, target write, EFFECT, T3 action, release, deployment, or automatic merge."]}
-->

## Common Execution Protocol

- Refresh accepted main, the current packet, exact PR heads, CI, review, and ledger receipts before every transition.
- Derive a route action only from the accepted current window, the checked inventory, current-main evidence, and existing durable owners.
- Existing route boundary (quoted for compatibility, not new packet authority): The sole exception is the current packet's dispatch-capsule-authorized, one-per-claim local OpenCode weak-worker Provider invocation; it cannot make the controller read, pass, persist, or report a credential. This packet's external-effect limit is zero and does not use that exception.
- Keep changing PRs Draft; require stable-head independent review and canonical exact-head CI before governed manual merge.
- Treat ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures as bounded recovery transitions through their existing owners.
- Preserve exact receipt bindings and failed/unknown evidence; never convert absence, stale evidence, or an unproven external outcome to success.
- Emergency-stop: revert the current window and retain detailed lifecycle evidence. authority, evaluator, recovery, and schema remain unchanged.

## Hard Stops

- no Provider call; no credential read, target write, release, deployment, automatic merge, EFFECT execution, or T3 action without its separate exact authority;
- no second controller, ledger, queue, lease, workflow owner, store, scheduler, evaluator, authority, or persistence owner;
- no future-route path/prose, model output, local checkpoint, or candidate PR accepted as current-main authority;
- no stale/ambiguous owner, caller, path, verification, rollback, cleanup, retention, evidence, schema, evaluator, authority, or recovery fact treated as proved;
- no retry of an outcome-unknown effect and no deletion or concealment of failure, pause, repair, or recovery evidence.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is the sole routing-only index. Promotion removes exactly one eligible packet, re-derives every `REFRESH_AT_PROMOTION` field from accepted main, validates the resulting candidate, and independently reviews the routing change. No future sketch, static path, or profile alone authorizes code or an effect.
