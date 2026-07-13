"""Exact-head verification and dispatch for the canonical tests workflow."""

from __future__ import annotations

import json
import os
import subprocess
import time
from datetime import datetime
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
    summary = _gh_json(
        "run",
        "view",
        str(run_id),
        "--json",
        "databaseId,status,conclusion,headSha,headBranch,workflowName,workflowDatabaseId,createdAt,updatedAt,event,jobs",
    )
    if not isinstance(summary, dict):
        return None
    target = os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY")
    if target:
        details = _gh_json("api", f"repos/{target}/actions/runs/{run_id}")
        if isinstance(details, dict):
            summary["repository"] = (details.get("repository") or {}).get("full_name") or target
            summary["headRepository"] = (details.get("head_repository") or {}).get("full_name")
            summary["workflowId"] = details.get("workflow_id")
            summary["path"] = details.get("path")
            summary["attempt"] = details.get("run_attempt")
            pull_requests = details.get("pull_requests")
            if isinstance(pull_requests, list):
                summary["pullRequestNumbers"] = sorted(
                    int(item["number"])
                    for item in pull_requests
                    if isinstance(item, dict) and str(item.get("number", "")).isdigit()
                )
    return summary


def verify_failed_run(run_id: int | str, expected_sha: str) -> dict[str, Any]:
    """Validate the exact failed canonical run used to prepare a repair."""

    requirements = load_requirements()
    run = run_info(run_id)
    if not run:
        raise CIVerificationError("workflow run is absent")
    if run.get("workflowName") != requirements["workflow_name"]:
        raise CIVerificationError("workflow name does not match canonical tests workflow")
    expected_repo = os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY")
    if expected_repo and run.get("repository") not in (None, expected_repo):
        raise CIVerificationError("CI repository does not match expected repository")
    if expected_repo and run.get("headRepository") != expected_repo:
        raise CIVerificationError("CI head repository is not trusted")
    workflow_id = requirements.get("workflow_id")
    if workflow_id is not None and run.get("workflowDatabaseId") not in (None, workflow_id):
        raise CIVerificationError("CI workflow identity does not match canonical workflow")
    workflow_path = requirements.get("workflow_path")
    if workflow_path and run.get("path") not in (None, workflow_path):
        raise CIVerificationError("CI workflow path does not match canonical workflow")
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


def _candidate_pr_numbers(run: dict[str, Any]) -> list[int] | None:
    """Return provider PR identity when the run API supplied it.

    Natural pull-request runs are required to carry the expected PR when this
    field is available.  Dispatch runs commonly have no pull-request payload,
    so an absent/empty list is intentionally allowed for that event type.
    """

    if "pullRequestNumbers" in run:
        value = run.get("pullRequestNumbers")
    elif "pull_requests" in run:
        value = run.get("pull_requests")
    elif "pull_request_number" in run or "pr_number" in run:
        value = [run.get("pull_request_number", run.get("pr_number"))]
    else:
        return None
    if not isinstance(value, list):
        return []
    numbers: list[int] = []
    for item in value:
        if isinstance(item, dict):
            item = item.get("number")
        if isinstance(item, int) or (isinstance(item, str) and item.isdigit()):
            numbers.append(int(item))
    return sorted(set(numbers))


def find_exact_runs(branch: str, head_sha: str, expected_pr: int | None = None) -> list[dict[str, Any]]:
    requirements = load_requirements()
    target = os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY")
    args = [
        "run", "list", "--workflow", f"{requirements['workflow_name']}.yml",
        "--branch", branch, "--limit", "50",
        "--json", "databaseId,status,conclusion,headSha,headBranch,workflowName,workflowDatabaseId,createdAt,updatedAt,event",
    ]
    if target:
        args[2:2] = ["--repo", target]
    runs = _gh_json(
        *args,
    )
    if not isinstance(runs, list):
        return []
    exact = [run for run in runs if run.get("headSha") == head_sha and run.get("headBranch") == branch]
    enriched = [run_info(run.get("databaseId")) or run for run in exact]
    return [
        run for run in enriched
        if _candidate_matches(run, branch, head_sha, requirements, expected_pr)
    ]


def find_exact_run(branch: str, head_sha: str, expected_pr: int | None = None) -> dict[str, Any] | None:
    return select_canonical_run(find_exact_runs(branch, head_sha, expected_pr))


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


