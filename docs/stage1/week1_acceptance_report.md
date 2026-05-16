# Stage 1 Week 1 Acceptance Report

## 1. Acceptance Summary

**Decision:** ACCEPTED.

Stage 1 Week 1 implemented the MVP runtime library for event storage, projection replay, project board lifecycle, task queue status mapping, validator coverage, and a minimal digest stub. The implementation stayed within the Stage 1 Week 1 boundary and did not modify `docs/stage0/events.jsonl`.

Pre-report verification:

```text
PYTHONPATH=src python3 -m unittest discover -s tests
Ran 58 tests in 0.005s
OK
```

Implementation commit at acceptance time: `1d82364`.

Working tree before this report: clean.

## 2. Components Implemented

### Event Store

- Append-only JSONL Event Store.
- Minimal `event.v1` schema validation.
- Canonical JSON + newline append.
- `event_id` uniqueness enforcement.
- Stable idempotency hash excluding `event_id` and `timestamp`.
- Duplicate idempotency key with same semantic hash is a no-op.
- Duplicate idempotency key with different semantic hash is rejected.
- JSONL validation and replay preflight.
- Stage 0 line 17 bad fixture detection.

### Projection Store

- Replays validated event streams into materialized views.
- Projects `project_item_state_changed` into Project Board item status.
- Projects `project_to_queue_handoff_created` into handoff records.
- Projects `project_dependency_resolved` into dependency resolution records.
- Rejects replay when preflight fails.
- Uses sanitized fixture for successful Stage 0 projection.

### Project Board Manager

- Defines the seven Project Board statuses.
- Validates legal status transitions.
- Rejects illegal transitions such as `todo -> done`.
- Preserves `task completed != project item done` by moving completed tasks to `review`.
- Provides Final Gate mapping: `pass -> done`, `pass_with_notes -> review`, `fail -> failed`.
- Provides allowed-files completeness checking.

### Task Queue Manager

- Accepts handoff only for `ready` Project Board items.
- Supports sequential scheduling only.
- Defines minimal task queue status transitions.
- Maps Task Queue status to Project Board status and blocked reason.
- Does not implement worker execution or concurrency.

### Validator Suite

- Event schema validator.
- `completion.json` validator.
- `handoff_pack.json` validator.
- `approval_request` validator.
- Advisor protocol validator with task-specific minimum call count.
- Canonical `failure_code` enum validator.
- Freeform `failure_subcode` support.
- Allowed-files completeness integration.
- Replay preflight validator.

### Batch Digest Stub

- Generates a minimal digest object from projection results.
- Reports completed, blocked, and failed items.
- Reports handoff and resolved dependency counts.
- Does not render full production digest YAML yet.

## 3. Commits

```text
bed5829 Checkpoint Stage 1 Day 1 event store
d354a31 Checkpoint Stage 1 Day 2 projection store
4952a3b Checkpoint Stage 1 Day 3 project board manager
c95109c Checkpoint Stage 1 Day 4 task queue manager
827619a Checkpoint Stage 1 Day 5 validators and digest stub
1d82364 Harden Stage 1 Week 1 failure code enum
```

## 4. Test Summary

Total tests at acceptance: **58**.

Key coverage areas:

- Valid event schema and schema violations.
- JSONL newline enforcement.
- Concatenated JSON object rejection.
- `event_id` duplicate rejection.
- Idempotency no-op and conflict behavior.
- Stage 0 line 17 fixture failure.
- Sanitized Stage 0 projection success.
- Project item statuses projected to all five items `done`.
- Handoff and dependency projections.
- Project Board legal/illegal transitions.
- Final Gate pass, pass-with-notes, and fail behavior.
- Task Queue handoff and status mapping.
- Validator coverage for completion, handoff pack, approval request, advisor protocol, failure codes, allowed files, and replay preflight.
- Batch Digest stub summary from projections.

Line 17 fixture status:

- `docs/stage0/events.jsonl` remains unchanged and intentionally malformed.
- `tests/fixtures/stage0_events_with_line17_issue.jsonl` preserves the bad line 17 issue.
- `tests/fixtures/stage0_events_sanitized.jsonl` provides the cleaned replay fixture.

Canonical failure code enum parity verified:

```text
F001_TIMEOUT
F002_BUDGET_EXCEEDED
F003_DEPENDENCY_FAILED
F004_APPROVAL_REJECTED
F005_PROVIDER_UNAVAILABLE
F006_SCOPE_VIOLATION
F007_TEST_FAILURE
F008_FORMAT_ERROR
F009_POLICY_VIOLATION
F010_CANCELLED
```

`F008_FORMAT_ERROR` with freeform `failure_subcode: handoff_pack_incomplete` remains valid for the task-005 fixture.

## 5. Known Gaps Not To Fix Yet

- No Web UI.
- No model calls.
- No provider failover.
- No dynamic DAG mutation.
- No real multi-agent concurrency.
- No projection persistence.
- Digest is still a stub.
- Validators are MVP-level, not full production strictness.
- No routing optimizer, skill extractor, fragment integrator, or build sampling.

These gaps are intentional and should remain out of scope until later stages.

## 6. Week 2 Recommendation

Recommended Week 2 focus: add a CLI wrapper around the Week 1 library.

Suggested commands:

- `validate-events`
- `project-state`
- `task-queue`
- `digest`

Constraints for Week 2:

- No model calls yet.
- No multi-agent execution yet.
- Continue treating `docs/stage0/events.jsonl` as a preserved bad fixture.
- Keep the Week 1 library as the source of truth for validation and projection behavior.
