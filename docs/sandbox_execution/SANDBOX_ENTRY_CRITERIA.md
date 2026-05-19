# Sandbox Execution Entry Criteria

## Overview

This document defines what must be true before real sandbox execution can be implemented, what the allowed first implementation is, and what patterns are forbidden. Entry criteria ensure the harness is ready for sandbox execution without compromising existing guarantees.

## Prerequisites

The following conditions must be met before any sandbox execution implementation begins:

### Infrastructure Prerequisites

| Prerequisite | Status | Evidence |
|---|---|---|
| Stage 4 sandbox abstraction complete | Complete | `docs/stage4/sandbox_concurrency_spec.md` |
| Write claim tracking operational | Complete | `src/harness_core/sandbox.py` |
| Conflict detection operational | Complete | `tests/test_sandbox_*.py` |
| All 787 tests passing | Current | `PYTHONPATH=src python3 -m unittest discover -s tests` |
| Audit record structure defined | This track | `SANDBOX_AUDIT_AND_RECOVERY.md` |
| Filesystem policy defined | This track | `FILESYSTEM_AND_WRITE_CLAIM_POLICY.md` |
| Process/network policy defined | This track | `PROCESS_AND_NETWORK_POLICY.md` |
| Entry criteria defined | This track | This document |

### Policy Prerequisites

| Prerequisite | Status |
|---|---|
| Write scope semantics (temp/disposable/artifact) defined | Defined |
| Forbidden paths list finalized | Finalized |
| Lock mode semantics defined | Defined |
| Conflict resolution rules defined | Defined |
| Snapshot and rollback rules defined | Defined |
| Process allowlist format defined | Defined (future) |
| Network allowlist format defined | Defined (future) |
| Incident severity levels defined | Defined |

### Governance Prerequisites

| Prerequisite | Status |
|---|---|
| Design track approved by governance | Pending |
| Security review of forbidden paths | Pending |
| Review of audit record structure | Pending |
| Approval of recovery behavior specification | Pending |

## Allowed First Implementation

The first implementation of sandbox execution must be:

### Local Tempdir Dry-Run

A **local tempdir dry-run** is the only permitted first implementation. This means:

| Constraint | Requirement |
|---|---|
| **Location** | Local filesystem only. No remote execution. |
| **Isolation** | `tempfile.TemporaryDirectory` for sandbox workspace. |
| **Execution** | No real process execution. Logical sandbox lifecycle only. |
| **Network** | No network access whatsoever. |
| **Duration** | Deterministic, bounded by test fixtures. |
| **Cleanup** | Automatic cleanup of temp directories on sandbox release. |

### Dry-Run Characteristics

A dry-run sandbox:

- Creates a temporary directory as the sandbox workspace.
- Simulates write claims by creating files in the temp directory.
- Simulates execution by recording a deterministic result.
- Simulates conflicts by detecting overlapping claims.
- Produces audit records for all lifecycle events.
- Cleans up the temp directory on release.

### Dry-Run Limitations

A dry-run sandbox:

- Does not execute real code.
- Does not run real processes.
- Does not access the network.
- Does not produce real artifacts.
- Does not evaluate task correctness.

## Forbidden Patterns

The following patterns are explicitly forbidden for sandbox execution, both in this design track and in any future implementation until governance approval:

### Shell / Container / VM

| Forbidden Pattern | Reason |
|---|---|
| `subprocess.run()` / `subprocess.Popen()` | Real process execution is outside design scope. |
| `os.system()` / `os.popen()` | Shell command execution is forbidden. |
| Docker / container runtimes | Container isolation is outside design scope. |
| Virtual machine managers (QEMU, VirtualBox) | VM isolation is outside design scope. |
| `chroot` / namespaces / cgroups | OS-level isolation primitives are forbidden. |
| `containerd` / `podman` / any container runtime | Container runtimes are outside design scope. |

### Network / Provider

| Forbidden Pattern | Reason |
|---|---|
| `requests` / `httpx` / `urllib` (in sandbox) | Network access is denied by default. |
| `socket` module usage (in sandbox) | Raw network access is forbidden. |
| WebSocket connections | Real-time network is outside design scope. |
| Model provider API calls | Real model calls are outside design scope. |
| `aiohttp` / async HTTP clients | Async network is forbidden. |

### Source / PR

| Forbidden Pattern | Reason |
|---|---|
| Modifying `src/harness_core/` | Harness source code is protected. |
| Modifying `tests/` | Test infrastructure is protected. |
| Modifying `events.jsonl` | Event store is protected. |
| Modifying `.claude/` | Harness configuration is protected. |
| Creating pull requests | PR creation is outside sandbox scope. |
| Pushing to remote | Remote push is outside sandbox scope. |

### Runtime Changes

| Forbidden Pattern | Reason |
|---|---|
| Adding new dependencies | This design track adds no dependencies. |
| Modifying the test suite | Existing tests must not be altered. |
| Changing harness configuration | Runtime behavior must not change. |
| Modifying the CLI | CLI interface must not change. |

## Verification Checklist

Before implementation proceeds beyond dry-run:

- [ ] All 787 tests still pass after dry-run implementation.
- [ ] Dry-run sandbox creates and cleans up temp directories.
- [ ] Dry-run sandbox produces audit records for all lifecycle events.
- [ ] Dry-run sandbox correctly detects write claim conflicts.
- [ ] Dry-run sandbox correctly blocks forbidden path writes.
- [ ] Dry-run sandbox correctly handles snapshot and rollback.
- [ ] No forbidden patterns are introduced in the codebase.
- [ ] No new dependencies are added.
- [ ] No changes to `events.jsonl`.
- [ ] Security review of forbidden path enforcement.
- [ ] Governance approval of entry criteria.

## Exit Criteria for Dry-Run

The dry-run implementation is complete when:

1. All verification checklist items are satisfied.
2. Audit records are produced for all sandbox lifecycle events.
3. Conflict detection works correctly across multiple concurrent sandboxes.
4. Forbidden path enforcement blocks all prohibited writes.
5. Snapshot and rollback work as specified.
6. The design track documentation is updated to reflect implementation status.
7. Governance reviews and approves the dry-run implementation.
