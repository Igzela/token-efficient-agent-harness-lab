# Next Decision

Last updated: 2026-07-23.

## Current Direction

The next product objective is to connect the already-implemented control-plane owners into one bounded, reviewable user-task transaction. The active lane is `PE7-PRODUCT-GOLDEN-PATH-1`, not Issue #266.

The verified reason is architectural: the repository already owns dispatch, plans, workflow runs, scheduler leases, executor selection, git worktrees, verification/repair, artifacts, approvals, target output, replay, scorecards, and Harness Evolution Level-1, but normal users must manually create and bind these records across separate endpoints. Level-2 evolution would otherwise optimize fixture generations before the product can reliably execute and measure one ordinary repository task.

The immediate bounded-maintenance prerequisite is `PE7-MANAGED-CLI-PROCESS-BOUNDARY-REPAIR-2`: repair the existing managed process owner before any live managed execution. It may extend only `engine/src/cli/` and the minimum authoritative documentation/tests needed to prove bounded capture, process-tree cleanup, typed failures, and hardened Claude version probing. It does not authorize a provider call, Claude admission expansion, Product Golden Path acceptance, target output, or a second runtime owner.

Downstream order: known repairs → managed-executor Golden Path → frozen first RWE corpus → small compatibility-preserving Architecture Convergence packets → same-corpus RWE rerun → Level-2 GO/NO-GO. Convergence does not replace the first baseline or authorize Level-2; Meta Improver remains later and separately authorized.

This ordering does not assert that every Golden Path task needs a paid provider or a real OpenCode binary. Final Golden Path acceptance requires both the deterministic fixture path and one already-admitted managed coding executor in disposable repositories, while preserving all live-provider and binary-admission gates. If no managed executor is safely available, the packet remains incomplete unless a separately reviewed authority decision changes that contract.

Do not create another roadmap, status, architecture, scheduler, runtime, queue, store, workspace owner, evaluator, output authority, or evidence source. Current facts belong in `docs/CURRENT_STATUS.md`; durable owner boundaries remain in `docs/ARCHITECTURE_BOOK.md`; ownership belongs in `docs/MODULE_MAP.md`; proven procedures belong in `docs/RUNBOOK.md`.

## Active Routing

1. `PE7-MANAGED-CLI-PROCESS-BOUNDARY-REPAIR-2` — `IN_PROGRESS`; exact-head CI/review pending.
2. `PE7-CLAUDE-ADMISSION-AUTHORITY-REPAIR-2` — `BLOCKED_PREREQUISITE`; waits for #1.
3. `PE7-UTF8-BOUNDARY-REPAIR-1` — `BLOCKED_PREREQUISITE`; waits for #2.
4. `PE7-PRODUCT-GOLDEN-PATH-RESIDUAL-SEAL-2` — `IN_PROGRESS`; managed-executor E2E remains open.
5. `PE7-REAL-WORKLOAD-EVIDENCE-1` — `BLOCKED_PREREQUISITE`; waits for Golden Path.
6. `PE7-ARCHITECTURE-CONVERGENCE-1` — `BLOCKED_PREREQUISITE`; waits for frozen first RWE.
7. `PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1` — `BLOCKED_PREREQUISITE`; waits for convergence.
8. Level-2 packet — `BLOCKED_PREREQUISITE`; waits for the replay and evidence GO/NO-GO. Issue #266 is proposal-only.
9. Meta Improver — `BLOCKED_PREREQUISITE`; OpenCode admission remains deferred; Issue #254 remains parked.
10. PR #225 remains independent presentation-only Dashboard work and is last.

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

## Packet PE7-MANAGED-CLI-PROCESS-BOUNDARY-REPAIR-2 — bounded managed process owner

**State:** `IN_PROGRESS`

**Goal:** Extend the existing managed CLI process owner with the versioned `managed_cli_output_limits.v1` per-stream/combined byte contract, concurrent bounded capture, process-tree cleanup evidence, typed failure taxonomy, and a bounded Claude version probe.

**Preserved owners and non-goals:** Reuse `engine/src/cli/mod.rs`, `cli_node_executor.rs`, and `config.rs`; keep the Rust engine/scheduler, `LocalProductStore`, Product Golden Path, receipts, late-write rules, and executor admission owners unchanged. No provider request, target-repository write, Claude filesystem-confinement claim, OpenCode admission, Vader/Issue #208 use, or active-Harness mutation is authorized by this packet.

