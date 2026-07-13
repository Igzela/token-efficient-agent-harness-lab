# Module Map

Last updated: 2026-07-13.

This file maps current ownership and known connection points. It is not a phase history.

Full Agent Autonomy Mode is active for repository-scoped work that remains testable, observable, reviewable, CI-gated, compatible, and rollbackable. Execution-ready packets in `docs/NEXT_DECISION.md` are the default work units. Merge, release, deployment, and other external-critical actions still require their own explicit authority.

## Core Ownership

| Module | State | Purpose | Verification |
|---|---|---|---|
| `AGENTS.md`, `CLAUDE.md`, `docs/NEXT_DECISION.md`, `docs/REAL_WORLD_TESTING_PLAYBOOK.md`, `scripts/check_agent_handoff.py` | active | autonomous authority, packet routing, evidence discipline, and governance-drift prevention | handoff guard, focused documentation checks, CI where required |
| `engine/src/main.rs`, `engine/src/http_server/` | active | sole Rust runtime and API | HTTP and engine tests |
| `engine/src/trusted_local.rs` | active | trusted-local readiness and execution gates | trusted-local tests |
| `dispatch_engine.rs`, `task_analyzer/`, `model_selector.rs`, `budget_manager.rs` | active | dispatch, routing, and bounded budget authority | dispatch and budget tests |
| `engine/src/provider/`, `engine/src/provider/fake.rs` | active | provider adapters, bounded audit/redaction, cost evidence, and fake-provider testing | adapter/audit tests, dependency audit, full stack |
| `engine/src/local_runner_provider.rs`, `engine/src/bin/local_runner_exec.rs` | active/manual-live | bounded Stub/Fake/Live local runner; live remains explicit local operator/CLI only | local runner/provider tests and binaries |
| `engine/src/agent_memory.rs` | active | bounded AgentState memory-policy helpers | memory, context, workflow, and operator-evidence tests |
| `workflow/`, `scheduler.rs`, `node_executor.rs`, `executor_pool.rs` | active | workflow, scheduler, executor, routing, and pool accounting | workflow/scheduler/executor tests |
| `engine/src/storage/local_product_store/` | active | sole application-owned SQLite/PostgreSQL-compatible storage | local store and PG integration tests |
| `target_repo_output.rs`, `target_repo_output/authority.rs` | active | target-output authority | target-output tests |
| `dashboard/` | active/read-mostly | local operator UI; mutation remains in explicit backend owners | tests, typecheck, lint, build |
| `sdk/typescript/`, `sdk/python/` | active | SDKs | SDK tests |
| `wire_contract/v1/`, `codegen/` | active | wire contracts and generated types | `scripts/check_wire_codegen_drift.sh` |
| `scripts/`, `tools/`, `.github/workflows/` | active | scripts, pilots, CI, release packaging, action/dependency gates, backup/restore, install/upgrade, and drills | script-specific tests, workflow checks, security baseline, CI |

## Product-Evolution Ownership and Connection State

| Capability | Primary owners | Connection state and boundary |
|---|---|---|
| PE-1 regression lab | scorecard import/export/comparison scripts; native/generic scorecard stores; regression report/batch/trend owners; HTTP/SDK/Dashboard reads | connected end to end; report-only and non-mutating |
| PE-2 forecast/anomaly derivation | `engine/src/budget_forecast.rs`, `engine/src/budget_anomaly.rs`, `budget_manager.rs`, provider audit and workflow usage evidence | implemented but no production runtime/API/CLI/scheduler caller currently derives evidence |
| PE-2 evidence persistence/read | `engine/src/storage/local_product_store/budget_evidence_artifacts.rs`, scorecard HTTP handlers, SDK/Dashboard reads | connected for already-created artifacts |
| PE-2 pause/recovery consumer | `budget_pause_decisions.rs`, operator decision handler, workflow pause/audit/resume owners | connected; must continue to consume only supported, fresh, high-confidence, policy-eligible evidence |
| PE-3 operator decision center | `engine/src/operator_decision/mod.rs`, queue/store adapter, HTTP handlers, SDKs, read-only Dashboard, allowlisted action owners | connected to approval, retry, workflow resume, budget pause, and recovery owners; no generic executor |
| PE-4 trace ownership and replay contracts | dispatch-history provenance, replay eligibility/recorder/offline evaluation, calibration, coverage, OOD owners | implemented |
| PE-4 replay persistence/read | `offline_replay_artifacts.rs`, scorecard HTTP handlers, SDK/Dashboard reads | connected for explicitly supplied valid replay requests; no production producer caller |
| PE-4 shadow/canary/promotion validation | `shadow_router.rs`, adaptive experiment/canary owners, `adaptive_auto_promotion.rs`, `adaptive_policy.rs` | validators and atomic policy/snapshot/rollback owner exist |
| PE-4 safe promotion entry | `record_offline_replay`, `promote_adaptive_fusion_policy_with_evidence_chain` | disconnected from production HTTP/operator/CLI/runtime entry; legacy observation path remains intentionally blocked |
| Native/local/LangGraph evidence | native scorecard export, local runner, provider-gated runner, LangGraph capture/import, fixtures, existing scorecard store/API | connected for importer/manual/local paths; ordinary workflow local runner remains Stub/Fake-only |
| Tool registry | existing tool capability/descriptor registry and allowlist owners | active corpus owner; no deterministic Top-K discovery benchmark yet |
| Tool discovery benchmark | future `TOOL-DISCOVERY-BENCH-1` under existing benchmark and PE-1 owners | not connected; benchmark only, no production dynamic-tool authority |

## Integration Repair Ownership

