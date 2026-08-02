# Architecture Book

Last updated: 2026-08-02.

Current version: v36.

This is the durable architecture and safety baseline for the Token-Efficient Agent Harness Lab. Current facts live in `docs/CURRENT_STATUS.md`; routing and gates live in `docs/NEXT_DECISION.md`; concrete owners live in `docs/MODULE_MAP.md`. Historical packet details remain available in git history.

The schema carries **v32** hash-linked decision-transition receipts, **v33** managed-acceptance spend/lease logical authorization, **v34** RWE authority rows, **v35** ProductTask workspace-preparation receipts, and **v36** immutable proposal/final-manifest delegation state plus the durable managed-provider request journal. The repository-agent two-loop control-plane seam described below adds no runtime schema or persisted product state. v36 reuses `LocalProductStore`, ProductTask budget, attempt, approval, output, audit, and rollback owners; it does not create another scheduler, runtime, store, budget, workspace, evaluator, or target-output owner.

## Mission

The system is a local/small-team self-hosted control plane for auditable coding-agent workflows. It may create bounded patches and Draft PRs for real repositories, but it is not a cloud SaaS, a direct-deploy tool, or an autonomous production operator.

Its single first-order objective is:

> Under non-negotiable quality, safety, traceability, compatibility, and rollback constraints, continuously increase verifiable and reusable task delivery per unit of total lifecycle cost.

The system does not optimize token count in isolation. A lower-token result is not better unless it meets the same accepted quality, safety, and integrity gates.

## Repository Agent Loop Control Plane

Repository automation separates two loops:

```text
execution inner loop: model -> tool -> observation -> next step
engineering outer loop: task state -> isolated execution -> verification -> repair -> review -> terminal state
```

The inner loop is disposable computation. A local Codex/OpenCode/Claude session may operate only inside its admitted worktree and bounded task packet; its conversation memory and self-reported completion are never durable authority.

The outer loop persists coarse state in GitHub Issue labels, typed/hash-bound events and receipts in Issue or PR comments, candidate changes in Draft PRs and exact commit SHAs, and machine verification in exact-head Actions checks/artifacts. Canonical documents own accepted direction and facts. Local worktrees, processes, caches, checkpoints, and journals are rebuildable projections, never the unique source of truth.

For a public repository, the preferred host interaction is an outbound local worker. One bounded `loopctl poll` may admit a capacity-bounded batch whose declared paths do not overlap active or selected tasks; independent `run-once` processes then claim one task and start one fresh isolated session each. A stateless supervisor may run those processes concurrently, but GitHub remains the queue and lease owner. Repository text and model output are untrusted data: the controller executes only repository-owned commands and never forwards GitHub/API credentials into the model child. Repository active capacity is fixed at canonical K=2, owned solely by `scripts/agent-control/state_manager.py` (`MAX_ACTIVE`); `loopctl poll --max-active` may only throttle that ceiling downward to 1..K.

The local adapter must reuse the existing `state_manager`, dispatcher, worktree, prompt, artifact, PR-binding, CI, review, and merge owners. It may not form a second controller or output authority. Before the self-hosted workflow path can be retired, `run-once` must prove remote serialization/lease recovery, exact accepted-main binding, one worktree per task, process-tree timeout/cancellation, validated patch artifact finalization, Draft-PR-only output, CI/review handoff, bounded repair, and safe restart after every externally visible transition.

## Decision Model

Architecture and evolution decisions use two ordered layers:

1. **Hard gates** — correctness, safety, authority, evidence integrity, compatibility, rollback, atomicity, restart, concurrency, and contamination controls.
2. **Optimization evidence after hard gates pass** — accepted delivery, reliability, token/request use, latency, cost semantics, implementation effort, maintenance surface, migration risk, failure recovery, observed reuse, and realistic implementation feasibility.

A failed hard gate is `INELIGIBLE`; it is not a low but otherwise acceptable economic score. Multi-objective Pareto comparison is the primary selection mechanism when dimensions conflict. A scalar may summarize an already eligible comparison for human display, but it may not grant execution, adoption, merge, release, deployment, or evaluator authority.

The repository calls this evidence model **Verified Delivery Economics (VDE)**. VDE is a read-only projection over existing Golden Path, usage, verification, artifact, approval, output, terminal-evidence, RWE, replay, and implementation-cost owners. It is not a second evaluator, store, budget, authority, adoption controller, or source of truth.

Success remains layered rather than collapsed:

