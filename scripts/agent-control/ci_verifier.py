"""Exact-head verification and dispatch for the canonical tests workflow."""

from __future__ import annotations

import json
import os
import subprocess
import time
from pathlib import Path
from typing import Any


REQUIREMENTS_PATH = Path(__file__).with_name("ci_requirements.json")
ACTIVE_RUN_STATUSES = {"queued", "in_progress", "requested", "waiting", "pending"}
SUPPORTED_COMPLETED_CONCLUSIONS = {"success", "failure"}


class CIVerificationError(RuntimeError):
    """Raised when exact-head CI evidence is missing or unsafe."""


def load_requirements() -> dict[str, Any]:
    with REQUIREMENTS_PATH.open(encoding="utf-8") as handle:
        data = json.load(handle)
    jobs = data.get("required_jobs")
    if data.get("workflow_name") != "tests" or not isinstance(jobs, list) or not jobs:
        raise CIVerificationError("invalid canonical CI requirements")
    if len(jobs) != len(set(jobs)) or any(not isinstance(job, str) or not job for job in jobs):
        raise CIVerificationError("invalid canonical required job set")
    return data


def _gh(*args: str) -> str | None:
    result = subprocess.run(
        ["gh", *args], capture_output=True, text=True, timeout=60
    )
    if result.returncode != 0:
        return None
    return result.stdout.strip()


def _gh_json(*args: str) -> Any | None:
    output = _gh(*args)
    if output is None:
        return None
    try:
        return json.loads(output)
    except json.JSONDecodeError:
        return None


def run_info(run_id: int | str) -> dict[str, Any] | None:
    return _gh_json(
        "run",
        "view",
        str(run_id),
        "--json",
        "databaseId,status,conclusion,headSha,headBranch,workflowName,createdAt,updatedAt,event,jobs",
    )


def verify_failed_run(run_id: int | str, expected_sha: str) -> dict[str, Any]:
    """Validate the exact failed canonical run used to prepare a repair."""

    requirements = load_requirements()
    run = run_info(run_id)
    if not run:
        raise CIVerificationError("workflow run is absent")
    if run.get("workflowName") != requirements["workflow_name"]:
        raise CIVerificationError("workflow name does not match canonical tests workflow")
    if run.get("status") != "completed":
        raise CIVerificationError("failed workflow run is not completed")
    if run.get("conclusion") != "failure":
        raise CIVerificationError("workflow run is not a failure")
    if run.get("headSha") != expected_sha:
        raise CIVerificationError("failed workflow run head SHA does not match expected head")
    return {
        "workflow_name": requirements["workflow_name"],
        "workflow_run_id": run.get("databaseId") or run_id,
        "head_sha": expected_sha,
        "status": run.get("status"),
        "conclusion": run.get("conclusion"),
    }


def find_exact_runs(branch: str, head_sha: str) -> list[dict[str, Any]]:
    requirements = load_requirements()
    runs = _gh_json(
        "run",
        "list",
        "--workflow",
        f"{requirements['workflow_name']}.yml",
        "--branch",
        branch,
        "--limit",
        "50",
        "--json",
        "databaseId,status,conclusion,headSha,headBranch,workflowName,createdAt,updatedAt,event",
    )
    if not isinstance(runs, list):
        return []
    exact = [
        run for run in runs
        if run.get("headSha") == head_sha
        and run.get("headBranch") == branch
        and run.get("workflowName") == requirements["workflow_name"]
    ]
    exact.sort(key=lambda run: int(run.get("databaseId", 0)))
    return [run_info(run.get("databaseId")) or run for run in exact]


def find_exact_run(branch: str, head_sha: str) -> dict[str, Any] | None:
    runs = find_exact_runs(branch, head_sha)
    return runs[0] if runs else None


