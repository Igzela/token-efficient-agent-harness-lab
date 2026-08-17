# Next Decision

Last updated: 2026-08-17.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

AC0 data/trace freeze, AC2 typed execution, AC3 Golden Path responsibility split, AC4 transaction views, AC5 composition root, and AC6 Rust-authoritative schema convergence are accepted on `main`. AC1 shared `ProcessSupervisor` remains deferred optional hardening. The AC7 removal manifest and deletion-only cleanup are accepted; `PE7-AC7-CLOSEOUT-1` is now the sole provider-free evidence/status window. No provider call, target write, authority consumption, or effect is authorized.

## Authoritative Forward Order

```text
[window: PE7-AC7-CLOSEOUT-1 — READY_FOR_EXECUTION, provider-free]
→ [successor: PE7-RWE-CR-RECONSTRUCTION-1 — BLOCKED_PREREQUISITE]

```

## Active Routing

1. `PE7-AC7-CLOSEOUT-1` — `READY_FOR_EXECUTION`

## Completed (PE7-AC6-COMPATIBILITY-CLOSEOUT-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #558 exact head `646e076cba8b349d45880dd84d4520109a11db69`; merge `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443`; exact-head `PASS`; canonical workflow `32006997709`.
## Completed (PE7-AC7-REMOVAL-MANIFEST-1)

**Historical state:** `COMPLETE`

**Accepted evidence:** PR #560 exact head `5567c670cb0338bf3bf089db95757714365829ec`; squash merge `eb692703ab3b3d030478b539fff4496014e45c7a`; exact-head review receipt comment `5314324232`; canonical workflow `32015963930`; exact-head check `32015963768`.

