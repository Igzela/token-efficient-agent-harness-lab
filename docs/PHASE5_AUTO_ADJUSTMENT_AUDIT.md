# Phase 5 Auto-Adjustment Audit

Date: 2026-06-11

Status: **PARTIAL — dry-run infrastructure only**

## Audit Summary

Phase 5 can safely proceed only as a read-only dry-run implementation. The repository now has the Phase 2-4 foundations needed to evaluate generated proposal candidates without mutating routing policy:

- Phase 2 feedback: `RunTraceRecorder`, `OutcomeAttributor`, `PatternDetector`, `/api/v1/feedback/patterns`
- Phase 3 simulation: `ShadowRouter`, `PolicySimulator`, `/api/v1/simulation/policy-delta`
- Phase 4 generated proposals: `PolicyProposer`, `ProposalValidator`, `ProposalSerializer`, `GET /api/v1/proposals/generated`
- Proposal lifecycle: create/list/get/approve/reject/deactivate/rollback, `active_routing_policy()`, `confirm_policy_override`, `team:admin`, safe-tier validation, and audit log events

## Prerequisites Confirmed

- PR #35 merged to `main` as `676e1b23cf0fd50372a3bbb62d8f985f0a5bb76a`.
- Latest `tests` GitHub workflow on `main` passed for `676e1b23`.
- Local `cargo test -p engine` passed before work.
- `uv run --no-project python scripts/check_agent_handoff.py` passed before work.
- Generated proposals remain read-only and are not persisted or activated.
- Proposal rollback exists for active manual proposals.
- Active policy can be inspected via `active_routing_policy()`.
- Safe-tier validation exists through `is_safe_policy_override_tier()`.
- Proposal lifecycle audit events exist for create and status transitions.

## Approved Scope

Approved implementation scope is dry-run only:

- `AutoAdjustmentPolicy` evaluates generated candidates and emits `auto_adjustment_policy_decision.v1`.
- `AutoAdjustmentGuard` reports disabled/dry-run state and keeps active mode blocked.
- `PolicySnapshotPreview` emits deterministic read-only `policy_snapshot.v1` previews.
- `GET /api/v1/auto-adjustments` returns gate state, dry-run decisions, and snapshot previews.
- Store report uses generated candidates and does not create, approve, activate, deactivate, or rollback proposals.

## Disallowed Scope

The following remain explicitly not approved:

- Active automatic adjustment
- `POST /api/v1/auto-adjustments/apply`
- Auto-adjustment rollback endpoint
- Auto-approval or auto-activation of generated proposals
- Provider/CLI/auth/security/deploy boundary expansion
- Hard constraint mutation
- Target repository writes
- DB migrations
- Release/tag/deploy behavior
- Dashboard, TypeScript SDK, or Python SDK changes in this PR

## Remaining Risks

- Dry-run decisions can identify eligible candidates, but active apply is intentionally unavailable.
- Snapshot support is preview-only and not persisted.
- Future active apply must prove deterministic snapshot persistence and rollback before approval.
- Full stack verifier is blocked locally because `bun` is missing from `PATH`; dashboard/TS changes are intentionally avoided.

## Runtime Gate

- Default mode is disabled when `ACP_ENABLE_AUTO_ADJUSTMENT` is unset.
- Dry-run mode requires both `ACP_ENABLE_AUTO_ADJUSTMENT=1` and `ACP_AUTO_ADJUSTMENT_DRY_RUN=1`.
- Active mode is reserved and unreachable in this PR even when `ACP_ENABLE_AUTO_ADJUSTMENT=1`.

## Rollback Strategy For Future Active Mode

Future active implementation must create a persisted `policy_snapshot.v1` before activation. Rollback must restore `active_policy_before`, record an audit event, and prove the active routing policy matches the snapshot. Until that exists, dry-run remains the maximum approved scope.

## Test Matrix

Required dry-run tests:

- Disabled mode is default.
- `GET /api/v1/auto-adjustments` reports disabled mode when the env gate is absent.
- Dry-run produces policy decisions and snapshot previews.
- Dry-run does not create `controlled_loop_policy_proposals` rows.
- Dry-run does not activate proposals or mutate `active_routing_policy()`.
- Policy rejects unsafe CLI/provider tiers, missing evidence, weak confidence, missing simulation evidence, simulation regression, and failed safety flags.
- Guard blocks active apply and reports `max_adjustments_remaining = 0`.
- Existing generated proposals endpoint remains read-only.
- Existing Phase 1-4 and proposal lifecycle tests still pass.
