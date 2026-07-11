# Next Decision

## Current Direction

The dispatch kernel, V2, Adaptive Fusion AF-0 through AF-7, Agent Runtime AR-0 through AR-6, Trusted Local Autonomous Execution IAE-0 through IAE-3, scorecard integrity hardening, and the importer-first LangGraph pilot are complete.

PR #166 completed generic `scorecard_artifact.v2`, canonical integrity checks, app-owned persistence, scenario API/Dashboard comparison, and the first real offline LangGraph evidence pair. The next direction is the Post-LGB Product Evolution plan, PE-1 through PE-6. It turns existing evidence and controls into regression detection, budget intelligence, operator decisions, trace-backed policy evaluation, release provenance, and verified recovery.

This is not AR-7, another LGB ladder, or a second control plane.

## Stable Tracks

| Track | Status |
|---|---|
| Core dispatch kernel | Complete |
| Architecture refactor R-series | Sealed at R7 |
| V2 Real Production Output | Complete through V2-5 |
| Adaptive Fusion | Complete through AF-7 |
| Agent Runtime | Complete and sealed at AR-6 |
| Trusted Local Autonomous Execution | Complete through IAE-3 |
| External Runtime Benchmark Boundary | Importer-first LangGraph pilot complete |
| Full Agent Autonomy Mode | Active |
| Agent Autonomous Maintenance Mode | Active |

Historical phase detail remains in `docs/ARCHITECTURE_BOOK.md`, archived plans, merged PRs, and repository history. This file is the single forward-plan artifact.

## Full Agent Autonomy Mode

Maintaining agents may autonomously propose, implement, test, review, merge, and iterate work when it is repo-scoped, testable, observable, and rollbackable.

### Autonomously maintain and evolve

Agents may proceed without per-PR confirmation when they start from latest `main`, choose one bounded PE acceptance slice, preserve existing authority boundaries, add focused and full-stack verification, repair CI until green, document compatibility and rollback, and update existing active docs rather than adding duplicate roadmap/status files.

## Hard Stops

Agents must not commit credentials, falsify evidence, hide failures, remove recovery paths, bypass existing auth/budget/audit/approval controls, create unbounded execution, persist raw sensitive runtime content in evidence, or perform irreversible external destruction without a recovery path.

## Architecture refactor (R-series)

The R-series remains sealed at R7. PE work must extend the existing Rust runtime/API/storage, SDK, Dashboard, release, and recovery owners. A replacement requires an explicit documented decision, threat-model delta, migration plan, verification, and rollback.

## External Runtime Benchmark Direction

External runtimes remain bounded benchmark, replay, or trace-summary ingest targets. Native v1 and generic v2 artifacts share the existing store/API boundary. Comparisons must validate canonical hashes, derived metrics, scenario compatibility, and quality equivalence. Provider calls remain forbidden in CI. Embedded external runners, scheduled external execution, and external target authority remain unauthorized unless this file is explicitly changed.

## Post-LGB Product Evolution Plan

The normative order is PE-1, PE-2, PE-3, PE-4, PE-5, and PE-6. Each stage should be split into scoped PRs. PE-5 may proceed after PE-1 in parallel when no release work conflicts.

| Stage | Priority | Goal | Status |
|---|---|---|---|
| PE-1 | P0 | Token Efficiency Regression Lab | In progress: registry, single-scenario/batch report cores, checked evidence pairs, idempotent LocalProductStore persistence, and repeat-safe file import implemented; bounded trend behavior is next |
| PE-2 | P0/P1 | Budget Intelligence and Anomaly Auto-Pause | Authorized after PE-1 contracts stabilize |
| PE-3 | P1 | Operator Decision Center | Authorized after PE-2 evidence shape exists |
| PE-4 | P1/P2 | Trace-backed Policy Replay | Authorized after sufficient versioned traces exist |
| PE-5 | P1.5 | Release Provenance | Authorized after PE-1; may proceed independently |
| PE-6 | P2 | Fault Injection and Recovery Drills | Authorized after recovery invariants are explicit |

### PE-1 — Token Efficiency Regression Lab

Build a bounded multi-scenario registry over existing scorecard v1/v2 artifacts. Each scenario must define digests, baseline/candidate roles, quality method and threshold, allowed regressions, and comparison metadata. Include at least three fixed summary-only scenarios, including the existing LangGraph pilot.

Compare current evidence with explicit baseline and best-known results across tokens, repeated context, state bytes, cost, latency, retries, and quality. Return `incomparable` when contracts or quality differ. Persist only bounded regression reports through the existing artifact/store/API boundary. Add Dashboard history, trend, baseline, best-known configuration, regression reason, and evidence links.

