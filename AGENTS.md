# Agent Instructions

This repository is the Token-Efficient Agent Harness Lab: a local deterministic harness and self-hosted macro-orchestrator control plane for studying token-efficient agent workflows.

## Current Status

The Stage 0-4 task-book scope is complete.

- Stage 0: schema validation and manual workflow simulation complete.
- Stage 1: deterministic local runtime complete.
- Stage 2: quality runtime complete.
- Stage 3: controlled intelligence stubs complete.
- Stage 4: advanced runtime abstractions complete.
- Project closeout complete.
- CA-7 sealed baseline complete.
- Harness App MVP0-MVP8 complete (local operations console).
- Trial 0 closed — real target `PASS` verdict.
- Trial 1 closed — `ACCEPTABLE_FOR_MULTI_TASK_TRIAL_AFTER_HARDENING`.
- Trial 2 final verification closed — `TRIAL_2_FINAL_VERIFICATION_PASS`.
- Trial 3 multi-repo generalization and target merge closed.
- Trial 4 real-use pilot closed — `TRIAL_4_REAL_USE_PILOT_PASS_AFTER_FIXES`.
- Trial 5 CLI execution beta closed — `TRIAL_5_CLI_EXECUTION_BETA_PASS_AFTER_FIXES`.
- Reliability Hardening 1 complete (negated risk and triage differentiation).
- Dispatch Kernel Phase 3 provider-adapter boundary stable and CA-7 compliant.
- Dispatch Kernel Phase 4 adaptive routing stable.
- Dispatch Kernel Phase 5 multi-agent orchestration stable (1454 tests, GPT approved).
- Dispatch Kernel Phase 6A local durable API/storage stable (1596 tests, GPT approved after 2 review rounds).
- Dispatch Kernel Phase 6B-1 per-server route isolation implemented (1603 tests).
- Dispatch Kernel Phase 6B-2 local API key + tenant boundary implemented and hardened (1654 tests).
- Dispatch Kernel Phase 7 SDK + Documentation System implemented (sdk.py, doc_generator.py).
- Language migration preparation approved: Rust core + axum API target, TypeScript dashboard/SDK target, Python SDK retained. Rust engine parity now covers wire schemas, golden fixtures, dispatch, routing/orchestration, infrastructure, storage, SDK/migrator helpers, doc generation, and a local axum API router. Phase 5 codegen plus TypeScript/Python REST SDK packages are implemented. Phase 6 dashboard is implemented with static export support. Phase 7 local Docker deploy is implemented as an optional verification path. Native local runtime is implemented so one Rust process can serve API + static dashboard with `ACP_DASHBOARD_DIR=dashboard/out`; Phase 8 closeout is recorded in `docs/AGENT_CONTROL_PLANE_MIGRATION_CLOSEOUT.md`. Rust + TypeScript cutover is complete: Rust `engine/` is the primary runtime/API/storage/provider-gated control plane, `dashboard/` and `sdk/typescript/` are the primary TypeScript surfaces. Python legacy reference retired; Python retained as REST SDK and utility scripts only.
- Local small-team productization is implemented: app-owned SQLite dispatch history/config/team/API-key metadata/audit/cost/plan/workflow-run state, live dashboard API state, optional local API key role boundary, export, confirmed local backup, operations metrics, backup verify/restore dry-run, audit redaction, provider pricing visibility, local ops/restore smoke scripts, and SDK methods. Broader provider execution beyond the explicit env-gated local beta path, target writes, sandbox/process/container/VM isolation, runtime workers, cloud SaaS, and hosted production deployment remain disallowed.
- GitHub private repository published.
- Architecture Refactor R-series sealed at R7. R8 is not approved. The `checkpoint.rs` split and `dispatch_decision.rs` split are deferred. No further R-series file splitting is approved.
- Post-R7 wire/type governance hardening implemented: dormant `app_layer` annotation, Rust golden fixture typed round-trip guardrail, active execution-result schema enums, generated/manual TypeScript split, schema-driven enum codegen with drift enforcement via `scripts/check_wire_codegen_drift.sh`, and localized dashboard union reuse.
- Supervised autonomous beta planning started as a planning-only track in `docs/adr/0002-supervised-planning-track.md`. Batch 0-6 governance/module/model/read-only-planner/durable-state/advisory/design-gate work is recorded, with `WorkflowGraph` selected as canonical planning model. Batch 7 Slice A-F and the production-grade track now add supervised execution runtime primitives in app-owned detached workspaces: `NodeExecutor`, explicit tick/scheduler paths, workspace lifecycle, patch capture, integrity validation, approval binding, and export gating. These are supervised local runtime primitives, not target-repo writes, sandbox/process/container/VM isolation, provider default-on execution, hosted deployment, or unattended autonomous workers.
- Current Rust test count: 1367 pass.
- Dynamic Workflow: ALL 7 BATCHES COMPLETE plus scheduler dynamic-mode recovery. Minimum acceptance target achieved.
- Macro-Orchestrator Direction: current product direction. Phase 1-5 repair batch COMPLETE. Self-Hosted GA Readiness Track SG-1 through SG-5 COMPLETE: real dynamic CLI pilot matrix, long-run soak/failure injection, mission-control dashboard visibility, enriched policy decision signals, and runbook/release/rollback handoff readiness. Track done.
- HA Hardening Track: started. HA-4 circuit breaker DONE (14 tests). Remaining: HA-1 Scheduler Resilience + Persistent Heartbeat, HA-2 Automated Backup, HA-3 Deep Health + Resource Monitoring + External Monitoring, HA-5 TLS, HA-6 Secret Encryption. User requested: PostgreSQL optional storage backend, persistent heartbeat, external monitoring.
- Existing CLI executor routing is a pre-existing local subprocess exception and is explicit opt-in via `ACP_ENABLE_CLI_EXECUTION=1`. Any expansion requires explicit scope and human approval.