```text
verified_success
= functional verification
  ∧ regression verification
  ∧ scope/mutable-surface compliance
  ∧ evidence completeness

maintainer_accepted_success
= verified_success
  ∧ versioned reviewer policy accepts without material rework

delivered_success
= maintainer_accepted_success
  ∧ approval/output confirmation
  ∧ bounded Draft-PR or export result
  ∧ cleanup and terminal evidence
```

Economic comparison uses `delivered_success` unless a versioned comparison contract explicitly names a lower layer. All three layers remain observable so test success cannot hide review rejection, output failure, or incomplete cleanup.

Task value uses one typed primary basis per comparison group:

```text
observed_market_value_usd
| human_equivalent_cost_usd
| human_expert_minutes
| domain_verified_delivery_units
```

Different value bases are not implicitly composable. Cross-unit aggregation requires a separately reviewed, versioned conversion contract; otherwise results remain separated or Pareto-compared. Unknown value or cost is unavailable, never zero.

Lifecycle evidence separates realized facts from forecasts:

```text
realized_lifecycle_cost
forecast_lifecycle_cost
observed_reuse_count
expected_reuse_scenario
```

Forecast maintenance, future reuse, or amortization may appear only as labeled low/base/high scenarios. It cannot establish an accepted improvement claim until observed under the pre-registered measurement window.

The general Lifecycle Cost of Accepted Pass is:

```text
LCAP
= expected cumulative realized lifecycle cost
  until first delivered_success or the frozen budget/stop rule terminates
```

`mean_attempt_cost / P(delivered_success)` is permitted only as an explicitly labeled simplification when attempts are independent, identically distributed, non-contaminating, fixed-cost, and governed by an unchanged retry policy. Adaptive repair, heterogeneous failure classes, restart, cancellation, and recovery require trajectory-level cumulative-cost estimation.

VDE outputs may include hard-gate status, accepted-success observations and intervals, LCAP, human-relative saving, comparable verified-delivery efficiency, net verified value when both sides have trustworthy monetary semantics, and a lifecycle-cost Pareto frontier. A baseline-relative `VDE Index` is display-only and must disclose corpus, baseline, value basis, cost completeness, evidence-sufficiency state, and uncertainty.

This is a decision and evidence principle, not caller-supplied production authority.

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

Every stage binds to exact current identities: tenant/workspace, ProductTask version, plan/run/node attempt, lease/owner token, executable/provider/model, budget, worktree/source revision/tree, allowed mutable paths, verification result, artifact, approval, output operation/receipt, and audit. A ProductTask status/version transition and its transition-audit record commit atomically in each supported storage backend.

Missing, stale, conflicting, duplicate, late, revoked, expired, paused, killed, over-budget, lost-lease, or outcome-unknown state fails closed. Fixture completion proves wiring only; it is not managed acceptance, live RWE, or product capability proof.

The accepted v35 change makes physical worktree preparation explicitly recoverable without creating another authority owner. Fresh intake completes read-only target/root/overlap checks, then atomically publishes `admitted → workspace_preparing`, the transition audit, and one ProductTask-owned receipt. The receipt pins the canonical configured local root, deterministic workspace path, marker hash/state, and receipt hash before any root creation, marker, lock, Git, or supervised-workspace effect. Only that receipt may authorize subsequent root creation, marker validation, physical preparation, or compensation.

The local file-descriptor lock protects one app-owned Git worktree path; PostgreSQL additionally supplies a try-only, session-scoped advisory coordinator for active ProductTask preparation. Those guards are synchronization only: they are not a durable lease, distributed fence, cross-host filesystem-visibility proof, or a second ProductTask/budget/rollback owner. A changed configured root, receipt/marker identity failure, unavailable guard, or failed contention audit is `reconciliation`—no cleanup, terminalization, or alternate-root worktree creation is allowed while the physical outcome is unproved. An observed contention audit contains only the ProductTask identity and redacted synchronization facts, never a path, command, prompt, or output.

The accepted v35 receipt retires only after a durable `workspace_bound` result or compensation that proves the exact Git registration and workspace path are absent; idempotent admission/recovery reaps crash-left retirement residue conservatively. v35 rollback is permitted only after its receipt table is drained, with the rollback audit committed atomically and matching SQLite/PostgreSQL semantics.

### Authorized managed-coding generalization

The managed-coding product boundary is executor-generic, even where accepted code still contains Codex-specific adapters. Its versioned, Rust-owned runtime profile binds executor kind, protocol kind, exact executable identity when one exists, required capability probes, requested and resolved model, thinking configuration, provider identity, symbolic credential reference, endpoint allowlist, usage-parser version, pricing source/version, and admission classification. A profile may express a compatible release range or an explicit list, but terminal evidence always records the actual canonical path, observed version, SHA-256, capability results, and profile hash. A process name, configured path, or caller assertion alone never admits execution.

