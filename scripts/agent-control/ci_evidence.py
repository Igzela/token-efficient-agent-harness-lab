"""Fetch bounded, redacted CI failure evidence by canonical workflow run ID."""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
from typing import Any


MAX_LOG_CHARS = 50_000
MAX_FAILED_JOBS = 20
MAX_FAILED_STEPS = 20


def _gh(*args: str) -> str:
    result = subprocess.run(["gh", *args], capture_output=True, text=True, timeout=60)
    if result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or "GitHub CI evidence query failed")
    return result.stdout


def _redact(text: str) -> str:
    patterns = (
        r"gh[pousr]_[A-Za-z0-9_]{20,}",
        r"github_pat_[A-Za-z0-9_]{20,}",
        r"(?i)(authorization:\s*bearer\s+)[^\s]+",
        r"(?i)(token[=:]\s*)[^\s]+",
    )
    for pattern in patterns:
        text = re.sub(pattern, r"\1[REDACTED]" if "(" in pattern else "[REDACTED]", text)
    return text[:MAX_LOG_CHARS]


def fetch(run_id: int, repo: str | None = None) -> dict[str, Any]:
    target = repo or os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY")
    if not target:
        raise RuntimeError("GITHUB_REPOSITORY is unavailable")
    raw = _gh("api", "--paginate", f"repos/{target}/actions/runs/{run_id}/jobs?per_page=100")
    try:
        jobs = json.loads(raw).get("jobs", [])
    except (json.JSONDecodeError, AttributeError) as exc:
        raise RuntimeError("failed-jobs response was invalid") from exc
    failed = []
    for job in jobs:
        if job.get("conclusion") in {"success", "skipped"}:
            continue
        failed.append(
            {
                "name": str(job.get("name", ""))[:200],
                "conclusion": job.get("conclusion"),
                "failed_steps": [
                    str(step.get("name", ""))[:200]
                    for step in job.get("steps", [])
                    if step.get("conclusion") not in {"success", "skipped"}
                ][:MAX_FAILED_STEPS],
            }
        )
    logs = _redact(_gh("run", "view", str(run_id), "--log-failed"))
    return {"schema_version": 1, "failed_run_id": run_id, "failed_jobs": failed[:MAX_FAILED_JOBS], "logs": logs}


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("Usage: ci_evidence.py <failed-run-id>")
    try:
        print(json.dumps(fetch(int(sys.argv[1])), sort_keys=True))
    except (RuntimeError, ValueError) as exc:
        print(f"CI_EVIDENCE_ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
