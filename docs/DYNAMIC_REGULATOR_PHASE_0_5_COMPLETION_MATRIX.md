# Dynamic Global Regulator — Phase 0–5 Completion Matrix

Generated: 2026-06-11 | PR: #31 | Commit: 713af59

## Summary Table

| Phase | Goal | Status | Evidence | Key Gaps |
|-------|------|--------|----------|----------|
| 0 | Baseline Documentation and Observability | PARTIAL | `/api/v1/dispatch-metrics` endpoint + dashboard Metrics subsection exist; data derived from existing dispatch bundles | No structured logging infrastructure; no `docs/DISPATCH_OBSERVABILITY.md`; no per-decision log calls for tier rationale/complexity/constraints |
| 1 | ContextPack Cross-Node Assembly | DONE | `context_pack` module with `assemble_context_injection()`, budget config, validation types; ContextBridge field mapping; ContextBudgetAllocator cross-node budget distribution; tick integration; 10 acceptance tests | None — Phase 1 is complete |
| 2 | Feedback Ledger and Replayable Run Traces | DONE | RunTraceRecorder module; OutcomeAttributor module; PatternDetector module; `/api/v1/feedback/traces`, `/api/v1/feedback/cost-of-pass`, `/api/v1/feedback/patterns` endpoints; dashboard Feedback subsection; `feedback_traces` uses stable RunTrace schema; `cost_of_pass` computed from stable trace model; tests cover recorder, attribution, pattern detection, filtering, empty state | None — Phase 2 is complete |
| 3 | Shadow Adaptive Policy Simulation | DONE | Shadow routes generated at dispatch time via `build_shadow_routes()`; `/api/v1/simulation/report` endpoint; all influence flags disabled; dashboard Simulation subsection; ShadowRouter; PolicySimulator; delta metrics; `/api/v1/simulation/policy-delta` endpoint; dashboard delta display; SDK methods | None — Phase 3 is complete |
| 4 | Human-Approved Policy Proposals | DONE | Full CRUD lifecycle (create/list/approve/reject/deactivate/rollback); `confirm_policy_override` guard; `team:admin` auth; safe-tier restriction; `active_routing_policy()` integration; v12 migration; audit trail; dashboard Proposals subsection; TS+Python SDK coverage; PolicyProposer; ProposalValidator; ProposalSerializer; GET /api/v1/proposals/generated; generated candidates with evidence/confidence/safety flags; dashboard generated suggestions; SDK generatedProposals()/generated_proposals() | None — Phase 4 is complete |
| 5 | Limited Automatic Adjustment Under Strict Guards | DONE | PR #36 dry-run guard, PR #37 active core, PR #38 hardening, PR #39 playbook, and PR #40 signoff: AutoAdjustmentPolicy, AutoAdjustmentGuard, PolicySnapshotPreview/Record, `GET /api/v1/auto-adjustments`, `POST /api/v1/auto-adjustments/apply`, `POST /api/v1/auto-adjustments/{id}/rollback`, default-off/dry-run/active gates, persistent snapshots, stable candidate IDs, per-policy active guard, stale rollback checks, audit events, apply/rollback tests, controlled SQLite active trial and rollback drill signed off in `docs/PHASE5_ACTIVE_TRIAL_PLAYBOOK.md` | Runtime active operation remains opt-in and disabled by default; PostgreSQL active trial was unavailable because `ACP_TEST_DATABASE_URL` was not set; no dashboard/SDK changes, background scheduling, batch auto-apply, or daemon loop |

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

**Status:** DONE

**Implemented in #31:**
- `context_pack` module (`engine/src/workflow/context_pack/`) with `assembly.rs`, `budget.rs`, `rules.rs`, `types.rs`, `validation.rs`
- `assemble_context_injection()` function: collects predecessor outputs, applies token budget, truncates, produces `context_injection.v1` metadata
- `ContextAssemblyConfig` with `ACP_CONTEXT_ASSEMBLY_ENABLED` and `ACP_CONTEXT_ASSEMBLY_MAX_TOKENS` env vars
- `ContextSource` struct (edge_id, from_node_id, output)
- `ContextBudget` struct (max_context_tokens, preferred_context_tokens, max_response_tokens, reserved_response_tokens)
- `check_budget_compliance()` and `apply_prune_policy()` for single-pack budget enforcement
- Context layers validation (`validate_context_layers`), advisor/model context pack validation