**Prerequisite:** PE7-AC6-COMPATIBILITY-CLOSEOUT-1 — COMPLETE on accepted main `73fed5fedf2361ee546b831b3e87acb6f0a096ec` (PR #558 exact head `646e076cba8b349d45880dd84d4520109a11db69`; merge `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443`; closeout PR #559 exact head `ea89603dbd25b16f958853a5425a5088b4352134`; canonical workflow `32013680486`; exact-head `PASS`).

**Class:** `CONTRACT`

**Outcome:** Freeze a deletion manifest grouped by one canonical owner and rollback point, with zero-caller proof and compatibility disposition per item.

**Allowed delta:** docs/ARCHITECTURE_BOOK.md, docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/MODULE_MAP.md, docs/NEXT_DECISION.md.

**Exit:** Exact files/symbols/tests/docs to delete, replacement owner, zero-caller proof, negative searches, fixture/script/SDK/Dashboard/replay checks, compatibility disposition per item, and batch order.

**Stop:** Any production, recovery, replay, fixture, script, or consumer dependency remains.

### Twelve-field contract

1. **Outcome and non-goals.** Freeze a deletion manifest owned by the AC7 architecture contract and grouped into runtime-owner evidence/rollback batches, with zero-caller proof and compatibility disposition per item; do not implement any runtime-owner deletion in this packet.
2. **Prerequisites and evidence.** Accepted main `73fed5fedf2361ee546b831b3e87acb6f0a096ec`; checked route manifest SHA `637bbc7b9c98021ce7af373fbfa04b7caa90a6024047bdd84b95dccb9ff5ac3e`; predecessor receipt PR #558 exact head `646e076cba8b349d45880dd84d4520109a11db69`; predecessor merge `4ea5f7707fa8c1f370cb8a8323c0b017bfcb3443`; closeout receipt PR #559 exact head `ea89603dbd25b16f958853a5425a5088b4352134`; canonical workflow `32013680486`; current-main evidence SHA `251c2c655078d715eb1dd954ffe2442426054f261d938b1b07390c3b3d9ac2f3`.
3. **Owner and paths.** Sole packet owner: the AC7 removal-manifest contract in `docs/ARCHITECTURE_BOOK.md`, projected by `docs/MODULE_MAP.md` and this route document. Runtime-owner evidence groups (not packet owners or ownership transfers): rollback group `ac7-http-compatibility-surface` covers `engine/src/http_server/routes.rs` route registration and `engine/src/http_server/handlers/product_tasks.rs::api_approve_and_output_product_task`, with authority test `product_approval_and_output_have_separate_authority_and_confirmation`; rollback group `ac7-local-store-compatibility` covers `engine/src/storage/local_product_store/product_tasks.rs::approve_and_output_product_task_for_tenant` and `::approve_and_output_product_task`, with the exact evidence/G3/recovery test functions enumerated in `docs/ARCHITECTURE_BOOK.md`; rollback group `ac7-consumer-compatibility-surface` covers Python `approve_and_output_product_task`, TypeScript `approveAndOutputProductTask`, and Dashboard `approveAndOutputProductTask`; replacements are the existing separate approve/output pairs.
4. **Frozen invariants.** Packet identity, sole manifest-contract owner, route manifest SHA `637bbc7b9c98021ce7af373fbfa04b7caa90a6024047bdd84b95dccb9ff5ac3e`, accepted-main SHA `73fed5fedf2361ee546b831b3e87acb6f0a096ec`, predecessor receipt, closeout receipt, CodeGraph call path, exact caller/test inventory, runtime-owner group boundaries, and current-main evidence digest are immutable for this candidate.
5. **Only semantic delta.** Execute only the independently reviewed candidate contract.
6. **Forbidden changes.** No static route hint is authority; no effect, T3 action, provider, target, automatic merge, or second owner.
7. **Ordered implementation slices.** `docs/ARCHITECTURE_BOOK.md`: freeze the three exact AC7 rollback groups, replacements, compatibility dispositions, rollback point, and zero-caller/negative-search gate; `docs/MODULE_MAP.md`: bind the owner boundary; `docs/NEXT_DECISION.md`: bind the exact candidate inventory and successor cleanup batch order.
8. **Failure, recovery, and stop taxonomy.** Cleanup: No temporary resources created (proved by docs/ARCHITECTURE_BOOK.md:cleanup); retention: Retain canonical schemas and audit trail invariants (proved by docs/ARCHITECTURE_BOOK.md:audit); decisions: authority unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore); evaluator unchanged (docs/ARCHITECTURE_BOOK.md:evaluator); recovery unchanged (docs/ARCHITECTURE_BOOK.md:rollback); schema unchanged (docs/ARCHITECTURE_BOOK.md:LocalProductStore).
9. **Verification.** Candidate inventory must bind the exact route, handler, both LocalProductStore compatibility symbols, Python/TypeScript/Dashboard wrappers, one authority assertion, five evidence functions, four G3 functions, and two recovery functions. On accepted `main` `73fed5fedf2361ee546b831b3e87acb6f0a096ec`, run `codegraph explore "approve_and_output_product_task approve_and_output_product_task_for_tenant api_approve_and_output_product_task POST /api/v1/product/tasks/:task_id/approve-and-output"` and bind the route → handler → tenant-helper → compatibility-helper path in the review evidence. Contract checks: `bash scripts/check_wire_codegen_drift.sh`; `bash scripts/verify_rust_typescript_stack.sh`; `uv run --no-project python tools/check_security_baseline.py`; `uv run --no-project python scripts/check_agent_handoff.py`; `git diff --check`. The successor `PE7-AC7-CLEANUP-1` must additionally prove a zero-match fixed-string search across `engine/src`, `engine/tests`, `sdk`, `dashboard`, `scripts`, `tools`, and `tests` after deletion, run the candidate-specific Rust authority/behavior/recovery tests, the Python SDK tests, the TypeScript/Dashboard checks, and the applicable fixture/script/replay checks before closeout.
10. **Compatibility, rollback, and retention.** Revertable documentation diff with zero database migrations (proved by docs/ARCHITECTURE_BOOK.md:rollback)
11. **Exit artifact.** Evidence destinations: accepted AC6 closeout and PR #559 receipt in `docs/CURRENT_STATUS.md`; the accepted AC7 manifest, owner boundary, PR #560 receipt, and cleanup promotion are now synchronized in the canonical documents.
12. **Next action.** Execute the promoted deletion-only cleanup packet under its separate owner-scoped batch and rollback gates.

