# Architecture Book

Last updated: 2026-08-28.

Current version: v38.

This is the durable architecture and safety baseline for the Token-Efficient Agent Harness Lab. Accepted facts live in `docs/CURRENT_STATUS.md`; the current executable window lives in `docs/NEXT_DECISION.md`; routing-only successors live in `docs/FUTURE_ROUTE.md`; concrete owners live in `docs/MODULE_MAP.md`. Historical packet details remain available in git history.

The schema carries **v32** hash-linked decision-transition receipts, **v33** managed-acceptance spend/lease logical authorization, **v34** RWE authority rows, **v35** ProductTask workspace-preparation receipts, **v36** immutable proposal/final-manifest delegation state plus the durable managed-provider request journal, **v37** evaluator-owned immutable EC2 prediction outcomes, and **v38** immutable EC3 lifecycle-cost observations. The repository-agent two-loop control-plane seam described below adds no runtime schema or persisted product state. v38 reuses `LocalProductStore`, ProductTask budget, attempt, approval, output, audit, and rollback owners; it does not create another scheduler, runtime, store, budget, workspace, evaluator, or target-output owner.

## Mission

The system is a local/small-team self-hosted control plane for auditable coding-agent workflows. It may create bounded patches and Draft PRs for real repositories, but it is not a cloud SaaS, a direct-deploy tool, or an autonomous production operator.

Its single first-order objective is:

> Under non-negotiable quality, safety, traceability, compatibility, and rollback constraints, continuously increase verifiable and reusable task delivery per unit of total lifecycle cost.

The system does not optimize token count in isolation. A lower-token result is not better unless it meets the same accepted quality, safety, and integrity gates.

### Autonomous Steward migration contract (PR1)

The provider-free migration contract is owned by
`scripts/agent-control/mission_contract.py`. `MaintenanceMission` binds one
canonical proposal payload to an owner approval digest, exact repository/source
identity, explicit scope and change categories, finite attempts/time/calls/cost
budgets, quality checks, stop taxonomy, and a tested rollback boundary. `Stage`
binds one verifiable integration result to the same repository identity and its
WorkCard graph. `WorkCard` narrows paths, steps, focused and negative checks,
evidence, dependencies, path locks, model tier, attempts, result state, and
rollback; it cannot widen Mission scope or budget.

The v1 boundary is fail-closed and provider-free: malformed, stale,
unauthorized, out-of-scope, unbounded, sensitive, destructive, or
`OUTCOME_UNKNOWN` inputs are rejected. Routine worker/test/CI/review failure
and main drift remain bounded recovery categories; scope, authority,
requirement, safety, and unknown external outcomes are owner-pause categories.
The wire `owner_identity` is only an approval claim: current-Mission
validation requires an existing authority owner to supply the trusted owner
identity allowlist, and the registered campaign is checked against its fixed
repository, branch, source reference, source digest, and PR0 base. Provider-free
PR1 WorkCards use T0-T2; T3 or any external effect requires a later authority
envelope and is rejected here.
The contract is a read-only compatibility reader during migration. The legacy
packet controller at `scripts/agent-control/local_loop.py` remains the sole
lifecycle writer and sole durable queue/lease path; the projection cannot
consume authority, write state, call a Provider, or create a second store,
scheduler, evaluator, budget, output, audit, rollback, or workspace owner.

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

An exact, claim-bound `block-plan` request may terminalize an expired plan lease only as `failed_unknown_output`; it frees capacity without treating the old worker output or handoff as proven. That immutable terminal record does not shadow a later fresh claim, while an unterminalized `outcome_unknown` remains fail-closed. Successful release and all other claim transitions still require a live lease.

### Repository-maintenance route transition

The repository-maintenance route controller is an extension of the existing
engineering outer loop, not a second orchestrator. Its durable queue and lease
remain GitHub Issue/PR state; accepted direction remains `NEXT_DECISION`,
routing-only successors remain `FUTURE_ROUTE`, and `CURRENT_STATUS` remains the
accepted receipt owner. `local_loop.py` remains the controller entrypoint;
`loopctl.py` is only its CLI transport. `state_manager.py` remains the sole
GitHub state/lease owner, `plan_lane.py` remains the packet compiler, and the
existing worktree, artifact, CI, review, merge, and closeout owners are reused.

Every route transition is bound to accepted-main SHA, packet digest, dispatch
digest, subject identity, branch, PR, exact head, CI run/check set, review
receipt, merge commit, and closeout receipt. A changed head invalidates CI and
review; main drift forces reconciliation; an unavailable or conflicting
external observation is `OUTCOME_UNKNOWN` or `DECISION_REQUIRED`, never a
successful self-report. A bounded autonomous worker is autonomous packet-internal
execution under a fixed authority/scope envelope; this describes its authority,
not model capability. Luna, Codex, Claude, OpenCode, or another model may fill
the role when it satisfies the same contract. It may produce only an untrusted
patch or proposal. It receives one compiled packet capsule and no GitHub merge token,
provider credential, T3 authority, or arbitrary command execution capability.

An EFFECT T3 request prepared in a promotion PR binds its source accepted-main
SHA and may be consumed after merge only when that source is a proven ancestor
of the current accepted main and the current route block still carries the same
request digest. This bridge accounts for the merge commit without widening the
finite action, scope, or operator receipt.

The route controller may request a repository-maintenance merge only through
the existing exact-head merge owner after canonical CI, independent exact
`PASS`, no unresolved blocking objection, accepted-base freshness, and tested
rollback are revalidated. This transition cannot merge target repositories,
release, deploy, consume Provider authority, or adopt product state. T3 and
human decision receipts remain outside model delegation.

### Shared investigation escalation (ask_sol)

When an autonomous coding worker encounters high-value uncertainty, contradictory evidence, cross-module ambiguity, or repeated hypothesis failures, it may invoke the shared read-only investigation tool (`scripts/ask_sol.py` / `scripts/ask_sol`).

The capability operates under strict architectural invariants:
- **No Multi-Agent Orchestration / No Mandatory Lanes**: The ordinary worker remains the sole task owner, file editor, and executor. Sol is not a mandatory planning, implementation, or review lane.
- **Harness Neutrality**: Any local worker (Codex, OpenCode, Claude, Gemini, local model, or script) reaches the same capability through the shared CLI tool; no harness-specific consultation logic or provider authority is created.
- **Independent Verification**: Sol treats caller hypotheses as untrusted and investigates first-party repository evidence (code, tests, git history, diffs, schema, module maps) directly.
- **Strict Read-Only Enforcement**: Sol executes in a Codex read-only sandbox (`-s read-only --ephemeral`). The tool captures git HEAD SHA and dirty-state digest before and after consultation; any mutation fails closed with `MUTATION_DETECTED`. Uncommitted caller changes are preserved.
- **Loop Bounds and Recursion Rejection**: Sol cannot recursively invoke `ask_sol` (`ASK_SOL_ACTIVE=1` fails closed). Per-state consultation count is capped (default maximum 2) and resets only when git state advances.
- **Zero Credential Persistence**: Local authenticated Codex CLI session is reused directly; credentials, secret tokens, and raw transcripts are never persisted or serialized into results.
- **Structured Envelope**: Returns a canonical `ask_sol_result.v1` envelope containing status, finding, evidence locations, confidence, rejected alternatives, unresolved uncertainties, and recommended next action.

