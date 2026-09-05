# Architecture

Current version: v38
Last updated: 2026-09-03.

This is the durable architecture, module ownership, and trust boundary specification for the Token-Efficient Agent Harness Lab. It consolidates system design, module ownership, single persistence authority, and trust boundaries into one authoritative document.

## System Mission

The system is a local and deterministic agent harness and workflow control plane for auditable coding-agent workflows. It provides:
- Rust `engine/` as the sole runtime, scheduler, policy, and application-owned storage authority.
- `LocalProductStore` as the sole persistence and audit owner across SQLite and PostgreSQL backends.
- Autonomous Steward as the repository-maintenance outer loop coordinating missions, stages, and workcards without creating parallel schedulers or state stores.

### Core Objective

> Under non-negotiable quality, safety, traceability, compatibility, and rollback constraints, continuously increase verifiable and reusable task delivery per unit of total lifecycle cost.

Lower token consumption is an optimization, not a reason to weaken verification, safety, audit, or recovery guarantees.

## Authority Layers

The repository separates maintenance automation from the product runtime and
from research decisions. Each authority has one owner:

| Layer | Canonical owner | Owns | Explicitly does not own |
|---|---|---|---|
| Repository-maintenance control plane | `scripts/agent-control/steward_service.py` | User-approved Missions, Steward Stages, bounded WorkCards, repository PR/review/CI/merge progression, and accepted-main readback | Product runtime, product scheduler, ProductStore, evaluator, research claims, Provider spend, or active-Harness replacement |
| Product and task runtime | Rust `engine/` | Execution, leases, scheduling, policy, task state, verification, output, effects, recovery, and rollback through its existing module owners | Repository-maintenance lifecycle, experimental adoption, or persistence outside the Store |
| Persistence and audit | `engine/src/storage/local_product_store/` | SQLite/PostgreSQL persistence, audit, idempotency, evidence/artifacts, effect envelopes, and terminal settlement | Runtime scheduling, research evaluation, or a second truth store |
| Common RWE measurement | `engine/src/rwe/` | Frozen task/corpus/protocol/schedule identity, comparable budgets, lifecycle evidence, missingness, and provider-free/live evidence seams | A shortcut around correctness, safety, comparability, Store persistence, or effect authority |
| Harness-Evolution evaluation | `engine/src/harness_evolution_eval.rs` | Sealed holdout, evaluator binding, hard gates, candidate/causal/Pareto evidence, and explicit `INCOMPARABLE` outcomes | Active-Harness replacement, merge, release, deployment, or adoption |
| Experimental descriptors and candidates | `engine/src/harness_evolution.rs`, `engine/src/harness_evolution/ledger_orchestration.rs` | Immutable Harness, Model, and Strategy descriptors, candidate Harness implementations (e.g. `ledger-orchestrated:provider-independent:v1`), matrix identity, adapter normalization, and explicit `INCOMPARABLE` projections | Runtime scheduling, budget/effect authority, evaluator mutation, or adoption |
| Adoption decision | User through an owner-approved Mission/Stage decision | Evidence-backed transfer/replication review and explicit adoption of a new Harness identity | Merge, release, deployment, evaluator replacement, or self-authorized active-Harness change |

RWE is therefore a shared measurement substrate, not a peer runtime or a
separate research authority. Context Working Set, memory, and skill mechanisms
are model-visible Strategy inputs when registered for an experiment; they do
not become truth, memory ownership, scheduling, evaluation, or approval merely
because they reduce context.

### Managed Codex credential boundary

The managed `agent-steward` service never depends on an interactive user's
home. Its production Codex executable is the fixed, non-symlink,
non-group/world-writable
`/usr/local/libexec/agent-steward/codex`; systemd exposes only the encrypted
credential named `codex-auth` through its private `CREDENTIALS_DIRECTORY`.
The parent validates that directory and credential as private regular
filesystem objects, mounts only that file as the isolated child's
`CODEX_HOME/auth.json`, and does not forward the source path. If the manager
declares a missing, malformed, symlinked, or over-permissive credential, the
adapter fails with a bounded authentication category and never falls back to
an operator HOME. The credential is never created, copied, journaled, logged,
or granted by a Mission; provisioning or rotating it remains a separately
authorized operator action.

