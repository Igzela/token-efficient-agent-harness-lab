# Module Map

Last updated: 2026-07-23.

This is the concise ownership map. Current facts are in `docs/CURRENT_STATUS.md`; packet routing is in `docs/NEXT_DECISION.md`.

Full Agent Autonomy Mode permits repository-scoped work that is testable, observable, reviewable, verification-gated, compatible, and rollbackable. Provider calls, target output, release, deployment, and authority-critical actions retain separate gates.

## Core Ownership

| Area | Canonical owner | Boundary |
|---|---|---|
| API/startup | `engine/src/main.rs`, `http_server/` | sole composition root; product/scheduler modes are gated |
| Dispatch/planning | `dispatch_engine.rs`, analyzers, selectors, planners, decomposers | advisory/default-noop unless an explicit contract admits execution |
| Workflow runtime | `workflow/`, `scheduler.rs`, `scheduler/runtime.rs`, `executor_pool.rs`, `node_executor.rs` | sole persisted run, lease, retry, and concurrency path |
| Agent/recursive runtime | AgentStep/recursive execution owners plus scheduler/store | typed bounded nodes; no autonomous root or self-improvement |
| Managed CLI/process | `cli/mod.rs`, `cli/config.rs`, `cli/cli_node_executor.rs`, `cli/codex_budget_authority.rs`, `cli/codex_mediation_admission.rs`, `cli/codex_residual_admission.rs`, `cli/codex_usage_journal.rs`, `cli/codex_session_usage.rs` | bounded process owner, probe, parser, admission, Codex loopback budget gateway, parent-owned fail-closed usage journal, bwrap+PID mediation isolation, residual-admission findings (`codex_residual_admission_finding.v1`), session usage evidence |
| Multi-executor usage evidence | `execution_usage/` (`codex_adapter`, `claude_adapter`, `opencode_adapter`, `provider_adapter`, `gateway_adapter`, `reconcile`) | normalized `execution_usage_event.v1` adapters + cross-source reconcile; evidence only, not a second budget owner; gateway is primary for mediated Codex |
| Workspace | supervised-patch and `target_repo_output` owners | app-owned worktree, patch, source, and target-output lifecycle |
| Verification/repair | supervised-patch verification, API-owned managed runs, tool-policy receipts | fixed read-only commands; hashed persisted text; pause/kill/late-write checks |
| Artifact/approval/output | supervised capture/store, redaction/integrity, workflow approval, target-output owners | atomic artifact; separate approval and explicit output; `acp/*` only |
| Persistence/evidence | `storage/local_product_store/` | sole SQLite/PostgreSQL store, transaction, audit, evidence, migration owner |
| Harness Evolution | `harness_evolution*.rs` and existing store owners | default-off Level-1 fixture lab; active Harness immutable |
| SDK/Dashboard | `sdk/`, `dashboard/` | typed interaction/projection; no backend authority |
| Wire contracts | `wire_contract/`, `codegen/` | shared schemas; checked by `scripts/check_wire_codegen_drift.sh` |
| Repository agent/CI | `scripts/agent-control/`, `.github/`, `scripts/`, `tools/` | verification and parked/optional automation; no implicit release authority |

## Product Data Flow

```text
intake → task/worktree/source binding → executable graph → scheduler lease
→ bounded executor → verification → artifact → approval → output receipt
→ acp/* Draft PR (optional) → canonical terminal evidence
```

All records bind to the product task, exact version, plan/run/node attempt, lease, workspace, source, artifact, approval, output, replay/scorecard, and audit owners. Missing, stale, conflicting, late, duplicate, over-budget, killed, paused, or outcome-unknown state fails closed.

## Capability Boundaries

- RWE must reuse existing scorecard, replay, usage, audit, terminal-evidence, and cleanup owners; it may not create a second evidence store.
- Architecture Convergence is a sequence of small packets: unified process supervision, typed execution boundary, Golden Path responsibility split, transaction-scoped domain views, runtime composition, API/SDK/Dashboard schema convergence, then obsolete-abstraction cleanup.
- Level-2 is only a later evidence decision; Meta requires a separately authorized unseen-task experiment.
- OpenCode binary admission remains deferred; Vader/#208 remains stopped; PR #225 remains presentation-only.

## PE-5 Release Provenance Ownership

Existing release/package/container, provenance/SBOM, installer, deployment, signing, and rollback owners remain authoritative. No product or evolution packet gains release/deploy/install authority.

## PE-6 Fault Injection and Recovery Ownership

Existing disposable fault scenarios, SQLite/PostgreSQL recovery tests, stubs, cleanup, and rollback drills remain authoritative. Product/evolution work may reuse them but may not create a second recovery authority.

## Active Documents

`README.md`, `AGENTS.md`, `CLAUDE.md`, `docs/ARCHITECTURE_BOOK.md`, `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, `docs/MODULE_MAP.md`, `docs/REAL_WORLD_TESTING_PLAYBOOK.md`, and `docs/RUNBOOK.md` are the maintained set. Prefer pruning these over adding new roadmap/status/policy documents.
