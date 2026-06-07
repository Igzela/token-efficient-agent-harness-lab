# ADR 0002: Supervised Planning Track Toward Autonomous Beta

Status: Accepted for planning plus storage-only/read-only metadata. Execution remains gated. Batch 7 Slice A storage-only metadata, Slice B read-only HTTP visibility, Slice C read-only SDK visibility, and Slice D approval-binding contract are implemented; supervised execution runtime remains NO-GO.

Date: 2026-06-05

## Context

The Rust + TypeScript cutover is complete, local small-team productization is complete for the current scope, and the repository is in autonomous maintainer mode for safe local work. The next useful track is to prepare a supervised autonomous beta without accidentally crossing existing hard boundaries.

The repository already contains several planning and workflow concepts:

- active dispatch, local API, SQLite, dashboard, provider-gated runtime, SDK, and ops surfaces
- partial/library workflow and orchestration modules, including `WorkflowGraph`, `DAGState`, and scheduling-local `DagState`
- dormant `app_layer` parity reference code from the retired Python implementation
- explicit prohibitions on real autonomous workers, target repository writes, sandbox/process/container/VM execution, deploy/merge controls, and default-on provider calls

## Decision

Add a supervised planning track that advances in gated batches:

1. Confirm governance scope and update the forward plan.
2. Classify modules as active, partial, library-only, dormant, or legacy-delete-candidate.
3. Choose one canonical DAG/workflow model and define migration adapters.
4. Implement a read-only planner only after the model decision is approved.
5. Persist workflow planning state only for app-owned records.
6. Integrate quality, routing, retry, and observability as recommendations or blockers only.
7. Document sandbox, target workspace, approval broker, rollback, and artifact capture before any execution implementation.
8. Enter supervised execution beta only after explicit human approval for that batch.

For this track, a read-only planner is not a runtime autonomous worker. It may generate non-executable plans and app-owned planning state. It must not spawn workers, execute tasks, mutate target repositories, manage deployment, or grant approval authority.

Batch 3 implements this first planning-only surface:

- `engine/src/read_only_planner.rs` generates a `read_only_plan.v1` record with canonical `WorkflowGraph`, task analysis, validation, execution-order waves, and explicit disabled-boundary fields.
- `POST /api/v1/plans` creates an app-owned plan record in local SQLite; `GET /api/v1/plans` and `GET /api/v1/plans/{plan_id}` read stored planning metadata.
- `workflow_plans` stores the plan in `LocalProductStore`; export/import/integrity and SDK methods cover the new state.
- The endpoint requires `dispatch:read` under protected mode and does not call providers, execute workers, write target repositories, start sandbox/process/container/VM isolation, or expose approve/run/deploy/merge controls.

Batch 4 implements inert durable state:

- `workflow_runs`, `workflow_run_nodes`, `workflow_run_edges`, `workflow_run_events`, and `workflow_run_approvals` persist app-owned workflow metadata in `LocalProductStore`.
- `POST /api/v1/workflow-runs` creates run metadata from an existing read-only plan; list/detail endpoints read stored metadata.
- Event, approval, resume, and cancel endpoints record metadata and status only. Resume/cancel do not call `WorkflowEngine`, spawn workers, execute subprocesses, cancel processes, write targets, call providers, or grant approval/run/deploy/merge authority.
- Export/import/integrity, operations counts, and TypeScript/Python SDK methods cover the new state.

Batch 5 implements recommendation-only advisory metadata:

- `engine/src/read_only_planner.rs` adds a top-level `advisory` record to read-only plans with quality preflight status, cold-start routing recommendation, retry-policy metadata, observability hints, blockers, and recommendations.
- The advisory path uses pure `TaskAnalysis`, `DynamicTierSelector` cold-start fallback, `BudgetManager` math, `RetryPolicy` serialization, and observability schema constants. It does not construct `RetryFallbackManager`, call `Provider::invoke`, run `EvaluationRunner`, execute workers, write targets, or grant approval/run/deploy/merge authority.
- The advisory is stored inside the existing app-owned `plan_json`; no target repository state or runtime execution state is modified.

Batch 6 documents the supervised execution design gate only:

- The contracts below define what a later supervised execution beta would have to implement and prove before Batch 7 can start.
- They are planning artifacts only. They do not select or implement a sandbox, create a target workspace, wire an approval broker, apply rollback, capture execution artifacts, run workers, write target repositories, call providers, or expose approve/run/deploy/merge controls.