**Acceptance:** Focused tests cover below/exact/over stream and combined limits, large-output drain, descendant cleanup after parent exit and timeout, nonzero exit, typed reader/wait/timeout/cleanup failures, unsupported-platform fail-closed admission, and bounded Claude probe hang/flood/malformed/nonzero/closed-stdin/cleared-environment behavior. Full applicable Rust, PostgreSQL, security, handoff, wire, stack, exact-head CI, correctness review, authority/security/recovery review, and rollback review are required before merge.

**Rollback:** Disable managed CLI admission and revert the focused PR. No schema migration or durable-data rollback is required; the existing bounded process owner and product receipts remain the sole owners.

## Packet PE7-CLAUDE-ADMISSION-AUTHORITY-REPAIR-2 — Claude authority

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `PE7-MANAGED-CLI-PROCESS-BOUNDARY-REPAIR-2` merged.

Before any model request, prove worktree-only confinement, synthetic app-owned HOME/TMP/config, exact binary identity, pre-call model/limit authority, and provider-free probes; otherwise disable managed Claude admission. No provider request, target output, or allowlist change.

## Packet PE7-UTF8-BOUNDARY-REPAIR-1 — deterministic UTF-8 previews

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `PE7-CLAUDE-ADMISSION-AUTHORITY-REPAIR-2` complete.

Repair only arbitrary UTF-8 slicing in Product Golden Path/Dynamic Workflow previews, with deterministic helper/tests and minimum factual docs corrections. No authority, persistence, scheduler, evaluator, or acceptance-contract change.

## Packet PE7-PRODUCT-GOLDEN-PATH-RESIDUAL-SEAL-2 — authoritative completion repair

**State:** `IN_PROGRESS`

**Prerequisite and preserved base:** PRs #268–#271 remain the accepted G1–G4, verification/scheduler, export-patch, and bounded `acp/*` foundations. This packet repairs their residual authority and evidence defects; it does not replace their owners or reactivate Vader, Issue #208, Issue #254, or OpenCode binary admission.

**Required slices, in order:**

1. **Merged via PR #272.** Separate product approval from output authority; require exact current evidence and explicit output confirmation; persist non-network receipts and phased branch-push/Draft-PR operations; create or idempotently reconcile a real GitHub Draft PR under exact HTTPS host/repository admission.
2. **Merged via PR #273.** Persist one canonical terminal-evidence record per terminal task version; make reads pure; use exact artifact/approval/output references and owner-backed replay, scorecard, executor, usage, cost, and actual process-outcome evidence.
3. **Merged via PR #274.** Enforce verification-time pause, kill, scheduler-kill, lease, version, workspace, timeout, elapsed-budget, supersession, and late-write authority; close focused SQLite/PostgreSQL recovery tests; require an attached runnable scheduler whose routing mode can consume the admitted executor. Every verification command routes through the existing API-owned managed-run and tool-policy receipt owner, admits only a fixed read-only command/relative-argument contract, redacts output before workflow persistence, observes the exact output patch through a temporary Git index, binds the pre-command patch identity durably for restart, caps timeout by remaining total budget, and holds the scheduler owner plus its worker-shared control gate through the atomic artifact/workspace/task/audit commit. The same gate covers worker-observed environment pause/kill. Exact head `7ca2b8e6` passed run `29880211660`, exact-head check `29880211865`, external validation `29880211694`, and both complete-diff review axes before squash merge `4588906c`.
4. **Code prerequisites merged via PRs #275 and #276; PR #278 adds dual-mode Claude model resolution with tracked resolved identity.** The admitted managed executor receives the exact persisted objective only in its in-memory leased input; public task/plan/terminal evidence stays redacted. Its `product_apply_binding.v2` additionally binds exact binary/version/hash, the model-resolution contract, context/output, and every standard/cache pricing dimension with source/check date, plus task token/call/retry/concurrency budgets. Claude Code 2.1.217 is admitted with `--max-turns 3`, a pre-reserved 792,000-token conservative ceiling, and a $2.16 worst-case client-estimate cap covering the higher one-hour cache-write price across all admitted turns. **Model pinning changed from required to optional by operator direction on 2026-07-23** — the operator's Claude Code authenticates through an opencode-go subscription import (first-party OAuth-style `ANTHROPIC_BASE_URL`/`ANTHROPIC_AUTH_TOKEN`/`ANTHROPIC_MODEL` environment variables), and the exact pinned Haiku snapshot was unreachable for that identity. Managed invocations pass `--setting-sources` with an empty value so host `settings.json` is not loaded as an authority source; credentials and model defaults reach the child only through the explicit env allowlist contract. When `ACP_CLAUDE_MODEL` is set the invocation passes `--model` with that value and pinned-mode identity checks apply; when unset the invocation omits `--model`, the CLI resolves its configured default from allowlisted environment, and the owner-reported per-model usage must prove exactly one resolved model identity which is persisted as `resolved_model` in node output and terminal usage evidence; missing or ambiguous identity fails closed. First-party `ANTHROPIC_BASE_URL`/`ANTHROPIC_AUTH_TOKEN`/`ANTHROPIC_MODEL` pass to the child only when the operator explicitly lists them in `ACP_CLI_ENV_ALLOWLIST`; generic proxy, alternate-model, cloud-routing, and TLS/proxy overrides remain discarded. Missing or changed identity, fewer than 792,000 authorized tokens, more than one scheduler call, any retry, or concurrency above one fails before the model process. Usage is owner reported, but CLI dollar fields are client-side estimates rather than authoritative billing and canonical cost evidence stays unavailable. This execution authorization never grants artifact approval or output confirmation. The deterministic fake-binary contract test exercises routing and evidence shape only and is not managed coding-agent acceptance evidence. Merge the default-off implementation after exact-head CI/review, then retain the packet as incomplete until one disposable live Claude (through the subscription import path) → verification → independent approval → Draft PR acceptance passes. Rollback disables Claude's separate gate and registration; no schema downgrade is required.
5. **Merged via PR #277.** Fixture control scaffolding removes itself before the declared change, and the artifact transaction rereads the current intake under the existing SQLite/PostgreSQL write lock before component-normalized allowed-path validation. Legal subtree spellings remain accepted; sibling-prefix or escaping output makes verification untrustworthy, quarantines the workspace, blocks the task, and commits no artifact. Exact head `6a0e43ae` passed run `29924682114`, exact-head check `29924682259`, and both complete-diff review axes before squash merge `ddf13020`; only one GitHub identity was available, so reviewer-identity independence is not claimed.