def _acquirable_runs(runs: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Exclude terminal runs that can never provide supported CI evidence."""

    return [
        run
        for run in runs
        if run.get("status") in ACTIVE_RUN_STATUSES
        or (
            run.get("status") == "completed"
            and run.get("conclusion") in SUPPORTED_COMPLETED_CONCLUSIONS
        )
    ]


def acquire_exact_ci(
    pr_number: int,
    branch: str,
    head_sha: str,
    observe_seconds: int = 20,
    dispatch_timeout_seconds: int = 60,
) -> dict[str, Any]:
    """Reuse one exact-head run, falling back to at most one dispatch."""

    requirements = load_requirements()
    deadline = time.monotonic() + observe_seconds
    all_runs = find_exact_runs(branch, head_sha)
    runs = _acquirable_runs(all_runs)
    while not runs and time.monotonic() < deadline:
        time.sleep(2)
        all_runs = find_exact_runs(branch, head_sha)
        runs = _acquirable_runs(all_runs)

    source = "pull_request"
    if not runs:
        result = subprocess.run(
            [
                "gh", "workflow", "run", f"{requirements['workflow_name']}.yml",
                "--ref", branch, "-f", f"expected_sha={head_sha}",
            ], capture_output=True, text=True, timeout=60,
        )
        if result.returncode != 0:
            raise CIVerificationError("canonical tests workflow dispatch failed")
        source = "workflow_dispatch"
        deadline = time.monotonic() + dispatch_timeout_seconds
        while time.monotonic() < deadline:
            all_runs = find_exact_runs(branch, head_sha)
            runs = _acquirable_runs(all_runs)
            if runs:
                break
            time.sleep(2)
    if not runs:
        raise CIVerificationError("exact-head CI run did not become observable")
    selected = runs[0]
    event = selected.get("event")
    if event == "workflow_dispatch":
        source = "workflow_dispatch"
    elif event == "pull_request":
        source = "pull_request"
    selected_id = selected.get("databaseId")
    duplicate_ids = [
        run.get("databaseId")
        for run in all_runs
        if run.get("databaseId") is not None and run.get("databaseId") != selected_id
    ]
    return {
        "kind": "agent-orchestrator-ci-acquisition",
        "pr_number": int(pr_number),
        "head_sha": head_sha,
        "workflow_run_id": selected.get("databaseId"),
        "source": source,
        "status": "bound",
        "duplicate_run_ids": duplicate_ids,
    }


def verify_exact_head_ci(
    pr_number: int,
    expected_sha: str,
    workflow_run_id: int | str | None = None,
    pr_snapshot: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Return structured evidence or fail for any stale/incomplete CI state."""
    requirements = load_requirements()
    if workflow_run_id is None:
        raise CIVerificationError("workflow run id is required for exact-head verification")
    run = run_info(workflow_run_id)
    if not run:
        raise CIVerificationError("workflow run is absent")
    if run.get("workflowName") != requirements["workflow_name"]:
        raise CIVerificationError("workflow name does not match canonical tests workflow")
    if run.get("headSha") != expected_sha:
        raise CIVerificationError("CI head SHA does not match expected head")
    if run.get("status") != "completed" or run.get("conclusion") != "success":
        raise CIVerificationError("CI workflow is not completed successfully")

    jobs = run.get("jobs")
    if not isinstance(jobs, list):
        raise CIVerificationError("CI job evidence is absent")
    by_name = {job.get("name"): job for job in jobs}
    required = requirements["required_jobs"]
    missing = [name for name in required if name not in by_name]
    if missing:
        raise CIVerificationError(f"required CI jobs are absent: {missing}")
    invalid = [
        name
        for name in required
        if by_name[name].get("status") != "completed"
        or by_name[name].get("conclusion") != "success"
    ]
    if invalid:
        raise CIVerificationError(f"required CI jobs are not successful: {invalid}")
    if pr_snapshot is not None and pr_snapshot.get("headRefOid") != expected_sha:
        raise CIVerificationError("PR head moved while verifying CI")

    return {
        "kind": "agent-orchestrator-ci-state",
        "pr_number": pr_number,
        "head_sha": expected_sha,
        "workflow_run_id": run.get("databaseId") or workflow_run_id,
        "workflow_name": requirements["workflow_name"],
        "required_jobs": required,
        "successful_jobs": required,
        "status": "success",
        "created_at": run.get("createdAt"),
        "updated_at": run.get("updatedAt"),
    }


def dispatch_exact_ci(branch: str, head_sha: str, timeout_seconds: int = 60) -> dict[str, Any]:
    acquisition = acquire_exact_ci(0, branch, head_sha, 0, timeout_seconds)
    acquisition["workflow_name"] = load_requirements()["workflow_name"]
    return acquisition


def main() -> None:
    import sys

    try:
        if len(sys.argv) == 4 and sys.argv[1] == "dispatch":
            result = dispatch_exact_ci(sys.argv[2], sys.argv[3])
        elif len(sys.argv) == 4 and sys.argv[1] == "verify-failed-run":
            result = verify_failed_run(sys.argv[2], sys.argv[3])
        elif len(sys.argv) == 5 and sys.argv[1] == "acquire":
            result = acquire_exact_ci(int(sys.argv[2]), sys.argv[3], sys.argv[4])
        else:
            raise SystemExit(
                "Usage: ci_verifier.py <dispatch branch sha|verify-failed-run run-id sha|acquire pr branch sha>"
            )
        print(json.dumps(result, sort_keys=True))
    except CIVerificationError as exc:
        print(f"CI_VERIFICATION_ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