## Batch 6 Design Gate Contracts

Batch 7 is not approved by this ADR. A later Batch 7 request must satisfy these contracts with implementation design, tests, and separate human approval before any execution code is added.

### Sandbox, Process, Container, Or VM Isolation

| Property | Design contract | Current implementation status |
|---|---|---|
| Isolation primitive | Pick one primitive before Batch 7: constrained process, container, or VM/microVM. Record why weaker primitives are sufficient or rejected. | Not implemented. |
| Resource limits | Define CPU, memory, disk, process count, and wall-clock timeout caps before any run starts. | Not implemented. |
| Network policy | Default-deny egress. Any allowlist must be explicit, narrow, audited, and unavailable by default. | Not implemented. |
| Filesystem boundary | Target repository is mounted read-only. Only harness-owned scratch/artifact dirs may be writable. | Not implemented. Existing target-repo boundary remains read-only. |
| Failure handling | Timeout, isolation error, or policy violation must stop the run, persist an audit/event record, and trigger rollback/capture review. | Not implemented. |

### Target Workspace Contract

| Property | Design contract | Current implementation status |
|---|---|---|
| Workspace source | Create an isolated harness-owned workspace from a selected target revision or read-only target mount. | Not implemented. |
| Write boundary | Writes are limited to scratch or an app-owned detached workspace/patch area outside registered target repositories; registered target repositories are not mutated directly. | Not implemented. Existing app behavior does not write target repos. |
| Concurrency | One active execution workspace per target/revision unless a conflict policy proves isolation. | Not implemented. |
| Lifecycle | Workspace creation, capture, rollback, retention, and cleanup must be explicit lifecycle states. | Not implemented. |
| Integrity evidence | Record source revision, workspace id, writable paths, and final diff/artifact inventory before human review. | Not implemented. |

### Approval Broker Contract

| Property | Design contract | Current implementation status |
|---|---|---|
| Approval trigger | Any node with human-review, target-write, provider, sandbox, high-risk, or cost gate must block before execution. | Inert approval metadata only from Batch 4. |
| Approver identity | Approval requires an authenticated local operator with an explicit future approval scope. | Not implemented. |
| Decision storage | Store decision, approver id, timestamp, scope, affected nodes, and expiry before execution begins. | Inert approval rows exist, not wired to execution. |
| Revocation | Revocation before start blocks execution; revocation during execution must stop or quarantine the run. | Not implemented. |
| Auditability | Approval, denial, expiry, revocation, and override attempts must emit immutable app-owned events. | Inert events exist, not execution-gating. |

### Rollback Strategy

| Property | Design contract | Current implementation status |
|---|---|---|
| Rollback trigger | Failure, timeout, policy violation, approval denial/revocation, or operator cancel. | Not implemented. |
| Scope | Restore app-owned run state and discard or quarantine workspace changes; never repair by writing directly to target repos. | Not implemented. |
| Mechanism | Use explicit snapshots/checkpoints plus graph-level compensation where applicable. `DAGManager::compensate()` may inform design but is not an execution rollback engine today. | Library logic exists; not wired. |
| Atomicity | Define all-or-nothing state transitions. Partial rollback must end in a blocked/quarantined state with evidence. | Not implemented. |
| Verification | Post-rollback checks must verify app-owned state integrity and workspace cleanliness. | Not implemented. |

### Artifact Capture

| Property | Design contract | Current implementation status |
|---|---|---|
| Capture scope | Planned inputs, approval decisions, stdout/stderr, provider audit refs if any, diffs, generated files, test logs, screenshots when applicable, and rollback evidence. | Not implemented. |
| Storage owner | Store under app-owned SQLite/filesystem paths, not inside registered target repositories. | Not implemented. |
| Redaction | Apply secret redaction before operator-facing display or export. | Existing audit/provider redaction exists; execution artifacts not implemented. |
| Access control | Read-only artifact access must require read scope; destructive cleanup must require explicit admin scope and confirmation. | Not implemented. |
| Retention | Define retention, export, and cleanup rules before Batch 7. | Not implemented. |

### Batch 7 Go/No-Go Prerequisites

Batch 7 may start only after a separate human-approved implementation plan proves:

- selected isolation primitive and threat model coverage
- target workspace contract with read-only target boundary and writable scratch/patch area
- approval broker wired to a pre-execution gate
- rollback strategy with failure-mode tests
- artifact capture schema/storage/redaction/access rules
- provider execution remains default-off and separately gated
- no automatic push, merge, deploy, or target-repo mutation

### Batch 7 Readiness Audit

Current go/no-go: **NO-GO for supervised execution runtime**.

The current repository now has a storage-only Batch 7 Slice A for app-owned workspace/artifact metadata, a read-only Batch 7 Slice B HTTP visibility surface, read-only Batch 7 Slice C SDK wrappers, and a docs-only Slice D approval-binding contract. It still does not have workspace directory creation, patch generation, approval-broker enforcement, rollback execution, artifact file capture, dashboard controls for this surface, create/update/delete supervised-patch routes, or supervised execution runtime controls:

| Prerequisite | Current evidence | Status |
|---|---|---|
| Isolation primitive selected | This ADR selects an app-owned detached patch workspace/snapshot as the first Batch 7 primitive. Slice A records only metadata and path-boundary evidence. It explicitly rejects registered-target `git worktree add` because that mutates target repository `.git/worktrees` metadata. No process/container/VM execution primitive is selected because Slice A does not run untrusted code or target commands. | Storage-only metadata implemented; execution primitive not selected |
| Target workspace contract | Slice A adds app-owned SQLite `supervised_patch_workspaces` metadata with source revision, target path, workspace path, lifecycle status, boundary JSON, and tests that reject workspace paths inside registered target repositories, including import bypass attempts. Slice B exposes workspace metadata through read-only GET routes. Slice C exposes those GET routes through TypeScript/Python SDKs. It does not create workspace directories or copy target files. | Metadata schema/storage/API/SDK visibility implemented; lifecycle runtime missing |
| Approval broker | This ADR defines future patch-generation and patch-review gate semantics bound to operator identity, scope, expiry, and diff/workspace evidence. Slice D specifies the `supervised_patch_approval_binding.v1` contract and blocking rules. `workflow_run_approvals` remain inert metadata with `execution_authority=disabled`. | Contract specified; implementation missing |
| Rollback | This ADR defines rollback as app-owned workspace discard/quarantine plus terminal run state and evidence. DAG compensation and backup restore helpers are not execution rollback engines. | Design specified; tests missing |
| Artifact capture | Slice A adds app-owned SQLite `supervised_patch_artifacts` metadata with patch hash, normalized changed-file inventory, redaction status, storage refs, export/import, integrity, and stats coverage. Slice B exposes artifact metadata through read-only GET routes. Slice C exposes those GET routes through TypeScript/Python SDKs. It does not create patch files, run redaction, expose patch files, or gate export/review. | Metadata schema/storage/API/SDK visibility implemented; capture runtime missing |
| Provider default-off | Existing env/auth/scope/cost gates keep provider execution default-off. | Satisfied, must remain unchanged |
| No push/merge/deploy/target mutation | Existing boundaries block these behaviors. | Satisfied, must remain unchanged |

The next safe action is a separate, test-first Slice E request. No workspace creation, patch generation, execution, provider, target-write, apply, push, merge, deploy, create/update/delete supervised-patch route, or runtime-worker code is approved by Slice A/B/C/D.

### Batch 7 Implementation Plan Artifact

Status: **documentation/design only**. This section records the smallest acceptable supervised-beta implementation direction. It does not authorize code, runtime workers, process/container/VM isolation, target repository writes, provider calls, push/merge/deploy controls, or automatic execution.

#### Selected First-Slice Primitive

The first supervised-beta primitive is an **app-owned detached patch workspace/snapshot**:

- The target repository is read only. The system may read a selected source revision and record commit/tree evidence.
- The writable workspace lives under an app-owned ACP data path outside the registered target repository, for example `<acp_data_dir>/workspaces/<workspace_id>`.
- If a default ACP data path would resolve inside the registered target repository, the implementation must relocate it to an approved app-owned directory or reject workspace creation.
- The app-owned workspace is detached from the registered target repository. The first slice must not run `git worktree add` against the registered target repository because that writes `.git/worktrees` metadata in the target repo.
- The first slice may generate a patch artifact by comparing the app-owned workspace manifest with the recorded source revision. It must not apply the patch to the registered target repository.
- The first slice must not run target code, test commands, package managers, shell commands, external CLIs, provider calls, containers, VMs, or autonomous workers. If a later slice needs command execution, it requires a separate isolation primitive decision and threat-model update.

