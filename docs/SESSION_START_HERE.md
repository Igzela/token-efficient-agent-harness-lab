# Session Start Here

Read this file first in any new AI session on this repository.

## Project Identity

Token-Efficient Agent Harness Lab is a local deterministic harness for studying event-sourced agent workflow infrastructure.

## Current State

| Milestone | Status |
|---|---|
| Stage 0–4 | Complete |
| CA-7 sealed baseline | Complete |
| Harness App MVP0–MVP8 | Complete |
| Trial 0 (real target) | Closed — `PASS` verdict |
| Trial 1 (multi-task) | Closed — `ACCEPTABLE_WITH_NOTES`, hardened to `ACCEPTABLE_FOR_MULTI_TASK_TRIAL_AFTER_HARDENING` |
| Reliability Hardening 1 | Complete |
| Demo packaging | Complete |
| Demo verification | Complete — all docs accurate and runnable |
| Trial 2 candidate selection | Planned — hermes-gateway-lab recommended |
| Trial 2 execution | Closed — `ACCEPTABLE_WITH_NOTES` (audit BLOCKED on target, generalization finding) |
| Trial 2 final verification | Closed — `TRIAL_2_FINAL_VERIFICATION_PASS` |
| Trial 3 multi-repo generalization | Closed — `TRIAL_3_MULTI_REPO_GENERALIZATION_PASS` |
| Trial 3 target merge | Closed — all 3 target PRs merged, audit PASS_WITH_NOTES |

Tests: 1603 pass.

Additional active architecture track:

| Track | Status |
|---|---|
| Dispatch Kernel Phase 1 | Stable |
| Dispatch Kernel Phase 2 | Stable |
| Dispatch Kernel Phase 3 — Provider Adapter Boundary | Stable, CA-7 compliant, no bundled real transport |
| Dispatch Kernel Phase 4 — Adaptive Routing | Stable |
| Dispatch Kernel Phase 5 — Multi-Agent Orchestration | Stable |
| Dispatch Kernel Phase 6A — Local Durable API/Storage | Stable |
| Dispatch Kernel Phase 6B+ | Eligible only when documented in the architecture book and kept inside repository-safe boundaries |

## What This Project Is Not

- **Not CA-8.** The CA-7 baseline is sealed. No CA-8 exists.
- **Not Stage 5.** No Stage 5 implementation has been started.
- **Not a production runtime.** No real model providers, sandboxes, workers, or deployment targets.
- **No real provider/model calls.** All advisor and model gateway components are stubs.
- **No real sandbox/process/container/VM execution.** Sandbox claims are logical file-claim tracking only.
- **No autonomous workers.** No real concurrent workers are spawned.
- **No target repo writes by default.** Target repositories are read-only. The app never writes to them.

## Must-Read Order

1. **[README.md](../README.md)** — Project identity, test command, safety boundaries, repo structure.
2. **[docs/CURRENT_STATUS.md](CURRENT_STATUS.md)** — Latest known state, completed tracks, current capabilities.
3. **[docs/NEXT_DECISION.md](NEXT_DECISION.md)** — What to do next and what is disallowed by default.
4. **[docs/MODULE_MAP.md](MODULE_MAP.md)** — Module-to-stage reference table.
5. **[docs/trials/TRIAL_1_REPORT.md](trials/TRIAL_1_REPORT.md)** — Latest trial results and hardening closeout.
6. **[docs/trials/TRIAL_2_FINAL_STATE_INDEX.md](trials/TRIAL_2_FINAL_STATE_INDEX.md)** — Trial 2 complete evidence chain and final state.
7. **[docs/demo/README.md](demo/README.md)** — Local demo walkthrough (optional).

## Default Behavior

The responsible coding agent may autonomously advance repository-safe work that keeps the project moving:

- repair stale docs and handoff drift
- fix failing tests, CI, security baseline, or deterministic regressions
- add focused tests for existing behavior
- harden completed phases when backed by concrete review findings
- implement documented dispatch-kernel phase work when the architecture book already defines the contract and the implementation does not add real providers, real sandbox/process execution, target repo writes, deployment, or real worker processes

Do **not** start any of the following without explicit human approval:

- MVP9
- Trial 2
- Stage 5
- Provider/model integration
- Sandbox/process/container/VM execution
- Autonomous workers
- Target repo writes
- Approval/run/execute/deploy/merge controls

Before proposing any new track, read `docs/CURRENT_STATUS.md` and `docs/NEXT_DECISION.md` first.

## Autonomous Session Closeout

A session is not complete until it leaves a durable handoff:

1. Relevant tests or verification commands were run and recorded.
2. `python3 scripts/check_agent_handoff.py` passes.
3. Handoff docs reflect the current branch, status, test count, stable commits, limitations, and next action.
4. The commit message is in English and the active branch is pushed when the tree contains only this session's intended changes.
5. The final report states latest commit, verification, remaining risks, and the next safe action.

## Documentation Maintenance

After any commit-sized change, update the handoff docs before committing:

- `docs/CURRENT_STATUS.md` for current state, test count, stable commit, limitations, and verification
- `docs/NEXT_DECISION.md` for allowed/disallowed next paths
- `docs/MODULE_MAP.md` for module ownership changes
- `README.md`, `CLAUDE.md`, and `AGENTS.md` for agent-facing workflow or boundary changes

If no docs changed, state the reason in the completion report.