6. **Merged via PRs #279 and #280.** PR #279 corrected residual status/subscription-source documentation and pinned-mode `resolved_model` assertions; merge `116170e30a814ee6dd924673769ecbebfa1f470a`, exact head `a2d5c91dde8424de2ac7b231effe1e8b8861d32e`. PR #280 repaired the existing managed CLI wait owner with closed stdin, concurrent stdout/stderr draining, fail-closed reader errors, and Unix process-group containment for inherited pipes; merge `edcfb2fc3bf762e1111c043438053538cf3ff7fd`, exact head `4049d9523f2e3793033c07ed40687c7c6c6a884d`. These repairs do not change executor admission and do not substitute Codex, OpenRouter, fixtures, or a fake binary for the mandatory live managed-executor E2E.

Approval uses the existing workflow-approval owner but is a distinct `product_output_approval.v1` record requiring `team:admin`. Output requires `dispatch:execute`, the exact persisted approval, expected-current task version, and `confirm_output=true`. The legacy combined route is compatibility-only and must satisfy both scopes while invoking the same separate owners. Missing confirmation creates no approval, transition, claim, branch, PR, or success audit.

Draft PR output uses one progressive `product_output_operation.v1` under the supervised artifact owner. Branch and PR phases are independently durable. A completed branch is reused; it never proves PR existence. Only an open Draft PR whose repository/base/head/commit/artifact/approval binding matches can complete `draft_pr`. Network-disabled, admission failure, known HTTP failure, outcome unknown, or retry exhaustion remains non-terminal or explicit outcome-unknown. Attempts are bounded, credentials stay environment-owned, and no merge/default-branch authority exists.

**Completion:** The parent Golden Path packet remains `IN_PROGRESS` until every acceptance item below passes, including the managed coding-executor task. “Managed E2E optional” is not an active exception. If no already-admitted managed executor with a task-scoped pre/during-call token authority is safely available after complete environment audit, record the exact binary/version/capability blocker and leave this packet incomplete unless a separately reviewed authority decision changes the acceptance contract. A post-execution measured threshold is useful failure evidence but does not satisfy this prerequisite.