Initial mode is report-only: no CI blocking, routing change, policy mutation, or provider call. Acceptance requires deterministic recomputation plus tamper, threshold, quality-failure, missing-baseline, incomparable, repeat-import, cross-version, and trend tests.

Implemented slice: `token_efficiency_regression_registry.v1` validates a canonical hash-bound registry of three summary-only scenarios against the existing LangGraph fixture, native deterministic pilot, and local stub runner. It fixes explicit baseline/candidate roles, quality thresholds, allowed regressions, supported v1/v2 artifact contracts, and report-only/zero-provider/zero-mutation metadata. It adds no storage, API, Dashboard, CI-blocking, routing, or policy authority. Next: deterministically recompute bounded current-vs-baseline and best-known regression reports from fixed evidence, including missing-baseline, incomparable, threshold, and quality-failure outcomes.

Implemented slice: `token_efficiency_regression_report.v1` deterministically recomputes one bounded current-vs-baseline/best-known report across tokens, repeated context, state bytes, cost, latency, retries, and quality. Canonical report and artifact hashes reject tampering; explicit outcomes cover pass, regression, missing baseline/best-known, incomparable contracts, and quality failure; native v1 and generic v2 envelopes remain compatible. It remains report-only with no persistence, CI blocking, routing, policy, provider, or mutation authority. Next: add checked evidence pairs for the native deterministic and local-stub scenarios, then registry-wide batch/repeat-import/trend behavior before storage/API/Dashboard work.

Implemented slice: checked `native_scorecard_artifact.v1` baseline/candidate evidence now covers the native deterministic and local-stub scenarios alongside the existing LangGraph pair. Tests rebuild both generated pairs, normalize only exporter capture time, verify the complete remaining envelope, and fix deterministic report hashes and outcomes. The evidence records the expected `baseline.state_bytes` regression for both stateful candidates instead of hiding that trade-off. Next: add deterministic registry-wide batch recomputation plus repeat-import and trend behavior before persistence/API/Dashboard work.

Implemented slice: `token_efficiency_regression_batch.v1` deterministically recomputes every registered scenario with exact coverage, sorted reports, outcome counts, nested report validation, and a canonical batch hash. Input order cannot affect output, missing baselines remain explicit without dropping scenarios, and malformed or tampered nested reports fail closed. The batch remains report-only with no persistence, provider, CI-blocking, routing, policy, or mutation authority. Next: add repeat-import and bounded trend semantics through the existing artifact/store boundary.

Implemented slice: schema v17 adds `regression_report_artifacts` inside the existing `LocalProductStore` SQLite/PostgreSQL boundary. Deterministic report and batch hashes become idempotent artifact IDs; repeated recording returns the existing envelope, and bounded list/scenario reads validate inner self-hashes plus envelope coherence. Audit records contain metadata and hashes only; raw or sensitive payload keys fail closed. The slice adds no API, SDK, Dashboard, provider, CI-blocking, routing, policy, or mutation authority. Next: add the bounded file importer and deterministic trend semantics over this table, then expose the existing boundary through API/SDK/Dashboard.

Implemented slice: the existing bounded local scorecard importer now dispatches `token_efficiency_regression_report.v1` and `token_efficiency_regression_batch.v1` files into the same `LocalProductStore` artifact boundary. It retains the legacy CLI/API name for compatibility, preserves the 1 MiB pre-parse ceiling, supports mixed scorecard/regression directories, and reports deterministic repeats as unchanged. It adds no second importer, provider calls, API/Dashboard state, or mutation authority. Next: add deterministic bounded history/trend semantics over persisted reports.

### PE-2 — Budget Intelligence and Anomaly Auto-Pause

Add simple versioned forecasts for exhaustion time and expected spend/tokens by run, workspace, provider, and model. Detect explainable cost, token, retry, latency, context-growth, and model-mix anomalies. Evidence must include version, window, coverage, confidence, and reason codes.

Automatic pause is permitted only for high-confidence, policy-enabled conditions through existing pause and audit controls. Resume or override requires operator evidence. No automatic termination, silent budget mutation, provider substitution, or opaque forecasting. Acceptance includes sparse-data, false-positive, concurrency, idempotency, audit, resume, and incomplete-pricing tests.

### PE-3 — Operator Decision Center

Create one prioritized derived action queue from existing approvals, blocked or stalled runs, repeated failures, budget risk, benchmark regressions, invalid policy configuration, scheduler state, and rollback candidates.

Each action item must include reason, severity, confidence, evidence links, recommended action, required authority, age, and resolution state. Reuse existing mutation endpoints. The center must not become a second scheduler, policy authority, approval store, or hidden mutation path. Acceptance includes deterministic derivation, deduplication, stale invalidation, permissions, pagination, and evidence-to-action traceability.

