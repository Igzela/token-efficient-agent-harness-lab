# Stage 4 DAG Mutation Spec

## Purpose

The Dynamic DAG Manager owns the in-memory representation of runtime task dependencies and the auditable mutation protocol used to evolve that graph. It must preserve acyclicity, enforce approval gates, apply size limits, and represent rollback as a compensating mutation event rather than deleting or rewriting history.

## Data Model

### DAGNode

`DAGNode` represents one graph vertex.

- `node_id`: stable caller-provided identifier
- `task_id`: linked task identifier or `None` for placeholder/control nodes
- `node_type`: `task`, `gate`, `decision`, or `merge`
- `status`: `pending`, `running`, `completed`, `failed`, `skipped`, or `cancelled`
- `tier`: execution/model tier label
- `metadata`: deterministic node configuration

### DAGEdge

`DAGEdge` represents a directed dependency.

- `edge_id`: stable caller-provided identifier
- `from_node`: predecessor node id
- `to_node`: successor node id
- `dependency_type`: `hard`, `soft`, `artifact`, or `approval`
- `status`: `pending`, `satisfied`, or `violated`

### DAGState

`DAGState` is an immutable graph snapshot.

- `dag_id`
- `version`
- `nodes`
- `edges`
- `created_at`
- `updated_at`

### DAGMutation

`DAGMutation` or equivalent proposal/result records describe intended state changes.

- `mutation_id` or `proposal_id`
- `dag_id`
- `mutation_type`
- `target_node_id`
- `target_edge_id`
- `payload`
- `reason`
- `requires_approval`
- `status`

Every state-changing operation produces an auditable event or auditable mutation record. Rejected mutations are also auditable because they explain why state did not change.

## Supported Mutations

- `add_node`: add a new node; `node_id` must not already exist.
- `remove_node`: remove a node only when connected edges are handled and approval rules permit it.
- `split_node`: replace one pending node with a deterministic subgraph and compensating edge rewrites.
- `retry_node`: create or mark a retry path for failed work without erasing the failed node history.
- `pause_node`: move a runnable node to a paused state or add a pause gate.
- `resume_node`: remove the pause gate or restore the paused node to pending/runnable state.
- `replace_edge`: replace an existing edge with a new edge definition; equivalent to auditable remove/add edge behavior.
- `rollback`: append compensating mutation records to return the DAG to an earlier logical state without deleting historical mutations.

Implementations may expose narrower method names such as `add_edge`, `remove_edge`, `rewire_edge`, or `update_node` if they preserve the protocol semantics above.

## Approval Rules

Approval is required for mutations that:

- Explicitly set `requires_approval`.
- Remove, pause, split, retry, or replace dependencies for a `running` or `completed` node.
- Rewire or replace an edge whose predecessor is `completed`.
- Reduce auditability, hide failed work, or alter a node that has already produced artifacts.
- Exceed configured DAG size limits or materially expand scope.

Unapproved mutations that require approval must be rejected with a deterministic reason and no graph mutation.

## Cycle Detection

All edge additions, edge replacements, node splits, and rollbacks that affect edges must run cycle detection before becoming the current DAG state. Cycle detection must be deterministic, typically DFS over sorted node ids and sorted adjacency lists.

If a mutation would introduce a cycle, it is rejected and the current DAG state remains unchanged.

## Size Limits

The manager must support explicit max node and edge limits. Recommended defaults for Stage 4 planning:

- `max_nodes`: 1,000
- `max_edges`: 5,000

Mutations that would exceed limits are rejected unless a future stage introduces an approved override. The rejection is auditable.

## Rollback

Rollback is forward-only. It must not delete events, truncate mutation history, rewrite checkpoints, or mutate committed event logs. A rollback produces compensating mutation events that describe the inverse logical action:

- Added node -> compensating remove or cancel marker
- Removed node -> compensating restore marker
- Added edge -> compensating remove edge
- Removed edge -> compensating restore edge
- Replaced edge -> compensating replace edge with prior endpoints
- Paused node -> compensating resume node

The resulting state may match an earlier graph snapshot, but the audit trail remains append-only.

## Determinism

- IDs are caller-provided or content-derived.
- Topological order breaks ties lexicographically by node id.
- Cycle detection and validation process nodes/edges in sorted order.
- No model calls, network calls, randomness, real worker execution, or wall-clock reads are permitted.
