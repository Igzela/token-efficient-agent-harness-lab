# Test Matrix

## Summary

The legacy Python reference test suite (2089 tests) has been retired along with `src/harness_core/`. The Rust engine is now the sole runtime implementation with comprehensive test coverage.

## Primary Test Commands

```bash
# Rust engine (primary)
cargo test -p engine

# Python SDK
cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests

# TypeScript SDK
cd sdk/typescript && bun run test

# Full cutover verification
bash scripts/verify_rust_typescript_stack.sh
```

Current result: 1158 Rust tests pass. Python SDK and TypeScript SDK tests run separately.

## Rust Engine Test Coverage

The Rust engine (`engine/`) has 1158 tests covering:

- **Dispatch kernel parity** — golden fixture parity with frozen wire contracts
- **Routing and orchestration** — schemas, history store, cost-of-pass router, promotion gate, feedback integrator, dynamic tier selector, agent roles, task decomposer, dependency resolver, work queue, workflow engine, conflict resolver, result aggregator, human approval gate, multi-agent budget
- **Infrastructure** — observability, auth, rate limiter, plugin system, plugin registry
- **Ecosystem** — community profiles, tool adapter, dashboard, benchmark
- **Storage** — durable store (SQLite), health checker, backup manager, local product store
- **SDK/migrator** — SDK helpers, storage migrator
- **HTTP server** — health, readiness, OpenAPI, dispatch, team/key CRUD, cost summary, dispatch detail, backup management, dashboard serving, auth middleware, scope checks, rate limiting
- **Provider stack** — config, credential, audit, redaction, transport, openai, anthropic, stub, executor, retry/fallback manager, cost gate
- **CLI executor** — Claude Code CLI, Codex CLI, multi-executor routing, complexity-based escalation

## Python SDK Test Coverage

The Python SDK (`sdk/python/`) has tests covering:

- Client construction and base URL handling
- All REST endpoint methods (health, dashboard, dispatch, history, config, team, costs, export, audit, backup, keys, storage integrity, import, restore)

## Utility Test Coverage

- `tools/test_security_baseline.py` — Tests for the CA-7 security baseline checker (secret scan, import scan, active routing guard, governance boundary guard, stage-0 event guard)
- `tools/test_dashboard_static.py` — Static dashboard boundary checks for `web/dashboard/`

## Governance Fixtures

`tests/fixtures/governance/` is preserved for the security baseline checker:

| Fixture | Purpose |
| --- | --- |
| `valid_all_gates_pass.json` | All governance gates pass |
| `gate_scope_fail.json` | Scope gate failure case |
| `gate_approval_fail.json` | Approval gate failure case |
| `gate_rollback_fail.json` | Rollback gate failure case |
| `gate_unknown_error_fail.json` | Unknown error gate failure case |
