# Module Map

Last updated: 2026-07-25.

This is the concise ownership map for accepted `main`. Current facts are in `docs/CURRENT_STATUS.md`; execution order and gates are in `docs/NEXT_DECISION.md`; architecture invariants are in `docs/ARCHITECTURE_BOOK.md`.

Open PR branches are listed separately and are not canonical owners until merged.

Full Agent Autonomy Mode permits repository-scoped work that is testable, observable, reviewable, verification-gated, compatible, and rollbackable. Provider calls, target output, release, deployment, spend, and authority-critical actions retain separate gates.

## Core Ownership

| Area | Canonical owner | Boundary |
|---|---|---|
| API and composition root | `engine/src/main.rs`, `engine/src/http_server/` | Sole startup/composition surface; runtime modes remain explicitly gated |
| Dispatch analysis | `engine/src/dispatch_engine.rs`, analyzers, selectors, planners, decomposers | Advisory/default-noop unless an accepted contract admits execution |
| Workflow runtime | `engine/src/workflow/`, `engine/src/scheduler.rs`, `engine/src/scheduler/`, `engine/src/executor_pool.rs`, `engine/src/node_executor.rs` | Sole persisted run, node, lease, retry, pause/kill, and concurrency path |
| Recursive/agent nodes | Existing AgentStep and recursive-execution modules plus scheduler/store | Typed bounded nodes; no autonomous root-goal generation, production self-update, or authority expansion |
| Managed CLI/process | `engine/src/cli/mod.rs`, `config.rs`, `cli_node_executor.rs`, process/probe owners | Bounded subprocess lifecycle, output limits, process-tree cleanup, exact executable identity, default-off admission |
| Codex mediation and budget | `engine/src/cli/codex_budget_authority.rs`, `codex_mediation_admission.rs`, `codex_usage_journal.rs`, `codex_session_usage.rs` | Parent-held credential, loopback gateway, ProductTask budget enforcement, parent-owned fail-closed journal, session corroboration; class remains partial |
| Multi-executor usage evidence | `engine/src/execution_usage/` — accepted adapters and reconcile owners | `execution_usage_event.v1`; evidence only; never a second budget or spend authority |
| Workspace and patch | Existing supervised-patch/workspace owners and target-repository output owners | App-owned detached worktree, exact source binding, bounded patch and cleanup lifecycle |
| Verification | Existing product verification, managed-run, process-outcome, and tool-policy owners | Fixed admitted commands, exact workspace/source/patch bindings, pause/kill/late-write refusal |
| Artifact | Existing supervised artifact capture, integrity, redaction, and store owners | Atomic content/hash-bound artifact; no approval or output authority |
| Approval | Existing workflow/product approval owner | Separate current-state human approval; no execution or output mutation authority |
| Output | Existing target-repository output owner | Separate confirmation, `acp/*` only, Draft PR or patch export; no merge/default-branch/release/deploy authority |
| Terminal evidence | `engine/src/storage/local_product_store/` product terminal-evidence owners | Exact persisted task/run/node/workspace/source/artifact/approval/output/audit binding |
| Persistence and audit | `engine/src/storage/local_product_store/` and PostgreSQL backend | Sole SQLite/PostgreSQL transaction, migration, audit, idempotency, evidence, and rollback owner |
| Scorecards/replay | Existing scorecard, trace, replay, and store owners | Derived comparison evidence; cannot mutate live routing or policy by itself |
| Harness Evolution Level-1 | `engine/src/harness_evolution*.rs` plus existing store owners | Default-off one-generation fixture laboratory; active Harness immutable |
| SDK and Dashboard | `sdk/`, `dashboard/` | Typed interaction and projection only; no backend authority |
| Wire contracts | `wire_contract/`, `codegen/` | Shared cross-language schemas; drift checked by `scripts/check_wire_codegen_drift.sh` |
| CI and repository automation | `.github/`, `scripts/`, `tools/`, `scripts/agent-control/` | Verification and optional/parked repository automation; no implicit product/release authority |

## Product Data Flow

```text
intake
→ ProductTask/worktree/source binding
→ executable graph
→ scheduler lease
→ bounded executor
→ verification
→ artifact
→ current approval
→ separate output confirmation
→ acp/* Draft PR or bounded export
→ canonical terminal evidence
```

Every effect binds to the exact task version, plan/run/node attempt, lease/owner token, workspace, source revision/tree, budget, artifact, approval, output operation/receipt, and audit identity. Missing, stale, conflicting, duplicate, late, over-budget, killed, paused, revoked, expired, or outcome-unknown state fails closed.