`ProtectSystem=strict` remains enabled. The managed service may write only its
journal root, the exact checked-out repository (including Git metadata), and
the repository-specific integration-worktree root; card worktrees remain in
its private `/tmp`. The checked-in unit names those three exact write roots.
This is repository-maintenance authority, not permission to write another
repository, an operator home, or an arbitrary host path.

### Codex Lifecycle Hooks Architecture & Trust Model (H0–H3)

Codex Lifecycle Hooks integrate the local Codex execution engine with Steward WorkCard constraints. Hooks are strictly worker-local event adapters; the Steward service remains the sole scheduler, reviewer, journal writer, and merge authority. Hooks never write to the durable journal or claim task completion.

- **Authority and Lifecycle Separation**:
  - The Steward owns WorkCard definition, path locks, git tree lifecycle, PR integration, and verification gates.
  - Hooks adapt events (`SessionStart`, `PreCompact`, `PostCompact`, `PreToolUse`, `PostToolUse`, `PermissionRequest`, `Stop`) inside the isolated child worker namespace during CLI execution.
  - No hook may write to `/var/lib/agent-steward/steward.sqlite3` or alter durable mission state.

- **H0: Runtime Capability & Trust Probe**:
  - Evaluates 14 distinct runtime capabilities (`hooks.basic`, `session_start`, `pre_tool`, `post_tool`, `permission_request`, `compact`, `stop`, `interrupt`, `subagent`, `async`, `mcp_tool`, `isolated_codex_home`, `hook_trust_bootstrap`, `definition_hash_invalidation`).
  - Reports explicit deterministic states: `VERIFIED`, `UNSUPPORTED`, `BLOCKED`, or `UNVERIFIED`.
  - Serves as the pre-flight gate before dispatching hook-enabled worker sessions.

- **H1: Context Bootstrap, Compaction Rehydration & Ephemeral Receipts**:
  - `SessionStart`: Injects bounded WorkCard objective, target paths, and execution invariants into the initial agent context, avoiding large whole-document ingestion.
  - `PreCompact` & `PostCompact`: Snapshots current task progress and git modifications prior to model context compaction, and rehydrates active constraints into the post-compaction context to prevent goal drift.
  - `PostToolUse`: Captures tool output receipts in the worker's ephemeral state directory (`hooks_state/receipts/`), compressing oversized command outputs.
  - Minimal ROI Telemetry: Measures bootstrap tokens saved, receipt bytes compressed, and compaction events.

- **H2: Worktree Path Boundary Guard & Permission Evaluation**:
  - `PreToolUse`: Intercepts file writes and shell execution. Enforces strict fail-closed validation on missing or malformed WorkCard context (`STEWARD_WORKCARD_ID`, `STEWARD_ALLOWED_PATHS`), verifies target files lie strictly within the active WorkCard's `allowed_paths`, prevents escaping the worktree root, and rejects strictly forbidden paths (e.g. `docs/ROADMAP.md`) or dangerous command patterns. Emits official wire decisions (`decision: "approve"|"block"`, `permissionDecision: "allow"|"deny"|"ask"`).
  - `PermissionRequest`: Evaluates requested actions fail-closed, emitting official wire schema `hookSpecificOutput.decision.behavior: "allow" | "deny"`. Only permits provably scoped, low-risk operations (e.g. read-only inspections, declared in-scope test suites) and denies out-of-scope or ambiguous commands (no auto-allow on blacklist-miss).

- **H3: WorkCard Acceptance Evidence Evaluation & Continuation Loop**:
  - `Stop`: Intercepts premature termination by validating declared WorkCard acceptance and verification evidence (workspace edits confined to `allowed_paths`, passing `focused_tests` / `verification_evidence.json`), rather than unverified raw git status mutations. When incomplete and continuation budget remains, blocks termination with top-level `decision="block"` and a targeted continuation prompt on stderr (exit code 2). When the retry budget is exhausted or execution paused, writes `completion_status.json` with an explicit `incomplete` status and exits cleanly.

- **Cryptographic Trust Bootstrap & Per-Handler Discovery**:
  - The production configuration strictly prohibits `--dangerously-bypass-hook-trust`.
  - Trust is established through native Codex discovery (`codex app-server --stdio` `hooks/list`), deriving authoritative per-handler hook keys (`<config_path>:<normalized_event>:<matcher_idx>:<hook_idx>`) and definition digests (`currentHash: "sha256:..."`).
  - `provision_trust` records per-handler trust entries under `[hooks.state."<key>"]` with `trusted_hash = "<currentHash>"`, and verifies readback directly against the Codex engine to prove `trustStatus == "trusted"`.
  - Any alteration to hook commands or definition hashes causes Codex discovery to report `modified` or `untrusted`, halting execution fail-closed.

