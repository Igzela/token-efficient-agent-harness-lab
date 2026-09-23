# Agent Instructions

Read `START_HERE.md` first. It is the canonical navigation entry and defines the role-specific route, source-of-truth hierarchy, frontier discovery, and handoff procedure. This file owns only stable implementation-agent permissions and stop boundaries; it does not own architecture detail, roadmap, or operational runbooks.

This repository is a local, deterministic, auditable Agent harness and workflow control plane. Rust `engine/` is the sole runtime, API, scheduler, policy, and application-owned storage authority; `LocalProductStore` is the sole persistence owner.

## Quality and Frontier Rule

Use this order: correctness, safety, evidence, recovery, and rollback → architecture and authority integrity → maintainability and one canonical owner → low duplication and low context cost. Conciseness must preserve quality.

Before acting, establish the **leading valid frontier** from accepted remote
`main`, the current checkout branch/HEAD, and the task stated by
the user's request or `START_HERE.md`. Read the relevant owner docs, code, and
tests directly. Generated capsules, local checkpoints, and task cards are not
entry requirements or authority.

## Autonomous Operating Model

**Full Agent Autonomy Mode** permits repository-scoped work that is testable, observable, reviewable, verification-gated, and rollbackable. An agent may inspect, plan task-internal execution, **resolve bounded design gaps**, implement, test, document, repair CI, create a focused PR, obtain independent review, and merge only when the canonical gates pass.

Normal reversible repository work and already-configured local/GitHub services are pre-authorized. Explicit confirmation remains required for irreversible destruction, production release/deploy, a new paid-provider POST, credential creation/rotation/disclosure, protected force-push, or unbounded external effects. Model and reasoning settings are user/tool choices: do not edit model configuration to satisfy repository instructions, and never reduce scope, test, review, audit, compatibility, recovery, or rollback requirements because of model choice.

## Authority and Ownership Boundaries

- Cross-mission direction, architecture/authority choices, scope, prerequisites, ordering, acceptance criteria, schema or durable-contract changes, security/recovery boundaries, and GO/NO-GO decisions belong to planning and their canonical owners.
- Task-internal implementation may reuse existing owners, choose local design details, add focused tests, repair root causes, and synchronize canonical documents. It may not broaden scope, create a parallel owner, or silently change a durable boundary.
- Repository automation or any external/experimental harness or model may be only an admitted bounded worker or experimental subject. None may become a second runtime, scheduler, store, evaluator, budget, approval, workspace, output, audit, rollback, merge, release, or deployment owner.
- Product/provider, managed acceptance, credential, redaction, and recovery rules are owned by `docs/ARCHITECTURE.md`; autonomy, testing, review, and merge rules by `docs/AUTONOMY.md`; closed-loop milestones and research programs by `docs/ROADMAP.md`.
- Never persist or report credentials, raw prompts/outputs/transcripts, private paths, or unredacted repository content.

If work needs to cross any boundary above, stop with `DECISION_REQUIRED` or `PAUSED_FOR_OWNER`: report evidence, options, consequences, and the smallest proposed owner update. A proposal is not accepted authority.

## Ordinary Repository Maintenance

Read `START_HERE.md`, inspect the relevant source and tests, choose one
coherent bounded change, and verify it locally. No Mission, Stage, WorkCard,
generated task card, journal, or checkpoint is required to start coding. Keep
the change on a non-main branch, protect `.git` and `.github`, preserve
rollback, and follow the repository's ordinary review, CI, and merge gates.
Review `PASS` is evidence for one exact head; it is not a substitute for
testing or delivery acceptance.

## Research execution continuation

For a finite owner-authorized experiment, follow the experiment-specific
procedure in `docs/AUTONOMY.md` and `docs/RUNBOOK.md`. Research execution is
separate from ordinary coding and must not turn the repository entrypoint into
a task-card workflow; pause only at the documented provider, credential,
effect, budget, or recovery boundaries.

## Investigation Escalation (`ask_sol`)

Routine work proceeds directly. For genuinely difficult uncertainty, contradictory evidence, cross-module ambiguity, or failed initial hypotheses, use the bounded read-only investigator:

```bash
scripts/ask_sol "<investigation goal>" [--hypothesis "<hypothesis>"] [--task-id "<task_id>"]
```

