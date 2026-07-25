# Architecture Book

Last updated: 2026-07-25.

This is the durable architecture and safety baseline for the Token-Efficient Agent Harness Lab. Current repository facts live in `docs/CURRENT_STATUS.md`; executable routing and gates live in `docs/NEXT_DECISION.md`; concrete owners live in `docs/MODULE_MAP.md`.

Historical phase plans and detailed closeout records remain available in git history. This file intentionally describes the current architecture rather than duplicating every historical packet.

## Mission

The system is a local/small-team self-hosted control plane for building and studying auditable coding-agent workflows. It may create bounded patches and Draft PRs for real repositories, but it is not a cloud SaaS, a direct-deploy tool, or an autonomous production operator.

Its single first-order objective is:

> Under non-negotiable quality, safety, traceability, compatibility, and rollback constraints, continuously increase verifiable and reusable task delivery per unit of total lifecycle cost.

The system does not optimize token count in isolation. A lower-token result is not better unless it meets the same accepted quality, safety, and integrity gates.

## Decision Model

Architecture and evolution decisions use two layers:

1. **Hard gates** — correctness, safety, authority, evidence integrity, compatibility, rollback, atomicity, restart, concurrency, and contamination controls.
2. **Optimization evidence after hard gates pass** — quality, token/request use, latency, cost semantics, robustness, implementation effort, maintenance surface, migration risk, failure recovery, expected reuse, and realistic implementation feasibility.

A single scalar score must not hide a failed hard gate. Multi-objective Pareto comparison is preferred when dimensions conflict.

A useful conceptual metric is:

```text
Verified Delivery Efficiency
=
comparable verified and reusable delivery
/
(runtime cost + amortized implementation cost + maintenance cost + failure-recovery cost)
```

This is a decision principle, not a caller-supplied production authorization.

## Product Boundary

Default posture:

- Provider and managed-CLI execution are default-off and require explicit accepted contracts.
- Target output is default-off and limited to app-owned worktrees, bounded patch export, or `acp/*` Draft PR creation after separate approval and output confirmation.
- Target working trees and protected/default branches remain unchanged.
- No runtime path may merge, tag, release, deploy, install, or adopt a candidate as production state.
- External runtimes, CLIs, and repositories are adapters or evidence sources; they are not replacement schedulers, stores, policy kernels, or authority owners.
- Provider calls are forbidden in CI.
- Secrets, credentials, raw prompts, raw outputs, transcripts, private paths, and unredacted repository content are excluded from durable evidence.

## Runtime Shape

