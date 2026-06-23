#!/usr/bin/env bash
set -euo pipefail

REPO="${GITHUB_REPOSITORY:-Igzela/token-efficient-agent-harness-lab}"
WORKFLOW="${ACP_WATCHDOG_WORKFLOW:-tests}"
BRANCH="${ACP_WATCHDOG_BRANCH:-main}"

if ! command -v gh >/dev/null 2>&1; then
  echo "ERROR: gh CLI is required for autonomy watchdog" >&2
  exit 1
fi

run_json="$(gh run list \
  --repo "$REPO" \
  --workflow "$WORKFLOW" \
  --branch "$BRANCH" \
  --limit 1 \
  --json databaseId,headSha,status,conclusion,createdAt,url)"

python3 - "$run_json" <<'PY'
import json
import sys

runs = json.loads(sys.argv[1])
if not runs:
    print("ERROR: no workflow runs found for watched workflow/branch", file=sys.stderr)
    raise SystemExit(1)

run = runs[0]
status = run.get("status")
conclusion = run.get("conclusion")
print(
    "watchdog latest run:",
    f"id={run.get('databaseId')}",
    f"sha={run.get('headSha')}",
    f"status={status}",
    f"conclusion={conclusion}",
    f"createdAt={run.get('createdAt')}",
    f"url={run.get('url')}",
)

if status != "completed":
    print("ERROR: latest watched workflow has not completed", file=sys.stderr)
    raise SystemExit(1)
if conclusion != "success":
    print("ERROR: latest watched workflow is not green", file=sys.stderr)
    raise SystemExit(1)
PY