It cannot mutate, approve, or grant authority; verify its result against first-party evidence. The detailed recursion, attempt, redaction, and state-integrity contract is owned by `docs/ARCHITECTURE.md`.

## Delivery and Verification

The complete Draft/Ready, exact-head review, CI, merge, rebase, recovery, and rollback contract is owned by `docs/AUTONOMY.md`. Invariant summary: one coherent branch/PR; keep changing work Draft; a new head invalidates prior review and CI; mark Ready only after local verification and complete-diff independent review; require canonical exact-head CI; merge manually only when eligible; refresh `main` afterward.

Run the accepted task's focused and full verification contract and the applicable **Verification Baseline**; a task may add checks but may not silently omit an applicable one. `scripts/check_wire_codegen_drift.sh`, security baseline, agent handoff, and `git diff --check` remain fail-closed where applicable. Never claim a command, result, review, cost, benchmark, effect, or acceptance that was not actually observed.

## Hard Stops

- do not commit real secrets;
- do not falsify test or CI evidence;
- do not intentionally hide failures, rejected candidates, outcome-unknown effects, or safety regressions;
- do not remove rollback paths without a tested replacement;
- do not perform irreversible external destruction without a recovery path and explicit authority;
- do not weaken host, repository, path, binary, executor, model, tool, environment, credential, approval, budget, evaluator, target-output, redaction, or recovery boundaries;
- do not perform a provider call, target write, EFFECT/T3 action, release, deployment, production installation, or protected-default-branch mutation without current explicit authority;
- do not merge while required CI/review evidence is failed, missing, stale, non-terminal, cancelled, skipped, unresolved, or bound to another head;
- do not treat aggregate approval, worker self-report, a capsule, fixture evidence, or branch-local prose as accepted exact-head authority.

A difficult implementation or failed first attempt is not a blocker: diagnose and repair within scope. Stop on contradictory requirements, overlapping ownership, secret exposure, unprovable external-effect status, missing recovery evidence, or a required decision outside the accepted scope.

## Autonomous Advancement Loop

1. Read `START_HERE.md`; refresh accepted main, the exact worktree/PR head, dependencies, reviews, CI, objections, and canonical owners.
2. Use the user's request, or the next unblocked product/maintenance roadmap item, as the task; state scope, non-goals, authority, risk, acceptance, rollback, and hard stops.
3. Reuse or deepen existing owners before adding mechanisms; implement one coherent bounded slice with focused negative tests where practical.
4. Run the exact verification contract, repair root causes without weakening guards, and review correctness, authority, security, compatibility, audit, recovery, cost, and SQLite/PostgreSQL parity where applicable.
5. Update canonical owners, run handoff/diff checks, then follow the exact-head PR/CI/merge path. Continue only from refreshed accepted state.

## Reading and Verification

After `START_HERE.md`, follow only the returned role route and targeted reads. `docs/ARCHITECTURE.md` owns architecture and module ownership; `docs/AUTONOMY.md` autonomy and testing; `docs/ROADMAP.md` roadmap; `docs/RUNBOOK.md` operator procedures. Code, tests, Git/GitHub, and accepted canonical documents outrank stale prose. Use CodeGraph first for structural, dependency, impact, or call-flow exploration. If no `.codegraph/` index exists, **proactively run `codegraph index` at the repo root** before proceeding with code exploration. If CodeGraph is unavailable, damaged, locked, or its interface is missing after an index attempt, make at most one bounded local repair attempt (unlock or re-index); if that does not restore it, immediately fall back to `rg`, raw source, compiler, and tests (a CodeGraph failure is not `DECISION_REQUIRED`; temporary indices or repair artifacts must never be committed).

## Documentation Maintenance Rule

One fact has one full owner:
- `START_HERE.md` owns navigation/context routing;
- `docs/ARCHITECTURE.md` owns durable architecture, authority, security, recovery, and module ownership;
- `docs/AUTONOMY.md` owns autonomy, testing, review, and merge rules; its historical managed-run lifecycle is compatibility-only;
- `docs/ROADMAP.md` owns closed-loop milestones and research programs;
- `docs/RUNBOOK.md` owns proven operator runbooks.

Other entrypoints link instead of copying. Replace stale status rather than appending history; add no document when an existing owner fits.