## Completed (PE7-AC7-CLEANUP-1)

**Historical state:** `COMPLETE`

**Accepted evidence:** PR #562 exact head `84735a064466b81a5bf521cf20b1a924c80408e6`; squash merge `8142a447c1b9ca861978bd3392da5ccea4263924`; exact-head review receipt comment `5315606973`; canonical workflow `32026577558`; exact-head check `32026577560`; merged-tree negative search zero matches; 10 files changed, `+65/-206`; no schema/migration, Provider call, target write, or effect.

**Rollback and convergence:** The pre-cleanup rollback tree is `eb692703ab3b3d030478b539fff4496014e45c7a`; accepted docs-only descendant before cleanup is `962aa81855673f7a2c0f5e72958ec24726e32c78`. Separate approve/output authority, audit, CAS/idempotency, and recovery paths remain present.

## Packet PE7-AC7-CLOSEOUT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-AC7-CLEANUP-1` — COMPLETE on accepted main `8142a447c1b9ca861978bd3392da5ccea4263924` (PR #562 exact head `84735a064466b81a5bf521cf20b1a924c80408e6`; squash merge `8142a447c1b9ca861978bd3392da5ccea4263924`; exact-head review receipt comment `5315606973`; canonical workflow `32026577558`; exact-head check `32026577560`).

**Class:** `CLOSEOUT`