**Implemented in PR #33:**
- ContextBridge — `bridge_context_fields()` maps output fields to context fields based on edge metadata `field_mapping`; passthrough when no mapping
- ContextBudgetAllocator — `allocate_context_budget()` distributes budget across predecessors deterministically by `from_node_id` sort order
- `assemble_context_injection_with_bridge()` — uses `allocate_context_budget()` for budget distribution, then applies ContextBridge per source
- Edge metadata `field_mapping` — `context_injection_for_node` reads `edge_json.field_mapping` from `workflow_run_edges`
- Context injection persistence — `persist_context_injection()` writes `context_injection` into `node_json` before execution, surviving Phase 3 result merge
- 11 acceptance tests + 14 unit tests

**Missing from plan:**
- None. Phase 1 is complete.

**Tests Present:**
- 25 tests total: 4 bridge unit tests (assembly.rs), 6 budget allocator unit tests (budget.rs), 4 assembly_with_bridge unit tests (assembly.rs), 11 integration tests (test_local_product_store.rs) — including `context_assembly_persisted_in_node_json_directly` which reads back node via `get_workflow_run` and asserts `node_json.context_injection` survives execution

**Tests Missing:**
- None for Phase 1 scope.

**Safety/Boundary Risks:**
- None — context assembly is now wired into tick execution with env rollback (`ACP_CONTEXT_ASSEMBLY_ENABLED=0`)

**Recommended Next PR:**
- Phase 2: RunTraceRecorder as a dedicated module capturing structured decision→execution→evaluation chains independently of dispatch bundle format

---

### Phase 2: Feedback Ledger and Replayable Run Traces

**Plan Goal:** Transform the event-sourced ledger into a queryable feedback store with outcome attribution.

**Status:** DONE

**Implemented:**
- RunTraceRecorder module — captures structured decision→execution→evaluation chains independently of dispatch bundle format
- OutcomeAttributor module — links success/failure to specific decision factors
- PatternDetector module — detects recurring failure patterns (e.g., "cheap tier fails on architecture tasks")
- `feedback_traces()` uses stable RunTrace schema with task_class/tier/status filtering
- `cost_of_pass()` computed from stable trace model with task_class/tier grouping, pass_rate, average_cost_usd
- `GET /api/v1/feedback/traces` endpoint
- `GET /api/v1/feedback/cost-of-pass` endpoint
- `GET /api/v1/feedback/patterns` endpoint
- Dashboard Feedback subsection in `DynamicRegulator.tsx`
- TS SDK: `feedbackTraces()`, `feedbackCostOfPass()`, `feedbackPatterns()`
- Python SDK: `feedback_traces()`, `feedback_cost_of_pass()`, `feedback_patterns()`

**Missing from plan:**
- None — Phase 2 is complete

**Tests Present:**
- Tests cover recorder, attribution, pattern detection, filtering, and empty state

**Tests Missing:**
- None for Phase 2 scope

**Safety/Boundary Risks:**
- None — endpoints are read-only; feedback data does not influence dispatch decisions; no mutation of active routing

**Recommended Next PR:**
- Phase 3: ShadowRouter module and PolicySimulator for replay through candidate policies with delta metrics

---

### Phase 3: Shadow Adaptive Policy Simulation

**Plan Goal:** Compare "what the regulator would do" against "what the kernel does" without affecting live traffic.

**Status:** DONE

**Implemented in #31 and #34:**
- `ShadowRoute` struct in `dispatch_decision.rs:152` with tier, profile_id, reason, admission_scope, estimated_cost, expected_tradeoff
- `build_shadow_routes()` in `dispatch_decision.rs:569` generates fallback + cheap_executor alternatives at dispatch time
- Shadow routes recorded in every dispatch bundle's `decision.shadow_routes` array
- `simulation_report()` storage method in `dispatch.rs:435` reads shadow_routes from dispatch bundles, produces `dispatch_simulation_report.v1`
- `GET /api/v1/simulation/report` endpoint in `handlers/dispatch.rs:227`
- `shadow_influence_disabled()` — all influence flags are false (selected_tier, budget_reservation, executor_selection, retry_path, decision_status, routing_mode)
- Dashboard Simulation subsection in `DynamicRegulator.tsx`
- TS SDK: `simulationReport()` with limit filter
- Python SDK: `simulation_report(limit)`
- ShadowRouter module — dedicated component computing "what the regulator would have chosen" as a standalone routing simulator
- PolicySimulator module — replays historical traces through candidate policies to measure outcome delta
- Delta metrics — `success_rate_delta`, `cost_delta`, `latency_delta`, `human_review_rate_delta` between real and shadow decisions
- `GET /api/v1/simulation/policy-delta` endpoint for before/after comparison between policy A and policy B
- Dashboard delta display in `DynamicRegulator.tsx` showing policy comparison results
- TS SDK: `policyDelta()` method
- Python SDK: `policy_delta()` method

