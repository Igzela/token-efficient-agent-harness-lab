# Module Map

Last updated: 2026-08-25.

This is the concise ownership map for accepted `main`. Accepted facts are in `docs/CURRENT_STATUS.md`; the current executable window is in `docs/NEXT_DECISION.md`; the routing-only horizon is in `docs/FUTURE_ROUTE.md`; architecture invariants are in `docs/ARCHITECTURE_BOOK.md`.

Open PR branches are not canonical owners until merged and are intentionally not listed here. Observe them through a fresh context capsule. Accepted owner identities come from remote `main`, not from a branch-local status claim.

Autonomy semantics and separate external-effect gates are owned by `AGENTS.md`; this map records only implementation owners.

## Core Ownership

| Area | Canonical owner | Boundary |
|---|---|---|
| Repository navigation, context capsule, and session recovery | `START_HERE.md`, `scripts/project_context.py`, `scripts/session_context.py`, `scripts/github_observer.py`, `tests/test_session_context.py`, `tools/test_project_context.py`, `tools/test_github_observer.py`, `scripts/check_agent_handoff.py`, `.github/workflows/tests.yml` (context-capsule publisher job), `scripts/agent-control/prompt_builder.py` | One accepted-main role router and one on-demand fail-closed transport view; the Git-private session checkpoint is a local digest-bound recovery projection whose verification evidence is bound to the accepted dispatch capsule's verification contract, never a queue, lease, status database, packet selector, review judge, or authority owner |
| Current executable packet routing | `docs/NEXT_DECISION.md` | Sole owner of active routing, common execution gates, one expanded current packet, its allowed delta, exit, stop, and consolidation eligibility |
| Long-horizon packet routing | `docs/FUTURE_ROUTE.md` | Sole routing-only index for blocked successors; cannot authorize implementation or effects, and an eligible packet must be removed and fully refreshed into `NEXT_DECISION.md` before execution |
| API and composition root | `engine/src/main.rs`, `engine/src/http_server/` | Sole startup/composition surface; configuration sources, precedence, validated schemas, acyclic dependency topology, and strict runtime mode gates are frozen under the AC5 composition root contract |
| Dispatch analysis | `engine/src/dispatch_engine.rs`, analyzers, selectors, planners, decomposers | Advisory/default-noop unless an accepted contract admits execution |
| Workflow runtime | `engine/src/workflow/`, `engine/src/scheduler.rs`, `engine/src/scheduler/`, `engine/src/executor_pool.rs`, `engine/src/node_executor.rs` | Sole persisted run, node, lease, retry, pause/kill, and concurrency path |
| Recursive/agent nodes | Existing AgentStep and recursive-execution modules plus scheduler/store | Typed bounded nodes; no autonomous root-goal generation, production self-update, or authority expansion |
| Managed CLI/process | `engine/src/cli/mod.rs`, `config.rs`, `cli_node_executor.rs`, `codex_managed_acceptance_preflight.rs`, process/probe owners | Bounded subprocess lifecycle, output limits, process-tree cleanup, exact executable identity, default-off admission, and lease-bound owner-derived pre-child preflight; managed-coding runtime profiles generalize beneath these same owners without a new process/scheduler/store owner. |
| Codex mediation and budget | `engine/src/cli/codex_budget_authority.rs`, `codex_mediation_admission.rs`, `codex_usage_journal.rs`, `codex_session_usage.rs` | Parent-held credential, loopback gateway, ProductTask budget enforcement, parent-owned fail-closed journal, session corroboration; class remains partial |
| Managed-acceptance authority | `engine/src/storage/local_product_store/managed_acceptance.rs` and existing store/migration owners through v36 | Store-owned immutable proposal/final manifest, authenticated delegation, separated manifest/spend and artifact/output receipts, one-use spend, attempt lease, provider-request journal, terminal cleanup, restart, replay, audit, and rollback authority; read-only current-lease validation supplies runtime preflight without creating another owner |
| Multi-executor usage evidence | `engine/src/execution_usage/` — accepted adapters and reconcile owners (PR #301) | `execution_usage_event.v1`; evidence only; never a second budget or spend authority |
| Managed provider-call protocol adapters | `engine/src/provider/managed_deepseek.rs`, `managed_deepseek_executor.rs`, existing protocol/usage adapters, and ProductTask managed-acceptance/store owners | Exact Pro-planner/Flash-implementer/Pro-reviewer routing beneath one ProductTask budget and durable pre-send request claim; parent-only credential resolution, exact usage reconciliation, and permanent no-retry after outcome unknown. Adapters never own budget, lease, approval, output, audit, or rollback state. |
| Workspace and patch | Existing supervised-patch/workspace owners, `engine/src/storage/local_product_store/product_tasks.rs`, and target-repository output owners | App-owned detached worktree, exact source binding, bounded patch and cleanup lifecycle including `local_folder` staging/manifest/preimage/rollback under these same owners; the v35 preparation receipt pins one local recovery path before mutation; local/try-only PostgreSQL guards never become a second workspace, lease, budget, or rollback owner. |
| Verification | `engine/src/product_golden_path.rs`, `engine/src/storage/local_product_store/product_tasks.rs`, `engine/src/storage/local_product_store/managed_acceptance.rs`, and existing process-outcome/tool-policy owners | Fixed admitted commands, exact workspace/source/patch bindings, pause/kill/late-write refusal; Product Golden Path orchestration remains distinct from the sole LocalProductStore mutation authority |
| Artifact | Existing supervised artifact capture, integrity, redaction, and store owners | Atomic content/hash-bound artifact; no approval or output authority |
| Approval | Existing workflow/product approval owner | Separate current-state human approval; no execution or output mutation authority |
| Output | Existing target-repository output owner | Separate confirmation, `acp/*` only, Draft PR or patch export; no merge/default-branch/release/deploy authority |
| Terminal evidence | `engine/src/storage/local_product_store/` product terminal-evidence owners | Exact persisted task/run/node/workspace/source/artifact/approval/output/audit binding |
| Persistence and audit | `engine/src/storage/local_product_store/` and PostgreSQL backend | Sole SQLite/PostgreSQL transaction, migration, audit, idempotency, evidence, and rollback owner |
| Scorecards/replay | Existing scorecard, trace, replay, and store owners | Derived comparison evidence; cannot mutate live routing or policy by itself |
| Real Workload Evidence freeze | `engine/src/rwe/operator_corpus.rs`, `engine/src/rwe/corpus.rs`, `engine/src/rwe/economic_protocol.rs`, `engine/src/rwe/execution_schedule.rs`, and versioned `engine/rwe/corpora/` artifacts | Sole provider-free operator corpus/protocol/schedule freeze and canonical-hash boundary. Versions coexist; a refreeze never overwrites prior artifacts or evidence. Freeze code grants no spend, live-run, evaluator, output, or adoption authority. |
| RWE run authorization and spend | `engine/src/storage/local_product_store/rwe_authority.rs` plus `engine/src/rwe/runner.rs` (PR #300 fixture authority; PR #361 v2 contract; Board B production issue/admit/spend) | Store-owned one-use RWE spend via `rwe_run_authorizations`; v1 fixture envelope and production `rwe_run_authorization.v2` issue/admit under this single owner; bindings derived from freeze owners and principal; durable B2 rule is caller-supplied finite `expires_at` (no invented freeze-duration TTL); no second spend/budget owner |
| RWE first live baseline composition | `engine/src/rwe/live_baseline_coordinator.rs`, thin CLI `rwe-live-baseline`; reuses Product Golden Path + `LocalProductStore` owners for exact frozen RWE bindings | merged PR #363 (`995e57e…`), accepted main capability; orchestrates frozen 4-cell schedule over Board B admit, store cell fence, ProductTask/managed executor/spend journal/verifier/artifact/Draft PR/terminal/cleanup; no second scheduler/store/budget/runtime |
| RWE economic protocol and VDE artifacts | `engine/src/rwe/economic_protocol.rs` (PR #319) | Immutable/hash-bound protocol and artifact validation only; no runtime, store, budget, reviewer, output, adoption, or release authority |
| Harness Evolution identity and candidate evidence | `engine/src/harness_evolution.rs` plus existing store owners | Sole active-Harness, candidate, lineage, mutable-surface, workspace, and content-binding owner; candidates cannot change evaluator, authority, budget, or target-output policy |
| Harness Evolution evaluator and holdout contract | `engine/src/harness_evolution_eval.rs` plus `engine/src/storage/local_product_store/harness_evolution.rs` | Sole contract boundary for evaluator-owned task-family, label/rubric, sealed-vault, hard-gate, metric, evaluation-bundle, prediction-outcome, and Pareto evidence; evaluator owns entrant-selection policy/request, while `LocalProductStore` owns one-use receipt validation/persistence/consumption. `PredictionOutcomeV1` implementation remains a gated successor, fixture helpers are not managed acceptance, and no second evaluator/store is allowed |
| Harness Evolution Level-1 | `engine/src/harness_evolution*.rs` plus existing store owners | Default-off one-generation laboratory; active Harness immutable, sealed labels/evaluator inputs remain outside candidate access, and prediction accuracy is non-authoritative calibration evidence |
| Product durable memory | `engine/src/storage/local_product_store/durable_memory.rs` plus existing store/migration/audit owners | Product-scoped persisted memory records under the sole store owner; not a Harness-Evolution projection, routing authority, evaluator, spend owner, or adoption source |
| SDK and Dashboard | `sdk/`, `dashboard/` | Typed interaction and read-only projection only; no backend, workflow, budget, or evaluator authority |
| Wire contracts and codegen | `wire_contract/`, `codegen/`, `engine/src/wire_types.rs` | Canonical JSON schema definitions and deterministic cross-language codegen; drift checked by `scripts/check_wire_codegen_drift.sh`; AC6 type governance contract and compatibility closeout |
| Event schema contract | `engine/src/event_schema.rs` | Canonical event schema validation, idempotency hashing, and JSONL evidence guard (`docs/stage0/events.jsonl`); production module with no reference-surface dependency |
| CI and repository automation | `.github/`, `scripts/`, `tools/`, `scripts/agent-control/`; outbound poll/run-once seam and plan parsing in `scripts/agent-control/local_loop.py`, `local_run_once.py`, `local_supervisor.py`, `local_verification.py`, `loopctl.py`, `plan_lane.py`; successor promotion, typed EFFECT/T3 pauses, and window compaction in `scripts/agent-control/route_driver.py` | Verification and repository-maintenance automation only; no implicit product/release authority. `route_driver.py` is the deep promotion boundary: its deterministic layer binds route identity/prerequisites/profile/manifest/predecessor receipt, its planner validates current-main MODULE_MAP/owner/caller/test evidence, and it cannot use FUTURE_ROUTE paths as authority, execute an EFFECT, mint T3 authority, or create any controller/ledger/store/workflow owner. The T3 decision and independent existing-owner outcome travel separate authenticated ledger transports owned by the controller workflow; the local loop re-proves both receipts before opening a provider-free closeout/promotion and remains the sole controller, never a daemon state store. Focused pre-artifact checks are allowlisted by `local_verification.py`. Durable design: Architecture Book repository-maintenance route transition; procedures: RUNBOOK. |
| Provider-free Steward executor | `scripts/agent-control/steward.py`, `steward_service.py`, `steward_journal.py`, `steward_workers.py`, `steward_github.py`, `steward.service` | Bounded provider-free WorkCard coordinator, heartbeat, restart/reconciliation projection, digest-bound worktree/path isolation, K=2 dispatch, retry/tier routing, independent-review binding, and exact Stage-PR observation. SQLite journal is rebuildable operator state only; no product/runtime/scheduler/store/queue/lease/budget/approval/output/audit/rollback/merge authority. Automatic merge and Provider access remain disabled. |
| Review Convergence Protocol | `scripts/agent-control/review_convergence.py` (canonical owner of `MAX_SUBSTANTIVE_REVIEW_ROUNDS`, `MAX_AUTONOMOUS_REPAIR_BATCHES`, finding/decision normalization, R1/repair/R2 transitions), `scripts/agent-control/state_manager.py` (durable ReviewState wire v3 persistence), `scripts/agent-control/validate_review.py` + `review_schema.json` (artifact validation), `scripts/agent-control/review_loop/` (transport), `scripts/agent-control/prompts/review.md` + `prompt_builder.py` (review prompt), `docs/REAL_WORLD_TESTING_PLAYBOOK.md` (protocol owner), `scripts/project_context.py` (bounded capsule projection) | Exact PASS is the only merge-authorizing review verdict and may carry deferred notes; R1/R2 budget with a single autonomous repair batch, no autonomous R3; trusted CI stays with the merge owner; capsule projects review state only and never decides severity, disposition, repair, Ready, or merge; no second review owner exists |
| Shared investigation escalation (`ask_sol`) | `scripts/ask_sol.py`, `scripts/ask_sol`, `scripts/agent-control/ask_sol_schema.json`, `scripts/agent-control/ask_sol_model_schema.json`, `tests/test_ask_sol.py` | Shared, harness-neutral read-only GPT-5.6 Sol investigation escalation tool; ordinary workers remain sole task owners and executors; pre/post worktree dirty-state non-mutation verification; per-state consultation budget and recursion rejection |

T3/Sol policy has one current-contract owner: [NEXT_DECISION.md](NEXT_DECISION.md)'s route-automation invariants. This map records only the existing implementation boundaries: `route_driver.py` interprets typed route state, `local_run_once.py` adapts it to existing lifecycle owners, and `agent-controller.yml` transports validated controller commands. No product/effect/evidence owner moves to route automation.

## Product Data Flow

Owned by the Architecture Book Product Golden Path contract; every effect binds exact task/run/lease/workspace/source/budget/artifact/approval/output/audit identities and fails closed on missing, stale, conflicting, late, over-budget, killed, revoked, expired, or outcome-unknown state.

## Authority Split

Admission, risk acknowledgement, one-use spend authorization, attempt admission/lease, artifact approval, output confirmation, and merge/release/deployment remain deliberately separate authorities; no earlier authority implies a later one. Owner: Architecture Book Authority Invariants.

## Ownership Maintenance

Managed-coding profiles, Provider protocol adapters, production runner wiring, and delegated autonomous Golden Path authority reuse the owners above. Current packet execution is owned solely by `docs/NEXT_DECISION.md`; longer-term ordering is owned solely by `docs/FUTURE_ROUTE.md`; neither creates runtime authority. No repository automation may become a second runtime, scheduler, store, budget, approval, output, audit, rollback, merge, release, or deployment owner.

Do not copy explanatory labels into file names. Always inspect the actual final branch tree before documenting an owner.

## Architecture Convergence Map

Architecture Convergence reused the owners above incrementally and is COMPLETE through AC7 (receipts: `docs/CURRENT_STATUS.md`). Frozen contracts live in their designated owners: AC3 responsibility matrix in `docs/CURRENT_STATUS.md`; AC4 transaction views and AC5 composition root and AC6 schema governance in `docs/ARCHITECTURE_BOOK.md`. The frozen RWE corpus remains the before/after compatibility oracle; architecture work cannot create a second scheduler, database, budget, approval, output, evidence, or rollback owner.

## Cost and Efficiency Evidence

Runtime usage evidence stays under existing execution-usage/scorecard/store owners; lifecycle-cost evidence vocabulary is owned by the Architecture Book evidence contract. It informs RWE replay and Level-2 GO/NO-GO and never becomes caller-supplied production authorization or a second budget system.

## Capability Boundaries

- Context capsules summarize and route from accepted owners; they never authorize execution, spend, output, merge, release, deployment, RWE acceptance, or production adoption.
- `CONTRACT`, `IMPLEMENT`, `EFFECT`, and `CLOSEOUT` are planning/execution classes, not new module owners. External effects and human decisions remain separately authorized even when adjacent provider-free packets can be safely consolidated.
- RWE must reuse existing task, scheduler, usage, scorecard, replay, audit, approval, output, terminal-evidence, and cleanup owners.
- Product durable memory and future experimental memory/skill projections are separate domains. No accepted Harness-Evolution projection owner exists yet; any future projection is derived, deletable, rebuildable, non-authoritative, and may not become routing, spend, evaluator, output, or adoption authority.
- External runtimes and repositories may provide bounded adapters, parsers, or comparison evidence; they may not replace the core owners.
- Provider/session logs are post-call evidence, not pre-call authority.
- Local price tables produce estimates only and must remain versioned and source-labeled.
- Deferred/stopped surfaces: OpenCode real-binary admission (Architecture Book), Vader/#208 (Playbook), Dashboard #225 (CURRENT_STATUS).
- Release, package, provenance, signing, installer, deployment, and rollback pipelines remain outside product/evolution authority.

## PE-5 Release Provenance Ownership

Existing package/container builders, dependency locks, SBOM/provenance, signing, installer, release, deployment, and rollback owners remain authoritative. Product, RWE, and Harness Evolution work may reference their evidence but gain no release, signing, installation, or deployment authority.

## PE-6 Fault Injection and Recovery Ownership

Existing disposable fault scenarios, SQLite/PostgreSQL recovery tests, stubs, cleanup, compensation, and rollback drills remain authoritative. Product/evolution work may reuse them but may not create a second recovery authority or convert a fixture result into production acceptance.

## Active Documents

- `START_HERE.md` — stable navigation, frontier discovery, capsule fields, staleness, and automation boundary.
- `docs/ARCHITECTURE_BOOK.md` — durable mission, architecture, authority, safety, and evidence invariants.
- `docs/CURRENT_STATUS.md` — accepted truth and confirmed gaps; never live PR/CI/review state.
- `docs/NEXT_DECISION.md` — one current executable window, entry/exit gates, and immediate next action.
- `docs/FUTURE_ROUTE.md` — blocked routing-only horizon; never execution authority.
- `docs/MODULE_MAP.md` — real current owners; never open-branch ownership claims.
- `docs/REAL_WORLD_TESTING_PLAYBOOK.md` and `docs/RUNBOOK.md` — operational validation and procedures.
- `README.md`, `AGENTS.md`, and `CLAUDE.md` — repository entry and contributor/agent instructions.

Prefer updating these documents over adding parallel roadmap, status, or policy files.
