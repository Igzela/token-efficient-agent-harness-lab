# Next Decision

Last updated: 2026-07-21.

## Current Direction

The next product objective is to connect the already-implemented control-plane owners into one bounded, reviewable user-task transaction. The active lane is `PE7-PRODUCT-GOLDEN-PATH-1`, not Issue #266.

The verified reason is architectural: the repository already owns dispatch, plans, workflow runs, scheduler leases, executor selection, git worktrees, verification/repair, artifacts, approvals, target output, replay, scorecards, and Harness Evolution Level-1, but normal users must manually create and bind these records across separate endpoints. Level-2 evolution would otherwise optimize fixture generations before the product can reliably execute and measure one ordinary repository task.

This ordering does not assert that every Golden Path task needs a paid provider or a real OpenCode binary. Initial acceptance must use a disposable repository and an already-supported deterministic or managed executor, while preserving all live-provider and binary-admission gates.

Do not create another roadmap, status, architecture, scheduler, runtime, queue, store, workspace owner, evaluator, output authority, or evidence source. Current facts belong in `docs/CURRENT_STATUS.md`; durable owner boundaries remain in `docs/ARCHITECTURE_BOOK.md`; ownership belongs in `docs/MODULE_MAP.md`; proven procedures belong in `docs/RUNBOOK.md`.

## Active Routing

1. `PE7-PRODUCT-GOLDEN-PATH-1` — `READY_FOR_EXECUTION`.
2. `PE7-REAL-WORKLOAD-EVIDENCE-1` — `BLOCKED_PREREQUISITE`.
3. `PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1` — `BLOCKED_PREREQUISITE`; Issue #266 remains open as a proposal, not the active lane.
4. `PE7-META-IMPROVER-EXPERIMENT-1` — `BLOCKED_PREREQUISITE`.
5. `PE7-OPENCODE-BINARY-ADMISSION-1` remains deferred. `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1` remains parked on Issue #254.
6. PR #225 remains an independent presentation-only Dashboard PR.

## Packet States

- `READY_FOR_EXECUTION`: current owners and acceptance contract are sufficient to begin.
- `BLOCKED_PREREQUISITE`: defined but cannot begin until the named prerequisite is complete and verified.
- `DECISION_REQUIRED`: a material product, architecture, or authority decision cannot be derived safely.
- `IN_PROGRESS`: one current branch or PR owns the packet.
- `COMPLETE`: merged, verified, reviewed, and synchronized in active documents.

## Common Execution Protocol

Historical packet references retained for handoff compatibility: Packet PR207-REPAIR-1, Packet PE2-RUNTIME-PRODUCER-1, Packet PE4-EVIDENCE-ENTRY-1, and Packet TOOL-DISCOVERY-BENCH-1. They are not active routing.

Every packet must:

- refresh actual `main`, open PRs/issues, CI, and overlapping ownership;
- use one focused branch/PR per coherent risk surface;
- preserve Rust `engine/` as the sole runtime and `LocalProductStore` as the sole application-owned store;
- reuse existing scheduler, executor pool, node executors, worktree, verification, artifact, approval, target-output, replay, scorecard, and audit owners;
- bind authority from persisted current owners rather than caller assertions;
- fail closed on missing, stale, conflicting, tampered, late, duplicate, oversized, over-budget, killed, paused, or outcome-unknown state;
- preserve SQLite/PostgreSQL behavior, restart recovery, concurrency, idempotency, budget, kill, pause, late-write refusal, compensation, and rollback;
- prohibit provider calls in CI and keep target `main`, merge, release, and deployment authority outside the runtime;
- run focused tests, applicable full verification, exact-head CI, and independent complete-diff review;
- keep auto-merge disabled;
- update only the smallest authoritative active documents.

Strictly documentation-only factual synchronization follows `docs/REAL_WORLD_TESTING_PLAYBOOK.md`: focused branch/PR by default, `git diff --check`, `uv run --no-project python scripts/check_agent_handoff.py`, applicable documentation/security checks, reviewable diff, and revert rollback.

## Hard Stops

Stop and report `BLOCKED` when:

- another active owner controls overlapping paths;
- a secret, raw prompt/output/transcript, credential, private path, or unredacted repository content would enter durable evidence;
- an existing runtime, scheduler, store, workspace, evaluator, verification, approval, target-output, audit, compensation, or rollback owner would be duplicated or bypassed;
- a node can execute before exact worktree/base/allowed-path binding;
- provider, external binary, network, target output, or repository mutation gates are weakened;
- target `main`, merge, auto-merge, release, deployment, or production installation would be authorized;
- SQLite/PostgreSQL, restart, concurrency, idempotency, budget, kill, pause, late-write, or rollback evidence is missing;
- exact-head CI, independent review, or known failures would be hidden or treated as passed.