## Research Mainline: Finite Frozen Canonical Experiments

Research evidence on the common RWE basis is obtained only through finite
frozen canonical experiments, never through live or ad-hoc comparison runs.
Every experiment binds, before any run, a frozen task/corpus identity, an
exact evaluator binding, immutable Harness, Model, and Strategy descriptors, a
deterministic schedule, comparable budgets and identities, a protocol and
seeds, and a lifecycle analysis with explicit results and missingness. The
experiment mainline never grants effect, spend, merge, release, deployment,
evaluator, or adoption authority by itself; a separate finite authorization is
required for any live effect.

| Component | Canonical owner | Owns | Explicitly does not own |
|---|---|---|---|
| Common evidence substrate | `engine/src/rwe/` | Frozen task/corpus/schedule identity, budgets, protocol seeds, lifecycle evidence, and missingness | Provider spend, live-effect authority, or a second evaluator |
| Experimental descriptors and candidates | `engine/src/harness_evolution.rs`, `engine/src/harness_evolution/ledger_orchestration.rs` | Immutable Harness, Model, and Strategy descriptors, candidate Harness implementations (e.g. `ledger-orchestrated:provider-independent:v1`), matrix identity, adapter normalization, and explicit `INCOMPARABLE` projections | Runtime scheduling, budget/effect authority, evaluator mutation, or adoption |
| Evaluation and disposition | `engine/src/harness_evolution_eval.rs` | Sealed holdout, evaluator binding, hard gates, candidate/causal/Pareto evidence, and explicit `INCOMPARABLE` outcomes | Active-Harness replacement, merge, release, deployment, or adoption |
| Persistence and audit | `engine/src/storage/local_product_store/` | Experiment evidence/artifacts, budgets, and terminal settlement receipts | Research evaluation or a second truth store |
| Adoption decision | User through an owner-approved Mission/Stage decision | Evidence-backed transfer/replication review and explicit adoption of a new Harness identity | Merge, release, deployment, evaluator replacement, or self-authorized change |

Level-1 evidence and disposition (transfer, replication, and memory+skill) and
Level-2/Meta gates (R4/R5/R6) require complete lower-rung evidence, hard
quality/safety/comparability gates, and explicit authorized adoption before any
change to the active Harness. The research milestone gates are owned by
`docs/ROADMAP.md`; autonomy, testing, review, and merge rules by
`docs/AUTONOMY.md`.

## Merge-dispatch recovery and emergency-stop contract

The merge-dispatch recovery boundary is owned here because it defines durable
authority, external evidence, mutation safety, and restart recovery. A merge
intent binds the repository, PR number, expected base, exact head, workflow
file, `main` ref, and a journal-derived intent key into a unique `dispatch_id`.
The REST `workflow_dispatch` request asks for `return_run_details=true`; its
returned `workflow_run_id`, run URL, and exact binding are durably journaled
before the dispatch is considered settled. A missing or malformed response is
`OUTCOME_UNKNOWN`; it never permits a second dispatch.

Reconciliation first reads the persisted run ID and verifies the workflow
run's ID, dispatch ref (`main`), `event`, workflow path, status, conclusion,
and time fence. Because GitHub reports the dispatch ref's `head_sha` rather
than the workflow input's PR head, the run's complete log must also carry the
exact `PR_NUMBER`/`EXPECTED_HEAD`/`DISPATCH_ID` markers. An old run without
the dispatch marker cannot be attributed to the intent. An empty scan, elapsed
time, or unrelated run cannot prove no effect. A successful run is not merge
success: only the authoritative merged PR and accepted-main readback can prove
success.

If no durable run identity exists, the old dispatch is not converted into a
no-effect fact by elapsed time, an empty run scan, owner assertion, or a
missing log. The approved Mission itself provides the standing, bounded authority
for routine repository-maintenance recovery (`repository_maintenance` grant with
`quarantine_exact_owned_candidate`). Within that standing Mission authority, Steward
autonomously executes `QUARANTINE_EXACT_PR` on the exact bound candidate when all
binding invariants match, without requiring a new per-orphan GitHub OWNER comment.
Historical or manual `steward-orphan-dispatch-recovery:v1` markers on the canonical
control Issue remain fully supported for backwards compatibility and out-of-band
operator intervention, but are not required for routine orphan recovery under an
approved Mission.

