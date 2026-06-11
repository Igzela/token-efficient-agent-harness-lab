# Phase 5 Active Trial Playbook

Date: 2026-06-11

Status: **PARTIAL / ACTIVE_CORE_HARDENED / TRIAL_PLAYBOOK_READY - Phase 5 is not final DONE until this drill is completed and signed off.**

This playbook is the real-world acceptance drill for Phase 5 active auto-adjustment. It validates that the active apply and rollback core can be operated safely under real workflow conditions without expanding automation scope.

## Scope

In scope:

- Verify disabled, dry-run, active apply, and rollback behavior for safe tier-map override candidates.
- Use existing endpoints: `GET /api/v1/auto-adjustments`, `POST /api/v1/auto-adjustments/apply`, `POST /api/v1/auto-adjustments/{adjustment_id}/rollback`, and `GET /api/v1/audit`.
- Confirm proposal, snapshot, active policy, and audit evidence.
- Record operator signoff before Phase 5 final seal.

Out of scope:

- Background scheduling.
- Batch auto-apply.
- Persistent daemon auto-adjustment loop.
- Provider, CLI, auth, security, or deploy boundary expansion.
- Target repository writes.
- Release, tag, or deploy behavior.
- Phase 6.
- Dashboard or SDK changes.

Stop rule: if any active trial verification fails, immediately disable active mode by unsetting `ACP_AUTO_ADJUSTMENT_ACTIVE` or setting it to `0`, restart the engine if required by the local launch method, and do not proceed to Phase 5 seal.

## Operating Modes

Phase 5 has three runtime modes.

| Mode | Environment | Behavior |
|---|---|---|
| Disabled | `ACP_ENABLE_AUTO_ADJUSTMENT` unset or not `1` | Default mode. `GET /api/v1/auto-adjustments` may report candidates and blocked reasons, but no active apply is available. |
| Dry-run | `ACP_ENABLE_AUTO_ADJUSTMENT=1` and `ACP_AUTO_ADJUSTMENT_DRY_RUN=1` | Read-only preview mode. Decisions and snapshot previews are visible. Active apply is blocked. No proposal row is created and `active_routing_policy` must not change. |
| Active | `ACP_ENABLE_AUTO_ADJUSTMENT=1`, `ACP_AUTO_ADJUSTMENT_ACTIVE=1`, and `ACP_AUTO_ADJUSTMENT_DRY_RUN` unset or not `1` | Active apply is available for one eligible candidate per request. Apply still requires configured local auth, `team:admin`, and `confirm_auto_adjustment=true` on each request. |

Rules:

- Disabled is the default.
- Dry-run blocks active apply even when `ACP_AUTO_ADJUSTMENT_ACTIVE=1`.
- Active requires both active gates and no dry-run.
- Active mode never removes per-request admin auth or confirmation requirements.
- Rollback requires configured local auth, `team:admin`, and `confirm_auto_adjustment_rollback=true`.

## Trial Prerequisites

Complete these checks before starting an active trial:

- `main` is clean, current, and synced with `origin/main`.
- CI is green for the base commit.
- Handoff guard passes: `uv run --no-project python scripts/check_agent_handoff.py`.
- Secret scan passes through the handoff guard, or directly through `uv run --no-project python scripts/acp_secret_scan.py`.
- No pending high-risk PRs affect auth, provider, CLI, security, deploy, DB migration, or policy mutation boundaries.
- Generated proposal candidates exist in `GET /api/v1/proposals/generated` or `GET /api/v1/auto-adjustments`.
- The selected candidate passes `ProposalValidator`.
- The selected candidate passes `AutoAdjustmentPolicy`.
- `GET /api/v1/auto-adjustments` shows the expected gate state for the current environment.
- Rollback endpoint availability is reported as `rollback_endpoint_available: true`.
- Audit endpoint is available: `GET /api/v1/audit?limit=50`.
- Operator knows the code rollback commit range for PR #39, PR #38, and PR #37.
- Operator has an admin API key with `team:admin`, `dispatch:read`, and `audit:read` scopes.
- Operator records whether PostgreSQL verification is available through `ACP_TEST_DATABASE_URL`.

## Candidate Selection Rules

A candidate is eligible for this trial only when all checks are true:

- The adjustment is a safe tier-map override only.
- `target_tier` is in the safe override tier set.
- Candidate confidence meets the strict Phase 5 threshold.
- Evidence trace IDs exist.
- Simulation evidence exists.
- Simulation evidence shows no regression above the allowed threshold.
- Safety flags are all true.
- Candidate has no provider, CLI, auth, security, or deploy boundary expansion.
- Candidate performs no target repository writes.
- Candidate performs no hard constraint mutation.
- Candidate is present in the `decisions` list from `GET /api/v1/auto-adjustments`.
- Candidate policy decision is eligible and has no blocked reasons.

Do not use a candidate if any eligibility fact is ambiguous.

## Dry-Run Trial

Start from disabled mode:

```bash
unset ACP_ENABLE_AUTO_ADJUSTMENT
unset ACP_AUTO_ADJUSTMENT_DRY_RUN
unset ACP_AUTO_ADJUSTMENT_ACTIVE
```

Call the report endpoint:

```bash
curl -sS \
  -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  "http://127.0.0.1:8080/api/v1/auto-adjustments?limit=50"
```

Expected disabled output shape:

```json
{
  "schema_version": "auto_adjustments_report.v1",
  "mode": "disabled",
  "env_gate": false,
  "dry_run": false,
  "no_live_mutation": true,
  "active_apply_available": false,
  "rollback_endpoint_available": true,
  "guard": {},
  "decisions": [],
  "snapshot_previews": [],
  "active_auto_adjustments": [],
  "blocked_reasons": []
}
```

Enable dry-run and restart the engine if the local launch method reads env only on startup:

```bash
export ACP_ENABLE_AUTO_ADJUSTMENT=1
export ACP_AUTO_ADJUSTMENT_DRY_RUN=1
unset ACP_AUTO_ADJUSTMENT_ACTIVE
```

Call the report endpoint again:

```bash
curl -sS \
  -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  "http://127.0.0.1:8080/api/v1/auto-adjustments?limit=50"
```

Verify:

- `schema_version` is `auto_adjustments_report.v1`.
- `mode` is `dry_run`.
- `env_gate` is `true`.
- `dry_run` is `true`.
- `no_live_mutation` is `true`.
- `active_apply_available` is `false`.
- `rollback_endpoint_available` is `true`.
- `decisions` contains policy decisions for generated candidates when candidates exist.
- `snapshot_previews` contains deterministic preview objects when candidates exist.
- No row is created in `controlled_loop_policy_proposals`.
- No active row is created in `controlled_loop_policy_snapshots`.
- `active_routing_policy` does not change.
- `GET /api/v1/audit?limit=50&search=auto_adjustment` has no `auto_adjustment.apply.accepted` event for the dry-run.

SQLite inspection, when using a local SQLite data file:

```bash
sqlite3 "$ACP_DATABASE_PATH" \
  "SELECT COUNT(*) FROM controlled_loop_policy_proposals;"

sqlite3 "$ACP_DATABASE_PATH" \
  "SELECT COUNT(*) FROM controlled_loop_policy_snapshots WHERE status = 'active';"
```

If proposal or snapshot rows already exist from previous manual testing, record the before and after counts and verify they are equal.

## Active Apply Trial

Disable dry-run and enable active mode:

```bash
export ACP_ENABLE_AUTO_ADJUSTMENT=1
unset ACP_AUTO_ADJUSTMENT_DRY_RUN
export ACP_AUTO_ADJUSTMENT_ACTIVE=1
```

Restart the engine if required by the local launch method. Then fetch the active report and select one eligible `candidate_id`:

```bash
curl -sS \
  -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  "http://127.0.0.1:8080/api/v1/auto-adjustments?limit=50"
```

Apply exactly one eligible candidate:

```bash
curl -sS -X POST \
  -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"confirm_auto_adjustment":true,"candidate_id":"'"$CANDIDATE_ID"'"}' \
  "http://127.0.0.1:8080/api/v1/auto-adjustments/apply"
```

Expected apply response shape:

```json
{
  "schema_version": "auto_adjustment_apply_result.v1",
  "adjustment_id": "policy-adjustment-...",
  "snapshot_id": "policy-snapshot-...",
  "proposal_id": "proposal-...",
  "candidate_id": "...",
  "policy_key": "...",
  "target_tier": "...",
  "status": "active",
  "applied": true,
  "blocked_reasons": [],
  "rollback_endpoint": "/api/v1/auto-adjustments/.../rollback"
}
```

Verify:

