# ADR 0001: Language Migration to Rust Core and TypeScript Surfaces

Status: Accepted for migration preparation; implementation remains gated.

Date: 2026-05-28

## Context

The repository has a stable Python stdlib-first harness with 2089 passing tests. The dispatcher architecture book previously recorded "Python core retained" and treated Go/Rust rewrite as a future reversal only if performance became a bottleneck.

The approved migration direction is now different:

| Layer | Language | Reason |
| --- | --- | --- |
| Core kernel: dispatch, eval, routing | Rust | Deterministic engine, type safety, future embedding in the API process |
| API gateway | Rust with axum | Same process as the core kernel and no Python process boundary |
| Dashboard frontend | TypeScript with Next.js | Frontend ergonomics and ecosystem fit |
| SDK | TypeScript plus Python | Client SDKs should match customer languages |
| Deployment and Docker | Shell plus Dockerfile | Operational glue does not need Rust |

This creates a direct conflict with the prior architecture-book decision. The conflict must be resolved before implementation so future agents do not block Rust work by following the older decision table.

## Decision

Adopt Rust as the target language for the deterministic core and API gateway, with TypeScript for dashboard and SDK surfaces and Python SDK support retained.

The migration must proceed in this order:

1. Repair baseline and handoff drift.
2. Record this ADR and update the architecture-book decision table.
3. Freeze the dispatch wire contract before writing Rust code.
4. Implement the first Rust parity kernel only after contract freeze, starting with `event_schema`, `task_analyzer`, and `dispatch_decision`.

The existing Python implementation remains the reference implementation until Rust parity tests prove equivalent behavior against frozen JSON contracts.

## Boundaries

This ADR does not approve real provider calls, real sandbox/process/container/VM execution, target-repo writes, deployment, runtime autonomous workers, provider failover, or executable UI controls.

The TypeScript dashboard target is an accepted language direction, not approval to create a production product surface. Any Next.js dashboard implementation must keep the existing read-only/non-executable policy unless separately approved.

## Consequences

- The architecture book's "Python core retained" decision is superseded by this ADR.
- Wire compatibility becomes a release gate for every migrated module.
- Rust structs must serialize to the same semantic JSON as the Python `to_dict()` outputs for frozen contracts.
- Python can continue serving as test oracle, reference SDK, and compatibility layer during migration.
- The first implementation work should be narrow parity work, not a broad runtime rewrite.

## Reversal Conditions

Revisit this decision if Rust parity work cannot preserve deterministic behavior, if contract drift cannot be controlled without slowing safe repository advancement, or if the migration creates new safety-boundary pressure around providers, execution, deployment, or UI controls.
