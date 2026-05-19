# Sandbox Audit and Recovery

## Overview

This document defines audit recording, recovery behavior, evidence handling, governance constraints, and incident conditions for sandbox execution. Every sandbox lifecycle event produces structured audit data.

## Audit Record Structure

Every sandbox lifecycle event produces an audit record with the following fields:

```json
{
  "audit_id": "audit-001",
  "sandbox_id": "sb-001",
  "task_id": "task-abc",
  "event_type": "sandbox_committed",
  "timestamp": "2026-05-19T10:00:01Z",
  "actor": "orchestrator",
  "details": {
    "files_written": ["docs/output.md"],
    "bytes_written": 2048,
    "duration_ms": 1234,
    "exit_code": 0
  },
  "snapshot_ref": "sb-001-snapshot-final",
  "policy_version": "1.0.0"
}
```

### Audit Record Fields

| Field | Type | Description |
|---|---|---|
| `audit_id` | string | Unique identifier for this audit event. |
| `sandbox_id` | string | The sandbox this event relates to. |
| `task_id` | string | The task that owns the sandbox. |
| `event_type` | enum | One of the event types listed below. |
| `timestamp` | timestamp | ISO 8601 timestamp of the event. |
| `actor` | string | The component that triggered the event. |
| `details` | object | Event-specific structured data. |
| `snapshot_ref` | string | Reference to a snapshot, if one was taken. |
| `policy_version` | string | The policy version in effect at event time. |

### Event Types

| Event Type | When Recorded |
|---|---|
| `sandbox_created` | A new sandbox is allocated. |
| `sandbox_activated` | Execution begins in the sandbox. |
| `sandbox_committed` | Execution completes successfully. |
| `sandbox_failed` | Execution fails with an error. |
| `sandbox_released` | Sandbox resources are freed. |
| `write_claim_acquired` | A new write claim is created. |
| `write_claim_released` | A write claim is released. |
| `conflict_detected` | A file conflict between sandboxes is detected. |
| `conflict_resolved` | A conflict is resolved (claim released or timeout). |
| `policy_violation` | A sandbox attempts a forbidden action. |
| `rollback_triggered` | A rollback is initiated. |
| `snapshot_taken` | A state snapshot is recorded. |

### Audit Record Integrity

- Audit records are append-only. No audit record may be modified or deleted after creation.
- Audit records are stored in a dedicated audit log, separate from `events.jsonl`.
- Each audit record includes a `policy_version` field for traceability.
- Audit records include a sequence number for ordering within a sandbox.

## Recovery Behavior

### Automatic Recovery

When a sandbox fails:

1. The sandbox is transitioned to `failed` status.
2. All exclusive write claims are released immediately.
3. Files in temp scope are destroyed.
4. Files in disposable scope are destroyed.
5. Files in artifact scope are preserved for diagnosis.
6. A failure snapshot is taken.
7. Downstream tasks are blocked until the orchestrator decides the next action.
8. The failure is recorded in the audit log.

### Orchestrator Recovery Options

After a sandbox failure, the orchestrator may:

| Action | Effect |
|---|---|
| **Retry** | Create a new sandbox with the same request. The failed sandbox remains in the audit log. |
| **Skip** | Mark the task as skipped. Downstream tasks that depend only on soft dependencies may proceed. |
| **Escalate** | Route the failure to a human operator for investigation. |
| **Rollback** | Trigger a manual rollback of any partial changes. |

### Recovery Constraints

- Recovery actions must not modify `events.jsonl`.
- Recovery actions must not affect other sandboxes' claims or files.
- Recovery actions must be recorded in the audit log.
- Retry creates a new sandbox; the failed sandbox is not reused.
- Recovery must be idempotent — repeated recovery requests produce the same final state.

### Snapshot-Based Recovery

Snapshots enable point-in-time state inspection:

- **Pre-execution snapshot**: Captures the initial state of the sandbox before execution. Used to verify what changed during execution.
- **Post-execution snapshot**: Captures the final state of a committed sandbox. Used for evidence extraction.
- **Failure snapshot**: Captures the state at the time of failure. Used for diagnosis.