## Repository Context Control Plane

Repository handoff separates three identities:

```text
accepted truth and confirmed gaps  -> CURRENT_STATUS
one executable window              -> NEXT_DECISION
blocked long-horizon sketches      -> FUTURE_ROUTE
```

Live PR heads, CI conclusions, reviews, mergeability, and next-action sequencing are observations, not accepted document state. `scripts/project_context.py` combines canonical documents from accepted `main` with bounded read-only GitHub observations and reports unavailable or conflicting evidence fail-closed. A capsule is short-lived transport context; it never becomes a status database, packet owner, review judge, CI authority, or permission source.

`scripts/session_context.py` is the single arbitrary-session navigation and local recovery projection. Its accepted-main role router returns at most six default documents and its packet extractor returns one selected packet rather than the whole horizon. Its mode-0600 Git-private checkpoint binds packet digest, accepted main, branch, exact head, dirty paths, per-path content digests, declared owned paths, preserved foreign paths, and the exact verification contract of the accepted dispatch capsule. Checkpoint verification evidence is a local recovery projection only: a checkpoint whose evidence set, ordered verification contract, work-state/verification-state invariant, or checkout subjects were changed or rehashed is rejected by the resume classifier rather than resumed. The checkpoint grants no task, lease, provider, output, review, merge, or lifecycle authority. Exact match yields `RESUME`; bounded drift solely within bound task paths yields `REPAIR`; missing identity, changed preserved work, unknown paths, packet/main/branch/contract conflict, blocker, or outcome unknown yields `DECISION_REQUIRED`.

GitHub remains the sole durable queue, lease, and controlled-worker state owner. Accepted documents remain packet/direction owners, exact commits remain code owners, and CI/review receipts remain verification owners. The Git-private checkpoint exists only so a later conversation in the same worktree can detect WIP drift; it is replaceable, local-only, and never accepted evidence. Missing checkpoint on a non-canonical or dirty checkout fails closed rather than guessing from chat. A `STABLE` boundary is produced only by the fixed checkpoint command after every declared verification command actually passed with the checkout unchanged; a caller-asserted `PASS` or rehash cannot create one.

Exactly one blocked successor may be removed from `FUTURE_ROUTE` and expanded into `NEXT_DECISION` only after its accepted prerequisite and any negative/insufficient disposition are reconciled against current `main`. Its promotion profile row supplies bounded promotion-time candidates; facts marked `REFRESH_AT_PROMOTION` must be re-derived from then-current `main`. Duplicate packet identities, future-route activation, or profile-based execution are invalid handoff states.

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

Full Agent Autonomy Mode permits repository-scoped work that remains testable, observable, reviewable, verification-gated, compatible, and rollbackable. It does not grant provider spend, target output, runtime- or candidate-controlled merge, release, deployment, or production-adoption authority. Repository-maintainer commits and merges are governed separately and exclusively by `docs/REAL_WORLD_TESTING_PLAYBOOK.md`, including exact-head review, canonical CI, objection, recovery, and rollback gates.

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

The accepted AC3 Golden Path responsibility contract is owned by the `PE7-AC3-CONTRACT-1` section in `docs/CURRENT_STATUS.md`; later AC3 implementation must preserve that contract and the architecture invariants here.

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
- The accepted-main schema marker is v38 after the EC3 instrumentation merge; later enforcement remains a separate provider-free packet and does not change the schema marker by itself.

## Real Workload Evidence

The first small live RWE run is a lifecycle-viability gate, not by itself the decision baseline for Architecture Convergence. Architecture Convergence requires an accepted task-level measurement contract and a decision-grade pre-convergence baseline after viability is proved.

A valid corpus is real, versioned, hash-bound, replayable, fixed before convergence, bound to exact task/source/mutable-surface/verification/output/executor/budget identities, executed under separate one-use RWE spend authority, and incapable of labeling fixture execution as a live baseline.

Before the first economic comparison, the frozen corpus also binds each task's primary value basis, value source/confidence, acceptance rubric, reviewer policy, minimum repetitions, budget points, stop rules, non-inferiority margins, and cost-completeness requirements. Fixture corpora continue to prove authority and persistence only; they cannot establish task value or an economic baseline.

The viability run and decision baseline both record layered success, failure class, runtime evidence, recovery behavior, approval/output/terminal bindings, realized lifecycle cost, evidence-sufficiency state, and the implementation-cost receipt. Cross-task inference treats tasks or pre-registered task families as the independent unit; repetitions estimate within-task variability and do not create additional independent tasks. Production task-mix value may later be observed separately, but it cannot replace the frozen-corpus comparison used for Architecture Convergence and Level-2 decisions.

Architecture attribution does not rely on an unqualified historical before/after comparison. The pre-convergence Harness, configuration, dependencies, and environment remain reconstructable so accepted pre- and post-convergence Harnesses can be randomized/interleaved in one controlled contemporary replay window. Historical evidence remains valid incident, compatibility, and drift evidence.

High-decision-density work is separated into four durable execution classes: provider-free contract freeze, bounded implementation, separately authorized external effect, and evidence/decision closeout. A packet may not both choose an authority/statistical/evaluator/spend/retention contract and implement or evaluate that choice. A live experiment may not repair its own protocol, and the process that executes an effect may not silently upgrade the resulting claim. Adjacent provider-free work may share one change only when it has one owner, allowed-path set, semantic delta, rollback point, and independently reviewable stop boundary; external effects, human decisions, schema/authority changes, and cross-owner work remain separate.

## Architecture Convergence

Architecture Convergence is incremental compatibility work, not a rewrite:

1. AC0 production-call-site, owner, invariant, golden-trace, order, and rollback inventory/freeze; no ownership move.
2. AC1 unified `ProcessSupervisor`.
3. AC2 typed execution boundary.
4. AC3 Golden Path responsibility split.
5. AC4 transaction-scoped domain views.
6. AC5 explicit runtime composition root.
7. AC6 Rust-authoritative API/SDK/Dashboard schema convergence.
8. AC7 obsolete-abstraction cleanup after all callers and evidence migrate.

Each AC stage changes one coherent ownership boundary, preserves compatibility and rollback, and records implementation cost. AC0 separates runtime inventory, data/contract inventory, and trace/order freeze. AC1–AC6 each freeze a current-main contract before additive core work and enumerate caller/consumer migration separately; AC6 also separates Rust/codegen, SDK, and Dashboard consumer migration. AC7 freezes a zero-caller removal manifest before deletion-only work and independent closeout. A migration or cleanup packet cannot enlarge its preceding contract. After AC7, the frozen RWE corpus and protocol are replayed through reconstructable old/new Harnesses in the contemporary controlled comparison above.

### AC4: Transaction-Scoped Domain Views Contract

`PE7-AC4-CONTRACT-1` freezes the transaction view boundary, borrow rules, commit/rollback invariants, and backend parity before the views-core and caller-migration implementation packets. The contract is strictly provider-free, introduces zero schema alterations or migrations, and creates no second store or runtime authority.

The transaction boundary rules are immutable:

1. **Sole Transaction Authority**: `LocalProductStore` is the sole persistence authority and the only component authorized to open, manage, commit, or abort database transactions.
2. **Borrowed View Lifetimes**: Transaction views (`WorkflowTx`, `ProductTaskTx`, `ManagedAcceptanceTx`, `RweTx`) are ephemeral wrappers borrowing the active database transaction (`&rusqlite::Transaction<'_>` on SQLite, `&mut postgres::Transaction<'_>` on PostgreSQL). Views cannot outlive the enclosing store transaction scope, hold connection pool handles, or open independent connections.
3. **Forbidden Nested Commits**: Transaction views expose only domain-specific mutation methods; they possess no `commit()` or `rollback()` operations.
4. **Single Atomic Commit Boundary**: Cross-domain operations execute beneath one unified store transaction. Commits occur exactly once when all operations succeed (`Ok(())`); any error (`Err(_)`) triggers an immediate full rollback of the entire atomic unit.
5. **Backend Parity**: SQLite transactions run under `BEGIN IMMEDIATE` guarded by the store mutex; PostgreSQL transactions execute via `client.transaction()` with deterministic row-level `FOR UPDATE` locking order to prevent deadlocks and guarantee parity across storage backends.

The four cross-domain transaction view groups:

| Transaction View | Touched Relational Tables | Core Operations & Invariants |
|---|---|---|
| `WorkflowTx` | `workflow_plans`, `workflow_runs`, `workflow_run_nodes`, `workflow_run_approvals`, `audit_log` | Monotonic plan and run creation, exclusive node leasing with atomic status CAS, node execution outcome recording, DAG transition evaluation, approval unpause, and structured audit trail emission. |
| `ProductTaskTx` | `product_tasks`, `supervised_patch_workspaces`, `supervised_patch_artifacts`, `audit_log` | Idempotent task admission, optimistic concurrency control via `version` CAS, atomic verification artifact capture coupling workspace status transition (`patch_prepared`), task state transition (`awaiting_approval`), and audit logging. |
| `ManagedAcceptanceTx` | `managed_acceptance_delegations`, `managed_acceptance_spend_authorizations`, `audit_log` | One-use spend authorization issuance (`executions_used: 0 -> 1`), attempt admission with lease issuance, immutable proposal and manifest approval recording, artifact output confirmation, and terminal spend reconciliation. |
| `RweTx` | `rwe_run_authorizations`, `rwe_cells`, `audit_log` | One-use RWE authorization issuance, run admission, realized cost and token spend deduction against authorized budget caps, cell fencing, and terminal store evidence projection. |

### AC5: Explicit Runtime Composition Root Contract

`PE7-AC5-CONTRACT-1` freezes configuration sources, precedence, validated schemas, dependency graph topology, runtime operational modes, secret-resolution boundaries, and module migration batches before the additive root core and module migration implementation packets. The contract is strictly provider-free, introduces zero schema alterations, and creates no second store or runtime authority.

#### 1. Configuration Sources & Precedence Hierarchy

The composition root (`engine/src/main.rs`) resolves system configuration using a deterministic, fail-closed 4-tier precedence hierarchy:

```text
Level 1: Explicit CLI Flags / Arguments (highest precedence, overrides all)
  ↓
Level 2: Explicit Environment Variables (`ACP_*`, `HOST`, `PORT`, etc.)
  ↓
Level 3: Persisted Settings / Configuration Files (`store.config_snapshot()`, `.claude.json`)
  ↓
Level 4: Deterministic Compiled Defaults (lowest precedence, fail-closed / default-off)
```

| Domain | Configuration Keys | Precedence & Fallback Chain | Default Value |
|---|---|---|---|
| **Server & Network** | `HOST`, `PORT`, `ACP_PROFILE`, `ACP_TLS_CERT_PATH`, `ACP_TLS_KEY_PATH`, `ACP_CORS_ORIGINS`, `ACP_DASHBOARD_DIR` (or `DASHBOARD_DIR`) | Env `HOST`/`PORT` → defaults (`127.0.0.1:8080`). Env `ACP_PROFILE` (`"local"` vs `"production"`). Symmetrical TLS validation (fail-closed if asymmetric). Dashboard: `ACP_DASHBOARD_DIR` → `DASHBOARD_DIR` → none. | Host: `127.0.0.1`, Port: `8080`, Profile: `"local"`, TLS: disabled, CORS: wildcard warning in local, forbidden wildcard in production. |
| **Storage & Persistence** | `ACP_DATABASE_URL`, `ACP_DB_PATH`, `ACP_DB_ENCRYPTION_KEY`, `ACP_BACKUP_DIR`, `ACP_BACKUP_INTERVAL_SEC`, `ACP_BACKUP_RETAIN_COUNT` | Env `ACP_DATABASE_URL` (PostgreSQL mode, feature `pg` required) → `ACP_DB_PATH` (SQLite mode) → default `.agent-control-plane/local-team.db`. `ACP_BACKUP_DIR` → default `<db_parent>/backups`. `ACP_BACKUP_INTERVAL_SEC` → default 0 (disabled). | Backend: SQLite unencrypted at `.agent-control-plane/local-team.db`, Backup interval: `0` (disabled), Retain count: `5`. |
| **Authentication & RBAC** | `ACP_REQUIRE_AUTH`, `ACP_ADMIN_API_KEY` | Env `ACP_REQUIRE_AUTH` (boolean flag). If enabled, `ACP_ADMIN_API_KEY` is mandatory and validated against `harness_<64 hex chars>`. Injects tenant `"local"` with `local_admin_scope_list()` and delegation ceilings. | Auth: `disabled` in local mode; mandatory in production. |
| **Execution Mode & Gates** | `ACP_EXECUTION_MODE`, `ACP_ENABLE_PROVIDER_EXECUTION`, `ACP_TRUSTED_LOCAL_PROFILE`, `ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT` | Env `ACP_EXECUTION_MODE` (`"off"` or `"provider"`; `"cli"` and `"auto"` panic as retired). `EffectiveExecutionGates` evaluates composite flags (`ACP_TRUSTED_LOCAL_PROFILE` / `ACP_TRUSTED_LOCAL_TASK_ADVANCEMENT` + persisted endpoints). | Execution Mode: `"off"` (noop direct dispatch; CLI tools only via confirmed workflow nodes). Gates: `default-off`. |
| **Providers & Model Selection** | `ACP_PROVIDER_TYPE`, `ACP_MODEL`, `ACP_CLAUDE_CODE_CONFIG_PATH`, `ACP_BASE_URL`, `ACP_API_KEY`, `ACP_PROVIDER_INPUT_COST_PER_1K_USD`, `ACP_PROVIDER_OUTPUT_COST_PER_1K_USD`, `ACP_CIRCUIT_BREAKER_THRESHOLD`, `ACP_CIRCUIT_BREAKER_RECOVERY_MS` | Model resolution: Env `ACP_MODEL` → `.claude.json` project config (`ACP_CLAUDE_CODE_CONFIG_PATH` or `$HOME/.claude.json`) → `"default"`. Provider construction: `ACP_PROVIDER_TYPE` (`"stub"`, `"openai_compatible"`, `"anthropic"`). Pricing: Env rates → unconfigured warning. | Provider: `None` (unconfigured), Model: `"default"`, CB Threshold: `5`, CB Recovery: `30000ms`. |
| **Adaptive Execution** | `ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON`, `ADAPTIVE_PROVIDER_ENDPOINTS_CONFIG_KEY`, `ACP_ENABLE_ADAPTIVE_FUSION_EXECUTION`, `ACP_ADAPTIVE_FUSION_KILL_SWITCH` | Endpoint configs: Env `ACP_ADAPTIVE_PROVIDER_ENDPOINTS_JSON` → `store.config_snapshot()["adaptive_provider_endpoints"]` → `None`. Kill switch: Env `ACP_ADAPTIVE_FUSION_KILL_SWITCH`. | Adaptive Execution: `default-off`, Endpoints: `empty/None`. |
| **Scheduler & Workers** | `ACP_ENABLE_SCHEDULER`, `ACP_ENABLE_SUPERVISED_WORKERS`, `ACP_SUPERVISED_WORKER_COUNT`, `ACP_SCHEDULER_INTERVAL_MS`, `ACP_SCHEDULER_MAX_CONCURRENT`, `ACP_SCHEDULER_LEASE_TIMEOUT_MS`, `ACP_SCHEDULER_EXECUTOR` | `SchedulerConfig::from_env_with_gates(&execution_gates)`. Bounds validated at startup: workers `1..=32`, max_concurrent `1..=32` (`worker_count <= max_concurrent`), interval `250..=60_000ms`, lease timeout `1_000..=3_600_000ms`. | Scheduler: `disabled`, Interval: `2000ms`, Max concurrent: `4`, Worker count: `1`, Executor: `"noop"`. |
| **Managed CLI Profiles** | `ACP_ENABLE_CLI_EXECUTION`, `ACP_CLI_EXECUTION_KILL_SWITCH`, `ACP_ENABLE_CLAUDE_CODE_EXECUTION`, `ACP_CLAUDE_CODE_BIN`, `ACP_CLAUDE_CODE_SHA256`, `ACP_CODEX_BIN`, `ACP_CODEX_SHA256`, `ACP_CODEX_VERSION_POLICY` | `CliConfig::from_env()`. Claude Code requires proven worktree confinement; Codex requires exact profile admission (`ManagedCodingRuntimeProfile`). Kill switch overrides enablement. | CLI Execution: `disabled` (`DEFAULT_CLI_EXECUTION_ENABLED = false`). |
| **External & Sub-Runtimes** | `ACP_ENABLE_AGENT_RUNTIME`, `ACP_EXTERNAL_RUNTIME_MODE`, `ACP_OPENCODE_RUNTIME_MODE` | `ExternalRuntimeConfig::from_env` & `OpenCodeRuntimeConfig::from_env`. Bounded timeouts and fixed identity constraints. | External Runtimes: `disabled`. |

