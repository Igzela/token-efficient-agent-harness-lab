# Module Map

Last updated: 2026-07-14.

This file maps current ownership and known connection points. It is not a phase history.

Full Agent Autonomy Mode is active for repository-scoped work that remains testable, observable, reviewable, CI-gated, compatible, and rollbackable. Execution-ready packets in `docs/NEXT_DECISION.md` are the default work units. Merge, release, deployment, and other external-critical actions still require their own explicit authority.

## Core Ownership

| Module | State | Purpose | Verification |
|---|---|---|---|
| `AGENTS.md`, `CLAUDE.md`, `docs/NEXT_DECISION.md`, `docs/REAL_WORLD_TESTING_PLAYBOOK.md`, `scripts/check_agent_handoff.py` | active | autonomous authority, packet routing, evidence discipline, and governance-drift prevention | handoff guard, focused documentation checks, CI where required |
| `engine/src/main.rs`, `engine/src/http_server/` | active | sole Rust runtime and API | HTTP and engine tests |
| `engine/src/trusted_local.rs` | active | trusted-local readiness and execution gates | trusted-local tests |
| `dispatch_engine.rs`, `task_analyzer/`, `model_selector.rs`, `budget_manager.rs` | active | dispatch, routing, and bounded budget authority | dispatch and budget tests |
| `engine/src/provider/`, `engine/src/provider/fake.rs` | active | provider adapters, bounded audit/redaction, cost evidence, strict typed Agent Runtime decisions, fixed-contract OpenRouter embeddings, and fake-provider testing | adapter/audit/embedding tests, dependency audit, full stack |
| `engine/src/local_runner_provider.rs`, `engine/src/bin/local_runner_exec.rs` | active/manual-live | bounded Stub/Fake/Live local runner; live remains explicit local operator/CLI only | local runner/provider tests and binaries |
| `engine/src/agent_memory.rs` | active | bounded AgentState memory-policy helpers | memory, context, workflow, and operator-evidence tests |
| `workflow/`, `scheduler.rs`, `node_executor.rs`, `executor_pool.rs`, `tool_policy_executor.rs` | active | workflow, sole scheduler, executor routing/pool accounting, bounded Agent Runtime, and real Command/CLI tool-policy enforcement | workflow/scheduler/executor/tool-policy tests |
| `engine/src/storage/local_product_store/` | active | sole application-owned SQLite/PostgreSQL-compatible storage | local store and PG integration tests |
| `target_repo_output.rs`, `target_repo_output/authority.rs` | active | target-output authority | target-output tests |
| `dashboard/` | active/read-mostly | local operator UI; mutation remains in explicit backend owners | tests, typecheck, lint, build |
| `sdk/typescript/`, `sdk/python/` | active | SDKs | SDK tests |
| `wire_contract/v1/`, `codegen/` | active | wire contracts and generated types | `scripts/check_wire_codegen_drift.sh` |
| `scripts/`, `tools/`, `.github/workflows/` | active | scripts, pilots, CI, release packaging, action/dependency gates, backup/restore, install/upgrade, and drills | script-specific tests, workflow checks, security baseline, CI |
| `scripts/agent-control/`, `.github/workflows/agent-*.yml`, `tests/test_agent_control_*.py`, `tests/test_agent_orchestrator_*.py` | active | disabled-by-default GitHub Issue-controlled maintenance orchestration, isolated Vader Codex workers, artifact finalizers, exact-head CI repair, independent review, and merge gating | explicit orchestrator suite in canonical `python-tests`, YAML/action-pin/security/handoff checks |

## Product-Evolution Ownership and Connection State

