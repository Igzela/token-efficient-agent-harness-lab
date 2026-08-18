# Next Decision

Last updated: 2026-08-18.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze, AC2 typed execution, AC3 Golden Path responsibility split, AC4 transaction views, AC5 composition root, and AC6 Rust-authoritative schema convergence are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. AC7 cleanup, closeout, and Contemporary RWE reconstruction are accepted. The provider-free read-only Store/auth/preflight repair is accepted on `main` `262b67b675c36859c3dee6e1556fa0090654b75c`; the contemporary protocol/preflight contract is the sole current provider-free window. No Provider call, target write, authority consumption, or replay effect is authorized.

## Authoritative Forward Order

```text
[window: PE7-RWE-CR-PROTOCOL-PREFLIGHT-1 — READY_FOR_EXECUTION, provider-free; contract and redacted preflight]
```

## Active Routing

1. `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC7-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178`.

## Completed (PE7-RWE-CR-RECONSTRUCTION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #566 exact head `57f4a5ee3a9be48a6ebdc20eddbd5df978c4440f`; squash merge `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`; exact-head review receipt comment `5324095735`; canonical workflow `32103730088`; exact-head check `32103730089`. The explicit Python 3.14.4 verifier and registered provider-free traces passed. No Provider call, target write, authority consumption, or effect occurred.

## Completed (PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #572 exact head `0f63ad49c2b5ba87bf5e661bcbae9fd5fab9a9a8`; squash merge `262b67b675c36859c3dee6e1556fa0090654b75c`; exact-head review receipt comment `5328249811`; canonical workflow `32137758400`; exact-head check `32137758389`. The accepted repair proves the SQLite read-only opener, non-touching managed authentication, provider-free unavailable-readiness projection, contemporary old/new identity validation, focused nonmutation tests, Rust nextest, engine unit tests, and PostgreSQL integration tests. No Provider call, credential-value read, target write, authority consumption, or effect occurred.

## Packet PE7-RWE-CR-PROTOCOL-PREFLIGHT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1` — COMPLETE on accepted main `262b67b675c36859c3dee6e1556fa0090654b75c`.

**Class:** `CONTRACT`

