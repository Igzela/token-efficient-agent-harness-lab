# Agent Instructions

This repository is the Token-Efficient Agent Harness Lab: a local deterministic harness and self-hosted macro-orchestrator control plane for studying token-efficient agent workflows.

## Current State (2026-06-18)

**Active tracks:**
- **Real-World Testing Mode** — validated through real tasks, branches, commits, PRs, CI, gated auto-merge
- **Agent Autonomous Maintenance Mode** — agents maintain docs/CI/tests/low-risk PR flow under playbook gates
- **V2 Real Production Output Track** — authorized, phase-gated path to auditable real-repository patch/PR production
- **V2 progress** — V2-0 through V2-3 merged; V2-4 PR #73 opened; V2-5 product output UX implemented on its stacked phase branch

**Complete tracks:**
- Dispatch Kernel Phases 1–7 (including 6A, 6B-1/2/3, Gates 1–3): STABLE
- Language migration: COMPLETE (Rust engine is sole runtime)
- Dynamic Workflow Batches 1–7 + scheduler dynamic-mode: COMPLETE
- Macro-Orchestrator Phases 1–5 repair batch: COMPLETE
- Self-Hosted GA Readiness Track SG-1 through SG-5: COMPLETE
- HA Hardening Track HA-1 through HA-6: COMPLETE
- HybridExecutor with `ACP_EXECUTION_MODE`: COMPLETE

**Key facts:**
- 1571 Rust tests pass, 0 failures (last recorded full verification)
- Architecture Refactor R-series sealed at R7. R8 is not approved.
- Post-R7 wire/type governance hardening implemented: `scripts/check_wire_codegen_drift.sh`

## App Runtime vs Agent Maintenance Boundary

**App/runtime** does not write target repos by default. V2-3 adds only env-gated, approval-bound output through an app-owned git worktree and `acp/*` branch or patch export; registered target working trees and `main` remain protected.

**Agent maintenance** may create branches, commits, PRs, and low-risk merges only through branch+PR workflow under `docs/REAL_WORLD_TESTING_PLAYBOOK.md` gates. This is a repository workflow mode, not an app-runtime feature.

**Requires explicit human approval:** Provider/CLI execution boundary expansion outside the V2 phase plan, auth/security boundary changes, DB migrations, release/tag/deploy, active YAML/rubric/policy mutation, destructive operations.

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
4. Ask for confirmation before starting any new product track.

## Autonomous Advancement Authority

The responsible coding agent is expected to move this repository forward end to end when work is inside the safe scope below. This authority applies to the external coding agent maintaining the repo; it does not authorize implementing runtime autonomous workers inside the harness.

Planning-only supervised beta work may classify modules, design schemas, and create non-executable app-owned planning records. It does not authorize runtime autonomous workers, target repository writes, sandbox/process/container/VM execution, deploy/merge controls, or default-on provider calls.

Batch 6 supervised-execution contracts and Batch 7 Slice A-E metadata/read-only/design/dashboard visibility did not by themselves authorize runtime execution. Slice F and the production-grade track authorize supervised execution primitives in app-owned detached workspaces. Implementing sandbox/process/container/VM behavior, target repository writes, provider default-on execution, hosted deployment, push/merge/deploy/apply controls, or unattended autonomous workers still requires a separate explicit human-approved batch.

The V2 Real Production Output Track approved on 2026-06-17 is that explicit batch for selected real-output capabilities. It must follow `docs/NEXT_DECISION.md`: V2-1 safety base, V2-2 provider/CLI output, V2-3 branch/worktree/PR flow, V2-4 bounded supervised workers, V2-5 product UX.

Allowed autonomous advancement:

- repair stale handoff docs, status drift, and wire-codegen guard drift
- fix failing tests, CI breakage, lint/security baseline failures, and deterministic regressions
- add focused tests for uncovered behavior in existing modules
- harden completed dispatch-kernel phases when evidence or review findings identify concrete defects
- advance the next documented dispatch-kernel phase when it is already described in the architecture book and can be implemented without broadening real provider behavior beyond approved env-gated paths, sandbox isolation, subprocess execution beyond the existing CLI executor path, target repo writes, deployment, or worker concurrency outside V2-4
- advance the next V2 phase documented in `docs/NEXT_DECISION.md` when the change stays inside that phase's gates and includes audit, tests, and rollback/kill path
- update architecture, module maps, and closeout reports required to make the new state durable
- create branches, commits, PRs, and low-risk merges through branch+PR workflow under `docs/REAL_WORLD_TESTING_PLAYBOOK.md` gates

Not allowed under autonomous authority:

- create a new cloud product surface, hosted service, or production runtime
- broaden real model-provider calls beyond approved V2/provider gates, add secrets, add default-on provider execution, expand subprocess execution beyond approved V2/CLI gates, add containers, VMs, direct target-repo `main` mutation, release/deploy controls, or real autonomous workers
- bypass the architecture book, phase gates, tests, or documentation maintenance rule
- provider/CLI execution boundary expansion, auth/security boundary changes, DB migrations, release/tag/deploy, active YAML/rubric/policy mutation, destructive operations (all require explicit human approval)

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

The V2 Real Production Output Track (user-approved 2026-06-17) is authorized only through the phase plan in `docs/NEXT_DECISION.md`. It upgrades selected old limits into guarded capabilities; it does not authorize default-on execution, direct `main` writes, deploy/release controls, cloud SaaS, or unattended autonomous-agent loops.

Do not modify:

- docs/stage0/events.jsonl

Do not add without explicit human approval:

- default-on or unattended real model API calls
- API keys or provider credentials
- real autonomous agents
- new or broadened sandbox/process/container/VM execution beyond the existing local CLI executor path
- concurrent workers outside the bounded, env-gated V2-4 scheduler model
- new cloud/hosted Web UI implementation
- provider failover
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
- supervised autonomous beta planning batches that remain non-executable and respect ADR-0002
- branch+PR workflow for docs/tests/CI/small code fixes under `docs/REAL_WORLD_TESTING_PLAYBOOK.md` gates

Requires explicit approval:

- cloud productionization
- real provider productionization beyond the existing explicit env-gated local beta path
- real sandbox execution
- new cloud/hosted UI/dashboard implementation
- benchmarking framework
- cloud or hosted deployment work
- broad runtime refactors
- provider/CLI execution boundary expansion
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