At admission and immediately before spawn, binary-backed executors must reject symlinks, non-regular/missing/non-executable files, path drift, identity/profile mutation, and missing or changed required capabilities. These checks add no second lease, budget, credential, store, or process owner. Existing Codex records retain their historical schema values through explicit compatibility adapters; historical evidence is not rewritten merely to generalize a name.

Product source is either `git_repository` or `local_folder`. A Git source binds remote/repository/default-branch SHA, an app-owned detached worktree, bounded mutable paths, and unchanged target main. A local-folder source binds an operator-selected absolute canonical root and safe exact manifest/tree hash, runs in an app-owned staging copy by default, excludes configured secret/private paths, and exposes only bounded relative paths/fingerprints outside the internal workspace owner. It refuses symlink escapes, traversal, devices, sockets, and unsafe special files. Original-source revalidation is mandatory before any output.

Local-folder `artifact_only`, bounded export, and `apply_local_changes` are separate output modes. Applying needs fresh preimage hashes, bounded atomic replacement where available, an app-owned rollback bundle created before mutation, refusal of stale/duplicate/late/cancelled/outcome-unknown/out-of-scope effects, and cleanup/rollback receipts. Separate current approval and output confirmation remain mandatory; direct in-place execution is not permitted.

## Managed Process Boundary

The accepted managed-process owner provides exact executable identity, cleared/minimal environment, bounded output and time, descendant cleanup where proved, typed process outcomes, and non-retryable handling after an effect may have begun. Managed coding profiles extend this owner rather than create an executor-specific process supervisor.

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

### Pre-child owner-derived preflight

The sole store-issued managed-Codex spawn lease atomically consumes its one-use spend before a gateway can start. After that consumption but before any child spawn, the runtime re-reads the current lease/spend/decision/risk/ProductTask owners, derives the actual launcher/gateway/journal attestation, and runs the lease-bound owner-derived preflight. It is an additional fail-closed admission check, not a second budget, lease, credential, evidence, or authorization owner. Preflight never executes the admitted binary outside the mediated child boundary: it compares the current re-hashed binary identity with the canonical spend and, for a runtime lease, its immutable launch facts; the final store confirmation revalidates before child spawn.

The active-unconsumed-spend inspection remains provider-free and cannot create a lease, start a child, call a provider, or establish live-task authority. A `draft_pending_operator` partial-mediation report remains blocked; only a fresh store-derived `operator_accepted` decision can let its bounded residual trial pass this runtime gate, and it is never labeled full admission. A pre-child failure has no forwarded provider request; the runtime shuts down a started temporary gateway and removes its ephemeral home and parent-owned journal before terminalizing the already-consumed lease as failed. The same artifact cleanup applies when gateway startup fails after journal setup but before a gateway is returned. If cleanup cannot establish that those temporary artifacts are gone, terminal failure reports a bounded `pre-child cleanup incomplete` state rather than claiming clean removal. These mechanics do not satisfy the external Golden Path authorization gate or remove the stated mediation residuals.

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

### Managed provider calls

Protocol adapters are evidence and transport adapters beneath one ProductTask-owned managed provider-call authority. Every send binds the current ProductTask/workflow/node/attempt, lease, one-use spend state, model role, exact provider/protocol/host/base URL/path, requested and resolved model, symbolic credential reference, and hard request/retry/input/output/cumulative-token/time/cost limits. The parent Harness resolves credentials only at the send boundary; raw values do not enter tool subprocesses, model-created commands, persistence, durable evidence, or public projections.

The initial admitted DeepSeek profiles are exact: `deepseek-v4-flash` and `deepseek-v4-pro`; OpenAI-compatible Chat Completions at `https://api.deepseek.com/chat/completions`; and Anthropic-compatible Messages at `https://api.deepseek.com/anthropic/v1/messages`. Unsupported-name fallback or alias mapping is not resolved-model evidence. Missing, ambiguous, conflicting, or untrusted returned model/usage evidence is fail-closed for an admitted class. Both protocols share the same ProductTask spend envelope; planner/implementer/reviewer sublimits never become spend owners.

