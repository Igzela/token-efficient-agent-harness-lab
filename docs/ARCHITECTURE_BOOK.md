# Architecture Book

Last updated: 2026-07-25.

Current accepted version: v31.

This is the durable architecture and safety baseline for the Token-Efficient Agent Harness Lab. Current facts live in `docs/CURRENT_STATUS.md`; routing and gates live in `docs/NEXT_DECISION.md`; concrete owners live in `docs/MODULE_MAP.md`. Historical packet details remain available in git history.

Open PR #299 proposes schema **v32** (hash-linked decision transition sequence receipts) and **v33** (managed-acceptance spend/lease logical authorization). Those versions are **not accepted architecture** until independent review and merge; accepted `main` remains schema v31 (see Storage).

## Mission

The system is a local/small-team self-hosted control plane for auditable coding-agent workflows. It may create bounded patches and Draft PRs for real repositories, but it is not a cloud SaaS, a direct-deploy tool, or an autonomous production operator.

Its single first-order objective is:

> Under non-negotiable quality, safety, traceability, compatibility, and rollback constraints, continuously increase verifiable and reusable task delivery per unit of total lifecycle cost.

The system does not optimize token count in isolation. A lower-token result is not better unless it meets the same accepted quality, safety, and integrity gates.

## Decision Model

Architecture and evolution decisions use two layers:

1. **Hard gates** — correctness, safety, authority, evidence integrity, compatibility, rollback, atomicity, restart, concurrency, and contamination controls.
2. **Optimization evidence after hard gates pass** — quality, token/request use, latency, cost semantics, robustness, implementation effort, maintenance surface, migration risk, failure recovery, expected reuse, and realistic implementation feasibility.

A single scalar score must not hide a failed hard gate. Multi-objective Pareto comparison is preferred when dimensions conflict.

```text
Verified Delivery Efficiency
=
comparable verified and reusable delivery
/
(runtime cost + amortized implementation cost + maintenance cost + failure-recovery cost)
```

This is a decision principle, not caller-supplied production authority.

## Product Boundary

Default posture:

- Provider and managed-CLI execution are default-off and require explicit accepted contracts.
- Target output is default-off and limited to app-owned worktrees, bounded patch export, or `acp/*` Draft PR creation after separate approval and output confirmation.
- Target working trees and protected/default branches remain unchanged.
- No runtime path may merge, tag, release, deploy, install, or adopt a candidate as production state.
- External runtimes, CLIs, and repositories are adapters or evidence sources; they are not replacement schedulers, stores, policy kernels, or authority owners.
- Provider calls are forbidden in CI.
- Secrets, credentials, raw prompts, raw outputs, transcripts, private paths, and unredacted repository content are excluded from durable evidence.

Full Agent Autonomy Mode permits repository-scoped work that remains testable, observable, reviewable, verification-gated, compatible, and rollbackable. It does not grant provider spend, target output, merge, release, deployment, or production-adoption authority.

## Runtime Shape

```text
HTTP API / SDK / Dashboard
        |
        v
Rust composition root
        |
        +--> Dispatch analysis and planning
        +--> Workflow scheduler / executor pool / node executors
        +--> Product Golden Path owners
        +--> Evidence / replay / scorecards / Harness Evolution laboratory
        |
        v
LocalProductStore
SQLite default / PostgreSQL parity backend
```

Rust is authoritative for state transitions, workflow execution, permissions, budgets, leases, approvals, evidence, output reconciliation, audit, and persistence. TypeScript and Python remain interaction, projection, adapter, evaluation, or offline-research layers.

## Authority Invariants

There is exactly one canonical owner for each effect class:

- workflow/run/node state and leases;
- ProductTask admission and budget;
- process execution and outcome;
- workspace/source/patch lifecycle;
- verification;
- artifact capture;
- human approval;
- output confirmation and Draft PR/export;
- terminal evidence;
- persistence, audit, migrations, and rollback.

The following remain separate:

```text
risk acknowledgement
!= spend authorization
!= execution admission
!= artifact approval
!= output confirmation
!= merge/release/deployment
```

