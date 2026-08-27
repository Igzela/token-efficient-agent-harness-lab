# Next Decision

Last updated: 2026-08-27.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; blocked successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, ruleset, Issue, and mergeability facts require fresh GitHub readback.

## Current Direction

The owner-approved Autonomous Steward migration campaign supersedes the former 54-packet maintenance route without erasing accepted capability or evidence. The current window is PR 0 baseline recovery only: repair the observed Rust formatting failure on accepted `main`, make the `ask_sol` invalid-directory unit test incapable of reaching a real Provider, prove the complete canonical matrix, install and verify a recoverable `main` ruleset, reconcile stale control-plane PR/Issues, and keep MX1 parked through its exact recovery references. No Steward implementation, Provider call, product effect, release, deployment, production action, target write, or automatic merge is authorized.

## Authoritative Forward Order

```text
[completed: PE7-HE-MX1-CORE-1 — COMPLETE, provider-free accepted baseline]
[window: PE7-AUTONOMOUS-STEWARD-PR0 — READY_FOR_EXECUTION, baseline recovery and repository-control reconciliation]
```

## Active Routing

1. `PE7-AUTONOMOUS-STEWARD-PR0` — `READY_FOR_EXECUTION`

## Completed (PE7-HE-MX1-CORE-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #621 exact head `199c12756e58ffaa6041a22cd01f23ce7a1eda15`; merge `628577c5e8cb404c4dcc2e689925414bbfda70ab`; exact-head `PASS`; canonical workflow `32848799358`.

## Packet PE7-AUTONOMOUS-STEWARD-PR0

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-HE-MX1-CORE-1 — COMPLETE.

**Class:** `IMPLEMENT`

**Outcome:** Restore a trustworthy baseline for the migration campaign: format the exact failing Rust source, repair the invalid-directory `ask_sol` test so it cannot invoke a real model, obtain a successful complete canonical matrix on the unchanged PR head and refreshed `main`, enable and read back a recoverable `main` ruleset for the canonical exact-head/check contract, supersede stale PR #574 and Issue #623, stop using Issue #383 as an internal event log after a compact final snapshot, and preserve the unfinished MX1 chains as explicitly unaccepted migration inputs.

