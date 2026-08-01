# Module Map

Last updated: 2026-08-01.

This is the concise ownership map for accepted `main`. Current facts are in `docs/CURRENT_STATUS.md`; execution order and gates are in `docs/NEXT_DECISION.md`; architecture invariants are in `docs/ARCHITECTURE_BOOK.md`.

Open PR branches are listed separately and are not canonical owners until merged. PR #300 (provider-free RWE authority), PR #301 (CC Switch observation-only adaptation), PR #306 (non-authoritative context-capsule automation), and PR #308 (provider-free ProductTask workspace preparation/recovery) are merged and accepted.

Full Agent Autonomy Mode permits repository-scoped work that is testable, observable, reviewable, verification-gated, compatible, and rollbackable. Provider calls, target output, release, deployment, spend, and authority-critical actions retain separate gates.

## Core Ownership

| Area | Canonical owner | Boundary |
|---|---|---|
| Repository navigation and context capsule | `START_HERE.md`, `scripts/project_context.py`, `tools/test_project_context.py`, `scripts/check_agent_handoff.py`, `.github/workflows/tests.yml` (context-capsule publisher job), `scripts/agent-control/prompt_builder.py` | One on-demand fail-closed transport view and exact-head workflow publication/session injection derived from accepted owners; not a status database or authority owner |
| API and composition root | `engine/src/main.rs`, `engine/src/http_server/` | Sole startup/composition surface; runtime modes remain explicitly gated |
| Dispatch analysis | `engine/src/dispatch_engine.rs`, analyzers, selectors, planners, decomposers | Advisory/default-noop unless an accepted contract admits execution |
| Workflow runtime | `engine/src/workflow/`, `engine/src/scheduler.rs`, `engine/src/scheduler/`, `engine/src/executor_pool.rs`, `engine/src/node_executor.rs` | Sole persisted run, node, lease, retry, pause/kill, and concurrency path |
| Recursive/agent nodes | Existing AgentStep and recursive-execution modules plus scheduler/store | Typed bounded nodes; no autonomous root-goal generation, production self-update, or authority expansion |
| Managed CLI/process | `engine/src/cli/mod.rs`, `config.rs`, `cli_node_executor.rs`, `codex_managed_acceptance_preflight.rs`, process/probe owners | Bounded subprocess lifecycle, output limits, process-tree cleanup, exact executable identity, default-off admission, and lease-bound owner-derived pre-child preflight. Packet 1 generalizes this current Codex-shaped adapter into the Rust-owned managed-coding runtime-profile boundary without adding a process/scheduler/store owner. |
| Codex mediation and budget | `engine/src/cli/codex_budget_authority.rs`, `codex_mediation_admission.rs`, `codex_usage_journal.rs`, `codex_session_usage.rs` | Parent-held credential, loopback gateway, ProductTask budget enforcement, parent-owned fail-closed journal, session corroboration; class remains partial |
| Managed-acceptance authority | `engine/src/storage/local_product_store/managed_acceptance.rs` and existing store/migration owners through v36 | Store-owned immutable proposal/final manifest, authenticated delegation, separated manifest/spend and artifact/output receipts, one-use spend, attempt lease, provider-request journal, terminal cleanup, restart, replay, audit, and rollback authority; read-only current-lease validation supplies runtime preflight without creating another owner |
| Multi-executor usage evidence | `engine/src/execution_usage/` — accepted adapters and reconcile owners (PR #301) | `execution_usage_event.v1`; evidence only; never a second budget or spend authority |
| Managed provider-call protocol adapters | `engine/src/provider/managed_deepseek.rs`, `managed_deepseek_executor.rs`, existing protocol/usage adapters, and ProductTask managed-acceptance/store owners | Exact Pro-planner/Flash-implementer/Pro-reviewer routing beneath one ProductTask budget and durable pre-send request claim; parent-only credential resolution, exact usage reconciliation, and permanent no-retry after outcome unknown. Adapters never own budget, lease, approval, output, audit, or rollback state. |
| Workspace and patch | Existing supervised-patch/workspace owners, `engine/src/storage/local_product_store/product_tasks.rs`, and target-repository output owners | App-owned detached worktree, exact source binding, bounded patch and cleanup lifecycle. Packet 1 adds `local_folder` staging/manifest/preimage/rollback behavior under these same owners; it must not create a second workspace, lease, budget, output, or rollback owner. Accepted PR #308 adds a v35 ProductTask preparation receipt that pins one local recovery path before mutation; local/try-only PostgreSQL guards coordinate active work only and never become a second workspace, lease, budget, or rollback owner. |
| Verification | Existing product verification, managed-run, process-outcome, and tool-policy owners | Fixed admitted commands, exact workspace/source/patch bindings, pause/kill/late-write refusal |
| Artifact | Existing supervised artifact capture, integrity, redaction, and store owners | Atomic content/hash-bound artifact; no approval or output authority |
| Approval | Existing workflow/product approval owner | Separate current-state human approval; no execution or output mutation authority |
| Output | Existing target-repository output owner | Separate confirmation, `acp/*` only, Draft PR or patch export; no merge/default-branch/release/deploy authority |
| Terminal evidence | `engine/src/storage/local_product_store/` product terminal-evidence owners | Exact persisted task/run/node/workspace/source/artifact/approval/output/audit binding |
| Persistence and audit | `engine/src/storage/local_product_store/` and PostgreSQL backend | Sole SQLite/PostgreSQL transaction, migration, audit, idempotency, evidence, and rollback owner |
| Scorecards/replay | Existing scorecard, trace, replay, and store owners | Derived comparison evidence; cannot mutate live routing or policy by itself |
| Real Workload Evidence corpus | `engine/src/rwe/` and fixture corpus owners from PR #300 | Provider-free corpus authority, authorization/run/task-attempt persistence; no live baseline from fixtures |
| RWE economic protocol and VDE artifacts | `engine/src/rwe/economic_protocol.rs` (PR #319) | Immutable/hash-bound protocol and artifact validation only; no runtime, store, budget, reviewer, output, adoption, or release authority |
| Harness Evolution Level-1 | `engine/src/harness_evolution*.rs` plus existing store owners | Default-off one-generation fixture laboratory; active Harness immutable |
| SDK and Dashboard | `sdk/`, `dashboard/` | Typed interaction and projection only; no backend authority |
| Wire contracts | `wire_contract/`, `codegen/` | Shared cross-language schemas; drift checked by `scripts/check_wire_codegen_drift.sh` |
| Event schema contract | `engine/src/event_schema.rs` | Canonical event schema validation, idempotency hashing, and JSONL evidence guard (`docs/stage0/events.jsonl`); production module with no dependency on the deleted reference surface. The reference-only `engine/src/event_source/` module (append-only event store, projections, task queue) and the reference-only error types in `engine/src/errors.rs` are deleted: they had no production caller, the active runtime and `LocalProductStore` are the sole store/runtime/audit owners, and event-sourcing reference value is owned by `docs/ARCHITECTURE_BOOK.md` plus `event_schema` |
| CI and repository automation | `.github/`, `scripts/`, `tools/`, `scripts/agent-control/` | Verification and optional/parked repository automation; no implicit product/release authority. `tools/check_security_baseline.py` is the sole fail-closed guard owner for unattended-automation patterns (`dangerously-skip-permissions`, unbound `gh run list --limit 1` CI judgment, `gh run watch` chained to an unbound list or without a run id) in repository-controlled automation (PR #326, semantics refined in PR #336), for the composite legacy fingerprint of the deleted plugin surface (PR #331, composite in PR #336), and for the dormant-surface heuristic gate (module-level dead-code blankets, module islands, self-described placeholder modules, no-op executors, conflicting sole-owner claims; PR #336). Guard exceptions require classification entries with owner, reason, review condition, and expiry/recheck condition; no second CI policy owner exists |

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
| #225 | Dashboard presentation | Last; may project accepted schemas only |

PR #297/#298 are closed without merge as superseded by accepted PR #299. PR #300 is merged and accepted. PR #301 is merged and accepted (observation-only; no authority import). PR #306 is merged and accepted (context transport only; no authority import). PR #308 is merged and accepted (provider-free workspace preparation/recovery only; no live authority import). PR #303 is closed without merge as superseded by accepted PostgreSQL ordering repair PR #304.

Managed-coding profiles, DeepSeek protocol adapters, production runner wiring, and delegated autonomous Golden Path authority reuse the owners above. The next external-effect frontier is the single separately bounded live seal; RWE and later architecture/evolution work remain blocked by their named evidence gates. PR #225 remains independent and last.

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

- Context capsules summarize and route from accepted owners; they never authorize execution, spend, output, merge, release, deployment, RWE acceptance, or production adoption.
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

- `START_HERE.md` — stable navigation, frontier discovery, capsule fields, staleness, and automation boundary.
- `docs/ARCHITECTURE_BOOK.md` — durable mission, architecture, authority, safety, and evidence invariants.
- `docs/CURRENT_STATUS.md` — merged truth, open review surfaces, and current blockers.
- `docs/NEXT_DECISION.md` — authoritative sequence, entry/exit gates, and immediate next action.
- `docs/MODULE_MAP.md` — real current owners and proposed-but-not-yet-canonical modules.
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` and `docs/RUNBOOK.md` — operational validation and procedures.
- `README.md`, `AGENTS.md`, and `CLAUDE.md` — repository entry and contributor/agent instructions.

Prefer updating these documents over adding parallel roadmap, status, or policy files.
