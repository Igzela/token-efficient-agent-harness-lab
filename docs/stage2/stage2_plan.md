# Stage 2 Plan — Quality Runtime

## 1. Definition

Stage 2 adds a deterministic Quality Runtime layer on top of Stage 1. It introduces scoring, quality gating, artifact verification, trajectory monitoring, controlled evaluation, baseline comparison, and quality digest generation. All behavior is local, deterministic, and rule-based. No model calls. No real agents.

## 2. Exit Criteria

Stage 2 is complete when the system can deterministically:

1. Score a completed task bundle using rule-based scoring
2. Evaluate quality gates with pass / pass_with_notes / fail_retryable / fail_terminal / requires_human_review
3. Verify artifact existence, schema, and consistency through an Artifact Gate
4. Detect trajectory anomalies (repeated failures, loops, missing handoffs) from event streams
5. Run controlled evaluation over known fixtures and produce comparable results
6. Store and compare baseline runs
7. Generate a quality-enriched batch digest
8. Expose all of the above as library APIs and tests
9. Preserve all Stage 1 invariants and docs/stage0/events.jsonl

## 3. Components

| # | Component | Module |
|---|-----------|--------|
| 1 | Scoring Engine | `scoring.py` |
| 2 | Artifact Gate | `artifact_gate.py` |
| 3 | Quality Gate Manager | `quality_gate.py` |
| 4 | Evaluation Runner | `evaluation.py` |
| 5 | Baseline Run Manager | `baseline.py` |
| 6 | Trajectory Monitor | `trajectory.py` |
| 7 | Quality Digest Generator | `quality_digest.py` |

## 4. Implementation Sequence

- Week 1: Scoring Engine (`scoring.py`)
- Week 2: Artifact Gate (`artifact_gate.py`) + Quality Gate Manager (`quality_gate.py`)
- Week 3: Evaluation Runner (`evaluation.py`) + Baseline Manager (`baseline.py`)
- Week 4: Trajectory Monitor (`trajectory.py`) + Quality Digest (`quality_digest.py`)

## 5. Non-Goals

- No model-based judging
- No model calls
- No real agents
- No sandbox execution
- No concurrency
- No provider failover
- No routing optimizer
- No skill extractor
- No dynamic DAG mutation
- No fragment integrator
- No build sampling
- No Web UI

## 6. Stop Conditions

Stop and ask if implementation requires modifying docs/stage0/events.jsonl, installing dependencies, model calls, agent execution, sandbox execution, concurrency, provider failover, dynamic DAG mutation, routing optimizer, skill extractor, broad rewrites of Stage 1 modules, event schema material changes, or behavior that constitutes Stage 3/4.
