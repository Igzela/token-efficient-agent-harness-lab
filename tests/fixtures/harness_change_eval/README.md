# Harness Change Evaluation Fixtures

Deterministic JSONL event streams for evaluating harness changes.

## Fixtures

- **good_flow/**: Normal happy-path event stream (todo -> ready -> running -> review -> done).
- **validation_failure/**: Event stream with a schema violation on line 2 (missing schema_version).
- **trajectory_anomaly/**: Event stream with repeated failures (3 failures on the same item).

## Usage

Run with: pytest tests/test_harness_change_eval.py -v
