# Project Instructions

## Product Scope

**What**: Local deterministic harness and self-hosted macro-orchestrator control plane for studying token-efficient agent workflows. Provides deterministic dispatch planning, local API/dashboard access, app-owned SQLite state, dynamic workflow state, executor coordination primitives, and cost-of-pass metrics.

**What NOT**: Not a cloud production SaaS, hosted multi-tenant service, or direct-deploy tool. No process/container/VM isolation or direct target-repo `main` writes exist. Installed local Claude/Codex CLIs are available for task runs. The approved IAE track may enable bounded trusted-local provider and agent execution when auth, credentials, budgets, audit, rollback, and kill controls are valid.

**Target user**: Solo developer or small local team studying deterministic agent infrastructure on one machine or LAN.

## Current State (2026-06-23)

**Active tracks:**
- Real-World Testing Mode — validated through real tasks, branches, commits, PRs, CI, gated auto-merge
- Agent Autonomous Maintenance Mode — agents autonomously audit, implement, verify, review, document, and ship bounded changes
- Full Agent Autonomy Mode — repo-scoped architecture, authority, security, migration, release-workflow, and target-output evolution is authorized when testable, observable, CI-gated, and rollbackable
- Trusted Local Autonomous Execution Track (IAE) — complete through IAE-3 trusted-local profile, bounded task advancement, and operator control/evidence
- Adaptive Fusion Routing Track — complete through AF-7; IAE trusted-local profile is the recommended bounded local path, with legacy gates retained for compatibility

