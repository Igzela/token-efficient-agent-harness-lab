# Agent Instructions

Read `START_HERE.md` first. It is the canonical navigation entry for every planning, implementation, review, CI-repair, and operator session. This file owns implementation-agent permissions and workflow; it does not duplicate current status or the forward plan.

This repository is a local, deterministic, auditable Agent harness and workflow control plane. Rust `engine/` is the sole runtime, API, scheduler, policy, and application-owned storage authority; `LocalProductStore` is the sole persistence owner.

## Quality and Frontier Rule

Use this priority order:

```text
correctness, safety, evidence, recovery, and rollback
→ architecture and authority integrity
→ maintainability and one canonical owner
→ low duplication and low context cost
```

Conciseness must preserve quality. At session start, establish the leading valid frontier: remote accepted `main`, the latest exact head of the earliest eligible packet’s owned PR, and any blocked future frontiers. Never continue from a stale local branch, stale review head, or blocked downstream PR. Enter through the `START_HERE.md` one-command bootstrap (`scripts/session_context.py enter --role <role>` for the digest-bound route, or `route` for non-coding roles), and use `uv run --no-project python scripts/project_context.py` as an on-demand frontier-evidence view only when the entry asks for it; verify its claims against Git/GitHub and canonical documents. Repository-controlled implementation, CI-repair, and review prompts regenerate and inject a fresh validated capsule at session start; arbitrary later sessions still require explicit regeneration.

## Current Guardrails

- Product Golden Path is default-off and target `main` is protected. No provider call in CI, target-default-branch write, auto-merge, release, deployment, or production installation. A local Provider call is permitted only when the accepted current packet's weak-agent dispatch capsule explicitly authorizes one; that exception never authorizes a target write, EFFECT, T3 action, release, deployment, or automatic merge.
- Do not use Vader or Issue #208 as product runtime; Issue #254 is parked. OpenCode may serve only as the explicitly packet-authorized local repository-maintenance weak worker through the existing controller and worktree boundary. It must not become a product runtime or create a second runtime, scheduler, store, evaluator, workspace, output, audit, or rollback owner.
- Fixture/fake/proxy evidence is not managed acceptance. Never persist or report credentials, raw prompts/outputs/transcripts, private paths, or unredacted repository content.
- Level-2 remains blocked until the post-convergence RWE decision; Meta remains blocked until accepted Level-2, separate authority, and the unseen-task experiment. No recursive self-improvement claim is allowed without that experiment.
- Post-R7 wire/type governance hardening implemented: `scripts/check_wire_codegen_drift.sh`.

## Autonomous Operating Model

Full Agent Autonomy Mode covers repository-scoped, testable, observable, verification-gated, and rollbackable work. The agent may inspect, resolve bounded design gaps, implement, test, review, document, repair CI, create PRs, and manually merge eligible work. Material architecture, authority, schema, security, evaluator, release, or recovery decisions must be recorded in an existing authoritative document.

## Planning and Execution Boundary

Cross-packet direction, architecture and authority choices, packet goal, prerequisites, ordering, acceptance criteria, and GO/NO-GO decisions belong to the planning or architecture process and their canonical document owners. The implementation agent may decide how to execute the accepted packet, but may not silently decide what the project should do next.

Packet-internal judgment may choose implementation details, reuse existing owners, add focused tests, repair root causes, and synchronize the smallest canonical documents. It must not broaden scope, reorder packets, create a parallel owner, or change a schema, durable contract, security/recovery boundary, acceptance gate, or strategic claim unless the active packet and verified authority explicitly permit that change.

When a necessary change crosses that boundary, stop with `DECISION_REQUIRED`; report the evidence, available options, consequences, and smallest proposed packet/doc update. Do not implement the proposed direction or mark it accepted before the planning process or canonical owner authorizes it.

## Model Selection

Model and reasoning effort are user/tool settings. Do not edit model configuration to satisfy repository instructions; model choice never reduces testing, review, CI, audit, compatibility, compensation, or rollback.

## Active-Wait Advancement Rule

During CI, compilation, tests, or audits, do not wait passively. Refresh state, inspect the diff/contracts, prepare the next safe check, or repair a bounded prerequisite. Do not start a later packet or broaden authority merely to fill time.

## Ship PR Path

```text
audit → focused Draft PR → focused checks → stable-head complete-diff review → Ready → canonical exact-head CI → manual squash merge → refresh main
```

