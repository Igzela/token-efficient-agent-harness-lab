# Agent Instructions

This repository is the Token-Efficient Agent Harness Lab: a local deterministic harness and self-hosted workflow control plane for studying token-efficient agent systems.

## Current State

The Rust `engine/` is the sole runtime, API, and storage implementation. The dispatch kernel, V2 output track, Adaptive Fusion through AF-7, Agent Runtime through AR-6, Trusted Local Autonomous Execution through IAE-3, and the importer-first benchmark path are complete. The active forward plan is PE-1 through PE-6 in `docs/NEXT_DECISION.md`.

Post-R7 wire/type governance hardening implemented: `scripts/check_wire_codegen_drift.sh`.

## Planner–Executor Operating Model

Repository implementation is split into two roles:

- **External planner:** the user and the planning ChatGPT session define architecture, authority, contracts, task packets, acceptance criteria, and recovery invariants.
- **Codex executor:** Codex implements one approved task packet, tests it, opens a PR, repairs ordinary CI failures, and reports evidence.

The executor must not silently absorb planner responsibilities. When a required decision is absent or contradicted by current code, stop with an evidence-backed blocker instead of inventing a replacement design.

## Mandatory Codex Execution Profile

All Codex implementation, review, test repair, and PR work for this repository must use:

- model: `gpt-5.6-terra`
- reasoning effort: `medium`
- review model: `gpt-5.6-terra`
- plan-mode reasoning effort: `medium`

The project default is stored in `.codex/config.toml`.

Do not switch to Sol, Luna, another premium model, `high`, `xhigh`, `max`, or `ultra`. Do not launch a Sol subagent or use Sol as a reviewer. A user may explicitly override a local or cloud session outside repository control, but an executor operating under this contract must stop and report `model_profile_mismatch` if it is not running Terra Medium.

Model escalation is not a recovery strategy. After two coherent implementation/repair cycles on the same root cause, stop and report the blocker, evidence, attempted repairs, and the missing decision.

## Terra-Ready Task Packets

Codex may implement only a packet in `docs/NEXT_DECISION.md` marked `READY_FOR_TERRA`.

Every packet must state:

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

Stage prose alone is not implementation authority. If no packet is ready, perform only bounded maintenance or report `no_terra_ready_packet`; do not create the next product design yourself.

## Full Agent Autonomy Mode

Full Agent Autonomy Mode remains active **inside an approved Terra-ready task packet**. Codex may autonomously inspect, implement, test, review, create a branch/commit/PR, repair CI, update active docs, and merge when the packet and playbook gates are satisfied.

The following remain planner-owned unless an approved packet supplies the complete decision:

- new architecture directions
- authority-boundary changes
- default execution/profile changes
- auth/security redesign
- database migrations and compatibility policy
- release/tag/deploy workflow changes
- target-output authority changes
- superseding accepted decisions
- automatic pause, termination, promotion, signing, or recovery semantics

Codex may implement such work with Terra Medium after the planner has fixed the contract, boundaries, tests, rollback, and stop conditions in a task packet. It must not redesign them during implementation.

## Minimal Reading Model

Before implementation, read:

1. `AGENTS.md`
2. `docs/CURRENT_STATUS.md`
3. `docs/NEXT_DECISION.md`
4. `docs/MODULE_MAP.md`
5. `docs/REAL_WORLD_TESTING_PLAYBOOK.md` when creating or merging a PR
6. `docs/ARCHITECTURE_BOOK.md` when a packet touches architecture, storage, authority, security, release, or recovery
7. `docs/RUNBOOK.md` only for proven operator procedures

If authoritative documents disagree with code or recent merged history, stop and report the exact conflict. Do not resolve architectural disagreement by assumption.

## Autonomous Advancement Authority

Allowed autonomous work:

- execute the earliest `READY_FOR_TERRA` packet from latest `main`
- repair deterministic test, CI, lint, security-baseline, action-pin, handoff, or wire-codegen failures caused by the packet
- add focused tests needed by the packet
- update the smallest authoritative active docs with implemented facts
- create a branch, commit, PR, and merge after all required checks are green
- perform narrow maintenance that does not create a new product decision

Not allowed without a ready packet:

- selecting a different stage or reordering PE stages
- broad refactors or parallel runtimes, schedulers, stores, policy authorities, mailboxes, or Dashboard state models
- changing schemas, permissions, live authority, promotion, pause, signing, or recovery behavior
- weakening tests, guards, budgets, audit, redaction, rollback, or fail-closed behavior
- provider calls in CI or raw sensitive evidence persistence

## Hard Stops

Stop immediately rather than work around any of these:

- do not commit real secrets
- do not falsify test or CI evidence
- do not intentionally hide failures
- do not remove rollback paths
- do not perform irreversible external destruction without a recovery path
- task packet conflicts with current code or another authoritative document
- implementation requires an unspecified authority, schema, migration, security, release, or recovery decision
- current execution profile is not Terra Medium
- the same root cause remains unresolved after two coherent repair cycles

## Autonomous Advancement Loop

For every autonomous session:

1. Confirm the current session uses Terra Medium.
2. Inspect branch and working-tree state and start from latest `main`.
3. Read the active docs and select the earliest `READY_FOR_TERRA` packet.
4. Audit existing code before assuming a capability is absent.
5. Restate packet scope, non-goals, stop triggers, and owning paths.
6. Add or update focused tests before behavior changes when practical.
7. Implement the smallest coherent packet slice without changing the packet contract.
8. Run focused checks and applicable full verification.
9. Review the diff against the packet, module ownership, security boundary, compatibility, and rollback.
10. Repair ordinary failures for at most two coherent cycles; otherwise report a blocker.
11. Update only the smallest necessary active docs.
12. Run `uv run --no-project python scripts/check_agent_handoff.py`.
13. Commit in English, push, open a PR, and wait for complete green CI.
14. Merge only when the playbook classifier permits it and no human objection exists.
15. Report packet ID, model profile, files, tests, CI run, compatibility, residual risk, rollback, and next packet state.

If another agent has in-progress changes, do not overwrite them. Build on them only after auditing scope and ownership; otherwise leave them untouched and report the conflict.

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

Add release, browser, Docker, migration, backup/restore, or fault-specific checks when the packet touches those surfaces.

## Documentation Maintenance Rule

Keep the documentation set small. Do not create new roadmap, status, policy, closeout, or productization documents by default.

Authoritative surfaces:

- `docs/ARCHITECTURE_BOOK.md` — current architecture, data ownership, and durable boundaries
- `docs/CURRENT_STATUS.md` — current facts and limitations
- `docs/NEXT_DECISION.md` — single forward plan and Terra-ready packets
- `docs/MODULE_MAP.md` — source/test ownership
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` — PR, CI, evidence, and merge discipline
- `docs/RUNBOOK.md` — proven operator procedures
- `AGENTS.md` — Codex execution contract
- `.codex/config.toml` — local Codex model defaults

Prefer editing, shortening, or deleting stale text over adding another document. When facts change, update only the smallest necessary authoritative surfaces.