Each newly persisted standing-recovery quarantine intent consumes one use of
the Mission grant's canonical `max_uses` ceiling. Restart and read-only
reconciliation of that same intent consume no additional use. If the ceiling
is exhausted before a new intent, Steward records the deterministic exhaustion
fact, performs no mutation, and pauses for a genuinely new owner ceiling; an
elapsed timeout or missing workflow run never changes that accounting.

Before executing autonomous quarantine, Steward re-reads the repository, exact PR/base/head,
accepted main, PR state, and emergency-stop state immediately before the one
authorized quarantine mutation. The accepted-main read is recorded even when
it has advanced beyond the orphan's expected base because the recovery action
is an exact close-only quarantine, not a merge or a rebind. The branch and
evidence are retained.

Exact-candidate quarantine is the fail-closed race-safe mechanism:
`.github/workflows/agent-merge.yml` enforces an open-PR preflight check
(`merge_pr_not_open`), ensuring a delayed old workflow cannot merge a PR that has
been closed by quarantine. After the quarantine mutation, authoritative GitHub
readback decides the external outcome:
- `MERGED`: the old workflow won the race and merged before quarantine. The merge is
  accepted, requiring merged-PR plus accepted-main readback with no replacement candidate;
- `CLOSED_UNMERGED`: quarantine succeeded in closing the PR before merge. A fresh
  replacement candidate is authorized and replanned under the existing Mission approval;
- Anything ambiguous or unavailable remains `OUTCOME_UNKNOWN` / read-only waiting.

A persisted quarantine intent fences repeats across restarts, ensuring idempotent
readback without duplicate close mutations, and no unverified old candidate and fresh
candidate may be merge-eligible concurrently.

Emergency stop remains a hard guard. The service re-reads `agent-emergency-stop`
immediately before quarantine intent logging and before the quarantine mutation,
and never clears it itself. If emergency stop is active, quarantine mutation is
inhibited and execution halts in `EMERGENCY_STOP` while preserving permitted
read-only reconciliation. Routine orphan recovery does not trigger emergency stop;
only genuine emergency stop transitions halt recovery.

## Three-Tier Operational Model

```mermaid
flowchart TD
    U["User Natural Language Goal"] --> S["Autonomous Steward Proposal & Grant"]
    S --> M["MaintenanceMission"]
    M --> G["Stage Integration Boundary"]
    G --> C["WorkCard (Weak Agent Task)"]
    C --> V["Test · Independent Review · CI · Merge"]
    V -->|Incomplete / Retry| G
    V -->|Stage Verified| M
    M -->|Stages Settled & Ledger Terminal| D["Mission Summary"]
```

| Layer | Responsibility | Authority / Decision Maker |
|---|---|---|
| **Mission** | High-level objective, boundary, budget, standing grants, and acceptance criteria | User approves once; Steward executes |
| **Stage** | Discrete, verifiable integration milestone and PR boundary | Autonomous Steward |
| **WorkCard** | Fine-grained, isolated task with exact paths, steps, tests, and evidence | Autonomous Steward schedules; Weak Agent executes |

## Core Module Ownership

