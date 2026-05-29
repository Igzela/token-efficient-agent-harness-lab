# Agent Control Plane Migration Closeout

Date: 2026-05-29

## Status

Agent-control-plane migration phases 0-7 are implemented on the active branch, and Phase 8 closeout is recorded here.

- Rust `engine/` owns deterministic dispatch parity, routing/orchestration parity, a disabled-by-default provider trait boundary, storage parity, and the local axum API.
- `wire_contract/v1/` remains the frozen JSON contract surface.
- `codegen/` generates Rust, TypeScript, and Python wire types from the frozen schemas.
- `sdk/typescript/` and `sdk/python/` provide REST SDKs and do not bind private Rust internals.
- `dashboard/` provides a read-only Next.js dashboard with dispatch, routing, agents/workflows, costs, settings, and health views.
- `deploy/` plus root `docker-compose.yml` provide local API + dashboard startup only.
- Python reference implementation remains in `src/harness_core/` until any future explicit removal decision.

## Boundary Evidence

- Real provider calls remain off by default.
- No target repository write path was added.
- No real sandbox/process/container/VM execution was added beyond local Docker build/run validation for this repository.
- No runtime autonomous workers were added.
- Dashboard has no approve/run/deploy/execute/merge controls and does not call the dispatch POST endpoint.
- Docker files contain no production credentials and are local development artifacts.

## Verification Evidence

Verified in the 2026-05-29 main-branch audit:

- `cargo fmt --check`
- `cargo clippy -p engine -- -D warnings`
- `cargo test -p engine`
- `cd dashboard && pnpm lint && pnpm typecheck && pnpm build`
- `cd sdk/typescript && pnpm build && npm pack --dry-run`
- `cd sdk/python && PYTHONPATH=src python3 -m unittest discover -s tests`
- `cd sdk/python && python -m build`
- `python3 tests/integration/parity/run.py`
- `python3 scripts/check_agent_handoff.py`
- `python3 tools/check_security_baseline.py`
- `PYTHONPATH=src python3 -m unittest discover -s tests`
- `docker compose build`
- `docker compose up --build -d`
- `GET /api/v1/health`
- `POST /api/v1/dispatch`
- dashboard HTTP smoke on `http://127.0.0.1:3000/`

Note: this environment's `python3 -m build` entrypoint is not available because the system `build` package lacks an executable module. The repository's documented Python SDK packaging check uses `python -m build`, which passed.

## Remaining Decision

No further migration implementation slice is known inside the approved scope. Future work should be maintenance, verification hardening, or an explicit user-approved decision about removing or relocating the Python reference implementation.
