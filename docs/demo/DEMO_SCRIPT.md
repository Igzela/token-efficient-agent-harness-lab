# Demo Script

Step-by-step operator walkthrough. Follow each section in order.

## A. Start the App

```bash
cd /home/igzela/Projects/token-efficient-agent-harness-lab

rm -f /tmp/harness-demo-registry.json /tmp/harness-demo-plans.json

python3 tools/harness_app_server.py \
  --host 127.0.0.1 \
  --port 8769 \
  --registry /tmp/harness-demo-registry.json \
  --plans /tmp/harness-demo-plans.json
```

Open `http://127.0.0.1:8769/` in a browser.

## B. Register a Local Repo

In the dashboard, fill in the **Register Repository** form:

| Field    | Value                                        |
|----------|----------------------------------------------|
| ID       | `alters-lab`                                 |
| Name     | `Alters Lab`                                 |
| Kind     | `local`                                      |
| Location | `/home/igzela/Projects/alters-lab`           |

Click **Register**. The repo appears in the repository selector.

## C. Refresh Status

Click **Refresh** in the Operations section. The diagnostics panel updates to show component health, data flow, and storage status.

## D. Audit the Selected Repo

Select `alters-lab` in the repository selector. Click **Run Audit**.

## E. Confirm Audit Results

Verify:

- **Verdict:** `PASS` or `PASS_WITH_NOTES`
- **Warnings:** empty list, or non-blocking notes about harness control files
- **Blockers:** empty list
- **Component health:** all components report `ok`
- **Data flow:** all steps report `ok`
- **Storage:** registry and plans point to `/tmp` demo files

## F. Create Three Non-Executable Demo Plans

With `alters-lab` still selected, create each plan using the **Generate Plan** form.

### Plan 1: Read-Only Docs Audit

| Field              | Value                                              |
|--------------------|----------------------------------------------------|
| Task ID            | `demo-read-only-audit`                             |
| Objective          | `Read-only audit of harness control files`         |
| Task Type          | `audit`                                            |
| Risk Level         | `low`                                              |
| Context Tokens     | `4000`                                             |
| Execution Tokens   | `3000`                                             |

Expected status: `ready_for_review`. No approval gates. No blockers.

### Plan 2: Provider/Sandbox Boundary Task

| Field              | Value                                                          |
|--------------------|----------------------------------------------------------------|
| Task ID            | `demo-provider-sandbox-gated`                                  |
| Objective          | `Provider integration with sandbox execution`                  |
| Task Type          | `provider`                                                     |
| Risk Level         | `high`                                                         |
| Context Tokens     | `4000`                                                         |
| Execution Tokens   | `3000`                                                         |

Expected status: `needs_approval`. Approval gates should include provider/sandbox boundary gates.

### Plan 3: Budget-Pressure Task and Lower-Budget Variant

First, create the high-budget version:

| Field              | Value                                              |
|--------------------|----------------------------------------------------|
| Task ID            | `demo-budget-pressure`                             |
| Objective          | `Write documentation for all modules`              |
| Task Type          | `write`                                            |
| Risk Level         | `medium`                                           |
| Context Tokens     | `4000`                                             |
| Execution Tokens   | `3000`                                             |

Then create a lower-budget variant:

| Field              | Value                                              |
|--------------------|----------------------------------------------------|
| Task ID            | `demo-budget-lower`                                |
| Objective          | `Write documentation for all modules`              |
| Task Type          | `write`                                            |
| Risk Level         | `medium`                                           |
| Context Tokens     | `1000`                                             |
| Execution Tokens   | `500`                                              |

The lower-budget variant should surface `review_token_budget` or `reduce_budget` in its review action or blockers.

## G. Use the Workbench Features

### Plan Review

In the **Plan Workbench** section, click **View plan** on each stored plan. Confirm the plan details, steps, approval gates, blockers, and token efficiency notes are displayed.

### Review Guidance

In the **Review Guidance** section, select a stored plan and click **Generate Guidance**. Confirm:

- Advisory options are listed (e.g., `continue_review`, `reduce_budget`, `inspect_blockers`)
- Evidence requirements are listed
- Token-efficiency guidance is listed
- Boundary notice states guidance is advisory only

### Portfolio Triage

In the **Portfolio Triage** section, click **Refresh Triage**. Confirm:

- Plans are ranked by semantic risk and budget, not merely stored order
- Token hotspots are identified
- Budget pressure items are flagged
- Boundary notice states triage is advisory only

### Operations Diagnostics

In the **Operations** section, confirm:

- `component_count`: 10
- `recent_errors`: empty list
- All components report `ok`
- All data flow steps report `ok`
- Storage shows registry and plans with record counts

## H. Stop the Server

Press `Ctrl+C` in the terminal. Confirm:

```
Stopping Harness App server.
```

## I. Confirm Target Repo Unchanged

```bash
git -C /home/igzela/Projects/alters-lab status -sb
git -C /home/igzela/Projects/alters-lab diff --stat
```

Both should show no changes. The target repository is read-only. All generated plans and registry entries are app-owned state in `/tmp`.

## What This Demo Shows

- Generated plans are **app-owned state only** — they live in `/tmp`, not in the target repo.
- Plans are **not execution authorization** — they are non-executable resource estimates.
- Guidance and triage are **human-review-only** — they do not approve, execute, or assign work.
- The target repository is **never written to** by the app.