| Area | Canonical Owner | Boundary and Invariants |
|---|---|---|
| **Repository Navigation and Context** | `START_HERE.md`, `scripts/project_context.py`, `scripts/session_context.py`, `scripts/check_agent_handoff.py` | Role-based reading routes and on-demand context projection; session checkpoints are local Git-private digests, never parallel authority stores. |
| **API and Composition Root** | `engine/src/main.rs`, `engine/src/http_server/` | Sole startup and composition surface; frozen acyclic dependency topology and strict runtime mode gates. |
| **Workflow Runtime and Scheduler** | `engine/src/workflow/`, `engine/src/scheduler.rs`, `engine/src/scheduler/`, `engine/src/executor_pool.rs`, `engine/src/node_executor.rs` | Sole persisted workflow run, node, lease, retry, pause/kill, and concurrency executor. |
| **Persistence and Audit Store** | `engine/src/storage/local_product_store/` and PostgreSQL backend | Sole SQLite/PostgreSQL transaction, migration, audit, idempotency, evidence, and rollback owner. |
| **Managed-Acceptance and Effect Authority** | `engine/src/storage/local_product_store/managed_acceptance.rs`, `rwe_authority.rs` | Single persistent effect owner; parent effect envelopes, one-use child authorization derivation, spend ledger, terminal settlement, and non-retryable `OUTCOME_UNKNOWN`. |
| **Queue Lease Management** | `engine/src/storage/local_product_store/workflow_runs/queue_lease.rs` | Claim/execute/settle separation; lease transactions commit before external node execution and settle in discrete subsequent transactions. |
| **Workspace and Target Repository Output** | `engine/src/target_repo_output.rs`, `engine/src/storage/local_product_store/product_tasks.rs` | Target default branch is never a workspace; mutations occur in dedicated branch worktrees; patch export requires approved gates. |
| **Autonomous Steward Outer Loop** | `scripts/agent-control/steward_service.py`, `steward.py`, `steward_journal.py`, `steward_workers.py`, `steward_github.py`, `mission_contract.py` | `StewardService` is the sole journal-backed Mission lifecycle writer: authenticated approval, K=2 isolated dispatch, Draft/Ready/CI/review repair, canonical merge dispatch, and PR/head-bound accepted-main readback. `steward.py` is an execution seam only; neither intrudes on Rust ProductStore authority. |
| **Review Convergence Protocol** | `scripts/agent-control/review_convergence.py`, `scripts/agent-control/review_loop/`, `scripts/agent-control/validate_review.py` | R1/R2 substantive review budget; exact `PASS` is the sole merge-authorizing verdict; structured blocker vs deferred note classification. |
| **Wire Contracts and Codegen** | `wire_contract/`, `codegen/`, `engine/src/wire_types.rs` | Canonical schema definitions and deterministic cross-language codegen for Rust, TypeScript, and Python SDKs. |
| **Event Schema and Evidence** | `engine/src/event_schema.rs`, `docs/stage0/events.jsonl` | Canonical event schema validation, idempotency hashing, and Stage-0 event integrity. |
| **Investigation Escalation (`ask_sol`)** | `scripts/ask_sol.py`, `scripts/ask_sol`, `tests/test_ask_sol.py` | Bounded read-only investigation escalation; pre/post worktree dirty state non-mutation verification; per-state consultation budget. |

## Managed Effect and Single Ownership Invariant

The system enforces a strict single-owner rule for all external effects:
1. **Single Persistent Owner**: All effect envelopes, child authorizations, spend ledgers, and terminal settlement receipts are owned exclusively by `LocalProductStore` in Rust `engine/`.
2. **Immutable Parent Envelopes**: Parent effect envelopes bind owner-approved goal, total budget, finite expiration, and target destination.
3. **Bounded Child Authorizations**: One-use child authorizations are derived from a live parent envelope and cannot exceed parent budget, expiration, or target bounds.
4. **No Cross-Stage Leakage**: Authority grants do not automatically transfer across stages. Unconsumed authorizations expire upon stage completion.
5. **No Outcome-Unknown Retries**: Any effect resulting in `OUTCOME_UNKNOWN` is immediately terminalized as non-retryable and halted fail-closed.

## Lease Lifecycle Separation

Lease management strictly separates claim, execution, and settlement:
- **Claim**: An atomic DB transaction claims the queue lease and records initial lease heartbeat.
- **Execute**: The external worker/task executes outside any open database transaction.
- **Settle**: On completion or failure, a separate DB transaction records the terminal settlement and releases the lease.

## Target Repository Isolation

Target repository output operations strictly disallow operating directly on the default branch:
- Work is staged and validated in dedicated detached worktrees on isolated feature branches (`agent/*` or `acp/*`).
- Default branch push is prohibited; changes are exported as patches or Draft PRs requiring explicit verification.

## Final Change Impact Map

The final migration boundary keeps five owners explicit. Calls cross these
boundaries through typed APIs or bounded adapters; ownership does not move with
the call.

```mermaid
flowchart LR
    SCHED["Scheduler"] -->|admit| CLAIM["Claim transaction"]
    CLAIM -->|commit lease + heartbeat| STORE["LocalProductStore\npersistence and audit"]
    CLAIM -->|after claim commit| EXEC["External executor / worker"]
    EXEC -->|result or failure| SETTLE["Settlement transaction"]
    SETTLE -->|record terminal state + release lease| STORE
    API["HTTP/API handlers"] -->|typed transaction views| STORE
    POLICY["ToolPolicy\nregistry + policy snapshots"] -->|validated policy data| STORE
    TASK["ProductTask\nintake + output gate"] -->|workspace-bound output| TARGET["Dedicated branch worktree"]
    STEWARD["agent-control\nSteward outer loop"] -->|repo-maintenance PR/review/CI| GIT["GitHub repository"]
    STEWARD -. must not own .-> STORE
    STEWARD -. must not own .-> SCHED
    TASK -->|effect envelope / settlement| STORE
```

