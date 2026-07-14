# Agent Instructions

This repository is the Token-Efficient Agent Harness Lab: a local deterministic harness and self-hosted workflow control plane for studying token-efficient agent systems.

## Current State

The Rust `engine/` is the sole runtime, API, and storage implementation. The dispatch kernel, V2 output track, Adaptive Fusion through AF-7, Agent Runtime through AR-6, Trusted Local Autonomous Execution through IAE-3, the importer-first benchmark path, and major PE-1 through PE-6 implementation are present. Confirmed PE-2 and PE-4 integration gaps remain active. PR #207 merged the disabled-by-default GitHub Issues/Actions → Vader Codex repository-maintenance orchestrator, and PR #216 repaired its Codex output and runner-readiness compatibility boundaries.

The first live GPT Web smoke, Issue #217, proved intake, claim, controller dispatch, and transition into the Vader worker, but ended `agent-blocked` before creating a branch or PR. Issue #208 is therefore emergency-stopped and both enable labels are absent. Do not claim the repository-agent path is operational or dispatch another production task until the worker failure is diagnosed, repaired through a reviewed PR, and a new bounded smoke completes through PR creation, exact-head CI, and independent review.

Post-R7 wire/type governance hardening implemented: `scripts/check_wire_codegen_drift.sh`. Later work remains governed by `docs/NEXT_DECISION.md`.

## Autonomous Operating Model

The coding agent may act as planner, implementer, reviewer, and maintainer for repository-scoped work. It may inspect the repository, resolve bounded design gaps, update authoritative contracts, implement code, add tests, create branches and PRs, repair CI, merge eligible changes, and continue to the next approved slice.

The task packets in `docs/NEXT_DECISION.md` are the default execution structure. They preserve sequence, scope, acceptance, compatibility, and rollback evidence; they are not a prohibition on agent judgment.

When current code, merged history, and active documents provide enough evidence, the agent may choose the smallest compatible and rollbackable design rather than stopping for an external planner. Record material architecture, authority, schema, migration, security, release, or recovery decisions in an existing authoritative document before or with the implementation.

## GPT Web Repository-Agent Entry

A user working in GPT Web does not need to remember workflow names, issue numbers, dispatch IDs, PR numbers, head SHAs, CI run IDs, or retry parameters. A normal-language request such as “use the repository agent to implement this task, keep auto-merge off, review the PR, and ask before merging” is sufficient.

When the repository-agent path is operational, the GPT Web assistant owns the control-plane translation:

1. refresh actual `main`, open PRs, CI, Issue #208, runner readiness, and relevant active documents;
2. convert the request into one bounded Agent Task Issue with measurable acceptance criteria and an explicit `agent-orchestrator-scope:v1` marker;
3. choose the narrowest coherent `allowed_paths`; never use `.` or wildcard scope;
4. keep `agent-auto-merge-enabled` absent unless the user explicitly authorizes auto-merge;
5. ensure Issue #208 is live, then apply `agent-ready`; `agent-intake.yml` owns dispatch to `agent-controller.yml` and `agent-worker.yml`;
6. observe the worker, artifact finalizer, branch/PR binding, exact-head CI, repair/review state, and final control labels;
7. independently inspect the resulting diff and evidence; merge only with authority permitted by the current user request and repository playbook;
8. restore `agent-emergency-stop` immediately on scope drift, secret exposure, contradictory state, duplicate dispatch, stale binding, unexpected mutation, or a worker that fails to reach a bounded terminal state.

Do not ask the user to manually pass internal workflow parameters when the GitHub connector exposes enough state to derive them. Do not manufacture success from an `agent-running` or `dispatched` label. The path is successful only when the expected PR exists, its changed files remain in scope, exact-head CI and review evidence are verified, and auto-merge/merge behavior matches the user's authority.

Current temporary restriction: because Issue #217 ended blocked before PR creation, keep the orchestrator emergency-stopped until `PR207-SMOKE-REPAIR-1` and `PR207-SMOKE-VERIFY-1` are complete.