def _candidate_matches(
    run: dict[str, Any],
    branch: str,
    head_sha: str,
    requirements: dict[str, Any],
    expected_pr: int | None = None,
) -> bool:
    if run.get("headSha") != head_sha or run.get("headBranch") != branch:
        return False
    if run.get("workflowName") != requirements["workflow_name"]:
        return False
    expected_repo = os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY")
    if expected_repo and run.get("headRepository") != expected_repo:
        return False
    workflow_id = requirements.get("workflow_id")
    if workflow_id is not None and run.get("workflowDatabaseId") not in (None, workflow_id):
        return False
    workflow_path = requirements.get("workflow_path")
    if workflow_path and run.get("path") not in (None, workflow_path):
        return False
    if expected_pr is not None:
        numbers = _candidate_pr_numbers(run)
        if numbers is not None and run.get("event") == "pull_request":
            if int(expected_pr) not in numbers:
                return False
        elif numbers:
            if int(expected_pr) not in numbers:
                return False
    return True


def _timestamp(run: dict[str, Any]) -> float:
    value = run.get("updatedAt") or run.get("createdAt") or ""
    try:
        return datetime.fromisoformat(str(value).replace("Z", "+00:00")).timestamp()
    except (TypeError, ValueError, OverflowError):
        return 0.0


def _run_id(run: dict[str, Any]) -> int:
    try:
        return int(run.get("databaseId", 0))
    except (TypeError, ValueError):
        return 0


def select_canonical_run(runs: list[dict[str, Any]]) -> dict[str, Any] | None:
    """Select newest usable evidence; event type only breaks otherwise equal ties."""
    supported = _acquirable_runs(runs)
    if not supported:
        return None
    completed = [run for run in supported if run.get("status") == "completed"]
    pool = completed or supported
    selected = max(
        pool,
        key=lambda run: (
            _timestamp(run),
            int(run.get("attempt", 0) or 0),
            1 if run.get("event") == "pull_request" else 0,
            _run_id(run),
        ),
    )
    return selected


def _selection_reason(selected: dict[str, Any], candidates: list[dict[str, Any]]) -> str:
    if selected.get("status") == "completed":
        return "newest_completed_supported"
    if selected.get("event") == "pull_request":
        return "natural_active_observed"
    return "fallback_active_observed"


def acquire_exact_ci(
    pr_number: int,
    branch: str,
    head_sha: str,
    observe_seconds: int = 20,
    dispatch_timeout_seconds: int = 60,
) -> dict[str, Any]:
    """Acquire canonical exact-head evidence with one bounded fallback."""

    requirements = load_requirements()
    deadline = time.monotonic() + max(0, observe_seconds)
    fallback_dispatched = False
    all_runs: list[dict[str, Any]] = []
    selected: dict[str, Any] | None = None
    while True:
        all_runs = [
            run for run in find_exact_runs(branch, head_sha, pr_number)
            if _candidate_matches(run, branch, head_sha, requirements, pr_number)
        ]
        selected = select_canonical_run(all_runs)
        if selected and selected.get("status") == "completed":
            break
        if selected and time.monotonic() < deadline:
            time.sleep(min(2, max(0.01, deadline - time.monotonic())))
            continue
        if not fallback_dispatched:
            result = subprocess.run(
                [
                    "gh", "workflow", "run", f"{requirements['workflow_name']}.yml",
                    "--ref", branch, "-f", f"expected_sha={head_sha}",
                ], capture_output=True, text=True, timeout=60,
            )
            if result.returncode != 0:
                raise CIVerificationError("canonical tests workflow dispatch failed")
            fallback_dispatched = True
            deadline = time.monotonic() + max(0, dispatch_timeout_seconds)
            continue
        raise CIVerificationError("exact-head CI run did not become observable")
    if selected is None:
        raise CIVerificationError("exact-head CI run did not become observable")
    event = selected.get("event")
    if event == "workflow_dispatch":
        source = "workflow_dispatch"
    elif event == "pull_request":
        source = "pull_request"
    selected_id = selected.get("databaseId")
    observed_ids = [
        run.get("databaseId")
        for run in all_runs
        if run.get("databaseId") is not None
    ]
    unsupported_ids = [
        run.get("databaseId") for run in all_runs
        if run.get("databaseId") is not None and run not in _acquirable_runs(all_runs)
    ]
    superseded_ids = [
        run.get("databaseId") for run in all_runs
        if run.get("databaseId") not in (None, selected_id) and run not in [
            candidate for candidate in all_runs if candidate.get("databaseId") in unsupported_ids
        ]
    ]
    return {
        "kind": "agent-orchestrator-ci-acquisition",
        "pr_number": int(pr_number),
        "head_sha": head_sha,
        "workflow_run_id": selected.get("databaseId"),
        "source": source,
        "status": "bound",
        "duplicate_run_ids": [value for value in superseded_ids],
        "observed_run_ids": [value for value in observed_ids],
        "selection_reason": _selection_reason(selected, all_runs),
        "superseded_run_ids": [value for value in superseded_ids],
        "unsupported_run_ids": [value for value in unsupported_ids],
        "fallback_dispatched": fallback_dispatched,
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