| Owner | Canonical calls and dependencies | Downstream impact | Acceptance invariant and evidence |
|---|---|---|---|
| **Store** | `LocalProductStore::with_transaction` and domain views under `engine/src/storage/local_product_store/` | SQLite/PostgreSQL persistence, audit, idempotency, effects, ProductTask state | One persistent owner for effects and receipts; `managed_acceptance` and PostgreSQL parity tests |
| **Scheduler** | `workflow_runs` uses `queue_lease` for claim, calls the external executor, then records settlement | Admission, concurrency, leases, retries, pause/kill, and run state | Claim transaction commits before external execution; settlement is a later transaction; scheduler/store tests |
| **ToolPolicy** | `tool_execution_policy`, `tool_registry`, and authenticated policy handlers | Capability, allowlist, hook validation, and execution gating | Policy mutations are hash-bound and audited by Store; tool registry and API policy tests |
| **ProductTask** | `product_tasks` transaction view, product-task handlers, and `target_repo_output` | Product intake, approval/output gates, workspace-bound patch export | Target default branch is never a workspace; target-output and golden-path recovery tests |
| **agent-control** | `steward_service.py`, `steward.py`, `steward_journal.py`, `steward_workers.py`, `steward_github.py`, `mission_contract.py`, `codex_hooks/` | Repository-maintenance missions, stages, WorkCards, reviews, PR integration, and guarded merge dispatch | One journal-backed lifecycle writer; authenticated Issue-comment approval; Codex CLI worker/reviewer transport; GitHub remains merge and accepted-main authority; no ProductStore runtime state |

### PR7 Acceptance Scope

The final non-regression check is provider-free and read-only outside the
repository's normal test/build outputs. Its acceptance evidence is owned by
the canonical PR and CI/review records described in `docs/AUTONOMY.md`; this
architecture map records the boundaries under test and is explanatory only.

## Documentation Test

This document carries a bounded, self-contained documentation test. It passes
when every assertion below holds against this document and the code it
describes at the accepted head. An independent reviewer verifies the
assertions directly against the accepted document and code; the focused hygiene
gate is `git diff --check`. This section is documentation-only and grants no
new authority.

1. **Single runtime authority** — the document asserts Rust `engine/` is the
   sole runtime, scheduler, policy, and application-owned storage authority.
2. **Single persistence authority** — the document asserts `LocalProductStore`
   is the sole persistence and audit owner across SQLite and PostgreSQL
   backends.
3. **Outer-loop non-ownership** — the document asserts the Autonomous Steward
   is a repository-maintenance outer loop that must not own the Store or the
   Scheduler.
4. **Single-owner effect rule** — the document asserts all effect envelopes,
   child authorizations, spend ledgers, and terminal settlement receipts are
   owned exclusively by `LocalProductStore`.
5. **Schema version agreement** — the documented `Current version: vN` matches
   `CURRENT_SQLITE_SCHEMA_VERSION` in
   `engine/src/storage/local_product_store/schema.rs`.
6. **Link, do not duplicate** — the document references `docs/AUTONOMY.md` for
   autonomy, testing, review, and merge rules instead of restating them.
7. **Frozen-experiment gate** — the document asserts research evidence on the
   common RWE basis is obtained only through finite frozen canonical
   experiments whose corpus, evaluator, descriptors, schedule, budgets,
   identities, protocol seeds, lifecycle analysis, and results are frozen
   before any run, and that the experiment mainline grants no effect, spend,
   merge, release, deployment, evaluator, or adoption authority by itself.
8. **Level-1/Level-2/Meta gates** — the document asserts Level-1
   (transfer/replication/memory+skill) and Level-2/Meta (R4/R5/R6) disposition
   require complete lower-rung evidence, hard gates, and explicit authorized
   adoption before any change to the active Harness.

Assertions 1-8 are bounded to this document and hold at the accepted head; the
change is documentation-only and adds no new authority.
