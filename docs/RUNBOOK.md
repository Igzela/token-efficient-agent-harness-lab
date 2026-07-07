# Agent Control Plane — Runbook

Operator procedures for the local Agent Control Plane.

Last updated: 2026-07-07.

This runbook keeps current local operator procedures. Use code and tests as the source of implementation detail.

## 1. Toolchain Check

```bash
uv run --no-project python scripts/acp_local_doctor.py
```

Expected: required local tools and storage checks pass.

## 2. Local Token-Efficiency Runner

Use this path to run the #154 local stateful-vs-stateless comparison. It is script-level and deterministic by default.

### Write scorecards and comparison

```bash
uv run --no-project python scripts/provider_gated_real_runner.py \
  --output-dir /tmp/acp-local-runner \
  --iterations 10
```

Expected files:

```text
/tmp/acp-local-runner/stateless_reread.scorecard.json
/tmp/acp-local-runner/stateful_store.scorecard.json
/tmp/acp-local-runner/comparison.json
```

### Print only the comparison

```bash
uv run --no-project python scripts/provider_gated_real_runner.py \
  --compare \
  --iterations 10
```

Expected acceptance:

- both scorecards validate as `token_efficiency_scorecard.v1`;
- both modes use the same scenario id;
- `stateless_reread` is the baseline row;
- `stateful_store` has a positive token-reduction ratio in the deterministic task;
- output remains bounded comparison evidence, not runtime storage mutation.

### Focused validation

```bash
python -m py_compile scripts/provider_gated_real_runner.py tools/test_provider_gated_real_runner.py
uv run --no-project python -m unittest tools.test_provider_gated_real_runner
```

Full PR validation should still use the normal CI suite.

## 3. Token-Efficiency Scorecards

Native workflow runs emit read-only scorecard evidence at terminal states. Manual scorecard projection remains:

```bash
uv run --no-project python scripts/native_scorecard_export.py native-summary.json --output token-scorecard-artifact.json
uv run --no-project python scripts/native_scorecard_export.py native-summary.json --scorecard-only
```

The local runner uses the same scorecard validator and comparison helper rather than adding a second evidence format.

## 4. Local Engine Path

Build the dashboard:

```bash
cd dashboard && bun install --frozen-lockfile && bun run build:static && cd ..
```

Start default local mode:

```bash
ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine
```

Health check:

```bash
curl http://127.0.0.1:8080/api/v1/health
```

Expected: JSON with `"status":"ok"`.

## 5. Operational Checks

```bash
uv run --no-project python scripts/acp_ops_check.py --token $ACP_ADMIN_API_KEY
curl -s http://127.0.0.1:8080/api/v1/metrics
```

Use dashboard and API readouts for queue state, executor status, recent decisions, storage integrity, and recent audit actions before changing configuration.
