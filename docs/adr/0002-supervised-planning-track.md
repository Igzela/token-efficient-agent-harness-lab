# ADR 0002: Supervised Planning Track Toward Autonomous Beta

Status: Accepted for planning only; execution remains gated.

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

Batch 2 should treat `WorkflowGraph` as the likely canonical planning model because it already carries workflow/node status, budget, cost, result, and resume/cancel-oriented semantics. `DAGState` should remain the graph-mutation model until an adapter is approved. Scheduling-local `DagState` should remain a concurrency view until an adapter is approved.

This does not approve R8, file splitting, or broad runtime refactoring.

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
