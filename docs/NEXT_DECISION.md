# Next Decision

Last updated: 2026-08-14.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. The v2 four-cell RUN and CLOSEOUT are accepted as lifecycle `CONTROLLED_FAILURE`, not a viable baseline. `PE7-RWE-MR-ESTIMANDS-1`, `PE7-RWE-MR-CORPUS-SAMPLING-1`, `PE7-RWE-MR-OPERATIONS-EVIDENCE-1`, and `PE7-RWE-MR-PROTOCOL-FREEZE-1` are accepted on main with their source-bound measurement, corpus, operations, and protocol contracts. The human value owner for the measurement-readiness route is Igzela, the repository owner; this session's implementation agent is explicitly delegated to record and execute the remaining T2 value judgments for this route. The current window is `PE7-RWE-DB-SNAPSHOT-CORPUS-1` `READY_FOR_EXECUTION` for provider-free snapshot and corpus production only.

## Authoritative Forward Order

```text
[window: PE7-RWE-DB-SNAPSHOT-CORPUS-1 — READY_FOR_EXECUTION, delegated pre-AC snapshot/corpus production]

→ `PE7-RWE-DB-PREFLIGHT-1` after this packet is merged and closed
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-DB-SNAPSHOT-CORPUS-1` — `READY_FOR_EXECUTION`

## Historical V2 Closeout

**State:** `COMPLETE`

**Evidence:** Disposition `CONTROLLED_FAILURE`; run `run-live-20260813-v2c`; authorization `auth-live-v2-003`; four frozen cells; restricted-bundle sha256 `9b345faf744c14d67157856a512b39d90c6e03ff1081783c793b987d6f93bf82`; redacted-bundle sha256 `e2eafa226700061cb000b35dec776ef0b49417aa5faece0b065923b49ee83d3f`; no seal and no target-default-branch write. Do not rerun this effect.

## Packet PE7-RWE-DB-SNAPSHOT-CORPUS-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-RWE-MR-PROTOCOL-FREEZE-1 — COMPLETE on accepted main f575b10a6de617bf3dab5611900bf0a48727c0c6; PR #447 exact head 00c8592676c5f73447f94b3abc1361087b371196; exact-head review receipt comment 5289552091; canonical workflow 31770551762; exact-head check 31770551749; manifest sha256 b5e37c7c2419a3acb42a8f21dbf2ba56aa8ddabb995b84b644f1b116a3321c12.

**Class:** `IMPLEMENT`

