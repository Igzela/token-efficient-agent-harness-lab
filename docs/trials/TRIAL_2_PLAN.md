# Trial 2 Plan

Status: PLANNING — Do not execute until explicitly approved.

## Goal

Validate that the Harness App generalizes beyond alters-lab to a second, operationally different local repository.

## Target

- **Repo:** `/home/igzela/Projects/hermes-gateway-lab`
- **Commit:** `e96c61d`
- **Type:** Operational gateway/worker system (approval queues, dry-run executors, permission boundaries, systemd services)

## Why This Target

hermes-gateway-lab is fundamentally different from alters-lab:
- alters-lab: research/experimental codebase
- hermes-gateway-lab: operational system with approval flows, permission deny patterns, dry-run executors, and systemd service drafts

This tests whether audit/planning generalizes beyond research repos to operational codebases.

## Execution Steps

### Step 1: Preflight

```bash
cd /home/igzela/Projects/token-efficient-agent-harness-lab
git checkout main && git pull --rebase origin main
git checkout -b trial2-execution
python3 tools/check_security_baseline.py
PYTHONPATH=src python3 -m unittest discover -s tests
node --check web/dashboard/app.js
```

### Step 2: Start Server

```bash
rm -f /tmp/harness-trial2-registry.json /tmp/harness-trial2-plans.json
python3 tools/harness_app_server.py \
  --host 127.0.0.1 --port 8769 \
  --registry /tmp/harness-trial2-registry.json \
  --plans /tmp/harness-trial2-plans.json
```

### Step 3: Register Target

POST to `/api/repos` with:
```json
{
  "id": "hermes-gateway-lab",
  "name": "Hermes Gateway Lab",
  "kind": "local",
  "path": "/home/igzela/Projects/hermes-gateway-lab"
}
```

### Step 4: Run Audit

GET `/api/audit?repo_id=hermes-gateway-lab`

Expected: verdict PASS or PASS_WITH_NOTES (this repo has no AGENTS.md or harness control files — may return BLOCKED or notes about missing files).

### Step 5: Create Plans

Create 5 plans via POST `/api/plans`:

1. `trial2-docs-audit` — audit, low risk
2. `trial2-approval-queue-review` — review, medium risk
3. `trial2-permission-boundary` — boundary, high risk
4. `trial2-budget-pressure-docs` — write, medium risk, high budget
5. `trial2-budget-lower` — write, medium risk, low budget

### Step 6: Verify

- Plan statuses match expected results
- Review guidance produces advisory options
- Triage ranks plans semantically
- Diagnostics report 10 components, no errors
- Storage points to /tmp trial2 files

### Step 7: Confirm Target Unchanged

```bash
git -C /home/igzela/Projects/hermes-gateway-lab status -sb
git -C /home/igzela/Projects/hermes-gateway-lab diff --stat
```

### Step 8: Stop Server

Press Ctrl+C. Verify no leftover process.

### Step 9: Write Report

Create `docs/trials/TRIAL_2_REPORT.md` with results.

### Step 10: Commit and Push

Commit trial2-execution branch, push, open PR against main. Do not merge automatically.

## Boundary

Same as TRIAL_2_CANDIDATE_SELECTION.md section 5. No target repo writes, no provider calls, no execution, no MVP9, no Stage 5.
