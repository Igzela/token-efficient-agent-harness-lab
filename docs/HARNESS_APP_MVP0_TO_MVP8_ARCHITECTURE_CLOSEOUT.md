# Harness App MVP0-MVP8 Architecture Closeout

## Executive Summary

Harness App MVP0 through MVP8 are complete.

The app is now a local, non-executable operations/control-plane prototype for
inspecting harness state, reviewing app-owned planning records, and surfacing
derived diagnostics. It is designed to help a human understand status, review
safe next actions, and inspect boundaries. It is not an autonomous runtime.

This closeout does not create CA-8, Stage 5, a production service, or a new
capability track. Future work that adds provider calls, sandbox/process
execution, autonomous workers, target repository writes, approval controls,
deployment, or active policy mutation requires explicit approval as a new track.

## Architecture Snapshot

MVP0 through MVP8 form a local read-only and app-owned-state stack:

| MVP | Capability | Boundary |
| --- | --- | --- |
| MVP0 | Instance auditor | Reads a target repository and reports harness control-file status. |
| MVP1 | Static JavaScript dashboard | Displays audit data without backend execution authority. |
| MVP2 | Local read-only control plane and registry | Adds an app-owned repository registry and local API handlers while keeping target repositories read-only. |
| MVP3 | Deterministic non-executable planning kernel | Produces app-owned resource plans that are explicitly not executable. |
| MVP4 | Plan review workbench | Derives plan history, summaries, comparisons, and advisory review actions. |
| MVP5 | Review guidance preview | Derives non-persistent advisory guidance from stored non-executable plans. |
| MVP6 | Planning portfolio triage | Derives review priority, bottlenecks, and token hotspots across stored plans. |
| MVP7 | Operations/debug diagnostics | Derives component status, data flow, storage health, recent errors, and debug actions. |
| MVP8 | Operations console simplification | Reorganizes the dashboard into a first-screen operations cockpit and lower collapsed tools. |

The resulting system is an observation and review surface. It can register local
app metadata, store non-executable plans in app-owned state, and derive read-only
views over those records. It cannot execute plans, mutate target repositories,
assign work, deploy, approve itself, or call model providers.

## State Boundary

| State area | Ownership | Persistence | Mutation authority |
| --- | --- | --- | --- |
| Target repositories | External to the app | External | Read-only from the Harness app. |
| App registry | App-owned | Local app registry file | App may add/update registry entries only. |
| Plan store | App-owned | Local append-only/non-executable planning records | App may append/store non-executable plans; it does not execute or approve them. |
| Audit reports | Derived from target repositories | Returned as API/UI data | Read-only derivation; no target repository writes. |
| Plan summaries/comparisons | Derived from plan store | Not independently persisted | Read-only derivation. |
| Review guidance | Derived from stored plans | Non-persistent preview | Advisory only; no plan mutation. |
| Portfolio triage | Derived from stored plans | Non-persistent view | Advisory only; no assignment or status mutation. |
| App diagnostics/recent errors | Derived from registry, plan store, and current API status | Snapshot-derived unless explicitly stated | Diagnostic view only; no recovery execution. |
| Dashboard | UI/control surface | Static frontend plus local API reads | Display and safe app-owned controls only; no execution authority. |

The most important boundary is that target repositories remain read-only. The
app may write its own registry or plan-store files, but it must not write into a
target project, mark work approved, execute planned steps, launch workers, or
perform deployment actions.

## Current API Surface

The Harness app API surface after MVP8 is:

| Endpoint | Purpose | Boundary |
| --- | --- | --- |
| `GET /api/health` | Reports local app server health. | Read-only status. |
| `GET /api/repos` | Lists app-owned registered repositories. | Reads app registry. |
| `POST /api/repos` | Registers repository metadata in app-owned state. | App registry only; no target repository writes. |
| `GET /api/audit` | Runs a read-only harness instance audit for a selected repo. | Reads target repository only. |
| `POST /api/plans` | Creates a deterministic non-executable resource plan. | App-owned plan store only. |
| `GET /api/plans` | Lists stored non-executable plans. | Read-only plan-store view. |
| `GET /api/plans/{plan_id}` | Reads one stored plan. | Read-only plan-store view. |
| `GET /api/plans/summary` | Derives plan summary counts and averages. | Derived read-only view. |
| `GET /api/plans/compare` | Compares two stored plans. | Derived read-only view. |
| `GET /api/plans/review-guidance` | Builds advisory guidance for a stored plan. | Non-persistent preview. |
| `GET /api/plans/triage` | Builds planning portfolio triage. | Derived read-only view. |
| `GET /api/app/status` | Reports current app status, data flow, and storage health. | Snapshot-derived diagnostics. |
| `GET /api/app/diagnostics` | Reports component status, recent derived errors, and debug actions. | Snapshot-derived diagnostics. |
| `GET /api/app/recent-errors` | Reports recent derived app errors. | Derived diagnostics, not an event log. |

MVP8 did not add new backend endpoints. It reorganized the existing dashboard so
the first screen emphasizes app health, data flow, component state, storage
health, recent errors, and debug next actions.

## Explicit Forbidden Scope

The Harness app closeout excludes:

- real provider or model API calls
- provider credentials or API keys
- provider failover
- sandbox, process, container, or VM execution
- autonomous agents or autonomous workers
- production concurrent worker execution
- deployment or production service operation
- target repository writes
- approval, run, execute, assign, deploy, merge, apply, launch, dispatch, or start controls
- plan execution
- plan status mutation from diagnostics, guidance, triage, or review views
- active policy mutation
- task assignment
- issue creation
- CA-8
- Stage 5

Any future work in those categories must be treated as a separately approved
track, not an automatic continuation of the MVP0-MVP8 app.

## Current Verification Baseline

The closeout baseline is:

```bash
PYTHONPATH=src python3 -m unittest discover -s tests
# Ran 897 tests: OK
```

Additional checks used for the MVP8 merge:

```bash
python3 tools/check_security_baseline.py
# RESULT: ALL CHECKS PASSED

node --check web/dashboard/app.js
# PASS

git diff --check
# PASS
```

Browser smoke for MVP8 confirmed:

- the first screen shows `Operations Console`, not the planning form
- first-screen primary actions are limited to `Refresh status` and `Audit selected repo`
- component status, data flow, storage health, recent errors, and debug actions are visible in the operations cockpit
- planning, plan review, portfolio triage, review guidance, repository registration, and raw JSON/sample tools are below the cockpit and collapsed or secondary
- no approval/run/execute/assign/deploy/merge controls are present

## Recommended Stop Point

Stop feature work after MVP8.

The next useful activity is user acceptance on real local projects: run the
dashboard against representative repositories, inspect whether the operations
cockpit makes status and boundaries clear, and collect friction points. That
trial should remain read-only for target repositories.

Do not start MVP9 by default. If MVP9 is approved later, it should be limited to
polish or reliability work, such as clearer copy, stronger browser smoke
coverage, better empty states, or documentation corrections. It should not add
execution, provider calls, workers, deployment, target repository writes, or
approval authority.

## Closeout Decision

Harness App MVP0-MVP8 is closed as a local, deterministic,
non-executable operations/control-plane prototype.

The sealed interpretation is:

- app-owned state may support registry and non-executable planning records
- target repositories remain read-only
- diagnostics, review guidance, and triage are derived/advisory views
- dashboard controls are for display, refresh, audit, and app-owned planning only
- execution authority remains outside this app