## Authority Split

The following are deliberately separate:

```text
execution admission
risk acknowledgement
one-use spend authorization
attempt admission and lease
artifact approval
output confirmation
merge/release/deployment
```

No earlier authority implies a later one. In particular, risk acknowledgement is not spend authority; execution is not artifact approval; approval is not output confirmation; Draft PR creation is not merge authority.

## Open Review Surfaces — Not Canonical Yet

| PR | Proposed modules/scope | Rule |
|---|---|---|
| #299 | Residual finding and partial-mediation decision under `engine/src/cli/`; proposed managed-acceptance decision/risk/spend/attempt tables and APIs under `engine/src/storage/local_product_store/`; owner-derived preflight and fixture dry-run | Cumulative authority review surface; supersedes #297/#298; not canonical until final approval/merge |
| #300 | Proposed `engine/src/rwe/`, real versioned corpus definitions/fixtures, RWE authorization/run/task-attempt persistence | Stacked on accepted #299 semantics; no live baseline from fixtures |
| #301 | Proposed `engine/src/execution_usage/{protocol_usage,model_normalize,pricing_estimate,endpoint_identity}.rs` plus adapter changes and third-party notices | Observation only; must canonicalize token buckets and preserve trustworthy provider/request identity; no authority import |
| #225 | Dashboard presentation | Last; may project accepted schemas only |

Do not copy explanatory labels into file names. Always inspect the actual final branch tree before documenting an owner.

## Architecture Convergence Map

Architecture Convergence reuses these owners and changes boundaries incrementally:

1. AC1 — one `ProcessSupervisor` owner for admitted process lifecycle.
2. AC2 — one typed execution boundary with executor-specific adapters.
3. AC3 — split Golden Path orchestration responsibilities without changing state semantics.
4. AC4 — transaction-scoped domain views over the existing store owner.
5. AC5 — one explicit runtime composition root.
6. AC6 — authoritative API/SDK/Dashboard schemas derived from Rust-owned contracts.
7. AC7 — delete obsolete abstractions only after all callers, fixtures, scripts, and replay evidence are migrated.

The frozen RWE corpus is the before/after compatibility oracle. Architecture work cannot create a second scheduler, database, budget, approval, output, evidence, or rollback owner.

## Cost and Efficiency Evidence

Runtime usage evidence stays under existing execution-usage, scorecard, and store owners. Engineering/lifecycle-cost evidence begins as a bounded board report and includes Agent sessions, review cycles, CI effort, repair iterations, migrations, authority boundaries touched, rollback complexity, maintenance surface, and expected reuse.

This evidence informs RWE replay and Level-2 GO/NO-GO. It does not become a caller-supplied production authorization or a second budget system.

## Capability Boundaries

- RWE must reuse existing task, scheduler, usage, scorecard, replay, audit, approval, output, terminal-evidence, and cleanup owners.
- External runtimes and repositories may provide bounded adapters, parsers, or comparison evidence; they may not replace the core owners.
- Provider/session logs are post-call evidence, not pre-call authority.
- Local price tables produce estimates only and must remain versioned and source-labeled.
- OpenCode binary admission remains deferred; Vader/#208 remains stopped; Dashboard #225 remains last.
- Release, package, provenance, signing, installer, deployment, and rollback pipelines remain outside product/evolution authority.

## PE-5 Release Provenance Ownership

Existing package/container builders, dependency locks, SBOM/provenance, signing, installer, release, deployment, and rollback owners remain authoritative. Product, RWE, and Harness Evolution work may reference their evidence but gain no release, signing, installation, or deployment authority.

## PE-6 Fault Injection and Recovery Ownership

Existing disposable fault scenarios, SQLite/PostgreSQL recovery tests, stubs, cleanup, compensation, and rollback drills remain authoritative. Product/evolution work may reuse them but may not create a second recovery authority or convert a fixture result into production acceptance.

## Active Documents

- `docs/ARCHITECTURE_BOOK.md` — durable mission, architecture, authority, safety, and evidence invariants.
- `docs/CURRENT_STATUS.md` — merged truth, open review surfaces, and current blockers.
- `docs/NEXT_DECISION.md` — authoritative sequence, entry/exit gates, and immediate next action.
- `docs/MODULE_MAP.md` — real current owners and proposed-but-not-yet-canonical modules.
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` and `docs/RUNBOOK.md` — operational validation and procedures.
- `README.md`, `AGENTS.md`, and `CLAUDE.md` — repository entry and contributor/agent instructions.

Prefer updating these documents over adding parallel roadmap, status, or policy files.