**Missing from plan:**
- None — Phase 3 is complete

**Tests Present:**
- ShadowRouter unit tests: shadow route generation, tier assignment, influence-flag enforcement
- PolicySimulator unit tests: trace replay, delta computation, empty-state handling
- HTTP integration tests: `/api/v1/simulation/report` endpoint, `/api/v1/simulation/policy-delta` endpoint
- Safety invariant tests: shadow simulation cannot alter dispatch tier, executor type, or routing policy
- SDK tests: TS `simulationReport sends limit query param`, Python `test_simulation_report_sends_limit`

**Safety/Boundary Risks:**
- None — safety invariants tested: shadow simulation cannot alter dispatch tier, executor type, or routing policy

---

### Phase 4: Human-Approved Policy Proposals

**Plan Goal:** Generate structured policy change proposals that require human approval before activation.

**Status:** DONE

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

**Implemented in PR #35:**
- PolicyProposer in `engine/src/feedback/policy_proposer.rs` — generates `ProposalCandidate` from Phase 2 `DetectedPattern` + Phase 3 `SimulationResult`
- ProposalValidator in `engine/src/feedback/proposal_validator.rs` — validates generated candidates (safe tier, domain, intent, evidence, confidence) and manual create requests
- ProposalSerializer in `engine/src/feedback/proposal_serializer.rs` — `serialize_candidate_to_proposal_request()` and `serialize_candidate_to_api_response()` for existing schema compatibility
- `GET /api/v1/proposals/generated` — read-only endpoint (`dispatch:read` scope), returns auto-generated candidates without persisting or activating
- `generated_proposals()` store method — reads traces, detects patterns, runs simulation, calls PolicyProposer; returns candidates as API response values
- Generated candidates include evidence (pattern_ids, trace_ids, simulation deltas), confidence, risk_level, safety_flags (all safe), requires_human_approval=true
- Dashboard: generated suggestions section in `DynamicRegulator.tsx` — labeled "not active until approved"
- TS SDK: `generatedProposals()` method
- Python SDK: `generated_proposals()` method

**Tests Present:**
- 29 tests: store lifecycle test (`proposal_lifecycle_builds_active_policy`), store CLI tier rejection test, HTTP integration test (full create→approve→dispatch lifecycle with confirmation guard), integrity table check, SDK tests for CRUD and confirmation guards (TS: 5, Python: 4), PolicyProposer tests (6), ProposalValidator tests (8), ProposalSerializer tests (4), generated proposals safety proof tests (4)

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

**Status:** DONE

**Implemented in Phase 5 PRs:**
- PR #37: active apply + rollback core under strict gates.
- PR #38: safety hardening for re-entry, stale data, storage parity, HTTP safety, audit details, and boundary invariants.
- `AutoAdjustmentPolicy` in `engine/src/feedback/auto_adjustment_policy.rs` emits `auto_adjustment_policy_decision.v1`
- `AutoAdjustmentGuard` in `engine/src/feedback/auto_adjustment_guard.rs` emits `auto_adjustment_guard_decision.v1`
- `PolicySnapshotPreview` and `PolicySnapshotRecord` in `engine/src/feedback/policy_snapshot.rs`
- `controlled_loop_policy_snapshots` table for persistent rollback snapshots
- `GET /api/v1/auto-adjustments` endpoint requiring `dispatch:read`
- `POST /api/v1/auto-adjustments/apply` endpoint requiring configured auth, `team:admin`, active env gates, and `confirm_auto_adjustment=true`
- `POST /api/v1/auto-adjustments/{adjustment_id}/rollback` endpoint requiring configured auth, `team:admin`, confirmation, and snapshot hash validation
- `auto_adjustments_report()` store read model derives generated candidates, policy decisions, guard state, snapshot previews, and active adjustments
- Default-off gate: disabled unless `ACP_ENABLE_AUTO_ADJUSTMENT=1`
- Dry-run gate: read-only mode when `ACP_AUTO_ADJUSTMENT_DRY_RUN=1`
- Active gate: apply only when `ACP_AUTO_ADJUSTMENT_ACTIVE=1` and dry-run is unset
- Stable generated candidate IDs derived from candidate content rather than list order
- Active apply rejects duplicate candidate re-entry and rejects a second active auto-adjustment for the same `policy_key`
- Rollback rejects corrupted snapshots, non-active adjustment status, missing snapshots, and stale linked proposal state before mutating policy
- SQLite and PostgreSQL snapshot indexes cover status, proposal, adjustment lookup, policy key, and active-per-key enforcement
- Audit events for apply accepted/rejected, snapshot created, rollback accepted/rejected
- Audit doc: `docs/PHASE5_AUTO_ADJUSTMENT_AUDIT.md`
- Active trial playbook: `docs/PHASE5_ACTIVE_TRIAL_PLAYBOOK.md`

