# Agent Instructions

This repository is the Token-Efficient Agent Harness Lab: a local deterministic harness and self-hosted macro-orchestrator control plane for studying token-efficient agent workflows.

## Current State (2026-06-22)

**Active tracks:**
- **Real-World Testing Mode** — validated through real tasks, branches, commits, PRs, CI, gated auto-merge
- **Agent Autonomous Maintenance Mode** — agents may advance implementation, docs, CI, tests, review, and low-risk merge flow under bounded safety gates
- **Trusted Local Autonomous Execution Track (IAE)** — IAE-1 trusted-local profile and IAE-2 bounded task advancement implemented; IAE-3 is next
- **Adaptive Fusion Routing Track** — complete through AF-7; legacy gates and the IAE-1 profile are supported

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
- Full Rust + TypeScript stack verification passes with 0 failures (2026-06-22)
- Latest release: `v0.1.0`, published 2026-06-21
- Architecture Refactor R-series sealed at R7. R8 is not approved.
- Post-R7 wire/type governance hardening implemented: `scripts/check_wire_codegen_drift.sh`

## App Runtime vs Agent Maintenance Boundary

**App/runtime** protects registered target working trees and `main`. Target output remains approval-bound through an app-owned git worktree and `acp/*` branch, patch export, or optional GitHub PR creation. Installed local Claude/Codex CLIs are available for workflow execution. The IAE track authorizes a trusted-local profile that may automatically enable provider APIs, adaptive routing, experiments, promotion, and bounded workers when protected auth, endpoint credentials, budgets, audit, rollback, and kill controls are configured. Missing prerequisites must fail closed.

**Agent maintenance** may autonomously audit, plan, implement, test, review, simplify, document, create branches/commits/PRs, repair CI, and merge low-risk green changes under `docs/REAL_WORLD_TESTING_PLAYBOOK.md`. Documentation-only corrections may be committed directly to `main` when the working tree is clean and validation passes.

**Requires explicit human approval:** credentials or paid-resource decisions, destructive or irreversible operations, DB migrations, production release/tag/deploy, cloud production, auth/security redesign, target-output authority expansion, or materially different user-visible product behavior outside `docs/NEXT_DECISION.md`.

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
4. Ask for confirmation only before starting a product track not already authorized in `docs/NEXT_DECISION.md`.

## Autonomous Advancement Authority

The responsible coding agent is expected to move this repository forward end to end when work is inside the authorized scope below. This includes repository maintenance and implementation of the bounded trusted-local execution track recorded in `docs/NEXT_DECISION.md`.

IAE authorizes bounded local runtime advancement through existing workflow, scheduler, provider, adaptive-fusion, supervised-patch, and target-output modules. It does not authorize unbounded loops, secret persistence, direct target-repository `main` writes, or bypassing approval, budget, audit, redaction, rollback, and kill controls.

The trusted-local profile may make provider, adaptive routing, experiment, promotion, and supervised-worker execution automatically available after startup validation proves protected auth, configured symbolic credentials, bounded cost/token/call/time/concurrency limits, persistent audit, and live kill/rollback controls. Missing or invalid prerequisites must leave execution unavailable.

The V2 Real Production Output Track approved on 2026-06-17 is that explicit batch for selected real-output capabilities. It must follow `docs/NEXT_DECISION.md`: V2-1 safety base, V2-2 provider/CLI output, V2-3 branch/worktree/PR flow, V2-4 bounded supervised workers, V2-5 product UX.

Allowed autonomous advancement:

- repair stale handoff docs, status drift, and wire-codegen guard drift
- fix failing tests, CI breakage, lint/security baseline failures, and deterministic regressions
- add focused tests for uncovered behavior in existing modules
- harden completed dispatch-kernel phases when evidence or review findings identify concrete defects
- advance IAE phases documented in `docs/NEXT_DECISION.md`, including trusted-local execution defaults, bounded autonomous task advancement, and operator evidence
- advance the next documented dispatch-kernel or IAE phase when it is described in the architecture book and extends existing modules without bypassing trusted-local safety controls
- advance the next V2 phase documented in `docs/NEXT_DECISION.md` when the change stays inside that phase's gates and includes audit, tests, and rollback/kill path
- update architecture, module maps, and closeout reports required to make the new state durable
- create branches, commits, PRs, and low-risk merges through branch+PR workflow under `docs/REAL_WORLD_TESTING_PLAYBOOK.md` gates

Not allowed under autonomous authority:

- create a new cloud product surface, hosted service, or production runtime
- add secrets, unbounded provider spending, unbounded autonomous loops, direct target-repo `main` mutation, release/deploy controls, or unaudited execution
- bypass auth, budget, token, call, timeout, concurrency, provider/model identity, redaction, audit, snapshot, rollback, approval, or kill controls
- bypass the architecture book, phase gates, tests, or documentation maintenance rule
- auth/security redesign, DB migrations, release/tag/deploy, destructive operations, and target-output authority expansion require explicit human approval

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

The V2 Real Production Output Track (user-approved 2026-06-17), Real Output Closeout (user-approved 2026-06-20), Adaptive Fusion Routing Track (user-approved 2026-06-21), and Trusted Local Autonomous Execution Track (user-approved 2026-06-22) authorize the guarded phases recorded in `docs/NEXT_DECISION.md`. IAE may change local execution defaults, but it does not authorize direct target-repository `main` writes, cloud SaaS, app-runtime release/deploy controls, or unbounded autonomous loops.

Do not modify:

- docs/stage0/events.jsonl

Do not add without explicit human approval:

- API keys or provider credentials
- unbounded or unaudited real model API calls
- autonomous loops without task, time, cost, concurrency, pause, and kill ceilings
- container/VM execution or host-level privilege expansion
- new cloud/hosted Web UI implementation
- cloud or hosted production deployment
- destructive filesystem operations
- Stage 5 implementation

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
- supervised autonomous beta planning batches that remain non-executable and respect ADR-0002
- branch+PR workflow for docs/tests/CI/small code fixes under `docs/REAL_WORLD_TESTING_PLAYBOOK.md` gates

Requires explicit approval:

- cloud productionization
- credentials or paid-resource decisions
- container/VM or host-privilege execution
- new cloud/hosted UI/dashboard implementation
- benchmarking framework
- cloud or hosted deployment work
- auth/security boundary changes
- DB migrations
- release/tag/deploy
- active YAML/rubric/policy mutation
- destructive operations

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
- Do not expand architecture without documenting the new track first.
- Treat future work as optional tracks, not automatic continuation of the completed Stage 0-4 task book.
