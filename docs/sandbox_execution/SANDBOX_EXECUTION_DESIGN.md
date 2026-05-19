# Sandbox Execution Design

## Goals

1. **Task isolation** — each task execution runs in a logically isolated environment with controlled filesystem access and resource limits.
2. **Auditable state transitions** — every sandbox lifecycle event (create, execute, commit, release, fail) is recorded as structured data.
3. **Failure containment** — sandbox failures do not corrupt the harness state, event store, or other task sandboxes.
4. **Evidence-first** — sandbox results are treated as evidence for downstream evaluation, not as approval of correctness.

## Non-Goals

- Real container or VM isolation in this design track.
- Network access for sandboxed tasks.
- Production-grade concurrency or worker pools.
- Modification of `events.jsonl` by sandbox execution.
- Provider integration or real model calls from within sandboxes.

## Sandbox Lifecycle

```
created → active → {committed | failed | released}
```

### States

| State | Meaning |
|---|---|
| `created` | Sandbox allocated; no execution has occurred. |
| `active` | Execution is in progress or write claims are held. |
| `committed` | Execution completed; results are available for evidence extraction. |
| `failed` | Execution failed; rollback and cleanup paths are available. |
| `released` | Sandbox resources are freed; claims are dropped; sandbox is inert. |

### Lifecycle Rules

- A sandbox transitions from `created` to `active` when execution begins.
- A sandbox may transition from `active` to `committed` on successful completion.
- A sandbox may transition from `active` to `failed` on any execution error.
- A sandbox may transition from `active` to `released` on explicit cancellation.
- A sandbox may transition from `committed` or `failed` to `released` after evidence extraction.
- A released sandbox must not be reused.

## Request Schema

A `sandbox_request` describes what a task execution needs:

```json
{
  "sandbox_request_id": "sr-001",
  "task_id": "task-abc",
  "requested_by": "orchestrator",
  "scope": {
    "write_claim_paths": ["docs/output.md"],
    "allowed_read_paths": ["docs/source.md", "src/module.py"],
    "forbidden_paths": ["events.jsonl", ".claude/"],
    "temp_dir": true,
    "disposable_dir": false
  },
  "resource_limits": {
    "max_execution_time_seconds": 300,
    "max_memory_mb": 512,
    "max_output_bytes": 10485760
  },
  "requested_at": "2026-05-19T10:00:00Z"
}
```

### Request Fields

| Field | Required | Description |
|---|---|---|
| `sandbox_request_id` | yes | Unique identifier for this request. |
| `task_id` | yes | The task requesting sandbox execution. |
| `requested_by` | yes | The component requesting execution (e.g., orchestrator). |
| `scope.write_claim_paths` | yes | File paths the sandbox intends to write. Must not include forbidden paths. |
| `scope.allowed_read_paths` | no | Explicit read paths beyond the temp/disposable dirs. |
| `scope.forbidden_paths` | yes | Paths that must never be accessed. Always includes `events.jsonl`. |
| `scope.temp_dir` | no | Whether a temporary directory is provisioned (default: false). |
| `scope.disposable_dir` | no | Whether a disposable directory is provisioned (default: false). |
| `resource_limits` | no | Execution constraints; defaults apply if omitted. |
| `requested_at` | yes | ISO 8601 timestamp. |

## Result Schema

A `sandbox_result` describes what happened during execution:

```json
{
  "sandbox_request_id": "sr-001",
  "sandbox_id": "sb-001",
  "task_id": "task-abc",
  "status": "committed",
  "outcome": {
    "exit_code": 0,
    "duration_ms": 1234,
    "files_written": ["docs/output.md"],
    "bytes_written": 2048,
    "files_read": ["docs/source.md"],
    "artifacts": ["docs/output.md"]
  },
  "evidence": {
    "stdout_summary": "Build completed successfully.",
    "stderr_summary": "",
    "structured_log_refs": ["sb-001-execution.log"],
    "resource_usage": {
      "peak_memory_mb": 128,
      "execution_time_ms": 1234
    }
  },
  "errors": [],
  "completed_at": "2026-05-19T10:00:01Z"
}
```

### Result Fields

| Field | Required | Description |
|---|---|---|
| `sandbox_request_id` | yes | Matches the originating request. |
| `sandbox_id` | yes | The assigned sandbox identifier. |
| `task_id` | yes | The task that was executed. |
| `status` | yes | One of: `committed`, `failed`, `released`. |
| `outcome.exit_code` | conditional | Process exit code; present for committed/failed statuses. |
| `outcome.duration_ms` | yes | Wall-clock execution time. |
| `outcome.files_written` | yes | Files actually written by the sandbox. |
| `outcome.bytes_written` | yes | Total bytes written. |
| `outcome.files_read` | yes | Files actually read by the sandbox. |
| `outcome.artifacts` | yes | Paths eligible for artifact promotion (subject to policy). |
| `evidence` | yes | Diagnostic data; never treated as approval. |
| `errors` | yes | List of error objects; empty on success. |
| `completed_at` | yes | ISO 8601 timestamp. |

## Rules

### Result Is Evidence, Not Approval

A `sandbox_result` with `status: committed` means the sandbox completed without error. It does **not** mean:
- The output is correct.
- The task is done.
- The artifacts should be promoted.

Downstream evaluation (quality gates, advisor review) determines correctness. The sandbox result is evidence for that evaluation.

### Network Defaults Denied

Sandboxed tasks have no network access by default. Network access is a future capability that requires:
- Explicit allowlist entry in the sandbox request.
- Governance approval for the network policy.
- Audit logging of all network requests made.

### Forbidden Paths

The following paths are always forbidden in any sandbox:

- `events.jsonl` — the event store must never be written by sandbox execution.
- `.claude/` — harness configuration and memory are protected.
- `src/harness_core/` — harness source code is protected.
- `tests/` — test infrastructure is protected.

### Resource Limits

Default resource limits apply when not explicitly specified:

- `max_execution_time_seconds`: 300
- `max_memory_mb`: 512
- `max_output_bytes`: 10485760 (10 MB)

These defaults are conservative design-time values, not production thresholds.

### Determinism

- Sandbox request and result schemas are stable across the design track.
- Sandbox IDs are assigned deterministically in test environments.
- Execution ordering follows task DAG dependencies, not arrival time.
