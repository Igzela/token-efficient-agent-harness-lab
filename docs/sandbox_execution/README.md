# Sandbox Execution Design Track

## What This Track Is

This directory contains the **design-only** specification for sandbox execution in the Token-Efficient Agent Harness Lab. It defines how isolated task execution *could* work in a future stage — without implementing anything.

## Design-Only Constraints

- No code is added or modified by this track.
- No containers, subprocesses, VMs, or network calls are introduced.
- No new dependencies are added.
- No runtime behavior changes.
- `events.jsonl` is never written to by sandbox execution.
- All 787 existing tests remain green.

## Document Map

| File | Scope |
|---|---|
| [SANDBOX_EXECUTION_DESIGN.md](SANDBOX_EXECUTION_DESIGN.md) | Goals, non-goals, lifecycle, request/result schemas, rules |
| [FILESYSTEM_AND_WRITE_CLAIM_POLICY.md](FILESYSTEM_AND_WRITE_CLAIM_POLICY.md) | Write claims, scopes, forbidden paths, lock modes, conflict/snapshot/rollback |
| [PROCESS_AND_NETWORK_POLICY.md](PROCESS_AND_NETWORK_POLICY.md) | Process allowlist, network default-deny, resource limits, failure mapping |
| [SANDBOX_AUDIT_AND_RECOVERY.md](SANDBOX_AUDIT_AND_RECOVERY.md) | Audit records, recovery behavior, evidence handling, governance, incident conditions |
| [SANDBOX_ENTRY_CRITERIA.md](SANDBOX_ENTRY_CRITERIA.md) | Prerequisites, allowed first implementation, forbidden patterns |

## Relationship to Stage 4

Stage 4 already defines a logical `SandboxManager` for file-claim tracking and conflict detection (see `docs/stage4/sandbox_concurrency_spec.md`). This design track extends that abstraction into a full execution model — but remains a specification only. No Stage 4 code or behavior is modified.

## Status

- Created: 2026-05-19
- Status: Design-only (no implementation)
- Approved by: pending governance