#### 2. Validated Configuration Schemas

The composition root aggregates typed, self-validating configuration schemas with strict validation bounds:

1. **`ServerConfig`** (`http_server.v1`, `axum_api.v1`): Binds host (`"127.0.0.1"` or `"0.0.0.0"`), port (`1..=65535`), API prefix (`"/api/v1"`).
2. **`DatabaseConfig` & `BackupConfig`**: Database backend (`Sqlite` or `Postgres`), paths, encryption keys, backup directory, interval, and retention count (>= 1).
3. **`TlsConfig`**: Symmetric certificate and private key paths; fails closed if asymmetric.
4. **`AuthConfig` & `TenantResolver`**: Auth enforcement, `harness_<64 hex chars>` key validation, bootstrap scopes with delegation ceilings.
5. **`EffectiveExecutionGates`**: Composite evaluation of profile readiness, cost limits, and endpoint pricing.
6. **`CliConfig` & `CliCapability`**: Execution switches, worktree confinement, binary hashes, and mandatory auth requirements.
7. **`SchedulerConfig`**: Verified intervals (`250..=60_000ms`), concurrency limits (`1..=32`), worker counts, lease timeouts, and executor wiring.
8. **`CostGateConfig` & `ProviderPricingConfig`**: Positive finite per-dispatch and daily cost caps, non-negative token pricing pairs.
9. **`ProviderConfig`**: Endpoint URL (safe HTTPS/loopback HTTP), model ID, timeouts (`1_000..=300_000ms`), and bounded retry policies.
10. **`AdaptiveProviderEndpointConfig`**: Symbolic credential names, pricing pairs, and endpoint registry mappings.

#### 3. Dependency Graph Topology (Acyclic Forward DAG)

In `engine/src/main.rs`, components are constructed through a strict, forward-only dependency injection graph without circular dependencies, global mutable variables, or runtime service locators:

```text
[1. Config & Env Validation]
  │ (host, port, profile, TLS, production violations)
  ▼
[2. Storage Construction]
  │ LocalProductStore (SQLite/Postgres + migrations + bootstrap keys)
  ├───────────────────────────────────────────┐
  ▼                                           ▼
[3. Shared Infrastructure]               [4. Provider & Audit Stack]
  │ CircuitBreakerRegistry                 │ ProviderAuditRecorder(store)
  │ EffectiveExecutionGates                │ Single Provider (OpenAI/Anthropic/Stub)
  │ CliConfig                              │ AdaptiveExecutionExecutor(providers, audit, kill)
  │                                        │ DispatchEngine (noop or provider)
  └─────────────────────┬─────────────────────┘
                        ▼
[5. API State Construction]
  │ AxumApiState (engine, store, cb_registry, cli_capability)
  │ + with_auth_live (TenantResolver, RateLimiter)
  │ + with_sub_executors (AgentStep, LangGraph, OpenCode)
  │
  ├───────────────────────────────────────────┐
  ▼                                           ▼
[6. Scheduler Construction]               [7. HTTP Router & Server]
  │ WorkflowScheduler(store, config)       │ build_axum_router / with_dashboard
  │ + with_node_executors                  │ + Middleware (cors_layer, request_id_layer)
  │ + with_auto_backup(BackupManager)      │ + axum_server / TcpListener bind
  │ + scheduler.start()                    │ + Graceful shutdown handler
  ▼                                           ▼
state.with_scheduler(scheduler_arc) ───────> runtime.block_on(server.serve())
```

- **No Circularity**: `LocalProductStore` has zero dependency on API state, scheduler, or executors. `WorkflowScheduler` borrows `store` and receives `NodeExecutor` trait objects. `AxumApiState` wraps the scheduler inside `Arc<Mutex<WorkflowScheduler>>`.
- **No Service Locators**: Every dependency is explicitly constructed at composition root and injected via constructor arguments or `with_*` builder methods.

#### 4. Runtime Operational Modes