**Outcome:** Materialize the frozen task artifacts and a reconstructable pre-AC Harness/config/toolchain snapshot under existing RWE artifact owners.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md`, and `engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v1.json` only. Provider-free artifact production; no task-semantic, evaluator, budget, runtime-owner, or accepted-Harness behavior change.

**Exit:** A hash-verified snapshot manifest and corpus binding whose declared rebuild commands and provider-free golden traces match accepted main; otherwise record the exact unavailable/mismatch stop and do not promote preflight.

**Stop:** A task cannot be legally retained/replayed, snapshot reconstruction is nondeterministic, a required digest or trace is unavailable, or artifact storage would create a second owner.

### Snapshot manifest — pre_ac_harness_snapshot.v1

1. **Scope and authority.** This is provider-free artifact production. It grants no spend, execution, output, merge, release, deployment, adoption, EFFECT, or T3 authority. It never rewrites the frozen v2 corpus, protocol, schedule, estimands, or decision rule.
2. **Frozen bindings.** Bind accepted main f575b10a6de617bf3dab5611900bf0a48727c0c6, RWE artifact freeze point ee43eac853644266614da09de764a3bf19f2d281, source commit 6240768506320a324d68787b9eaa86971c8c930c, source tree 137e912f416a3a8d5be307e91bb2580154fc8fc34c6de52c2441ef3e3f93a064, corpus 044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20, protocol bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db, schedule 6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38, and both task-definition hashes.
3. **Snapshot contents.** The manifest must record normalized repository identity, exact source/tree bindings, RWE artifact hashes, task-definition hashes, Rust/toolchain identity, dependency-lockfile digests, tracked configuration digests, admitted executor/model/binary identity, bounded rebuild commands, and provider-free golden-trace commands/results. It must exclude raw prompts/outputs, credentials, private paths, and host-specific secrets.
4. **Existing owners and paths.** Frozen corpus/protocol/schedule identity remains under engine/src/rwe/operator_corpus.rs, corpus.rs, economic_protocol.rs, execution_schedule.rs, and engine/rwe/corpora/. Artifact capture, integrity, redaction, and persistence reuse the existing artifact and LocalProductStore owners; this snapshot adds no store, schema, ledger, or parallel artifact owner.
5. **Reconstruction rule.** Rebuild only from the accepted main checkout, declared lockfiles/configuration, pinned toolchain identity, and bounded provider-free commands. Any absent or conflicting value is UNAVAILABLE_NOW, sets reconstructable=false, and blocks preflight; no caller assertion or guessed environment fills it.
6. **Golden traces.** Compare only provider-free traces produced from the exact accepted main and frozen task artifacts. A missing, nondeterministic, or semantically different trace is a hard stop; fixture success is not live-baseline evidence.
7. **Retention and recovery.** Keep only bounded redacted/digest evidence through existing artifact owners. Do not copy/delete restricted raw bundles; preserve prior digests and failure evidence. Rollback is removal of the new snapshot manifest plus documentation revert.
8. **Next action.** Keep the PR Draft while changing; complete the focused artifact/reconstruction checks, obtain stable-head two-axis PASS, mark Ready once, wait for exact-head canonical CI, manually squash-merge, record closeout, refresh main, and only then promote PE7-RWE-DB-PREFLIGHT-1.
9. **Current reconstruction result.** Manifest sha256 `d13834c8ad41376f2884c906b335dce3a397fa0464ba83da0af6310fe2837ce2`. Local provider-free command observations are non-acceptance evidence. Exact frozen source-task verification is `UNAVAILABLE_NOW` because required active YAML artifacts are absent from the exact source commit; the source `apps/api/pyproject.toml` has no lockfile; and accepted main has no checked-in Rust toolchain pin. The manifest is `UNAVAILABLE_NOW`, `reconstructable=false`, and preflight promotion remains blocked.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-RWE-DB-SNAPSHOT-CORPUS-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"opencode_local_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Materialize a provider-free, hash-verified pre-AC Harness and corpus snapshot under existing RWE artifact owners.","rollback":"Revert the single snapshot/documentation commit and retain prior frozen protocol and evidence.","allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/NEXT_DECISION.md","engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v1.json"],"allowed_outputs":["The bounded pre_ac_harness_snapshot.v1 manifest and its provider-free verification evidence."],"prerequisites":["PE7-RWE-MR-PROTOCOL-FREEZE-1"],"prerequisite_receipts":["PE7-RWE-MR-PROTOCOL-FREEZE-1 COMPLETE: PR #447 exact head `00c8592676c5f73447f94b3abc1361087b371196`; merge `f575b10a6de617bf3dab5611900bf0a48727c0c6`; exact-head review receipt comment `5289552091`; canonical workflow `31770551762`; exact-head check `31770551749`; manifest sha256 `b5e37c7c2419a3acb42a8f21dbf2ba56aa8ddabb995b84b644f1b116a3321c12`"],"forbidden_changes":["Any Provider call, credential access, target write, EFFECT/T3 action, release, deployment, runtime, schema, evaluator, scheduler, store, budget, or accepted Harness behavior change.","Do not rewrite frozen corpus/protocol/schedule artifacts or include raw/private environment content."],"forbidden_next_actions":["Do not call a Provider, read credentials, execute a task, or rerun run-live-20260813-v2c.","Do not write a target default branch, issue/admit/consume RWE authority, or start PE7-RWE-DB-PREFLIGHT-1 before this packet is merged and closed.","Do not guess unavailable toolchain, dependency, runner, or golden-trace evidence."],"ordered_steps":["Bind the accepted protocol-freeze receipt and exact v2 artifact hashes.","Materialize the bounded snapshot manifest under the existing RWE artifact owner.","Run provider-free reconstruction and golden-trace checks; fail closed on unavailable evidence; prepare the governed Draft PR."],"read_paths":["docs/NEXT_DECISION.md","docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/MODULE_MAP.md","docs/ARCHITECTURE_BOOK.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","engine/src/rwe/operator_corpus.rs","engine/src/rwe/corpus.rs","engine/src/rwe/economic_protocol.rs","engine/src/rwe/execution_schedule.rs","engine/src/rwe/runner.rs","engine/src/rwe/live_baseline_coordinator.rs","engine/src/storage/local_product_store/rwe_authority.rs","engine/rwe/corpora/rwe-minimum-first-corpus/v2/protocol/rwe_economic_protocol.v1.json","engine/rwe/corpora/rwe-minimum-first-corpus/v2/schedule/execution_schedule.v1.json"],"verification":["PYTHONPATH=src uv run --no-project python -m unittest tests.test_session_context.CheckpointTests.test_current_repository_packet_binds_safe_live_capsule","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"expected_artifacts":["engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v1.json","Provider-free reconstruction and golden-trace verification evidence."],"pause_gates":["Stop before any Provider call, credential access, task execution, authority issue/admit/consume, target write, EFFECT, T3 action, release, deployment, or automatic merge.","Stop if any snapshot identity, lockfile/toolchain digest, rebuild command, or golden-trace comparison is unavailable or conflicting."]}
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