### PE-4 — Trace-backed Policy Replay

Replace fixed heuristic simulation incrementally with versioned trace-backed calibration by task class, complexity, provider/model, and execution profile. Record sample size, coverage, time window, confidence, and out-of-distribution status. Refuse recommendations when evidence is sparse, stale, or outside supported coverage.

Compare candidate policies on success, quality, cost, latency, retries, and review outcomes. Reuse existing shadow evaluation, canary, promotion snapshot, pause, and rollback paths. Progress from offline replay to shadow evaluation to bounded canary only after explicit thresholds pass. Keep observed and estimated outcomes separate.

### PE-5 — Release Provenance

Add SPDX or CycloneDX SBOMs, artifact and image signatures, and build attestations tied to source commit, workflow, target, dependency state, and artifact digest. Installer and upgrade verification must reject invalid required evidence while preserving atomic rollback. Keep signing material outside the repository. Acceptance includes tamper rejection, target correctness, verification tests, existing advisory gates, and rollback-compatible installation.

### PE-6 — Fault Injection and Recovery Drills

Define recovery invariants before each drill: permitted data loss, authority behavior, fail-closed expectation, recovery sequence, and required evidence. Add bounded deterministic scenarios for audit-store failure, provider timeout or invalid response, budget concurrency, scheduler restart, database interruption, artifact corruption, upgrade interruption, pause recovery, and duplicate mailbox delivery.

Drills must remain local/CI-safe and isolated from real external state. Record recovery success, divergence, duplicate execution, data loss, fail-open violations, and recovery time. Acceptance requires repeatable assertions, cleanup verification, explicit residual risk, and operator procedures added to `RUNBOOK.md` only after they work.

## Cross-Stage Rules

- Reuse existing `LocalProductStore`, workflow, scheduler, provider, feedback, operator-evidence, Dashboard, SDK, release, and recovery owners.
- Do not add a second scheduler, graph kernel, mailbox, storage layer, policy authority, artifact truth source, or Dashboard state model.
- Use bounded versioned contracts and deterministic recomputation where practical.
- Maintain SQLite/PostgreSQL compatibility when storage changes.
- Separate facts, estimates, recommendations, and live actions.
- Start with observation before granting new mutation authority.
- Include compatibility, residual risk, and rollback in every PR.

## Allowed Next Paths

- Implement the earliest incomplete PE stage in normative order.
- Split a stage into multiple scoped PRs.
- Perform maintenance, regression hardening, CI repair, documentation correction, and real-world validation.
- Advance PE-5 after PE-1 when release work is not already active.
- Supersede the order only by updating this file with rationale, risks, acceptance criteria, and rollback.
- Maintain completed AR, AF, IAE, V2, scorecard, and importer behavior without reopening their phase ladders.

## Auto-Merge Policy

Scoped docs, tests, fixes, implementation, migration, security, release, and authority changes may be merged autonomously when green, reviewed, observable, and rollbackable. External release publication or irreversible effects require verified evidence and recovery.

## Minimum Verification

Run focused checks plus applicable full repository validation:

```bash
cargo fmt --all -- --check
cargo clippy -p engine --all-targets --all-features -- -D warnings
cargo test -p engine
cargo test -p engine --features pg-tests -- --test-threads=1
PYTHONPATH=src uv run --no-project python -m unittest discover -s tests
bash scripts/verify_rust_typescript_stack.sh
bash scripts/check_wire_codegen_drift.sh
uv run --no-project python tools/check_security_baseline.py
uv run --no-project python scripts/check_agent_handoff.py
git diff --check
```

Add release, browser, Docker, backup/restore, or fault-specific checks when those surfaces change.

## Documentation Maintenance

Use existing active docs only. Put durable contracts in `ARCHITECTURE_BOOK.md`, current facts in `CURRENT_STATUS.md`, forward authority here, ownership in `MODULE_MAP.md`, validation discipline in `REAL_WORLD_TESTING_PLAYBOOK.md`, and proven operator procedures in `RUNBOOK.md`.

## Before Starting Autonomous Work

1. Read `docs/CURRENT_STATUS.md`, this file, and `docs/MODULE_MAP.md`.
2. Select the earliest incomplete PE stage and one bounded acceptance slice.
3. Audit current code before assuming functionality is absent.
4. State non-goals and safety boundaries.
5. Implement on a branch, test, review, and repair CI until green.
6. Update active docs with facts.
7. Report PR, commits, compatibility, evidence, CI, residual risk, rollback, and the next unfinished item.
