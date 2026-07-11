# Project Instructions

## Product Scope

**What**: Local deterministic harness and self-hosted workflow control plane for studying token-efficient agent workflows. It provides deterministic dispatch planning, local API/Dashboard access, app-owned SQLite/PostgreSQL-compatible state, dynamic workflow state, executor coordination primitives, and cost-of-pass metrics.

**What NOT**: Not a cloud production SaaS, hosted multi-tenant service, or unbounded direct-deploy tool. Registered target working trees and `main` remain protected by existing output authority. Trusted-local execution remains bounded by auth, credentials, budgets, audit, rollback, and kill controls.

**Target user**: Solo developer or small local team studying deterministic agent infrastructure on one machine or LAN.

## Current State

**Active tracks:**
- Real-World Testing Mode — validated through real tasks, branches, commits, PRs, CI, and gated autonomous merge
- Agent Autonomous Maintenance Mode — agents audit, plan, implement, verify, review, document, and ship bounded changes
- Full Agent Autonomy Mode — repository-scoped architecture, authority, security, migration, release-workflow, recovery, and target-output evolution is authorized when testable, observable, CI-gated, and rollbackable
- Post-LGB Product Evolution — PE-1 and PE-2 are complete; PE-3 is active with its decision contract implemented; later stages are governed by `docs/NEXT_DECISION.md`

**Complete tracks:**
- Dispatch Kernel Phases 1–7
- Rust runtime migration
- Dynamic Workflow Batches 1–7
- Macro-Orchestrator repair track
- Self-Hosted GA readiness
- HA hardening
- V2 Real Production Output
- Adaptive Fusion through AF-7
- Agent Runtime through AR-6
- Trusted Local Autonomous Execution through IAE-3
- PE-1 Token Efficiency Regression Lab

**Key facts:**
- Rust `engine/` is the sole runtime, API, and storage implementation
- Architecture Refactor R-series remains baselined at R7; documented, tested, rollbackable decisions may supersede it
- **Post-R7 Wire/Type Governance Hardening**: IMPLEMENTED through `scripts/check_wire_codegen_drift.sh`

## App Runtime vs Agent Maintenance Boundary

**App/runtime** does not write target repos by default. Existing approval, worktree, branch, patch, audit, budget, and rollback controls remain authoritative.

**Agent maintenance** may autonomously inspect, plan, implement, test, review, merge, and iterate repository-scoped work. This includes new architecture directions, authority-boundary changes, execution-profile changes requested by the user, auth/security redesign, database migrations, release/tag/deploy workflow changes, target-output authority changes, recovery contracts, and superseding accepted decisions.

Material decisions must be evidence-backed, recorded in an existing authoritative document, tested, reviewable, and rollbackable. Do not silently create parallel runtimes, stores, schedulers, policy authorities, or state models.

**Hard stops:** committing real secrets; falsifying test/CI evidence; intentionally hiding failures; removing rollback without a tested replacement; bypassing required human approval; or performing irreversible external destruction without explicit authority and recovery.

## Model Selection

Model and reasoning-effort selection are controlled by the user or execution environment. This repository does not require a particular model, model family, or reasoning tier.

Do not edit model configuration merely to satisfy repository instructions. Model choice never reduces testing, review, CI, audit, compatibility, compensation, or rollback requirements.

## Minimal Agent Reading Model

Read before implementation:

- `AGENTS.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` for PR, CI, review, and merge work
- `docs/ARCHITECTURE_BOOK.md` for architecture, storage, authority, security, release, or recovery work
- `docs/RUNBOOK.md` only for proven operator procedures

Treat current code, merged history, tests, and CI as authoritative evidence. Repair stale documentation rather than silently following it.

## Architecture Summary

Deterministic, rule-based pipeline:

```text
Request → TaskAnalyzer → ModelSelector → BudgetManager → DispatchDecision → Executor → Evaluation → Ledger
```

Key principles:
- Rule-based dispatch kernel
- Versioned bounded schemas
- Explicit authority and failure states
- Persistent auditability
- Compatibility and rollback

Current architecture baseline: `docs/ARCHITECTURE_BOOK.md`.

## Autonomous Advancement Protocol

For each autonomous session:

1. Inspect branch, working tree, open PRs, recent merges, and CI state.
2. Read the active documents and choose the highest-value eligible packet, prerequisite repair, or bounded decision.
3. Audit existing code before assuming a capability is absent.
4. Resolve bounded missing decisions from repository evidence; record material decisions in an authoritative document.
5. Add or update focused tests before behavior changes when practical.
6. Implement one coherent reviewable slice using the tools available in the current environment.
7. Run focused verification and all applicable full checks.
8. Review authority, compatibility, security, audit, compensation, and rollback.
9. Repair failures at their root cause; do not weaken tests or guards.
10. Update the smallest necessary active documentation.
11. Run `uv run --no-project python scripts/check_agent_handoff.py` and `bash scripts/check_wire_codegen_drift.sh`.
12. Commit in English, push, open or update a PR, and wait for complete green CI.
13. Merge only when the real-world testing playbook permits it.
14. Refresh `main` and continue when the bounded objective includes later packets.
15. Report decisions, files, tests, CI, compatibility, residual risks, rollback, and next state.

## Rules

1. Reference the architecture book for durable implementation decisions.
2. Update schemas and authoritative documentation together.
3. Preserve safety, audit, approval, budget, and rollback boundaries.
4. Use `docs/NEXT_DECISION.md` as the single forward plan.
5. Keep documentation current and small.
6. Run the handoff guard before commit.
7. Do not invent evidence or claim CI success before all required jobs complete.
8. Do not overwrite another agent's active work without reconciling scope and ownership.

## Documentation Maintenance

Authoritative surfaces:
- `docs/ARCHITECTURE_BOOK.md` — current architecture and boundaries
- `docs/CURRENT_STATUS.md` — current state and limitations
- `docs/NEXT_DECISION.md` — single forward plan
- `docs/MODULE_MAP.md` — source/test ownership
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` — maintenance, PR, CI, and merge workflow
- `docs/RUNBOOK.md` — proven operator procedures
- `README.md`, `CLAUDE.md`, `AGENTS.md` — entrypoints and agent boundaries

Prefer prune/archive/link over adding more prose.

## Code Style

- Python 3.10+, dataclasses for schemas, no pydantic in the deterministic kernel
- Deterministic, testable, auditable behavior
- Comments only when the reason is non-obvious
- English commit messages, concise and focused on why

## Test Strategy

- Rust `cargo test` for engine behavior
- Python `unittest` for Python SDK and tools
- TypeScript/Bun tests for SDK and Dashboard
- Real PostgreSQL tests for persistence compatibility
- Test alongside implementation
- GitHub Actions on pull requests and pushes to `main`

## External Dependencies

The dispatch kernel remains free of runtime LLM dependencies. Provider integrations stay behind existing bounded adapters and gates.