| Capability | Primary owners | Connection state and boundary |
|---|---|---|
| Agent Runtime execution | typed plan/run HTTP handlers; `AgentStepExecutor`; scheduler/executor pool; `agent_action_receipts`; provider `agent_action.v1` decision source | connected end to end; one leased node produces at most one typed action; default-off provider and runtime gates; atomic restart/concurrency replay |
| Command/CLI tool policy | tool capability/allowlist/hook stores; `ToolPolicyNodeExecutor`; workflow approvals/operator actions; scheduler, pool, tick, supervised-patch callers | connected on production Command/CLI paths; configured allowlists authoritative; exact-action approval consumed once; hooks bounded and audited |
| PE-1 regression lab | scorecard import/export/comparison scripts; native/generic scorecard stores; regression report/batch/trend owners; HTTP/SDK/Dashboard reads | connected end to end; report-only and non-mutating |
| Durable memory and retrieval | `durable_memory.rs`; `provider/embedding.rs`; `provider_audit.rs`; scheduler context injection; memory HTTP handlers; SDKs; v23/v25 SQLite/PostgreSQL schema; backup/integrity | connected end to end with exact scope/version/source/provider-model-pricing hashes, hash-bound restart receipts and catalog evidence, atomic failure/audit, typed bounded reconciliation, immutable re-embedding, default-off guarded OpenRouter embeddings, bounded vector Top-K, explicit lexical degradation, restart/concurrency safety, and metadata-only retrieval evidence |
| PE-2 forecast/anomaly derivation | `budget_intelligence.rs`, `budget_forecast.rs`, `budget_anomaly.rs`, `budget_manager.rs`; terminal workflow producer; authenticated recompute API | connected to normalized deduplicated native-scorecard, provider-audit, and workflow evidence through fenced restart-safe jobs |
| PE-2 evidence persistence/read | `budget_evidence_artifacts.rs`, normalized usage/v23 job stores, scorecard HTTP handlers, SDK/Dashboard reads | connected for automatic and operator-recomputed artifacts with completeness and measurement provenance |
| PE-2 pause/recovery consumer | `budget_pause_decisions.rs`, operator decision handler, workflow pause/audit/resume owners | connected; must continue to consume only supported, fresh, high-confidence, policy-eligible evidence |
| PE-3 operator decision center | `engine/src/operator_decision/mod.rs`, queue/store adapter, typed HTTP handlers, SDKs, Dashboard | connected to approval/reject, retry, workflow resume, budget pause/recovery, read-only inspect, non-approving acknowledge, and exact-snapshot rollback owners; no generic executor |
| PE-4 trace ownership and replay contracts | dispatch-history provenance, `policy_replay_producer.rs`, replay eligibility/recorder/offline evaluation, calibration, coverage, OOD owners | connected to normal dispatch persistence and authenticated deterministic generation; never calls providers |
| PE-4 replay persistence/read | `offline_replay_artifacts.rs`, `replay_producer_bindings`, scorecard HTTP handlers, SDK/Dashboard reads | automatic eligible artifacts and exact immutable producer bindings are connected |
| PE-4 shadow/canary/promotion validation | `shadow_router.rs`, adaptive experiment/canary owners, `adaptive_auto_promotion.rs`, `adaptive_policy.rs` | validators and atomic policy/snapshot/rollback owner exist |
| PE-4 safe promotion entry | replay generation API/profile, `promote_adaptive_fusion_policy_with_evidence_chain`, typed operator rollback | connected through exact evidence/current-state binding, explicit permission and confirmation; observation-summary mutation path removed |
| Native/local/LangGraph evidence | `engine/src/external_runtime.rs`, `engine/src/storage/local_product_store/external_runtime.rs`, `adapters/langgraph/`, native scorecard owners, provider-gated runner, capture/import compatibility paths | managed `langgraph_external` nodes are leased by Rust; scoped receipts/checkpoints and scorecards are app-owned; fixture is network-free and live is explicitly gated |
| Tool registry | existing tool capability/descriptor registry, configured-profile allowlist owner, hook owner, and execution authorization owner | active production policy owner; no deterministic Top-K discovery benchmark yet |
| Efficiency and tool discovery benchmark | `scripts/efficiency_live_benchmark.py`, `engine/src/efficiency_benchmark_runtime.rs`, `efficiency_{native,langgraph}_runtime`, scorecard matrix API/Dashboard | four exact memory strategies and static-all/Top-K evidence; fixture and guarded live operator paths; no dynamic production-tool authority |

## Integration Repair Ownership

### `AR-RUNTIME-INTEGRATION-1`

Primary owners:

- typed `agent_step` plan and run creation in the existing HTTP/store owners;
- `AgentStepExecutor` for one-step observe → decide → act → persist;
- the existing scheduler and executor pool for admission, lease, retry, cooldown, pause/resume, restart, and global/per-run concurrency;
- `provider/agent_decision.rs` for a single gated provider decision returning `agent_action.v1`;
- `agent_action_receipts` for atomic exactly-once action application;
- `ToolPolicyNodeExecutor`, configured allowlists, hooks, workflow approvals, and operator decisions for Command/CLI policy.

The engine is the sole runtime and state authority. The GitHub/Vader repository-maintenance orchestrator remains a separate disabled external control plane and receives no Agent Runtime scheduling or provider authority from this slice.

### `PR207-REPAIR-1`

Owned by existing PR #207:

- `scripts/agent-control/`;
- `.github/workflows/agent-*.yml`;
- orchestrator regression tests and shared CI wiring;
- orchestrator-specific additions to architecture/status/module/runbook documents.

The orchestrator must remain disabled and emergency-stopped. GitHub-hosted finalizers own mutations; Vader remains artifact-only. No replacement orchestrator, GitHub App, OpenAI API key, or Actions Variable control plane is authorized.

`control_state.py` is the authoritative setup and transition owner: `setup_labels.py` is a compatibility delegate, stop/resume never implicitly reauthorize, and every label mutation is followed by a live-state verification. `ci_verifier.py`, `ci_handler.py`, and the Issue-backed acquisition state own deterministic exact-head ranking, bounded natural-run observation/fallback, and supersession persistence. `validate_review.py`, `state_manager.py`, and `agent-review.yml` own bounded review validation, exact-head verdict persistence, non-authorizing review outcomes, and separate malformed-result evidence; `state_manager.py` also owns current-effective GitHub review evaluation and complete bounded review-thread pagination for merge gating. `codex_wrapper.sh` owns the shared allowlisted environment for implementation, repair, and review workers. Stage B activation remains out of scope and disabled.

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

## Program Coordination

PR #207 and its compatibility repair are merged history. Issue #217 demonstrated a separate live-smoke failure, so the repository orchestrator remains emergency-stopped while local control-plane integration proceeds. `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1` owns the diagnosis, repair, and replacement bounded smoke; no earlier slice may enable or dispatch it.

Every implementation PR refreshes actual `main`, open PRs, CI, and overlapping paths before push. A PR may merge only when the current user authority covers it and the exact final head satisfies the full playbook gates.

## Active Routing

1. `PR1-AR-RUNTIME-INTEGRATION-1` is the active packet.
2. `PR2-MEMORY-BUDGET-POLICY-LOOP-1` follows only after PR 1 is merged and refreshed.
3. `PR3-EXTERNAL-RUNTIME-LIVE-SEAL-1` follows only after PR 2 is merged and refreshed.
4. The original durable-memory, PE-2, PE-4, LangGraph, efficiency-benchmark, tool-discovery, and live-seal component requirements remain normative inside those three vertical slices.
5. Extend existing owners; do not create another runtime, scheduler, storage layer, release pipeline, signing authority, recovery authority, artifact truth source, tool registry, or Dashboard mutation model without an explicit replacement decision, compatibility evidence, and rollback.

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
