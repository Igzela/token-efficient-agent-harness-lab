# Harness App MVP8 Operations Console Simplification

MVP8 refactors the local Harness app dashboard into a simpler operations console.
It is a frontend information-architecture change only. It does not add backend
capability, provider integration, sandbox execution, workers, deployment, target
repository writes, or approval/run controls.

## Scope

- Rename the first-screen experience to `Operations Console`.
- Keep only two first-screen primary actions: `Refresh status` and `Audit selected repo`.
- Make the first screen a read-only cockpit for derived app status, audit summary,
  component health, data flow, storage health, recent API errors, and debug actions.
- Move repository registration, raw JSON/sample loading, planning, plan review,
  portfolio triage, and review guidance into the tools area below the cockpit.
- Preserve existing deterministic, non-executable, advisory behavior.

## Dashboard Layout

The default view is:

- Header: `Harness App MVP8` and `Operations Console`.
- Operations cockpit: overall status, selected repo, last checked, warnings,
  blockers, audit verdict, component status matrix, data flow status, storage
  health, recent API errors, and recommended debug actions.
- Tools area:
  - Repository Audit
  - Planning
  - Plan Review
  - Portfolio Triage
  - Review Guidance
  - Raw JSON / Sample under Repository Audit

Planning, review, triage, and guidance remain available, but they no longer
dominate the first screen.

## Boundaries

MVP8 does not add:

- model API calls
- provider credentials
- provider failover
- sandbox, process, container, or VM execution
- autonomous agents
- concurrent workers
- production deployment
- target repo writes
- plan status mutation
- approval, run, execute, dispatch, assign, start, apply, merge, launch, or deploy controls

Operations diagnostics remain snapshot-derived UI data, not live telemetry.