**Completed seal evidence:**
- PR #40 executed the controlled active trial against isolated SQLite DB `/tmp/acp-phase5-final-20260611.sqlite`.
- Disabled mode returned `active_apply_available=false` and `no_live_mutation=true`.
- Dry-run mode returned decisions and snapshot previews without creating proposals or active snapshots.
- Active apply of candidate `proposal-0ad30c37eb42` created `auto-adjustment-0001`, `policy-snapshot-0001`, and `policy-proposal-0001`.
- Re-entry apply was rejected with duplicate/active policy-key blocked reasons.
- Rollback returned `rolled_back=true` and restored the active policy state for the affected key.
- Repeated rollback returned `rolled_back=false` with non-active snapshot/proposal blocked reasons.
- Isolated corrupted-hash rollback returned `rolled_back=false` with `snapshot safety hash mismatch`, then the original hash was restored and cleanup rollback completed.
- Audit contained snapshot/apply/rollback accepted and rejected events.
- Final SQLite counts were `active_proposals=0`, `active_snapshots=0`, `rolled_back_snapshots=2`, and `total_snapshots=2`.
- PostgreSQL active trial execution was unavailable because `ACP_TEST_DATABASE_URL` was not set.

**Future work outside Phase 5 core:**
- Dashboard Auto-Adjustments tab with timeline and rollback controls.
- Auto-rollback trigger on success-rate degradation, requiring explicit approval because it implies background monitoring behavior.
- TS/Python SDK methods and dashboard surface, deferred because dashboard/TS changes were out of approved PR #40 scope.

**Tests Present:**
- AutoAdjustmentPolicy unit tests: accepts only high-confidence safe generated candidates; rejects unsafe CLI tiers, missing evidence, weak confidence, missing simulation, simulation regression, and failed safety flags
- AutoAdjustmentGuard unit tests: disabled by default; dry-run requires env gate plus dry-run env; active requires enable + active gates and is blocked by dry-run
- PolicySnapshotPreview/PolicySnapshotRecord unit tests: deterministic read-only preview and tamper-detecting hash
- HTTP integration test: `GET /api/v1/auto-adjustments` disabled mode, dry-run mode, decisions/snapshot previews, no proposal row creation, no active policy mutation
- HTTP integration test: apply env gates, confirmation, active apply, active adjustment listing, and audit events
- HTTP integration test: admin scope required
- HTTP integration test: rollback confirmation, corrupted hash rejection, exact prior policy restore, repeated rollback block, and audit events
- HTTP integration test: duplicate apply and same-policy active re-entry are blocked without mutating active policy; different policy key remains allowed
- HTTP integration test: rollback requires `team:admin`, missing snapshots are rejected, and stale linked proposal state is blocked without changing active policy
- HTTP integration test: unsafe CLI/provider tier proposal creation is rejected
- Storage test: SQLite migration v13/integrity includes `controlled_loop_policy_snapshots`; SQLite and PostgreSQL DDL include required snapshot indexes

**Safety/Boundary Risks:**
- Default and dry-run modes cannot mutate policy.
- Active apply is limited to one safe tier-map candidate per request.
- Active apply allows only one active auto-adjustment per `policy_key`.
- Snapshot persistence is a narrow DB schema addition supporting SQLite and PostgreSQL DDL.
- No provider/CLI/auth/security/deploy boundary expansion, target repo write, release/tag/deploy, background scheduling, batch apply, or dashboard one-click enable was added.

**Recommended Next PR:**
- Decide whether to begin Phase 6 design or prioritize post-Phase-5 production monitoring/observability. Keep runtime active operation opt-in and disabled by default. Do not auto-merge high-risk policy mutation changes.

---

## Phase 5 Gap Analysis

