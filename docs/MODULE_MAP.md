# Module Map

Last updated: 2026-07-22.

This map records current code ownership and actual connection state. It is not a phase history. Forward routing is governed by `docs/NEXT_DECISION.md`; current facts are in `docs/CURRENT_STATUS.md`.

Full Agent Autonomy Mode permits repository-scoped work that is testable, observable, reviewable, verification-gated, compatible, and rollbackable. Provider calls, external mutation, target output, release, deployment, and authority-critical actions retain their separate gates.

## Core Ownership

| Area | Canonical owner | Connection truth |
|---|---|---|
| API/startup | `engine/src/main.rs`, `engine/src/http_server/` | sole process/API composition root; execution modes and scheduler are default-off unless explicit gates pass |
| Dispatch | `engine/src/dispatch_engine.rs`, `task_analyzer/`, `model_selector.rs`, `budget_manager.rs` | connected for `/dispatch`; default executor is `noop`; not the workflow/worktree orchestrator |
| Planning | `engine/src/read_only_planner.rs`, `orchestration/task_decomposer.rs`, DAG/dependency/context-budget owners | read-only advisory graph by default; executable Agent/adaptive graphs require caller-supplied explicit contracts |
| Workflow runtime | `engine/src/workflow/`, `engine/src/scheduler.rs`, `engine/src/scheduler/runtime.rs`, `executor_pool.rs`, `node_executor.rs` | sole persisted run/lease/retry/concurrency path; supervised workers and scheduler are default-off |
| Agent Runtime | `AgentStepExecutor`, provider typed-action contracts, agent state/proposals/receipts | explicit `agent_steps`; one leased node/one typed action; default-off |
| Recursive execution | `recursive_execution`, workflow/scheduler/Agent Runtime/store owners | bounded child-task tree via PR #239; no autonomous root goal or self-improvement |
| Durable memory | `agent_memory.rs`, `durable_memory.rs`, store/retrieval/context injection | scoped and connected for compatible Agent Runtime nodes; not automatic for ordinary plans |
| Adaptive efficiency | adaptive completion/executor, contextual policy, experiment/promotion, replay, scorecard, budget producers | connected behind gates; ordinary plans do not automatically select or compile it |
| Dynamic workflow | `workflow/dynamic_controller.rs`, scheduler runtime | explicit/scheduler mode; not automatic product composition |
| Supervised workspace | supervised-patch handlers/store and `target_repo_output` worktree owner | real copy/git-worktree lifecycle; created separately from ordinary plan/run |
| Verification/repair | supervised-patch verification, canonical operation/attempt receipts, CLI repair executor | real and bounded; API-owned run path remains separate from ordinary workflow transaction |
| Artifact/approval | supervised-patch capture/store, redaction/secret scan/integrity, workflow approval owner | product approval is a separate `team:admin` step bound to exact task/run/node/workspace/source/artifact/verification/output intent and current version |
| Target output | `target_repo_output.rs`, supervised artifact receipt/operation owner, GitHub Draft PR adapter | `dispatch:execute` plus confirmation; artifact/export receipt or phased `acp/*` push then Draft PR; default-off; no target `main` or merge authority |
| Persistence | `engine/src/storage/local_product_store/` | sole application-owned SQLite/PostgreSQL store, migrations, audit, evidence, backup/integrity |
| Harness Evolution Level-1 | `harness_evolution.rs`, `harness_evolution_eval.rs`, `harness_evolution_pr_ready.rs`, v27-v29 store owners | accepted fixture laboratory through PR #265; default-off; active Harness immutable |
| Dashboard | `dashboard/` | Mission Control exposes separate finalize, approve, and confirmed output controls; PR #225 theme files remain independent |
| Product Golden Path | `product_golden_path.rs`, `storage/local_product_store/product_tasks.rs`, workflow/supervised-patch owners, product handlers, schema v30 `product_tasks`, schema v31 `product_task_terminal_evidence` | default-off root task through scheduler/verification/artifact/output and canonical terminal evidence; Residual Seal 2 still owns verification-time authority and final acceptance |
| SDKs | `sdk/typescript/`, `sdk/python/` | typed intake/compile/finalize plus separate approve/output clients; legacy combined method remains compatibility-only |
| Contracts | `wire_contract/v1/`, `codegen/` | cross-language schemas; checked by `scripts/check_wire_codegen_drift.sh` |
| Repository agent | `scripts/agent-control/`, `.github/workflows/agent-*.yml` | implemented, production-disabled, parked on Issue #254 |
| CI/release/recovery | `.github/`, `scripts/`, `tools/`, release/fault assets | verification and bounded operator support; no implicit release/deploy authority |

## Actual Product Data Flow

```text
Product Golden Path (default-off)
  canonical task + target repo + source revision
    -> create supervised git worktree and executable graph
    -> existing scheduler leases and advances the run
    -> verify
    -> capture artifact
    -> awaiting_approval
    -> team:admin exact approval
    -> dispatch:execute + explicit output confirmation
    -> durable artifact/export receipt
       OR phased acp/* push -> open Draft PR receipt

  -> atomic canonical terminal evidence + pure reads

Remaining seal
  verification-time pause/kill/lease/version/late-write authority + recovery/E2E
```

