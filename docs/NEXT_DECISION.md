# Next Decision

Last updated: 2026-08-18.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze, AC2 typed execution, AC3 Golden Path responsibility split, AC4 transaction views, AC5 composition root, and AC6 Rust-authoritative schema convergence are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC7 removal manifest, deletion-only cleanup, and closeout are accepted; `PE7-RWE-CR-RECONSTRUCTION-1` is complete on accepted main `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`. `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` is now the sole provider-free contract window. No Provider call, target write, authority consumption, or replay effect is authorized.

## Authoritative Forward Order

```text
[window: PE7-RWE-CR-PROTOCOL-PREFLIGHT-1 — READY_FOR_EXECUTION, provider-free]


```

## Active Routing

1. `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC7-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178`.

## Completed (PE7-RWE-CR-RECONSTRUCTION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #566 exact head `57f4a5ee3a9be48a6ebdc20eddbd5df978c4440f`; squash merge `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`; exact-head review receipt comment `5324095735`; canonical workflow `32103730088`; exact-head check `32103730089`. The explicit Python 3.14.4 verifier and registered provider-free traces passed. No Provider call, target write, authority consumption, or effect occurred.

The reconstruction contract and historical implementation details remain in PR #566 and its merged diff. Its frozen pre-AC inputs are retained unchanged; the next executable packet is the provider-free protocol/preflight contract below.

## Packet PE7-RWE-CR-PROTOCOL-PREFLIGHT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-RWE-CR-RECONSTRUCTION-1` — COMPLETE on accepted main `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0` (PR #566 exact head `57f4a5ee3a9be48a6ebdc20eddbd5df978c4440f`; merge `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`; exact-head review receipt `5324095735`; canonical workflow `32103730088`; exact-head check `32103730089`).

**Class:** `CONTRACT`

