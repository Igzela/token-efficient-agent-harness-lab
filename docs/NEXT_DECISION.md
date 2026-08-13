# Next Decision

Last updated: 2026-08-13.

This document owns one current execution or planning window only. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only packet sketches belong in `docs/FUTURE_ROUTE.md`; the Plan Execution Ledger and merged history retain detailed lifecycle evidence. Live PR, CI, review, and mergeability facts come only from a fresh context capsule.

## Current Direction

The repository improves verifiable task delivery only under hard quality, safety, evidence, compatibility, recovery, rollback, and authority gates. A route label, future-route sketch, model response, or candidate PR does not authorize implementation or an external effect.

The repository-maintenance route is continuous only through the existing Plan Execution Ledger, dispatcher, worktree, PR, CI, review, merge, closeout, and context owners. It does not create product-runtime authority, auto-merge, an unauthorized Provider call, target write, release, deployment, EFFECT execution, or T3 authority.

The durable B2 rule is caller-supplied finite `expires_at` on `rwe_run_authorization.v2`. Provider-free viability preflight is accepted. The four-cell RUN is complete with honest `controlled_failure`. The current window is `PE7-RWE-V2-VIABILITY-CLOSEOUT-1` `READY_FOR_EXECUTION`: independently validate, redact, digest, and classify that run without another Provider request. Do not rerun a failed cell or upgrade the claim.

## Authoritative Forward Order

```text
[window: PE7-RWE-V2-VIABILITY-CLOSEOUT-1 — READY_FOR_EXECUTION, evidence_review]

→ remaining ordered FUTURE_ROUTE packets
```

Every successor remains routing-only until its accepted predecessor closes and the promotion planner proves a bounded current-main contract. A negative, insufficient, unknown, or authority-required disposition is `DECISION_REQUIRED` and rewrites or pauses the route; it never silently follows the nominal order.

## Active Routing

1. `PE7-RWE-V2-VIABILITY-CLOSEOUT-1` — `READY_FOR_EXECUTION`

## Completed (PE7-RWE-V2-VIABILITY-RUN-1)

**Historical state:** `COMPLETE`

**Historical evidence:** PR #441 exact head `ba47462d6cd200d28cb55b1f547924b52afa0584`; merge `2933ba1353f1cda3fc82209b6025094afb79b29e`; exact-head `PASS`; canonical workflow `31704360890`; 4/4 controlled_failure `run-live-20260813-v2c` / `auth-live-v2-003`.

## Packet PE7-RWE-V2-VIABILITY-CLOSEOUT-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** PE7-RWE-V2-VIABILITY-RUN-1 — COMPLETE on accepted main `2933ba1353f1cda3fc82209b6025094afb79b29e` (PR #441 exact head `ba47462d6cd200d28cb55b1f547924b52afa0584`; merge `2933ba1353f1cda3fc82209b6025094afb79b29e`; exact-head `PASS`; canonical workflow `31704360890`; 4/4 controlled_failure `run-live-20260813-v2c` / `auth-live-v2-003`).

**Class:** `CLOSEOUT`

**Worker tier:** `T2`

**Risk class:** `none`

**Verification family:** `evidence_review`