**Current audited blocker (2026-07-23):** Codex CLI `0.145.0` exposes usage only after execution and has no task-scoped pre/during-call token cap; the Claude subscription path remains blocked by the known API 404; OpenCode `1.18.4` is not admitted as a real upstream binary. The current operator direction is Codex-only for available quota, so no CLI substitution or OpenRouter provider path is authorized as managed Golden Path acceptance. RWE, Level-2, and Meta Improver remain blocked.

## Packet PE7-PRODUCT-GOLDEN-PATH-1 — canonical user-task orchestration

**State:** `IN_PROGRESS`

**Merged implementation:**

- G1 PR #268 → `main` `178d020e` (schema v30 `product_tasks`, intake, worktree-first bind).
- G2–G4 PR #269 → `main` `8fa85c15` (executable graph, finalize, artifact_only approve, SDK, Dashboard button).
- Authority repair PR #270 → `main` `f7293548` (real verification receipts, no finalize tick loop, live executor pool, fixture honesty, recovery matrix).
- Evidence/output PR #271 → `main` `fe742052` (dynamic task-rooted terminal summary, export_patch, gated `acp/*` push; canonical persisted terminal evidence is owned by the current residual slice).
- Residual output-authority PR #272 → `main` `c6806841` (separate approval/output permissions, terminal output semantics, progressive branch/PR receipt, real GitHub Draft PR adapter/reconciliation).
- Residual terminal-evidence PR #273 → `main` `1d125252` (schema v31 canonical evidence, pure reads, exact owner bindings, authoritative process outcomes, SQLite/PostgreSQL atomicity).
- Residual verification-authority PR #274 → `main` `4588906c` (per-command and artifact-commit authority, runnable-scheduler admission, read-only verification confinement, deterministic SQLite/PostgreSQL recovery synchronization).
- Residual managed-binding/authority PRs #275–#276 → `main` `3e94501d` / `4647b376` (exact private objective injection, workspace/run binding, cumulative measured usage, call/retry enforcement).
- Residual artifact-boundary PR #277 → `main` `ddf13020` (fixture helper exclusion plus atomic current-intake path-scope validation).

**Residual before `COMPLETE`:** governed by `PE7-PRODUCT-GOLDEN-PATH-RESIDUAL-SEAL-2` above.

1. **Passed for the fixture path.** Live `draft_pr` / `acp/*` push under existing target-output gates produced open Draft PR #1 in private disposable repository `Igzela/pe7-golden-path-acceptance-20260722`; its base is `main`, its head is `acp/product-ptask-20260722135332-18c4a108f1d4e757` at `6c70195c`, and target `main` remains `926f3d47`. The mandatory managed-executor acceptance must reuse this output authority and still prove the combined path.
2. Managed coding-executor E2E using an already-admitted executor without gate weakening, or an explicit separately reviewed acceptance-contract change.

Do not start `PE7-REAL-WORKLOAD-EVIDENCE-1` until residual is closed or explicitly accepted.

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

Freeze the first accepted corpus before Convergence, including outcomes, retries, available usage/cost, timeout/cancel, pause/kill, restart/recovery, SQLite/PostgreSQL, approvals, output/terminal evidence, target-main identity, and Draft PR behavior where applicable.

## Packet PE7-ARCHITECTURE-CONVERGENCE-1 — compatibility-preserving convergence track

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `PE7-REAL-WORKLOAD-EVIDENCE-1` accepted with a frozen corpus.

This is a sequence of small packets, starting with AC1 Unified ProcessSupervisor. Rust remains the authority; TypeScript is projection/interaction; Python is bounded adapter/evaluation/automation; one database/transaction authority remains. Repair unsafe subprocess owners before extraction, preserve persisted compatibility and SQLite/PostgreSQL atomicity, and do not start Level-2 before acceptance.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1 — same-corpus convergence rerun

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** `PE7-ARCHITECTURE-CONVERGENCE-1` accepted.

Rerun the same frozen corpus without relabeling tasks or changing evaluator authority. Compare outcomes, recovery, persistence, approvals, output, target-main, Draft PR, and available usage/cost; unexplained regression blocks Level-2.

## Packet PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1 — bounded multi-generation laboratory

**State:** `BLOCKED_PREREQUISITE`

**Prerequisite:** PE7-REAL-WORKLOAD-EVIDENCE-1, PE7-ARCHITECTURE-CONVERGENCE-1, and PE7-REAL-WORKLOAD-EVIDENCE-REPLAY-1

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
