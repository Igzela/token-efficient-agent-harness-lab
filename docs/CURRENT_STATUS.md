# Current Status

Last verified: 2026-05-28.

## Repository State

- Branch: `main` synced after `6d11c0f` (Phase 6A hardening — GPT review P0/P1 fixes).
- Tests: **1654 pass**, 0 failures.
- Security baseline: ALL CHECKS PASSED.

## New Session / Documentation Discipline

New Codex, Claude Code, or other coding-agent sessions must start with `docs/SESSION_START_HERE.md`, then this file, then `docs/NEXT_DECISION.md`.

The responsible coding agent has standing authority to autonomously advance repository-safe work: documentation repair, focused regression fixes, CI/security/test hardening, and architecture-book-defined dispatch-kernel phase work that remains deterministic, local, test-first, and does not add real providers, real sandbox/process execution, target repo writes, deployment, or real runtime workers.

After every commit-sized change, update this file if the change affects current state, verification, test count, stable commits, limitations, or next steps. Update `README.md`, `CLAUDE.md`, `AGENTS.md`, and `docs/MODULE_MAP.md` when their quickstart or ownership details change.

Run `python3 scripts/check_agent_handoff.py` before committing so the handoff surface remains self-consistent.

## Completed Tracks

| Track | Status |
|---|---|
| Stage 0 — Foundation | Complete |
| Stage 1 — Deterministic Harness Core | Complete |
| Stage 2 — Quality Runtime | Complete |
| Stage 3 — Controlled Intelligence Stubs | Complete |
| Stage 4 — Advanced Runtime Abstractions | Complete |
| CA-7 Sealed Baseline | Complete — policy baseline sealed |
| Post-closeout hardening/design | Complete |
| Harness App MVP0–MVP8 | Complete |
| Trial 0 — Real target acceptance | Closed — `PASS` |
| Trial 1 — Multi-task budget validation | Closed — `ACCEPTABLE_FOR_MULTI_TASK_TRIAL_AFTER_HARDENING` |
| Reliability Hardening 1 — Negated risk and triage | Complete |
| Demo packaging | Complete |
| Demo verification | Complete — all docs accurate and runnable |
| Trial 2 candidate selection | Planned — hermes-gateway-lab recommended |
| Trial 2 execution | Closed — `ACCEPTABLE_WITH_NOTES` (audit BLOCKED on target, generalization finding) |
| Target repo onboarding plan | Complete — plan and templates ready, awaiting user approval for target writes |
| Target repo onboarding (hermes-gateway-lab) | Complete — PR #1 merged (commit `77cf282`), onboarding files on target main, audit PASS_WITH_NOTES / blockers [] |
| Trial 2 onboarded replay | Closed — `ACCEPTABLE_FOR_ONBOARDED_SECOND_PROJECT_TRIAL` (audit PASS_WITH_NOTES, 0 blockers, 5 plans created) |
| Trial 2 final verification | Closed — `TRIAL_2_FINAL_VERIFICATION_PASS` (audit PASS_WITH_NOTES from target main, 5 plans, all boundary confirmed) |
| Trial 3 multi-repo generalization | Closed — `TRIAL_3_MULTI_REPO_GENERALIZATION_PASS` (3 repos: API/CLI/infra, all BLOCKED→PASS_WITH_NOTES, 6 plans, triage working) |
| Trial 3 target merge | Closed — all 3 target PRs merged, audit PASS_WITH_NOTES, blockers [] |
| Global Architecture Book v1 | Approved — 3-round Claude+GPT collaborative review, Phase 1 implementation-ready |
| Phase 1 — Dispatch Kernel | **STABLE** — 8 source files, 20 fixtures, 1074 total tests, commits `a4227e9`→`aed213b`→`592803f` |
| Phase 2 — Manual Execution Bridge | **STABLE** — 6 source modules, 6 test files, 1131 total tests, commits `afbba23`→`19c8a17`→`8f683ad` |
| Phase 3 — Provider Adapter Boundary | **STABLE** — 8 source modules, 8 test files, 1188 total tests, commits `c0ec508`→`e34ad8e`→`29fd12b`→`0092a1c`→`c631b4d`→`ef52704` |
| Phase 4 — Adaptive Routing | **STABLE** — 8 source modules, 8 test files, 1270 total tests, commits `ed2c762`→`66ebbc7` |
| Phase 5 — Multi-Agent Orchestration | **STABLE** — 11 source modules, 12 test files, 1454 total tests, GPT approved |
| Phase 6A — Local Durable API/Storage | **STABLE** — 5 source modules, 5 test files, 1596 total tests, GPT approved |
| Phase 6B-1 — Per-Server Route Isolation | **STABLE** — ServerContext pattern, backward-compatible, 4 isolation tests |
| Phase 6B-2 — Local API Key + Tenant Boundary | **HARDENED** — 1 source module (auth.py), 1 test file (test_auth.py), auth middleware in http_server.py, RequestContext flow into RouteMatch, 1654 total tests |