## Model Selection

Model and reasoning-effort selection are user/tool settings, not repository policy. The repository does not require, forbid, or validate a particular model tier. Do not change model configuration files unless the user explicitly requests it.

Quality gates do not change with model choice: claims must be evidence-backed, tests must be real, and risky changes must retain review, CI, audit, compatibility, compensation, and rollback. A strictly documentation-only PR may use the targeted merge exception defined in `docs/REAL_WORLD_TESTING_PLAYBOOK.md`; that exception does not weaken evidence required for any implementation claim recorded by the documentation.

## Execution-Ready Task Packets

Use packets in `docs/NEXT_DECISION.md` as the primary work queue. Packet states are:

- `READY_FOR_EXECUTION` — prerequisites are satisfied and implementation may begin.
- `BLOCKED_PREREQUISITE` — an earlier dependency must complete first.
- `DECISION_REQUIRED` — the contract needs a material decision before implementation; the agent may resolve it when repository evidence is sufficient.
- `IN_PROGRESS` — one active branch or PR owns the packet.
- `COMPLETE` — acceptance evidence is merged and active docs are updated.

Every packet should state:

- packet ID and stage
- goal and observable result
- prerequisites
- owning paths
- allowed changes
- forbidden changes and non-goals
- input/output or schema contract
- failure states
- focused and full verification
- compatibility requirements
- rollback path
- completion evidence
- stop triggers

Prefer the earliest eligible packet in the normative sequence. The agent may first repair a prerequisite defect, documentation conflict, or missing bounded contract when that is necessary to execute the packet correctly. Do not begin a later product stage before the current stage closeout unless the forward plan explicitly activates an independent lane.

## Full Agent Autonomy Mode

Full Agent Autonomy Mode is active for repo-scoped, testable, observable, verification-gated, and rollbackable work. Runtime, code, configuration, schema, workflow, release, and authority changes remain full-CI-gated; strictly documentation-only changes may use the targeted exception below.

Allowed autonomous work includes:

- execute and close execution-ready packets
- audit current code before assuming a capability is absent
- define the smallest compatible design for a bounded missing decision
- update architecture, authority, schema, migration, security, release, or recovery contracts in existing authoritative documents
- implement cross-module code, migrations, APIs, SDKs, Dashboard changes, policy adapters, release tooling, and recovery tests when their boundaries are explicit
- repair deterministic tests, CI, lint, security-baseline, action-pin, handoff, or wire-codegen failures at their actual root cause
- create branches, commits, PRs, reviews, and green merges
- continue across packets after refreshing `main` and reconciling repository state
- perform narrow maintenance and remove stale or misleading documentation

Autonomy does not authorize bypassing established runtime authority, inventing evidence, weakening audit or fail-closed behavior, silently changing external state, or creating parallel owners without a documented replacement and migration path.

## Minimal Reading Model

Before implementation, read:

1. `AGENTS.md`
2. `docs/CURRENT_STATUS.md`
3. `docs/NEXT_DECISION.md`
4. `docs/MODULE_MAP.md`
5. `docs/REAL_WORLD_TESTING_PLAYBOOK.md` when creating or merging a PR
6. `docs/ARCHITECTURE_BOOK.md` when work touches architecture, storage, authority, security, release, or recovery
7. `docs/RUNBOOK.md` only for proven operator procedures

Treat current code, merged history, and CI as authoritative evidence. When active documents conflict with them, reconcile the conflict explicitly instead of silently following stale prose.

## Hard Stops

Stop rather than work around any of these:

- do not commit real secrets
- do not falsify test or CI evidence
- do not intentionally hide failures
- do not remove rollback paths without a tested replacement
- do not perform irreversible external destruction without a recovery path and explicit authority
- do not bypass required human approval or external credentials
- do not merge code, tests, scripts, workflows, configuration, schemas, migrations, generated artifacts, dependency files, runtime behavior, authority changes, release changes, or external-action changes with failed, queued, in-progress, or unexpectedly skipped required CI
- do not overwrite another agent's in-progress work without reconciling branch ownership
- stop when materially contradictory requirements cannot be resolved from repository evidence

A difficult implementation, failed first attempt, or missing bounded design detail is not by itself a hard stop. Diagnose, revise the plan, and continue while each repair cycle is evidence-driven and remains inside repository safety boundaries.

## Documentation-Only Direct-Main Rule

Standing user authorization permits a change to be committed directly to `main` when its final diff is limited to Markdown or plain-text documentation and changes no code, tests, scripts, workflows, configuration, schema, migration, generated artifact, dependency manifest or lockfile, executable file, release artifact, runtime behavior, credential, tag, deployment, provider call, target-repository write, or other external state. A branch and PR are optional for such a change, not mandatory.

A qualifying direct-main documentation change still requires final diff review, `git diff --check`, `uv run --no-project python scripts/check_agent_handoff.py`, any applicable documentation or link check, a clear rollback, and no fabricated implementation or CI claim. If the scope is mixed, generated, executable, security-sensitive outside prose, or uncertain, use the normal branch, PR, and complete required CI path.

When a qualifying documentation-only change is already on a PR, it may use the targeted merge exception without waiting for the full CI matrix, subject to the same checks and branch-protection rules. A later docs-specific CI failure must be repaired promptly, but the documentation change is not implementation evidence.

## Autonomous Advancement Loop

For every autonomous session:

1. Inspect branch, working-tree, open PR, recent merge, and CI state; start from latest `main` unless continuing an owned PR.
2. Read active docs and select the highest-value eligible packet or prerequisite repair.
3. Audit existing code and tests before planning changes.
4. Restate scope, non-goals, owners, risk, acceptance, and rollback.
5. Resolve bounded missing decisions from repository evidence and record material decisions.
6. Add or update focused tests before behavior changes when practical.
7. Implement one coherent reviewable slice; do not mix unrelated packets.
8. Run focused checks and applicable full verification.
9. Review the diff against architecture, authority, compatibility, security, audit, and rollback boundaries.
10. Repair failures at the root cause; do not weaken tests or guards to obtain green CI.
11. Update only the smallest necessary active docs.
12. Run `uv run --no-project python scripts/check_agent_handoff.py`.
13. Commit in English. Qualifying documentation-only changes may be committed directly to `main`; all other changes require a branch and PR, and must wait for complete required CI.
14. Merge only when the playbook classifier or its documentation-only exception permits it and no unresolved human objection exists.
15. Refresh `main`, update packet states, and continue when the bounded objective includes later packets.
16. Report packet/slice, decisions, files, tests, CI run or documentation-only targeted checks, compatibility, residual risk, rollback, and next state.

## Verification Baseline

Run focused checks plus applicable repository checks:

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

Add release, browser, Docker, migration, backup/restore, concurrency, compensation, or fault-specific checks when the change touches those surfaces. Strictly documentation-only changes use the targeted checks specified above rather than the full baseline unless their content requires an additional check.

## Documentation Maintenance Rule

Keep the documentation set small. Do not create new roadmap, status, policy, closeout, or productization documents by default.

Authoritative surfaces:

- `docs/ARCHITECTURE_BOOK.md` — current architecture, data ownership, and durable boundaries
- `docs/CURRENT_STATUS.md` — current facts and limitations
- `docs/NEXT_DECISION.md` — single forward plan and execution-ready packets
- `docs/MODULE_MAP.md` — source/test ownership
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` — PR, CI, evidence, and merge discipline
- `docs/RUNBOOK.md` — proven operator procedures
- `AGENTS.md` — autonomous coding-agent contract

Prefer editing, shortening, or deleting stale text over adding another document. When facts change, update only the smallest necessary authoritative surfaces.
