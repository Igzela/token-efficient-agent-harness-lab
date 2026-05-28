# Agent Instructions

This repository is the Token-Efficient Agent Harness Lab.

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
- Reliability Hardening 1 complete (negated risk and triage differentiation).
- Dispatch Kernel Phase 3 provider-adapter boundary stable and CA-7 compliant.
- Dispatch Kernel Phase 4 adaptive routing stable.
- Dispatch Kernel Phase 5 multi-agent orchestration stable (1454 tests, GPT approved).
- Dispatch Kernel Phase 6A local durable API/storage stable (1596 tests, GPT approved after 2 review rounds).
- Dispatch Kernel Phase 6B-1 per-server route isolation implemented (1603 tests).
- Dispatch Kernel Phase 6B-2 local API key + tenant boundary implemented and hardened (1654 tests).
- GitHub private repository published.

This project is now in autonomous maintainer mode for repository advancement. The responsible coding agent may keep advancing approved documentation, test, CI, hardening, and dispatch-kernel tracks without waiting for a new human instruction on every commit. The local dashboard remains a prototype. New UI, product, production, deployment, real-provider, or real-execution tracks still require explicit scope and human approval.

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

Allowed autonomous advancement:

- repair stale handoff docs and status drift
- fix failing tests, CI breakage, lint/security baseline failures, and deterministic regressions
- add focused tests for uncovered behavior in existing modules
- harden completed dispatch-kernel phases when evidence or review findings identify concrete defects
- advance the next documented dispatch-kernel phase when it is already described in the architecture book and can be implemented without real providers, real sandbox execution, target repo writes, deployment, or concurrent worker processes
- update architecture, module maps, and closeout reports required to make the new state durable

Not allowed under autonomous authority:

- create a new product surface, UI track, hosted service, or production runtime
- add real model-provider calls, secrets, SDK dependency wiring, network transports, process execution, containers, VMs, target-repo mutation, approval/run/deploy controls, or real autonomous workers
- bypass the architecture book, phase gates, tests, or documentation maintenance rule

## Autonomous Advancement Loop

For every autonomous session:

1. Read the bootstrap docs, inspect `git status --short --branch`, and identify whether another agent has uncommitted work.
2. Pick the highest-value safe task from failing verification, documented next phase work, concrete review findings, stale docs, or narrowly scoped hardening.
3. Write or update tests first when behavior changes.
4. Implement the smallest coherent change and run the relevant verification command.
5. Update `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`, `README.md`, `CLAUDE.md`, and this file when their facts changed.
6. Run `python3 scripts/check_agent_handoff.py` before commit.
7. Commit with an English message and push the active branch when the working tree contains only this session's intended changes.
8. Leave the next session a clear handoff: latest commit, verification run, remaining risk, and next recommended action.

If another agent has in-progress changes, do not overwrite them. Either build on them deliberately after reading the files, or leave them untouched and record the conflict in the handoff.

## Hard Boundaries

Do not modify:

- docs/stage0/events.jsonl

Do not add without explicit human approval:

- real model API calls
- API keys or provider credentials
- real autonomous agents
- real sandbox/process/container/VM execution
- real concurrent workers
- Web UI implementation
- provider failover
- production deployment
- destructive filesystem operations
- Stage 5 implementation

## Current Safe Work Categories

Allowed by default:

- documentation cleanup
- README / roadmap / module map updates
- test-only improvements
- CI maintenance
- GitHub issue planning
- security review planning
- packaging planning
- architecture audit updates
- approved dispatch-kernel phase work that respects all hard boundaries

Requires explicit approval:

- productionization
- real provider integration
- real sandbox execution
- UI/dashboard implementation
- benchmarking framework
- deployment work
- broad runtime refactors

## Documentation Maintenance Rule

After every commit-sized change, update the handoff docs before committing:

- `docs/CURRENT_STATUS.md` for current branch, latest stable commit, test count, sealed tracks, and known limitations
- `docs/NEXT_DECISION.md` when the allowed/disallowed next paths change
- `docs/MODULE_MAP.md` when source/test ownership changes
- `README.md` when quickstart commands, status, test counts, or repo structure change
- `CLAUDE.md` and this file when agent behavior, boundaries, or workflow rules change

If a change does not require documentation updates, state that explicitly in the commit or final report. Never leave a new session guessing which document is authoritative.

## Test Command

Run:

PYTHONPATH=src python3 -m unittest discover -s tests

## Repository Principles

- Preserve deterministic behavior.
- Prefer small, reviewable commits.
- Keep tests passing.
- Do not expand architecture without documenting the new track first.
- Treat future work as optional tracks, not automatic continuation of the completed Stage 0-4 task book.
