# Evaluation Runner Spec

## Evaluation Runner

### Purpose

Run controlled evaluation over known fixtures and produce comparable results.

### Data Structures

```python
@dataclass(frozen=True)
class EvalSpec:
    case_id: str
    fixture_path: Path
    expected_outcome: str       # pass | fail | no_op
    task_dir: Path | None
    item_id: str | None
    description: str

@dataclass(frozen=True)
class EvalCase:
    case_id: str
    fixture_path: Path
    expected_outcome: str
    actual_outcome: str
    passed: bool
    score: TaskScore | None

@dataclass(frozen=True)
class EvaluationReport:
    suite_id: str
    cases: tuple[EvalCase, ...]
    total: int
    passed: int
    failed: int
    score: RunScore | None
```

### APIs

```python
class EvaluationRunner:
    def run_single(case: EvalSpec) -> EvalCase
    def run_suite(cases: tuple[EvalSpec, ...]) -> EvaluationReport
```

### Evaluation Targets

1. Sanitized Stage 0 fixture: validates, produces correct projections
2. Known bad fixture (line 17): validation fails
3. Task record bundles: pass/fail Final Gate as expected
4. Orchestrator full flow: ready -> review -> done
5. Edge cases: empty log, missing files, invalid JSON

### Rules

- Each case isolated
- One failing case does not abort suite
- No model calls
- No task execution
- Use temp fixtures only
- Do not mutate docs/stage0

## Baseline Run Manager

### Purpose

Store evaluation results as baselines and compare future runs against them.

### Data Structures

```python
@dataclass(frozen=True)
class BaselineRecord:
    baseline_id: str
    timestamp: str
    run_score: RunScore
    evaluation_report: EvaluationReport
    metadata: dict[str, Any]

@dataclass(frozen=True)
class BaselineComparison:
    baseline_id: str
    current_run_score: RunScore
    score_delta: float
    regression_detected: bool
    improved_cases: tuple[str, ...]
    regressed_cases: tuple[str, ...]
```

### APIs

```python
class BaselineManager:
    def __init__(self, baseline_dir: Path)
    def save_baseline(report: EvaluationReport, score: RunScore) -> BaselineRecord
    def load_latest_baseline() -> BaselineRecord | None
    def compare(current: EvaluationReport, current_score: RunScore) -> BaselineComparison
```

### Storage

JSON files in baseline_dir. One file per baseline. No database.

### Rules

- No routing optimization
- Comparison is observational only
- Baseline save/load deterministic enough for tests