**Complete tracks:**
- Dispatch Kernel Phases 1–7 (including 6A, 6B-1/2/3, Gates 1–3): STABLE
- Language migration: COMPLETE (Rust engine is sole runtime)
- Dynamic Workflow Batches 1–7 + scheduler dynamic-mode: COMPLETE
- Macro-Orchestrator Phases 1–5 repair batch: COMPLETE
- Self-Hosted GA Readiness Track SG-1 through SG-5: COMPLETE
- HA Hardening Track HA-1 through HA-6: COMPLETE (scheduler resilience, automated backup, deep health, circuit breaker, TLS, SQLite encryption)
- HybridExecutor with `ACP_EXECUTION_MODE`: COMPLETE
- Dashboard Onboarding UX ON-1 through ON-5: COMPLETE
- V2 Real Production Output Track: COMPLETE (V2-0 through V2-5 merged in PRs #69-#75)
- Real Output Closeout: COMPLETE (`v0.1.0` published and online installer verified)

**Key facts:**
- Full Rust + TypeScript stack verification passes with 0 failures (2026-06-22)
- Latest release: `v0.1.0`, published 2026-06-21
- TypeScript strict + boundary lint + build + static export pass
- `cargo fmt`, `cargo clippy`, handoff guard all pass
- **Architecture Refactor R-series**: baselined at R7; documented, tested, rollbackable decisions may supersede it.
- **Post-R7 Wire/Type Governance Hardening**: IMPLEMENTED

## App Runtime vs Agent Maintenance Boundary

**App/runtime** does not write target repos by default. V2-3 adds only env-gated, approval-bound output through an app-owned git worktree and `acp/*` branch or patch export; registered target working trees and `main` remain protected.

**Agent maintenance** may autonomously propose, implement, test, review, merge, and iterate high-risk architecture work. Full Agent Autonomy Mode includes new architecture directions, authority-boundary changes, default execution/profile changes, auth/security redesign, database migrations, release/tag/deploy workflow changes, target-output authority changes, and superseding accepted decisions. Changes should remain repo-scoped, testable, observable, reviewable, and rollbackable. Documentation-only corrections may go directly to `main` after validation.

**Only hard stops:** committing real secrets; falsifying test/CI evidence; intentionally hiding failures; removing rollback paths; or performing irreversible external destruction without a recovery path.

## Minimal Agent Reading Model

**Default:** Read `CLAUDE.md` only.

**Read conditionally:**
- `docs/NEXT_DECISION.md` — when choosing or validating next work
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` — when opening PRs, auto-merging, fixing CI, cleaning docs, or running real-world pilot tasks
- `docs/MODULE_MAP.md` — when changing code or deciding module ownership
- `docs/CURRENT_STATUS.md` — when status facts are unclear or when updating status
- `docs/ARCHITECTURE_BOOK.md` — when current architecture or boundary facts are needed

## Architecture Summary

Deterministic, rule-based pipeline:
```
Request → TaskAnalyzer → ModelSelector → BudgetManager → DispatchDecision → Executor → Evaluation → Ledger
```

Key principles:
- Rule-based only (no LLM calls in dispatch kernel)
- Dataclass schemas, no pydantic
- Phase boundaries enforce safety
- Event-sourced ledger for auditability

Current architecture baseline: `docs/ARCHITECTURE_BOOK.md`

Historical dispatch architecture and closeout details are retained in release-tagged git history; `docs/archive/README.md` is the working-tree index.

## Autonomous Advancement Protocol

For each autonomous session:

1. Inspect `git status --short --branch` and read this file.
2. Conditionally read `docs/NEXT_DECISION.md` and `docs/REAL_WORLD_TESTING_PLAYBOOK.md` based on task type.
3. Choose the highest-value task from failing verification, CI/docs/test drift, review findings, the next authorized IAE phase, or bounded product improvement.
4. **All sessions must use Workflow tool for implementation.** Write a workflow script to `.claude/workflows/` with `parallel()` for independent subtasks and `pipeline()` for sequential dependencies. Use `model: 'opus'` for implementation agents and `model: 'sonnet'` for verification. The only exception is trivial single-line edits (typo, doc wording, env var). Anything touching 2+ files goes through Workflow.
5. Update or add tests before behavior changes.
6. Run the full verification suite: `cargo test -p engine`, `cargo fmt --check`, `cargo clippy -p engine --all-targets -- -D warnings`, TypeScript build/test, dashboard build, `uv run --no-project python scripts/check_agent_handoff.py`, `bash scripts/check_wire_codegen_drift.sh`.
7. **CI must be green before starting the next batch.** After pushing, use `gh run list --limit 3` to check CI status. If CI fails, fix and re-push. A green CI is required before the next session's work is considered safe to build on.
8. Update the smallest necessary handoff surface before commit.
9. Commit in English and push when the working tree only contains this session's intended changes.
10. Leave the next action, latest commit, verification, and residual risks in the final report.

## Rules

1. **Reference architecture book** before implementation decisions
2. **Never deviate from schemas** without updating the book first
3. **Phase boundaries are sacred**: Follow safety constraints strictly
4. **When blocked**: Discuss with GPT, iterate until agreement, then update architecture book
5. **Document maintenance**: Keep handoff surface current and small
6. **Autonomous closeout**: Run `uv run --no-project python scripts/check_agent_handoff.py` before commit
7. **Single forward plan**: `docs/NEXT_DECISION.md` is the only roadmap surface
8. **Workflow tool is the default for all implementation**: All sessions use Workflow tool for any task touching 2+ files. Write script to `.claude/workflows/`, use `parallel()`/`pipeline()`, launch with `Workflow({scriptPath})`. Only trivial single-line edits bypass Workflow.

## Documentation Maintenance

Before committing, update smallest necessary handoff surface if change affects:
- Status, scope, tests, commands, boundaries, modules, or next steps

Authoritative surfaces:
- `docs/ARCHITECTURE_BOOK.md` — current architecture and boundaries
- `docs/CURRENT_STATUS.md` — current state, verification, test counts
- `docs/NEXT_DECISION.md` — single forward plan
- `docs/MODULE_MAP.md` — source/test ownership
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`, `docs/RUNBOOK.md` — maintenance workflow and operator procedures
- `README.md`, `CLAUDE.md`, `AGENTS.md` — quickstart, agent workflow, boundaries

Prefer prune/archive/link over adding more prose.

## Code Style

- Python 3.10+, dataclasses for schemas, no pydantic
- Rule-based logic (no LLM calls in dispatch kernel)
- Deterministic, testable, auditable
- No comments unless WHY is non-obvious
- Commit messages: English, concise, focus on why

## Test Strategy

- **Framework**: Rust `cargo test` for engine; Python `unittest` for SDK
- **Run commands**: `cargo test -p engine` (primary); SDK tests in `sdk/python/`; PostgreSQL integration: `cargo test -p engine --features pg-tests` (requires `ACP_TEST_DATABASE_URL`)
- **Current count**: 1654 Rust tests, 0 failures
- **CI**: GitHub Actions on push/PR to main
- **Test-first**: Write tests alongside implementation

## GPT Collaboration Protocol

ChatGPT session: https://chatgpt.com/c/69fc96b0-2e48-839f-a031-557e9e2317ca

When you encounter schema ambiguity, interface questions, phase boundary edge cases, or cross-phase integration uncertainties:
1. Get GPT's analysis
2. Independently audit suggestions
3. Share perspectives with user if needed
4. Update architecture book with agreed changes
5. Push to GitHub for GPT reference

## External Dependencies

- Python stdlib only — zero runtime dependencies
- No runtime LLM dependencies in dispatch kernel
