# Agent Instructions

This repository is the Token-Efficient Agent Harness Lab: a local deterministic harness and self-hosted macro-orchestrator control plane for studying token-efficient agent workflows.

## Current State (2026-06-25)

**Active tracks:**
- **Real-World Testing Mode** — validated through real tasks, branches, commits, PRs, CI, gated auto-merge
- **Agent Autonomous Maintenance Mode** — agents may advance implementation, docs, CI, tests, review, and routine merge flow under bounded safety gates
- **Full Agent Autonomy Mode** — agents may autonomously evolve repo-scoped architecture, authority, security, migration, release-workflow, and target-output designs when changes are testable, observable, CI-gated, and rollbackable
- **Trusted Local Autonomous Execution Track (IAE)** — complete through IAE-3 trusted-local profile, bounded task advancement, and operator control/evidence
- **Adaptive Fusion Routing Track** — complete through AF-7; IAE trusted-local profile is the recommended bounded local path, with legacy gates retained for compatibility

**Complete tracks:**
- Dispatch Kernel Phases 1–7 (including 6A, 6B-1/2/3, Gates 1–3): STABLE
- Language migration: COMPLETE (Rust engine is sole runtime)
- Dynamic Workflow Batches 1–7 + scheduler dynamic-mode: COMPLETE
- Macro-Orchestrator Phases 1–5 repair batch: COMPLETE
- Self-Hosted GA Readiness Track SG-1 through SG-5: COMPLETE
- HA Hardening Track HA-1 through HA-6: COMPLETE
- HybridExecutor with `ACP_EXECUTION_MODE`: COMPLETE
- V2 Real Production Output Track: COMPLETE (V2-0 through V2-5 merged in PRs #69-#75)
- Real Output Closeout: COMPLETE (`v0.1.0` published and online installer verified)

**Key facts:**
- Full Rust + TypeScript stack verification passes with 0 failures (recent green main CI evidence: run `28158603008`)
- Latest release: `v0.1.0`, published 2026-06-21
- Architecture Refactor R-series is baselined at R7; a documented, tested, rollbackable architecture decision may supersede it.
- Post-R7 wire/type governance hardening implemented: `scripts/check_wire_codegen_drift.sh`

## App Runtime vs Agent Maintenance Boundary

**App/runtime** protects registered target working trees and `main`. Target output remains approval-bound through an app-owned git worktree and `acp/*` branch, patch export, or optional GitHub PR creation. Installed local Claude/Codex CLIs are available for workflow execution. The IAE track authorizes a trusted-local profile that may automatically enable provider APIs, adaptive routing, experiments, promotion, and bounded workers when protected auth, endpoint credentials, budgets, audit, rollback, and kill controls are configured. This trusted-local path is the preferred internal execution mode; missing prerequisites must fail closed.

**Agent maintenance** may autonomously audit, plan, implement, test, review, simplify, document, create branches/commits/PRs, repair CI, merge, and iterate high-risk architecture work under `docs/REAL_WORLD_TESTING_PLAYBOOK.md`. Full Agent Autonomy Mode includes new architecture directions, authority-boundary changes, default execution/profile changes, auth/security redesign, database migrations, release/tag/deploy workflow changes, target-output authority changes, and superseding accepted decisions. Changes should remain repo-scoped, testable, observable, reviewable, and rollbackable. Documentation-only corrections may be committed directly to `main` when the working tree is clean and validation passes.

**Only hard stops:** committing real secrets; falsifying test/CI evidence; intentionally hiding failures; removing rollback paths; or performing irreversible external destruction without a recovery path.

## Minimal Agent Reading Model

**Default:** Read `AGENTS.md` only.

**Read conditionally:**
- `docs/NEXT_DECISION.md` — when choosing or validating next work
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` — when opening PRs, auto-merging, fixing CI, cleaning docs, or running real-world pilot tasks
- `docs/MODULE_MAP.md` — when changing code or deciding module ownership
- `docs/CURRENT_STATUS.md` — when status facts are unclear or when updating status
- `docs/ARCHITECTURE_BOOK.md` — when current architecture or boundary facts are needed

## New Session Bootstrap

Read this file first. Then conditionally read other docs based on your task type (see Minimal Agent Reading Model above).

If authoritative docs disagree with `README.md`, `CLAUDE.md`, or recent git history, repair the documentation before continuing feature work.

## Default Agent Behavior

Agents must not assume a new Stage 5 exists.

Before doing any work:

1. Inspect the current branch and working tree.
2. Run the test suite unless the task is documentation-only.
3. Confirm the requested task is inside the approved scope.
4. Do not ask for confirmation for repo-scoped Full Agent Autonomy Mode work; pause only at one of the five hard stops.

## Autonomous Advancement Authority

The responsible coding agent is expected to move this repository forward end to end. Full Agent Autonomy Mode authorizes repo-scoped, testable, observable, and rollbackable evolution, including high-risk architecture and authority changes recorded in `docs/NEXT_DECISION.md`.

IAE authorizes bounded local runtime advancement through existing workflow, scheduler, provider, adaptive-fusion, supervised-patch, and target-output modules. It does not authorize unbounded loops, secret persistence, direct target-repository `main` writes, or bypassing approval, budget, audit, redaction, rollback, and kill controls.

The trusted-local profile may make provider, adaptive routing, experiment, promotion, and supervised-worker execution automatically available after startup validation proves protected auth, configured symbolic credentials, bounded cost/token/call/time/concurrency limits, persistent audit, and live kill/rollback controls. This is the normal internal execution profile for approved local work. Missing or invalid prerequisites must leave execution unavailable.

The V2 Real Production Output Track approved on 2026-06-17 is that explicit batch for selected real-output capabilities. It must follow `docs/NEXT_DECISION.md`: V2-1 safety base, V2-2 provider/CLI output, V2-3 branch/worktree/PR flow, V2-4 bounded supervised workers, V2-5 product UX.

Allowed autonomous advancement:

- repair stale handoff docs, status drift, and wire-codegen guard drift
- fix failing tests, CI breakage, lint/security baseline failures, and deterministic regressions
- add focused tests for uncovered behavior in existing modules
- harden completed dispatch-kernel phases when evidence or review findings identify concrete defects
- advance IAE phases documented in `docs/NEXT_DECISION.md`, including trusted-local execution defaults, bounded autonomous task advancement, and operator evidence
- advance the next documented dispatch-kernel or IAE phase when it is described in the architecture book and extends existing modules without bypassing trusted-local safety controls
- advance the next V2 phase documented in `docs/NEXT_DECISION.md` when the change stays inside that phase's gates and includes audit, tests, and rollback/kill path
- design and implement repo-scoped auth/security, database migration, release-workflow, target-output, authority-boundary, and architecture changes through reviewed green PRs
- supersede accepted decisions when the replacement and rollback path are documented and verified
- update architecture, module maps, and closeout reports required to make the new state durable
- create branches, commits, PRs, and green merges through branch+PR workflow under `docs/REAL_WORLD_TESTING_PLAYBOOK.md` gates

Non-negotiable bottom lines:

- do not commit real secrets
- do not falsify test or CI evidence
- do not intentionally hide failures
- do not remove rollback paths
- do not perform irreversible external destruction without a recovery path

## Autonomous Advancement Loop

For every autonomous session:

1. Read this file, inspect `git status --short --branch`, and identify whether another agent has uncommitted work.
2. Conditionally read `docs/NEXT_DECISION.md` and `docs/REAL_WORLD_TESTING_PLAYBOOK.md` based on task type.
3. Pick the highest-value safe task from failing verification, documented next phase work, concrete review findings, stale docs, or narrowly scoped hardening.
4. Write or update tests first when behavior changes.
5. Implement the smallest coherent change and run the relevant verification command.
6. Update `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`, `README.md`, `CLAUDE.md`, and this file when their facts changed.
7. Run `uv run --no-project python scripts/check_agent_handoff.py` before commit (includes toolchain and wire-codegen drift guards).
8. Commit with an English message and push the active branch when the working tree contains only this session's intended changes.
9. Leave the next session a clear handoff: latest commit, verification run, remaining risk, and next recommended action.

If another agent has in-progress changes, do not overwrite them. Either build on them deliberately after reading the files, or leave them untouched and record the conflict in the handoff.

## Hard Boundaries

The production-grade hosted/self-hosted productization track (user-approved 2026-06-06) is authorized. It extends existing modules (node_executor, workflow_runs, supervised_patch, http_server) without creating parallel runtime kernels. See `docs/NEXT_DECISION.md` for phase details and constraints.

The V2 Real Production Output Track (user-approved 2026-06-17), Real Output Closeout (user-approved 2026-06-20), Adaptive Fusion Routing Track (user-approved 2026-06-21), Trusted Local Autonomous Execution Track (user-approved 2026-06-22), and Full Agent Autonomy Mode (user-approved 2026-06-23) authorize the work recorded in `docs/NEXT_DECISION.md`. Existing runtime controls remain authoritative until a replacement is explicitly implemented, tested, documented, and made rollbackable.

Current runtime controls remain authoritative until Full Agent Autonomy Mode explicitly replaces them through a documented, tested, observable, and rollbackable change. Do not assume an undefined Stage 5; define it before implementation.

## Current Safe Work Categories

Allowed by default:

- documentation cleanup
- README / forward-plan / module map updates
- test-only improvements
- CI maintenance
- GitHub issue planning
- security review planning
- packaging planning
- architecture audit updates
- approved dispatch-kernel phase work that respects all hard boundaries
- approved V2 phase work that follows `docs/NEXT_DECISION.md`
- approved Adaptive Fusion Routing phase work that follows `docs/NEXT_DECISION.md`
- approved IAE work that follows `docs/NEXT_DECISION.md`
- repo-scoped architecture, auth/security, migration, release-workflow, authority-boundary, target-output, and default-profile evolution with tests, evidence, review, and rollback
- supervised autonomous beta planning batches that remain non-executable and respect ADR-0002
- branch+PR workflow under `docs/REAL_WORLD_TESTING_PLAYBOOK.md` gates

Hard stops:

- committing real secrets
- falsifying test or CI evidence
- intentionally hiding failures
- removing rollback paths
- irreversible external destruction without a recovery path

## Documentation Maintenance Rule

Keep the documentation set small. Do not create new roadmap, next-steps, closeout, status, or productization documents unless the user explicitly asks for a new artifact.

Authoritative maintenance surfaces:

- `docs/ARCHITECTURE_BOOK.md` — current architecture, execution modes, data ownership, and safety boundaries
- `docs/CURRENT_STATUS.md` — current state, verification, test counts, stable tracks, and limitations
- `docs/NEXT_DECISION.md` — the single forward plan, including local productization phases and disallowed paths
- `docs/MODULE_MAP.md` — source/test ownership
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` and `docs/RUNBOOK.md` — PR/CI/maintenance workflow and operator procedures
- `README.md`, `CLAUDE.md`, and this file — quickstart, agent workflow, and hard boundaries

Prefer editing, shortening, or deleting stale documents over adding another file. When facts change, update only the smallest necessary set of authoritative surfaces. If no document update is needed, say why in the completion report.

## Test Command

Primary Rust + TypeScript cutover verification:

bash scripts/verify_rust_typescript_stack.sh

PostgreSQL integration tests (requires running PostgreSQL + `ACP_TEST_DATABASE_URL`):

cargo test -p engine --features pg-tests

Python SDK verification:

cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests

## Repository Principles

- Preserve deterministic behavior.
- Prefer small, reviewable commits.
- Keep tests passing.
- Document architecture changes and rollback before implementation.
- Treat future work as optional tracks, not automatic continuation of the completed Stage 0-4 task book.