### `PR207-REPAIR-1`

Owned by existing PR #207:

- `scripts/agent-control/`;
- `.github/workflows/agent-*.yml`;
- orchestrator regression tests and shared CI wiring;
- orchestrator-specific additions to architecture/status/module/runbook documents.

The orchestrator must remain disabled and emergency-stopped. GitHub-hosted finalizers own mutations; Vader remains artifact-only. No replacement orchestrator, GitHub App, OpenAI API key, or Actions Variable control plane is authorized.

### `PE2-RUNTIME-PRODUCER-1`

Primary owners to reuse:

- provider audit/cost evidence and workflow run/node evidence;
- `budget_forecast.rs` and `budget_anomaly.rs`;
- `budget_evidence_artifacts.rs`;
- existing scorecard/budget read handlers and SDK/Dashboard surfaces;
- existing budget pause, workflow pause, audit, and recovery owners.

Do not add a second scheduler, second pause owner, or caller-supplied Supported evidence.

### `PE4-EVIDENCE-ENTRY-1`

Primary owners to reuse:

- dispatch-history provenance and replay eligibility;
- `offline_evaluation.rs` and `offline_replay_artifacts.rs`;
- `shadow_router.rs` and existing canary/experiment contracts;
- `adaptive_auto_promotion.rs` and `adaptive_policy.rs`;
- existing policy snapshot, apply, compensation, and rollback state.

Do not restore observation-summary-only auto-promotion. Promotion must pass the complete evidence-chain owner with explicit current-state binding, confirmation, and permission.

### `TOOL-DISCOVERY-BENCH-1`

Primary owners to reuse:

- existing tool descriptors/capabilities as the canonical corpus;
- existing benchmark/scenario/scorecard comparison utilities;
- PE-1 report, batch, trend, store, API, SDK, and Dashboard owners.

The packet may add bounded retrieval/scenario evidence and compatible metrics. It may not add dynamic production tool execution or a second tool registry.

## PE-5 Release Provenance Ownership

PR #214 merged the active post-seal repair. Historical PRs #210-#211 remain useful evidence but do not supersede the repaired v2 authority.

| Capability | Primary owners | Boundary |
|---|---|---|
| Release provenance contract | `scripts/release_provenance.py`, v2 and installation tests, existing build/install/upgrade scripts and workflow | `release_provenance.v2`; exact distinct SLSA, SPDX, and custom-manifest bundles; v1 fixture-only and production-non-authorizing |
| Deterministic SBOM | release provenance helper, Cargo/Bun lockfiles, package/container builders | canonical SPDX 2.3 and deterministic dependency inventory; no second packaging pipeline |
| Signed attestation | `.github/workflows/release.yml`, pinned `actions/attest`, v2 verifier | external ephemeral production identity; no persistent private key or signing on Vader |
| Installer/upgrader verification | `install-from-release.sh`, install/upgrade scripts, installation/provenance tests | immutable verified bootstrap, bounded extraction, exact bundle/predicate verification, transactional activation and rollback |
| Publish gate | release workflow, release contract checker, semantic workflow tests, action-pin/security checks | verification precedes publication; no unauthorized tag, public release, or deployment |

## PE-6 Fault Injection and Recovery Ownership

PR #214 merged the owner-evidence and claim-alignment repair. Historical PRs #212-#213 remain evidence only where compatible with v2 semantics.

| Capability | Primary owners | Boundary |
|---|---|---|
| Recovery/fault contracts | `fault_drill_contract.py`, focused contract tests | active v2 scenario, owner evidence, result, and report contracts; v1 is not reinterpreted |
| Fault harness | `fault_drill_harness.py`, registry, owner adapters, focused tests | fixed allowlisted child commands, disposable resources, exact owner bytes, monotonic timing, bounded cleanup; no arbitrary command/runtime service |
| Storage drills | Rust fault-drill tests, LocalProductStore, migrations, integrity/audit tables, BackupManager | SQLite and real disposable PG interruption/retry/cleanup; no production database corruption |
| Workflow/executor drills | workflow runs, scheduler, node executor, Rust fault tests | timeout/retry/concurrent tick/stale lease/restart through existing owners |
| Provider/budget/audit drills | provider adapters, FakeProvider, pricing/cost gate, redacted audit | fake/stub only; unsupported states remain explicit |
| Release/rollback drills | release drill tests and accepted PE-5 owners | temporary-root invalid-evidence and previous-install preservation; no public release or host damage |
| Registry/evidence/CI | `run_fault_drills.py`, registry/evidence tests, runbook, canonical CI | bounded deterministic reports and explicit unsupported state |

## Open PR Coordination

PR #207 touches shared CI and active documents. Before any implementation packet starts:

1. inspect its actual head, merge state, review state, and exact-head CI;
2. repair it only on its existing branch;
3. refresh it from current `main` and preserve the integration-gap and PE-5/PE-6 ownership recorded here;
4. reconcile shared `.github/workflows/tests.yml` changes explicitly;
5. do not merge without separate user authorization.

## Active Routing

1. `PR207-REPAIR-1` is the active packet.
2. `PE2-RUNTIME-PRODUCER-1` follows after PR #207 is resolved or its shared conflicts are explicitly separated.
3. `PE4-EVIDENCE-ENTRY-1` follows as a separate PR.
4. `TOOL-DISCOVERY-BENCH-1` follows as a separate benchmark/evidence PR.
5. No new product phase is active.
6. Extend existing owners; do not create another runtime, scheduler, storage layer, release pipeline, signing authority, recovery authority, artifact truth source, tool registry, or Dashboard mutation model without an explicit replacement decision, compatibility evidence, and rollback.

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