**Allowed delta:** `engine/src/product_golden_path.rs`, `tests/test_ask_sol.py`, `tests/test_session_context.py`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md`; repository-control mutations are limited to the exact `main` ruleset and PR/Issues #574, #623, and #383 after recovery snapshots and readback.

**Required ruleset checks:** Logical contract: `python-tests`, `rust-tests`, `pg-integration-tests`, `typescript-tests`, `native-runtime`, `docker-build`, `rust-typescript-cutover`, `exact-head-check`, and terminal `context-capsule`. GitHub ruleset contexts are the exact live check names: logical `exact-head-check` is supplied by context `exact-head`; every other logical name uses its same-named context. No bypass actor, force push, or direct-push exception is allowed.

**Exit:** Rust formatting and every applicable local baseline check pass; the stable PR head has independent exact `PASS` review and terminal-success canonical CI; the expected head is merged and refreshed `main` has terminal-success canonical CI; the `main` ruleset readback matches the required check contract without bypass; #574/#623 are superseded with recovery context; #383 has one compact terminal snapshot and receives no further internal receipts; both MX1 recovery refs resolve to their recorded exact heads.

**Stop:** Any required check name cannot be proved, ruleset mutation would lock out the guarded merge path or require an unreviewed bypass, GitHub mutation outcome is unknown, a stale PR/Issue contains unresolved owner work, MX1 recovery refs drift, main changes on an overlapping surface, or repair requires a semantic product/control-plane change beyond the observed formatting root cause.

### Twelve-field contract

1. **Outcome and non-goals.** Repair only the observed baseline failure and repository-control drift required by PR 0. Do not implement the Steward, change managed-provider semantics, resume MX1, call a Provider, release, deploy, write a target, or enable automatic merge.
2. **Prerequisites and evidence.** Accepted main `01a8acbccbfd21b8c06b12fc2f331549ffbb9783`; proposal digest `4b6eacaa4ff58337a02a6a73f458ffb0e4d3cb4e71f256c1024b3dd6205e1d39`; failed canonical workflow `32975735664`; locally reproduced `cargo fmt --all -- --check` failure only in `engine/src/product_golden_path.rs`; a local full-unit run proved `tests/test_ask_sol.py` can invoke real `codex exec` because a worktree subdirectory remains valid Git context; local handoff, toolchain drift, and security baseline passed; no branch protection or ruleset existed at inspection; PR #574 was the only open PR.
3. **Owners and paths.** Rust formatting owner: `engine/src/product_golden_path.rs`; provider-free investigation regression: `tests/test_ask_sol.py`; accepted truth and parked inputs: `docs/CURRENT_STATUS.md`; current execution: `docs/NEXT_DECISION.md`; blocked campaign order: `docs/FUTURE_ROUTE.md`; delivery owner: `docs/REAL_WORLD_TESTING_PLAYBOOK.md`; GitHub repository owner retains ruleset and Issue/PR authority.
4. **Frozen invariants.** Exact accepted-main, proposal, failed-run, archived-MX1, PR/Issue, ruleset snapshot, PR head, review, CI, and merge identities are immutable evidence; a new head or main invalidates stale conclusions.
5. **Only semantic delta.** Apply canonical Rust formatting to the observed source and reconcile repository-control state to the already documented exact-head/CI/review contract. The planning documents change routing, not runtime behavior.
6. **Forbidden changes.** No Steward implementation, second writer, provider/effect execution, credential disclosure, product semantic change, ruleset bypass, automatic merge, release, deployment, production action, target write, or deletion of recovery evidence.
7. **Ordered implementation slices.** Preserve and verify MX1 archive refs; create the isolated PR 0 branch from accepted main; apply only the observed Rust formatting delta; make the invalid-directory `ask_sol` test use a truly non-repository path and assert no model subprocess; run focused and full local verification with no real Provider binary; snapshot current ruleset/PR/Issue state and prepare an exact rollback payload; publish a Draft PR and converge independent exact-head review; mark Ready and obtain canonical exact-head CI; apply/read back the ruleset without bypass; reconcile #574, #623, and #383 with compact supersession evidence; merge only through the guarded expected-head path; refresh main and require terminal-success canonical CI; synchronize closeout truth without beginning PR 1.
8. **Failure, recovery, and stop taxonomy.** Ordinary formatting, test, review, CI, and base-drift failures are repaired inside PR 0. Before each GitHub mutation, persist the exact prior state; after ambiguous responses, read back rather than retry. Reopen PR/Issues and restore or delete only the PR-0-created ruleset from the recorded payload if rollback is required. Retain the MX1 archive refs and all evidence needed to classify unknown outcomes.
9. **Verification.** `cargo fmt --all -- --check`; `cargo clippy -p engine --all-targets --all-features -- -D warnings`; `cargo test -p engine`; `cargo test -p engine --features pg-tests -- --test-threads=1`; `PYTHONPATH=src uv run --no-project python -m unittest discover -s tests`; `bash scripts/verify_rust_typescript_stack.sh`; `bash scripts/check_wire_codegen_drift.sh`; `uv run --no-project python tools/check_security_baseline.py`; `uv run --no-project python scripts/check_agent_handoff.py`; `git diff --check`; exact GitHub ruleset, PR, Issue, workflow, head, and archived-ref readback.
10. **Compatibility, rollback, and retention.** Revert the bounded source/docs commit, restore the pre-mutation GitHub state from the private recovery snapshot, reopen superseded PR/Issues if required, retain both MX1 archive refs, and never run old and new lifecycle writers together. No data migration or product-state replay occurs in PR 0.
11. **Exit artifact.** Exact formatting diff, local verification receipt, ruleset before/after and rollback payload, PR/Issue reconciliation receipt, MX1 archive-ref readback, independent exact-head review, canonical PR/main CI, merge receipt, and refreshed accepted status.
12. **Next action.** Complete the governed PR 0 path and stop at its accepted closeout; PR 1 remains blocked until PR 0 evidence is canonical.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["Canonical formatting repair in engine/src/product_golden_path.rs.", "Provider-free invalid-directory regression in tests/test_ask_sol.py.", "Session-context route assertion synchronization in tests/test_session_context.py.", "Exact PR 0 verification, ruleset, reconciliation, rollback, and merge evidence.", "Canonical campaign routing with parked MX1 recovery references."], "allowed_paths": ["engine/src/product_golden_path.rs", "tests/test_ask_sol.py", "tests/test_session_context.py", "docs/CURRENT_STATUS.md", "docs/NEXT_DECISION.md", "docs/FUTURE_ROUTE.md"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["engine/src/product_golden_path.rs canonical formatting diff", "tests/test_ask_sol.py no-provider invalid-directory regression", "tests/test_session_context.py canonical route assertions", "main ruleset before-after and rollback receipt", "Required ruleset checks: python-tests, rust-tests, pg-integration-tests, typescript-tests, native-runtime, docker-build, rust-typescript-cutover, exact-head (logical exact-head-check), context-capsule", "PR 574 and Issues 623 and 383 reconciliation readback", "MX1 archive refs and exact heads", "exact-head review CI merge and refreshed-main receipts"], "external_effect_limit": 0, "forbidden_changes": ["Do not implement the Steward or create a second lifecycle writer.", "Do not change product or managed-provider semantics beyond canonical formatting.", "Do not call a Provider, execute a product effect, write a target, release, deploy, or enable automatic merge.", "Do not bypass review, CI, expected-head, ruleset, rollback, or unknown-outcome guards."], "forbidden_next_actions": ["Do not begin PE7-AUTONOMOUS-STEWARD-PR1 before PR 0 is accepted and closed.", "Do not resume the parked MX1 PILOT or treat archive refs as accepted capability.", "Do not retry an ambiguous GitHub mutation before factual readback.", "Do not delete the private recovery snapshot or MX1 archive refs during PR 0."], "goal": "Restore a green and server-enforced repository baseline before Autonomous Steward implementation begins.", "ordered_steps": ["Verify the accepted main and both MX1 archive refs.", "Apply canonical Rust formatting only to the observed failing source.", "Repair the ask_sol invalid-directory test so no real model subprocess can start.", "Run the declared focused and full local verification contract.", "Snapshot GitHub ruleset PR and Issue state with a tested rollback payload.", "Publish a Draft PR and obtain stable exact-head independent review.", "Mark Ready and require terminal-success canonical exact-head CI.", "Apply and read back the exact main ruleset without a bypass path.", "Supersede PR 574 and Issue 623 and freeze Issue 383 after a compact final snapshot.", "Merge only through the guarded expected-head path then refresh main and canonical CI.", "Record PR 0 closeout without starting PR 1."], "packet_id": "PE7-AUTONOMOUS-STEWARD-PR0", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop if a required check or guarded merge path cannot be proved before ruleset mutation.", "Stop if any GitHub mutation has unknown outcome until readback resolves it.", "Stop if PR or Issue reconciliation would discard unresolved owner work.", "Stop if MX1 archive refs drift or become unreachable.", "Stop before any Provider product effect release deployment target write or second writer."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PE7-HE-MX1-CORE-1 COMPLETE: PR #621 exact head `199c12756e58ffaa6041a22cd01f23ce7a1eda15`; merge `628577c5e8cb404c4dcc2e689925414bbfda70ab`; exact-head `PASS`; canonical workflow `32848799358`"], "prerequisites": ["PE7-HE-MX1-CORE-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "4b6eacaa4ff58337a02a6a73f458ffb0e4d3cb4e71f256c1024b3dd6205e1d39", "read_paths": ["engine/src/product_golden_path.rs", "tests/test_ask_sol.py", "tests/test_session_context.py", "docs/CURRENT_STATUS.md", "docs/NEXT_DECISION.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/ARCHITECTURE_BOOK.md", "docs/REAL_WORLD_TESTING_PLAYBOOK.md", ".github/workflows/tests.yml", ".github/workflows/exact-head-check.yml", "scripts/check_agent_handoff.py", "scripts/session_context.py", "tools/check_security_baseline.py"], "risk_class": "authority", "rollback": "Revert the bounded source and canonical-document commit, restore or delete only the PR-0-created ruleset from its exact private snapshot, reopen superseded PR or Issues when needed, and retain the MX1 archive refs and unknown-outcome evidence.", "route_manifest_sha256": "3a8203a6b745e68fac4418946558e37d49a6963978c6aaebc4f818a97bf0e67d", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["cargo fmt --all -- --check", "cargo clippy -p engine --all-targets --all-features -- -D warnings", "cargo test -p engine", "cargo test -p engine --features pg-tests -- --test-threads=1", "PYTHONPATH=src uv run --no-project python -m unittest discover -s tests", "bash scripts/verify_rust_typescript_stack.sh", "bash scripts/check_wire_codegen_drift.sh", "uv run --no-project python tools/check_security_baseline.py", "uv run --no-project python scripts/check_agent_handoff.py", "git diff --check"], "verification_family": "source_focused_full", "worker_tier": "T2"}
-->

## Common Execution Protocol

- Keep a changing PR Draft; batch repairs before final exact-head review and Ready CI.
- A new head invalidates prior review and CI; a new main invalidates stale baseline conclusions.
- The ruleset is applied only after its exact required checks and guarded merge path are proved with a rollback payload.
- GitHub API ambiguity requires readback before retry; `OUTCOME_UNKNOWN` never becomes success.
- PR 0 completion does not authorize PR 1, a Provider call, effect, target write, release, deployment, production action, or automatic merge.

## Hard Stops

- `DECISION_REQUIRED` on conflicting owner direction, unprovable required checks, missing rollback, unresolved stale work, overlapping main drift, secret exposure, or unknown external-mutation outcome.
- Never weaken exact-head review, canonical CI, expected-head merge, credential, effect, target, release, deployment, recovery, or single-writer boundaries.
- Never treat the plan, archive refs, branch-local prose, fixture evidence, or worker self-report as accepted capability.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` contains only blocked PR 1-7 routing. Promotion requires refreshed accepted PR 0 evidence and a new exact dispatch capsule.