This primitive is weaker than process/container/VM isolation by design. It is sufficient only for human-approved app-owned patch artifact generation because no untrusted process execution is allowed in the first slice. It is not sufficient for target test execution, build execution, provider-backed implementation, or unattended autonomous work.

#### Target Workspace Lifecycle

| State | Meaning | Required evidence |
|---|---|---|
| `requested` | Operator requests a supervised patch-generation workspace for a plan/run. | plan id, run id, target id, requested source revision, requester id |
| `source_recorded` | The target revision is inspected read-only and pinned. | commit/tree hash, dirty-state policy result, readable path inventory summary |
| `workspace_created` | App-owned detached workspace/snapshot is created outside the registered target repository. | workspace id, app-owned path, writable path inventory |
| `patch_prepared` | Proposed changes exist only in the app-owned workspace. | changed-file inventory, patch hash, advisory snapshot hash |
| `review_blocked` | Human review is required before export or any later action. | diff/artifact refs, approval requirement, expiry |
| `approved_for_artifact_export` | Operator approves review of the patch artifact only. | approver id, scope, timestamp, expiry, reviewed patch hash |
| `rejected` | Operator rejects the patch artifact. | denial reason, terminal event |
| `quarantined` | Failure or policy violation preserves workspace for diagnosis without target mutation. | failure event, quarantine path/ref, cleanup requirement |
| `cleaned` | App-owned workspace is removed after retention or explicit cleanup. | cleanup event, remaining artifact refs |

Concurrency policy for the first implementation slice: one active app-owned workspace per target id and source revision unless a later approved conflict policy proves isolation. Registered target repositories remain read-only throughout the lifecycle.

#### Approval Broker Scope And Gate

Future implementation must treat approval as scoped metadata bound to evidence, not blanket execution authority:

- Candidate approval scope: `workflow:patch_review`.
- Required binding: plan id, run id, workspace id, target id, source revision, patch hash, changed-file inventory hash, approver id, decision timestamp, and expiry.
- Stale, revoked, wrong-scope, wrong-hash, or wrong-identity approvals must block artifact export and emit app-owned events.
- Approval grants permission to expose/export the captured patch artifact for human use only. It does not grant permission to run commands, apply patches to target repos, push, merge, deploy, call providers, or start workers.
- Batch 4 `workflow_run_approvals` remain inert until a separate implementation slice wires and tests this gate.

#### Rollback And Failure Tests

For the first slice, rollback never repairs a target repository because the target repository is never mutated. Rollback means:

- mark the app-owned workspace/run terminal as `rejected`, `rolled_back`, or `quarantined`
- delete or quarantine the app-owned workspace according to retention policy
- preserve redacted artifact and event evidence
- verify the registered target repository path and `.git` metadata were not changed by the operation

Required future tests before code is acceptable:

- workspace create failure leaves no writable target residue
- workspace path canonicalization rejects paths inside the registered target repository
- wrong source revision or dirty-state policy blocks workspace creation
- stale/wrong-scope approval blocks patch artifact export
- rollback removes or quarantines the app-owned workspace and records a terminal event
- registered target repository `.git` metadata is unchanged, including no registered `git worktree` metadata
- provider gates remain default-off and no provider call path is reachable
- no push, merge, deploy, apply, run, or execute control is exposed

#### Artifact Capture Schema And Access

Future implementation should persist patch artifacts in app-owned SQLite/filesystem storage, not target repositories. Minimum schema fields:

| Field | Meaning |
|---|---|
| `schema_version` | `supervised_patch_artifact.v1` |
| `artifact_id` | app-owned artifact id |
| `plan_id` / `run_id` / `workspace_id` | source planning and workspace refs |
| `target_id` / `source_revision` / `source_tree_hash` | read-only target evidence |
| `workspace_manifest_hash` | manifest of app-owned workspace inputs |
| `patch_hash` | hash of generated patch content |
| `changed_files` | normalized changed-file inventory |
| `advisory_snapshot_hash` | hash/ref for recommendation-only advisory state used during review |
| `approval_refs` | approval/denial ids bound to this artifact |
| `redaction_status` | pending, redacted, or failed |
| `storage_refs` | app-owned file/db refs only |
| `retention_expires_at` | cleanup deadline |

