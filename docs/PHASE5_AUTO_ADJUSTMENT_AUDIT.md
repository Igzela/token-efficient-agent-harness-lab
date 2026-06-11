# Phase 5 Auto-Adjustment Audit

Date: 2026-06-11

Status: **PARTIAL / ACTIVE_CORE_HARDENED / TRIAL_PLAYBOOK_READY - active apply + rollback are implemented under strict gates; final Phase 5 seal requires real-world trial evidence from `docs/PHASE5_ACTIVE_TRIAL_PLAYBOOK.md`.**

## Audit Summary

Phase 5 implements a minimal active auto-adjustment core for safe tier-map changes only. Default mode remains disabled. Dry-run mode remains read-only. Active apply requires two explicit environment gates, configured auth, `team:admin`, and request confirmation. The core is not final-sealed until the real-world active trial and rollback drill are completed.

PR #37 implemented the active apply + rollback core. PR #38 hardened that core before any real-world trial: re-entry protection, stable generated candidate identity, stale rollback checks, SQLite/PostgreSQL index parity, HTTP-level safety tests, audit details, and boundary invariant coverage. Phase 5 final DONE still requires the real-world trial playbook run, operator signoff, and a final seal PR.

Implemented surfaces:

- `AutoAdjustmentPolicy` evaluates generated candidates and emits `auto_adjustment_policy_decision.v1`.
- `AutoAdjustmentGuard` enforces disabled, dry-run, and active modes.
- `PolicySnapshotPreview` emits deterministic read-only `policy_snapshot.v1` previews.
- `PolicySnapshotRecord` persists pre-apply policy snapshots with deterministic safety hashes.
- `GET /api/v1/auto-adjustments` returns disabled/dry-run/active gate state, decisions, snapshot previews, and active adjustments.
- `POST /api/v1/auto-adjustments/apply` applies exactly one generated candidate per request.
- `POST /api/v1/auto-adjustments/{adjustment_id}/rollback` restores the previous safe tier-map policy for the affected key after hash validation.
- Audit events cover apply accepted/rejected, snapshot created, rollback accepted/rejected.
- Active apply permits at most one active auto-adjustment per `policy_key`.
- Generated candidate IDs are content-stable rather than list-order-based.

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
- Final Phase 5 seal.

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
- no existing active auto-adjustment for the same `policy_key`
- current generated candidate ID still resolves to the same evidence-backed candidate
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
- linked proposal still active

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
- Apply rejects duplicate candidate re-entry.
- Apply rejects a second active auto-adjustment for the same policy key.
- Apply allows a different policy key when the generated candidate remains valid.
- Apply writes snapshot and audit events.
- Apply rejected audit events include `blocked_reasons`.
- Rollback requires `team:admin`.
- Rollback requires confirmation.
- Missing snapshot rollback is rejected.
- Rollback validates snapshot hash.
- Rollback validates linked proposal state before mutating policy.
- Corrupted snapshot rollback is blocked.
- Stale proposal-state rollback is blocked without changing the current active policy.
- Rollback restores the exact prior active policy for the affected policy key.
- Repeated rollback is safely blocked.
- Rollback writes audit events.
- Rollback rejected audit events include `blocked_reasons`.
- Policy rejects unsafe CLI/provider tiers, missing evidence, weak confidence, missing simulation evidence, simulation success regression, and failed safety flags.
- Generated proposals endpoint remains read-only.
- SQLite migration v13 and PostgreSQL DDL include snapshot indexes for status, proposal, adjustment lookup, policy key, and active-per-key enforcement.

## Remaining Limits

- Dashboard and SDK wiring are intentionally not included in PR #38.
- PostgreSQL DDL supports fresh PG stores; PostgreSQL integration test execution still depends on `ACP_TEST_DATABASE_URL` and `pg-tests`.
- This remains a high-risk policy mutation feature and requires human PR review before merge.
- Candidate staleness is evidence-based because generated candidates do not carry timestamps: apply reselects from the current generated candidate set and reruns `ProposalValidator` plus `AutoAdjustmentPolicy`.
- Final Phase 5 seal requires `docs/PHASE5_ACTIVE_TRIAL_PLAYBOOK.md` completion and operator signoff.
