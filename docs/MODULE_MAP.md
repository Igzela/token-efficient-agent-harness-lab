# Module Map

Last updated: 2026-07-13.

This file maps current ownership. It is not a phase history.

Full Agent Autonomy Mode is active for repository-scoped work that remains testable, observable, reviewable, CI-gated, compatible, and rollbackable. Execution-ready packets in `docs/NEXT_DECISION.md` are the default work units.

## Core Ownership

| Module | State | Purpose | Verification |
|---|---|---|---|
| `AGENTS.md`, `CLAUDE.md`, `docs/NEXT_DECISION.md`, `docs/REAL_WORLD_TESTING_PLAYBOOK.md`, `scripts/check_agent_handoff.py` | active | autonomous authority, packet routing, evidence discipline, and governance-drift prevention | handoff guard, focused documentation checks, CI where required |
| `engine/src/main.rs`, `engine/src/http_server/` | active | Rust engine and API | HTTP and engine tests |
| `engine/src/trusted_local.rs` | active | trusted-local readiness policy | trusted-local tests |
| `dispatch_engine.rs`, `task_analyzer/`, `model_selector.rs`, `budget_manager.rs` | active | dispatch, routing, and bounded budget authority | dispatch and budget tests |
| `engine/src/provider/`, `engine/src/provider/fake.rs` | active | provider adapters, persistent bounded audit/redaction, and fake-provider testing | adapter/audit tests, dependency audit, full stack |
| `engine/src/local_runner_provider.rs`, `engine/src/bin/local_runner_exec.rs` | active | bounded Stub/Fake/Live local runner | local runner/provider tests and binaries |
| `engine/src/cli/` | active | CLI adapters | engine tests |
| `engine/src/agent_memory.rs` | active | bounded AgentState memory-policy helpers | agent memory, context, workflow, operator-evidence tests |
| `workflow/`, `scheduler.rs`, `node_executor.rs`, `executor_pool.rs` | active | workflow, scheduler, executor, routing, and pool accounting | workflow/scheduler/executor tests |
| `engine/src/storage/local_product_store/` | active | SQLite/PostgreSQL-compatible application-owned storage | local store and PG integration tests |
| `target_repo_output.rs`, `target_repo_output/authority.rs` | active | target-output authority | target-output tests |
| `dashboard/` | active | local UI | tests, typecheck, lint, build |
| `sdk/typescript/`, `sdk/python/` | active | SDKs | SDK tests |
| `wire_contract/v1/`, `codegen/` | active | wire contracts and generated types | `scripts/check_wire_codegen_drift.sh` |
| `scripts/`, `tools/`, `.github/workflows/` | active | scripts, pilots, CI, release packaging, dependency/action-pin gates, backup/restore, install/upgrade, and atomic rollback | script-specific tests, workflow checks, security baseline, CI |

## Token-Efficiency and Product-Evolution Ownership

| Capability | Owning paths and boundary |
|---|---|
| PE-1 regression lab | regression scripts/tests/fixtures; native and generic scorecard stores; scorecard HTTP/SDK/Dashboard read surfaces; report-only |
| PE-2 budget intelligence and auto-pause | `budget_manager.rs`, `budget_anomaly.rs`, provider audit/cost evidence, budget evidence store, existing workflow pause/audit/resume owners; no auto-kill or second pause owner |
| PE-3 operator decision center | `engine/src/operator_decision/mod.rs`, derived queue/store adapter, operator decision HTTP handlers, SDKs, read-only Dashboard, and allowlisted existing action owners; no generic executor |
| PE-4 trace-backed policy replay | replay eligibility/recorder/offline/shadow/experiment/promotion/contextual-policy owners; schema v21 dispatch provenance; replay artifact store; read-only API/SDK/Dashboard; existing canary/pause/rollback authority reused |
| Scorecard validation/comparison | `scripts/token_efficiency_scorecard.py`, `scripts/scorecard_comparison.py` |
| Native/local/LangGraph evidence | native scorecard export, local runner, provider-gated runner, LangGraph capture/import, fixtures, and existing scorecard store/API |

## PE-5 Release Provenance Ownership

PE-5 is complete and independently acceptance-sealed. Grouped `PE5-IMPLEMENT-1` completed internal `PE5-CONTRACT-1` through `PE5-PUBLISH-1`; detailed packet contracts and evidence remain in `docs/NEXT_DECISION.md`.

