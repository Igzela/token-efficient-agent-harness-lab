# V1 Safety Boundaries — Final GA Stance

Date: 2026-06-12
Last updated: 2026-06-17
Status: AUTHORITATIVE

## Summary

This document records the final v1 GA safety boundary decisions. These boundaries define what v1 is and is not. v2 proposals must reference this document when proposing boundary changes.

## Operator Dashboard

v1 GA dashboard is a **local operator console**. It is not purely read-only.

Dashboard surfaces fall into three groups:

- Read-only observability: dispatches, routing, costs, health, audit, scheduler, queue, executor pool, workflow graph, patch/artifact metadata, regulator state, and decision traces.
- Guarded app-owned controls: team/API-key administration, backup create/verify/restore/delete, policy proposal approve/reject/deactivate/rollback, workflow tick/cancel, and supervised patch approval/export. These must remain protected by backend auth/scopes, explicit confirmation where required, and audit logging.
- Boundary status: local-only deployment, provider/CLI gates, target-repository write boundary, and release/deploy boundary.

The legacy `dashboard/scripts/lint-readonly.mjs` guard was created for the Phase 7 read-only operator-surface slice and only scans `dashboard/src/app`. It must not be treated as proof that the current dashboard is globally read-only.

Dashboard controls must not mutate target repositories, perform release/tag/deploy actions, broaden provider/CLI execution gates, bypass backend auth/scopes, or introduce unattended autonomous workers.

**Rationale:** v1 permits guarded mutation only for app-owned local state. Target-repository writes, hosted/cloud controls, deploy/apply controls, and default-on external execution remain outside v1.

## Provider Execution

Default: **OFF**. Requires `ACP_ENABLE_PROVIDER_EXECUTION=1`.

When enabled: requires `ACP_REQUIRE_AUTH=1`, `dispatch:execute` scope, cost caps (`ACP_COST_PER_DISPATCH_USD`, `ACP_COST_DAILY_USD`).

CI never calls real provider APIs. Local beta only.

## CLI Execution

Default: **OFF**. Requires `ACP_ENABLE_CLI_EXECUTION=1`.

When enabled: spawns local CLI binaries (`claude`, `codex`) with timeout (`ACP_CLI_TIMEOUT_MS`) and shell-metachar rejection.

CI uses stub/noop paths only.

## Target Repository Writes

**Disabled.** App runtime never writes to target repositories.

Agent maintenance may create branches/commits/PRs through the Real-World Testing Playbook gates. This is a repository workflow mode, not an app-runtime feature.

## Release / Tag / Deploy

No auto release, tag, or deploy behavior. Release requires explicit human approval and tag push.

## Active Policy Mutation

Requires: `ACP_ENABLE_AUTO_ADJUSTMENT=1`, `ACP_AUTO_ADJUSTMENT_ACTIVE=1`, `team:admin`, `confirm_auto_adjustment=true`.

Default: **OFF**. One adjustment per request. Snapshot before mutation. Rollback with hash validation.

## Destructive Operations

Backup restore/delete, key revoke/delete/rotate, team member delete, workflow cancellation, and policy rollback/deactivation require appropriate admin scope, confirmation where implemented, and audit evidence. Destructive behavior remains limited to app-owned local state.

## Hosted / Cloud / Multi-Tenant

**NOT the v1 target.** v1 is local/small-team self-hosted.

Hosted/cloud/multi-tenant requires separately approved tracks for: multi-tenant isolation, resource quotas, sandbox/process/container/VM execution, production deployment, and operations automation.

## Boundary Change Process

Any v1 boundary change requires:
1. Explicit human approval
2. Implementation plan with tests, docs, audit, and rollback
3. CI green
4. Updated safety boundary doc (this file)
