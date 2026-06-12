# V1 Safety Boundaries — Final GA Stance

Date: 2026-06-12
Status: AUTHORITATIVE

## Summary

This document records the final v1 GA safety boundary decisions. These boundaries define what v1 is and is not. v2 proposals must reference this document when proposing boundary changes.

## Operator Dashboard

v1 GA dashboard is a **read-only operator console**. No mutation controls.

Forbidden words enforced by `dashboard/lint-readonly.mjs`: approve, deploy, execute, merge, run.

Dashboard displays: dispatches, routing, proposals, snapshots, teams, costs, backups, audit, health, settings, workflow runs, scheduler, patches, regulator.

**Rationale:** Mutation controls require admin auth, confirmation dialogs, audit trails, and rollback safety. These are not implemented in v1. v2 may add them with proper gates.

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

Backup delete, key revoke, team member delete all require `team:admin` scope and confirmation.

## Hosted / Cloud / Multi-Tenant

**NOT the v1 target.** v1 is local/small-team self-hosted.

Hosted/cloud/multi-tenant requires separately approved tracks for: multi-tenant isolation, resource quotas, sandbox/process/container/VM execution, production deployment, and operations automation.

## Boundary Change Process

Any v1 boundary change requires:
1. Explicit human approval
2. Implementation plan with tests, docs, audit, and rollback
3. CI green
4. Updated safety boundary doc (this file)