Trial 2 complete evidence chain: [`docs/trials/TRIAL_2_FINAL_STATE_INDEX.md`](trials/TRIAL_2_FINAL_STATE_INDEX.md).
Trial 3 report: [`docs/trials/TRIAL_3_REPORT.md`](trials/TRIAL_3_REPORT.md).
Trial 3 target merge closeout: [`docs/trials/TRIAL_3_TARGET_MERGE_CLOSEOUT.md`](trials/TRIAL_3_TARGET_MERGE_CLOSEOUT.md).

## Phase 1 Dispatch Kernel — Closeout

**Stable commit:** `592803f`
**P0 fixes:** `aed213b` (5 P0 blockers from GPT review)
**P1 evidence precision:** `592803f` (flag-specific negative evidence)
**Tests:** 1074 pass (was 914 at Phase 0 end)
**GPT verdict:** Phase 1 Stable — approved for Phase 2 planning

**Phase 1 boundaries (sacred):** no real provider calls, no sandbox execution, no target repo writes, no autonomous workers.

**Accepted limitations (non-blocking, Phase 2/3 refinement):**
- Compound "or" negations ("without any X or Y") only match first phrase
- Evidence spans use placeholder (0, 0) instead of exact phrase position
- Budget pressure is diagnostic, not a selector-changing mechanism
- fallback_tier mixes fallback/escalation semantics

**Next eligible path:** Phase 2 Manual Execution Bridge planning

## Phase 2 Manual Execution Bridge — Closeout

**Stable commit:** `8f683ad`
**P0 fixes (round 1):** `19c8a17` (5 P0 blockers from GPT review)
**P0 fixes (round 2):** `8f683ad` (2 unsafe defaults removed)
**Tests:** 1131 pass (was 1074 at Phase 1 end)
**GPT verdict:** Phase 2 Stable — approved for Phase 3 planning

**Phase 2 boundaries:** no provider calls, no automatic execution, human is executor, no real token counting.

**Source modules:**
- `prompt_pack_gen.py` — PromptPackGenerator (dispatch_id required)
- `manual_session.py` — ManualExecutionSession lifecycle tracking
- `pasteback_parser.py` — PastebackParser validates/hashes human-pasted output
- `manual_evaluator.py` — ManualEvaluator with 5 checks + boundary heuristics
- `manual_usage_bridge.py` — bridges PastebackSubmission → UsageLedgerRow (eval_result required)
- `cost_of_pass.py` — CostOfPassAccumulator aggregates by group

**Accepted limitations (non-blocking, Phase 3 refinement):**
- Pasteback stores raw_output inline (no redaction policy)
- ManualSessionStore lacks strict transition validation (happy-path only)
- Boundary compliance is heuristic, not authoritative
- Token estimates are rough char/4 estimates

**Next eligible path:** Phase 3 provider integration design

## Phase 4 Adaptive Routing — Closeout

**Stable commit:** `66ebbc7`
**Initial implementation:** `ed2c762` (8 new source modules, 8 test files, 72 new tests)
**Hardening fixes:** `66ebbc7` (2 P0 + 3 P1 issues from GPT review)
**Tests:** 1270 pass (was 1188 at Phase 3 end)
**GPT verdict:** Phase 4 Stable — approved for Phase 5 planning
**Review rounds:** 2 rounds of GPT review (Beta → Stable)

**Phase 4 boundaries:** adaptive routing is recommendation-only, never mutates state. Cold start always falls back to static rules. Promotion requires ALL gate conditions met. No real LLM calls in routing logic (rule-based). No persistence layer (in-memory only).

**Source modules:**
- `routing/schemas.py` — RoutingSelection, RoutingExperiment, RoutingArm, RoutingObservation, PromotionVerdict, make_task_group/parse_task_group
- `routing/history_store.py` — tier-aware history indexing wrapping CostOfPassAccumulator
- `routing/cost_of_pass_router.py` — best-tier-from-history logic
- `routing/promotion_gate.py` — shadow→active gate + RoutingObservationStore
- `routing/auto_policies.py` — AutoDowngradePolicy, AutoUpgradePolicy
- `routing/feedback_integrator.py` — quality→routing feedback loop
- `routing/dynamic_tier_selector.py` — adaptive tier selection wrapping ModelSelector
- `routing/__init__.py` — package wiring

