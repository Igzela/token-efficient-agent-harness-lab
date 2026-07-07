# Agent Control Plane — Runbook

Operator procedures for the local Agent Control Plane.

Last updated: 2026-07-07.

## Local Token-Efficiency Runner

Run the #154 local stateful-vs-stateless comparison:

```bash
uv run --no-project python scripts/provider_gated_real_runner.py --output-dir /tmp/acp-local-runner --iterations 10
```

Expected files:

```text
/tmp/acp-local-runner/stateless_reread.scorecard.json
/tmp/acp-local-runner/stateful_store.scorecard.json
/tmp/acp-local-runner/comparison.json
```

Print only the comparison:

```bash
uv run --no-project python scripts/provider_gated_real_runner.py --compare --iterations 10
```

Focused validation:

```bash
python -m py_compile scripts/provider_gated_real_runner.py tools/test_provider_gated_real_runner.py
uv run --no-project python -m unittest tools.test_provider_gated_real_runner
```

Acceptance:

- both scorecards validate as `token_efficiency_scorecard.v1`;
- both modes use the same scenario id;
- `stateless_reread` is the baseline row;
- `stateful_store` has a positive token-reduction ratio on the deterministic task;
- the runner emits bounded comparison files and does not mutate runtime storage.

## Local Engine Reminder

For normal local engine operation, use the existing dashboard build, engine start, health check, metrics, backup, restore, release, and incident-triage scripts in `scripts/` and the CI workflow as the source of truth.