| Mode | Trigger & Flag | Invariants & Startup Assertions | Default Behaviors |
|---|---|---|---|
| **`local`** | `ACP_PROFILE=local` (or unset) | Local SQLite persistence (`.agent-control-plane/local-team.db`), binds to `127.0.0.1`. Non-fatal warning if exposed to LAN (`0.0.0.0`) without auth. Permissive CORS allowed with warning. | Auth: `off` by default. Provider execution, adaptive fusion, scheduler workers, and CLI execution: `default-off`. |
| **`production`** | `ACP_PROFILE=production` | **Strict Fail-Closed Startup Gate** (`production_profile_violations`):<br>1. `ACP_REQUIRE_AUTH=1` strictly required.<br>2. `ACP_ADMIN_API_KEY` mandatory with valid `harness_<64 hex>` format.<br>3. `ACP_CORS_ORIGINS` must be explicit and **must not be `"*"` or empty**.<br>4. `ACP_BACKUP_DIR` must be explicitly configured.<br>5. LAN bind (`0.0.0.0`) strictly requires auth.<br>6. Symmetrical TLS configuration required if TLS is enabled.<br>*Any violation aborts process with fatal exit code 1.* | Fail-closed on all unsafe configurations. |
| **`test` / `stub` / `fixture`** | In-memory DB / explicit test harnesses | In-memory SQLite (`":memory:"`), `StubProvider` (`provider_type="stub"`), mock executors. Fixed virtual clocks (`fixed_now`) for deterministic auth and rate-limiter verification. Zero network calls or external credential requirements. | Deterministic offline verification. |

#### 5. Secret-Resolution Boundary

1. **Symbolic Credential References Only**: Configuration files, JSON payloads, and database tables store only symbolic names (e.g. `credential_env: "OPENAI_API_KEY"`) and redacted display tags (`CredentialRef` with `storage_backend: "env"` and `redacted_display: "***"`). Raw API keys, bearer tokens, and private keys are rejected at parse time.
2. **Late Pre-Send Resolution Boundary**: Secret environment variables are resolved **only inside the pre-send network invocation boundary** (`ReqwestTransport`, `OpenAiProvider::send`, `AnthropicProvider::send`) immediately prior to HTTP dispatch.
3. **Audit & Log Redaction**: All audit logs, event traces, serialized execution errors, and database receipts pass through `redact_sensitive_patterns` before emission.

#### 6. Successor Migration Sequencing