Access rules:

- read requires a future read scope compatible with `dispatch:read`
- export/review requires the future `workflow:patch_review` gate above
- destructive cleanup requires explicit admin scope and confirmation
- operator-facing display/export must run secret redaction first

#### Explicit Non-Goals For Batch 7 First Slice

The implementation plan still forbids:

- direct target repository mutation, including registered-target `git worktree add`
- process, shell, container, VM, package-manager, test, or external CLI execution
- real autonomous workers or concurrent runtime workers
- default-on provider calls or unattended provider calls
- patch apply, push, merge, deploy, run, execute, or release controls
- hosted/cloud production behavior

### Batch 7 Slice A: Storage-Only Metadata

Status: **implemented as inert app-owned metadata only**.

Slice A adds:

- SQLite schema version 3 with `supervised_patch_workspaces` and `supervised_patch_artifacts`.
- `LocalProductStore` methods to record/list/get/import/export supervised patch workspace and artifact metadata.
- Boundary JSON on workspace records proving `metadata_only`, `execution_authority=disabled`, `target_repository_writes=disabled`, `workspace_directory_creation=not_performed`, `registered_git_worktree=forbidden`, provider calls disabled, and push/merge/deploy/apply disabled.
- Path validation that canonicalizes the registered target repository and planned workspace path without creating the workspace directory, then rejects workspace paths inside the registered target repository.
- Changed-file validation for artifact metadata that rejects absolute, empty, or traversal paths.
- Integrity, stats, and export/import coverage for the new app-owned tables.
- Seven focused Rust tests plus full Rust verification; Slice A Rust test count reached 1200 pass.

Slice A deliberately does not add HTTP routes, SDK methods, dashboard UI, workspace directory creation, target file copying, patch file generation, redaction runtime, approval-broker wiring, rollback execution, command execution, provider calls, target repository writes, sandbox/process/container/VM execution, workers, or apply/push/merge/deploy/run controls. Claude Code recommended exposing HTTP/SDK routes for consistency with prior batches; the controller rejected that for Slice A because it would expand the operator surface before the approval/export gate is designed.

### Batch 7 Slice B: Read-Only HTTP Visibility

Status: **implemented as GET-only metadata views**.

Slice B adds:

- `/api/v1/supervised-patch/workspaces` and `/api/v1/supervised-patch/workspaces/{workspace_id}` for workspace metadata inspection.
- `/api/v1/supervised-patch/artifacts` and `/api/v1/supervised-patch/artifacts/{artifact_id}` for artifact metadata inspection.
- `dispatch:read` authorization on every route.
- Response boundary fields proving `metadata_only=true` and `execution_authority=disabled`.
- OpenAPI entries for all four routes.
- Four focused Rust HTTP tests plus full Rust verification; current Rust test count is 1204 pass.

Slice B deliberately does not add POST/PUT/DELETE routes, SDK methods, dashboard UI, workspace directory creation, target file copying, patch file generation, redaction runtime, approval-broker wiring, rollback execution, command execution, provider calls, target repository writes, sandbox/process/container/VM execution, workers, or apply/push/merge/deploy/run controls.

### Batch 7 Slice C: Read-Only SDK Visibility

Slice C adds TypeScript and Python REST SDK methods for the four existing Slice B GET-only supervised patch metadata routes:

- `GET /api/v1/supervised-patch/workspaces`
- `GET /api/v1/supervised-patch/workspaces/{workspace_id}`
- `GET /api/v1/supervised-patch/artifacts`
- `GET /api/v1/supervised-patch/artifacts/{artifact_id}`

Slice C also adds hand-maintained TypeScript response types for `supervised_patch_workspace.v1` and `supervised_patch_artifact.v1`, plus SDK tests that verify GET-only methods, limit query parameters, and path-segment encoding.

Slice C deliberately does not add Rust runtime/API route changes, POST/PUT/DELETE SDK methods, dashboard UI, workspace directory creation, target file copying, patch file generation, redaction runtime, approval-broker wiring, rollback execution, command execution, provider calls, target repository writes, sandbox/process/container/VM execution, workers, or apply/push/merge/deploy/run controls.

### Batch 7 Slice D: Approval Binding Contract

