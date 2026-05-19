# Filesystem and Write Claim Policy

## Overview

This document defines how sandbox execution interacts with the filesystem: what can be written, where, under what locks, and how conflicts are resolved. It extends the Stage 4 `WriteClaim` abstraction into a full filesystem access policy.

## Write Claims

A write claim records a sandbox's intent to modify a specific file path. Write claims are the primary mechanism for preventing concurrent modification conflicts.

### Claim Lifecycle

```
claimed → released
```

- A claim is created when a sandbox requests write access to a path.
- A claim is released when the sandbox completes, fails, or is explicitly released.
- Released claims are archived for audit but no longer block other sandboxes.

### Claim Fields

| Field | Type | Description |
|---|---|---|
| `claim_id` | string | Unique identifier for this claim. |
| `sandbox_id` | string | The sandbox holding this claim. |
| `task_id` | string | The task that owns the sandbox. |
| `file_path` | string | The absolute or relative path being claimed. |
| `scope` | enum | One of: `temp`, `disposable`, `artifact`. |
| `claimed_at` | timestamp | When the claim was created. |
| `released` | boolean | Whether the claim has been released. |
| `released_at` | timestamp | When the claim was released (null if still active). |

## Allowed Write Scopes

### Temp Scope

Temporary files for intermediate computation. Files in temp scope:
- Are created in a sandbox-specific temporary directory.
- Are automatically cleaned up when the sandbox is released.
- Must not be promoted as artifacts.
- May be read by other sandboxes only if explicitly shared.

### Disposable Scope

Ephemeral files that exist only during execution. Files in disposable scope:
- Are created in a sandbox-specific disposable directory.
- Are destroyed immediately on sandbox release or failure.
- Must never be read by other sandboxes.
- Must never be promoted as artifacts.

### Artifact Scope

Output files that may be evaluated downstream. Files in artifact scope:
- Are written to the sandbox's designated output directory.
- Survive sandbox release for evidence extraction.
- May be promoted through the artifact lifecycle (subject to policy).
- Are subject to quality gate evaluation before promotion.

## Forbidden Paths

The following paths are absolutely forbidden for sandbox writes, regardless of scope:

| Path | Reason |
|---|---|
| `events.jsonl` | Event store integrity — sandbox execution must never modify the event log. |
| `.claude/` | Harness configuration and agent memory are protected. |
| `src/harness_core/` | Harness source code must not be modified by sandboxed tasks. |
| `tests/` | Test infrastructure is protected from sandbox interference. |
| `docs/sandbox_execution/` | Sandbox design documents are protected. |

### Forbidden Path Enforcement

- Sandbox requests that include forbidden paths in `write_claim_paths` must be rejected before sandbox creation.
- If a sandbox attempts to write to a forbidden path during execution, the execution must be terminated with a `policy_violation` error.
- Forbidden path checks are mandatory and cannot be overridden.

## Lock Modes

### Exclusive Lock

An exclusive lock on a file path allows only one sandbox to hold a claim at a time. This is the default lock mode for artifact-scope writes.

- If sandbox A holds an exclusive lock on `path/to/file`, sandbox B's claim request for the same path is blocked.
- Exclusive locks are released when the claiming sandbox completes, fails, or is released.

### Shared Lock

A shared lock allows multiple sandboxes to read a file concurrently but prevents writes. This is used for read-only access to shared source files.

- Multiple sandboxes may hold shared locks on the same path.
- A shared lock blocks exclusive lock requests until all shared locks are released.
- Shared locks are used for `allowed_read_paths` in sandbox requests.

### No Lock

Paths in temp and disposable scopes do not require explicit locks because they are sandbox-private. The filesystem isolation (sandbox-specific directories) provides implicit mutual exclusion.

## Conflict Detection

### Conflict Conditions

A conflict exists when:
- Sandbox A holds an active exclusive lock on path P.
- Sandbox B requests an exclusive lock on path P.
- Both sandboxes are in `active` status.

### Conflict Resolution

Conflicts are resolved by blocking, not preemption:
- Sandbox B's claim request is queued until Sandbox A releases the lock.
- If the queue exceeds a configurable timeout, Sandbox B's request fails with a `conflict_timeout` error.
- No sandbox is forcibly terminated to resolve a conflict.

### Conflict Reporting

Conflict detection produces a structured report:

```json
{
  "conflict_detected": true,
  "conflicting_sandbox_id": "sb-001",
  "conflicting_path": "docs/output.md",
  "message": "File 'docs/output.md' is claimed by sandbox sb-001 (task task-abc)."
}
```

## Snapshot Rules

Snapshots capture sandbox state at a point in time for recovery or audit.

### When Snapshots Are Taken

- Before sandbox execution begins (initial state).
- After successful sandbox completion (final state).
- On sandbox failure (failure state for diagnosis).

### Snapshot Contents

- File manifest (paths, sizes, checksums) of all sandbox-scoped files.
- Write claim status at the time of snapshot.
- Sandbox lifecycle state at the time of snapshot.

### Snapshot Retention

- Snapshots for committed sandboxes are retained for evidence extraction.
- Snapshots for failed sandboxes are retained for diagnosis.
- Snapshots for released sandboxes are archived and eventually purged.

## Rollback Rules

### Automatic Rollback

When a sandbox fails:
1. All exclusive locks held by the sandbox are released.
2. Files written in temp and disposable scope are destroyed.
3. Files written in artifact scope are preserved for diagnosis (not promoted).
4. The sandbox transitions to `failed` status.

### Manual Rollback

When an operator or orchestrator triggers rollback:
1. The sandbox is transitioned to `released` status.
2. All locks are released.
3. Artifact-scope files may be preserved or destroyed based on the rollback policy.
4. The rollback action is recorded in the audit log.

### Rollback Constraints

- Rollback must not modify files outside the sandbox's claimed paths.
- Rollback must not affect other sandboxes' claims or files.
- Rollback must not modify `events.jsonl`.
- Rollback is idempotent — multiple rollback requests for the same sandbox produce the same result.