Use one branch/PR per coherent packet. Auto-merge stays disabled. A new head invalidates prior CI and review. Rebase only for conflict, overlapping assumptions, explicit freshness, or proven integration risk.

Review has no `COMPLETE` verdict. Exact `PASS` binds only one reviewed head and satisfies only the independent-review gate. Keep `packet_state` separate from `independent_review`; packet lifecycle `COMPLETE` requires accepted canonical owners to establish merge, verification, review, and documentation.

Normal reversible repository work and already-configured local/GitHub services are pre-authorized. Confirmation is still required for irreversible destruction, production release/deploy, new paid-provider POST, credential creation/rotation/disclosure, protected force-push, or unbounded external effects.

## CI Lane Discipline

Keep an implementation PR in Draft while its diff is changing. The `pr-fast-checks` workflow enforces that `opened`, `synchronize`, and `reopened` heads are Draft; a non-Draft mutating event fails immediately and must be corrected by converting the PR to Draft. Draft pushes provide exact-head governance feedback only and cannot authorize review, merge, release, deployment, or acceptance.

Run focused and applicable full checks locally, finish the complete-diff review, post the exact-head review receipt on the stable head (exact SHA, complete diff, axes, outcome — see `docs/REAL_WORLD_TESTING_PLAYBOOK.md`), collect all known findings into one repair batch, and only then mark the PR Ready once. A replacement head invalidates the prior review receipt; re-receipt the new exact head before Ready. The `ready_for_review` event triggers the single canonical `tests` workflow. Its trusted classifier selects complete source tests or strict documentation-only mode. On a normal `main` push, the accepted-before verifier may instead reuse the tree-identical merged PR's already-successful canonical matrix only when the unique PR, exact tree, strict `PASS`, exact-head check, and every required job are re-proved; control-plane changes and uncertainty force the complete matrix. Explicit fallback dispatches always use the complete matrix.

A documentation-only candidate remains canonical CI, not fast feedback. Classification is computed from the accepted base checkout, not candidate-controlled classifier code. Every required job must still check out and verify the exact head and finish successfully; only compiler, runtime, database-test, TypeScript, Docker-build, and other non-applicable source-test steps are skipped. Empty, mixed, executable, symlink, submodule, workflow, script, test, configuration, dependency, schema, migration, generated, or otherwise uncertain diffs fail closed to the complete matrix.

Rust source lanes use the pinned `sccache` action, and the Docker source lane uses pinned Buildx/Bake actions with separately scoped engine and Dashboard layer caches. These GitHub Actions caches are performance optimizations only. Cache contents, hit rates, misses, and statistics are never test or acceptance evidence. A cache failure may not turn a required command into success, and cached outputs never replace exact-head checkout, compilation, image builds, tests, audit, or review.

Do not push one repair at a time while CI is running. If a Ready candidate needs code or document changes, convert the PR back to Draft first, batch the repairs, validate locally, publish one replacement head, and mark it Ready again. A new head automatically cancels obsolete in-progress runs. Never restart an unchanged successful job or duplicate a workflow dispatch. Infrastructure-only failures may rerun only the failed job; code or test failures require a repaired head.

A missing canonical PR `tests` run is not success. A fast-check success, a Draft head, a stale run, a prior-head review, or a skipped required exact-head proof cannot satisfy merge eligibility. Documentation-only and post-merge tree-equivalence modes remain governed by `docs/REAL_WORLD_TESTING_PLAYBOOK.md`; neither turns `pr-fast-checks`, a cache, or an unverified prior run into canonical evidence.

## Execution-Ready Task Packets

Packet states: `READY_FOR_EXECUTION`, `BLOCKED_PREREQUISITE`, `DECISION_REQUIRED`, `IN_PROGRESS`, `COMPLETE`. These are lifecycle states owned by `docs/NEXT_DECISION.md`, not review verdicts. Each packet records goal, owner paths, prerequisites, allowed changes, forbidden changes, versioned authority/budget/failure contracts, focused/full verification, compatibility, rollback, evidence, and stop triggers. Prefer the earliest eligible packet; do not begin later behavior before its predecessor is accepted.

## Full Agent Autonomy Mode

Autonomy does not authorize invented evidence, weakened fail-closed behavior, bypassed authority, sealed-evaluator changes, hidden failures, or parallel owners. Current code, merged history, tests, CI, and authoritative documents outrank stale claims.