Status: **implemented as documentation/design only**.

Slice D defines the future evidence-bound approval record that must exist before any patch artifact can become export-eligible. It does not add tables, routes, SDK methods, dashboard UI, approval broker wiring, export runtime, or execution authority.

Future approval binding schema:

| Field | Meaning |
|---|---|
| `schema_version` | `supervised_patch_approval_binding.v1` |
| `binding_id` | app-owned approval binding id |
| `workspace_id` | supervised patch workspace id under review |
| `artifact_id` | supervised patch artifact id under review |
| `plan_id` / `run_id` | source planning and inert run refs |
| `target_id` | registered target id |
| `source_revision` / `source_tree_hash` | read-only source evidence copied from workspace/artifact metadata |
| `patch_hash` | reviewed artifact patch hash |
| `changed_files_hash` | hash of normalized `changed_files` inventory |
| `approver_id` | authenticated local operator id |
| `approver_scope` | future scope used for this decision; candidate value `workflow:patch_review` |
| `decision` | `requested`, `approved`, `rejected`, `expired`, or `revoked` |
| `decision_timestamp` | local decision timestamp |
| `expires_at` | approval expiry timestamp; export must block after this time |
| `revoked_at` | revocation timestamp, or null |
| `stale_reason` | reason export is blocked, or null when binding is export-eligible |
| `metadata_only` | true |
| `execution_authority` | `disabled` |
| `patch_apply_authority` | `disabled` |

Validation rules for any future implementation:

- `workspace_id` must resolve to a Slice A workspace record.
- `artifact_id` must resolve to a Slice A artifact record for the same `workspace_id`, `run_id`, `plan_id`, `target_id`, and `source_revision`.
- `patch_hash` must equal the artifact metadata `patch_hash`.
- `changed_files_hash` must be computed from the normalized artifact `changed_files` array; mismatches block export.
- `approver_id` must identify an active local team member at decision time.
- `approver_scope` must include the future `workflow:patch_review` scope. This scope is not currently part of the runtime scope list and must not be granted until a separate implementation slice adds and tests it.
- `decision=approved` is required for export eligibility.
- `expires_at` must be in the future at export time.
- Non-null `revoked_at` blocks export.
- Any stale, wrong-scope, wrong-hash, wrong-artifact, wrong-workspace, wrong-identity, expired, revoked, or rejected binding must block export and emit an app-owned event/audit record.

State transitions:

| From | To | Meaning |
|---|---|---|
| `requested` | `approved` | Operator approves review/export of this exact artifact evidence. |
| `requested` | `rejected` | Operator denies review/export. |
| `approved` | `expired` | Time-based expiry makes approval unusable. |
| `approved` | `revoked` | Operator/admin revokes approval before export. |
| any non-terminal state | `rejected` | Operator denies after review. |

Export eligibility requires all of:

- binding validates against current workspace/artifact metadata
- `decision=approved`
- `approver_scope=workflow:patch_review`
- `expires_at` has not passed
- `revoked_at` is null
- `stale_reason` is null
- artifact `redaction_status=redacted`
- artifact and workspace still report `metadata_only=true`, `execution_authority=disabled`, and `patch_apply_authority=disabled` where applicable

Slice D deliberately does not implement approval storage, route enforcement, export, redaction, dashboard controls, workspace creation, patch generation, rollback execution, command execution, provider calls, target repository writes, sandbox/process/container/VM execution, workers, or apply/push/merge/deploy/run controls.

## Boundaries

This ADR does not approve:

- real autonomous workers or concurrent runtime workers
- target repository writes
- sandbox/process/container/VM execution or isolation runtime
- deploy, merge, apply, run, or execute controls
- default-on provider calls
- provider productionization beyond the existing explicit env-gated local beta path
- subprocess expansion beyond the existing explicit CLI executor path
- cloud SaaS, hosted production, or remote user-facing deployment

Design documents for sandbox, target workspace, approval broker, rollback, or artifact capture are allowed only as planning artifacts. Code that implements those capabilities requires a later batch gate and explicit human approval.

## Canonical Model Direction

Batch 2 selects `WorkflowGraph` as the canonical planning model because it already carries workflow/node status, budget, cost, result, and resume/cancel-oriented semantics. `DAGState` remains the graph-mutation model for versioned proposals, approval-aware mutation, and rollback. Scheduling-local `DagState` remains a concurrency view for file-overlap scheduling.

