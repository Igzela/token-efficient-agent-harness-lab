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

Tests: 914 pass.

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
6. **[docs/demo/README.md](demo/README.md)** — Local demo walkthrough (optional).

## Default Behavior

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
