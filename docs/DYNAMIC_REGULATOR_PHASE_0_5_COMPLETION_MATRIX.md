# Dynamic Global Regulator — Phase 0–5 Completion Matrix

Generated: 2026-06-11 | PR: #31 | Commit: 713af59

## Summary Table

| Phase | Goal | Status | Evidence | Key Gaps |
|-------|------|--------|----------|----------|
| 0 | Baseline Documentation and Observability | PARTIAL | `/api/v1/dispatch-metrics` endpoint + dashboard Metrics subsection exist; data derived from existing dispatch bundles | No structured logging infrastructure; no `docs/DISPATCH_OBSERVABILITY.md`; no per-decision log calls for tier rationale/complexity/constraints |
| 1 | ContextPack Cross-Node Assembly | FOUNDATION_ONLY | `context_pack` module with `assemble_context_injection()`, budget config, validation types; store-level integration test passes | Not wired into `DynamicWorkflowController::tick()`; no ContextBridge (field mapping); no ContextBudgetAllocator (cross-node distribution); nodes execute in isolation |
| 2 | Feedback Ledger and Replayable Run Traces | PARTIAL | `/api/v1/feedback/traces` and `/api/v1/feedback/cost-of-pass` endpoints; dashboard Feedback subsection; outcome attribution inline; cost-of-pass calculator inline | No RunTraceRecorder (traces derived ad-hoc from bundles); no OutcomeAttributor module; no PatternDetector; no replayability guarantee |
| 3 | Shadow Adaptive Policy Simulation | PARTIAL | Shadow routes generated at dispatch time via `build_shadow_routes()`; `/api/v1/simulation/report` endpoint; all influence flags disabled; dashboard Simulation subsection | No ShadowRouter module; no PolicySimulator (replay through candidate policies); no delta metrics (success rate, cost, latency, human review) |
| 4 | Human-Approved Policy Proposals | PARTIAL | Full CRUD lifecycle (create/list/approve/reject/deactivate/rollback); `confirm_policy_override` guard; `team:admin` auth; safe-tier restriction; `active_routing_policy()` integration; v12 migration; audit trail; dashboard Proposals subsection; TS+Python SDK coverage | No PolicyProposer (auto-generates from feedback); no ProposalValidator module; no ProposalSerializer module; proposals created manually via API only |
| 5 | Limited Automatic Adjustment Under Strict Guards | NOT_STARTED | Nothing exists | No AutoAdjustmentPolicy, AutoAdjustmentGuard, PolicySnapshot, `/api/v1/auto-adjustments`, dashboard tab, `ACP_ENABLE_AUTO_ADJUSTMENT` gate, auto-rollback, or boundary mutation tests |

## Phase Details

### Phase 0: Baseline Documentation and Observability

**Plan Goal:** Establish the measurement foundation — know what we're measuring before changing anything.

**Status:** PARTIAL

**Implemented in #31:**
- `dispatch_metrics()` storage method in `dispatch.rs:303` with aggregation by tier, task_class, final_status, evaluation_status
- `GET /api/v1/dispatch-metrics` HTTP endpoint in `handlers/dispatch.rs:166`
- Route registration in `routes.rs:66-67`
- Dashboard `DynamicRegulator.tsx` renders dispatch metrics: totals (dispatches, pass rate, cost, shadow routes, active proposals) and tier metrics table
- TS SDK: `dispatchMetrics()` method with `DispatchMetricsOptions`
- Python SDK: `dispatch_metrics(limit)` method
- Dashboard api-client: `fetchDispatchMetrics()`

**Missing from plan:**
- Structured logging for tier selection rationale, complexity score, constraint matches, retry triggers, evaluation outcomes at dispatch time (data is recorded in bundles but not via structured logging infrastructure)
- `docs/DISPATCH_OBSERVABILITY.md` defining metrics taxonomy
- Per-decision structured log calls in the dispatch pipeline

**Tests Present:**
- 2 tests (SDK only): TS `dispatchMetrics sends limit query param`, Python `test_dispatch_metrics_sends_limit`