**Outcome:** Freeze the contemporary old/new comparison contract and perform only the provider-free read-only preflight projection. Resolve the former owner-gap stop by binding the accepted read-only owners, explicit redacted credential-readiness semantics, balanced arm allocation/interleaving, paired capacity, drift rules, and two finite unissued authorization-package shapes. Do not issue or consume authority, call a Provider, or run an arm.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md` only; no runtime, schema, migration, frozen corpus/protocol/schedule input, evaluator, authority, target, or effect change.

**Exit:** Accepted docs receipt binds the repaired owner paths, exact old/new comparison identity, the unchanged frozen corpus/protocol/schedule hashes, a redacted readiness schema, a balanced blinded allocation/interleaving rule, paired-capacity and drift checks, and two finite `issued=false`/`admitted=false` authorization-package digests. Any unavailable readiness remains an explicit blocker; this packet never claims a live-ready baseline.

**Stop:** Missing or conflicting identity, stale source or toolchain, absent existing Store, non-empty SQLite companion, missing same-tenant Golden Path terminal evidence, unavailable redacted readiness, unpaired capacity, allocation/interleaving mismatch, authority drift, or any Provider call, credential-value read, target write, authority issuance/consumption, or effect.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze and verify only the contemporary old/new protocol/preflight contract. Do not run an arm, issue or consume authority, call a Provider, produce an effect, analyze results, or declare a decision-grade baseline.
2. **Prerequisites and evidence.** Accepted current main is `262b67b675c36859c3dee6e1556fa0090654b75c`; reconstruction is PR #566 merge `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`; read-only repair is PR #572 merge `262b67b675c36859c3dee6e1556fa0090654b75c`; dependency lock repair is PR #573 merge `0cb9a7c4a20de8e3f6760c0568e4c851417e72fe`; the frozen old/new binding remains the accepted `rwe_comparison_identity.v1` projection and is not rewritten to current control-plane main.
3. **Owners and paths.** Reuse only `LocalProductStore::open_existing_read_only`, `authenticate_managed_acceptance_principal_read_only`, `operator_preflight_read_only`, `current_comparison_manifest`, `freeze_current_operator_contract_set`, and their existing tests/evidence owners. No new runtime, Store, controller, evaluator, credential, or rollback owner.
4. **Frozen invariants.** Preserve old source commit/tree `6240768506320a324d68787b9eaa86971c8c930c`/`f8d22ebf5009842d37285624f345d47bf6da5548032eb84cb7528407169d9cc3`, new comparison identity `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`/`c81a2e4e635da05a8a1c15630371e98943c70c86`, corpus/protocol/schedule hashes `044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20`/`bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db`/`6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38`, and all existing fail-closed authority/effect boundaries.
5. **Only semantic delta.** Contract/status/routing promotion plus a redacted, non-secret readiness schema: provider kind/base URL/path, model, binary/version, environment digest, capacity class, and a boolean readiness disposition supplied by an accepted operator owner. Credential values and credential symbols are never read, stored, or emitted by this packet.
6. **Forbidden changes.** No Provider call, effect, T3 action, authority issuance/consumption, target write, credential-value read/output/persistence, schema/migration, frozen corpus/protocol/schedule/threshold change, fixture-only acceptance, new runtime/Store/evaluator, or second owner.
7. **Ordered slices.** (1) Bind `current_comparison_manifest` and the accepted read-only seam. (2) Keep the four frozen task cells and per-cell budgets unchanged; allocate exactly two cells per arm with one old/new assignment per task, using a pre-results blinded balanced-permutation receipt and no adjacent same-arm execution where the finite cell order permits. (3) Require same provider/model/binary/environment identity and a paired capacity class for both arms; record drift covariates before results and stop on mismatch. (4) Require a same-tenant completed Golden Path terminal receipt. (5) Bind two finite, one-use, unissued, unadmitted authorization-package digests with caller-supplied finite expiry; do not issue or consume them.
8. **Failure, recovery, and stop taxonomy.** Fail closed on missing DB, non-empty WAL/SHM/journal, snapshot drift, auth metadata mutation, missing redacted readiness, identity collision, stale/swapped arms, allocation or interleaving failure, capacity confounding, missing terminal evidence, authority mismatch, or unknown external outcome. Revert only the docs promotion; retain all hash-bound receipts and leave `PE7-RWE-CR-RUN-1` blocked.
9. **Verification.** Run the read-only and identity focused tests already bound by the repair, `cargo test -p engine --features pg-tests --test test_pg_integration`, `bash scripts/check_wire_codegen_drift.sh`, `uv run --no-project python tools/check_security_baseline.py`, `uv run --no-project python scripts/check_agent_handoff.py`, and `git diff --check`. The provider-free projection must report `credential_readiness=unavailable` when no separately accepted redacted owner supplies readiness; no Provider/effect command is permitted.
10. **Compatibility, rollback, and retention.** Existing mutable Store/auth/admit/run paths retain behavior; this contract uses only the accepted read-only path and frozen RWE inputs. Revert the docs-only PR to roll back; retain the reconstruction manifest, comparison projection, redacted readiness disposition, allocation receipt, and unissued package digests.
11. **Exit artifact.** Accepted status and route receipt, exact old/new identity projection, redacted readiness disposition, paired-capacity/drift/allocation/interleaving receipt, two finite unissued package digests, and explicit `provider_call_performed=false`, `target_write_performed=false`, and `authority_consumed=false` fields. `ready=false` or `credential_readiness=unavailable` remains a truthful blocker, not replay success.
12. **Next action.** Promote `PE7-RWE-CR-RUN-1` only after this packet has an accepted exact-head contract/preflight receipt with no unresolved blocker and a separate finite operator authorization; otherwise remain fail-closed at this packet. Do not issue, consume, call, or run an effect from this contract packet.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs":["A provider-free contemporary old/new protocol and read-only preflight contract limited to the canonical document owners.","Exact-head documentation verification and review evidence bound to the accepted repair and frozen comparison identities.","An explicit fail-closed redacted-readiness and unissued-authorization receipt with no external effect."],"allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"authority_consumption_allowed":false,"dispatch_lane":"provider_free_repository_maintenance","expected_artifacts":["Canonical protocol/preflight contract binds repaired read-only owners and frozen old/new comparison identities.","Redacted readiness, paired capacity, drift, balanced allocation/interleaving, and finite unissued authorization-package receipt.","Exact-head review and canonical CI evidence with no Provider call, target write, authority consumption, or external effect."],"external_effect_limit":0,"forbidden_changes":["Do not modify runtime, schema, migration, frozen corpus, protocol, schedule, evaluator, authority, target, or external state.","Do not read, output, persist, or validate credential values; readiness must be redacted and owner-supplied.","Do not create a second runtime, Store, controller, evaluator, credential, or rollback owner.","Do not use fixture or fake results as managed acceptance evidence."],"forbidden_next_actions":["Do not issue or consume an authorization or execute PE7-RWE-CR-RUN-1 from this contract packet.","Do not treat credential_readiness=unavailable, missing, stale, or conflicting evidence as success.","Do not retry a possibly executed external effect whose outcome is unknown."],"goal":"Re-promote the contemporary old/new protocol and provider-free read-only preflight contract after the accepted owner repair without authorizing replay.","ordered_steps":["Bind accepted reconstruction, dependency, and read-only repair receipts plus the immutable old/new comparison manifest.","Freeze balanced blinded arm allocation/interleaving, paired capacity, drift covariates, same-tenant Golden Path evidence, and finite unissued authorization-package fields without changing frozen task inputs.","Record explicit redacted credential-readiness semantics and fail closed when the separately accepted readiness owner is unavailable.","Run documentation and existing provider-free verification only; leave replay blocked until a separate operator authorization exists."],"packet_id":"PE7-RWE-CR-PROTOCOL-PREFLIGHT-1","packet_state":"READY_FOR_EXECUTION","pause_gates":["Stop when an owner, caller, path, operation, destination, identity, or decision cannot be re-proved from accepted main.","Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.","Stop before any credential-value read, Provider, target, authority issuance/consumption, or effect.","Stop when redacted readiness, capacity pairing, allocation/interleaving, or finite package evidence is missing or conflicting."],"plan_lane_state":"plan_lane_active","prerequisite_receipts":["PR #566 exact head 57f4a5ee3a9be48a6ebdc20eddbd5df978c4440f; merge 7cfa817a82ea3a638bd3e50af5266ee54eefe0c0; provider-free reconstruction passed","PR #573 exact head 0943d41eafd3ebf03c4dfcb20b5ab678d33e9f95; merge 0cb9a7c4a20de8e3f6760c0568e4c851417e72fe; h2 RustSec repair passed","PR #572 exact head 0f63ad49c2b5ba87bf5e661bcbae9fd5fab9a9a8; merge 262b67b675c36859c3dee6e1556fa0090654b75c; read-only repair passed"],"prerequisites":["PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1"],"private_paths_allowed":false,"promotion_evidence_sha256":"ff28f69819fecfd7a5442ea4b6291632b59db53c1240cf8655fe58815439a23","read_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"risk_class":"none","rollback":"Revert this documentation-only protocol/preflight promotion; retain the accepted reconstruction and read-only repair with no database, authority, or external cleanup.","route_manifest_sha256":"2b78045a6d4bb5df5ef7965d78ae8978102c6e80dbc7e5b53b5d57f58e25be75","schema_version":"weak_agent_dispatch.v1","secret_values_allowed":false,"verification":["cargo test -p engine viability_preflight_reports_unavailable_without_credential_claim","cargo test -p engine viability_preflight_is_read_only_without_store_creation_or_auth_touch","cargo test -p engine current_comparison_manifest_rejects_identity_collision","cargo test -p engine --features pg-tests --test test_pg_integration","bash scripts/check_wire_codegen_drift.sh","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py","git diff --check"],"verification_family":"docs_evidence_review","worker_tier":"T2"}
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

`docs/FUTURE_ROUTE.md` is routing-only. The contemporary protocol/preflight packet is governed by this document; no future sketch authorizes code or an effect until promoted into this document from accepted `main`.