**Outcome:** Independently validate, redact, digest, and classify the v2 run without another Provider request.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/FUTURE_ROUTE.md`, `docs/MODULE_MAP.md`, `docs/NEXT_DECISION.md` only. Evidence validation and canonical status only. Do not rerun a failed cell, retune the envelope, repair code, or upgrade the claim.

**Exit:** A durable redacted receipt bound to the restricted bundle digest and exact run/cell identities, with `VIABLE`, `CONTROLLED_FAILURE`, `OUTCOME_UNKNOWN`, or `INSUFFICIENT` disposition.

**Stop:** Raw/redacted mismatch, missing failure/cost evidence, unverifiable cleanup, or any claim stronger than lifecycle viability.

### 11. Weak-Agent Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"allowed_outputs": ["A durable redacted v2 closeout receipt with an honest CONTROLLED_FAILURE, VIABLE, OUTCOME_UNKNOWN, or INSUFFICIENT disposition.", "Canonical status and routing synchronized to that receipt."], "allowed_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md"], "authority_consumption_allowed": false, "dispatch_lane": "provider_free_repository_maintenance", "expected_artifacts": ["Accepted packet receipt index. (docs/CURRENT_STATUS.md:Accepted)"], "external_effect_limit": 0, "forbidden_changes": ["Do not use FUTURE_ROUTE static paths as current-main authority.", "Do not create a second controller, ledger, queue, lease, store, or workflow owner.", "Do not write the target default branch.", "Do not auto-merge."], "forbidden_next_actions": ["Do not treat missing, conflicting, stale, or outcome-unknown routing or receipts as success.", "Do not start a successor whose promotion candidate has not been independently accepted.", "Do not invent a B2 duration freeze constant.", "Do not rerun a failed cell, retune the envelope, repair code, or upgrade the claim."], "goal": "Independently validate, redact, digest, and classify the v2 four-cell run without another Provider request.", "ordered_steps": ["docs/CURRENT_STATUS.md, docs/NEXT_DECISION.md: Bind exact run/cell identities, redacted cost/failure evidence, and a restricted-bundle digest.", "docs/CURRENT_STATUS.md, docs/FUTURE_ROUTE.md, docs/NEXT_DECISION.md: Record the closeout disposition without promoting measurement-readiness."], "packet_id": "PE7-RWE-V2-VIABILITY-CLOSEOUT-1", "packet_state": "READY_FOR_EXECUTION", "pause_gates": ["Stop when an owner, caller, test, path, operation, destination, or decision cannot be re-proved from accepted main.", "Stop when exact-head review or canonical CI is missing, stale, failed, or conflicting.", "Do not retry a possibly executed external effect whose outcome is unknown."], "plan_lane_state": "plan_lane_active", "prerequisite_receipts": ["PR #441 exact head `ba47462d6cd200d28cb55b1f547924b52afa0584`; merge `2933ba1353f1cda3fc82209b6025094afb79b29e`; exact-head `PASS`; canonical workflow `31704360890`; 4/4 controlled_failure run-live-20260813-v2c auth-live-v2-003"], "prerequisites": ["PE7-RWE-V2-VIABILITY-RUN-1"], "private_paths_allowed": false, "promotion_evidence_sha256": "1052578b6d415bf6dceccadd7396eea3e59080d9b00b0ef29e6f38d9ab1c2fb5", "read_paths": ["docs/CURRENT_STATUS.md", "docs/FUTURE_ROUTE.md", "docs/MODULE_MAP.md", "docs/NEXT_DECISION.md"], "risk_class": "none", "rollback": "Revert the current window and retain detailed lifecycle evidence. (proved by docs/NEXT_DECISION.md:Emergency-stop)", "route_manifest_sha256": "bd4a3701dcac388672f1e1b2694d4361646503502cb62e6f8be1307361a24937", "schema_version": "weak_agent_dispatch.v1", "secret_values_allowed": false, "verification": ["git diff --check", "python scripts/check_agent_handoff.py"], "verification_family": "evidence_review", "worker_tier": "T2"}
-->

## Common Execution Protocol

- Refresh accepted main, the current packet, exact PR heads, CI, review, and ledger receipts before every transition.
- Derive a route action only from the accepted current window, the checked inventory, current-main evidence, and existing durable owners.
- Keep changing PRs Draft; require stable-head independent review and canonical exact-head CI before governed manual merge.
- Treat ordinary worker, CI, review, checkpoint, duplicate, restart, and main-drift failures as bounded recovery transitions through their existing owners.
- Preserve exact receipt bindings and failed/unknown evidence; never convert absence, stale evidence, or an unproven external outcome to success.
- Emergency-stop: revert the current window and retain detailed lifecycle evidence. authority, evaluator, recovery, and schema remain unchanged.

## Hard Stops

- no Provider call; no credential read, target write, release, deployment, automatic merge, EFFECT execution, or T3 action without its separate exact authority;
- no second controller, ledger, queue, lease, workflow owner, store, scheduler, evaluator, authority, or persistence owner;
- no future-route path/prose, model output, local checkpoint, or candidate PR accepted as current-main authority;
- no stale/ambiguous owner, caller, path, verification, rollback, cleanup, retention, evidence, schema, evaluator, authority, or recovery fact treated as proved;
- no retry of an outcome-unknown effect and no deletion or concealment of failure, pause, repair, or recovery evidence.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is the sole routing-only index. Promotion removes exactly one eligible packet, re-derives every `REFRESH_AT_PROMOTION` field from accepted main, validates the resulting candidate, and independently reviews the routing change. No future sketch, static path, or profile alone authorizes code or an effect.