**Tests Missing:**
- No Rust integration test for the `/api/v1/dispatch-metrics` HTTP endpoint
- No store-level unit test for `dispatch_metrics()` method on `LocalProductStore`
- No test verifying correct aggregation buckets (by_tier, by_task_class, by_final_status, by_evaluation_status)
- No test verifying empty-store returns zero totals
- No test verifying `limit.clamp(0, 500)` boundary

**Safety/Boundary Risks:**
- None — endpoint is read-only, no dispatch behavior change, metrics are diagnostic only

**Recommended Next PR:**
- Add Rust integration test for `/api/v1/dispatch-metrics` HTTP endpoint
- Add store-level test for `dispatch_metrics()` aggregation correctness
- Create `docs/DISPATCH_OBSERVABILITY.md` with metrics taxonomy

---

### Phase 1: ContextPack Cross-Node Assembly

**Plan Goal:** Enable completed nodes to pass outputs as context to downstream nodes.

**Status:** FOUNDATION_ONLY

**Implemented in #31:**
- `context_pack` module (`engine/src/workflow/context_pack/`) with `assembly.rs`, `budget.rs`, `rules.rs`, `types.rs`, `validation.rs`
- `assemble_context_injection()` function: collects predecessor outputs, applies token budget, truncates, produces `context_injection.v1` metadata
- `ContextAssemblyConfig` with `ACP_CONTEXT_ASSEMBLY_ENABLED` and `ACP_CONTEXT_ASSEMBLY_MAX_TOKENS` env vars
- `ContextSource` struct (edge_id, from_node_id, output)
- `ContextBudget` struct (max_context_tokens, preferred_context_tokens, max_response_tokens, reserved_response_tokens)
- `check_budget_compliance()` and `apply_prune_policy()` for single-pack budget enforcement
- Context layers validation (`validate_context_layers`), advisor/model context pack validation

**Missing from plan:**
- ContextBridge — no output-field-to-input-field mapping based on DAG edge metadata (all predecessor output injected wholesale)
- ContextBudgetAllocator — no cross-node budget distribution proportional to complexity/relevance (single global `max_context_tokens` config)
- `DynamicWorkflowController::tick()` integration — `tick()` at `dynamic_controller.rs:150` does NOT call `assemble_context_injection`; nodes execute in isolation
- `context_injection` metadata on `WorkflowRunNode` for auditability
- No test proving Node B receives Node A's output via context injection in a real workflow execution

**Tests Present:**
- 3 tests: `assembles_sources_with_budget_metadata` (assembly.rs:108), `disabled_config_returns_none` (assembly.rs:130), `workflow_tick_injects_completed_predecessor_context_into_metadata` (test_local_product_store.rs:717)

**Tests Missing:**
- No test for budget truncation when predecessor output exceeds `max_context_tokens`
- No test verifying context propagation is skipped for nodes with no predecessor edges
- No test for multiple predecessor nodes feeding into a single downstream node
- No test for context injection additive behavior (does not replace existing node input)

**Safety/Boundary Risks:**
- Foundation types exist but are not wired into execution — no runtime risk from dead code, but misleading to claim Phase 1 is complete when context assembly does not actually run during workflow execution

**Recommended Next PR:**
- Wire `assemble_context_injection()` into `DynamicWorkflowController::tick()` so nodes actually receive predecessor context
- Add integration test proving end-to-end context propagation
- Implement ContextBridge field mapping for selective output injection

---

### Phase 2: Feedback Ledger and Replayable Run Traces

**Plan Goal:** Transform the event-sourced ledger into a queryable feedback store with outcome attribution.

**Status:** PARTIAL

**Implemented in #31:**
- `feedback_traces()` storage method in `dispatch.rs:349` with task_class/tier/status filtering, produces `feedback_traces.v1`
- `GET /api/v1/feedback/traces` endpoint in `handlers/dispatch.rs:186`
- `cost_of_pass()` storage method in `dispatch.rs:378` with task_class/tier grouping, pass_rate, average_cost_usd, produces `feedback_cost_of_pass.v1`
- `GET /api/v1/feedback/cost-of-pass` endpoint in `handlers/dispatch.rs:209`
- Dashboard Feedback subsection in `DynamicRegulator.tsx` with traces table (8 rows) and cost-of-pass table (8 rows)
- Outcome attribution done inline from `final_status` + `evaluation_status` in dispatch bundles
- CostOfPassCalculator logic embedded in `cost_of_pass()` method
- TS SDK: `feedbackTraces()` with filters, `feedbackCostOfPass()` with filters
- Python SDK: `feedback_traces()`, `feedback_cost_of_pass()`