**Outcome:** Independently verify AC7 convergence completeness, aggregate implementation cost and rollback evidence, and bind the contemporary old/new RWE replay inputs without claiming that the replay or a Provider effect has run.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md` evidence/status synchronization only.

**Exit:** Accepted AC7 closeout receipt, zero unowned compatibility island, implementation-cost aggregation, rollback index, and contemporary-replay inputs.

**Stop:** An obsolete path still executes, separate authority/order or golden traces differ without accepted reason, the pre-AC identity is not reconstructable, or any proposed step requires a Provider, target write, authority consumption, or effect.

### Twelve-field contract

1. **Outcome and non-goals.** Close out the already-merged deletion-only cleanup; do not change runtime code, schemas, migrations, authority, evaluator, corpus, protocol, or Provider routing, and do not run Contemporary RWE.
2. **Prerequisites and evidence.** Accepted main `8142a447c1b9ca861978bd3392da5ccea4263924`; cleanup PR #562 exact head `84735a064466b81a5bf521cf20b1a924c80408e6`; merge `8142a447c1b9ca861978bd3392da5ccea4263924`; exact-head review receipt `5315606973`; canonical workflow `32026577558`; exact-head check `32026577560`; refreshed Future Route inventory manifest SHA `1a26757d57ba1a232f1f605f1b4390005fb4924d30a2ae3af4354ba1617193bf`.
3. **Owners and paths.** Evidence/status owners are `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`; pre-AC identity input is `engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json` under the existing RWE snapshot owner; provider-free validation is `scripts/verify_rwe_snapshot.py`; post-AC identity is the accepted `token-efficient-agent-harness-lab@8142a447` tree.
4. **Frozen invariants.** The AC7 manifest groups, separate approve/output authority order, audit/CAS/idempotency/recovery semantics, pre-cleanup rollback tree, and zero-match inventory remain unchanged; this packet adds evidence only.
5. **Only semantic delta.** Replace stale route/status claims with the exact accepted cleanup receipt, cost, rollback, and replay-input identities.
6. **Forbidden changes.** No runtime source/test/SDK/Dashboard change, schema/migration/generated-wire change, Provider call, credential read/output/persistence, target write, EFFECT/T3 action, authority consumption, automatic merge, or second owner.
7. **Ordered closeout slices.** (1) prove the merged-tree zero-match inventory and preserved separate paths; (2) record the exact cleanup diff cost, owner-batch attribution, complete `implementation_cost_receipt`, and rollback index; (3) record the accepted pre-AC reconstruction manifest and post-AC main/tree/toolchain identities; (4) leave Contemporary RWE reconstruction and all later EFFECT packets blocked until separately promoted.
8. **Failure, recovery, and stop taxonomy.** A docs-only closeout change is reverted if any receipt, hash, path, rollback, or identity is inconsistent. The accepted pre-cleanup tree remains the code rollback point. The pre-AC source checkout is not present in this repository, so the snapshot verifier's unavailable-source result is recorded as a prerequisite for the later reconstruction packet, never as replay success.
9. **Verification.** Re-run the fixed-string zero-match search over `engine/src`, `engine/tests`, `sdk`, `dashboard`, `scripts`, `tools`, and `tests`; verify separate approve/output symbols; bind `git diff --shortstat 962aa818... 8142a447` (`10 files, +65/-206`), the complete `implementation_cost_receipt` fields in `docs/CURRENT_STATUS.md`, accepted main/tree identity, and `pre_ac_harness_snapshot.v2.json` fields `RECONSTRUCTABLE=true`, source commit `6240768506320a324d68787b9eaa86971c8c930c`, manifest `a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c`; cleanup's exact-head canonical matrix remains the behavioral evidence.
10. **Compatibility, rollback, and retention.** Revert this documentation PR to remove only its evidence/status delta; revert cleanup code to the accepted pre-cleanup tree `eb692703ab3b3d030478b539fff4496014e45c7a` if a later accepted decision requires rollback; retain the snapshot manifest and all redacted/hash-bound evidence.
11. **Exit artifact.** Record the exact cleanup head/merge, zero-match result, complete `implementation_cost_receipt` with the three owner-batch patch-size attributions and explicit unavailable per-owner lifecycle-cost allocation, review/CI receipts, rollback tree, pre-AC manifest, post-AC accepted-main identity `8142a447` with tracked-manifest hash `6e883449d1beceb29c4cf114aad075ed6de0b3845f91d9e64059956173e2b7d6`, Git tree `df6ede05fdc6f1a533793a17591478ac5ecc7b9d`, `Cargo.lock` hash `cf68982734f8a72148950f119408b676dd5b42ce65d7af69c02eca017a551653`, and `rust-toolchain.toml` hash `e59c5da37d1f9f4e0f815bc188cb6056fc7410c9cdaa9673c2d44da557c75d12`.
12. **Next action.** After this closeout is independently reviewed, canonically merged, and refreshed on main, promote `PE7-RWE-CR-RECONSTRUCTION-1`; do not run `PE7-RWE-CR-RUN-1` or any HE packet before its own contract/preflight gates.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{
  "allowed_outputs": [
    "Provider-free AC7 convergence closeout with exact accepted receipts, cost, rollback, and replay-input evidence.",
    "A canonical route promotion to PE7-RWE-CR-RECONSTRUCTION-1 only after this closeout is accepted."
  ],
  "allowed_paths": [
    "docs/CURRENT_STATUS.md",
    "docs/NEXT_DECISION.md",
    "docs/FUTURE_ROUTE.md"
  ],
  "authority_consumption_allowed": false,
  "dispatch_lane": "provider_free_repository_maintenance",
  "expected_artifacts": [
    "Exact AC7 cleanup receipt and merged-tree zero-match inventory in docs/CURRENT_STATUS.md.",
    "Implementation-cost and rollback index bound to the accepted cleanup head.",
    "Hash-bound pre-AC snapshot and post-AC accepted-main identity inputs for later Contemporary RWE reconstruction."
  ],
  "external_effect_limit": 0,
  "forbidden_changes": [
    "Do not modify runtime code, schema, migration, evaluator, corpus, protocol, or Provider route.",
    "Do not call a Provider, read or persist credentials, write a target, consume authority, execute an EFFECT/T3 action, or auto-merge.",
    "Do not promote or run Contemporary RWE or Harness Evolution before this closeout is accepted."
  ],
  "forbidden_next_actions": [
    "Do not treat the pre-AC snapshot manifest as a completed replay.",
    "Do not start PE7-RWE-CR-RECONSTRUCTION-1 until this closeout is accepted.",
    "Do not skip PE7-RWE-CR-PROTOCOL-PREFLIGHT-1 or execute PE7-RWE-CR-RUN-1.",
    "Do not create a second runtime, store, controller, evaluator, or rollback owner."
  ],
  "goal": "Close out the accepted AC7 deletion while preserving exact rollback and reconstructable old/new Harness identities.",
  "ordered_steps": [
    "Prove zero callers and preserved separate approve/output paths on accepted main.",
    "Bind cleanup cost, review/CI/merge receipts, rollback tree, and documentation owners.",
    "Bind the accepted pre-AC snapshot and current post-AC main identities as provider-free replay inputs.",
    "Keep all Contemporary RWE and HE effects blocked behind their own promoted contracts and preflights."
  ],
  "packet_id": "PE7-AC7-CLOSEOUT-1",
  "packet_state": "READY_FOR_EXECUTION",
  "pause_gates": [
    "Stop on any inconsistent receipt, digest, path, owner, or rollback identity.",
    "Stop if source reconstruction is unavailable or changes behavior; record the gap and remain blocked.",
    "Stop before any Provider, target, authority, EFFECT, T3, automatic merge, or external effect."
  ],
  "plan_lane_state": "plan_lane_active",
  "prerequisite_receipts": [
    "PE7-AC7-CLEANUP-1 COMPLETE: PR #562 exact head `84735a064466b81a5bf521cf20b1a924c80408e6`; squash merge `8142a447c1b9ca861978bd3392da5ccea4263924`; exact-head review receipt comment `5315606973`; canonical workflow `32026577558`; exact-head check `32026577560`; merged-tree fixed-string inventory zero matches; 10 files, `+65/-206`; no schema/migration, Provider call, target write, or effect"
  ],
  "prerequisites": [
    "PE7-AC7-CLEANUP-1"
  ],
  "private_paths_allowed": false,
  "read_paths": [
    "docs/CURRENT_STATUS.md",
    "docs/NEXT_DECISION.md",
    "docs/FUTURE_ROUTE.md",
    "engine/rwe/corpora/rwe-minimum-first-corpus/v2/snapshot/pre_ac_harness_snapshot.v2.json",
    "scripts/verify_rwe_snapshot.py",
    "scripts/check_agent_handoff.py"
  ],
  "risk_class": "none",
  "rollback": "Revert the documentation/status PR; the accepted cleanup rollback point remains eb692703ab3b3d030478b539fff4496014e45c7a and no database migration or external effect is involved.",
  "route_manifest_sha256": "1a26757d57ba1a232f1f605f1b4390005fb4924d30a2ae3af4354ba1617193bf",
  "schema_version": "weak_agent_dispatch.v1",
  "secret_values_allowed": false,
  "verification": [
    "fixed-string zero-match search across engine/src engine/tests sdk dashboard scripts tools tests",
    "separate approve/output symbol and route-preservation check",
    "accepted cleanup receipt, exact diff cost, rollback tree, snapshot manifest, and post-AC identity consistency check",
    "uv run --no-project python scripts/check_agent_handoff.py",
    "git diff --check"
  ],
  "verification_family": "evidence_review",
  "worker_tier": "T2"
}
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
