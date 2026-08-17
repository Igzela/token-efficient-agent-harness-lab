# Next Decision

Last updated: 2026-08-17.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze, AC2 typed execution, AC3 Golden Path responsibility split, AC4 transaction views, AC5 composition root, and AC6 Rust-authoritative schema convergence are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC7 removal manifest, deletion-only cleanup, and closeout are accepted; `PE7-RWE-CR-RECONSTRUCTION-1` is now the sole provider-free reconstruction window. Protocol/preflight and any replay or effect remain gated successors; no Provider call, target write, authority consumption, or effect is authorized.

## Authoritative Forward Order

```text
[window: PE7-RWE-CR-RECONSTRUCTION-1 — IN_PROGRESS, provider-free]


```

## Active Routing

1. `PE7-RWE-CR-RECONSTRUCTION-1` — `IN_PROGRESS`

## Completed (PE7-AC7-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178`.
## Packet PE7-RWE-CR-RECONSTRUCTION-1

**State:** `IN_PROGRESS`

**Prerequisite:** PE7-AC7-CLOSEOUT-1 — COMPLETE on accepted main `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b` (PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178`).

**Class:** `IMPLEMENT`

**Outcome:** Rebuild and provider-free validate the frozen pre-AC Harness beside the accepted post-AC Harness under isolated identities.

**Allowed delta:** docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md, engine/src/rwe/frozen_rwe_bindings.rs, scripts/verify_rwe_snapshot.py, tests/test_verify_rwe_snapshot.py.

**Exit:** Both Harnesses pass registered provider-free traces and bind exact binaries/config/toolchains without shared mutable state.

**Stop:** Old Harness cannot be reproduced, isolation fails, or compatibility shims change the measured behavior.

### Twelve-field contract

1. **Outcome and non-goals.** Rebuild and provider-free validate the frozen pre-AC Harness beside the accepted post-AC Harness under isolated identities.
2. **Prerequisites and evidence.** Accepted main `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; checked route manifest SHA `8569f8aad2dd6672e75a44ed6f61c72f852c0a3290744d957e0bc762d9acde7f`; predecessor receipt PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178`; current-main evidence SHA `b12c155ae340712b46bc6e788401802dcfdb0721f118760c5da68d57209f9208`.
3. **Owners and paths.** Read-only frozen inputs: engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json, engine/src/rwe/corpus.rs, engine/src/rwe/economic_protocol.rs, engine/src/rwe/execution_schedule.rs, engine/src/rwe/operator_corpus.rs, and engine/tests/test_pg_integration.rs. Implementation owners: engine/src/rwe/frozen_rwe_bindings.rs, scripts/verify_rwe_snapshot.py, and tests/test_verify_rwe_snapshot.py; frozen input owners are not modified by this packet.
4. **Frozen invariants.** Packet identity, route manifest SHA `8569f8aad2dd6672e75a44ed6f61c72f852c0a3290744d957e0bc762d9acde7f`, accepted-main SHA, predecessor receipt, and current-main evidence digest are immutable for this candidate. The frozen pre-AC identity is source commit `6240768506320a324d68787b9eaa86971c8c930c`, source tree `f8d22ebf5009842d37285624f345d47bf6da5548032eb84cb7528407169d9cc3`, recipe commit `de0b3bb5158f07100d9ee3846b0555193503629d`, recipe tree `8fc5610c47cc4477c5ab7c65fe680ddf970bca4e612558701b316cc2ca038766`, and corpus/protocol/schedule identities `044fcd7bf4c35c6a4798f60b5b87d79d8549b45351f4e350b397a63a0fe2ce20` / `bc68bfb320f891ee5490019385c17d71ee7bfc725bb43cd0c006d33c5d5d35db` / `6a729f1213384d2306091ce5f258c9ddd08fe569374167c04e7f10c930cb1b38`. The accepted post-AC identity is main `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`, tree `c81a2e4e635da05a8a1c15630371e98943c70c86`, `Cargo.lock` SHA-256 `cf68982734f8a72148950f119408b676dd5b42ce65d7af69c02eca017a551653`, and `rust-toolchain.toml` SHA-256 `e59c5da37d1f9f4e0f815bc188cb6056fc7410c9cdaa9673c2d44da557c75d12`; observed toolchain is Cargo/Rust 1.96.0, Python 3.14.4, Git 2.53.0, uv 0.11.17.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** docs/CURRENT_STATUS.md, docs/NEXT_DECISION.md, docs/FUTURE_ROUTE.md: promote one provider-free reconstruction window and bind the accepted predecessor.; engine/src/rwe/frozen_rwe_bindings.rs: add only the isolated old/new binding needed for reconstruction; scripts/verify_rwe_snapshot.py, tests/test_verify_rwe_snapshot.py: verify the source commit, recipe overlay, frozen corpus inputs, and provider-free reconstruction traces without changing frozen corpus, schedule, operator, or snapshot inputs.
8. **Failure, recovery, and stop taxonomy.** Cleanup: Remove only disposable reconstruction workspaces and derived provider-free traces after validation; retain hash-bound manifests and redacted evidence.; retention: Retain the pre_ac_harness_snapshot.v2.json manifest, source/recipe/corpus/toolchain hashes, and redacted verification receipts under existing RWE evidence owners.; decisions: schema unchanged (docs/NEXT_DECISION.md:schema); evaluator unchanged (docs/NEXT_DECISION.md:evaluator); authority unchanged (docs/NEXT_DECISION.md:authority); recovery unchanged (docs/NEXT_DECISION.md:recovery).
9. **Verification.** git diff --check; cargo fmt --all -- --check; cargo clippy -p engine --all-targets --all-features -- -D warnings; cargo test -p engine; uv run --no-project python -m unittest tests.test_verify_rwe_snapshot; uv run --no-project python scripts/verify_rwe_snapshot.py --manifest engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json --source-root /tmp/pe7-rwe-pre-ac-source --harness-root .; uv run --no-project python -m unittest discover -s tests -p test_agent_*.py
10. **Compatibility, rollback, and retention.** Revert the reconstruction adapter/artifact change and retain the accepted post-AC Harness; do not alter the frozen pre-AC snapshot or accepted RWE corpus identities.
11. **Exit artifact.** Evidence destinations: docs/CURRENT_STATUS.md, docs/NEXT_DECISION.md, engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json.
12. **Next action.** Governed PR, exact-head review/CI, manual merge, closeout, then repeat evidence-backed promotion.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A provider-free change limited to the independently proved current-main allowed paths.", "Exact-head verification and review evidence through the existing lifecycle owners."], "allowed_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/NEXT_DECISION.md", "engine/src/rwe/frozen_rwe_bindings.rs", "scripts/verify_rwe_snapshot.py", "tests/test_verify_rwe_snapshot.py"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Bound old/new reconstruction identities and hash-bound provider-free verification evidence.", "A focused snapshot-verifier test receipt and source-overlay verification result.", "No mutation of frozen corpus, protocol, schedule, operator, or snapshot inputs."], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not modify frozen corpus, protocol, schedule, operator, or snapshot inputs.", "Do not mint T3 authority, execute an EFFECT, auto-merge, call a Provider, or write a target."], "forbidden_next_actions": ["Do not skip an EFFECT node or execute an EFFECT or T3 path without its exact valid finite receipt.", "Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted."], "goal": "Rebuild and provider-free validate the frozen pre-AC Harness beside the accepted post-AC Harness under isolated identities.", "ordered_steps": ["docs/CURRENT_STATUS.md, docs/NEXT_DECISION.md, docs/FUTURE_ROUTE.md: promote one provider-free reconstruction window and bind the accepted predecessor.", "engine/src/rwe/frozen_rwe_bindings.rs: materialize only the isolated old/new binding needed for reconstruction without changing frozen Harness inputs.", "scripts/verify_rwe_snapshot.py, tests/test_verify_rwe_snapshot.py: verify the source commit, recipe overlay, frozen corpus inputs, and provider-free reconstruction traces."], "packet_id": "PE7-RWE-CR-RECONSTRUCTION-1", "packet_state": "IN_PROGRESS", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Recover ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures through existing owners; stop if recovery evidence is unproved.", "Stop before a Provider, target, automatic merge, authority consumption, or external effect.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #563 exact head `80e68a108eb1d752f6632944300786fe9ea6511d`; merge `42fcfa5ad7e349d27d3caa815163340f9c0d5c0b`; exact-head `PASS`; canonical workflow `32030794178`"], "prerequisites": ["PE7-AC7-CLOSEOUT-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "b12c155ae340712b46bc6e788401802dcfdb0721f118760c5da68d57209f9208", "read_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/NEXT_DECISION.md", "engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json", "engine/src/rwe/corpus.rs", "engine/src/rwe/economic_protocol.rs", "engine/src/rwe/execution_schedule.rs", "engine/src/rwe/frozen_rwe_bindings.rs", "engine/src/rwe/operator_corpus.rs", "engine/tests/test_pg_integration.rs", "scripts/verify_rwe_snapshot.py", "tests/test_verify_rwe_snapshot.py"], "risk_class": "none", "rollback": "Revert the reconstruction adapter/artifact change and retain the accepted post-AC Harness; do not alter the frozen pre-AC snapshot or accepted RWE corpus identities.", "route_manifest_sha256": "8569f8aad2dd6672e75a44ed6f61c72f852c0a3290744d957e0bc762d9acde7f", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["git diff --check", "cargo fmt --all -- --check", "cargo clippy -p engine --all-targets --all-features -- -D warnings", "cargo test -p engine", "uv run --no-project python -m unittest tests.test_verify_rwe_snapshot", "uv run --no-project python scripts/verify_rwe_snapshot.py --manifest engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json --source-root /tmp/pe7-rwe-pre-ac-source --harness-root .", "uv run --no-project python -m unittest discover -s tests -p test_agent_*.py"], "verification_family": "source_focused_full", "worker_tier": "T1"}
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