**Missing from plan:**
- RunTraceRecorder as a dedicated component — traces are derived ad-hoc from `dispatch_history` bundles, not captured as structured decision→execution→evaluation chains with timestamps
- OutcomeAttributor as a dedicated reusable module — attribution is inline/computed, not a module that links success/failure to specific decision factors
- PatternDetector — no recurring failure pattern detection (e.g., "cheap tier fails on architecture tasks")
- Replayability guarantee — traces read from `dispatch_history`, not stored as separate replayable trace records

**Tests Present:**
- 8 tests: SDK URL-construction mocks (2), cost-of-pass parsing/aggregation unit tests (4), routing history store aggregation tests (2)

**Tests Missing:**
- No Rust integration test for `/api/v1/feedback/traces` HTTP endpoint
- No Rust integration test for `/api/v1/feedback/cost-of-pass` HTTP endpoint
- No store-level test for `feedback_traces()` method on `LocalProductStore`
- No store-level test for `cost_of_pass()` method on `LocalProductStore`
- No test verifying `feedback_traces` filtering by task_class, tier, and status
- No test verifying `cost_of_pass` `average_cost_usd` calculation correctness
- No test verifying traces are replayable

**Safety/Boundary Risks:**
- Endpoints are read-only; feedback data does not influence dispatch decisions; no mutation of active routing — all safety gates met

**Recommended Next PR:**
- Add Rust integration tests for both `/api/v1/feedback/*` HTTP endpoints
- Add store-level tests for `feedback_traces()` and `cost_of_pass()` methods
- Consider extracting RunTraceRecorder as a dedicated module for trace independence from bundle format

---

### Phase 3: Shadow Adaptive Policy Simulation

**Plan Goal:** Compare "what the regulator would do" against "what the kernel does" without affecting live traffic.

**Status:** PARTIAL

**Implemented in #31:**
- `ShadowRoute` struct in `dispatch_decision.rs:152` with tier, profile_id, reason, admission_scope, estimated_cost, expected_tradeoff
- `build_shadow_routes()` in `dispatch_decision.rs:569` generates fallback + cheap_executor alternatives at dispatch time
- Shadow routes recorded in every dispatch bundle's `decision.shadow_routes` array
- `simulation_report()` storage method in `dispatch.rs:435` reads shadow_routes from dispatch bundles, produces `dispatch_simulation_report.v1`
- `GET /api/v1/simulation/report` endpoint in `handlers/dispatch.rs:227`
- `shadow_influence_disabled()` — all influence flags are false (selected_tier, budget_reservation, executor_selection, retry_path, decision_status, routing_mode)
- Dashboard Simulation subsection in `DynamicRegulator.tsx`
- TS SDK: `simulationReport()` with limit filter
- Python SDK: `simulation_report(limit)`

**Missing from plan:**
- ShadowRouter as a dedicated component — shadow routes built inline in `dispatch_decision.rs`, not as a separate module computing "what the regulator would have chosen"
- PolicySimulator — no replay of historical traces through candidate policies to measure outcome delta
- Delta metrics — simulation report shows shadow routes but does not compute `success_rate_delta`, `cost_delta`, `latency_delta`, `human_review_rate_delta` between real and shadow decisions
- Before/after comparison between policy A and policy B

**Tests Present:**
- 2 tests (SDK only): TS `simulationReport sends limit query param`, Python `test_simulation_report_sends_limit`

**Tests Missing:**
- No Rust integration test for `/api/v1/simulation/report` HTTP endpoint
- No store-level test for `simulation_report()` method on `LocalProductStore`
- No test verifying simulation_report correctly counts shadow routes per tier
- No test verifying shadow routing never influences actual dispatch outcome (safety gate untested at integration level)
- No test verifying simulation_report with dispatches that have no shadow_routes returns empty `by_shadow_tier`