## Canonical Identity Map

| Record | Existing identity | Current parent binding |
|---|---|---|
| dispatch | `dispatch_id` | request/tenant; independent direct-dispatch lifecycle |
| plan | `plan_id`, `workflow_id`, plan `dispatch_id` | raw request and advisory graph |
| run | `run_id`, `plan_id`, workflow scope | derived from plan; optional `workspace_id` scope string |
| node | `node_id`, run/workflow | task type/dependencies; generic plan lacks repository task contract |
| supervised workspace | `workspace_id`, `run_id`, optional `plan_id`, target/source | separate post-run creation |
| verification | operation/attempt/run identities | supervised workspace owner |
| artifact | `artifact_id`, workspace/patch/source | captured from supervised workspace |
| approval | `approval_id`, task/run/node/workspace/source/artifact/verification/output bindings | separate workflow approval owner; no execution authority |
| target output | receipt or progressive `operation_id` | exact task/artifact/approval/request; branch and Draft PR phases distinct |
| replay/scorecard | owner-specific artifact/run/dispatch IDs | terminal evidence links only exact owner-backed records; absence is explicit unavailable |
| Harness candidate | proposal/candidate/lineage/evaluation/PR_READY IDs | Level-1 laboratory identity, not user task identity |

`PE7-PRODUCT-GOLDEN-PATH-RESIDUAL-SEAL-2` must finish verification-time authority and final acceptance without replacing these owners.

## Top-Level Directory Classification

| Path | Role | State | Ordinary runtime participation |
|---|---|---|---|
| `engine/` | runtime, API, policy, scheduler, store, evidence, output | active | partial and fragmented |
| `dashboard/` | operator UI and guarded controls | active | manual composition |
| `sdk/` | typed clients | active | mirrors APIs |
| `scripts/` | demos, validation, evidence import, ops, release, repository agent | active | mostly outside runtime |
| `tools/` | security, provenance, fault and maintenance tooling | active | support/verification |
| `adapters/` | external runtime adapters | guarded | explicit fixture/external nodes |
| `wire_contract/`, `codegen/` | schema/type governance | active | contract support |
| `.github/` | CI, exact-head, release and parked repository-agent workflows | active/default-off by lane | delivery/verification |
| `docs/` | authority, state, architecture, runbook | active | governance only |
| `tests/`, fixtures, benchmarks | deterministic evidence | active | validation only |
| `deploy/` | local packaging/deployment support | bounded | not production authority |
| `site/` | public presentation | bounded | none |

## Capability Boundaries

- Bounded recursive task execution: persisted child-task admission inside existing workflow/scheduler owners.
- Adaptive/dynamic efficiency: routing, fusion, observations, experiments, promotion/rollback, scheduler feedback, replay, and scorecards.
- Harness Evolution Level-1: one default-off fixture candidate/evaluation/archive/PR_READY owner path with immutable active Harness.
- Harness Evolution Level-2: proposed multi-generation controller only; Issue #266 has no implementation.
- Meta Improver: separate blocked experiment; not implied by Level-1/Level-2.
- Recursive self-improvement: not implemented or claimed. It would require repeated accepted improvement of the active system under separate authority and real evidence.

## PE-5 Release Provenance Ownership

Existing release workflow, package/container builders, lockfiles, signed provenance/SBOM/custom-manifest verification, installer/upgrader, and rollback owners remain authoritative. No Golden Path or evolution packet gains tag, publication, release, deployment, installation, or signing authority.

## PE-6 Fault Injection and Recovery Ownership

Existing fixed scenario registry, disposable fault harness, SQLite/PostgreSQL recovery tests, fake/stub provider owners, release rollback drills, and cleanup evidence remain authoritative. Product/evolution work may reuse their failure models but may not target production resources or create a second recovery authority.

## Active Connection Decision

1. Connect existing owners through `PE7-PRODUCT-GOLDEN-PATH-1`.
2. Collect trusted product evidence through `PE7-REAL-WORKLOAD-EVIDENCE-1`.
3. Reassess and, only if justified, activate `PE7-HARNESS-EVOLUTION-LEVEL2-GENERATIONAL-CONTROLLER-1`.
4. Keep `PE7-META-IMPROVER-EXPERIMENT-1` blocked behind Level-2 and a separate authority review.
5. Keep OpenCode binary admission deferred, repository-agent smoke parked, provider/live gates unchanged, and PR #225 independent.

## Active Documents

- `README.md`
- `AGENTS.md`
- `CLAUDE.md`
- `docs/ARCHITECTURE_BOOK.md`
- `docs/CURRENT_STATUS.md`
- `docs/NEXT_DECISION.md`
- `docs/MODULE_MAP.md`
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md`
- `docs/RUNBOOK.md`

Prefer reconciling these files over adding another roadmap, status, architecture, policy, or closeout document.