**Hardening fixes (GPT review):**
1. RoutingSelection dataclass replaces 7-tuple — carries routing_mode/routing_experiment_id metadata
2. Task group delimiter changed from `_` to `/` via make_task_group/parse_task_group
3. PromotionGate requires both candidate AND baseline sample counts
4. routing_experiment_id propagated from RoutingSelection through DispatchEngine
5. Duplicate PromotionVerdict removed from promotion_gate.py, imported from schemas

**Accepted limitations (non-blocking, future refinement):**
- routing_experiment_id is supported but usually None until richer experiment tracking exists
- History and observation stores are in-memory
- Promotion logic is deterministic threshold-based, not statistical
- Adaptive routing depends on quality/cost observations supplied by upstream evaluators

## Phase 5 Multi-Agent Orchestration — Closeout

**Initial implementation:** 11 new source modules, 9 test files, 111 new tests
**Hardening commit:** `80641b0` — all GPT review P0/P1 items fixed, 1 new test file (23 hardening tests)
**Mandatory dispatch fix:** — dispatch_id now required in create_workflow() and decompose()
**Terminal cleanup fix:** — cancel/budget-overrun paths now release agents, cancel all non-terminal nodes
**Tests:** 1454 pass (was 1270 at Phase 4 end)
**Status:** **STABLE** — GPT re-review approved after 3 rounds (Beta → Re-review #1 → Re-review #2 → Stable)

**P0 fixes (8):**
- Dispatch gating: create_workflow rejects non-decided decisions
- Mandatory dispatch context: create_workflow() requires dispatch_id (no analysis_id fallback)
- Terminal cleanup: cancel() releases all non-terminal nodes; budget-overrun releases agent before returning failed
- Terminal semantics: failed nodes never silently complete
- Approval reachability: failed/completed nodes trigger approval gate
- Budget enforcement: overrun triggers fail, agent_id recorded correctly
- Registry lifecycle: agents released on complete/fail/cancel
- State unification: WorkQueue stateless, graph is sole source of truth

**P1 fixes (4):**
- Schema: WorkflowGraph.updated_at field added
- DependencyResolver: execution_order validates graph first
- ConflictResolver: resource_conflict only for concurrent running nodes
- TaskDecomposer: node_id generated before registry assignment

**Phase 5 boundaries:** WorkflowEngine orchestrates independently (does NOT call DispatchEngine). Agent execution is simulated (StubAgent returns mock output). No autonomous agent spawning without a dispatch decision. In-memory stores only. Rule-based decomposition and conflict resolution (no LLM calls). Human approval gates block workflow progression.

**Source modules:**
- `orchestration/schemas.py` — WorkflowGraph, WorkflowNode, WorkflowEdge, AgentMessage, ConflictRecord, AgentRole + constants
- `orchestration/agent_role_registry.py` — AgentRoleRegistry: register, lookup, assign, release
- `orchestration/task_decomposer.py` — TaskDecomposer: TaskAnalysis → WorkflowGraph (rule-based, 1/2/4 node graphs)
- `orchestration/dependency_resolver.py` — DependencyResolver: validate (cycle detection), execution_order (topological sort), ready_nodes
- `orchestration/work_queue.py` — WorkQueue: enqueue, dequeue_ready, start, complete, fail, cancel
- `orchestration/workflow_engine.py` — WorkflowEngine: create_workflow, tick, resume_after_approval, cancel, complete_node, fail_node
- `orchestration/conflict_resolver.py` — ConflictResolver: detect_conflicts (output/resource/dependency/budget), resolve
- `orchestration/result_aggregator.py` — ResultAggregator: aggregate, is_complete
- `orchestration/human_approval_gate.py` — HumanApprovalGate: requires_approval, approve, reject
- `orchestration/multi_agent_budget.py` — MultiAgentBudgetManager: workflow/agent/node level budget enforcement
- `orchestration/__init__.py` — barrel re-exports

**Accepted limitations (non-blocking, future refinement):**
- WorkflowEngine does not call DispatchEngine (no real provider integration)
- Agent execution is simulated — no real LLM calls
- In-memory stores only (no persistence)
- Rule-based decomposition (1/2/4 node graphs based on complexity thresholds)
- Conflict resolution is deterministic, not statistical
- HumanApprovalGate triggers are heuristic (budget threshold, failure)

## Phase 6A Local Durable API/Storage — Closeout

**Stable commit:** `6d11c0f`
**Initial implementation:** `2e9f754` (5 new source modules, 5 test files, 137 new tests)
**Hardening fixes:** `6d11c0f` (2 P0 + 6 P1 issues from GPT review)
**Tests:** 1596 pass (was 1454 at Phase 5 end)
**Status:** **STABLE** — GPT approved after 2 review rounds (Beta → Stable)

**Phase 6A boundaries:** stdlib only (`http.server`, `sqlite3`, `logging`). No FastAPI, no PostgreSQL, no third-party packages. No auth/tenancy, no rate limiting, no real providers.

**Source modules:**
- `observability.py` — StructuredFormatter, MetricsCollector (ring buffer), RequestTracer (trace/span propagation)
- `durable_store.py` — SQLite-backed DurableStore (plans, repos, events, migration_log), thread-safe with write lock
- `storage_migrator.py` — JSON/JSONL → SQLite batch migration with MigrationReport/FullMigrationReport
- `http_server.py` — HarnessHTTPHandler wrapping stdlib http.server, route dispatch, ServerConfig
- `health_checker.py` — HealthChecker with storage/events/plans probes, /api/v1/health and /api/v1/ready

**Accepted limitations (non-blocking, future refinement):**
- No auth/tenancy (Phase 6B scope)
- No PostgreSQL (Phase 6B scope)
- No rate limiting (Phase 6B scope)
- In-memory MetricsCollector (no persistence)
- RequestTracer spans are in-memory only
- Health probes are basic connectivity checks

## Phase 3 Provider Adapter Boundary — Closeout

**Stable commit:** `ef52704`
**P0 fixes (round 1):** `e34ad8e` (5 P0 blockers from GPT review)
**P0 fixes (round 2):** `29fd12b` (provider execution blocked when decision not decided)
**P1 hardening:** `0092a1c` (5 P1 items: user intent guard, mocked tests, audit safety, retry docs, cost tracking)
**Final fix:** `c631b4d` (ProviderConfig.enabled enforcement)
**CA-7 compliance fix:** `ef52704` (removed bundled `urllib` transport; provider adapter now requires test-injected transport seam)
**Tests:** 1188 pass (was 1131 at Phase 2 end)
**GPT verdict:** Phase 3 Stable — approved for Phase 4 planning
**Review rounds:** 4 rounds of GPT review (Alpha → Beta → Release Candidate → Stable)

**Phase 3 boundaries:** provider execution only when decision_status == "decided" and no user-negated provider intent. Budget-exhausted is terminal. Disabled provider config blocks all execution. No bundled network transport, provider SDK, API key, or real model call is active under the CA-7 baseline.

**Source modules:**
- `provider/provider_config.py` — ProviderConfig, CredentialRef, RetryPolicy (with pricing fields)
- `provider/credential_boundary.py` — env-only credential resolution
- `provider/redaction.py` — secret stripping from text and audit fields
- `provider/audit_recorder.py` — ProviderAuditEvent + in-memory recorder (never stores raw prompt/response)
- `provider/provider_executor.py` — duck-typed ProviderExecutor + StubProvider
- `provider/openai_provider.py` — OpenAI-compatible adapter with test-injected transport; no bundled network import under CA-7
- `provider/retry_manager.py` — RetryFallbackManager with budget check, backoff, fallback routing

**Accepted limitations (non-blocking, future refinement):**
- Only env credential backend active (file/keyring/vault are schema-reserved)
- Audit recorder is in-memory (no persistent store)
- OpenAI-compatible request/response adapter only; real transport, Anthropic, and local adapters are future CA-8/provider-integration work
- Cost depends on configured pricing and provider-reported usage
- No production auth/multitenancy/rate-limit service layer

## Current App Capability

The local Harness App (MVP0–MVP8) provides:

- **Repo registry** — register local or remote target repositories.
- **Local target audit** — read-only inspection of harness control files in a target repo.
- **Non-executable planning** — deterministic resource plans with steps, budgets, approval gates, and blockers. Plans are never executed.
- **App-owned plan store** — plans persist in a local JSON file owned by the app.
- **Plan review workbench** — plan history, summary, comparison, and advisory review actions.
- **Review guidance** — non-persistent advisory guidance derived from stored plans.
- **Portfolio triage** — read-only ranking of stored plans by risk, budget, and bottleneck.
- **Operations diagnostics** — component health, data flow, storage status, recent errors.

## State Boundary

| State | Owner | Writable | Description |
|---|---|---|---|
| Target repositories | User | No (read-only by app) | The app never writes to target repos. |
| App registry | App | Yes | Stores registered repo metadata. |
| Plan store | App | Yes | Stores non-executable resource plans. |
| Diagnostics | Derived | No | Computed on each request from app state. |
| Review guidance | Derived | No | Computed from plan store. Not persisted. |
| Portfolio triage | Derived | No | Computed from plan store. Not persisted. |

No app output constitutes execution authority. The human operator remains the final decision-maker.
