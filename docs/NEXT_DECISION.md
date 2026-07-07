# Next Decision

Last updated: 2026-07-07.

## Current Direction

Use the #154 local stateful-vs-stateless token-efficiency runner as the working proof path.

The comparison remains:

```text
stateless_reread -> growing history context
stateful_store   -> compact current context from retained state
```

Both modes use the same deterministic task, iteration budget, pass rule, quality method, and scorecard comparison path.

## Current Baseline

| Area | Status |
|---|---|
| Core dispatch kernel | Complete |
| V2 Real Production Output | Complete |
| Adaptive Fusion | Complete through AF-7 |
| Agent Runtime | Complete and sealed at AR-6 |
| Trusted Local Autonomous Execution | Complete through IAE-3 |
| Native token-efficiency scorecards | Implemented |
| External runtime comparison | Implemented as bounded import and comparison |
| Native deterministic stateful pilot | Implemented |
| Local stateful-vs-stateless runner | Implemented in #154 |

## Decision

Treat the #154 runner as the first local runner foundation. Further work should be separate, small, testable PRs.

Preferred next paths:

1. Keep runbook examples current.
2. Decide whether to connect the runner to workflow, storage, and operator evidence.
3. Keep scorecard output compatible with `token_efficiency_scorecard.v1`.

## Minimum Verification

For docs-only updates:

```bash
uv run --no-project python scripts/check_agent_handoff.py
git diff --check
```

For runner changes:

```bash
python -m py_compile scripts/provider_gated_real_runner.py tools/test_provider_gated_real_runner.py
uv run --no-project python -m unittest tools.test_provider_gated_real_runner
uv run --no-project python scripts/check_agent_handoff.py
bash scripts/verify_rust_typescript_stack.sh
git diff --check
```
