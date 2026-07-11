# Module Map

Last updated: 2026-07-11.

This file maps current ownership. It is not a phase history.

Full Agent Autonomy Mode remains active inside approved Terra-ready task packets for repo-scoped, testable, observable, CI-gated, rollbackable work.

## Core Ownership

| Module | Stage | Purpose | Verification |
|---|---|---|---|
| `.codex/config.toml`, `AGENTS.md`, `docs/NEXT_DECISION.md`, `scripts/check_agent_handoff.py` | active | Terra Medium executor default, planner/executor boundary, packet authority, and profile drift prevention | agent handoff check and CI |
| `engine/src/main.rs`, `engine/src/http_server/` | active | Engine and API | HTTP and engine tests |
| `engine/src/trusted_local.rs` | active | Local readiness policy | trusted-local tests |
| `dispatch_engine.rs`, `task_analyzer/`, `model_selector.rs`, `budget_manager.rs` | active | Dispatch, routing, and bounded budget authority | dispatch and budget tests |
| `engine/src/provider/`, `engine/src/provider/fake.rs` | active | Model adapters, persistent bounded audit/redaction, and FakeProvider for testing | focused adapter/audit tests, RustSec audit, full stack verification |
| `engine/src/local_runner_provider.rs`, `engine/src/bin/local_runner_exec.rs` | active | Provider-backed stateful-vs-stateless runner (Stub/Fake/Live); Live requires gates, persistent audit, positive pricing, pre-call cost reservation, bounded usage, timeout, and kill control | local_runner_provider tests, local-runner-exec binary, provider audit store tests |
| `engine/src/cli/` | active | CLI adapters | engine tests |
| `engine/src/agent_memory.rs` | active | Bounded AgentState memory policy helpers; no storage/runtime ownership | agent_memory and agent-step/context-injection/operator-evidence tests |
| `workflow/`, `scheduler.rs`, `node_executor.rs`, `executor_pool.rs` | active | Workflow, durable dynamic-controller limits, deterministic/pinned routing, and pool accounting | workflow and scheduler tests |
| `storage/local_product_store/` | active | Storage | local store tests |
| `target_repo_output.rs`, `target_repo_output/authority.rs` | active | Target output | target-output tests |
| `dashboard/` | active | Local UI | dashboard tests, typecheck, lint, and build |
| `sdk/typescript/`, `sdk/python/` | active | SDKs | SDK tests |
| `wire_contract/v1/`, `codegen/` | active | Wire contracts | `scripts/check_wire_codegen_drift.sh` |
| `scripts/`, `tools/`, `.github/workflows/` | active | Scripts, pilots, CI, release packaging, dependency/action pin gates, and atomic upgrade rollback | script-specific tests, release contract, action pin guard, CI |

## Token-Efficiency Ownership

| Capability | Owning paths |
|---|---|
| Scorecard validation | `scripts/token_efficiency_scorecard.py` |
| Scorecard comparison | `scripts/scorecard_comparison.py` |
| PE-1 scenario registry, fixed evidence, reports/batches, persistence, and bounded trends | `scripts/token_efficiency_regression.py`, `tools/test_token_efficiency_regression.py`, `tests/fixtures/token_efficiency_regression/registry.json`, `tests/fixtures/token_efficiency_regression/*/*.artifact.json`, `engine/src/storage/local_product_store/regression_report_artifacts.rs`, `engine/tests/test_regression_report_artifacts.rs` |
| Native scorecard export | `scripts/native_scorecard_export.py` |
| Native deterministic stateful pilot | `scripts/native_stateful_experiment_pilot.py` |
| Provider-gated real runner | `scripts/provider_gated_real_runner.py`, `tools/test_provider_gated_real_runner.py` |
| Local runner validation | `scripts/validate_local_runner.py`, `tools/test_validate_local_runner.py` |
| LangGraph offline capture, bounded import, and v2 artifact export | `tools/capture_langgraph_pilot.py`, `scripts/langgraph_trace_import.py`, `tests/fixtures/langgraph_pilot/` |
| Native v1 and generic v2 artifact persistence/comparison | `engine/src/storage/local_product_store/native_scorecard_artifacts.rs` |
| Local scorecard and PE-1 regression artifact import (legacy CLI name retained) | `engine/src/local_scorecard_import.rs`, `engine/src/bin/import_native_scorecard_artifacts.rs` |
| Local runner validation executor | `engine/src/node_executor.rs` (`LocalRunnerValidationExecutor`), `engine/src/executor_pool.rs`; automatic native scorecard artifact recording via `workflow_runs.rs` tick path |
| Local runner provider adapter | `engine/src/local_runner_provider.rs`, `engine/src/provider/fake.rs` |
| CLI stateful-vs-stateless experiment | `engine/src/bin/local_runner_exec.rs` |
| Scorecard comparison and PE-1 regression read-only API/SDK | `engine/src/http_server/handlers/scorecards.rs`, `sdk/python/src/agent_control_plane_sdk/client.py`, `sdk/typescript/src/index.ts`, `sdk/typescript/src/api-types.ts` |
| PE-1 Dashboard regression evidence and history | `dashboard/src/components/BenchmarkScorecards.tsx`, `dashboard/src/lib/regression-evidence.ts`, `dashboard/src/lib/regression-evidence.test.ts` |
| Operator scorecard evidence | `engine/src/http_server/handlers/operator_evidence.rs`, `dashboard/src/components/ScorecardEvidence.tsx` |

