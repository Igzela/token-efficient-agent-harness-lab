# Routing, Sampling, and Skill Extraction Spec

## Routing Experiment Manager

### Purpose

Run controlled experiments comparing different routing policies (model tier selections) against the same task set. Observational only — does not auto-apply routing changes.

### Data Structures

```python
@dataclass(frozen=True)
class RoutingPolicy:
    policy_id: str
    tier_map: dict[str, str]  # task_type -> model_tier
    description: str

@dataclass(frozen=True)
class RoutingExperimentSpec:
    experiment_id: str
    policies: tuple[RoutingPolicy, ...]
    eval_cases: tuple[EvalSpec, ...]
    description: str

@dataclass(frozen=True)
class RoutingExperimentResult:
    policy_id: str
    run_score: RunScore
    eval_report: EvaluationReport

@dataclass(frozen=True)
class RoutingExperimentReport:
    experiment_id: str
    results: tuple[RoutingExperimentResult, ...]
    best_policy_id: str
    score_delta: float
    recommendation: str  # "adopt" | "no_change" | "needs_more_data"
```

### API

```python
class RoutingExperimentManager:
    def __init__(self, gateway: ModelGateway, evaluator: EvaluationRunner, scoring: ScoringEngine)
    def run_experiment(spec: RoutingExperimentSpec) -> RoutingExperimentReport
```

### Rules

- Observational only — never modifies active routing policy
- No production routing changes
- Uses Stage 2 EvaluationRunner / ScoringEngine
- One failing policy does not abort experiment
- Recommendation does not auto-apply anything
- Recommendation thresholds: "adopt" if score_delta > 0.10, "needs_more_data" if < 3 cases, otherwise "no_change"

---

## Sampling Runner

### Purpose

Run N deterministic variants of a task execution and compare using Stage 2 Scoring Engine to select the best candidate.

### Data Structures

```python
@dataclass(frozen=True)
class SamplingCandidate:
    candidate_id: str
    score: TaskScore
    output: str  # model output or stub output
    tier: str

@dataclass(frozen=True)
class SamplingReport:
    task_id: str
    candidates: tuple[SamplingCandidate, ...]
    best_candidate_id: str
    best_score: float
    selection_method: str  # "highest_score" | "majority_vote"
```

### API

```python
class SamplingRunner:
    def __init__(self, gateway: ModelGateway, scoring: ScoringEngine)
    def run(task_spec: dict, n: int, tier: str) -> SamplingReport
```

### Rules

- Uses ModelGateway stub
- N deterministic variants (seed derived from candidate index)
- No real model calls
- No randomness unless seeded deterministically
- Compares using Stage 2 scoring
- Selection method: highest_score (majority_vote optional)
- One failing sample does not abort run
- Invalid n (<= 0) raises ValueError

---

## Skill Extractor

### Purpose

Extract reusable skills from run logs, retrospectives, and advisor records. Store for future retrieval.

### Data Structures

```python
@dataclass(frozen=True)
class SkillRecord:
    skill_id: str
    source_task_id: str
    skill_type: str  # fix_pattern | approach | config_template | test_pattern
    title: str
    description: str
    applicable_when: str
    evidence_refs: tuple[str, ...]
    confidence: float
    extracted_from: str  # "run_log" | "retrospective" | "advisor"

@dataclass(frozen=True)
class SkillLibrary:
    skills: tuple[SkillRecord, ...]
```

### APIs

```python
class SkillExtractor:
    def extract_from_bundle(bundle: TaskRecordBundle) -> tuple[SkillRecord, ...]
    def extract_from_advisor(response: AdvisorResponse, task_id: str) -> tuple[SkillRecord, ...]

class SkillStore:
    def __init__(self, store_dir: Path)
    def save(skill: SkillRecord)
    def load(skill_id: str) -> SkillRecord | None
    def list_skills() -> tuple[SkillRecord, ...]
    def search(query: str) -> tuple[SkillRecord, ...]
```

### Extraction Patterns

| Source | What to Extract |
|--------|-----------------|
| `run_log.md` | "Fixed by...", "Root cause...", "Approach..." patterns |
| `retrospective.md` | "What worked", "What didn't", "Lesson learned" sections |
| Advisor responses | `recommended_action`, `do_not_do` fields |
| Completion records | `failure_code` + resolution pattern |

### Rules

- Deterministic — no model calls
- No prompt mutation — skills are informational
- No automatic task modification
- Empty/malformed input yields zero skills, not crash
- Storage is JSON files in skill directory
- skill_id deterministic from content hash

### Dependencies

- `TaskRecordBundle` from `task_records.py`
- `AdvisorResponse` from `advisor.py`
- `ScoringEngine` from `scoring.py`
- `EvaluationRunner` from `evaluation.py`
- `ModelGateway` from `model_gateway.py`
