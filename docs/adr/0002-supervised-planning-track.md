# ADR 0002: Supervised Planning Track Toward Autonomous Beta

Status: Accepted for planning only; execution remains gated. Batch 7 readiness audit is NO-GO.

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
| Write boundary | Writes are limited to scratch or an isolated worktree/patch area; registered target repositories are not mutated directly. | Not implemented. Existing app behavior does not write target repos. |
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

Current go/no-go: **NO-GO for implementation**.

The current repository does not yet satisfy the Batch 7 prerequisites:

| Prerequisite | Current evidence | Status |
|---|---|---|
| Isolation primitive selected | No constrained process/container/VM primitive is selected. Existing sandbox-like code is logical file-claim tracking only. | Missing |
| Target workspace contract | No harness-owned isolated worktree/scratch lifecycle, source revision evidence, diff capture, or cleanup path is wired. | Missing |
| Approval broker | `workflow_run_approvals` are inert metadata with `execution_authority=disabled`; no scoped future approval authority or pre-execution gate exists. | Missing |
| Rollback | DAG compensation and backup restore helpers exist, but no workspace-level rollback strategy or execution failure-mode tests exist. | Missing |
| Artifact capture | Artifact lifecycle/gate modules are library-level; no persisted execution artifact schema, storage, redaction, access control, or retention path exists. | Missing |
| Provider default-off | Existing env/auth/scope/cost gates keep provider execution default-off. | Satisfied, must remain unchanged |
| No push/merge/deploy/target mutation | Existing boundaries block these behaviors. | Satisfied, must remain unchanged |

The next safe artifact is a Batch 7 implementation plan that selects the isolation primitive, defines target workspace lifecycle and artifact schema, defines approval scopes and gate semantics, defines rollback tests, and updates threat-model controls. That artifact remains documentation/design until separately accepted.

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
- Batch 7 readiness audit blocks implementation until the missing prerequisites above are resolved in a separate accepted plan.

## Reversal Conditions

Revisit this ADR if:

- planning-only code gains execution authority
- target repository writes become possible without a separate approved gate
- sandbox/process/container/VM behavior is implemented before a dedicated design gate
- Batch 7 implementation starts before the Batch 6 contracts receive separate human approval
- module unification requires R8-style file splitting or broad refactoring
- provider calls become default-on or unattended
