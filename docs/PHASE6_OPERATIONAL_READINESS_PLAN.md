# Phase 6: Operational Readiness and Observability

Status: **DONE**
Plan created: 2026-06-11
Completed: 2026-06-12
Schema version: 13
Test count baseline: 1534 Rust tests, 0 failures

## Purpose

Phase 6 makes the system observable, operationally verifiable, and safe to continue into Phase 7. It does not add features, expand boundaries, or change runtime behavior.

## Phase 6 Scope

### In-Scope

| # | Item | Acceptance Criterion |
|---|---|---|
| 1 | Structured operational logs for dispatch/regulator decisions | Structured log records exist for dispatch routing, policy evaluation, auto-adjustment paths |
| 2 | Per-decision observability for routing/policy/auto-adjustment paths | Each decision path emits a structured record with decision type, inputs, outcome, and timing |
| 3 | Read-only operator visibility for regulator state | A GET endpoint exposes current regulator state (proposals, snapshots, active adjustments) without mutation |
| 4 | PostgreSQL active-trial status resolution | PostgreSQL active trial is either PASS (with evidence) or BLOCKED (with exact reason and env dependency) |
| 5 | Docs/architecture drift checks | Automated check or documented validation command prevents docs/code/schema drift |
| 6 | Documentation consistency | Architecture Book, CURRENT_STATUS, NEXT_DECISION, MODULE_MAP, DOCS_INVENTORY all agree |

### Out-of-Scope

The following are explicitly NOT Phase 6 work:

- No Phase 7 features (Operator Surface / UI & UX)
- No dashboard active controls (approve/reject/rollback from dashboard)
- No daemonized auto-adjustment loop
- No batch auto-apply
- No new provider/CLI/auth/security/deploy boundary expansion
- No target repo writes
- No runtime behavior changes to dispatch decisions
- No new auto-apply behavior beyond existing Phase 5 gates

## Planned PR Sequence

| PR | Branch | Purpose | Risk | Type |
|---|---|---|---|---|
| #42 | `phase6/scope-lock` | This plan document + acceptance checklist | Low | docs-only |
| #43 | `phase6/structured-observability` | Structured logs for dispatch/regulator/policy paths using tracing crate | Low | additive logging, no behavior change |
| #44 | `phase6/read-only-observability-surface` | GET endpoint exposing regulator state snapshot | Low | read-only GET endpoint |
| #45 | `phase6/postgres-active-trial` | PostgreSQL active trial completion or BLOCKED status documentation | Low | docs if blocked, test-only if available |
| #46 | `phase6/docs-architecture-drift-checks` | Script or CI check for docs/architecture/code drift | Low | script/CI only |
| #47 | `phase6/final-seal` | Seal Phase 6 as DONE, update handoff docs | Low | docs-only |

## PR Detail

### PR #42: Scope Lock (this PR)

- Add `docs/PHASE6_OPERATIONAL_READINESS_PLAN.md`
- Update `docs/NEXT_DECISION.md` to point at Phase 6 sequence
- No code changes

### PR #43: Structured Observability

**Goal:** Replace println/eprintln logging with structured tracing spans for dispatch and regulator decision paths.

**Scope:**
- Initialize `tracing-subscriber` in `engine/src/main.rs` (currently declared in Cargo.toml but unused)
- Add structured `tracing::info!`/`tracing::debug!` spans to:
  - Dispatch routing decisions (tier selected, confidence, cost gate pass/fail)
  - Policy evaluation paths (proposal created, validated, serialized)
  - Auto-adjustment apply/rollback events (snapshot created, applied, rolled back, rejected)
  - Regulator context assembly, feedback recording, simulation runs
- Log payloads must not contain secrets (API keys, tokens, connection strings)
- Existing `MetricsCollector` and `RequestTracer` remain unchanged; structured logs are additive

**Files to modify:**
- `engine/src/main.rs` — tracing subscriber initialization
- `engine/src/http_server/handlers/dispatch.rs` — dispatch path instrumentation
- Regulator module files — decision path instrumentation

**New tests:**
- Serialization tests for structured log fields
- Secret-free log payload test (no API keys, tokens, or connection strings in log output)

**Does NOT:**
- Change dispatch routing logic
- Change regulator behavior
- Add new endpoints
- Expand boundaries

### PR #44: Read-Only Observability Surface

**Goal:** A single GET endpoint that operators can use to inspect current regulator state without triggering mutations.

**Scope:**
- `GET /api/v1/regulator/state` — returns a JSON snapshot combining:
  - Active policy proposals (from `controlled_loop_policy_proposals`)
  - Active policy snapshots (from `controlled_loop_policy_snapshots`)
  - Auto-adjustment status (enabled/disabled, active/dry-run mode)
  - Current tier overrides (if any active)
- Read-only: no POST, no mutation, no state change
- Requires no auth (read-only diagnostic endpoint, consistent with `/api/v1/health`)

**New tests:**
- Empty-state test (no proposals/snapshots returns empty arrays)
- No-mutation test (endpoint does not write to database)

**Does NOT:**
- Add approve/reject/rollback controls
- Add dashboard mutation controls
- Expose secrets

### PR #45: PostgreSQL Active Trial

**Goal:** Resolve PostgreSQL active trial status truthfully.

**If `ACP_TEST_DATABASE_URL` is available:**
- Run the existing `pg-tests` cargo feature integration tests
- Document PASS with evidence (test output, commit hash)
- Update CURRENT_STATUS to reflect PG active trial PASS

**If `ACP_TEST_DATABASE_URL` is not available (expected):**
- Document BLOCKED with exact reason: `ACP_TEST_DATABASE_URL` environment variable not available in this environment
- Record the external dependency: requires a running PostgreSQL instance with a test database
- Update CURRENT_STATUS to track this as an external environment-dependent follow-up
- Do NOT claim PG active trial passed

