# Stage 3 Plan — Controlled Intelligence Layer

## Definition

Stage 3 introduces a **Controlled Intelligence Layer** on top of the deterministic Stage 1 runtime and the rule-based Stage 2 quality runtime. Stage 3 asks: can we safely introduce model calls and advisor intelligence while preserving determinism, auditability, and safety?

Stage 3 adds advisor protocol integration, model gateway abstraction, routing experiments, sampling, skill extraction, and a controlled model evaluation harness. The key principle is **stub-first, real-later**: every component that will eventually call a real model starts as a deterministic stub that can be swapped for a real implementation without changing the surrounding architecture.

## What Stage 3 Is Not

- Not Stage 4 (no dynamic DAG mutation, no multi-sandbox orchestration, no autonomous agent execution)
- Not production deployment (no real provider failover, no concurrency, no Web UI)
- Not uncontrolled model invocation (every model call is gated, budgeted, and auditable)
- Not automatic routing optimization (experiments are observational, not auto-applied)

## How It Differs from Stage 1 and Stage 2

| Aspect | Stage 1 | Stage 2 | Stage 3 |
|--------|---------|---------|---------|
| Core concern | Deterministic orchestration | Quality measurement | Controlled intelligence |
| Model calls | None | None | Stubbed, then controlled |
| Scoring | None | Rule-based | Rule-based + model-judge option |
| Advisor | Not present | Not present | Stubbed protocol |
| Routing | Fixed sequential | Observational baseline | Experiment framework |
| Skills | Not present | Not present | Extraction from logs |

## Exit Criteria

Stage 3 is complete when the system can deterministically:

1. Invoke a stubbed advisor for preflight, correction, arbitration, and risk scan
2. Route tasks to different model tiers via a gateway abstraction
3. Run routing experiments and compare results using Stage 2 scoring
4. Run N deterministic sampling variants and select the best
5. Extract skills from run logs, retrospectives, and advisor records
6. Validate advisor protocol events against a schema
7. Register model capabilities per tier
8. Run controlled model evaluation over known fixtures
9. Expose all of the above as library APIs and tests
10. Preserve all Stage 1 and Stage 2 invariants

## Core Components

| # | Component | Module |
|---|-----------|--------|
| 1 | Advisor Broker | `advisor.py` |
| 2 | Model Gateway Stub | `model_gateway.py` |
| 3 | Model Capability Registry | `model_gateway.py` |
| 4 | Routing Experiment Manager | `routing.py` |
| 5 | Sampling Runner | `sampling.py` |
| 6 | Skill Extractor | `skills.py` |
| 7 | Advisor Protocol Validator | `advisor.py` |
| 8 | Controlled Model Evaluation Harness | `model_eval.py` |

## Implementation Sequence

### Week 1: Advisor Protocol + Broker Stub
- `src/harness_core/advisor.py`
- `tests/test_advisor.py`

### Week 2: Model Gateway Stub + Capability Registry
- `src/harness_core/model_gateway.py`
- `tests/test_model_gateway.py`

### Week 3: Routing Experiment Manager + Controlled Evaluation
- `src/harness_core/routing.py`
- `src/harness_core/model_eval.py`
- `tests/test_routing.py`
- `tests/test_model_eval.py`

### Week 4: Sampling Runner + Skill Extractor
- `src/harness_core/sampling.py`
- `src/harness_core/skills.py`
- `tests/test_sampling.py`
- `tests/test_skills.py`

### Week 5: Integration and Final Acceptance
- Integration audit
- `docs/stage3/stage3_final_acceptance_report.md`

## Safety and Stop Conditions

Stop and ask if implementation requires:

1. Real model API keys or credentials
2. Installing dependencies
3. Sandbox execution
4. Concurrency or parallel execution
5. Provider failover
6. Dynamic DAG mutation
7. Web UI
8. Modifying `docs/stage0/events.jsonl`
9. Autonomous agent execution
10. Broad rewrites of Stage 1 or Stage 2 modules
11. Event schema material changes
12. Tests fail and cannot be fixed within Stage 3 scope
13. Any behavior that constitutes Stage 4

## Non-Goals

- No real model calls
- No real providers
- No API keys
- No real agents
- No sandbox execution
- No concurrency
- No provider failover
- No dynamic DAG mutation
- No Web UI
- No automatic routing adoption
- No automatic prompt mutation from skills
