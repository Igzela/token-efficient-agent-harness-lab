# ADR 0002: Supervised Planning Track Toward Autonomous Beta

Status: Accepted for planning only; execution remains gated. Batch 4 inert durable state implemented.

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
- Batch 5 may add quality/routing/retry/observability records only as recommendation/block/status metadata. It must not call providers, retry provider execution, route live workers, or start execution.

## Consequences

- `docs/NEXT_DECISION.md` remains the single forward-plan surface.
- `docs/MODULE_MAP.md` records reachability classes so later batches do not mistake dormant or library-only code for active runtime.
- Batch 3 and later implementation must be small, test-first, and scoped to planning-only behavior unless the user approves a broader batch.
- Any future supervised execution beta must use a separate approval gate and threat model before implementation.

## Reversal Conditions

Revisit this ADR if:

- planning-only code gains execution authority
- target repository writes become possible without a separate approved gate
- sandbox/process/container/VM behavior is implemented before a dedicated design gate
- module unification requires R8-style file splitting or broad refactoring
- provider calls become default-on or unattended