```text
HTTP API / SDK / Dashboard
        |
        v
Rust composition root
        |
        +--> Dispatch analysis and planning
        |
        +--> Workflow scheduler / executor pool / node executors
        |       run queue -> lease -> bounded execution -> retry/pause/kill
        |
        +--> Product Golden Path owners
        |       worktree -> verification -> artifact -> approval -> output
        |
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

The following authorities remain separate:

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

The canonical product flow is:

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

Missing, stale, conflicting, duplicate, late, revoked, expired, paused, killed, over-budget, lost-lease, or outcome-unknown state fails closed.

Fixture completion proves wiring only. It is not managed acceptance, live RWE, or product capability proof.

## Managed Process Boundary

The accepted managed-process owner provides:

- exact executable path/version/SHA validation;
- cleared/minimal environment and closed stdin where required;
- bounded stdout, stderr, combined output, and wall time;
- Unix process-group/descendant cleanup where proved;
- typed spawn, wait, timeout, reader, output-limit, signal, and cleanup outcomes;
- non-retryable handling after an effect may have begun;
- no partial-output success on bounded-failure paths.

There is no universal, cross-executor sandbox that can be treated as a complete security boundary.

Codex has executor-specific bubblewrap filesystem mediation and optional user/PID namespaces when the host supports them. This does not establish full admission: internal retry identity remains wire-unproved, product-enforced loopback-only network confinement remains unproved under the current unprivileged profile, and host namespace capability may fail closed.

Claude Code remains fail-closed because provider-independent worktree-only confinement is not proved. OpenCode real-binary admission remains deferred.

## Codex Mediation and Budget

Accepted main includes a Rust-owned loopback `CodexBudgetGateway`, parent-held upstream credential, parent-owned fail-closed usage journal outside the child sandbox, exact provider identity binding, and gateway-to-`execution_usage_event.v1` evidence mapping.

The child receives no reusable upstream credential. ProductTask remains the sole budget owner. Gateway measurement is primary; Codex JSONL/session records are corroborating post-call evidence.

Current class: `mediation_hardened_partial`.

Residual risks:

1. no trustworthy wire identity for Codex internal retries;
2. no proved product-enforced loopback-only network confinement under the current host profile;
3. host-dependent user/PID namespace support;
4. live operation requires separately accepted operator risk and spend authority.

A bounded live trial under partial mediation requires an explicit authority decision. It must not be described as full admission.

## Usage and Cost Evidence

`execution_usage_event.v1` is the normalized post-call evidence contract across admitted provider/gateway and local executor sources.

Canonical token buckets must be non-overlapping:

```text
fresh_input
cache_read
cache_creation
non_reasoning_output
reasoning_output
```

When a provider reports totals that include cache or reasoning sub-buckets, adapters canonicalize before writing the normalized event. Ambiguous semantics are marked partial/ambiguous rather than guessed.

Provider/request identities must come from trustworthy owner or provider evidence. An execution ID must not be reused as a pretend per-request identity when multiple provider rounds are possible.

Cost semantics are explicit:

- `provider_reported` only from trustworthy provider/gateway monetary semantics;
- `local_estimate` only from a versioned, provider/model-bound pricing table with estimate labeling;
- `cost_unavailable` when neither is trustworthy.

Unknown price is unavailable, not zero. Local estimates never become billing receipts or pre-call spend authority unless a separately reviewed gateway contract actually enforces them.

## Evidence and Efficiency

The repository stores bounded, redacted, hash-bound evidence through existing owners. Derived metrics are recomputed from trusted counters and are never accepted from caller input.

Runtime evidence includes quality/pass status, tokens, context/repetition, tool calls, retries, latency, cost source, recovery, approval/output, and terminal bindings.

Engineering/lifecycle-cost evidence begins as a bounded `implementation_cost_receipt` in each board report, recording when available:

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

- SQLite is default and uses the existing app-managed connection, WAL, transactional writes, integrity checks, and backup/restore owners.
- PostgreSQL is optional and must preserve equivalent state, validation, locking, idempotency, audit, and rollback behavior.
- Schema migrations are additive unless a separately reviewed destructive rollback is explicitly authorized.
- Restart, concurrency, exact replay, conflicting reuse, late-write refusal, and outcome-unknown semantics must be tested on both backends where the affected state is persisted.

Accepted `main` is merged through product terminal evidence schema v31. Higher managed-acceptance/RWE schema versions proposed on open PR branches are not current architecture until their final heads are accepted and merged.

## Real Workload Evidence

The first RWE baseline is the prerequisite for Architecture Convergence.

A valid corpus is:

- real, versioned, hash-bound, and replayable;
- fixed before convergence;
- bound to exact task definitions/references, source repository/commit or fixture tree, mutable paths, verification, expected class, output bounds, executor identity, budget, timeout/cancel behavior, and cleanup;
- executed under a separate one-use RWE spend authorization;
- incapable of labeling fixture execution as a live baseline.

The baseline records product quality and runtime evidence plus the implementation-cost receipt for producing it.

## Architecture Convergence

Architecture Convergence is incremental compatibility work, not a rewrite:

1. AC1 — unified `ProcessSupervisor`.
2. AC2 — typed execution boundary with executor adapters.
3. AC3 — Golden Path responsibility split.
4. AC4 — transaction-scoped domain views over the existing store.
5. AC5 — explicit runtime composition root.
6. AC6 — Rust-authoritative API/SDK/Dashboard schema convergence.
7. AC7 — obsolete abstraction cleanup only after all callers and evidence migrate.

Each packet changes one coherent ownership boundary, preserves compatibility, runs focused/full tests, and records implementation cost. The identical frozen RWE corpus is replayed after AC1–AC7.

## Harness Evolution

Level-1 is a default-off one-generation laboratory with immutable active-Harness identity, candidate lineage, equal-budget evaluation, hard gates, sealed holdout, Pareto archive, operator acknowledgement, and PR_READY output. It stops before production adoption.

Level-2 is eligible only after an evidence-backed GO decision using Golden Path stability, pre/post-convergence RWE, contamination risk, lifecycle cost, implementation feasibility, and existing Level-1 composition.

Even on GO, Level-2 remains bounded:

- small fixed generation/candidate/evaluation limits;
- deterministic global request/token/time/workspace/artifact/concurrency budgets;
- one selected laboratory parent per generation;
- restart, lease, concurrent selection, stale-parent, duplicate, tamper, stagnation, regression, pause/kill, and SQLite/PostgreSQL parity evidence;
- no automatic `main` mutation, merge, deploy, evaluator rewrite, authority expansion, or production-Harness adoption.

The Meta Improver is later and separately authorized. It requires unseen tasks, immutable evaluator/labels, contamination controls, baselines, statistical thresholds, seeds, budgets, and stop/rollback rules. A NO-GO result is valid completion.

## External Adapter Boundary

External runtimes and projects may provide bounded parsers, adapters, protocol compatibility, or comparison evidence. They must not become required core dependencies or replacement authorities.

For example, CC Switch may be used as an MIT-licensed implementation reference for usage parsing, stream aggregation, model normalization, endpoint recognition, and pricing estimates. Its OAuth/account switching, credential persistence, automatic failover/retries, desktop UI authority, proxy database, and configuration ownership are outside this architecture.

Every adapted component records exact upstream commit, source mapping, license/attribution, semantic differences, and tests proving that core authority remains unchanged.

## Safety and Non-Claims

The repository does not currently claim:

- full Codex admission;
- managed Claude or OpenCode admission;
- accepted live Golden Path completion;
- accepted live RWE baseline;
- completed Architecture Convergence;
- automatic multi-generation evolution;
- demonstrated continuous learning or recursive self-improvement;
- production self-update;
- autonomous merge, release, or deployment.

Those claims require the evidence and gates in `docs/NEXT_DECISION.md`.

## Document Roles

- `ARCHITECTURE_BOOK.md` — durable mission, owners, boundaries, and invariants.
- `CURRENT_STATUS.md` — accepted main truth, open review surfaces, and current blockers.
- `NEXT_DECISION.md` — execution order, entry/exit evidence, and immediate next action.
- `MODULE_MAP.md` — concrete canonical owners and proposed-but-unmerged surfaces.
- `REAL_WORLD_TESTING_PLAYBOOK.md` / `RUNBOOK.md` — operational validation and procedures.

Prefer updating these active documents over adding parallel strategy, status, or policy files.
