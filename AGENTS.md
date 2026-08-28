# Agent Instructions

Read `START_HERE.md` first. It is the canonical navigation entry and defines the role-specific route, source-of-truth hierarchy, frontier discovery, and handoff procedure. This file owns only stable implementation-agent permissions and stop boundaries; it does not own current status, packet scope, future routing, architecture detail, or PR/CI procedure.

This repository is a local, deterministic, auditable Agent harness and workflow control plane. Rust `engine/` is the sole runtime, API, scheduler, policy, and application-owned storage authority; `LocalProductStore` is the sole persistence owner.

## Quality and Frontier Rule

Use this order: correctness, safety, evidence, recovery, and rollback → architecture and authority integrity → maintainability and one canonical owner → low duplication and low context cost. Conciseness must preserve quality.

Before acting, establish the **leading valid frontier** from accepted remote `main`, the earliest eligible packet, its owned exact PR head when one exists, and blocked downstream work. Enter through the command returned by `START_HERE.md`; treat `scripts/project_context.py` and generated capsules as verified transport views, never authority. Do not continue from a stale branch, review, CI run, checkpoint, or downstream PR.

## Autonomous Operating Model

**Full Agent Autonomy Mode** permits repository-scoped work that is testable, observable, reviewable, verification-gated, and rollbackable. An agent may inspect, plan packet-internal execution, **resolve bounded design gaps**, implement, test, document, repair CI, create a focused PR, obtain independent review, and manually merge only when the canonical gates pass.

Normal reversible repository work and already-configured local/GitHub services are pre-authorized. Explicit confirmation remains required for irreversible destruction, production release/deploy, a new paid-provider POST, credential creation/rotation/disclosure, protected force-push, or unbounded external effects. Model and reasoning settings are user/tool choices: do not edit model configuration to satisfy repository instructions, and never reduce scope, test, review, audit, compatibility, recovery, or rollback requirements because of model choice.

## Authority and Ownership Boundaries

- Cross-packet direction, architecture/authority choices, packet goal, prerequisites, ordering, acceptance criteria, schema or durable-contract changes, security/recovery boundaries, evaluator semantics, release policy, and GO/NO-GO decisions belong to planning and their canonical owners.
- Packet-internal implementation may reuse existing owners, choose local design details, add focused tests, repair root causes, and synchronize the smallest canonical documents. It may not broaden scope, reorder packets, create a parallel owner, or silently change a durable boundary or strategic claim.
- Repository automation or any external/experimental harness or model may be only an admitted bounded worker or experimental subject. None may become a second runtime, scheduler, store, evaluator, budget, approval, workspace, output, audit, rollback, merge, release, or deployment owner.
- Product/provider, managed acceptance, experimental Harness Evolution, Level-2/Meta, credential, redaction, and recovery rules are owned by `docs/ARCHITECTURE_BOOK.md`; accepted capability and confirmed gaps by `docs/CURRENT_STATUS.md`; blocked successors by `docs/FUTURE_ROUTE.md`. Their current restrictions must not be bypassed or copied here as stale status.
- Never persist or report credentials, raw prompts/outputs/transcripts, private paths, or unredacted repository content. Fixture, fake, proxy, self-report, cache, or projection evidence is not managed acceptance.

If work needs to cross any boundary above, stop with `DECISION_REQUIRED`: report evidence, options, consequences, and the smallest proposed owner/packet update. A proposal is not accepted authority.

## Execution-Ready Task Packets

Packet lifecycle is owned solely by `docs/NEXT_DECISION.md`: `READY_FOR_EXECUTION`, `BLOCKED_PREREQUISITE`, `DECISION_REQUIRED`, `IN_PROGRESS`, and `COMPLETE`. Execute only the earliest eligible accepted packet and only its exact owners, allowed changes, ordered steps, verification, rollback, budgets, pause gates, and forbidden next actions. Review `PASS` satisfies one exact-head review gate; it is never packet `COMPLETE`.

Do not begin a later packet to fill time. During CI, tests, compilation, review, or audit, refresh evidence, inspect the bounded diff/contracts, prepare the next permitted check, or repair an in-scope prerequisite.

## Investigation Escalation (`ask_sol`)