No earlier authority implies a later one. Caller assertions, environment booleans, free-form actor strings, fixture identities, or locally computed hashes cannot establish production authority.

## Product Golden Path

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
→ acp/* Draft PR or bounded patch export
→ canonical terminal evidence
```

Every stage binds to exact current identities: tenant/workspace, ProductTask version, plan/run/node attempt, lease/owner token, executable/provider/model, budget, worktree/source revision/tree, allowed mutable paths, verification result, artifact, approval, output operation/receipt, and audit.

Missing, stale, conflicting, duplicate, late, revoked, expired, paused, killed, over-budget, lost-lease, or outcome-unknown state fails closed. Fixture completion proves wiring only; it is not managed acceptance, live RWE, or product capability proof.

## Managed Process Boundary

The accepted managed-process owner provides exact executable identity, cleared/minimal environment, bounded output and time, descendant cleanup where proved, typed process outcomes, and non-retryable handling after an effect may have begun.

There is no universal cross-executor sandbox that can be treated as a complete security boundary.

Codex has executor-specific bubblewrap filesystem mediation and optional user/PID namespaces. Full admission is not established: internal retry identity remains wire-unproved, product-enforced loopback-only network confinement remains unproved under the current unprivileged profile, and host namespace capability may fail closed.

Claude Code remains fail-closed because provider-independent worktree-only confinement is not proved. OpenCode real-binary admission remains deferred.

## Codex Mediation and Budget

Accepted `main` includes a Rust-owned loopback `CodexBudgetGateway`, parent-held upstream credential, parent-owned fail-closed usage journal outside the child sandbox, exact provider identity binding, and gateway-to-`execution_usage_event.v1` evidence mapping.

The child receives no reusable upstream credential. ProductTask remains the sole budget owner. Gateway measurement is primary; Codex JSONL/session records are corroborating post-call evidence.

Current class: `mediation_hardened_partial`.

Residual risks remain explicit:

1. no trustworthy wire identity for Codex internal retries;
2. no proved product-enforced loopback-only network confinement under the current host profile;
3. host-dependent user/PID namespace support;
4. live operation requires separately accepted operator risk and spend authority.

A bounded live trial under partial mediation requires an explicit authority decision and must not be described as full admission.

## Usage and Cost Evidence

`execution_usage_event.v1` is the normalized post-call evidence contract.

Canonical token buckets are non-overlapping:

```text
fresh_input
cache_read
cache_creation
non_reasoning_output
reasoning_output
```

Provider totals that include cache or reasoning sub-buckets must be canonicalized before persistence. Ambiguous semantics are marked partial/ambiguous rather than guessed.

Provider/request identities must come from trustworthy owner or provider evidence. An execution ID must not be reused as a pretend per-request identity when multiple provider rounds are possible.

Cost semantics are explicit:

- `provider_reported` only from trustworthy monetary semantics;
- `local_estimate` only from a versioned provider/model-bound pricing table;
- `cost_unavailable` when neither is trustworthy.

Unknown price is unavailable, not zero. Local estimates never become billing receipts or pre-call spend authority unless a separately reviewed gateway contract enforces them.

## Evidence and Lifecycle Cost

Runtime evidence includes quality/pass status, tokens, context/repetition, tool calls, retries, latency, cost source, recovery, approval/output, and terminal bindings.

Engineering/lifecycle-cost evidence begins as a bounded `implementation_cost_receipt` in each board report:

```text
agent_sessions
review_cycles
repair_iterations
ci_runs
ci_compute_minutes
files_changed
schema_migrations
compatibility_adapters_added
authority_boundaries_touched
external_dependencies_added
rollback_complexity
known_maintenance_surface
expected_reuse_count
```

This evidence informs RWE replay and Level-2 decisions. It does not create a second runtime budget, scheduler, store, or evaluator.

## Storage

`LocalProductStore` is the sole application-owned persistence and transaction boundary.

- SQLite is default and uses existing transactional, integrity, backup, and restore owners.
- PostgreSQL must preserve equivalent validation, locking, idempotency, audit, restart, concurrency, and rollback behavior.
- Schema migrations are additive unless separately reviewed destructive rollback is explicitly authorized.
- Accepted `main` is currently schema v31. Higher managed-acceptance/RWE schema versions proposed on open PR branches are not current architecture until final acceptance and merge.

## Real Workload Evidence

The first RWE baseline is the prerequisite for Architecture Convergence.

A valid corpus is real, versioned, hash-bound, replayable, fixed before convergence, bound to exact task/source/mutable-surface/verification/output/executor/budget identities, executed under separate one-use RWE spend authority, and incapable of labeling fixture execution as a live baseline.

The baseline records product quality, runtime evidence, recovery behavior, approval/output/terminal bindings, and the implementation-cost receipt.

## Architecture Convergence

Architecture Convergence is incremental compatibility work, not a rewrite:

1. AC1 unified `ProcessSupervisor`.
2. AC2 typed execution boundary.
3. AC3 Golden Path responsibility split.
4. AC4 transaction-scoped domain views.
5. AC5 explicit runtime composition root.
6. AC6 Rust-authoritative API/SDK/Dashboard schema convergence.
7. AC7 obsolete-abstraction cleanup after all callers and evidence migrate.

Each packet changes one coherent ownership boundary, preserves compatibility and rollback, and records implementation cost. The identical frozen RWE corpus is replayed after AC1–AC7.

## Harness Evolution

Level-1 is a default-off one-generation laboratory with immutable active-Harness identity, candidate lineage, equal-budget evaluation, hard gates, sealed holdout, Pareto archive, operator acknowledgement, and PR_READY output. It stops before production adoption.

Level-2 is eligible only after an evidence-backed GO decision using Golden Path stability, pre/post-convergence RWE, contamination risk, lifecycle cost, implementation feasibility, and existing Level-1 composition.

Even on GO, Level-2 remains bounded and may not modify `main`, merge, deploy, rewrite its evaluator, expand its authority, or adopt a production Harness automatically.

The Meta Improver is later and separately authorized. It requires unseen tasks, immutable evaluator/labels, contamination controls, baselines, statistical thresholds, seeds, budgets, and stop/rollback rules. A NO-GO result is valid completion.

## External Adapter Boundary

External projects may provide bounded parsers, adapters, protocol compatibility, or comparison evidence. They must not become required core dependencies or replacement authorities.

CC Switch may be used as an MIT-licensed implementation reference for usage parsing, stream aggregation, model normalization, endpoint recognition, and pricing estimates. Its OAuth/account switching, credential persistence, automatic failover/retries, desktop UI authority, proxy database, and configuration ownership are outside this architecture.

Every adaptation records exact upstream commit, source mapping, license/attribution, semantic differences, and tests proving that core authority remains unchanged.

## Dashboard Boundary

The Dashboard and SDKs project accepted Rust-owned schemas and controls. They may display status, evidence, budgets, approvals, output operations, and lifecycle-cost summaries, but they do not become workflow, spend, approval, output, merge, release, or deployment authorities. Dashboard PR #225 remains presentation-only and last.

## Safety and Non-Claims

The repository does not currently claim full Codex admission, managed Claude/OpenCode admission, accepted live Golden Path completion, accepted live RWE, completed Architecture Convergence, automatic multi-generation evolution, demonstrated continuous learning, production self-update, or autonomous merge/release/deployment.

Those claims require the evidence and gates in `docs/NEXT_DECISION.md`.

## Document Roles

- `ARCHITECTURE_BOOK.md` — durable mission, owners, boundaries, and invariants.
- `CURRENT_STATUS.md` — accepted main truth, open review surfaces, and blockers.
- `NEXT_DECISION.md` — execution order, entry/exit evidence, and immediate next action.
- `MODULE_MAP.md` — canonical owners and proposed-but-unmerged surfaces.
- `REAL_WORLD_TESTING_PLAYBOOK.md` / `RUNBOOK.md` — operational validation and procedures.

Prefer updating these active documents over adding parallel strategy, status, or policy files.
