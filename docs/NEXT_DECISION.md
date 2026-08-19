# Next Decision

Last updated: 2026-08-19.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze, AC2 typed execution, AC3 Golden Path responsibility split, AC4 transaction views, AC5 composition root, and AC6 Rust-authoritative schema convergence are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC7 removal manifest, deletion-only cleanup, and closeout are accepted. `PE7-RWE-CR-RECONSTRUCTION-1` is complete on accepted main `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`. `PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1` is complete on accepted main `262b67b675c36859c3dee6e1556fa0090654b75c`. `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` is the sole provider-free contract window and remains parked at `DECISION_REQUIRED` until remaining redacted-readiness and two-arm comparison-protocol decisions are separately re-proved. No Provider call, target write, authority consumption, or replay effect is authorized.

## Authoritative Forward Order

```text
[window: PE7-RWE-CR-PROTOCOL-PREFLIGHT-1 — DECISION_REQUIRED, provider-free; planning parked]


```

## Active Routing

1. `PE7-RWE-CR-PROTOCOL-PREFLIGHT-1` — `DECISION_REQUIRED`

## Completed (PE7-AC7-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178`.

## Completed (PE7-RWE-CR-RECONSTRUCTION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #566 exact head `57f4a5ee3a9be48a6ebdc20eddbd5df978c4440f`; squash merge `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`; exact-head review receipt comment `5324095735`; canonical workflow `32103730088`; exact-head check `32103730089`. The explicit Python 3.14.4 verifier and registered provider-free traces passed. No Provider call, target write, authority consumption, or effect occurred.

The reconstruction contract and historical implementation details remain in PR #566 and its merged diff. Frozen pre-AC inputs are unchanged.

## Completed (PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #572 exact head `0f63ad49c2b5ba87bf5e661bcbae9fd5fab9a9a8`; squash merge `262b67b675c36859c3dee6e1556fa0090654b75c`; exact-head review receipt comment `5328249811`; canonical workflow `32137758400`; exact-head check `32137758389`. The existing-owner SQLite read-only Store/auth/preflight seam and old/new identity projection are accepted. No Provider call, credential-value read, authority consumption, target write, or effect occurred. This packet does not claim a live-ready baseline.

## Packet PE7-RWE-CR-PROTOCOL-PREFLIGHT-1

**State:** `DECISION_REQUIRED`

