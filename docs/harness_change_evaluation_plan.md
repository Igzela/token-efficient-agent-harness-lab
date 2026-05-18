# Harness Change Evaluation Track

Post-closeout optional track. Not Stage 5.

## Purpose

Create a deterministic evaluation harness for comparing future harness changes against fixed
fixture suites. When a code change touches scoring, validation, projection, or quality gate
logic, this track lets you run the same fixtures before and after and get a stable, comparable
snapshot.

## Constraints

- No runtime behavior modification unless explicitly approved.
- No real model calls. No sandbox execution.
- Do not modify `docs/stage0/events.jsonl`.
- Uses only existing APIs: `replay_preflight`, `replay_all`, `generate_batch_digest`,
  `ScoringEngine`, `QualityDigestGenerator`.

## Evaluation Snapshot Schema

An evaluation snapshot captures the full deterministic output of running a fixture suite
through the current harness code.

```json
{
  "snapshot_id": "string",
  "timestamp": "string (ISO 8601)",
  "harness_commit": "string (git SHA or placeholder)",
  "suite_id": "string",
  "cases": [
    {
      "case_id": "string",
      "fixture_path": "string",
      "preflight_ok": true,
      "event_count": 0,
      "projection_item_count": 0,
      "digest_completed": [],
      "digest_blocked": [],
      "digest_handoff_count": 0,
      "scoring_aggregate": 0.0,
      "scoring_grade": "string",
      "trajectory_ok": true,
      "trajectory_anomaly_count": 0,
      "quality_gate_result": "string",
      "keep_rate": null,
      "user_feedback": null
    }
  ],
  "aggregate_score": 0.0,
  "aggregate_grade": "string",
  "total_cases": 0,
  "passed_cases": 0,
  "summary": "string"
}
```

### Fields

- `snapshot_id`: Deterministic identifier (hash of suite_id + harness_commit + fixture hashes).
- `harness_commit`: Git SHA at time of snapshot. Placeholder until real tracking is added.
- `keep_rate`: Placeholder for future Git-based "keep rate" metric. Always `null` in v1.
- `user_feedback`: Placeholder for future LLM-based feedback judge. Always `null` in v1.

## Before/After Comparison Schema

Compares two snapshots to detect regressions and improvements.

```json
{
  "comparison_id": "string",
  "before_snapshot_id": "string",
  "after_snapshot_id": "string",
  "score_delta": 0.0,
  "grade_changed": false,
  "regressed_cases": [],
  "improved_cases": [],
  "unchanged_cases": [],
  "new_cases": [],
  "removed_cases": [],
  "per_case_deltas": [
    {
      "case_id": "string",
      "before_score": 0.0,
      "after_score": 0.0,
      "delta": 0.0,
      "before_grade": "string",
      "after_grade": "string",
      "status": "unchanged|improved|regressed"
    }
  ],
  "regression_detected": false,
  "summary": "string"
}
```

### Regression Detection

A case is **regressed** when its `quality_gate_result` changes from a passing state
(`pass`, `pass_with_notes`) to a failing state (`fail_retryable`, `fail_terminal`,
`requires_human_review`), or when its aggregate score drops by more than 0.05.

A case is **improved** when the opposite occurs.

## Fixture Suite Structure

```
tests/fixtures/harness_change_eval/
  good_flow/events.jsonl          # Normal happy-path event stream
  validation_failure/events.jsonl # Stream with schema violation
  trajectory_anomaly/events.jsonl # Stream with repeated failures
  README.md                       # Documents fixture purposes
```

Each fixture is a minimal JSONL event stream that exercises one path through the harness.

## Test Strategy

One test (`test_fixture_suite_produces_stable_snapshot`) runs the full fixture suite
twice against the same code and asserts that the resulting snapshots are identical.
This proves determinism.

A second test (`test_comparison_detects_no_regression`) runs the suite, creates a
baseline snapshot, then runs again and compares -- asserting no regressions when code
hasn't changed.

## Future Extensions

- Real Git history tracking for `harness_commit` and `keep_rate`.
- LLM-based feedback judge for `user_feedback`.
- CI integration to run on every PR.
- Threshold gates to block merges on regression.
