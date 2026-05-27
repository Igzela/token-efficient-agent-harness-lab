# Quickstart

Get the Harness App running locally in under two minutes.

## 1. Verify the Repository

```bash
cd /home/igzela/Projects/token-efficient-agent-harness-lab
```

Run the security baseline checker:

```bash
python3 tools/check_security_baseline.py
```

Run the test suite:

```bash
PYTHONPATH=src python3 -m unittest discover -s tests
```

Check the dashboard JavaScript for syntax errors:

```bash
node --check web/dashboard/app.js
```

All three should pass before continuing.

## 2. Clean Demo State

Remove any leftover demo registry and plan files:

```bash
rm -f /tmp/harness-demo-registry.json /tmp/harness-demo-plans.json
```

## 3. Start the App Server

```bash
python3 tools/harness_app_server.py \
  --host 127.0.0.1 \
  --port 8769 \
  --registry /tmp/harness-demo-registry.json \
  --plans /tmp/harness-demo-plans.json
```

The server prints:

```
Serving Harness App on http://127.0.0.1:8769/
Registry: /tmp/harness-demo-registry.json
Plans: /tmp/harness-demo-plans.json
```

## 4. Open the Dashboard

Open in a browser:

```
http://127.0.0.1:8769/
```

The dashboard loads with a static sample report. API-connected features activate after the first repo is registered.

## 5. Stop the Server

Press `Ctrl+C` in the terminal running the server. The server prints:

```
Stopping Harness App server.
```

## 6. Confirm Clean Shutdown

Check no server process remains:

```bash
pgrep -af "harness_app_server|claude" || true
```

Confirm the target repository was not modified:

```bash
git -C /home/igzela/Projects/alters-lab status -sb
git -C /home/igzela/Projects/alters-lab diff --stat
```

Both should show no changes.