| Component | Exists? | Evidence |
|-----------|---------|----------|
| AutoAdjustmentPolicy (whitelist of adjustment types) | Yes | `engine/src/feedback/auto_adjustment_policy.rs` |
| AutoAdjustmentGuard (magnitude limit, rate limit, rollback trigger) | Yes | Default-off, dry-run, active two-gate mode, one adjustment per request, rollback endpoint |
| Whitelist of allowed adjustment types | Yes | Policy requires generated candidate shape, safe override tier, `tier_map_override`, evidence, simulation, safety flags, and strict confidence |
| Magnitude caps on adjustments | Yes | Apply handles exactly one generated candidate per request |
| Rate limits on adjustments | Partial | One adjustment per request; no background scheduler or batch apply; external API rate limits still apply |
| PolicySnapshot (before/after state capture) | Yes | `PolicySnapshotPreview` + persistent `PolicySnapshotRecord` in `controlled_loop_policy_snapshots` |
| Auto-rollback on degradation | No | Requires future explicit approval; no background scheduler was added |
| `/api/v1/auto-adjustments` endpoint | Yes | GET report route plus POST apply/rollback routes registered in `routes.rs` |
| Dashboard Auto-Adjustments tab with timeline | No | No component in dashboard |
| Human manual revert controls | Yes | Confirmed rollback endpoint restores prior policy for affected key |
| `ACP_ENABLE_AUTO_ADJUSTMENT=0` default gate | Yes | Guard is disabled unless `ACP_ENABLE_AUTO_ADJUSTMENT=1` |
| Boundary mutation tests (no safety/auth/provider/CLI/hard-constraint mutation) | Yes | Dry-run no-mutation tests plus active apply/rollback tests for gates, admin auth, confirmation, hash validation, exact restore, unsafe tier rejection, safety flags, and stale-state blocks |
| Real-world active trial playbook | Yes | `docs/PHASE5_ACTIVE_TRIAL_PLAYBOOK.md` |
| Real-world active trial signoff | Yes | PR #40 signoff in `docs/PHASE5_ACTIVE_TRIAL_PLAYBOOK.md`; SQLite PASS, PostgreSQL unavailable |

## Overclaims Fixed

### 1. CURRENT_STATUS.md — "Phase 4 controlled loop"

**Original:** "Dynamic Global Regulator MVP + Phase 4 controlled loop pilot"

**Corrected:** "Dynamic Global Regulator MVP + Phase 4 proposal lifecycle CRUD (manual creation via API; PolicyProposer auto-generation not implemented)"

**Reason:** The term "controlled loop" implies the feedback-to-proposal loop is closed (PolicyProposer auto-generates proposals from feedback analysis). In reality, the three core Phase 4 components — PolicyProposer, ProposalValidator, ProposalSerializer — are absent. Proposals are created manually via API. This is a CRUD lifecycle, not a closed loop.

### 2. CURRENT_STATUS.md — "derived regulator read models"

**Original:** "Dispatch history now exposes derived regulator read models for outcome metrics, feedback traces, cost-of-pass aggregates, and shadow simulation reports."

**Corrected:** "Dispatch history now exposes read-only API endpoints that derive metrics, traces, cost-of-pass, and shadow route data from existing dispatch bundles. Phase 0 structured logging, Phase 2 RunTraceRecorder, and Phase 3 ShadowRouter/PolicySimulator are not implemented. Phase 1 (ContextBridge, ContextBudgetAllocator, tick integration) is DONE."

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

**Corrected:** "Validate Dynamic Global Regulator read-model endpoints and proposal CRUD lifecycle through real-world pilot tasks, CI, and targeted hardening. Phase 1 (ContextBridge, ContextBudgetAllocator) is DONE. Core Phase 2–4 components (RunTraceRecorder, ShadowRouter, PolicySimulator, PolicyProposer) remain unimplemented."

**Reason:** Framing validation of "Phase 4 controlled loop" when the loop is not closed sets incorrect expectations for what validation means.

### 6. NEXT_DECISION.md — Missing component inventory

**Original:** Allowed paths mention "dynamic regulator hardening: focused tests and CI fixes for metrics/traces/cost/simulation/proposal paths"