- Response `schema_version` is `auto_adjustment_apply_result.v1`.
- `applied` is `true`.
- `adjustment_id` is present.
- `snapshot_id` is present.
- `proposal_id` is present.
- `policy_key` matches the selected candidate's policy key.
- One active proposal was created for that candidate.
- `active_routing_policy` changed only for that `policy_key`.
- `GET /api/v1/auto-adjustments` lists the adjustment in `active_auto_adjustments`.
- `GET /api/v1/audit?limit=50&search=auto_adjustment` includes `auto_adjustment.snapshot.created`.
- `GET /api/v1/audit?limit=50&search=auto_adjustment` includes `auto_adjustment.apply.accepted`.
- No provider, CLI, auth, security, deploy, target-write, release, tag, or deploy behavior changed.

SQLite inspection examples:

```bash
sqlite3 "$ACP_DATABASE_PATH" \
  "SELECT proposal_id, status, task_domain, task_intent, target_tier FROM controlled_loop_policy_proposals WHERE proposal_id = '$PROPOSAL_ID';"

sqlite3 "$ACP_DATABASE_PATH" \
  "SELECT adjustment_id, snapshot_id, status, policy_key, target_tier FROM controlled_loop_policy_snapshots WHERE adjustment_id = '$ADJUSTMENT_ID';"
```

## Re-Entry Rejection Drill

Attempt to apply the same candidate again:

```bash
curl -sS -X POST \
  -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"confirm_auto_adjustment":true,"candidate_id":"'"$CANDIDATE_ID"'"}' \
  "http://127.0.0.1:8080/api/v1/auto-adjustments/apply"
```

Verify:

- The response is either `auto_adjustment_apply_result.v1` with `applied: false`, or an HTTP rejection such as `BAD_REQUEST`, according to the current API shape.
- `blocked_reasons` includes a duplicate candidate, active candidate, or active `policy_key` reason.
- `GET /api/v1/audit?limit=50&search=auto_adjustment.apply.rejected` includes `auto_adjustment.apply.rejected`.
- `active_routing_policy` remains unchanged from the post-apply state.
- No additional active row is created for the same `policy_key` in `controlled_loop_policy_snapshots`.

Attempt another candidate with the same `policy_key`, if one exists in the generated candidate set:

```bash
curl -sS -X POST \
  -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"confirm_auto_adjustment":true,"candidate_id":"'"$SAME_POLICY_KEY_CANDIDATE_ID"'"}' \
  "http://127.0.0.1:8080/api/v1/auto-adjustments/apply"
```

Verify:

- The request is rejected before policy mutation.
- `blocked_reasons` identifies an existing active adjustment for the `policy_key`.
- Audit contains `auto_adjustment.apply.rejected`.
- `active_routing_policy` remains unchanged.

## Rollback Drill

Call rollback for the active `adjustment_id`:

```bash
curl -sS -X POST \
  -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"confirm_auto_adjustment_rollback":true}' \
  "http://127.0.0.1:8080/api/v1/auto-adjustments/$ADJUSTMENT_ID/rollback"
```

Expected rollback response shape:

```json
{
  "schema_version": "auto_adjustment_rollback_result.v1",
  "adjustment_id": "...",
  "snapshot_id": "...",
  "proposal_id": "...",
  "policy_key": "...",
  "target_tier": "...",
  "status": "rolled_back",
  "rolled_back": true,
  "blocked_reasons": []
}
```

Verify:

- Response `schema_version` is `auto_adjustment_rollback_result.v1`.
- `rolled_back` is `true`.
- Snapshot status is `rolled_back`.
- Previous active proposal is restored when one existed.
- `active_routing_policy` returns to the previous state for that `policy_key`.
- `GET /api/v1/audit?limit=50&search=auto_adjustment` includes `auto_adjustment.rollback.accepted`.
- Repeated rollback is rejected or returned as a safe blocked result with `rolled_back: false`.
- Corrupted hash rollback is rejected and includes a blocked reason equivalent to `snapshot safety hash mismatch`.

Repeated rollback check:

```bash
curl -sS -X POST \
  -H "Authorization: Bearer $ACP_ADMIN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"confirm_auto_adjustment_rollback":true}' \
  "http://127.0.0.1:8080/api/v1/auto-adjustments/$ADJUSTMENT_ID/rollback"
```

Corrupted hash rollback must be exercised only in an isolated trial database. Do not corrupt a shared or production-like operator database. In an isolated SQLite drill, mutate one snapshot hash, call rollback, verify rejection, then discard the database.