**Safety/Boundary Risks:**
- Safety gate `shadow_influence_disabled()` correctly enforces fire-and-forget — no code path allows shadow decision to override real decision. However, this critical invariant has no dedicated integration test.

**Recommended Next PR:**
- Add Rust integration test for `/api/v1/simulation/report` HTTP endpoint
- Add store-level test for `simulation_report()` aggregation
- Add dedicated test for `shadow_influence_disabled()` invariant

---

### Phase 4: Human-Approved Policy Proposals

**Plan Goal:** Generate structured policy change proposals that require human approval before activation.

**Status:** PARTIAL

**Implemented in #31:**
- `create_policy_proposal()` in `policy_proposals.rs:23` with `validate_proposal_request`, `validate_policy_override`, audit logging
- `list_policy_proposals()` in `policy_proposals.rs:114` with limit/offset/status filtering
- `get_policy_proposal()` in `policy_proposals.rs:198` by proposal_id
- `approve_policy_proposal()` in `policy_proposals.rs:238` with `confirm_policy_override` guard
- `reject_policy_proposal()` in `policy_proposals.rs:251`
- `deactivate_policy_proposal()` in `policy_proposals.rs:260` with `confirm_policy_override` guard
- `rollback_policy_proposal()` in `policy_proposals.rs:273` with `confirm_policy_override` guard
- `transition_policy_proposal()` in `policy_proposals.rs:344` with status validation and `supersede_same_key` logic
- `active_routing_policy()` in `policy_proposals.rs:286` builds `DispatchRoutingPolicy` from all active proposals
- `confirm_policy_override` required flag on approve, deactivate, rollback
- `team:admin` authorization on approve/reject/deactivate/rollback handlers
- `require_auth_for_policy_override()` in `handlers/dispatch.rs:425` requires configured `tenant_resolver`
- `is_safe_policy_override_tier()` in `model_selector.rs:81` restricts to `SAFE_POLICY_OVERRIDE_TIERS`
- `validate_policy_override()` in `policy_proposals.rs:552` checks task_domain, task_intent, target_tier validity
- Integration with dispatch path: `dispatch_with_policy()` in `dispatch_engine.rs:145` creates `ModelSelector` with policy override
- v12 migration creates `controlled_loop_policy_proposals` table with indexes
- HTTP endpoints: `POST /api/v1/proposals`, `GET /api/v1/proposals`, `GET /api/v1/proposals/:id`, `POST .../approve`, `POST .../reject`, `POST .../deactivate`, `POST .../rollback`
- Dashboard Proposals subsection in `DynamicRegulator.tsx`
- ConfirmDialog supports approve/reject/rollback action types
- TS SDK: full CRUD + approve/reject/rollback/deactivate
- Python SDK: full CRUD + approve/reject/rollback/deactivate
- SQLite + PostgreSQL dual-backend support for all proposal operations
- Audit trail for all lifecycle events (create, approve, reject, deactivate, rollback, supersede)

**Missing from plan:**
- PolicyProposer as an automated component — proposals are created manually via API, not auto-generated from feedback patterns + shadow simulation results
- No automatic proposal generation from feedback traces/simulation data
- Dashboard `DynamicRegulator.tsx` renders proposals as read-only table — no action buttons invoke approve/reject/rollback despite `ConfirmDialog` supporting these action types
- No proposal creation form in dashboard
- No evidence visualization in dashboard proposals view
- No proposal detail view in dashboard

**Tests Present:**
- 11 tests: store lifecycle test (`proposal_lifecycle_builds_active_policy`), store CLI tier rejection test, HTTP integration test (full create→approve→dispatch lifecycle with confirmation guard), integrity table check, SDK tests for CRUD and confirmation guards (TS: 5, Python: 4)