**Corrected:** Should enumerate which plan components are missing: RunTraceRecorder, ShadowRouter, PolicySimulator, PolicyProposer, ProposalValidator, ProposalSerializer. ContextBridge and ContextBudgetAllocator are DONE (PR #33). The current phrasing implies the paths exist and need hardening, not that core components are absent.

## Recommended PR Sequence

### 1. Current PR (#31) — Description Correction

**What it is:** Read-model endpoints (`/api/v1/dispatch-metrics`, `/api/v1/feedback/traces`, `/api/v1/feedback/cost-of-pass`, `/api/v1/simulation/report`) derived from existing dispatch bundles, plus a policy proposal CRUD lifecycle with safety gates (`confirm_policy_override`, `team:admin`, safe-tier restriction, `active_routing_policy()` dispatch integration).

**What it is NOT:** A "Phase 4 controlled loop" or completion of Phases 0–4. The feedback-to-proposal loop is not closed. RunTraceRecorder, ShadowRouter, PolicySimulator, PolicyProposer, ProposalValidator, and ProposalSerializer are absent. ContextBridge and ContextBudgetAllocator are DONE (PR #33).

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

### 3. PR #33 — Phase 1 Context Assembly Wiring (DONE)

**Scope:** ContextBridge field mapping, ContextBudgetAllocator cross-node distribution, `assemble_context_injection_with_bridge()`, edge metadata `field_mapping`, tick integration, 10 acceptance tests. Phase 1 is complete.

### 4. PR #34 — Phase 2 RunTraceRecorder + OutcomeAttributor + PatternDetector (DONE)

**Scope:** RunTraceRecorder, OutcomeAttributor, PatternDetector modules; `/api/v1/feedback/patterns` endpoint; stable RunTrace schema for `feedback_traces`; cost-of-pass from stable trace model; tests for recorder, attribution, pattern detection, filtering, empty state. Phase 2 is complete.

### 5. PR #34 + Follow-up — Phase 3 ShadowRouter + PolicySimulator (DONE)

**Scope:** ShadowRouter computes "what the regulator would have chosen." PolicySimulator replays traces through candidate policies and computes delta metrics (success_rate_delta, cost_delta, latency_delta, human_review_rate_delta). `/api/v1/simulation/policy-delta` surfaces the report.

### 6. PR #35 — Phase 4 PolicyProposer (DONE)

**Scope:** PolicyProposer auto-generates proposals from feedback patterns and simulation results. ProposalValidator checks safety constraints. ProposalSerializer produces generated proposal responses with evidence and safety flags. Generated proposals remain read-only until human-approved through the existing proposal lifecycle.

### 7. Phase 5 Auto-Adjustment Guard (DONE)

**Scope:** Implement AutoAdjustmentPolicy, AutoAdjustmentGuard, PolicySnapshotPreview/Record, persistent snapshots, `GET /api/v1/auto-adjustments`, `POST /api/v1/auto-adjustments/apply`, rollback endpoint, default-off/dry-run/active gates, audit events, safety tests, PR #38 active-core hardening, PR #39 active trial playbook, and PR #40 controlled active apply/rollback signoff. Active operation remains high-risk, opt-in, disabled by default, and not auto-merge eligible.

## Merge Decision

**PR #31 is NOT auto-merge eligible. It requires human review.**

**Reasons:**

1. **Status document overclaims:** CURRENT_STATUS.md and NEXT_DECISION.md describe the implementation as "Phase 4 controlled loop" when the feedback-to-proposal loop is not closed. Three of Phase 4's core components (PolicyProposer, ProposalValidator, ProposalSerializer) are absent. Phases 0–3 are described as complete when dedicated components specified in the plan (RunTraceRecorder, ShadowRouter, PolicySimulator, ContextBridge, ContextBudgetAllocator) do not exist. The status documents must be corrected before merge to prevent incorrect expectations.

2. **Critical safety gate test gaps:** CLI tier override rejection is only tested at the store level, not through the HTTP endpoint. `team:admin` enforcement has no negative test (missing scope returns 403). `confirm_policy_override` is only tested for approve at HTTP level, not for rollback/deactivate. These are safety-critical boundaries that should have HTTP-level regression tests before the feature is exposed.

3. **Phase 1 is now complete:** The `context_pack` module is wired into `DynamicWorkflowController::tick()` via ContextBridge and ContextBudgetAllocator (PR #33). 12 tests cover the full scope.

4. **No Rust integration tests for new endpoints:** Four new HTTP endpoints (`/api/v1/dispatch-metrics`, `/api/v1/feedback/traces`, `/api/v1/feedback/cost-of-pass`, `/api/v1/simulation/report`) have zero Rust integration tests. Only SDK URL-construction mocks exist.

**Recommended action:** Correct status documents to accurately describe what is implemented (read-model endpoints + proposal CRUD lifecycle + Phase 1 context assembly) vs what remains (Phases 0, 2–4 core components + Phase 5). Add HTTP-level safety gate tests. Then merge.
