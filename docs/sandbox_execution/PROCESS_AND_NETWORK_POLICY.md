# Process and Network Policy

## Overview

This document defines how sandbox execution handles process execution and network access. In this design track, process execution and network access are denied by default. Future implementation may introduce controlled allowlists.

## Process Policy

### Current Status: Denied

In this design track, sandboxed tasks do **not** execute real processes. The harness uses logical sandbox abstractions for file-claim tracking and conflict detection only.

### Future Allowlist Design

When process execution is introduced, it must follow an explicit allowlist model:

```json
{
  "process_policy": {
    "default": "deny",
    "allowlist": [
      {
        "command_pattern": "python3 scripts/build.py",
        "description": "Build script execution",
        "max_duration_seconds": 120
      }
    ]
  }
}
```

#### Allowlist Fields

| Field | Type | Description |
|---|---|---|
| `command_pattern` | string | Exact command or glob pattern to allow. |
| `description` | string | Human-readable purpose. |
| `max_duration_seconds` | integer | Maximum allowed execution time for this command. |

#### Allowlist Rules

- The default policy is `deny` — any command not on the allowlist is rejected.
- Allowlist entries are evaluated against the exact command string, not partial matches.
- Glob patterns in `command_pattern` are limited to simple `*` wildcards, not regex.
- Allowlist entries require governance approval before activation.
- Each allowlist entry is independently auditable.

### Process Execution Constraints

When process execution is enabled in the future:

- Each process runs in the sandbox's temporary directory as the working directory.
- Processes inherit only the environment variables explicitly set in the sandbox request.
- Processes have no access to the host's environment beyond what is explicitly provided.
- Process stdout and stderr are captured and included in the sandbox result.
- Process exit codes are recorded in the sandbox result.

### Resource Limits for Processes

| Resource | Default Limit | Description |
|---|---|---|
| `max_execution_time_seconds` | 300 | Wall-clock time before the process is terminated. |
| `max_memory_mb` | 512 | Peak memory usage before the process is terminated. |
| `max_output_bytes` | 10485760 | Maximum combined stdout/stderr output. |
| `max_open_files` | 256 | Maximum file descriptors. |

These defaults are conservative design-time values.

### Process Failure Mapping

| Condition | Sandbox Status | Error Type |
|---|---|---|
| Process exits with code 0 | `committed` | (none) |
| Process exits with non-zero code | `failed` | `process_exit_error` |
| Process exceeds time limit | `failed` | `process_timeout` |
| Process exceeds memory limit | `failed` | `process_oom` |
| Process exceeds output limit | `failed` | `process_output_overflow` |
| Process not on allowlist | rejected before creation | `command_not_allowed` |
| Process signal (SIGKILL/SIGTERM) | `failed` | `process_signal` |

## Network Policy

### Current Status: Default Deny

Sandboxed tasks have **no network access** by default. This is a hard constraint, not a configurable default.

### Future Network Allowlist Design

When network access is introduced, it must follow an explicit allowlist model:

```json
{
  "network_policy": {
    "default": "deny",
    "allowlist": [
      {
        "host": "registry.example.com",
        "port": 443,
        "protocol": "https",
        "description": "Package registry access",
        "max_requests": 100
      }
    ]
  }
}
```

#### Network Allowlist Fields

| Field | Type | Description |
|---|---|---|
| `host` | string | Target hostname (exact match, not glob). |
| `port` | integer | Target port. |
| `protocol` | enum | One of: `https`, `http`, `tcp`, `udp`. |
| `description` | string | Human-readable purpose. |
| `max_requests` | integer | Maximum number of requests to this endpoint. |

#### Network Allowlist Rules

- The default policy is `deny` — any outbound connection not on the allowlist is blocked.
- Allowlist entries are evaluated per (host, port, protocol) tuple.
- DNS resolution is performed by the harness, not by the sandbox.
- Network allowlist entries require governance approval before activation.
- Each network allowlist entry is independently auditable.

### Network Monitoring

When network access is enabled:

- All outbound connections are logged with timestamps.
- Request/response metadata (not bodies) is recorded.
- Bandwidth usage is tracked per sandbox.
- Anomalous patterns (high request count, unusual hosts) trigger alerts.

### Network Failure Mapping

| Condition | Sandbox Status | Error Type |
|---|---|---|
| Connection to non-allowlisted host | blocked, connection refused | `network_denied` |
| Connection timeout | `failed` | `network_timeout` |
| Connection reset | `failed` | `network_reset` |
| Max requests exceeded | blocked | `network_rate_limited` |

## Resource Limits Summary

| Resource | Default | Enforced By |
|---|---|---|
| Execution time | 300s | Timeout kill |
| Memory | 512 MB | OOM kill |
| Output size | 10 MB | Truncation |
| Open files | 256 | File descriptor limit |
| Network requests | 0 (deny all) | Connection block |
| Write scope | per claim | Lock enforcement |

## Failure Propagation

Sandbox failures propagate upstream through the task DAG:

1. A sandbox failure produces a `sandbox_result` with `status: failed`.
2. The task that owns the sandbox is marked as `failed`.
3. Downstream tasks that depend on the failed task are blocked.
4. The orchestrator decides whether to retry, skip, or escalate.
5. The failure is recorded in the audit log with full context.

### Failure Isolation

- A sandbox failure must not affect other sandboxes.
- A sandbox failure must not corrupt the harness state.
- A sandbox failure must not modify `events.jsonl`.
- Cleanup of failed sandbox resources follows the rollback rules in `FILESYSTEM_AND_WRITE_CLAIM_POLICY.md`.
