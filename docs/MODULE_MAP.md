# Module Map

Last updated: 2026-07-08.

This file maps current ownership. It is not a phase history.

Full Agent Autonomy Mode remains active for repo-scoped, testable, observable, CI-gated, rollbackable work.

## Core Ownership

| Module | Stage | Purpose | Verification |
|---|---|---|---|
| `engine/src/main.rs`, `engine/src/http_server/` | active | Engine and API | HTTP and engine tests |
| `engine/src/trusted_local.rs` | active | Local readiness policy | trusted-local tests |
| `dispatch_engine.rs`, `task_analyzer/`, `model_selector.rs`, `budget_manager.rs` | active | Dispatch | dispatch tests |
| `engine/src/provider/`, `engine/src/provider/fake.rs` | active | Model adapters + FakeProvider for testing | focused adapter tests plus full stack verification |
| `engine/src/local_runner_provider.rs`, `engine/src/bin/local_runner_exec.rs` | active | Provider-backed stateful-vs-stateless runner | local_runner_provider unit tests, local-runner-exec binary |
| `engine/src/cli/` | active | CLI adapters | engine tests |
| `workflow/`, `scheduler.rs`, `node_executor.rs`, `executor_pool.rs` | active | Workflow | workflow and scheduler tests |
| `storage/local_product_store/` | active | Storage | local store tests |
| `target_repo_output.rs`, `target_repo_output/authority.rs` | active | Target output | target-output tests |
| `dashboard/` | active | Local UI | dashboard typecheck and build |
| `sdk/typescript/`, `sdk/python/` | active | SDKs | SDK tests |
| `wire_contract/v1/`, `codegen/` | active | Wire contracts | `scripts/check_wire_codegen_drift.sh` |
| `scripts/`, `tools/` | active | Scripts and pilots | script-specific tests |

## Token-Efficiency Ownership

| Capability | Owning paths |
|---|---|
| Scorecard validation | `scripts/token_efficiency_scorecard.py` |
| Scorecard comparison | `scripts/scorecard_comparison.py` |
| Native scorecard export | `scripts/native_scorecard_export.py` |
| Native deterministic stateful pilot | `scripts/native_stateful_experiment_pilot.py` |
| Provider-gated real runner | `scripts/provider_gated_real_runner.py`, `tools/test_provider_gated_real_runner.py` |
| Local runner validation | `scripts/validate_local_runner.py`, `tools/test_validate_local_runner.py` |
| LangGraph bounded import | `scripts/langgraph_trace_import.py` |
| Native artifact persistence | `engine/src/storage/local_product_store/native_scorecard_artifacts.rs` |
| Local scorecard artifact import | `engine/src/local_scorecard_import.rs`, `engine/src/bin/import_native_scorecard_artifacts.rs` |
| Local runner validation executor | `engine/src/node_executor.rs` (`LocalRunnerValidationExecutor`), `engine/src/executor_pool.rs` |
| Scorecard API | `engine/src/http_server/handlers/scorecards.rs` |
| Operator/dashboard evidence | `operator_evidence.rs`, `dashboard/` scorecard surfaces |

## Real Runner Routing

Real local stateful-vs-stateless runner work is the next token-efficiency direction. Route it through existing modules:

1. `workflow/` for bounded run lifecycle.
2. `node_executor.rs` for step integration.
3. `provider/` for model adapter calls behind existing readiness gates.
4. `budget_manager.rs` for cost and token ceilings.
5. `storage/local_product_store/` for bounded state and artifacts.
6. scorecard scripts for validation and comparison.
7. dashboard/operator surfaces only after backend evidence exists.

Do not create another runtime, scheduler, graph kernel, mailbox, storage layer, or dashboard data model.

## Active Docs

Keep current direction and routing inside active docs only:

- `docs/ARCHITECTURE_BOOK.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
- `docs/RUNBOOK.md`

Do not add new policy/status/roadmap docs by default.
