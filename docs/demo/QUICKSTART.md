# Quickstart

Get the Harness App running locally in under two minutes.

## 1. Verify the Repository

```bash
cd /home/igzela/Projects/token-efficient-agent-harness-lab
```

Run the security baseline checker:

```bash
uv run --no-project python tools/check_security_baseline.py
```

Run the Rust engine tests:

```bash
cargo test -p engine
```

Both should pass before continuing.

## 2. Build the Static Dashboard

```bash
cd dashboard && bun install --frozen-lockfile && bun run build:static && cd ..
```

## 3. Start the Engine

API only:

```bash
cargo run -p engine
```

API plus dashboard from the same Rust process:

```bash
ACP_DASHBOARD_DIR=dashboard/out cargo run -p engine
```

The engine starts on `http://127.0.0.1:8080` and creates app-owned local state at `.agent-control-plane/local-team.db`.

## 4. Verify

```bash
curl http://127.0.0.1:8080/api/v1/health
curl http://127.0.0.1:8080/api/v1/dashboard
curl -X POST http://127.0.0.1:8080/api/v1/dispatch \
  -H 'content-type: application/json' \
  -d '{"raw_request":"Summarize docs without provider calls","request_source":"api"}'
```

If using the dashboard, open `http://127.0.0.1:8080` in a browser.

## 5. Stop the Engine

Press `Ctrl+C` in the terminal running the engine.

## 6. Confirm Clean Shutdown

Check no engine process remains:

```bash
pgrep -af engine || true
```
