# Module Map

Last updated: 2026-07-07.

This file maps current ownership. It is not a phase history.

## Core Ownership

| Area | Owning paths | Verification |
|---|---|---|
| Engine and API | `engine/src/main.rs`, `engine/src/http_server/` | HTTP and engine tests |
| Local readiness policy | `engine/src/trusted_local.rs` | trusted-local tests |
| Dispatch | `dispatch_engine.rs`, `task_analyzer/`, `model_selector.rs`, `budget_manager.rs` | dispatch tests |
| Model adapters | `engine/src/provider/` | focused adapter tests plus full stack verification |
| CLI adapters | `engine/src/cli/` | engine tests |
| Workflow | `workflow/`, `scheduler.rs`, `node_executor.rs`, `executor_pool.rs` | workflow and scheduler tests |
| Storage | `storage/local_product_store/` | local store tests |
| Target output | `target_repo_output.rs`, `target_repo_output/authority.rs` | target-output tests |
| Dashboard | `dashboard/` | dashboard typecheck and build |
| SDKs | `sdk/typescript/`, `sdk/python/` | SDK tests |
| Wire contracts | `wire_contract/v1/`, `codegen/` | wire drift guard |
| Scripts | `scripts/`, `tools/` | script-specific tests |

## Token-Efficiency Ownership

| Capability | Owning paths |
|---|---|
| Scorecard validation | `scripts/token_efficiency_scorecard.py` |
| Scorecard comparison | `scripts/scorecard_comparison.py` |
| Native scorecard export | `scripts/native_scorecard_export.py` |
| Native deterministic stateful pilot | `scripts/native_stateful_experiment_pilot.py` |
| LangGraph bounded import | `scripts/langgraph_trace_import.py` |
| Native artifact persistence | `engine/src/storage/local_product_store/native_scorecard_artifacts.rs` |
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
