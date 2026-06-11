# Phase 5 Auto-Adjustment Audit

Date: 2026-06-11

Status: **DONE — active apply + rollback implemented under strict gates**

## Audit Summary

Phase 5 implements a minimal active auto-adjustment loop for safe tier-map changes only. Default mode remains disabled. Dry-run mode remains read-only. Active apply requires two explicit environment gates, configured auth, `team:admin`, and request confirmation.

Implemented surfaces:

- `AutoAdjustmentPolicy` evaluates generated candidates and emits `auto_adjustment_policy_decision.v1`.
- `AutoAdjustmentGuard` enforces disabled, dry-run, and active modes.
- `PolicySnapshotPreview` emits deterministic read-only `policy_snapshot.v1` previews.
- `PolicySnapshotRecord` persists pre-apply policy snapshots with deterministic safety hashes.
- `GET /api/v1/auto-adjustments` returns disabled/dry-run/active gate state, decisions, snapshot previews, and active adjustments.
- `POST /api/v1/auto-adjustments/apply` applies exactly one generated candidate per request.
- `POST /api/v1/auto-adjustments/{adjustment_id}/rollback` restores the previous safe tier-map policy for the affected key after hash validation.
- Audit events cover apply accepted/rejected, snapshot created, rollback accepted/rejected.

## Scope Boundaries

Approved:

- Persistent policy snapshots.
- Active apply endpoint for safe tier-map overrides.
- Rollback endpoint for auto-adjustments.
- Strict env gates.
- Admin auth and confirmation requirements.
- Narrow SQLite/PG schema addition for snapshot persistence.

Not included:

- Provider/CLI execution boundary expansion.
- Auth/security/deploy boundary changes.
- Target repository writes.
- Hard constraint mutation.
- Multi-adjustment batch apply.
- Dashboard/TypeScript SDK changes.
- Automatic background scheduling.
- Auto-merge.
- Release/tag/deploy behavior.

## Runtime Gates

- Default mode: disabled when `ACP_ENABLE_AUTO_ADJUSTMENT` is unset.
- Dry-run mode: `ACP_ENABLE_AUTO_ADJUSTMENT=1` and `ACP_AUTO_ADJUSTMENT_DRY_RUN=1`.
- Active mode: `ACP_ENABLE_AUTO_ADJUSTMENT=1` and `ACP_AUTO_ADJUSTMENT_ACTIVE=1`.
- Dry-run wins over active: `ACP_AUTO_ADJUSTMENT_DRY_RUN=1` blocks active apply even if active is set.

## Apply Path

Active apply requires:

- configured auth
- `team:admin`
- `confirm_auto_adjustment=true`
- active env gates
- dry-run unset
- one generated candidate
- `ProposalValidator` success
- `AutoAdjustmentPolicy` eligibility
- safe target tier
- persisted `PolicySnapshotRecord` before proposal activation

The apply path creates a controlled-loop policy proposal, creates a persistent snapshot, then activates the proposal through the existing proposal approval lifecycle. It does not bypass `active_routing_policy()`.

## Rollback Path

Rollback requires:

- configured auth
- `team:admin`
- `confirm_auto_adjustment_rollback=true`
- existing snapshot
- matching deterministic snapshot safety hash
- active adjustment status

Rollback marks the active proposal rolled back, restores the previously active proposal for the same policy key when one existed, marks the snapshot rolled back, and records audit evidence. Corrupted snapshot hashes and repeated rollback attempts are safely rejected as blocked results.

## Test Matrix

Covered by Rust tests:

- Disabled mode is default.
- Dry-run remains read-only and creates no proposal rows.
- Dry-run does not mutate active routing policy.
- Apply rejects by default.
- Apply rejects with only `ACP_ENABLE_AUTO_ADJUSTMENT=1`.
- Apply rejects when dry-run is enabled.
- Apply requires `team:admin`.
- Apply requires confirmation.
- Apply accepts only with active gates and a valid generated candidate.
- Apply creates one active adjustment and one active proposal.
- Apply writes snapshot and audit events.
- Rollback requires confirmation.
- Rollback validates snapshot hash.
- Corrupted snapshot rollback is blocked.
- Rollback restores the exact prior active policy for the affected policy key.
- Repeated rollback is safely blocked.
- Rollback writes audit events.
- Policy rejects unsafe CLI/provider tiers, missing evidence, weak confidence, missing simulation evidence, simulation success regression, and failed safety flags.
- Generated proposals endpoint remains read-only.

## Remaining Limits

- Dashboard and SDK wiring are intentionally not included in this PR.
- PostgreSQL DDL supports fresh PG stores; PostgreSQL integration test execution still depends on `ACP_TEST_DATABASE_URL` and `pg-tests`.
- This remains a high-risk policy mutation feature and requires human PR review before merge.