**Tests Missing:**
- No HTTP test proving proposals cannot override CLI tiers through the HTTP endpoint (only tested at store level)
- No HTTP test proving `confirm_policy_override` is required for deactivate and rollback (only approve tested at HTTP level)
- No HTTP test proving `team:admin` scope is required (existing test uses a key WITH `team:admin`; no test with a key missing it returns 403)
- No test proving proposals cannot override unsafe tiers beyond CLI
- No store-level tests for reject, rollback, deactivate, list with filter, get-missing-ID, supersede-same-key flows
- No test verifying proposal audit trail entries

**Safety/Boundary Risks:**
- CLI tier override rejection is tested at store level but not at HTTP level — a regression in the HTTP handler could bypass this safety gate
- `team:admin` enforcement has no negative test (missing scope → 403) at HTTP level
- `confirm_policy_override` guard is only tested for approve at HTTP level, not for rollback/deactivate

**Recommended Next PR:**
- Add HTTP integration tests for: CLI tier rejection via POST endpoint, `team:admin` 403 rejection, `confirm_policy_override` required for rollback/deactivate
- Add store-level tests for reject/rollback/deactivate/list-filter/get-missing flows
- Wire `ConfirmDialog` into `DynamicRegulator.tsx` proposals table for approve/reject/rollback actions

---

### Phase 5: Limited Automatic Adjustment Under Strict Guards

**Plan Goal:** Allow a narrow class of policy adjustments to apply automatically, with strict guardrails.

**Status:** NOT_STARTED

**Implemented in #31:**
- Nothing.

**Missing from plan:**
- AutoAdjustmentPolicy — whitelist of adjustment types allowed without human approval
- AutoAdjustmentGuard — enforces adjustment magnitude limit, rate limit, rollback trigger
- PolicySnapshot — captures full policy state before each auto-adjustment
- `/api/v1/auto-adjustments` endpoint showing adjustment history and current state
- Dashboard Auto-Adjustments tab with timeline and rollback controls
- `ACP_ENABLE_AUTO_ADJUSTMENT=0` default environment variable gate
- Auto-rollback trigger on success rate degradation
- Rate limiting for auto-adjustments
- Before/after state recording for each adjustment
- Human manual revert controls
- Tests: no safety/auth/provider/CLI/hard-constraint boundary mutation

**Tests Present:**
- 0 tests (consistent with NOT_STARTED status)

**Tests Missing:**
- All Phase 5 tests are missing — component does not exist

**Safety/Boundary Risks:**
- N/A — nothing is implemented

**Recommended Next PR:**
- Phase 5 should not begin until Phases 0–4 gaps are closed and tested

---

## Phase 5 Gap Analysis

| Component | Exists? | Evidence |
|-----------|---------|----------|
| AutoAdjustmentPolicy (whitelist of adjustment types) | No | No file, struct, or enum defines allowed adjustment types |
| AutoAdjustmentGuard (magnitude limit, rate limit, rollback trigger) | No | No guard module or enforcement logic exists |
| Whitelist of allowed adjustment types | No | No whitelist definition anywhere in codebase |
| Magnitude caps on adjustments | No | No magnitude limit logic exists |
| Rate limits on adjustments | No | No rate limiting logic exists |
| PolicySnapshot (before/after state capture) | No | No snapshot capture mechanism exists |
| Auto-rollback on degradation | No | No auto-rollback trigger logic exists |
| `/api/v1/auto-adjustments` endpoint | No | No route registered in `routes.rs` |
| Dashboard Auto-Adjustments tab with timeline | No | No component in dashboard |
| Human manual revert controls | No | No revert UI or API for auto-adjustments |
| `ACP_ENABLE_AUTO_ADJUSTMENT=0` default gate | No | No environment variable defined |
| Boundary mutation tests (no safety/auth/provider/CLI/hard-constraint mutation) | No | No tests exist because no component exists |

## Overclaims Fixed

### 1. CURRENT_STATUS.md — "Phase 4 controlled loop"

**Original:** "Dynamic Global Regulator MVP + Phase 4 controlled loop pilot"

**Corrected:** "Dynamic Global Regulator MVP + Phase 4 proposal lifecycle CRUD (manual creation via API; PolicyProposer auto-generation not implemented)"