| Capability | Primary owners | Boundary |
|---|---|---|
| Release provenance contract | `scripts/release_provenance.py`, `tools/test_release_provenance.py`, `tools/test_release_closeout.py`, existing release/build/install/upgrade scripts and workflow | one versioned `release_provenance.v1` and `release_verification.v1` contract; strict media/predicate/policy/transcript bindings; no publication or signing in the contract helper |
| Deterministic SBOM | `scripts/release_provenance.py`, existing package/container builders; Cargo/Bun/Python locks; `tools/test_release_provenance.py` | canonical SPDX 2.3 package/container subject; target/artifact/lockfile-bound; no second package pipeline |
| Signed provenance/attestation | existing `.github/workflows/release.yml`, pinned `actions/attest` commit `a1948c3f048ba23858d222213b7c278aabede763`, `scripts/release_provenance.py`, fixture tests | external ephemeral production identity; no persistent private key or Vader signing credential |
| Installer/upgrader verification | `scripts/install-from-release.sh`, `scripts/install.sh`, `scripts/upgrade.sh`, `tools/test_release_installation.py`, `tools/test_release_closeout.py`, existing checksum/staging/health/atomic rollback owners | complete evidence verifies before extraction/activation; safe archive members; previous known-good state preserved through health success |
| Publish gate | `.github/workflows/release.yml`, `scripts/check_release_contract.sh`, existing artifact upload/release helpers, action-pin/security checks | build → SBOM → attest → provenance → verify → publish ordering; no unauthorized public release/tag/deploy |
| PE-5 closeout | release contract/tests/workflow dry run, installer rollback, `tools/test_release_closeout.py`, active docs | independent acceptance without requiring a real public release; sealed under PR #211 |

## PE-6 Fault Injection and Recovery Ownership

PE-6 grouped implementation is merged after the PE-5 acceptance seal. Internal milestones completed in order within `PE6-IMPLEMENT-1`; `PE6-CLOSE-1` independently audited and sealed the chain.

| Capability | Primary owners | Boundary |
|---|---|---|
| Recovery invariants and fault contract | `scripts/fault_drill_contract.py`, `tools/test_fault_drill_contract.py` | `fault_scenario.v1`, `fault_drill_result.v1`, recovery/cleanup evidence, bounded reason codes; no runtime schema or mutation |
| Fault-injection harness | `scripts/fault_drill_harness.py`, `scripts/fault_drill_registry.py`, `tools/test_pe6_harness_drill.py` | fixed registered child commands, deterministic worker/resource identities, timeout and finally cleanup; no arbitrary command/runtime service |
| Storage drills | `engine/tests/test_pe6_fault_drills.rs`, `LocalProductStore`, migrations, integrity/audit tables, `BackupManager` | SQLite atomicity/replay/restart, integrity, backup/restore/tamper, PG service-gated owner check; no real DB corruption |
| Workflow/executor drills | `engine/tests/test_pe6_fault_drills.rs`, workflow runs, scheduler, node executor | timeout/retry/concurrent tick/stale lease/restart behavior through existing owners |
| Provider/budget/audit drills | `engine/tests/test_pe6_fault_drills.rs`, provider adapters, `FakeProvider`, pricing/cost gate, redacted audit | fake/stub only; bounded kill/timeout/cost/audit evidence; no live provider or credentials |
| Release/rollback drills | `tools/test_pe6_release_drill.py`, accepted PE-5 verifier/installer/upgrader | invalid evidence before activation and previous-install preservation in temporary roots; no public release or host installation damage |
| Drill registry/evidence | `tools/run_fault_drills.py`, `tools/test_fault_drill_registry.py`, `tools/test_pe6_evidence.py`, `docs/RUNBOOK.md` | allowlisted suites/IDs, bounded deterministic JSON/human reports, explicit unsupported state, existing CI discovery; no new runtime state model or mutation API |
| PE-6 closeout | all drill owners and evidence | independent audit of recovery, cleanup, isolation, compatibility, report binding, and residual risk; sealed under the grouped closeout boundary |

## Open PR Coordination

PR #207 owns a disabled-by-default repository-maintenance orchestrator under:

- `scripts/agent-control/`;
- `.github/workflows/agent-*.yml`;
- orchestrator tests and CI wiring;
- related additions to architecture/status/module/runbook documents.

It does not own PE-5 release-provenance semantics or PE-6 recovery semantics. Because it touches shared CI and active documents, it must refresh from current `main` before merge and preserve this ownership map. PE-5/PE-6 work must not overwrite its orchestrator paths; shared `.github/workflows/tests.yml` changes require explicit reconciliation.

## Active Routing

1. PE-1 through PE-4 remain acceptance-sealed; PE-4 is sealed under PR #206 and `PE4-POST-CLOSE-REPAIR-1`.
2. PE-5 is acceptance-sealed under grouped `PE5-IMPLEMENT-1` and `PE5-CLOSE-1` (PRs #210 and #211).
3. The PE-6 internal milestones are complete in order within grouped `PE6-IMPLEMENT-1`; `PE6-CLOSE-1` independently sealed the separate audit/repair boundary.
4. No later packet is activated by this objective. Extend existing owners; do not create another runtime, scheduler, storage layer, release pipeline, signing authority, recovery authority, artifact truth source, or Dashboard data model without an explicit replacement decision, migration, compatibility evidence, and rollback.

## Active Documents

- `AGENTS.md`
- `CLAUDE.md`
- `docs/ARCHITECTURE_BOOK.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
- `docs/RUNBOOK.md`

Prefer editing, shortening, and reconciling these surfaces over adding another policy, roadmap, status, packet, or closeout document.
