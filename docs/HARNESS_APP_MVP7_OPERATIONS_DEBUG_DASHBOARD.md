# Harness App MVP7 Operations & Debug Dashboard

MVP7 adds a read-only operations and debug dashboard for the local Harness app.
It shows the state of the app itself: component status, data flow, storage
health, recent derived errors, boundary status, and recommended debug actions.

MVP7 is not an execution console, scheduler, provider integration, sandbox, or
worker system. It observes app-owned state and stops.

## Scope

- Read app-owned registry and plan-store paths.
- Derive component status for the local app.
- Derive data-flow status from repo registry through audit, planning, review,
  and triage panels.
- Derive storage health for registry and plan store.
- Derive recent app errors from blocked or unavailable components.
- Render the diagnostics in the local dashboard.

The diagnostics output is not persisted and does not mutate registry, plan
store, plan status, or target repositories.

## APIs

`GET /api/app/status`

Returns:

- `schema_version=app_status.v1`
- overall status
- mode
- last checked timestamp
- component status matrix
- data flow status
- storage health
- boundary notice

`GET /api/app/diagnostics`

Returns:

- `schema_version=app_diagnostics.v1`
- system overview
- component status matrix
- data flow status
- storage health
- recent derived errors
- recommended debug actions
- boundary notice

`GET /api/app/recent-errors`

Returns derived recent errors from current component status. It is not a
persistent event log. `recent_errors` is derived diagnostics, not an event log.

## Components

MVP7 reports these components:

- `app_server`
- `app_registry`
- `instance_audit`
- `plan_store`
- `resource_planner`
- `plan_workbench`
- `review_guidance`
- `plan_triage`
- `dashboard_frontend`
- `security_boundary`

Each component contains:

- `status`: `ok`, `warning`, `blocked`, or `unavailable`
- `message`
- `last_checked`
- `evidence`
- `recommended_action`

## Dashboard

The dashboard adds `Operations & Debug Dashboard` with:

- System Overview
- Component Status Matrix
- Data Flow Status
- Storage Health
- Recent API Errors
- Recommended Debug Actions

The dashboard also keeps a small in-memory list of client-observed API errors
so a failed fetch can be displayed immediately. That list is not persisted.

## Boundaries

MVP7 does not add:

- model API calls
- provider credentials
- provider failover
- sandbox, process, container, or VM execution
- autonomous agents
- concurrent workers
- production deployment
- target repo writes
- plan store mutation from diagnostics endpoints
- plan status mutation
- approval, run, execute, dispatch, assign, start, apply, merge, launch, or deploy controls
- task assignment
- issue creation