**Prerequisite:** `PE7-RWE-CR-PROTOCOL-PREFLIGHT-REPAIR-1` — COMPLETE on accepted main `262b67b675c36859c3dee6e1556fa0090654b75c` (PR #572 exact head `0f63ad49c2b5ba87bf5e661bcbae9fd5fab9a9a8`; exact-head review receipt `5328249811`; canonical workflow `32137758400`; exact-head check `32137758389`). Reconstruction remains COMPLETE on `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`.

**Class:** `CONTRACT`

**Outcome:** Freeze randomization/interleaving, allocation concealment, drift covariates, capacity, finite authorizations, and immediate provider-free preflight for the old/new comparison.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md` only until remaining owner facts are re-proved; no live execution or post-AC threshold change; reuse the pre-registered measurement protocol and the accepted read-only Store/auth/preflight seam.

**Exit:** Zero-mismatch preflight and two unissued, finite operator authorization packages for both arms in the same bounded window.

**Stop:** Provider/model/environment identity cannot remain comparable, capacity creates arm-time confounding, old/new evidence paths can collide, any identity/evidence is stale or missing, redacted credential readiness remains unavailable without an accepted owner, or any step would consume authority, call a Provider, or write a target.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze the protocol/preflight contract only. Do not run an arm, issue or consume authority, call a Provider, produce an effect, or perform analysis.
2. **Prerequisites and evidence.** Accepted repair main `262b67b675c36859c3dee6e1556fa0090654b75c`; reconstruction `7cfa817a82ea3a638bd3e50af5266ee54eefe0c0`; pre-AC snapshot manifest `a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c`, source commit `6240768506320a324d68787b9eaa86971c8c930c`, source tree `f8d22ebf5009842d37285624f345d47bf6da5548032eb84cb7528407169d9cc3`, recipe commit `de0b3bb5158f07100d9ee3846b0555193503629d`, recipe tree `8fc5610c47cc4477c5ab7c65fe680ddf970bca4e612558701b316cc2ca038766`; corpus/protocol/schedule `044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20`/`bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db`/`6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38`; post-AC lock/toolchain hashes `cf68982734f8a72148950f119408b676dd5b42ce65d7af69c02eca017a551653`/`e59c5da37d1f9f4e0f815bc188cb6056fc7410c9cdaa9673c2d44da557c75d12`.
3. **Owners and paths.** Existing owners only: read-only `LocalProductStore` open/auth, `engine/src/rwe/live_baseline_coordinator.rs::operator_preflight_read_only`, `engine/src/rwe/operator_corpus.rs::freeze_current_operator_contract_set`, `engine/src/rwe/frozen_rwe_bindings.rs`, `engine/src/rwe/economic_protocol.rs`, `engine/src/rwe/execution_schedule.rs`; this packet adds no runtime owner.
4. **Frozen invariants.** Bind old/new identities, corpus/protocol/schedule hashes, accepted main and toolchain identities, route manifest, packet identity, pre-registered protocol, and the existing preflight/corpus owners before any authorization is issued.
5. **Only semantic delta.** Contract, status, and routing promotion only; no runtime or measurement-threshold change.
6. **Forbidden changes.** No Provider call, effect, T3 action, authority consumption, target write, credential-value read/output/persistence, schema/migration, new runtime/store/evaluator, or second owner.
7. **Ordered slices.** Reconcile current identities; freeze allocation/interleaving/drift/capacity and finite budgets; run the accepted read-only owner preflight without issuing or consuming; retain redacted/hash-bound receipts and promote the run only after this packet is accepted.
8. **Failure, recovery, and stop taxonomy.** Fail closed on mismatch, missingness, stale identity, capacity confounding, unavailable redacted readiness, or evidence collision; do not retry or consume authority; preserve `unknown`; rollback is a docs-only revert.
9. **Verification.** Run `cargo test -p engine viability_preflight_is_read_only_without_store_creation_or_auth_touch`, `cargo test -p engine current_comparison_manifest_rejects_identity_collision`, `git diff --check`, `bash scripts/check_wire_codegen_drift.sh`, `uv run --no-project python tools/check_security_baseline.py`, and `uv run --no-project python scripts/check_agent_handoff.py`; no Provider or effect command is permitted until a later EFFECT packet.
10. **Compatibility, rollback, and retention.** Reuse the accepted read-only Store/RWE owners and pre-registered measurement protocol; no migration or schema change; revert the docs-only packet promotion to roll back.
11. **Exit artifact.** Accepted status receipt, promoted route/NEXT packet, and redacted/hash-bound provider-free preflight evidence owned by the existing RWE artifact/evidence owners; the two finite authorization packages remain unissued and are only a next-run input, not an effect of this packet.
12. **Next action.** After accepted zero-mismatch preflight, promote `PE7-RWE-CR-RUN-1`; do not issue, consume, call, or run an effect from this packet.

### Decision-required boundary

The accepted repair on `262b67b675c36859c3dee6e1556fa0090654b75c` added a strict existing-file SQLite read-only open, non-touching authentication, and a provider-free preflight projection that does not read credential values. That seam is not a live-ready claim.

This packet remains parked because the remaining comparison/readiness decisions are not re-proved:

- Encrypted SQLCipher preflight still returns `encryption_readiness_unavailable` pending a separately authorized redacted-readiness owner; unencrypted SQLite is the only path the repair guaranteed.
- No provider-free live preflight was run against an existing Store; `ready=true` is not accepted.
- Two-arm interleaving, allocation concealment, drift covariates, capacity pairing, and two finite unissued authorization packages are not yet frozen as a contemporary protocol receipt.
- The identity projection exists in the accepted binding owner and fails closed on missing/swapped/colliding identities, but that is not a completed protocol/preflight freeze.

No database, credential value, authority, Provider, target, or effect may be touched to “unstick” this window. `PE7-RWE-CR-RUN-1` remains blocked until this packet is separately re-promoted and accepted.

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

`docs/FUTURE_ROUTE.md` is routing-only. The parked protocol packet remains governed by this document; no future sketch authorizes code or an effect until promoted into this document from accepted `main`.