The parent-owned journal conservatively reserves before send, retains consumption after failed, killed, cancelled, timed-out, or outcome-unknown effects, and never retries an outcome-unknown request. Protocol-specific parsers normalize stream/non-stream text and tool semantics, request identity, stop status, cache and reasoning buckets, and usage into `execution_usage_event.v1`; they never substitute a second journal or budget owner. A dollar-denominated live gate requires current, versioned, source-labeled conservative pricing. Token-only fixture limits remain allowed, but unknown/stale price is unavailable rather than zero.

### Delegated autonomous Golden Path

A proposal manifest is immutable after persistence. Execution derives a separate final manifest containing the non-null cost cap and exact target SHA, mutable paths, verifier, provider, protocol, role/model, request, usage, retry, recovery, and output limits before canonical hashing. Approval is a separate hash-bound receipt; changing any execution fact requires a new final-manifest hash. A one-use spend authorization and attempt lease then bind that exact hash.

Delegation is versioned, hash-bound, revocable, expiring, replay-protected, and cumulatively budgeted. The delegated manifest/spend approver may approve only a final manifest entirely inside a current authenticated operator delegation. The independently separated artifact/output confirmer rechecks the current delegation, target SHA, exact artifact/diff, paths, verifier result, reviewer result, realized cost, and output restrictions before authorizing one unmerged `acp/*` Draft PR. Execution and model outputs cannot issue either receipt, and no component may execute and approve the same attempt.