**Reason:** The term "controlled loop" implies the feedback-to-proposal loop is closed (PolicyProposer auto-generates proposals from feedback analysis). In reality, the three core Phase 4 components — PolicyProposer, ProposalValidator, ProposalSerializer — are absent. Proposals are created manually via API. This is a CRUD lifecycle, not a closed loop.

### 2. CURRENT_STATUS.md — "derived regulator read models"

**Original:** "Dispatch history now exposes derived regulator read models for outcome metrics, feedback traces, cost-of-pass aggregates, and shadow simulation reports."

**Corrected:** "Dispatch history now exposes read-only API endpoints that derive metrics, traces, cost-of-pass, and shadow route data from existing dispatch bundles. Phase 0 structured logging, Phase 1 ContextBridge/ContextBudgetAllocator, Phase 2 RunTraceRecorder, and Phase 3 ShadowRouter/PolicySimulator are not implemented."

**Reason:** The framing as fulfilling Phases 0–3 is an overclaim. The endpoints expose existing data, not the dedicated components the plan requires.

### 3. CURRENT_STATUS.md — "controlled-loop policy proposals"

**Original:** "A v12 local-store table records controlled-loop policy proposals; activation requires configured auth, team:admin, and confirm_policy_override=true."

**Corrected:** "A v12 local-store table records manually-created policy proposals; activation requires configured auth, team:admin, and confirm_policy_override=true. No automated proposal generation from feedback analysis exists."

**Reason:** "Controlled-loop" implies auto-generation from feedback. The table stores manually-created proposals.

### 4. CURRENT_STATUS.md — "/api/v1/proposals stores pending controlled-loop proposals"

**Original:** "/api/v1/proposals stores pending controlled-loop proposals; approve/deactivate/rollback require local auth, team:admin, and explicit confirmation."

**Corrected:** "/api/v1/proposals stores pending manually-created proposals with full CRUD lifecycle; approve/deactivate/rollback require local auth, team:admin, and explicit confirmation."

**Reason:** Same overclaim — "controlled-loop" implies auto-generation.

### 5. NEXT_DECISION.md — "Validate Dynamic Global Regulator MVP + Phase 4 controlled loop"

**Original:** "Validate Dynamic Global Regulator MVP + Phase 4 controlled loop through real-world pilot tasks, CI, and targeted hardening."

**Corrected:** "Validate Dynamic Global Regulator read-model endpoints and proposal CRUD lifecycle through real-world pilot tasks, CI, and targeted hardening. Core Phase 1–4 components (ContextBridge, ContextBudgetAllocator, RunTraceRecorder, ShadowRouter, PolicySimulator, PolicyProposer) remain unimplemented."

**Reason:** Framing validation of "Phase 4 controlled loop" when the loop is not closed sets incorrect expectations for what validation means.

### 6. NEXT_DECISION.md — Missing component inventory

**Original:** Allowed paths mention "dynamic regulator hardening: focused tests and CI fixes for metrics/traces/cost/simulation/proposal paths"

**Corrected:** Should enumerate which plan components are missing: ContextBridge, ContextBudgetAllocator, RunTraceRecorder, ShadowRouter, PolicySimulator, PolicyProposer, ProposalValidator, ProposalSerializer. The current phrasing implies the paths exist and need hardening, not that core components are absent.

## Recommended PR Sequence

### 1. Current PR (#31) — Description Correction

**What it is:** Read-model endpoints (`/api/v1/dispatch-metrics`, `/api/v1/feedback/traces`, `/api/v1/feedback/cost-of-pass`, `/api/v1/simulation/report`) derived from existing dispatch bundles, plus a policy proposal CRUD lifecycle with safety gates (`confirm_policy_override`, `team:admin`, safe-tier restriction, `active_routing_policy()` dispatch integration).

**What it is NOT:** A "Phase 4 controlled loop" or completion of Phases 0–4. The feedback-to-proposal loop is not closed. ContextBridge, ContextBudgetAllocator, RunTraceRecorder, ShadowRouter, PolicySimulator, PolicyProposer, ProposalValidator, and ProposalSerializer are all absent.

**Merge recommendation:** See Merge Decision below.

### 2. Next PR — Safety Gate Test Hardening