## Packet PE7-BOUNDED-RECURSIVE-EXECUTION-1 — bounded recursive task execution

**State:** `COMPLETE`

PR #239 implements a default-off persisted task tree through existing Agent Runtime, workflow, scheduler, receipt, and store owners. It provides bounded child-task admission, not autonomous root-goal creation, online self-update, or recursive self-improvement.

## Packet PE7-OPENCODE-EXTERNAL-ADAPTER-1 — fixture external coding adapter

**State:** `COMPLETE`

The default-off OpenCode adapter is accepted only in fixture form after PR #255 and the honesty repair in PR #257. It does not admit a real upstream binary, network tools, provider routing, or production repository mutation.

## Packet PE7-OPENCODE-BINARY-ADMISSION-1 — real binary admission

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-OPENCODE-EXTERNAL-ADAPTER-1

Deferred until an exact upstream release/source identity, immutable checksum, license/supply-chain evidence, confinement contract, and independent review are available. This packet is not a prerequisite for the initial Golden Path because Golden Path acceptance can use existing deterministic/managed executors.

## Packet PE7-HARNESS-EVOLUTION-LEVEL1-ACCEPTANCE-1 — fixture owner-path acceptance

**State:** `COMPLETE`

PRs #258–#260 created B1–B3 scaffolding; PRs #262–#264 repaired active-identity/workspace authority, evaluator/sealed-task ownership, and PR_READY finalizer ownership; PR #265 accepted the fixture end-to-end owner path at `main` commit `6b4091e876b2fca0da03485540e5ce4f579ac13c`.

The result is neutral/no-improvement fixture evidence. The active Harness remains immutable. This is not Level-2, Meta Improver, production recursion, or recursive self-improvement.

## Packet PE7-PRODUCT-GOLDEN-PATH-1 — canonical user-task orchestration

**State:** `READY_FOR_EXECUTION`

**Goal:** Add one canonical task-intake and orchestration path that turns an ordinary-language repository task into a bounded executable workflow using only existing owners, ending in an approval-bound patch or Draft PR against a disposable target while leaving target `main` unchanged.

**Prerequisites:**

- current `main` and active documents are refreshed;
- no overlapping owner is active;
- existing worktree, scheduler, verification, artifact, approval, output, replay, and scorecard owners remain authoritative;
- initial acceptance uses a disposable repository and no provider, Vader, Issue #208, real OpenCode binary, release, or deployment.

**Existing owners:**

- intake/API/auth: `engine/src/http_server/`, existing request validation, auth, rate limits, audit;
- analysis/routing/budget: `task_analyzer`, `model_selector`, `budget_manager`, active routing policy;
- plan/graph: `read_only_planner`, `TaskDecomposer`, DAG/dependency/context-budget owners;
- execution: `workflow_runs`, `scheduler`, `scheduler/runtime.rs`, `executor_pool`, node/tool-policy/CLI/Agent Runtime/external executors;
- workspace: supervised-patch and `target_repo_output` git-worktree owners;
- verification/repair: supervised-patch verification and canonical API-owned repair-loop receipts;
- artifact/approval/output: supervised-patch capture, integrity, redaction/secret scan, workflow/operator approval binding, patch export, `acp/*` branch push, optional Draft PR;
- evidence: dispatch history, workflow events, orchestration decisions, replay producer, scorecards, budget producer, audit;
- persistence: existing `LocalProductStore` SQLite/PostgreSQL transaction owners.

**Canonical task identity:**

Introduce one versioned root task record or compatible extension under `LocalProductStore`; do not create a second store. The control plane derives the canonical `task_id`. Every plan, workflow/run, workspace, node lease/execution, verification operation, artifact, approval, target-output receipt, replay/scorecard, audit record, and Draft PR reference must bind back to that task identity and exact tenant/workspace scope.

Caller-supplied downstream IDs may be optimistic references only; they do not establish authority.

**Task intake contract:**

One authenticated request must bind at least:

- natural-language objective;
- target repository identity and operator-provided path;
- exact source revision and derived source tree/base identity;
- allowed mutable paths/surfaces;
- verification commands and timeout bounds;
- output intent (`artifact_only`, `export_patch`, or `draft_pr`);
- executor policy/allowed executor set, not an unchecked arbitrary binary;
- budget, token/call/time/retry/concurrency ceilings;
- risk and approval requirements;
- explicit execution and output confirmations where existing owners require them;
- idempotency key and expected-current binding.

Raw prompts and repository content follow existing bounded/redaction policies; no new raw-content evidence store is allowed.

**Worktree-first binding:**

Prepare or reuse the app-owned controlled git worktree before any executable node can be leased. Persist exact target, source revision, workspace path identity, allowed paths, and workspace content/base hash. Compile these bindings into every executable node. Missing, changed, escaped, symlinked, stale, quarantined, or cleaned workspaces block execution with no cwd fallback.

**Route selection:**

Use existing task analysis, active routing policy, budget manager, executor pool, and capability/tool-policy owners to choose among admitted executors. Route decisions must be persisted and current-state-bound. Default-off capabilities stay off; unavailable executors fail closed rather than silently becoming useful noops.

**Executable graph compilation:**

Compile the advisory analysis into a versioned executable graph whose nodes contain bounded task objective/input references, exact workspace binding, executor/capability requirements, verification/output dependencies, and budget slices. Do not simply relabel the current generic read-only nodes. Preserve compatibility for existing `/plans` and explicit Agent Runtime/adaptive callers.

**Scheduler advancement:**

Create the run and make it eligible for the existing scheduler only after intake/worktree/graph commit succeeds atomically. The existing scheduler must lease and advance ready nodes until terminal, approval-waiting, paused, killed, blocked, or budget-exhausted state. No second loop, queue, worker, or scheduler is allowed. Explicit manual tick remains compatible but is not required for the accepted Golden Path.

**Verification and repair:**

After execution, invoke the existing supervised-patch verification owner with exact task/run/workspace/command binding. Reuse canonical operation/attempt receipts and bounded repair attempts. A failed or outcome-unknown execution cannot be hidden by later success. Late writes after timeout/kill/lease loss must be rejected or quarantined.

**Artifact, approval, and output:**

Capture only after trustworthy verification evidence exists. Reuse existing patch hashing, changed-file bounds, secret scan, redaction, integrity, and approval binding. Output uses existing target-output receipts and may export a patch or push an `acp/*` branch and create a Draft PR. Target default branch and `main` remain unchanged; merge and auto-merge remain unavailable.

**Feedback and evidence:**

Terminal task processing must idempotently emit or link:

- dispatch/routing/budget decision;
- workflow/node/executor/verification outcomes;
- artifact/output identity;
- normalized usage/cost availability;
- replay eligibility/production result;
- scorecard or explicit unavailable reason;
- operator/audit evidence.

No fabricated zero cost, quality, or improvement. This evidence is the prerequisite input for `PE7-REAL-WORKLOAD-EVIDENCE-1`.

**Hard gates:**

Auth, tenant/workspace scope, target-output enable/kill, executor enable/kill, tool policy, approval, secret scan, integrity, budget, timeout, concurrency, pause, scheduler kill, late-write refusal, replay provenance, and rollback remain authoritative. Provider calls are forbidden in CI. No Vader/Issue #208 path.

**Non-goals:**

- no Level-2 or Meta Improver;
- no real OpenCode binary admission;
- no provider evolution or active-policy auto-promotion;
- no new scheduler/runtime/store/queue/workspace/evaluator/output owner;
- no target `main` write, merge, release, deploy, or production installation;
- no claim of general autonomy or production recursive self-improvement;
- no requirement to redesign PR #225 presentation work.

**Acceptance:**

1. One API/SDK path accepts the complete task contract and returns the canonical task identity.
2. SQLite and PostgreSQL persist equivalent task/identity/owner bindings.
3. Duplicate/restart/concurrent intake creates one canonical task and one bounded run/worktree effect.
4. The worktree exists and is bound before any executor invocation; a mismatch results in zero subprocess/provider/output effect.
5. The existing scheduler automatically advances the executable graph without manual endpoint stitching.
6. A deterministic fixture task and one already-supported managed coding-executor task run in disposable repositories.
7. At least one ordinary-language coding task produces a real worktree change, passes declared verification, captures a redacted artifact, receives current approval, pushes only an `acp/*` branch, and opens a Draft PR while target `main` remains byte-for-byte unchanged.
8. Negative tests cover auth/scope, stale source, path escape, unsupported executor, budget, timeout, retry, pause, kill, restart, concurrency, duplicate request, late write, verification failure, secret scan, stale approval, output outcome-unknown, and rollback.
9. Terminal evidence links task, plan, run, workspace, node, verification, artifact, approval, output, replay, scorecard, and audit identities.
10. Dashboard and both SDKs expose the canonical task path without creating alternate authority.
11. Existing legacy endpoints remain compatible or receive an explicit versioned migration contract.
12. Focused checks, SQLite/PostgreSQL tests, full applicable CI, exact-head verification, independent complete-diff review, and disposable-repository E2E all pass.