**Final status wording if blocked:**
> "Phase 6 DONE for operational readiness and observability, with PostgreSQL active trial explicitly tracked as an external environment-dependent follow-up."

### PR #46: Docs/Architecture Drift Checks

**Goal:** Prevent documentation from drifting out of sync with code.

**Scope:**
- A Python script (e.g., `scripts/check_docs_architecture_drift.sh` or `.py`) that validates:
  - Schema version in `CURRENT_STATUS.md` matches `CURRENT_SCHEMA_VERSION` in `migrations.rs`
  - Test count in `CURRENT_STATUS.md` is within reasonable range of actual `cargo test` output
  - Files listed in `DOCS_INVENTORY.md` actually exist
  - Files in `docs/` that are NOT in DOCS_INVENTORY.md are flagged
  - `MODULE_MAP.md` references match actual module directories
- Integrate into the existing handoff guard or document as a standalone validation command

**New tests:**
- Python script tests (if implemented as .py)
- Local script execution verification

### PR #47: Final Seal

**Goal:** Seal Phase 6 as DONE.

**Scope:**
- Update `docs/CURRENT_STATUS.md` with Phase 6 completion status and test count
- Update `docs/NEXT_DECISION.md` to point at Phase 7 (Operator Surface / UI & UX)
- Update `docs/ARCHITECTURE_BOOK.md` with Phase 6 completion status
- Update `docs/DOCS_INVENTORY.md` to reflect Phase 6 completion
- Update `docs/PHASE6_OPERATIONAL_READINESS_PLAN.md` to reflect DONE status
- Run full verification suite
- No code changes

## Acceptance Criteria

Phase 6 is DONE only when ALL of the following are true:

1. Structured operational logs exist for dispatch/regulator decisions
2. Per-decision observability exists for routing/policy/auto-adjustment paths
3. Operators can inspect regulator state via a read-only endpoint without mutating state
4. PostgreSQL active-trial status resolved truthfully (PASS with evidence or BLOCKED with exact reason)
5. Docs/architecture drift checks exist and run in CI or are documented as a validation command
6. Architecture Book, CURRENT_STATUS, NEXT_DECISION, MODULE_MAP, DOCS_INVENTORY are consistent with each other and with code
7. CI is green
8. No Phase 7 features started

## Rollback Strategy

Each PR is independently revertable via `git revert`:

| PR | Rollback |
|---|---|
| #42 | Delete `PHASE6_OPERATIONAL_READINESS_PLAN.md`, revert doc updates |
| #43 | Remove tracing subscriber initialization, revert handler instrumentation, remove structured logging tests |
| #44 | Remove `GET /api/v1/regulator/state` endpoint and its tests |
| #45 | Remove PG trial status documentation |
| #46 | Remove drift check script/CI additions |
| #47 | Revert seal documentation updates |

## Validation Commands

Run for every PR before merge:

```bash
cargo fmt --check
cargo clippy -p engine --all-targets -- -D warnings
cargo test -p engine
uv run --no-project python scripts/check_agent_handoff.py
uv run --no-project python scripts/acp_secret_scan.py
git diff --check
```

Additional per-PR:

| PR | Additional validation |
|---|---|
| #43 | Structured log serialization tests, secret-free log payload tests |
| #44 | Empty-state endpoint test, no-mutation test |
| #45 | `cargo test -p engine --features pg-tests` if `ACP_TEST_DATABASE_URL` available |
| #46 | Python script tests, local script execution test |

## Stop Conditions

Stop and report immediately if any of:

- Repository state unavailable (git status dirty from unknown changes)
- PR #41 not merged (verify: it IS merged as of `26fa181`)
- Phase 5 status inconsistent (Dynamic Regulator Phase 5 should be DONE)
- Runtime code changes needed in docs-only PRs
- PostgreSQL trial reveals a real runtime bug (report, do not fix in Phase 6)
- Any feature expands provider/CLI/auth/security/deploy boundary
- Any feature writes to target repos
- Any feature adds background auto-adjustment
- Any feature adds batch auto-apply
- Any feature adds daemonized auto-adjustment
- Any feature starts dashboard active controls

## Dependency Chain

```
PR #42 (scope lock)
  |
  +-- PR #43 (structured observability)
  |     |
  |     +-- PR #44 (read-only regulator endpoint)
  |
  +-- PR #45 (PG trial) -- independent, can parallel with #43/#44
  |
  +-- PR #46 (drift checks) -- independent, can parallel with #43/#44/#45
  |
  +-- PR #47 (final seal) -- after all above merged
```

PRs #43, #44, #45, and #46 are independent of each other and can be developed in any order. PR #47 depends on all prior PRs being merged.

## Final Status Rules

Phase 6 can be marked DONE if and only if:

- Structured observability is implemented and tested
- Read-only operator observability surface is implemented (or existing equivalent documented and tested)
- Docs/architecture drift checks are implemented
- PostgreSQL active trial is either completed OR explicitly marked as blocked by missing `ACP_TEST_DATABASE_URL` with no false success claim
- CI is green
- No Phase 7 features were started

**If PostgreSQL active trial is blocked**, Phase 6 DONE wording must be:
> "Phase 6 DONE for operational readiness and observability, with PostgreSQL active trial explicitly tracked as an external environment-dependent follow-up."

Do not claim PostgreSQL active trial passed unless it actually passed.

## Post-Phase-6

After Phase 6 is sealed as DONE:

- Next global stage: **Phase 7 — Operator Surface / UI & UX**
- Do not start Phase 7 code in Phase 6
- Phase 7 scope will be defined in its own plan document