Recovery does not use snapshots to restore state (no rollback-to-snapshot). Snapshots are read-only diagnostic artifacts.

## Evidence Handling

### Evidence Is Diagnostic Only

A `sandbox_result` contains evidence fields (stdout/stderr summaries, resource usage, log references). This evidence is:

- **Used for diagnosis**: Understanding what happened during execution.
- **Used for evaluation**: Feeding into quality gates and advisor review.
- **Not used for approval**: A committed sandbox result does not imply correctness.
- **Not used for promotion**: Artifact promotion requires explicit quality gate approval.

### Evidence Record Structure

```json
{
  "evidence_id": "ev-001",
  "sandbox_id": "sb-001",
  "task_id": "task-abc",
  "collected_at": "2026-05-19T10:00:02Z",
  "stdout_summary": "Build completed successfully.",
  "stderr_summary": "",
  "structured_log_refs": ["sb-001-execution.log"],
  "resource_usage": {
    "peak_memory_mb": 128,
    "execution_time_ms": 1234,
    "files_written": 1,
    "bytes_written": 2048
  }
}
```

### Evidence Retention

| Sandbox Status | Retention Period | Purpose |
|---|---|---|
| `committed` | Until evidence extraction complete | Quality gate evaluation |
| `failed` | Until diagnosis complete | Failure investigation |
| `released` (no failure) | Archived | Audit trail |

### Evidence Access

- Evidence is accessible to the orchestrator and quality gates.
- Evidence is not accessible to the sandboxed task itself.
- Evidence is not exposed to external systems without governance approval.

## Governance

### No Policy Activation

This design track defines policies but does **not** activate them:

- Process allowlists are defined but not enforced (process execution is denied).
- Network allowlists are defined but not enforced (network access is denied).
- Audit records are structured but not persisted to a real audit log.
- Recovery behaviors are specified but not implemented.

Policy activation requires:
1. Governance review and approval.
2. Implementation in a dedicated stage.
3. Test coverage for all policy enforcement paths.
4. Documentation update reflecting active policies.

### Governance Approval Path

When policies are activated in the future:

1. A policy change candidate is created with full specification.
2. The candidate is reviewed against security and operational requirements.
3. The candidate is tested in a controlled environment.
4. The candidate is approved by the designated authority.
5. The candidate is activated and documented.

### Policy Versioning

- Each policy has a version number (e.g., `1.0.0`).
- Policy changes increment the version number.
- Audit records reference the policy version in effect at event time.
- Policy versions are immutable once activated.

## Incident Conditions

The following conditions constitute sandbox execution incidents:

### Severity: High

| Condition | Description |
|---|---|
| `events.jsonl` modification | A sandbox attempted to write to `events.jsonl`. This is a policy violation and must be investigated immediately. |
| Forbidden path access | A sandbox accessed a forbidden path (`events.jsonl`, `.claude/`, `src/harness_core/`, `tests/`). |
| Cross-sandbox corruption | A sandbox modified files belonging to another sandbox. |
| Audit log tampering | An attempt to modify or delete audit records. |

### Severity: Medium

| Condition | Description |
|---|---|
| Repeated timeout | A sandbox repeatedly exceeds execution time limits. |
| Memory pressure | A sandbox consistently hits memory limits. |
| Conflict storms | High frequency of file conflicts between sandboxes. |
| Resource leak | Sandbox resources are not properly cleaned up after release. |

### Severity: Low

| Condition | Description |
|---|---|
| Single timeout | A one-time execution timeout. |
| Single conflict | A one-time file conflict that resolves normally. |
| Configurable limit hit | A resource limit is hit but execution continues within bounds. |

### Incident Response

1. **Detect**: Monitoring or audit log analysis identifies the incident condition.
2. **Contain**: Affected sandboxes are released; conflicting operations are halted.
3. **Investigate**: Audit records and evidence are reviewed to determine root cause.
4. **Remediate**: The issue is fixed (policy adjustment, code fix, or configuration change).
5. **Report**: The incident is documented with timeline, root cause, and remediation.
6. **Review**: The incident is reviewed to prevent recurrence.