## Failure Response Playbook

If apply is rejected:

- Stop active trial.
- Keep `ACP_AUTO_ADJUSTMENT_ACTIVE` disabled until the rejection is understood.
- Read `blocked_reasons` from `auto_adjustment_apply_result.v1` or the HTTP error.
- Check candidate eligibility, `ProposalValidator`, `AutoAdjustmentPolicy`, env gates, admin auth, and confirmation.
- Do not select a less-safe candidate to force success.

If rollback is rejected:

- Stop active trial.
- Disable `ACP_AUTO_ADJUSTMENT_ACTIVE`.
- Read `blocked_reasons` from `auto_adjustment_rollback_result.v1` or the HTTP error.
- Verify `adjustment_id`, snapshot status, snapshot safety hash, and linked proposal status.
- If an active policy remains changed, keep the service in disabled mode and escalate before any Phase 5 seal.

If snapshot hash mismatch occurs:

- Treat the snapshot as tampered or inconsistent.
- Do not bypass hash validation.
- Disable active mode.
- Preserve the database for inspection if this is not an isolated corruption drill.

If linked proposal is stale:

- Disable active mode.
- Compare snapshot `proposal_id` to `controlled_loop_policy_proposals`.
- Verify whether another operator changed the same `policy_key`.
- Do not run another active apply for that policy key until the stale link is explained.

If active policy differs from expected:

- Disable active mode.
- Compare before/after active policy state for the affected `policy_key`.
- Use the rollback endpoint before any code revert when an active adjustment has already been applied.
- Escalate if rollback cannot restore the expected policy.

If CI fails after PR:

- Do not merge or seal Phase 5.
- Fix the failing check on the PR branch or revert PR #39 if the docs/scripts introduced the failure.
- Re-run handoff guard and CI.

If migration mismatch or PostgreSQL behavior differs:

- Do not mark PostgreSQL accepted.
- Record the exact PostgreSQL version, `ACP_TEST_DATABASE_URL` availability, failing command, and failure output.
- Keep SQLite and PostgreSQL acceptance separate in the checklist.

If audit log is missing a required event:

- Stop active trial.
- Disable active mode.
- Verify `audit:read` access and query filters.
- Do not seal Phase 5 until `snapshot.created`, `apply.accepted`, and `rollback.accepted` evidence is present.

If env gates behave unexpectedly:

- Stop active trial.
- Disable active mode.
- Print the process environment used by the engine launch method.
- Restart with only the intended env variables.
- Verify dry-run wins over active before any apply attempt.

## Runtime Rollback and Code Rollback Plan

- When an active adjustment has already been applied, use the runtime rollback endpoint before any code revert.
- After runtime rollback, verify `active_routing_policy`, `controlled_loop_policy_snapshots`, `controlled_loop_policy_proposals`, and audit log evidence.
- If PR #39 docs or any optional trial support scripts are wrong, revert PR #39.
- If PR #38 hardening broke operation, revert PR #38 after recording the failing safety-hardening behavior.
- If PR #37 active apply or rollback path is unsafe, revert PR #37 after runtime rollback is complete.
- Never revert blindly before considering runtime rollback of active policy state.
- After code revert, rerun CI and `uv run --no-project python scripts/check_agent_handoff.py`.

## Acceptance Checklist

Complete this checklist manually before Phase 5 final seal:

- [ ] Disabled mode verified.
- [ ] Dry-run mode verified.
- [ ] Active apply verified.
- [ ] Re-entry rejection verified.
- [ ] Rollback verified.
- [ ] Corrupted rollback rejected.
- [ ] Repeated rollback rejected or safe no-op verified.
- [ ] Audit events verified.
- [ ] Active policy limited to one `policy_key`.
- [ ] No provider, CLI, auth, security, or deploy expansion.
- [ ] No target repo writes.
- [ ] No release, tag, or deploy.
- [ ] SQLite path verified.
- [ ] PostgreSQL path verified or documented as unavailable.
- [ ] CI green.
- [ ] Handoff guard green.
- [ ] Secret scan green.
- [ ] Operator signoff recorded.

## Signoff Record

| Field | Value |
|---|---|
| Operator | |
| Date/time | |
| Base commit | |
| Trial branch/PR | |
| SQLite result | |
| PostgreSQL result | |
| CI result | |
| Handoff guard result | |
| Secret scan result | |
| Final decision | |