This does not approve R8, file splitting, or broad runtime refactoring.

## Batch 2 Model Contract

Canonical direction:

- `WorkflowGraph` is the canonical planning, persistence, and future read-only planner model.
- `DAGState` is an adapter source/target for graph mutations, not the canonical plan record.
- `DagState` is an adapter target for concurrency scheduling, not the canonical plan record.
- No existing Rust modules are moved, split, or wired by this decision.

Field correspondence for later adapters:

| Canonical field | `DAGState` mapping | `DagState` mapping | Notes |
|---|---|---|---|
| `WorkflowGraph.workflow_id` | `DAGState.dag_id` | `DagState.dag_id` | Adapter must define deterministic id policy. |
| `WorkflowGraph.dispatch_id` | not present | not present | Planner must supply dispatch/planning id separately. |
| `WorkflowGraph.status` | not graph-level semantic equivalent | not graph-level semantic equivalent | Keep workflow lifecycle canonical in `WorkflowGraph`. |
| `WorkflowGraph.nodes[].node_id` | `DAGNode.node_id` | `DagNode.node_id` | Direct id mapping. |
| `WorkflowGraph.nodes[].task_type` | `DAGNode.node_type` or metadata | `DagNode.node_type` | Adapter must preserve current task type semantics. |
| `WorkflowGraph.nodes[].status` | `DAGNode.status` | `DagNode.status` | Status strings need validation at adapter boundary. |
| `WorkflowGraph.nodes[].budget` / `cost_incurred` | not present | not present | Planning/persistence-only fields stay canonical. |
| `WorkflowGraph.nodes[].assigned_agent_id` | not present | not present | Later planner can keep this `None`; no worker authority implied. |
| `WorkflowGraph.edges[].edge_id` | `DAGEdge.edge_id` | `DagEdge.edge_id` | Direct id mapping. |
| `WorkflowGraph.edges[].from_node_id` | `DAGEdge.from_node` | `DagEdge.from_node` | Naming bridge only. |
| `WorkflowGraph.edges[].to_node_id` | `DAGEdge.to_node` | `DagEdge.to_node` | Naming bridge only. |
| `WorkflowGraph.edges[].edge_type` | `DAGEdge.dependency_type` | `DagEdge.dependency_type` | Adapter must map `hard`/`soft`/`artifact` to workflow edge semantics. |
| Version metadata | `DAGState.version` | `DagState.version` | Do not add a `WorkflowGraph.version` field in Batch 2. Decide in Batch 3 only if persistence tests require it. |

Batch 3 adapter design constraints:

- Batch 3 implemented only the `WorkflowGraph` to persisted app-owned SQLite row path.
- Preserve existing module files; no R8-style split.
- Keep adapters planning-only and deterministic.
- Do not use adapters to start execution, spawn workers, write targets, run sandboxes, or grant approval authority.
- Batch 4 implemented durable workflow run/node/edge/event/approval records only as inert app-owned state. It did not add runtime workers, execution resume/cancel authority, target writes, or sandbox behavior.
- Batch 5 added quality/routing/retry/observability records only as recommendation/block/status metadata. It does not call providers, retry provider execution, route live workers, or start execution.

## Consequences

- `docs/NEXT_DECISION.md` remains the single forward-plan surface.
- `docs/MODULE_MAP.md` records reachability classes so later batches do not mistake dormant or library-only code for active runtime.
- Batch 3 and later implementation must be small, test-first, and scoped to planning-only behavior unless the user approves a broader batch.
- Any future supervised execution beta must use a separate approval gate and threat model before implementation.
- Batch 6 makes the future execution gate concrete, but it is not itself implementation authority.
- Batch 7 Slice A/B/C/D implements only app-owned storage metadata, read-only HTTP views, read-only SDK wrappers, and approval-binding design for the first supervised patch artifact slice; supervised execution runtime remains blocked until a separate approved, test-first batch.

## Reversal Conditions

Revisit this ADR if:

- planning-only code gains execution authority
- target repository writes become possible without a separate approved gate
- sandbox/process/container/VM behavior is implemented before a dedicated design gate
- Batch 7 implementation starts before the Batch 6 contracts receive separate human approval
- module unification requires R8-style file splitting or broad refactoring
- provider calls become default-on or unattended
