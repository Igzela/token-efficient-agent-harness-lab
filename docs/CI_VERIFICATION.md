# CI Verification

## Purpose

This document describes the GitHub Actions CI pipeline that verifies the security baseline and test suite on every push and pull request to `main`.

## What CI Verifies

- **Security baseline checker** (`tools/check_security_baseline.py`): five-part gate covering secret scanning, AST import analysis, active routing guard, governance boundary guard, and stage-0 event guard.
- **Rust engine tests** (`cargo test -p engine`): 1158 parity/component/API test cases, including local small-team state/API coverage and provider audit/usage bridge coverage.
- **TypeScript SDK/dashboard checks**: SDK tests/build plus dashboard lint/typecheck/build.
- **Native runtime smoke**: static dashboard export plus Rust engine binary smoke without Docker, using a temporary SQLite database and verifying live dashboard/export state.
- **Optional Docker build**: local compose images for API and dashboard.

## What CI Does Not Verify

- No real provider calls in CI — network-capable provider adapters are explicit env-gated beta paths and are exercised with stub/mock transports in automated verification.
- No secret or API-key-dependent flows.
- No integration tests against live infrastructure.
- CI is **not** production certification.

## Local Commands

```bash
# Security baseline checker
uv run --no-project python tools/check_security_baseline.py

# Rust engine
cargo test -p engine

# Dashboard (bun manages Node dependencies)
cd dashboard && bun run lint && bun run typecheck && bun run build && bun run build:static

# SDKs
cd sdk/typescript && bun run build && bun run test
cd sdk/python && PYTHONPATH=src uv run --no-project python -m unittest discover -s tests

# Native runtime without Docker
cargo build -p engine
uv run --no-project python scripts/smoke_native_runtime.py
```

## Failure Interpretation

| Failure | Likely Cause | Action |
|---------|-------------|--------|
| Secret scan fail | Hardcoded credential detected | Remove or externalise the secret |
| Import scan fail | Disallowed module imported | Replace with approved alternative |
| Routing guard fail | Active routing logic in sealed code | Gate behind governance boundary |
| Governance guard fail | Governance metadata missing | Add required control annotations |
| Stage-0 event guard fail | Unauthorised event mutation | Restore stage-0 immutability |
| Test failure | Logic regression | Read traceback, fix, re-run |

## Security Checker Relationship

The checker is the authoritative gate for the CA-7 sealed baseline. CI runs the same checker locally and in the pipeline — there is no divergence.

## CA-7 Sealed Baseline

This CI pipeline preserves the CA-7 sealed baseline. It does **not** start CA-8. Any baseline change requires a new controlled-adaptive closeout cycle.

## Future Expansion Candidates

- Coverage reporting (coverage.py)
- Linting (ruff / flake8)
- Type checking (mypy)
- Dependency vulnerability scanning (pip-audit)
- Matrix testing across Python versions
