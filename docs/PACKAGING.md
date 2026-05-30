# Packaging

## Current State

The legacy root `pyproject.toml` (setuptools config for `src/harness_core/`) has been removed along with the Python reference implementation. The only remaining Python packaging is the self-contained Python REST SDK.

## Python REST SDK

`sdk/python/pyproject.toml` declares the `agent-control-plane-sdk` package:

- Build system: setuptools >= 64
- Python: >= 3.11
- Dependencies: none (pure stdlib, uses `urllib.request` for REST calls)
- Package source: `sdk/python/src/agent_control_plane_sdk/`

Build and verify:

```bash
cd sdk/python && python -m build
```

## Rust Engine

The Rust engine is built with Cargo:

```bash
cargo build -p engine
cargo build -p engine --release
```

Release packaging:

```bash
bash scripts/package-release.sh
```

This produces `dist/agent-control-plane-v0.1.0-linux-x86_64.tar.gz` containing the engine binary, static dashboard, and install/upgrade scripts.

## Dashboard

The Next.js dashboard is built with Bun:

```bash
cd dashboard && bun install --frozen-lockfile && bun run build:static
```

The static export goes to `dashboard/out/` and can be served by the Rust engine via `ACP_DASHBOARD_DIR=dashboard/out`.

## Docker (Optional)

```bash
docker compose build
docker compose up --build -d
```