This project is now in autonomous maintainer mode for repository advancement. The responsible coding agent may keep advancing approved documentation, test, CI, hardening, dispatch-kernel, and local small-team self-hosting tracks without waiting for a new human instruction on every commit. The Self-Hosted GA Readiness Track is the active approved local/small-team self-hosting track and must deepen existing runtime modules only. New cloud/SaaS, hosted production, real-provider, target-write, new sandbox/container/VM, or unattended autonomous-worker tracks still require explicit scope and human approval.

## New Session Bootstrap

Every Codex, Claude Code, or other coding-agent session must start by reading:

1. `docs/SESSION_START_HERE.md`
2. `docs/CURRENT_STATUS.md`
3. `docs/NEXT_DECISION.md`
4. `docs/MODULE_MAP.md`

Treat those files as the handoff surface. If they disagree with `README.md`, `CLAUDE.md`, or recent git history, repair the documentation before continuing feature work.

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

Allowed autonomous advancement:

- repair stale handoff docs, status drift, and wire-codegen guard drift
- fix failing tests, CI breakage, lint/security baseline failures, and deterministic regressions
- add focused tests for uncovered behavior in existing modules
- harden completed dispatch-kernel phases when evidence or review findings identify concrete defects
- advance the next documented dispatch-kernel phase when it is already described in the architecture book and can be implemented without broadening real provider behavior beyond the existing explicit env-gated local beta path, sandbox isolation, subprocess execution beyond the existing CLI executor path, target repo writes, deployment, or concurrent worker processes
- update architecture, module maps, and closeout reports required to make the new state durable

Not allowed under autonomous authority:

- create a new cloud product surface, hosted service, or production runtime
- broaden real model-provider calls beyond the existing explicit env-gated local beta path, add secrets, add default-on provider execution, expand subprocess execution beyond the existing CLI executor path, add containers, VMs, target-repo mutation, approval/run/deploy controls, or real autonomous workers
- bypass the architecture book, phase gates, tests, or documentation maintenance rule

## Autonomous Advancement Loop

For every autonomous session:

1. Read the bootstrap docs, inspect `git status --short --branch`, and identify whether another agent has uncommitted work.
2. Pick the highest-value safe task from failing verification, documented next phase work, concrete review findings, stale docs, or narrowly scoped hardening.
3. Write or update tests first when behavior changes.
4. Implement the smallest coherent change and run the relevant verification command.
5. Update `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`, `README.md`, `CLAUDE.md`, and this file when their facts changed.
6. Run `uv run --no-project python scripts/check_agent_handoff.py` before commit (includes toolchain and wire-codegen drift guards).
7. Commit with an English message and push the active branch when the working tree contains only this session's intended changes.
8. Leave the next session a clear handoff: latest commit, verification run, remaining risk, and next recommended action.

If another agent has in-progress changes, do not overwrite them. Either build on them deliberately after reading the files, or leave them untouched and record the conflict in the handoff.

## Hard Boundaries

The production-grade hosted/self-hosted productization track (user-approved 2026-06-06) is authorized. It extends existing modules (node_executor, workflow_runs, supervised_patch, http_server) without creating parallel runtime kernels. See `docs/NEXT_DECISION.md` for phase details and constraints.

Do not modify:

- docs/stage0/events.jsonl

Do not add without explicit human approval:

- default-on or unattended real model API calls
- API keys or provider credentials
- real autonomous agents
- new or broadened sandbox/process/container/VM execution beyond the existing local CLI executor path
- real concurrent workers
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
- supervised autonomous beta planning batches that remain non-executable and respect ADR-0002

Requires explicit approval:

- cloud productionization
- real provider productionization beyond the existing explicit env-gated local beta path
- real sandbox execution
- new cloud/hosted UI/dashboard implementation
- benchmarking framework
- cloud or hosted deployment work
- broad runtime refactors

## Documentation Maintenance Rule

Keep the documentation set small. Do not create new roadmap, next-steps, closeout, status, or productization documents unless the user explicitly asks for a new artifact.

Authoritative maintenance surfaces:

- `docs/CURRENT_STATUS.md` — current state, verification, test counts, stable tracks, and limitations
- `docs/NEXT_DECISION.md` — the single forward plan, including local productization phases and disallowed paths
- `docs/MODULE_MAP.md` — source/test ownership
- `README.md`, `CLAUDE.md`, and this file — quickstart, agent workflow, and hard boundaries

Prefer editing, shortening, or deleting stale documents over adding another file. When facts change, update only the smallest necessary set of authoritative surfaces. If no document update is needed, say why in the completion report.

## Test Command

Primary Rust + TypeScript cutover verification:

bash scripts/verify_rust_typescript_stack.sh

Python SDK verification:

cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests

## Repository Principles

- Preserve deterministic behavior.
- Prefer small, reviewable commits.
- Keep tests passing.
- Do not expand architecture without documenting the new track first.
- Treat future work as optional tracks, not automatic continuation of the completed Stage 0-4 task book.