**Outcome:** Freeze randomization/interleaving, allocation concealment, drift covariates, capacity, finite authorizations, and immediate provider-free preflight for the old/new comparison.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md` only; no live execution or post-AC threshold change; reuse the pre-registered measurement protocol.

**Exit:** Zero-mismatch preflight and two unissued, finite operator authorization packages for both arms in the same bounded window.

**Stop:** Provider/model/environment identity cannot remain comparable, capacity creates arm-time confounding, old/new evidence paths can collide, any identity/evidence is stale or missing, or any step would consume authority, call a Provider, or write a target.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze the protocol/preflight contract only. Do not run an arm, issue or consume authority, call a Provider, produce an effect, or perform analysis.
2. **Prerequisites and evidence.** Accepted main `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`; reconstruction receipt above; pre-AC snapshot manifest `a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c`, source commit `6240768506320a324d68787b9eaa86971c8c930c`, source tree `f8d22ebf5009842d37285624f345d47bf6da5548032eb84cb7528407169d9cc3`, recipe commit `de0b3bb5158f07100d9ee3846b0555193503629d`, recipe tree `8fc5610c47cc4477c5ab7c65fe680ddf970bca4e612558701b316cc2ca038766`; corpus/protocol/schedule `044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20`/`bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db`/`6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38`; post-AC lock/toolchain hashes `cf68982734f8a72148950f119408b676dd5b42ce65d7af69c02eca017a551653`/`e59c5da37d1f9f4e0f815bc188cb6056fc7410c9cdaa9673c2d44da557c75d12`.
3. **Owners and paths.** Existing owners only: `engine/src/rwe/live_baseline_coordinator.rs::operator_preflight`, `engine/src/rwe/operator_corpus.rs::freeze_current_operator_contract_set`, `engine/src/rwe/economic_protocol.rs`, `engine/src/rwe/execution_schedule.rs`, `engine/tests/test_pg_integration.rs`, and `engine/tests/test_operator_evidence.rs`; this packet adds no runtime owner.
4. **Frozen invariants.** Bind old/new identities, corpus/protocol/schedule hashes, accepted main and toolchain identities, route manifest, packet identity, pre-registered protocol, and the existing preflight/corpus owners before any authorization is issued.
5. **Only semantic delta.** Contract, status, and routing promotion only; no runtime or measurement-threshold change.
6. **Forbidden changes.** No Provider call, effect, T3 action, authority consumption, target write, credential use, schema/migration, new runtime/store/evaluator, or second owner.
7. **Ordered slices.** Reconcile current identities; freeze allocation/interleaving/drift/capacity and finite budgets; run the existing provider-free owner preflight without issuing or consuming; retain redacted/hash-bound receipts and promote the run only after this packet is accepted.
8. **Failure, recovery, and stop taxonomy.** Fail closed on mismatch, missingness, stale identity, capacity confounding, or evidence collision; do not retry or consume authority; preserve `unknown`; rollback is a docs-only revert.
9. **Verification.** Run `cargo test -p engine viability_preflight_is_ready_without_issuing_or_consuming`, `cargo test -p engine current_contract_set_binds_candidate_freeze_point_and_hashes`, `git diff --check`, `bash scripts/check_wire_codegen_drift.sh`, `uv run --no-project python tools/check_security_baseline.py`, and `uv run --no-project python scripts/check_agent_handoff.py`; no Provider or effect command is permitted.
10. **Compatibility, rollback, and retention.** Reuse the current Store/RWE owners and pre-registered measurement protocol; no migration or schema change; revert the docs-only packet promotion to roll back.
11. **Exit artifact.** Accepted status receipt, promoted route/NEXT packet, and redacted/hash-bound provider-free preflight evidence owned by the existing RWE artifact/evidence owners; the two finite authorization packages remain unissued and are only a next-run input, not an effect of this packet.
12. **Next action.** After accepted zero-mismatch preflight, promote `PE7-RWE-CR-RUN-1`; do not issue, consume, call, or run an effect from this packet.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["A provider-free contract/status/routing change limited to the independently proved current-main allowed paths.","Exact-head verification and review evidence through the existing lifecycle owners."],"allowed_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","docs/FUTURE_ROUTE.md"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["Accepted status receipt and promoted route/NEXT packet.","Redacted/hash-bound provider-free preflight evidence owned by the existing RWE artifact/evidence owners.","No Provider call, authority consumption, target write, or external effect."],"external_effect_limit":0,"forbidden_changes":["Do not use FUTURE_ROUTE static paths as current-main authority.","Do not create a second controller, ledger, queue, lease, store, evaluator, or workflow owner.","Do not modify frozen corpus, protocol inputs, schedule, operator, or snapshot inputs.","Do not mint T3 authority, execute an EFFECT, call a Provider, write a target, or auto-merge."],"forbidden_next_actions":["Do not issue or consume an authorization or execute an EFFECT/T3 path from this contract packet.","Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.","Do not start the run successor until this packet has an independently accepted exact-head receipt."],"goal":"Freeze the provider-free protocol/preflight contract for the contemporary old/new comparison.","ordered_steps":["Reconcile accepted old/new identities and bind the reconstruction receipt.","Freeze allocation/interleaving, drift covariates, capacity, and finite authorization budgets.","Use existing provider-free preflight owners to produce unissued hash-bound readiness evidence."],"packet_id":"PE7-RWE-CR-PROTOCOL-PREFLIGHT-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.","Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.","Stop before a Provider, target, authority consumption, or external effect.","Do not retry a possibly executed external effect whose outcome is unknown."],"plan_lane_state":"plan_lane_active","prerequisite_receipts":["PR #566 exact head `57f4a5ee3a9be48a6ebdc20eddbd5df978c4440f`; squash merge `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`; exact-head review receipt comment `5324095735`; canonical workflow `32103730088`; exact-head check `32103730089`; explicit Python 3.14.4 verifier and provider-free traces passed; no Provider call, target write, authority consumption, or effect"],"prerequisites":["PE7-RWE-CR-RECONSTRUCTION-1"],"private_paths_allowed":false,"promotion_evidence_sha256":"c33125ee031142eb1ced9153eb04f6635554e57301ffdd2b4d8b73eae52b536e","read_paths":["docs/CURRENT_STATUS.md","docs/NEXT_DECISION.md","docs/FUTURE_ROUTE.md","docs/ARCHITECTURE_BOOK.md","docs/REAL_WORLD_TESTING_PLAYBOOK.md","engine/src/rwe/live_baseline_coordinator.rs","engine/src/rwe/operator_corpus.rs","engine/src/rwe/economic_protocol.rs","engine/src/rwe/execution_schedule.rs","engine/tests/test_pg_integration.rs","engine/tests/test_operator_evidence.rs"],"risk_class":"none","rollback":"Revert the docs-only protocol/preflight promotion; no runtime, data, authority, or external state changes.","route_manifest_sha256":"2b78045a6d4bb5df5ef7965d78ae8978102c6e80dbc7e5b53b5d57f58e25be75","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["cargo test -p engine viability_preflight_is_ready_without_issuing_or_consuming","cargo test -p engine current_contract_set_binds_candidate_freeze_point_and_hashes","git diff --check","bash scripts/check_wire_codegen_drift.sh","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"verification_family":"docs_evidence_review","worker_tier":"T2"}
-->

## Common Execution Protocol

- `READY_FOR_EXECUTION` packets require a valid dispatch capsule; blocked or decision-required packets carry no executable capsule.
- Refresh accepted `main`, the current packet, exact PR head, CI, and review receipts before each transition.
- Keep a changing PR Draft; run focused/local full checks, then one final stable-head Standards/Spec review, Ready, canonical exact-head CI, manual squash merge, and main refresh.
- No Provider call, credential read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner in this packet.
- Unknown, stale, missing, or conflicting evidence remains fail-closed; never retry an effect whose outcome may be unknown.

## Hard Stops

- `DECISION_REQUIRED` when a caller, owner, path, semantics, compatibility fact, rollback, or evidence cannot be re-proved from accepted `main`.
- No Provider call, credential read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner.
- Unknown or possibly executed effects are never treated as success or retried.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is routing-only. The promoted packet was removed from that index and its manifest was refreshed; no future sketch authorizes code or an effect until promoted into this document from accepted `main`.
