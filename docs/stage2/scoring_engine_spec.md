# Scoring Engine Spec

## Purpose

Produce deterministic, rule-based quality scores for task bundles, artifacts, and runs.

## Data Structures

### ScoreComponent

```python
@dataclass(frozen=True)
class ScoreComponent:
    name: str
    weight: float        # 0.0 - 1.0
    raw_score: float     # 0.0 - 1.0
    weighted_score: float  # weight * raw_score
    penalties: tuple[str, ...]
```

### ArtifactScore

```python
@dataclass(frozen=True)
class ArtifactScore:
    artifact_id: str
    existence_ok: bool
    schema_ok: bool
    evidence_refs_ok: bool
    score: float          # 0.0 - 1.0
    penalties: tuple[str, ...]
```

### TaskScore

```python
@dataclass(frozen=True)
class TaskScore:
    task_id: str
    completion_score: float
    handoff_score: float
    artifact_score: float
    run_log_score: float
    failure_code_penalty: float
    weighted_score: float  # 0.0 - 1.0
    grade: str             # A/B/C/D/F
    penalties: tuple[str, ...]
```

### RunScore

```python
@dataclass(frozen=True)
class RunScore:
    run_id: str
    task_scores: tuple[TaskScore, ...]
    aggregate_score: float
    grade: str
    item_count: int
    passed_count: int
    failed_count: int
```

## APIs

```python
class ScoringEngine:
    def score_task_bundle(bundle: TaskRecordBundle, decision: FinalGateDecision) -> TaskScore
    def score_artifact(artifact_ref: dict, bundle: TaskRecordBundle) -> ArtifactScore
    def score_run(task_scores: tuple[TaskScore, ...]) -> RunScore
```

## Scoring Rules

### Per-Task Score

| Component | Weight | Rule |
|-----------|--------|------|
| completion_score | 0.25 | 1.0 if status=completed and exit_code=0; 0.0 otherwise |
| handoff_score | 0.20 | 1.0 if all required fields present and valid; penalty per missing field |
| artifact_score | 0.25 | Average of per-artifact scores |
| run_log_score | 0.10 | 1.0 if present; 0.5 if present but short; 0.0 if missing |
| failure_code_penalty | -0.20 | -0.20 if any canonical failure code present; 0.0 otherwise |

### Per-Artifact Score

| Check | Weight | Rule |
|-------|--------|------|
| existence | 0.40 | File exists at referenced path |
| schema | 0.30 | Passes schema validator |
| evidence_refs | 0.30 | All evidence refs have valid paths |

### Per-Run Score

Aggregate: mean(task_scores.weighted_score) across all tasks.

### Grade Mapping

| Score Range | Grade |
|-------------|-------|
| 0.90 - 1.00 | A |
| 0.75 - 0.89 | B |
| 0.60 - 0.74 | C |
| 0.40 - 0.59 | D |
| 0.00 - 0.39 | F |

## Constraints

- Deterministic only
- No model judge
- No embeddings
- No external calls
- Missing data creates penalties, not crashes
- Score range clamped to 0.0-1.0