**Scope:** Add missing HTTP integration tests for critical safety invariants:
- CLI tier rejection through POST `/api/v1/proposals` (HTTP level, not just store level)
- `team:admin` 403 rejection for all proposal endpoints
- `confirm_policy_override` required for rollback and deactivate (HTTP level)
- `shadow_influence_disabled()` invariant (integration level)
- Store-level tests for reject/rollback/deactivate/list-filter/get-missing proposal flows
- Rust integration tests for `/api/v1/dispatch-metrics`, `/api/v1/feedback/traces`, `/api/v1/feedback/cost-of-pass`, `/api/v1/simulation/report` HTTP endpoints

**Rationale:** These are pure test additions with no behavior change. They close the test coverage gaps for existing safety gates before any new functionality is added.

### 3. Future PR — Phase 1 Wiring (Context Assembly)

**Scope:** Wire `assemble_context_injection()` into `DynamicWorkflowController::tick()` so nodes actually receive predecessor context. Add integration test proving end-to-end context propagation.

### 4. Future PR — Phase 2 RunTraceRecorder

**Scope:** Extract RunTraceRecorder as a dedicated module that captures structured decision→execution→evaluation chains independently of dispatch bundle format. Add PatternDetector for recurring failure detection.

### 5. Future PR — Phase 3 Delta Metrics

**Scope:** Implement PolicySimulator to replay traces through candidate policies. Compute delta metrics (success_rate_delta, cost_delta, latency_delta, human_review_rate_delta). Surface in simulation report.

### 6. Future PR — Phase 4 PolicyProposer

**Scope:** Implement PolicyProposer to auto-generate proposals from feedback patterns and simulation results. Implement ProposalValidator (safety constraint checking) and ProposalSerializer (human-readable output with evidence and rollback plan). Wire dashboard ConfirmDialog for approve/reject/rollback actions.

### 7. Future PR — Phase 5 Implementation

**Scope:** Only after Phases 0–4 are fully implemented and tested. Implement AutoAdjustmentPolicy (whitelist), AutoAdjustmentGuard (magnitude/rate/rollback), PolicySnapshot, `/api/v1/auto-adjustments`, dashboard timeline, `ACP_ENABLE_AUTO_ADJUSTMENT=0` gate, auto-rollback trigger, manual revert controls, boundary mutation tests.

## Merge Decision

**PR #31 is NOT auto-merge eligible. It requires human review.**

**Reasons:**

1. **Status document overclaims:** CURRENT_STATUS.md and NEXT_DECISION.md describe the implementation as "Phase 4 controlled loop" when the feedback-to-proposal loop is not closed. Three of Phase 4's core components (PolicyProposer, ProposalValidator, ProposalSerializer) are absent. Phases 0–3 are described as complete when dedicated components specified in the plan (RunTraceRecorder, ShadowRouter, PolicySimulator, ContextBridge, ContextBudgetAllocator) do not exist. The status documents must be corrected before merge to prevent incorrect expectations.

2. **Critical safety gate test gaps:** CLI tier override rejection is only tested at the store level, not through the HTTP endpoint. `team:admin` enforcement has no negative test (missing scope returns 403). `confirm_policy_override` is only tested for approve at HTTP level, not for rollback/deactivate. These are safety-critical boundaries that should have HTTP-level regression tests before the feature is exposed.

3. **Phase 1 foundation is dead code:** The `context_pack` module (assembly, budget, rules, types, validation) exists but is not wired into `DynamicWorkflowController::tick()`. This is not harmful but contributes to a misleading impression that Phase 1 is functional.

4. **No Rust integration tests for new endpoints:** Four new HTTP endpoints (`/api/v1/dispatch-metrics`, `/api/v1/feedback/traces`, `/api/v1/feedback/cost-of-pass`, `/api/v1/simulation/report`) have zero Rust integration tests. Only SDK URL-construction mocks exist.

**Recommended action:** Correct status documents to accurately describe what is implemented (read-model endpoints + proposal CRUD lifecycle) vs what remains (Phases 0–4 core components + Phase 5). Add HTTP-level safety gate tests. Then merge.