The production DeepSeek route is one ProductTask graph: Pro planning, Flash typed bounded workspace actions, deterministic verification, then Pro review. The deterministic verifier—not a model—decides verification success. Each provider request is claimed in the existing store-owned journal before send and reconciled with exact model/protocol/request/usage identity afterward. A crash-left `sending` or outcome-unknown record is permanently non-retryable across restart. Success reconciles actual usage/cost; failure conservatively retains reservation when actual usage is unknown. Every terminal path performs bounded cleanup, expires spend and delegation, closes the attempt lease, and persists rollback/terminal evidence with SQLite/PostgreSQL parity.

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
cost_or_measurement_unavailable_fields
```

The accepted economic projection distinguishes observed evidence from forecasts. Failed, cancelled, timed-out, killed, recovered, and outcome-unknown attempts retain their consumed cost; successful-run-only costing is forbidden. `expected_reuse_count` remains a forecast input until reuse is observed. Reviewer time, material rework, operator interruptions, recovery work, and CI effort are part of lifecycle cost when measured; unavailable fields remain explicitly unavailable.

Evidence sufficiency is stateful rather than falsely precise:

```text
INSUFFICIENT_REPETITIONS
POINT_ESTIMATE_ONLY
INTERVAL_AVAILABLE
COMPARISON_ELIGIBLE
```

A single live Golden Path sample may prove evidence wiring and realized cost capture, but cannot establish a stable success probability, VDE improvement, ROI, or Level-2 decision. Repeated comparisons freeze corpus, source, verifier, reviewer policy, budget grid, seeds, stop rules, and statistical method before results are observed. Reviewer identity class, rubric, blinding, permitted repair, disagreement resolution, and review-time measurement are versioned parts of that protocol.

Initial persistence is artifact-first and reuses existing evidence/artifact owners. Candidate contracts include:

```text
task_value_profile.v1
implementation_cost_receipt.v1
verified_delivery_observation.v1
verified_delivery_comparison.v1
```

They are immutable/hash-bound evidence projections, not new authority. A database query table or automated adoption consumer requires a later reviewed design after live evidence proves the schemas stable. The current Level-1 `MetricVector` is not expanded merely to host VDE; a later Level-2 decision may map validated artifact fields into versioned Pareto objectives without replacing the existing evaluator owner.

This evidence informs RWE replay and Level-2 decisions. It does not create a second runtime budget, scheduler, store, evaluator, or production-adoption path.

## Storage

`LocalProductStore` is the sole application-owned persistence and transaction boundary.

- SQLite is default and uses existing transactional, integrity, backup, and restore owners.
- PostgreSQL must preserve equivalent validation, locking, idempotency, audit, restart, concurrency, and rollback behavior.
- Schema migrations are additive unless separately reviewed destructive rollback is explicitly authorized.
- The accepted-main schema marker changes only after independent review and merge; branch-local migration candidates are never accepted truth.

## Real Workload Evidence

The first RWE baseline is the prerequisite for Architecture Convergence.

A valid corpus is real, versioned, hash-bound, replayable, fixed before convergence, bound to exact task/source/mutable-surface/verification/output/executor/budget identities, executed under separate one-use RWE spend authority, and incapable of labeling fixture execution as a live baseline.

Before the first economic comparison, the frozen corpus also binds each task's primary value basis, value source/confidence, acceptance rubric, reviewer policy, minimum repetitions, budget points, stop rules, non-inferiority margins, and cost-completeness requirements. Fixture corpora continue to prove authority and persistence only; they cannot establish task value or an economic baseline.

The baseline records layered success, failure class, runtime evidence, recovery behavior, approval/output/terminal bindings, realized lifecycle cost, evidence-sufficiency state, and the implementation-cost receipt. Production task-mix value may later be observed separately, but it cannot replace the frozen-corpus comparison used for Architecture Convergence and Level-2 decisions.

## Architecture Convergence

Architecture Convergence is incremental compatibility work, not a rewrite:

1. AC1 unified `ProcessSupervisor`.
2. AC2 typed execution boundary.
3. AC3 Golden Path responsibility split.
4. AC4 transaction-scoped domain views.
5. AC5 explicit runtime composition root.
6. AC6 Rust-authoritative API/SDK/Dashboard schema convergence.
7. AC7 obsolete-abstraction cleanup after all callers and evidence migrate.

Each packet changes one coherent ownership boundary, preserves compatibility and rollback, and records implementation cost. The identical frozen RWE corpus and pre-registered economic protocol are replayed after AC1–AC7.

## Harness Evolution

Level-1 is a default-off one-generation laboratory with immutable active-Harness identity, candidate lineage, equal-budget evaluation, hard gates, sealed holdout, Pareto archive, operator acknowledgement, and PR_READY output. It stops before production adoption. VDE does not rewrite or silently broaden its current evaluator or `MetricVector`.

Level-2 is eligible only after an evidence-backed GO decision using Golden Path stability, pre/post-convergence identical-corpus RWE, contamination risk, layered accepted-success reliability, realized lifecycle cost, review/rework/recovery burden, maintenance surface, implementation feasibility, and existing Level-1 composition.

A Level-2 GO requires every hard gate to pass, pre-registered quality and reliability non-inferiority, an eligible comparable value basis, uncertainty-aware improvement evidence, and no unacceptable authority, review, recovery, maintenance, or rollback regression. Pareto evidence precedes any scalar summary.

Even on GO, Level-2 remains bounded and may not modify `main`, merge, deploy, rewrite its evaluator, expand its authority, or adopt a production Harness automatically.

The Meta Improver is later and separately authorized. It requires unseen tasks, immutable evaluator/labels, contamination controls, baselines, statistical thresholds, seeds, budgets, and stop/rollback rules. A NO-GO result is valid completion.

## External Adapter Boundary

External projects may provide bounded parsers, adapters, protocol compatibility, or comparison evidence. They must not become required core dependencies or replacement authorities.

CC Switch may be used as an MIT-licensed implementation reference for usage parsing, stream aggregation, model normalization, endpoint recognition, and pricing estimates. Its OAuth/account switching, credential persistence, automatic failover/retries, desktop UI authority, proxy database, and configuration ownership are outside this architecture.

Every adaptation records exact upstream commit, source mapping, license/attribution, semantic differences, and tests proving that core authority remains unchanged.

## Dashboard Boundary

The Dashboard and SDKs project accepted Rust-owned schemas and controls. They may display status, evidence, budgets, approvals, output operations, lifecycle-cost summaries, VDE evidence-sufficiency states, and baseline-relative indices, but they do not become workflow, evaluator, spend, approval, adoption, output, merge, release, or deployment authorities. A scalar VDE index must remain expandable to corpus, task, run, value basis, cost source, reviewer policy, failure class, and uncertainty. Dashboard PR #225 remains presentation-only and last.

## Safety and Non-Claims

The repository does not currently claim full Codex admission, managed Claude/OpenCode admission, accepted live Golden Path completion, accepted live RWE, stable accepted-success probability, realized VDE improvement, completed Architecture Convergence, automatic multi-generation evolution, demonstrated continuous learning, production self-update, or autonomous merge/release/deployment.

Those claims require the evidence and gates in `docs/NEXT_DECISION.md`.

## Document Roles

- `ARCHITECTURE_BOOK.md` — durable mission, owners, boundaries, and invariants.
- `CURRENT_STATUS.md` — accepted main truth, open review surfaces, and blockers.
- `NEXT_DECISION.md` — execution order, entry/exit evidence, and immediate next action.
- `MODULE_MAP.md` — canonical owners and proposed-but-unmerged surfaces.
- `REAL_WORLD_TESTING_PLAYBOOK.md` / `RUNBOOK.md` — operational validation and procedures.

Prefer updating these active documents over adding parallel strategy, status, or policy files.
