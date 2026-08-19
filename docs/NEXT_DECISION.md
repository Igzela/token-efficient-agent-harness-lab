# Next Decision

Last updated: 2026-08-20.

This document owns one current execution window. Accepted receipts belong in `docs/CURRENT_STATUS.md`; routing-only successors belong in `docs/FUTURE_ROUTE.md`; live PR, CI, review, and mergeability facts come from a fresh context capsule.

## Current Direction

`PE7-CWS-CACHE-PARTITION-1` is complete. `PE7-RWE-CR-RUN-1` remains a retained live-ready blocker. The current window is `PE7-CWS-BENCHMARK-PROTOCOL-1`: freeze the hard-gate-first baseline-versus-CWS measurement protocol. No live Provider, no RUN-1.

## Authoritative Forward Order

```text
[window: PE7-CWS-BENCHMARK-PROTOCOL-1 — READY_FOR_EXECUTION, provider-free; CWS measurement protocol]


```

## Active Routing

1. `PE7-CWS-BENCHMARK-PROTOCOL-1` — `READY_FOR_EXECUTION`

## Retained live-ready blocker (historical: PE7-RWE-CR-RUN-1)

**Historical state:** `BLOCKED_PREREQUISITE`

**Historical source:** `90d093f473a013db512a4adddbd29e9f3a8344d8`

## Completed (PE7-CWS-RUNTIME-INTEGRATION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #585 exact head `7cbe7a0f3660468862302075f024b627a26a0a2e`; squash merge `1dffbc4271a68aebce93a540e7a5793eacefa546`.

## Completed (PE7-CWS-CACHE-PARTITION-1)

**State:** `COMPLETE`

**Accepted evidence:** PR #586 exact head `ecb1367a26d56a633902f0685b3d13d02efff9b4`; squash merge `5a3929dc97b0a94bcec0a95b6e77450238d437da`; exact-head review comments `5347162514` and `5347162743`; canonical workflow `32294752392`; exact-head check `32294752539`.

## Packet PE7-CWS-BENCHMARK-PROTOCOL-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** `PE7-CWS-CACHE-PARTITION-1`

**Class:** `CONTRACT`

**Outcome:** Pre-register a hard-gate-first comparison of the exact post-AC baseline with the CWS treatment using existing token-efficiency, scorecard, artifact, review, and lifecycle-cost owners.

**Allowed delta:** `docs/CURRENT_STATUS.md`, `docs/NEXT_DECISION.md`, and `docs/FUTURE_ROUTE.md` only.

**Exit:** Frozen identities, gates, metrics, missingness, analysis, and stop rules recorded. Maintenance-burden metrics are decision evidence only, not evaluator authority.

**Stop:** Treatment differs beyond accepted context projection; post-hoc thresholds; live Provider; RUN-1.

### Twelve-field contract

1. **Outcome and non-goals.** Protocol freeze only. No preflight execution, Provider POST, or RUN-1.
2. **Prerequisites and evidence.** Cache partition COMPLETE on `5a3929dc`.
3. **Owners and paths.** Canonical docs; cites existing scorecard/artifact/cost owners.
4. **Frozen invariants.** Hard gates before burden evidence. Cache telemetry is missingness-aware observation.
5. **Only semantic delta.** Measurement protocol tables.
6. **Forbidden changes.** No live Provider, threshold tuning from CWS outcomes, new evaluator.
7. **Ordered slices.** Record protocol; stop before preflight.
8. **Failure taxonomy.** Unknown metrics stay unknown; missing cache fields stay missing.
9. **Verification.** Handoff, security baseline, diff check.
10. **Compatibility and rollback.** Revert this PR.
11. **Exit artifact.** Protocol in `docs/CURRENT_STATUS.md`.
12. **Next action.** Promote `PE7-CWS-BENCHMARK-PREFLIGHT-1`.

### 11. Bounded Autonomous Worker Dispatch Capsule

<!-- weak-agent-dispatch:v1
{"schema_version":"weak_agent_dispatch.v1","packet_id":"PE7-CWS-BENCHMARK-PROTOCOL-1","packet_state":"READY_FOR_EXECUTION","dispatch_lane":"provider_free_repository_maintenance","external_effect_limit":0,"authority_consumption_allowed":false,"secret_values_allowed":false,"private_paths_allowed":false,"plan_lane_state":"plan_lane_active","goal":"Freeze the hard-gate-first baseline-versus-CWS measurement protocol without a live Provider call.","allowed_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md"],"read_paths":["docs/CURRENT_STATUS.md","docs/FUTURE_ROUTE.md","docs/NEXT_DECISION.md","docs/ARCHITECTURE_BOOK.md","docs/MODULE_MAP.md"],"allowed_outputs":["A frozen CWS comparison protocol that does not authorize a Provider request."],"prerequisites":["PE7-CWS-CACHE-PARTITION-1"],"prerequisite_receipts":["PE7-CWS-CACHE-PARTITION-1 COMPLETE: PR #586 exact head `ecb1367a26d56a633902f0685b3d13d02efff9b4`; squash merge `5a3929dc97b0a94bcec0a95b6e77450238d437da`; exact-head review comments `5347162514` and `5347162743`; canonical workflow `32294752392`; exact-head check `32294752539`"],"forbidden_changes":["Do not call a Provider.","Do not tune thresholds from observed CWS outcomes.","Do not start PE7-RWE-CR-RUN-1."],"ordered_steps":["Record cache-partition COMPLETE.","Freeze comparison identities and gates.","Record missingness and stop rules.","Stop before preflight."],"verification":["git diff --check","uv run --no-project python tools/check_security_baseline.py","uv run --no-project python scripts/check_agent_handoff.py"],"rollback":"Revert this documentation PR; no Provider request, Store mutation, or threshold change is introduced.","pause_gates":["Stop before a live Provider call.","Stop before PE7-CWS-BENCHMARK-PREFLIGHT-1 execution."],"expected_artifacts":["CWS benchmark protocol in docs/CURRENT_STATUS.md."],"forbidden_next_actions":["Do not start PE7-RWE-CR-RUN-1."],"worker_tier":"T2","known_store_mutations":[]}
-->

## Common Execution Protocol

- `READY_FOR_EXECUTION` packets require a valid dispatch capsule; blocked or decision-required packets carry no executable capsule.
- Refresh accepted `main`, the current packet, exact PR head, CI, and review receipts before each transition.
- Keep a changing PR Draft; run focused/local full checks, then one final stable-head Standards/Spec review, Ready, canonical exact-head CI, manual squash merge, and main refresh.
- No Provider call, credential-value read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner in this packet.
- Unknown, stale, missing, or conflicting evidence remains fail-closed; never retry an effect whose outcome may be unknown.

## Hard Stops

- `DECISION_REQUIRED` when a caller, owner, path, semantics, compatibility fact, rollback, or evidence cannot be re-proved from accepted `main`.
- No Provider call, credential-value read/output/persistence, target write, EFFECT/T3 action, auto-merge, or second runtime/store/authority owner.
- Unknown or possibly executed effects are never treated as success or retried.

## Future Route Boundary

`docs/FUTURE_ROUTE.md` is routing-only. RUN-1 remains a retained live-ready blocker.