## Hard Stops

- do not commit real secrets;
- do not falsify test or CI evidence;
- do not intentionally hide failures, rejected candidates, outcome-unknown effects, or safety regressions;
- do not remove rollback paths without a tested replacement;
- do not perform irreversible external destruction without a recovery path and explicit authority;
- do not weaken host, repository, path, binary, executor, model, tool, environment, credential, approval, budget, evaluator, or target-output boundaries;
- do not merge while required CI is failed, queued, in progress, cancelled, action-required, skipped, missing, or under unresolved objection;
- do not treat an aggregate approval label as exact-head independent acceptance unless its commit binding and unresolved-objection state are verified.

A difficult implementation or failed first attempt is not itself a blocker: diagnose, repair at root cause, and preserve scope. Stop on contradictory requirements, overlapping ownership, secret exposure, unprovable external-effect status, or missing recovery evidence.

## Autonomous Advancement Loop

1. Generate or inspect a fresh context capsule; refresh remote `main`, the owned PR exact head, dependencies, required CI, exact-head reviews, unresolved objections, controls, and active documents from the accepted baseline.
2. Select the earliest eligible packet and audit its code, tests, owners, prior attempts, and planned deletions.
3. State scope, non-goals, authority, risk, acceptance, rollback, hard stops, and the simplest quality-preserving implementation path.
4. Prefer deletion, reordering, interface tightening, and reuse before adding mechanisms. Add focused negative tests where practical; implement one coherent slice.
5. Run focused and applicable full checks; repair failures without weakening guards.
6. Review correctness, authority, security, compatibility, audit, recovery, cost, rollback, and SQLite/PostgreSQL parity where applicable.
7. Update only the smallest canonical documents; replace stale status instead of appending history; run handoff checks.
8. Commit in English, push/open the PR, wait for exact-head green required CI and complete-diff review, merge only when eligible and explicitly authorized, refresh `main`, and report the handoff capsule fields from `START_HERE.md`.

## Reading and Verification

After `START_HERE.md`, read the relevant sections of `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/MODULE_MAP.md`. Read `docs/FUTURE_ROUTE.md` only when planning or refreshing the successor packet; it never authorizes implementation. Read `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for PR/CI/merge work, `docs/ARCHITECTURE_BOOK.md` for architecture/authority/security/recovery, and `docs/RUNBOOK.md` only for proven operator procedures. Use targeted reads.

Baseline checks:

```bash
cargo fmt --all -- --check
cargo clippy -p engine --all-targets --all-features -- -D warnings
scripts/ci/run_rust_tests.py
cargo test -p engine --features pg-tests -- --test-threads=1
PYTHONPATH=src uv run --no-project python -m unittest discover -s tests
bash scripts/verify_rust_typescript_stack.sh
bash scripts/check_wire_codegen_drift.sh
uv run --no-project python tools/check_security_baseline.py
uv run --no-project python scripts/check_agent_handoff.py
git diff --check
```

New dormant production surfaces (module-level dead-code blankets, module islands, self-described placeholder modules, no-op executors, conflicting sole-owner claims) fail closed in `tools/check_security_baseline.py`; exceptions require a classification entry with owner, reason, review condition, and expiry/recheck condition.

Add migration, recovery, concurrency, browser, evaluator, or external-validation checks when relevant. Never claim a check, review, acceptance, cost, or benchmark that was not run and evidenced.

## Documentation Maintenance Rule

Keep the set small and role-specific. `START_HERE.md` owns navigation and frontier discovery; `docs/ARCHITECTURE_BOOK.md` durable architecture; `docs/CURRENT_STATUS.md` accepted truth and confirmed gaps; `docs/NEXT_DECISION.md` one current executable window; `docs/FUTURE_ROUTE.md` the blocked routing-only horizon; `docs/MODULE_MAP.md` accepted ownership; `docs/REAL_WORLD_TESTING_PLAYBOOK.md` PR/CI/merge; `docs/RUNBOOK.md` proven procedures; `README.md`, `CLAUDE.md`, and this file are entry adapters. Live PR, CI, review, and mergeability facts belong only in a fresh capsule.

One fact has one full owner. Other documents link rather than copy. Quality outranks brevity, but stale history, duplicate policy, and branch-local status must be pruned. Documentation-only changes may use the playbook's direct-main exception only when the final diff is strictly prose and has handoff/diff checks plus a clear revert.
