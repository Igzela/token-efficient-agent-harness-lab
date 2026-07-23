# Agent Instructions

This repository is a local, deterministic, auditable Agent harness and workflow control plane. Rust `engine/` is the sole runtime, API, scheduler, policy, and application-owned storage authority; `LocalProductStore` is the sole persistence owner.

## Current Guardrails

- Product Golden Path is default-off and target `main` is protected. No provider call in CI, target-default-branch write, auto-merge, release, deployment, or production installation.
- Do not use Vader or Issue #208 as product runtime; Issue #254 is parked. Do not admit OpenCode, unpark Issue #254, replace the active Harness, or create a second runtime, scheduler, store, evaluator, workspace, output, audit, or rollback owner.
- Fixture/fake/proxy evidence is not managed acceptance. Never persist or report credentials, raw prompts/outputs/transcripts, private paths, or unredacted repository content.
- Level-2 remains blocked until the post-convergence RWE decision; Meta remains blocked until accepted Level-2, separate authority, and the unseen-task experiment. No recursive self-improvement claim is allowed without that experiment.
- Post-R7 wire/type governance hardening implemented: `scripts/check_wire_codegen_drift.sh`.

## Autonomous Operating Model

Full Agent Autonomy Mode covers repository-scoped, testable, observable, verification-gated, and rollbackable work. The agent may inspect, resolve bounded design gaps, implement, test, review, document, repair CI, create PRs, and manually merge eligible work. Material architecture, authority, schema, security, evaluator, release, or recovery decisions must be recorded in an existing authoritative document.

## Model Selection

Model and reasoning effort are user/tool settings. Do not edit model configuration to satisfy repository instructions; model choice never reduces testing, review, CI, audit, compatibility, compensation, or rollback.

## Active-Wait Advancement Rule

During CI, compilation, tests, or audits, do not wait passively. Refresh state, inspect the diff/contracts, prepare the next safe check, or repair a bounded prerequisite. Do not start a later packet or broaden authority merely to fill time.

## Ship PR Path

```text
audit → focused branch/PR → focused checks → exact-head CI → complete-diff review → manual squash merge → refresh main
```

Use one branch/PR per coherent packet. Auto-merge stays disabled. A new head invalidates prior CI and review. Rebase only for conflict, overlapping assumptions, explicit freshness, or proven integration risk.

Normal reversible repository work and already-configured local/GitHub services are pre-authorized. Confirmation is still required for irreversible destruction, production release/deploy, new paid-provider POST, credential creation/rotation/disclosure, protected force-push, or unbounded external effects.

## Execution-Ready Task Packets

Packet states: `READY_FOR_EXECUTION`, `BLOCKED_PREREQUISITE`, `DECISION_REQUIRED`, `IN_PROGRESS`, `COMPLETE`. Each packet records goal, owner paths, prerequisites, allowed changes, forbidden changes, versioned authority/budget/failure contracts, focused/full verification, compatibility, rollback, evidence, and stop triggers. Prefer the earliest eligible packet; do not begin later behavior before its predecessor is accepted.

## Full Agent Autonomy Mode

Autonomy does not authorize invented evidence, weakened fail-closed behavior, bypassed authority, sealed-evaluator changes, hidden failures, or parallel owners. Current code, merged history, tests, CI, and authoritative documents outrank stale claims.

## Hard Stops

- do not commit real secrets;
- do not falsify test or CI evidence;
- do not intentionally hide failures, rejected candidates, outcome-unknown effects, or safety regressions;
- do not remove rollback paths without a tested replacement;
- do not perform irreversible external destruction without a recovery path and explicit authority;
- do not weaken host, repository, path, binary, executor, model, tool, environment, credential, approval, budget, evaluator, or target-output boundaries;
- do not merge while required CI is failed, queued, in progress, cancelled, action-required, skipped, or under unresolved objection.

A difficult implementation or failed first attempt is not itself a blocker: diagnose, repair at root cause, and preserve scope. Stop on contradictory requirements, overlapping ownership, secret exposure, unprovable external-effect status, or missing recovery evidence.

## Autonomous Advancement Loop

1. Refresh `main`, branch/worktree, PRs/issues, CI, controls, and active documents.
2. Select the earliest eligible packet and audit its code/tests/owners.
3. State scope, non-goals, authority, risk, acceptance, rollback, and hard stops.
4. Add focused tests where practical; implement one coherent slice.
5. Run focused and applicable full checks; repair failures without weakening guards.
6. Review correctness, authority, security, compatibility, audit, recovery, cost, and rollback.
7. Update only the smallest authoritative documents; run handoff checks.
8. Commit in English, push/open the PR, wait for exact-head green CI and complete-diff review, merge only when eligible, refresh `main`, and report evidence.

## Reading and Verification

Always read `AGENTS.md`, then the relevant sections of `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/MODULE_MAP.md`. Read `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for PR/CI/merge work, `docs/ARCHITECTURE_BOOK.md` for architecture/authority/security/recovery, and `docs/RUNBOOK.md` only for proven operator procedures. Use targeted reads.

Baseline checks:

```bash
cargo fmt --all -- --check
cargo clippy -p engine --all-targets --all-features -- -D warnings
cargo test -p engine
cargo test -p engine --features pg-tests -- --test-threads=1
PYTHONPATH=src uv run --no-project python -m unittest discover -s tests
bash scripts/verify_rust_typescript_stack.sh
bash scripts/check_wire_codegen_drift.sh
uv run --no-project python tools/check_security_baseline.py
uv run --no-project python scripts/check_agent_handoff.py
git diff --check
```

Add migration, recovery, concurrency, browser, evaluator, or external-validation checks when relevant. Never claim a check, review, acceptance, cost, or benchmark that was not run and evidenced.

## Documentation Maintenance Rule

Keep the set small. Authoritative surfaces are `docs/ARCHITECTURE_BOOK.md` (durable architecture), `docs/CURRENT_STATUS.md` (facts), `docs/NEXT_DECISION.md` (single forward plan), `docs/MODULE_MAP.md` (ownership), `docs/REAL_WORLD_TESTING_PLAYBOOK.md` (PR/CI/merge), `docs/RUNBOOK.md` (proven procedures), and `README.md`/`CLAUDE.md`/`AGENTS.md` (entry boundaries). Prefer pruning or shortening stale text over adding documents. Documentation-only changes may use the playbook's direct-main exception only when the final diff is strictly prose and has handoff/diff checks plus a clear revert.
