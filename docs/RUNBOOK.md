# Agent Control Plane — Runbook

Operator procedures for the local Agent Control Plane.

Last updated: 2026-07-10.

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

Run the Rust local runner with the gated OpenAI-compatible live provider:

```bash
export ACP_ENABLE_PROVIDER_EXECUTION=1
export ACP_LOCAL_RUNNER_PROVIDER_TYPE=openai_compatible
export ACP_LOCAL_RUNNER_BASE_URL=https://api.openai.com/v1
export ACP_LOCAL_RUNNER_MODEL=gpt-4o-mini
export ACP_LOCAL_RUNNER_API_KEY_ENV=OPENAI_API_KEY
export ACP_PROVIDER_INPUT_COST_PER_1K_USD="REPLACE_WITH_PROVIDER_INPUT_RATE"
export ACP_PROVIDER_OUTPUT_COST_PER_1K_USD="REPLACE_WITH_PROVIDER_OUTPUT_RATE"
cargo run -p engine --bin local-runner-exec -- \
  --provider live \
  --iterations 10 \
  --db .agent-control-plane/local-team.db \
  --output-dir /tmp/acp-local-runner-live
```

`ACP_LOCAL_RUNNER_API_KEY_ENV` is a symbolic environment-variable reference, not the secret value; the referenced variable must already be present in the operator environment. Pricing must be the provider's positive USD cost per 1,000 tokens. Do not put provider credentials in command lines, docs, scorecards, logs, or artifacts. Live mode writes bounded redacted request/response/error and cost evidence to the app-owned database, reserves worst-case call cost before invocation, and fails closed unless gates, metadata, credential reference, pricing, budget, timeout, and audit storage are ready. Set `ACP_LOCAL_RUNNER_KILL_SWITCH=1` to block new live calls immediately.

Focused validation:

```bash
python -m py_compile scripts/provider_gated_real_runner.py tools/test_provider_gated_real_runner.py
uv run --no-project python -m unittest tools.test_provider_gated_real_runner
```

Validate the local runner and export app-owned scorecard artifacts:

```bash
uv run --no-project python scripts/validate_local_runner.py \
  --output-dir /tmp/acp-local-runner \
  --artifact-dir /tmp/acp-local-runner-artifacts \
  --iterations 10 \
  --keep-output
```

Expected artifact files:

```text
/tmp/acp-local-runner-artifacts/stateless_reread.artifact.json
/tmp/acp-local-runner-artifacts/stateful_store.artifact.json
```

Import exported `native_scorecard_artifact.v1` files into the local app-owned store:

```bash
cargo run -p engine --bin import-native-scorecard-artifacts -- \
  --db "${ACP_DB_PATH:-.agent-control-plane/local-team.db}" \
  /tmp/acp-local-runner-artifacts
```

The import is idempotent. It records through `LocalProductStore`, keeps artifacts read-only and metadata-only, and rejects raw prompt/output/transcript-shaped fields or secret-shaped values.

Check scorecard API reads after the engine is running:

```bash
curl -fsS "http://127.0.0.1:8080/api/v1/scorecards?run_id=real-runner-stateful_store"
curl -fsS "http://127.0.0.1:8080/api/v1/operator/evidence/real-runner-stateful_store"
```

If `ACP_REQUIRE_AUTH=1`, include the local admin bearer token:

```bash
curl -fsS -H "Authorization: Bearer ${ACP_ADMIN_API_KEY}" \
  "http://127.0.0.1:8080/api/v1/scorecards?run_id=real-runner-stateful_store"
curl -fsS -H "Authorization: Bearer ${ACP_ADMIN_API_KEY}" \
  "http://127.0.0.1:8080/api/v1/operator/evidence/real-runner-stateful_store"
```

Acceptance:

- both scorecards validate as `token_efficiency_scorecard.v1`;
- both modes use the same scenario id;
- `stateless_reread` is the baseline row;
- `stateful_store` has a positive token-reduction ratio on the deterministic task;
- the runner emits bounded comparison files and does not mutate runtime storage.
- exported artifacts import into `LocalProductStore` as read-only `native_scorecard_artifact.v1` rows;
- `GET /api/v1/scorecards?run_id=...` and `GET /api/v1/scorecards/{artifact_id}` expose app-owned artifacts;
- `GET /api/v1/operator/evidence/:run_id` exposes bounded scorecard metadata and derived metrics, not `steps`, raw traces, prompts, outputs, transcripts, or unbounded content.

## Local Engine Reminder

For normal local engine operation, use the existing dashboard build, engine start, health check, metrics, backup, restore, release, and incident-triage scripts in `scripts/` and the CI workflow as the source of truth.

## Release Upgrade and Rollback

From an extracted release directory, upgrade a user-local installation atomically:

```bash
./upgrade.sh \
  --prefix "$HOME/.local" \
  --data-dir "$HOME/.agent-control-plane"
```

The default upgrade replaces the binary and dashboard directory atomically, retains the prior binary as `agent-control-plane.bak`, removes stale dashboard assets, and does not guess how the service is managed. Restart it with the operator's process manager after success.

For a managed service, provide paired explicit hooks:

```bash
./upgrade.sh \
  --prefix "$HOME/.local" \
  --data-dir "$HOME/.agent-control-plane" \
  --stop-command '<process-manager stop command>' \
  --restart-command '<process-manager start command>'
```

Both hooks are required together. If binary/dashboard replacement or restart fails, the script restores the prior binary and dashboard and attempts the old restart hook. Validate the packaged upgrade contract with `bash scripts/check_release_contract.sh` before publishing a release.