**Rollback:**

Disable the new intake/orchestrator gate, stop new admissions, pause/drain existing canonical tasks, preserve receipts and evidence, compensate or reconcile any outcome-unknown target output through existing owners, discard only unapproved app-owned worktrees, revert the PR, and leave additive schema inert. Destructive schema rollback is allowed only under existing empty-authority/stopped-writer/backup rules.

**Sequencing:**

- Slice G1: canonical task schema/intake/identity and worktree-first transaction.
- Slice G2: executable graph compiler and existing scheduler/executor routing.
- Slice G3: verification/repair, artifact, approval, and output orchestration.
- Slice G4: evidence/replay/scorecard linking, SDK/Dashboard surface, disposable E2E and recovery matrix.
- Merge each slice only when it is coherent and independently useful; refresh `main` between PRs. Do not activate the next packet until the full Golden Path acceptance is complete.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-1 — trustworthy product evidence

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-PRODUCT-GOLDEN-PATH-1

**Goal:** Run a bounded corpus of disposable, ordinary-language repository tasks through the accepted Golden Path and persist owner-backed evidence suitable for product diagnosis and later evolution research.

**Required evidence:**

- task classes, repository/fixture identity, exact source revision, executor/runtime identity;
- success/failure/blocked reason, verification quality method/result, repair count;
- input/output/context/retrieval tokens when trustworthy;
- provider/model/pricing and cost only when authoritative;
- tool calls, redundant calls, retries, elapsed time, scheduler/lease events;
- changed-file and patch-size bounds, artifact/approval/output result;
- restart, concurrency, pause, kill, late-write, and rollback outcomes;
- redacted references to dispatch, run, workspace, verification, artifact, replay, scorecard, and audit owners.

No raw prompt/output/transcript/repository-content corpus is authorized. The workload set and quality evaluator must be versioned, bounded, reviewable, and separated from any candidate being evaluated.

**Acceptance:** repeated runs establish trustworthy baselines and failure distributions across more than one task class and executor mode; missing evidence is explicit; no fixture-only result is relabeled as production evidence.

## Packet PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1 — bounded multi-generation laboratory

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-REAL-WORKLOAD-EVIDENCE-1

Issue #266 remains open as the initial proposal. Before activation it must be rewritten or amended to bind its generations to accepted real-workload evidence, current Level-1 owner contracts, deterministic per-lineage/global budgets, restart/concurrency/idempotency semantics, stop reasons, and an independently reviewed evidence threshold.

Level-2 may orchestrate multiple Level-1 proposal/workspace/evaluation/archive/finalizer cycles. It may not create a second evaluator, store, queue, scheduler, workspace owner, promotion authority, target-output authority, merge owner, or active-Harness mutation path. Up to three generations in the issue is a proposal, not an accepted product default.

**Activation evidence:**

- Golden Path complete and stable;
- real-workload evidence packet complete;
- at least one failure or efficiency pattern that Level-2 can validly optimize;
- evaluator/workload separation and contamination review;
- deterministic generation/global budget and stop contract;
- Level-1 restart/concurrency/idempotency/late-write guarantees shown to compose across generations;
- explicit decision that the expected research value exceeds the complexity and overfitting risk.

## Packet PE7-META-IMPROVER-EXPERIMENT-1 — bounded meta-improver research

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1

This remains a separate experiment requiring a stable, independently reviewed Level-2 result, a new authority/threat-model decision, and proof that evaluator, sealed tasks, permissions, budgets, audit, promotion thresholds, rollback, and active-version binding remain immutable. It must not be inferred from recursive task execution or Level-1/Level-2 candidate generation.

## Packet PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1 — parked repository-agent smoke

**State:** `BLOCKED_PREREQUISITE`

Parking issue: #254. The Vader/Issue #208 path is not active product work and must not be enabled by Golden Path, Level-2, or Meta Improver implementation.