Routine work proceeds directly. For genuinely difficult uncertainty, contradictory evidence, cross-module ambiguity, or failed initial hypotheses, use the bounded read-only investigator:

```bash
scripts/ask_sol "<investigation goal>" [--hypothesis "<hypothesis>"] [--task-id "<task_id>"]
```

It cannot mutate, approve, or grant authority; verify its result against first-party evidence. The detailed recursion, attempt, redaction, and state-integrity contract is owned by `docs/ARCHITECTURE_BOOK.md`.

## Delivery and Verification

The complete Draft/Ready, exact-head review, CI, documentation-only classification, merge, rebase, recovery, and rollback contract is owned by `docs/REAL_WORLD_TESTING_PLAYBOOK.md`. Invariant summary: one coherent branch/PR; keep changing work Draft; a new head invalidates prior review and CI; mark Ready only after local verification and complete-diff independent review; require canonical exact-head CI; merge manually only when eligible; refresh `main` afterward. Auto-merge remains disabled.

Run the accepted packet's focused and full verification contract and the applicable `docs/REAL_WORLD_TESTING_PLAYBOOK.md` **Verification Baseline**; a packet may add or classify checks but may not silently omit an applicable one. `scripts/check_wire_codegen_drift.sh`, security baseline, agent handoff, and `git diff --check` remain fail-closed where applicable. Never claim a command, result, review, cost, benchmark, effect, or acceptance that was not actually observed.

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

A difficult implementation or failed first attempt is not a blocker: diagnose and repair within scope. Stop on contradictory requirements, overlapping ownership, secret exposure, unprovable external-effect status, missing recovery evidence, or a required decision outside the accepted packet.

## Autonomous Advancement Loop

1. Enter through `START_HERE.md`; refresh accepted main, exact worktree/PR head, dependencies, reviews, CI, objections, and canonical owners.
2. Select and audit the earliest eligible packet; state scope, non-goals, authority, risk, acceptance, rollback, and hard stops.
3. Reuse or deepen existing owners before adding mechanisms; implement one coherent bounded slice with focused negative tests where practical.
4. Run the exact verification contract, repair root causes without weakening guards, and review correctness, authority, security, compatibility, audit, recovery, cost, and SQLite/PostgreSQL parity where applicable.
5. Update only canonical owners, run handoff/diff checks, then follow the playbook's exact-head PR/CI/manual-merge path. Continue only from refreshed accepted state.

## Reading and Verification

After `START_HERE.md`, follow only the returned role route and packet-targeted reads. `docs/MODULE_MAP.md` owns modules; `docs/ARCHITECTURE_BOOK.md` durable design and boundaries; `docs/REAL_WORLD_TESTING_PLAYBOOK.md` repository delivery; `docs/RUNBOOK.md` only proven operator procedures. Code, tests, Git/GitHub, and accepted canonical documents outrank stale prose. When `.codegraph/` exists, invoke `codegraph_explore` before broad grep/reads for structural, dependency, or call-flow navigation; read raw files afterward only for unestablished details. In linked worktrees without `.codegraph/`, agents are authorized to run `codegraph index` to establish branch-local code intelligence.

## Documentation Maintenance Rule

One fact has one full owner. `START_HERE.md` owns navigation/context routing; `docs/CURRENT_STATUS.md` accepted truth and confirmed gaps; `docs/NEXT_DECISION.md` one executable window; `docs/FUTURE_ROUTE.md` blocked routing only; `docs/MODULE_MAP.md` ownership; `docs/ARCHITECTURE_BOOK.md` durable architecture/authority/security/recovery; `docs/REAL_WORLD_TESTING_PLAYBOOK.md` PR/CI/review/merge; `docs/RUNBOOK.md` proven operations. Other entrypoints link instead of copying. Replace stale status rather than appending history; add no document when an existing owner fits.

One standing execution obligation: packet closeout/promotion must leave `docs/NEXT_DECISION.md` with exactly one active window plus at most one compact immediate-predecessor bridge, collapse accepted history into compact `docs/CURRENT_STATUS.md` receipts, and never re-accumulate chronology owned by Git/PR history. Recurrence fails CI through `scripts/check_agent_handoff.py`; no byte-size cap substitutes for this semantic rule.
