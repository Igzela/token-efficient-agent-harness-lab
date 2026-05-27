# Troubleshooting

Common issues and safe fixes.

## Port Already in Use

**Symptom:** `OSError: [Errno 98] Address already in use` when starting the server.

**Likely cause:** Another process is using port 8769.

**Safe fix:**

```bash
pgrep -af "harness_app_server" || true
```

If found, stop it with `Ctrl+C` or `kill <PID>`. If not found, another service is using the port. Use a different port:

```bash
python3 tools/harness_app_server.py \
  --host 127.0.0.1 \
  --port 8770 \
  --registry /tmp/harness-demo-registry.json \
  --plans /tmp/harness-demo-plans.json
```

**What not to do:** Do not use `kill -9` unless the process is unresponsive. Do not bind to `0.0.0.0`.

---

## Node Command Unavailable

**Symptom:** `node: command not found` when running `node --check web/dashboard/app.js`.

**Likely cause:** Node.js is not installed or not on PATH.

**Safe fix:** Install Node.js or skip the dashboard syntax check. The dashboard is static HTML/JS and does not require Node at runtime. The check is a preflight validation only.

**What not to do:** Do not install Node.js globally just for this demo if you do not need it otherwise.

---

## Target Repo Path Missing

**Symptom:** Audit returns `BLOCKED` with error about invalid repo path.

**Likely cause:** The path `/home/igzela/Projects/alters-lab` does not exist or is not a git repository.

**Safe fix:** Verify the path exists and is a git repo:

```bash
ls -la /home/igzela/Projects/alters-lab/.git
```

If using a different target repo, register it with its actual path.

**What not to do:** Do not create the directory manually. Do not modify the target repo to make it pass.

---

## Audit Returns PASS_WITH_NOTES

**Symptom:** Audit verdict is `PASS_WITH_NOTES` instead of `PASS`.

**Likely cause:** The target repo has harness control files but some have structural notes (e.g., missing optional files, policy wording differences).

**Safe fix:** This is expected and acceptable. Review the notes in the audit output. No action required for the demo.

**What not to do:** Do not modify the target repo to suppress notes. The audit is read-only.

---

## Audit Returns BLOCKED

**Symptom:** Audit verdict is `BLOCKED`.

**Likely cause:** Required harness control files are missing from the target repo. The auditor checks for `AGENTS.md`, `docs/harness/PROJECT_BOARD.md`, `docs/harness/TASK_QUEUE.md`, `docs/harness/QUALITY_GATES.md`, `docs/harness/DECISION_RECORD.md`, `docs/harness/RISK_REGISTER.md`.

**Safe fix:** Verify the target repo has these files. If using `alters-lab`, it should have them. If using a different repo, it may not.

**What not to do:** Do not create missing files in the target repo just to pass the audit.

---

## Plan Store Corrupted

**Symptom:** API returns `plan_store_error` or the dashboard shows errors when loading plans.

**Likely cause:** The plan file at `/tmp/harness-demo-plans.json` was modified or corrupted.

**Safe fix:** Delete the plan file and restart the server:

```bash
rm -f /tmp/harness-demo-plans.json
```

Restart the server. Plans will need to be recreated.

**What not to do:** Do not manually edit the plan JSON file.

---

## Registry Corrupted

**Symptom:** API returns `invalid_registry_request` or repos do not load.

**Likely cause:** The registry file at `/tmp/harness-demo-registry.json` was modified or corrupted.

**Safe fix:** Delete the registry file and restart the server:

```bash
rm -f /tmp/harness-demo-registry.json
```

Restart the server. Repos will need to be re-registered.

**What not to do:** Do not manually edit the registry JSON file.

---

## Dashboard Cannot Connect to API

**Symptom:** Dashboard shows "Static sample" in the API state indicator. Repos do not load.

**Likely cause:** The server is not running, or the browser is connecting to a different host/port.

**Safe fix:**

1. Confirm the server is running in the terminal.
2. Confirm the browser URL is `http://127.0.0.1:8769/` (not `localhost`, not a different port).
3. Check the browser console for fetch errors.

**What not to do:** Do not change the server bind host. Only `127.0.0.1` and `localhost` are allowed.

---

## Recent Errors Not Empty

**Symptom:** Operations diagnostics show entries in `recent_errors`.

**Likely cause:** A previous API call failed (e.g., invalid plan request, missing repo). The dashboard caches up to 5 client-observed errors.

**Safe fix:** Click **Refresh** in the Operations section. If errors persist, check the error message for the specific issue. Most errors are transient and clear after a successful request.

**What not to do:** Do not restart the server just to clear errors unless the server itself is failing.

---

## Target Repo Not Clean

**Symptom:** `git -C /home/igzela/Projects/alters-lab status -sb` shows changes.

**Likely cause:** The target repo had uncommitted changes before the demo, or something outside the demo modified it.

**Safe fix:** Check `git -C /home/igzela/Projects/alters-lab diff --stat` to see what changed. The Harness App does not write to target repos. If changes exist, they are from another source.

**What not to do:** Do not assume the app caused the changes. Verify by checking the diff content.

---

## Server Left Running

**Symptom:** `pgrep -af "harness_app_server"` returns a PID after the demo.

**Likely cause:** The server was not stopped with `Ctrl+C`.

**Safe fix:**

```bash
pkill -f "harness_app_server.*8769"
```

Or find the PID and send SIGTERM:

```bash
pgrep -af "harness_app_server"
kill <PID>
```

**What not to do:** Do not use `kill -9` unless the process is unresponsive. Do not leave the server running after the demo.
