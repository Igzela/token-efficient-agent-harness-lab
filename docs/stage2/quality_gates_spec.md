# Quality Gates Spec

## Artifact Gate

### Purpose

Verify that task artifacts exist, match schemas, have valid evidence references, and are consistent with completion/handoff records.

### Data Structures

```python
@dataclass(frozen=True)
class ArtifactCheck:
    name: str
    passed: bool
    message: str

@dataclass(frozen=True)
class ArtifactGateResult:
    ok: bool
    checks: tuple[ArtifactCheck, ...]
    missing_artifacts: tuple[str, ...]
    schema_violations: tuple[str, ...]
    forbidden_violations: tuple[str, ...]
```

### API

```python
class ArtifactGate:
    def evaluate(
        bundle: TaskRecordBundle,
        allowed_files: tuple[str, ...] | None = None,
        forbidden_files: tuple[str, ...] | None = None,
    ) -> ArtifactGateResult
```

### Checks

1. Artifact existence: every artifact_ref in completion.json points to existing file
2. Schema: completion.json and handoff_pack.json pass validators
3. Evidence refs: every entry in handoff_pack.evidence_refs has valid path
4. Allowed/forbidden: no artifact path violates allowed_files or forbidden_files
5. Consistency: completion.handoff_pack_ref points to existing file

## Quality Gate Manager

### Purpose

Evaluate whether a task item should pass, retry, fail terminally, or require human review.

### Data Structures

```python
@dataclass(frozen=True)
class QualityGateDecision:
    result: str          # pass | pass_with_notes | fail_retryable | fail_terminal | requires_human_review
    reasons: tuple[str, ...]
    score: TaskScore | None
    artifact_result: ArtifactGateResult | None
    trajectory_result: TrajectoryReport | None
    next_project_status: str
```

### API

```python
class QualityGateManager:
    def evaluate(
        bundle: TaskRecordBundle,
        final_gate: FinalGateDecision,
        artifact_result: ArtifactGateResult,
        trajectory_report: TrajectoryReport | None = None,
        task_score: TaskScore | None = None,
    ) -> QualityGateDecision
```

### Decision Rules

| Condition | Result |
|-----------|--------|
| Final Gate pass + no blocking artifact failure + score >= 0.75 | pass |
| Final Gate pass + minor warnings + score >= 0.60 | pass_with_notes |
| Final Gate fail + score >= 0.40 + retry_count < 3 | fail_retryable |
| Final Gate fail + score < 0.40 OR retry_count >= 3 | fail_terminal |
| Pending approval or unresolved human-review anomaly | requires_human_review |

### Status Mapping

| Result | Next Status |
|--------|-------------|
| pass | done |
| pass_with_notes | done |
| fail_retryable | ready |
| fail_terminal | failed |
| requires_human_review | blocked |

### Invariants

- requires_human_review never auto-transitions to done
- approval_request.decision=pending must not execute approval action
- missing critical input returns fail_terminal, not exception