Completed via owner-scoped additive batches (receipts: PR #548 family); Git history owns the batch plan. The composition-root invariants in §1–§5 remain authoritative and unchanged.

### AC6: Rust-Authoritative API/SDK/Dashboard Schema Convergence Contract

`PE7-AC6-CONTRACT-1` freezes the single-source-of-truth schema authority, wire codegen rules, typed projection boundaries, compatibility invariants, version/deprecation windows, and migration ordering across the Rust engine, Python SDK, TypeScript SDK, and Dashboard.

#### 1. Schema Authority & Codegen Governance

```text
               ┌────────────────────────────────────────────────────────┐
               │              Rust Engine Domain Types                  │
               │   (Sole runtime, scheduling, and storage authority)    │
               └──────────────────────────┬─────────────────────────────┘
                                          │ defines / validates
                                          ▼
               ┌────────────────────────────────────────────────────────┐
               │         Canonical Wire Schemas (JSON Schema)           │
               │               (wire_contract/v1/*.json)                │
               └──────────────────────────┬─────────────────────────────┘
                                          │ codegen/generate_wire_types.py
                                          ▼ (deterministic code emission)
             ┌────────────────────────────┼────────────────────────────┐
             ▼                            ▼                            ▼
┌─────────────────────────┐  ┌─────────────────────────┐  ┌─────────────────────────┐
│     Rust Wire Types     │  │     Python SDK Types    │  │   TypeScript SDK Types  │
│  (engine/wire_types.rs) │  │  (sdk/python/.../wire)   │  │  (sdk/typescript/...   │
└─────────────────────────┘  └─────────────────────────┘  └────────────┬────────────┘
                                                                       │ consumed by
                                                                       ▼
                                                          ┌─────────────────────────┐
                                                          │   Dashboard UI Types    │
                                                          │  (dashboard/src/lib/..) │
                                                          └─────────────────────────┘
```

1. **Sole Domain Authority**: The Rust `engine/` is the sole runtime, API, scheduling, policy, and application-owned storage authority. Wire types in SDKs and Dashboard are strictly **read-only projections**, never domain authorities or alternative policy engines.
2. **Canonical Schema Source**: `wire_contract/v1/*.schema.json` and Rust engine canonical types constitute the sole wire contract definition.
3. **Deterministic Codegen**: All cross-language wire bindings must be generated by `codegen/generate_wire_types.py`. Manual edits to generated artifacts (`sdk/typescript/src/generated-wire-types.ts`, `sdk/python/src/agent_control_plane_sdk/wire_types.py`, `engine/src/wire_types.rs`) are strictly forbidden and rejected.
4. **Drift Enforcement Gate**: `scripts/check_wire_codegen_drift.sh` is an automated gate that enforces zero difference between schema definitions and generated targets across all CI and local verification workflows.

#### 2. Authoritative Wire Type Manifest

The AC6 contract governs these canonical wire entities: `dispatch_request.v1`, `task_analysis.v1`, `dispatch_decision.v1`, `execution_result.v1`, `evaluation_result.v1`, `dispatch_record.v1`, `dispatch_bundle.v1`, `budget_reservation.v1`, `local_cost_summary.v2`, `local_dispatch_cost_detail.v1`, `axum_api.v1`, and `local_dashboard.v1`. The authoritative machine surface is the schemas under `wire_contract/v1/*.schema.json` plus the generated wire types they emit (`engine/src/wire_types.rs`, `sdk/python/src/agent_control_plane_sdk/wire_types.py`, `sdk/typescript/src/generated-wire-types.ts`); per-entity field and authority descriptions live with the schemas, not in this document.

#### 3. Projection Boundaries & Non-Authority Rules

1. **Rust Engine $\rightarrow$ Wire Serialization**: Engine types serialize strictly to validated wire contract representations. Missing or unparsed fields fail closed at the deserialization boundary.
2. **Wire Types $\rightarrow$ SDK Consumers**: Python and TypeScript SDKs provide typed client interfaces (`AgentControlPlaneClient`, `ACPClient`) that consume and return generated wire models. SDKs must not implement fallback policy, synthetic state transitions, or client-side admission logic.
3. **SDK Types $\rightarrow$ Dashboard Projections**: The Dashboard UI consumes SDK client types for visualization and user interaction. Dashboard state is strictly a read-only projection of engine persistence; the Dashboard possesses zero workflow admission, evaluator, budget, approval, or output authority.

#### 4. Compatibility, Versioning & Deprecation Policy

1. **Additive Evolution Invariant**: New fields added to schemas must be optional or have backward-compatible default values. Existing fields must not be renamed or have their types narrowed within the same schema version.
2. **Breaking Schema Changes**: Any breaking change requires incrementing the schema version (e.g. `dispatch_request.v2`). The Rust engine must maintain dual-read support (`v_{N-1}` and `v_N`) throughout the transition window.
3. **Deprecation Window**: Deprecated wire fields and aliases are marked `@deprecated` in TypeScript and Python, remaining supported for exactly one convergence cycle before removal in AC7.
4. **Fail-Closed Parsing**: Unrecognized enum variants or structurally malformed payloads are rejected immediately with structured error codes rather than coerced into default states.

#### 5. Successor Migration Sequencing

Completed via the four dedicated successor packets (Rust codegen, SDK migration, Dashboard migration, compatibility closeout); Git history owns their plans and receipts. That ordering was execution sequencing only and grants no authority; the invariants in §1–§4 remain binding.

#### 6. AC6 Compatibility Closeout & AC7 Legacy Removal Manifest

All four AC6 implementation/migration packets converged with zero producer/consumer wire drift across every canonical entity and enum, verified continuously via `scripts/check_wire_codegen_drift.sh`. Git history owns the executed batch plans, caller inventories, call-path proofs, and pre-cleanup tree states behind that convergence. The durable invariants that survive convergence:

1. **Separate authority endpoints are the sole canonical path**: `POST /api/v1/product/tasks/:task_id/approve` and `POST /api/v1/product/tasks/:task_id/output`. Any composite approve-and-output surface was deprecated compatibility only; it is not a canonical path and cleanup code must not call it.
2. **Remove-after-zero-caller-proof**: a compatibility surface is deleted only after rerunning the fixed-string negative search across all tracked source, SDK, Dashboard, fixture, script, replay, and authority-test paths plus candidate-specific behavior, recovery, and consumer checks. Any newly discovered caller, owner, replacement mismatch, or recovery gap stops cleanup and requires a new manifest decision.
3. **Rollback-group principle**: deletion executes as separately revertable, owner-scoped groups (HTTP routing/handler surface, `LocalProductStore` methods, typed SDK/Dashboard wrappers); a scoped check failure stops and reverts that group, and cleanup never crosses a group boundary after a failure.

Merged receipts: PRs #560, #562, and #563 (Git history owns the detailed removal manifests they executed).

## Harness Evolution

Experiment control is established in separate bounded layers: identity/lineage/mutation registry; evaluator/holdout/contamination/gaming boundary; equal total-lifecycle budget; diversity/exploration controls; and hard-gate-first Pareto/stop/restart/recovery behavior. No layer creates a second evaluator, budget, store, scheduler, or adoption owner.

### EC2 evaluator and holdout contract

`PE7-HE-EC2-CONTRACT-1` freezes the evaluator boundary before the holdout,
sentinel, and prediction-outcome implementation packets. The contract is
provider-free and does not enable the default-off laboratory, authorize a
candidate run, or change selection, adoption, merge, release, deployment, or
production authority.

The existing owners remain canonical:

| Concern | Existing owner | Contract boundary |
|---|---|---|
| Active identity, candidate, lineage, mutable surface, and content binding | `engine/src/harness_evolution.rs` | Candidate identity and source-bound content are immutable inputs; candidates cannot replace the active Harness, evaluator identity, or policy. |
| Task family, labels, rubric identity, sealed vault, budgets, hard gates, metrics, evaluation bundle, and archive projection | `engine/src/harness_evolution_eval.rs` | The evaluator derives results from evaluator-owned task and evidence inputs; fixture helpers are not authoritative acceptance. |
| One-use sealed selection, evaluation persistence, receipts, archive, audit, replay, and rollback | `engine/src/storage/local_product_store/harness_evolution.rs` plus existing migration/audit owners | The evaluator supplies entrant IDs under the frozen policy; `LocalProductStore` alone validates, persists, and consumes the one-use receipt. It does not choose policy or create a competing evaluator. |
| Review, verification, scorecard, and PR-ready evidence | Existing repository review, verification, scorecard, replay, and output owners | These owners consume immutable redacted evaluator evidence; none can rewrite labels, rubric, outcomes, or evaluator identity. |

The access envelope is fixed:

1. A candidate or bounded autonomous worker may access only its admitted
   workspace and the explicitly permitted development/validation inputs. It
   cannot read plaintext labels, sealed membership, rubric internals, sentinel
   state, or prior sealed outcomes, and it cannot write an evaluation outcome.
2. The evaluator owns the frozen task-family manifest, plaintext labels and
   rubric, sealed vault, entrant-selection policy/request, sentinel
   disposition, and derivation of evaluation and prediction outcomes. Label
   and rubric changes require a new versioned manifest; an in-place edit is
   invalid.
3. A reviewer receives only the immutable, redacted evidence needed for review.
   Reviewer disagreement is evidence for the existing review owner; it is not
   permission to edit evaluator inputs or convert missingness into a pass.
4. The operator/controller may acknowledge receipts or separately authorize a
   later T3 effect, but cannot alter the evaluator constellation, labels,
   rubric, holdout, sentinel rules, or outcome derivation.

The exact EC2 contract manifest is one versioned, digest-bound control-plane
document, not a new runtime or persistence schema. Its v1 bindings are:

| Manifest field | Frozen v1 binding |
|---|---|
| Contract/evaluator | `manifest_id = harness_evolution_ec2_contract.v1`; `contract_id = PE7-HE-EC2-CONTRACT-1`; evaluator owner `engine/src/harness_evolution_eval.rs`; `EVAL_SCHEMA_VERSION = harness_evolution_eval.v1`; evaluator identity is the active identity's stored `evaluator_identity_hash`. |
| Task/holdout | Task-family manifest with `development`, `validation`, and `sealed_holdout`; `SEALED_SCHEMA_VERSION = harness_evolution_sealed_holdout.v1`; task membership digest `sha256("sealed|task_id|family_id|label_sha256")`, vault digest `sha256(join(sealed_task_hashes, "|"))`; plaintext tasks/labels remain evaluator-only. |
| Labels/rubric | Every task binds `task_id`, `family_id`, `label_sha256`, and an immutable rubric/version digest; a label or rubric change creates a new family/contract epoch. |
| Access | `ec2-access-policy.v1` with exactly the candidate/worker, evaluator, reviewer, and operator/controller classes defined above; no class may write another class's inputs or outcome. |
| Sentinels/invalidation | `ec2-sentinel-policy.v1` with contamination, gaming, and safety input-owner classes and `harness_evolution_sentinel_receipt.v1` receipts defined below; sentinel statuses are `PASS`, `FAIL`, and `UNKNOWN`; the separate invalidation state is `VALID`, `INVALIDATED`, or `UNKNOWN`, and only `VALID` may proceed. |
| Outcome | `PredictionOutcomeV1` evaluator derivation bound to hypothesis-manifest, evaluation/bundle, evaluator-identity, and evidence digests; statuses are `correct`, `incorrect`, `partially_supported`, `contradicted`, and `unavailable`. |
| Review | `ec2-review-policy.v1` and `reviewer_policy_sha256`: independent reviewer identity class; immutable evidence/hard-gate rubric; sealed-label/rubric blinding; no repair after evaluation; preserve-and-escalate disagreement; record review duration and rework timestamps as non-authoritative evidence. |
| Existing owners | Verification, replay, scorecard, review, output, audit, and `LocalProductStore` owner identities listed in the table above; no parallel owner is admitted. |

A candidate, reviewer, or worker cannot rewrite this manifest; changing any
field creates a new contract epoch and requires a separately accepted packet.
The manifest is canonical UTF-8 JSON with lexicographically sorted object
keys, no insignificant whitespace, no unlisted optional fields, access classes
in `[candidate_worker, evaluator, reviewer, operator_controller]`, and
sentinels in `[contamination, gaming, safety]`. `manifest_sha256` is
`sha256(canonical_json(manifest with manifest_sha256=""))`. Every component
policy digest is `sha256(canonical_json({component_id, version, payload,
digest:""}))`, with the component's declared payload and no hidden fields.
The per-epoch task-family, vault, evaluator, and reviewer-policy values are
required materialized fields; an empty value is invalid, not a deferred
placeholder.

The canonical manifest shape is fixed; every listed value is required and must be non-null in a materialized epoch. The `task` object is the sole
manifest owner of the label-policy and rubric digests; there is no second
`labels` object that repeats those facts. The byte-level JSON template is
owned by code, not prose: `engine/src/harness_evolution_eval.rs` defines the
schema and shape, and `engine/src/storage/local_product_store/harness_evolution.rs`
materializes and validates every field before any archive or effect. Each
`<sha256>` is a required epoch-bound digest computed by its component rule and
then included in the manifest digest; an empty or caller-claimed value is
invalid, never a deferred placeholder.

The evaluator constellation and holdout are immutable for one evaluation
epoch. A task family has development, validation, and sealed-holdout splits;
each task is bound to a stable task identity, family identity, label digest,
and rubric/version digest. The sealed vault exposes only membership hashes and
its canonical digest outside the evaluator. A store-owned, one-use selection
receipt may admit only the preselected bounded entrant set under the existing
evaluator/store limit; this contract does not change that limit. Sealed
execution never feeds mutation, prediction, parent selection, or archive
eligibility. Development and validation evidence may be recorded, but only
complete validation evidence with a passing hard gate, all three sentinel
receipts equal to `PASS`, and no invalidation can enter the existing
Pareto/archive path.

Archive admission remains one conjunctive, evaluator-owned rule: exact manifest,
selection-policy, and evaluator-identity digest binding; a consumed one-use
`harness_evolution_sealed_selection.v1` selection receipt associated through a
separate evaluator-owned binding keyed by its stable `receipt_id`; complete
validation records with passing hard gates; exactly the
contamination/gaming/safety sentinel receipts, each digest-bound and `PASS`;
an independent authenticated reviewer session, distinct from the evaluator and
implementation sessions, whose receipt has `disposition == PASS`; and
invalidation state `VALID`. The authoritative implementation lives in
`engine/src/harness_evolution_eval.rs`, with receipt persistence and
consumption in `engine/src/storage/local_product_store/harness_evolution.rs`.
Any missing, mismatched, stale, or outcome-unknown term makes the predicate
false and retains the rejected evidence under existing owners.

The existing `harness_evolution_sealed_selection.v1` receipt is preserved
byte-for-byte with its current fields (`schema_version`, `receipt_id`,
`family_id`, ordered `candidate_ids`, and `used`); the EC2 contract does not
add manifest, policy, evaluator, or digest fields to that v1 persistence
record.

Receipt bindings are fixed and versioned separately from that v1 record. A
sentinel receipt contains `schema_version`, `sentinel_id`, `policy_version`,
`receipt_schema`, `manifest_sha256`, `policy_sha256`, `input_owner`,
`candidate_id`, `evaluation_id`, `evaluator_identity_hash`,
`source_evidence_digest`, `status`, and `receipt_sha256`. A reviewer receipt
uses `harness_evolution_review_receipt.v1` and contains `manifest_sha256`,
`policy_sha256`, `candidate_id`, `evaluation_id`, `evidence_sha256`,
`reviewer_session_id`, `disposition`, `disagreement_digest`, and
`receipt_sha256`. Every such digest uses the blanked-self rule
(`sha256(canonical_json(receipt with receipt_sha256=""))`). Missing
authentication/session identity, digest, or owner binding is `UNKNOWN`, never
an implicit pass.

After the separate entrant-admission receipt and evaluation, three independent
sentinel classes are required before eligible archive or parent selection:

- contamination detects access to sealed tasks, labels, rubric, restricted
  corpus, or post-result information;
- gaming detects candidate control of evaluator inputs, selective evidence,
  metric/rubric manipulation, hidden rejection, or attempts to disable a
  sentinel; and
- safety detects scope, authority, secret, target-output, compatibility, or
  recovery-boundary violations.

The sentinel inputs are independent of candidate-authored status: contamination
uses access/audit and sealed-vault boundary evidence; gaming uses evaluator
configuration, evidence-completeness, and tool-policy cross-checks; and safety
uses the existing scope, authority, secret, target-output, and recovery
verification receipts. The contamination input owner is the existing
workspace/access/audit and `LocalProductStore` owner; the gaming input owner is
the evaluator and verification owner; the safety input owner is the existing
Product Golden Path, tool-policy, and output-boundary owner. Each class emits
one `harness_evolution_sentinel_receipt.v1` with its policy digest, input-owner
class, candidate/evaluation/evaluator identities, source-evidence digest, and
status. No class can disable another, and a missing, conflicting, or
candidate-controlled input makes independence `UNKNOWN`, not `PASS`.
Each sentinel is evaluator/owner-derived and returns a fail-closed invalidation
on `FAIL` or `UNKNOWN`. Invalidation keeps the candidate, rejection reason,
evidence digests, cleanup, and replay binding under existing owners, but
prevents acceptance, archive entry, PR-ready output, adoption, or a claim of
safety. Sentinel results never mutate labels or act as a scalar quality
override.

`PredictionOutcomeV1` is an evaluator-owned immutable derivation. For every
addressable prediction in the pre-execution
`MutationHypothesisManifestV1`, the evaluator joins the frozen prediction to
the candidate/lineage, task and metric or invariant identity, actual
verification/runtime evidence, and explicit missingness. The derived record
is bound to the hypothesis-manifest digest, evaluation/bundle digest,
evaluator identity, and evidence digest, then content-hashed before durable
recording. Its outcome is exactly one of `correct`, `incorrect`,
`partially_supported`, `contradicted`, or `unavailable`; incomplete, tampered,
invalidated, or otherwise missing evidence is `unavailable` and never a pass.
An absent regression prediction is not evidence that no regression occurred.
The fixed derivation precedence is: `unavailable` when required evidence is
missing, incomplete, tampered, or invalidated; `contradicted` when complete
evidence violates an invariant or crosses the opposite-direction threshold;
`correct` when every declared assertion meets its direction and threshold;
`partially_supported` when at least one assertion meets its threshold and at
least one remains unsupported without contradiction; and `incorrect` when
complete evidence supports none of the declared assertions without crossing an
opposite-direction threshold. An unpredicted regression is recorded as
counterevidence/invalidation input and never silently becomes `correct` or
safe.
The candidate may provide predictions as immutable input, but cannot author,
revise, suppress, or finalize the derived outcome. Prediction accuracy and
model confidence are calibration/audit evidence only: they cannot satisfy a
hard gate, select a Pareto parent, grant safety, authorize adoption, or change
the evaluator.

EC3 freezes total lifecycle budget as an accounting contract before any
instrumentation or enforcement exists. `Ec3LifecycleBudgetContractV1` covers
diagnosis, hypothesis construction, prediction, candidate materialization,
evaluation, review, repair, CI, recovery, human effort, and outcome
reconciliation. Its candidate and global envelopes name model tokens,
Provider calls and cost, wall-clock time, compute, and human effort explicitly.
Only directly measured or deterministically derived values are trustworthy;
`unavailable` remains an explicit source state. Caller estimates are not cost
evidence, and partial or unavailable required cost makes the candidate
ineligible. A true zero requires explicit evidence rather than absence. Every
terminal attempt, including rejected, failed, cancelled, and recovery work,
remains charged. Candidate limits apply per candidate; global limits aggregate
all candidates, and the first exhausted resource or count cap rejects the next
reservation before execution. The contract requires reservation before
execution and exact-once reconciliation after terminal outcome, but performs
neither operation itself: existing spend, admission, runtime, evaluator, and
`LocalProductStore` owners remain unchanged.

Candidate generation uses one source-bound causal-mutation evidence chain. `FailurePatternEvidenceV1` separates observed verifier/runtime facts, causal status, counterevidence, Harness addressability, and the admitted mutable surface; existing feedback traces, pattern detection, and outcome attribution remain observation inputs rather than a second failure-intelligence authority. `MutationHypothesisManifestV1` freezes the exact candidate delta, predicted improvements and regressions, invariants, thresholds, and evaluation plan before execution. The existing evaluator path alone derives `PredictionOutcomeV1` after evaluation under the binding above.

Level-1 core is a default-off one-generation laboratory with immutable active-Harness identity, candidate lineage, total-lifecycle-budget evaluation, hard gates, sealed holdout, Pareto archive, operator acknowledgement, and PR_READY output. Memory and skill projections are disabled in the core comparison so attribution remains identifiable. Optional memory-only and skill-only factor experiments may follow Level-1 but do not block the core Level-2 route. VDE does not rewrite or silently broaden the current evaluator or `MetricVector`.

Product durable memory and Harness-Evolution projections are separate domains. Sensitive raw prompts, raw outputs, transcripts, private paths, and unredacted repository content remain excluded from durable repository evidence. Experimental projections are derived, source-bound, deletable, rebuildable, invalidatable, and non-authoritative; they cannot grant routing, spend, evaluator, output, parent-selection, or adoption authority.

Level-2 is eligible only after a pre-registered-rule audit, independent evidence analysis, and explicit human GO decision using Golden Path stability, pre/post-convergence identical-corpus RWE, contamination risk, layered accepted-success reliability, realized lifecycle cost, review/rework/recovery burden, maintenance surface, implementation feasibility, and existing Level-1 composition.

A Level-2 GO requires every hard gate to pass, pre-registered quality and reliability non-inferiority, an eligible comparable value basis, uncertainty-aware improvement evidence, and no unacceptable authority, review, recovery, maintenance, or rollback regression. Pareto evidence precedes any scalar summary.

Even on GO, Level-2 remains bounded and may not modify `main`, merge, deploy, rewrite its evaluator, expand its authority, or adopt a production Harness automatically. Controller work separates the accepted state-machine contract, LocalProductStore state/lease persistence, generation orchestration, immutable evaluation/selection integration, stop/recovery semantics, provider-free simulation, one separately authorized pilot, and independent closeout. No implementation slice inherits authority from a later slice.

After final sealed transfer, production adoption and Meta Improver research are independent decisions. Adoption is human-authorized and does not require Meta research; Meta research does not grant adoption. Meta research first requires a separate human GO/NO-GO and bounded second-order claim protocol, then isolates operator interface, unseen-family corpus/evaluator, equal lifecycle budget, O0 baseline, O1 treatment, disjoint mechanics pilot, full comparison, independent replication, and final analysis. One improved descendant is never operator-level evidence. A NO-GO, harm, null, or insufficient result is valid completion.

Optional deeper research uses an explicit improvement-depth ladder, distinct from bounded task-tree recursion: R1 mutates one Harness generation; R2 runs multiple generations under a fixed controller; R3 compares fixed improvement operators; R4 may make only the admitted meta-operator procedure self-referential; R5 may co-evolve Harness code with parameter-efficient model adapters; R6 may evolve exactly one outer search-policy family. R4 and R5 are independently attributable sibling axes after R3 rather than a claim that weight adaptation is a deeper meta-level; R6 requires explicit dispositions from both. Every activated branch requires its named predecessor evidence, a separate human GO, equal lifecycle budgets, sealed comparison and replication sets, finite effects, complete rejected-lineage retention, and an explicit maximum depth. No level may self-modify evaluator/labels, goals, immutable safety policy, permissions, budgets, stop rules, production adoption, release, or deployment. There is no routed “meta level” beyond R6.

R4 is DGM-H-inspired but keeps the outer evaluator, archive admission, parent rule, budgets, authority, and stops fixed until a separately isolated R6 experiment. R5 is SIA-inspired but begins with an immutable open-weight base checkpoint and parameter-efficient adapter artifacts under a separate training-effect contract. Its first causal comparison is four-arm (`base`, `harness-only`, `weight-only`, `harness+weight`) with fixed schedules; interleaved lever selection is eligible only after that factorial result. Base/full-model weight evolution, model-architecture evolution, and Provider-hosted weight changes remain unrouted. External research code may inform an exact-commit, license- and threat-reviewed adapter, but cannot become the Rust runtime, `LocalProductStore`, evaluator, budget, training, or untrusted-code execution authority.

## External Adapter Boundary

External projects may provide bounded parsers, adapters, protocol compatibility, or comparison evidence. They must not become required core dependencies or replacement authorities.

CC Switch may be used as an MIT-licensed implementation reference for usage parsing, stream aggregation, model normalization, endpoint recognition, and pricing estimates. Its OAuth/account switching, credential persistence, automatic failover/retries, desktop UI authority, proxy database, and configuration ownership are outside this architecture.

Every adaptation records exact upstream commit, source mapping, license/attribution, semantic differences, and tests proving that core authority remains unchanged.

## Dashboard Boundary

The Dashboard and SDKs project accepted Rust-owned schemas and controls. They may display status, evidence, budgets, approvals, output operations, lifecycle-cost summaries, VDE evidence-sufficiency states, and baseline-relative indices, but they do not become workflow, evaluator, spend, approval, adoption, output, merge, release, or deployment authorities. A scalar VDE index must remain expandable to corpus, task, run, value basis, cost source, reviewer policy, failure class, and uncertainty. Dashboard PR #225 remains presentation-only and last.

## Safety and Non-Claims

The repository does not currently claim full Codex admission, managed Claude/OpenCode admission, accepted live Golden Path completion, accepted live RWE, stable accepted-success probability, realized VDE improvement, completed Architecture Convergence, automatic multi-generation evolution, demonstrated continuous learning, production self-update, or autonomous merge/release/deployment.

Those claims require the current gates in `docs/NEXT_DECISION.md` and the separately promoted contracts currently sketched in `docs/FUTURE_ROUTE.md`.

## Document Roles

- `ARCHITECTURE_BOOK.md` — durable mission, owners, boundaries, and invariants.
- `CURRENT_STATUS.md` — accepted main truth and confirmed gaps; no live PR/CI/review state.
- `NEXT_DECISION.md` — one current executable window, entry/exit evidence, and immediate next action.
- `FUTURE_ROUTE.md` — blocked long-horizon routing sketches; no execution authority.
- `MODULE_MAP.md` — accepted canonical owners; no proposed branch ownership.
- `REAL_WORLD_TESTING_PLAYBOOK.md` / `RUNBOOK.md` — operational validation and procedures.

Prefer updating these active documents over adding parallel strategy, status, or policy files.