## Post-LGB Product Evolution Ownership

The detailed Terra-ready packet sequence is defined in `docs/NEXT_DECISION.md`. Packets extend existing owners rather than create parallel kernels or state sources.

| Stage | Primary owning paths | Boundary |
|---|---|---|
| PE-1 Token Efficiency Regression Lab | regression script/tests/fixtures; `native_scorecard_artifacts.rs`; `regression_report_artifacts.rs`; scorecard HTTP handlers; SDKs; benchmark Dashboard components | complete; reuse scorecard v1/v2 and existing LocalProductStore/API; report-only; no provider calls in CI |
| PE-2 Budget Intelligence and Anomaly Auto-Pause | `budget_manager.rs`; provider audit/cost evidence; scheduler/workflow pause controls; HTTP/operator evidence; SDKs; Dashboard | forecasts/anomalies are derived evidence; auto-pause only through existing policy/audit; no auto-kill |
| PE-3 Operator Decision Center | operator-evidence handlers; approvals; workflow/scheduler read models; Dashboard | derived action queue only; no second state machine, authority source, or hidden mutation path |
| PE-4 Trace-backed Policy Replay | `engine/src/feedback/run_trace_recorder.rs`; `engine/src/feedback/policy_simulator.rs`; adaptive experiment/canary modules; operator evidence | shadow-first, versioned evidence, coverage/OOD checks; reuse canary/promotion/rollback |
| PE-5 Release Provenance | `.github/workflows/release.yml`; release/install/upgrade scripts; container build paths | add SBOM, signatures, attestations, and verification without weakening audits or atomic rollback |
| PE-6 Fault Injection and Recovery Drills | focused engine integration tests; storage/provider/scheduler fault seams; backup/restore and upgrade rollback scripts; CI tooling | bounded deterministic drills; no destructive external testing; recovery invariants remain authoritative |

## Planned Evolution Routing

1. Execute only the earliest `READY_FOR_TERRA` packet whose prerequisites are complete.
2. PE-1 is acceptance-sealed; execute PE-2 contract, forecast, anomaly, read surfaces, policy-gated pause, and closeout in packet order.
3. Build PE-3 as a derived read model before connecting existing mutation endpoints.
4. Progress PE-4 from calibration to offline replay, shadow, and bounded canary.
5. PE-5 may run independently only after explicit lane activation.
6. Implement PE-6 only after each affected subsystem has explicit recovery invariants.

Do not create another runtime, scheduler, graph kernel, mailbox, storage layer, policy authority, artifact truth source, or Dashboard data model.

## Active Docs

Keep direction and routing inside active surfaces only:

- `AGENTS.md`
- `.codex/config.toml`
- `docs/ARCHITECTURE_BOOK.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
- `docs/RUNBOOK.md`

Do not add new policy/status/roadmap docs by default.
