"""Exact-head verification and dispatch for the canonical tests workflow.

This module separates exact-run acquisition from run completion:

* ``acquire_exact_run`` binds a trustworthy run ID without waiting for
  completion.  It returns once a natural or fallback run is observable
  on the exact head, regardless of whether the run is still queued or
  in progress.  This is the function worker finalize steps call.
* ``wait_for_run_completion`` boundedly polls the bound run ID for a
  terminal state, re-reading GitHub evidence, re-checking control, and
  rejecting stale bindings.  This is the function the explicit
  ``agent-ci-monitor`` path calls.
* ``acquire_exact_ci`` is the legacy combined function.  It now
  composes ``acquire_exact_run`` and ``wait_for_run_completion`` so
  external callers retain the old contract.
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import time
import uuid
from datetime import datetime
from pathlib import Path
from typing import Any, Callable


REQUIREMENTS_PATH = Path(__file__).with_name("ci_requirements.json")
ACTIVE_RUN_STATUSES = {"queued", "in_progress", "requested", "waiting", "pending"}
SUPPORTED_COMPLETED_CONCLUSIONS = {"success", "failure"}
FALLBACK_RUN_LOOKUP_SECONDS = 20
FALLBACK_STOP_RECONCILIATION_SECONDS = 30
# Must match the step name in .github/workflows/tests.yml.  Exact-head
# evidence requires this step to have executed successfully in every
# required job; a skipped or absent step is not acceptable proof that the
# job checked out and tested the claimed commit.
EXACT_HEAD_VERIFY_STEP = "Verify exact requested head"
TERMINAL_UNSUPPORTED_CONCLUSIONS = {
    "action_required",
    "cancelled",
    "neutral",
    "skipped",
    "stale",
    "startup_failure",
    "timed_out",
}


class CIVerificationError(RuntimeError):
    """Raised when exact-head CI evidence is missing or unsafe."""


class CIRunObservationTimeout(CIVerificationError):
    """No trustworthy exact-head run became observable during the bounded window."""


class CICompletionTimeout(CIVerificationError):
    """The bound run did not reach a terminal state during the bounded window."""


class CIStaleBinding(CIVerificationError):
    """The bound run no longer matches the expected head, branch, or identity."""


class CIControlStopped(CIVerificationError):
    """The live control state changed while the run was being observed."""

    def __init__(
        self,
        reason: str,
        *,
        ci_run_id: int | None = None,
        head_sha: str = "",
        observed_run: dict[str, Any] | None = None,
        dispatch_nonce: str | None = None,
    ) -> None:
        super().__init__(reason)
        self.reason = reason
        self.ci_run_id = ci_run_id
        self.head_sha = head_sha
        self.observed_run = observed_run or {}
        self.dispatch_nonce = dispatch_nonce


class ExactRunCandidates(list):
    """Valid exact-head runs plus rejected production identities."""

    def __init__(self, runs: list[dict[str, Any]], rejections: list[str]):
        super().__init__(runs)
        self.identity_rejections = rejections


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
            if "repository" in details:
                summary["repository"] = (details.get("repository") or {}).get("full_name")
            if "head_repository" in details:
                summary["headRepository"] = (details.get("head_repository") or {}).get("full_name")
            if "workflow_id" in details:
                summary["workflowId"] = details.get("workflow_id")
            if details.get("workflow_id") is not None:
                summary["workflowDatabaseId"] = details["workflow_id"]
            if "path" in details:
                summary["path"] = details.get("path")
            if "run_attempt" in details:
                summary["attempt"] = details.get("run_attempt")
            pull_requests = details.get("pull_requests")
            if isinstance(pull_requests, list):
                summary["pullRequestNumbers"] = sorted(
                    int(item["number"])
                    for item in pull_requests
                    if isinstance(item, dict) and str(item.get("number", "")).isdigit()
                )
    return summary


def verify_failed_run(
    run_id: int | str,
    expected_sha: str,
    expected_branch: str,
    expected_pr: int,
) -> dict[str, Any]:
    """Validate the exact failed canonical run used to prepare a repair."""

    requirements = load_requirements()
    run = run_info(run_id)
    if not run:
        raise CIVerificationError("workflow run is absent")
    try:
        expected_run_id = int(run_id)
    except (TypeError, ValueError) as exc:
        raise CIVerificationError("run_id_identity_invalid") from exc
    if run.get("databaseId") is None:
        raise CIVerificationError("run_id_identity_missing")
    if str(run.get("databaseId")) != str(expected_run_id):
        raise CIVerificationError("run_id_identity_mismatch")
    identity_failure = _validate_run_identity(
        run, expected_sha, expected_branch, int(expected_pr),
    )
    if identity_failure:
        raise CIVerificationError(f"CI identity rejected: {identity_failure}")
    if run.get("status") != "completed":
        raise CIVerificationError("failed workflow run is not completed")
    if run.get("conclusion") != "failure":
        raise CIVerificationError("workflow run is not a failure")
    if run.get("headSha") != expected_sha:
        raise CIVerificationError("failed workflow run head SHA does not match expected head")
    return {
        "workflow_name": requirements["workflow_name"],
        "workflow_run_id": expected_run_id,
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
    candidates, _ = _find_exact_runs_with_rejections(branch, head_sha, expected_pr)
    return candidates


def _find_exact_runs_with_rejections(
    branch: str, head_sha: str, expected_pr: int | None = None,
) -> tuple[list[dict[str, Any]], list[str]]:
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
        return [], []
    exact = [run for run in runs if run.get("headSha") == head_sha and run.get("headBranch") == branch]
    enriched = [run_info(run.get("databaseId")) or run for run in exact]
    candidates = []
    rejections = []
    for run in enriched:
        identity_failure = _validate_run_identity(run, head_sha, branch, expected_pr)
        if identity_failure:
            rejections.append(identity_failure)
            continue
        candidates.append(run)
    return ExactRunCandidates(candidates, sorted(set(rejections))), sorted(set(rejections))


def find_exact_run(branch: str, head_sha: str, expected_pr: int | None = None) -> dict[str, Any] | None:
    return select_canonical_run(find_exact_runs(branch, head_sha, expected_pr))


def _acquirable_runs(runs: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Exclude terminal runs that can never provide supported CI evidence.

    A trustworthy run is either still active (queued, in_progress, etc.)
    or completed with a supported conclusion (success, failure).  An
    unsupported terminal conclusion (cancelled, action_required, etc.)
    is not acquirable: the worker should dispatch a single fallback.
    """

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
    return _validate_run_identity(run, head_sha, branch, expected_pr) is None


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


def _acquisition_selection_reason(selected: dict[str, Any]) -> str:
    if selected.get("status") == "completed":
        if selected.get("event") == "pull_request":
            return "natural_completed_observed"
        return "fallback_completed_observed"
    if selected.get("event") == "pull_request":
        return "natural_active_observed"
    return "fallback_active_observed"


def _dispatch_fallback(
    requirements: dict[str, Any], branch: str, head_sha: str,
) -> int | None:
    dispatch_nonce = uuid.uuid4().hex
    dispatch_args = [
        "gh", "workflow", "run", f"{requirements['workflow_name']}.yml",
        "--ref", branch,
        "-f", f"expected_sha={head_sha}",
        "-f", f"dispatch_nonce={dispatch_nonce}",
    ]
    # Keep this check adjacent to the external mutation.  The workflow-level
    # shared concurrency group and emergency-stop cancellation provide the
    # cross-runner serialization; this is the final in-process authorization
    # gate before the dispatch syscall.
    if not control_is_live():
        raise CIControlStopped(
            "ci_control_stopped_before_fallback_dispatch",
            head_sha=head_sha,
            observed_run={
                "status": "not_dispatched",
                "conclusion": "",
                "headSha": head_sha,
                "headBranch": branch,
                "workflowName": requirements["workflow_name"],
                "dispatch_nonce": dispatch_nonce,
            },
            dispatch_nonce=dispatch_nonce,
        )
    result = subprocess.run(
        dispatch_args, capture_output=True, text=True, timeout=60,
    )
    if result.returncode != 0:
        raise CIVerificationError("canonical tests workflow dispatch failed")
    output = result.stdout if isinstance(result.stdout, str) else ""
    match = re.search(r"/actions/runs/(\d+)(?:$|[/?#])", output.strip())
    if match:
        return int(match.group(1))
    # ``gh workflow run`` returns the URL when available, but provider
    # visibility can lag or omit it.  Correlate the unique nonce below in
    # both production and fixture mode; production identity validation still
    # rejects any incomplete run once it becomes observable.
    if not isinstance(result.stdout, str):
        return None
    deadline = time.monotonic() + FALLBACK_RUN_LOOKUP_SECONDS
    control_stopped_during_lookup = False
    stop_deadline: float | None = None
    while True:
        # A workflow-dispatch response can precede provider run visibility.  A
        # stop during that interval must not turn into generic identity loss;
        # keep correlating by the unique nonce until the exact run is visible
        # so the caller can record the run-bound stop evidence.
        if not control_is_live():
            control_stopped_during_lookup = True
            if stop_deadline is None:
                stop_deadline = time.monotonic() + FALLBACK_STOP_RECONCILIATION_SECONDS
        listing = subprocess.run(
            [
                "gh", "run", "list", "--workflow", f"{requirements['workflow_name']}.yml",
                "--branch", branch, "--limit", "20",
                "--json", "databaseId,name,headSha,headBranch,event",
            ], capture_output=True, text=True, timeout=60,
        )
        if listing.returncode != 0:
            if control_stopped_during_lookup:
                time.sleep(1)
                continue
            raise CIVerificationError("fallback_run_identity_missing")
        try:
            candidates = json.loads(listing.stdout or "[]")
        except json.JSONDecodeError as exc:
            if control_stopped_during_lookup:
                time.sleep(1)
                continue
            raise CIVerificationError("fallback_run_identity_missing") from exc
        matches = [
            candidate for candidate in candidates
            if candidate.get("name") == f"tests-{dispatch_nonce}"
            and candidate.get("headSha") == head_sha
            and candidate.get("headBranch") == branch
            and candidate.get("event") == "workflow_dispatch"
            and _run_id(candidate) > 0
        ]
        if len(matches) == 1:
            return _run_id(matches[0])
        if len(matches) > 1:
            raise CIVerificationError("fallback_run_identity_ambiguous")
        # A stopped run gets a bounded reconciliation grace period.  If the
        # provider does not expose the nonce-bound run in that period, do not
        # fabricate a CI identity: fail closed with a typed reason.  When the
        # exact run is visible, the normal path above raises the typed stop
        # outcome with durable issue/PR/head/run evidence.
        if control_stopped_during_lookup:
            if stop_deadline is not None and time.monotonic() >= stop_deadline:
                raise CIControlStopped(
                    "ci_control_stopped:fallback_run_identity_missing",
                    head_sha=head_sha,
                    observed_run={
                        "status": "unknown",
                        "conclusion": "",
                        "headSha": head_sha,
                        "headBranch": branch,
                        "workflowName": requirements["workflow_name"],
                        "dispatch_nonce": dispatch_nonce,
                    },
                    dispatch_nonce=dispatch_nonce,
                )
        elif time.monotonic() >= deadline:
            raise CIVerificationError("fallback_run_identity_missing")
        time.sleep(1)


def acquire_exact_run(
    pr_number: int,
    branch: str,
    head_sha: str,
    observe_seconds: int = 20,
    dispatch_timeout_seconds: int = 60,
) -> dict[str, Any]:
    """Bind a trustworthy exact-head run ID without waiting for completion.

    The function returns as soon as one of the following is observable at
    the exact head:

    * a natural run that is queued, requested, waiting, pending, in
      progress, or completed with a supported conclusion (success or
      failure);
    * a single dispatched fallback run, after the bounded initial
      observation window has elapsed with no natural candidate.

    The function never dispatches a fallback while a trustworthy natural
    run is still active.  The returned ``workflow_run_id`` may be in any
    status, including ``queued`` and ``in_progress``; the caller is
    expected to delegate completion handling to ``wait_for_run_completion``.
    """

    requirements = load_requirements()
    deadline = time.monotonic() + max(0, observe_seconds)
    fallback_dispatched = False
    fallback_run_id: int | None = None
    all_runs: list[dict[str, Any]] = []
    identity_rejections: list[str] = []
    selected: dict[str, Any] | None = None
    while True:
        if not control_is_live():
            raise CIControlStopped("ci_control_stopped_during_run_acquisition")
        observed_runs = find_exact_runs(branch, head_sha, pr_number)
        identity_rejections = list(getattr(observed_runs, "identity_rejections", []))
        all_runs = []
        for run in observed_runs:
            identity_failure = _validate_run_identity(
                run, head_sha, branch, pr_number,
            )
            if identity_failure:
                identity_rejections.append(identity_failure)
            else:
                all_runs.append(run)
        identity_rejections = sorted(set(identity_rejections))
        if fallback_dispatched and fallback_run_id is not None:
            # Once a fallback was dispatched, a newer or otherwise more
            # attractive same-head run is not an authorization to switch
            # identities.  The nonce/URL-bound fallback is the only run that
            # may be accepted for this acquisition.
            selected = next(
                (run for run in all_runs if _run_id(run) == fallback_run_id),
                None,
            )
            if selected is None:
                observed_fallback = run_info(fallback_run_id)
                if observed_fallback and _run_id(observed_fallback) == fallback_run_id:
                    identity_failure = _validate_run_identity(
                        observed_fallback, head_sha, branch, pr_number,
                    )
                    if identity_failure:
                        raise CIVerificationError(
                            f"ci_stale_binding:{identity_failure}"
                        )
                    selected = observed_fallback
                    all_runs.append(observed_fallback)
            if selected is not None and selected.get("status") not in ACTIVE_RUN_STATUSES | {"completed"}:
                selected = None
        else:
            selected = select_canonical_run(all_runs)
        if selected is not None:
            if not control_is_live():
                raise CIControlStopped("ci_control_stopped_before_run_acceptance")
            break
        if time.monotonic() < deadline:
            time.sleep(min(2, max(0.01, deadline - time.monotonic())))
            continue
        if not fallback_dispatched:
            if not control_is_live():
                raise CIControlStopped("ci_control_stopped_before_fallback_dispatch")
            if identity_rejections:
                raise CIVerificationError(
                    f"ci_stale_binding:{identity_rejections[0]}"
                )
            dispatched_run_id = _dispatch_fallback(requirements, branch, head_sha)
            fallback_run_id = dispatched_run_id
            fallback_dispatched = True
            if not control_is_live():
                observed = {}
                if dispatched_run_id is not None:
                    candidate = run_info(dispatched_run_id)
                    if candidate and _run_id(candidate) == dispatched_run_id:
                        observed = candidate
                    else:
                        observed = {
                            "databaseId": dispatched_run_id,
                            "event": "workflow_dispatch",
                            "status": "dispatched",
                            "conclusion": "",
                            "headSha": head_sha,
                            "headBranch": branch,
                            "workflowName": requirements["workflow_name"],
                        }
                raise CIControlStopped(
                    "ci_control_stopped_after_fallback_dispatch",
                    ci_run_id=dispatched_run_id or _run_id(observed) or None,
                    head_sha=head_sha,
                    observed_run=observed,
                )
            deadline = time.monotonic() + max(0, dispatch_timeout_seconds)
            continue
        raise CIRunObservationTimeout(
            "exact-head CI run did not become observable"
        )
    if selected is None:
        raise CIRunObservationTimeout("exact-head CI run did not become observable")
    event = selected.get("event")
    if event == "workflow_dispatch":
        source = "workflow_dispatch"
    elif event == "pull_request":
        source = "pull_request"
    else:
        source = str(event or "unknown")
    selected_id = selected.get("databaseId")
    observed_ids = [
        run.get("databaseId")
        for run in all_runs
        if run.get("databaseId") is not None
    ]
    if selected_id not in observed_ids and selected_id is not None:
        observed_ids.append(selected_id)
    unsupported_ids = [
        run.get("databaseId") for run in all_runs
        if run.get("databaseId") is not None
        and not _is_acquirable_run(run)
    ]
    superseded_ids = [
        run.get("databaseId") for run in all_runs
        if run.get("databaseId") not in (None, selected_id)
        and _is_acquirable_run(run)
    ]
    return {
        "kind": "agent-orchestrator-ci-acquisition",
        "pr_number": int(pr_number),
        "head_sha": head_sha,
        "workflow_run_id": selected_id,
        "source": source,
        "status": "bound",
        "duplicate_run_ids": [value for value in superseded_ids if value is not None],
        "observed_run_ids": [value for value in observed_ids if value is not None],
        "selection_reason": _acquisition_selection_reason(selected),
        "superseded_run_ids": [value for value in superseded_ids if value is not None],
        "unsupported_run_ids": [value for value in unsupported_ids if value is not None],
        "fallback_dispatched": fallback_dispatched,
        "bound_status": selected.get("status"),
        "bound_conclusion": selected.get("conclusion"),
        "bound_branch": selected.get("headBranch"),
    }


def _is_acquirable_run(run: dict[str, Any]) -> bool:
    return run.get("status") in ACTIVE_RUN_STATUSES or (
        run.get("status") == "completed"
        and run.get("conclusion") in SUPPORTED_COMPLETED_CONCLUSIONS
    )


def _control_is_live() -> bool:
    """Return True iff the live orchestrator control is still enabled.

    Reads the same GitHub state the ``control_state`` module reads but
    never raises; an emergency stop or state outage maps to ``False`` so
    the caller can fail closed.  This is a module-level binding so tests
    can monkey-patch it.
    """

    try:
        sys.path.insert(
            0, str(Path(__file__).resolve().parent)
        )
        import control_state  # type: ignore[import-not-found]
    except Exception:
        return False
    try:
        control_state.require_live()
    except Exception:
        return False
    return True


# Module-level binding so tests can monkey-patch the control check.
control_is_live = _control_is_live


def _validate_run_identity(
    run: dict[str, Any],
    expected_head: str,
    expected_branch: str,
    pr_number: int | None,
) -> str | None:
    """Return a stable reason string if the run is unsafe, else None."""

    requirements = load_requirements()
    if run.get("headSha") is None:
        return "head_identity_missing"
    if run.get("headSha") != expected_head:
        return "head_moved"
    if not run.get("headBranch"):
        return "branch_identity_missing"
    if expected_branch and run.get("headBranch") != expected_branch:
        return "branch_moved"
    if "workflowName" not in run or not run.get("workflowName"):
        return "workflow_name_identity_missing"
    if run.get("workflowName") != requirements["workflow_name"]:
        return "workflow_changed"
    expected_repo = os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY")
    fixture_mode = os.environ.get("AGENT_CI_FIXTURE_MODE") == "true"
    production_identity = bool(
        not fixture_mode
        and (expected_repo or os.environ.get("GITHUB_ACTIONS") == "true")
    )
    if production_identity and "repository" not in run:
        return "repository_identity_missing"
    if production_identity and run.get("repository") != expected_repo:
        return "foreign_repository"
    if production_identity and "headRepository" not in run:
        return "head_repository_identity_missing"
    if production_identity and run.get("headRepository") != expected_repo:
        return "fork_head_repository"
    workflow_id = requirements.get("workflow_id")
    if production_identity and workflow_id is not None and "workflowDatabaseId" not in run:
        return "workflow_id_identity_missing"
    if production_identity and workflow_id is not None and run.get("workflowDatabaseId") != workflow_id:
        return "workflow_id_mismatch"
    workflow_path = requirements.get("workflow_path")
    if production_identity and workflow_path and "path" not in run:
        return "workflow_path_identity_missing"
    if production_identity and workflow_path and run.get("path") != workflow_path:
        return "workflow_path_mismatch"
    if production_identity and pr_number is not None and run.get("event") == "pull_request":
        numbers = _candidate_pr_numbers(run)
        if numbers is None:
            return "pr_binding_identity_missing"
        if int(pr_number) not in numbers:
            return "pr_mismatch"
    return None


def _run_is_superseded(
    bound_run: dict[str, Any],
    branch: str,
    head_sha: str,
    pr_number: int | None,
) -> bool:
    """True when a newer trustworthy exact-head run exists for the same head."""

    bound_id = _run_id(bound_run)
    if bound_id == 0:
        return True
    candidates = find_exact_runs(branch, head_sha, pr_number)
    for candidate in candidates:
        if not _is_acquirable_run(candidate):
            continue
        candidate_id = _run_id(candidate)
        if candidate_id == bound_id:
            continue
        if _timestamp(candidate) > _timestamp(bound_run):
            return True
    return False


def wait_for_run_completion(
    ci_run_id: int | str,
    *,
    expected_head: str,
    expected_branch: str,
    pr_number: int | None = None,
    completion_timeout_seconds: int = 1800,
    poll_seconds: int = 30,
    sleep: Callable[[float], None] = time.sleep,
    validator: Callable[[], str | None] | None = None,
) -> dict[str, Any]:
    """Bounded wait for the bound run to reach a terminal state.

    Re-reads GitHub evidence on every poll.  Re-checks live control
    state.  Rejects head movement, branch movement, foreign identity,
    fork head, PR closure, and supersession by a newer trustworthy
    exact-head run.  Distinguishes ``success``, ``failure``, and each
    unsupported terminal conclusion (``cancelled``, ``skipped``,
    ``action_required``, ``stale``, ``timed_out``, etc.) so the caller
    can map each to a typed reason code.

    When *validator* is provided, it is called on every poll cycle.  If
    it returns a non-``None`` string the wait stops with
    ``ci_stale_binding`` and that string as reason.

    Returns a stable dict with at least ``status``, ``reason``,
    ``ci_run_id``, ``head_sha``, and (when terminal) ``conclusion``.
    """

    requirements = load_requirements()
    if not expected_head:
        return {
            "status": "ci_stale_binding",
            "reason": "missing_expected_head",
            "ci_run_id": ci_run_id,
            "head_sha": expected_head or "",
        }
    deadline = time.monotonic() + max(0, completion_timeout_seconds)
    try:
        bound_id = int(ci_run_id)
    except (TypeError, ValueError):
        return {
            "status": "ci_stale_binding",
            "reason": "invalid_run_id",
            "ci_run_id": ci_run_id,
            "head_sha": expected_head,
        }

    def control_stopped(observed_run=None):
        observed_run = observed_run or run_info(bound_id) or {}
        return {
            "status": "ci_control_stopped",
            "reason": "control_emergency_stop_activated",
            "ci_run_id": bound_id,
            "head_sha": expected_head,
            "observed_status": str(observed_run.get("status") or "unknown"),
            "run": observed_run,
        }

    while True:
        if not control_is_live():
            return control_stopped()
        if validator is not None:
            validation_failure = validator()
            if validation_failure is not None:
                return {
                    "status": "ci_stale_binding",
                    "reason": validation_failure,
                    "ci_run_id": bound_id,
                    "head_sha": expected_head,
                }
        run = run_info(bound_id)
        if not run:
            return {
                "status": "ci_stale_binding",
                "reason": "run_absent",
                "ci_run_id": bound_id,
                "head_sha": expected_head,
            }
        if run.get("databaseId") is None:
            return {
                "status": "ci_stale_binding",
                "reason": "run_id_identity_missing",
                "ci_run_id": bound_id,
                "head_sha": expected_head,
            }
        if str(run.get("databaseId")) != str(bound_id):
            return {
                "status": "ci_stale_binding",
                "reason": "run_id_identity_mismatch",
                "ci_run_id": bound_id,
                "head_sha": expected_head,
            }
        identity_failure = _validate_run_identity(
            run, expected_head, expected_branch, pr_number
        )
        if identity_failure:
            return {
                "status": "ci_stale_binding",
                "reason": identity_failure,
                "ci_run_id": bound_id,
                "head_sha": expected_head,
            }
        if _run_is_superseded(run, expected_branch, expected_head, pr_number):
            return {
                "status": "ci_stale_binding",
                "reason": "run_superseded",
                "ci_run_id": bound_id,
                "head_sha": expected_head,
            }
        status = run.get("status")
        if status == "completed":
            conclusion = run.get("conclusion", "") or ""
            if conclusion in SUPPORTED_COMPLETED_CONCLUSIONS:
                mapped = "success" if conclusion == "success" else "failure"
                return {
                    "status": mapped,
                    "reason": f"ci_{mapped}",
                    "conclusion": conclusion,
                    "ci_run_id": bound_id,
                    "head_sha": expected_head,
                    "branch": run.get("headBranch", expected_branch),
                    "run": run,
                }
            return {
                "status": f"ci_terminal_failure",
                "reason": f"ci_terminal_{conclusion or 'unsupported'}",
                "conclusion": conclusion,
                "ci_run_id": bound_id,
                "head_sha": expected_head,
                "branch": run.get("headBranch", expected_branch),
                "run": run,
            }
        if time.monotonic() >= deadline:
            return {
                "status": "ci_completion_timeout",
                "reason": "run_still_active",
                "ci_run_id": bound_id,
                "head_sha": expected_head,
                "current_status": status,
            }
        if not control_is_live():
            return control_stopped(run)
        sleep(max(0, min(poll_seconds, deadline - time.monotonic())))


def acquire_exact_ci(
    pr_number: int,
    branch: str,
    head_sha: str,
    observe_seconds: int = 20,
    dispatch_timeout_seconds: int = 60,
    completion_timeout_seconds: int = 1800,
    poll_seconds: int = 30,
    final_validator: Callable[[], tuple[dict[str, Any] | None, str | None]] | None = None,
) -> dict[str, Any]:
    """Acquire canonical exact-head evidence and wait for completion.

    This is the legacy combined contract retained for external callers
    that have not yet split acquisition from completion.  It first binds
    a trustworthy run via :func:`acquire_exact_run` and then boundedly
    waits for that run to reach a terminal state via
    :func:`wait_for_run_completion`.  Even when the bound run is already
    completed at acquisition time, the completion path is re-entered so
    the final live PR/binding/control validator is not bypassed.
    """

    production_identity = (
        os.environ.get("AGENT_CI_FIXTURE_MODE") != "true"
        and bool(os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_ACTIONS") == "true")
    )
    if production_identity and final_validator is None:
        raise CIVerificationError("final_binding_validator_required")

    acquisition = acquire_exact_run(
        pr_number,
        branch,
        head_sha,
        observe_seconds=observe_seconds,
        dispatch_timeout_seconds=dispatch_timeout_seconds,
    )
    if acquisition.get("bound_status") == "completed":
        conclusion = acquisition.get("bound_conclusion")
        completion_status = (
            "success" if conclusion == "success"
            else "failure" if conclusion == "failure" else None
        )
        if completion_status in {"success", "failure"}:
            if final_validator is not None:
                _, final_binding_failure = final_validator()
                if final_binding_failure:
                    observed_run = {
                        "databaseId": acquisition["workflow_run_id"],
                        "status": "completed",
                        "conclusion": conclusion,
                        "headSha": head_sha,
                        "headBranch": branch,
                        "workflowName": load_requirements()["workflow_name"],
                    }
                    if final_binding_failure.startswith("ci_control_stopped:"):
                        raise CIControlStopped(
                            final_binding_failure,
                            ci_run_id=acquisition["workflow_run_id"],
                            head_sha=head_sha,
                            observed_run=observed_run,
                        )
                    raise CIStaleBinding(final_binding_failure)
            return {
                "kind": "agent-orchestrator-ci-acquisition",
                "pr_number": int(pr_number),
                "head_sha": head_sha,
                "workflow_run_id": acquisition["workflow_run_id"],
                "source": acquisition.get("source"),
                "status": "completed" if completion_status == "success" else completion_status,
                "conclusion": conclusion,
                "duplicate_run_ids": acquisition.get("duplicate_run_ids", []),
                "observed_run_ids": acquisition.get("observed_run_ids", []),
                "selection_reason": acquisition.get("selection_reason"),
                "superseded_run_ids": acquisition.get("superseded_run_ids", []),
                "unsupported_run_ids": acquisition.get("unsupported_run_ids", []),
                "fallback_dispatched": acquisition.get("fallback_dispatched", False),
            }
    return _finalize_acquisition_with_wait(
        acquisition,
        pr_number=pr_number,
        branch=branch,
        head_sha=head_sha,
        completion_timeout_seconds=completion_timeout_seconds,
        poll_seconds=poll_seconds,
        final_validator=final_validator,
    )


def _finalize_acquisition_with_wait(
    acquisition: dict[str, Any],
    *,
    pr_number: int,
    branch: str,
    head_sha: str,
    completion_timeout_seconds: int,
    poll_seconds: int,
    final_validator: Callable[[], tuple[dict[str, Any] | None, str | None]] | None = None,
) -> dict[str, Any]:
    completion = wait_for_run_completion(
        acquisition["workflow_run_id"],
        expected_head=head_sha,
        expected_branch=branch,
        pr_number=pr_number,
        completion_timeout_seconds=completion_timeout_seconds,
        poll_seconds=poll_seconds,
    )
    final_pr_snapshot = None
    if final_validator is not None:
        final_pr_snapshot, final_binding_failure = final_validator()
        if final_binding_failure:
            observed_run = completion.get("run") or {}
            if final_binding_failure.startswith("ci_control_stopped:"):
                raise CIControlStopped(
                    final_binding_failure,
                    ci_run_id=completion.get("ci_run_id") or acquisition["workflow_run_id"],
                    head_sha=head_sha,
                    observed_run=observed_run,
                )
            raise CIStaleBinding(final_binding_failure)
    if completion["status"] in {"success", "failure"}:
        completion_status = completion["status"]
    elif completion["status"] == "ci_completion_timeout":
        raise CICompletionTimeout(completion.get("reason") or "ci_completion_timeout")
    elif completion["status"] == "ci_control_stopped":
        raise CIControlStopped(
            completion.get("reason") or "ci_control_stopped",
            ci_run_id=completion.get("ci_run_id") or acquisition["workflow_run_id"],
            head_sha=completion.get("head_sha") or head_sha,
            observed_run=completion.get("run"),
        )
    elif completion["status"] == "ci_stale_binding":
        raise CIStaleBinding(completion.get("reason") or "ci_stale_binding")
    elif completion["status"].startswith("ci_terminal_"):
        raise CIVerificationError(
            f"unsupported CI conclusion: {completion.get('conclusion', 'unknown')}"
        )
    else:
        raise CIVerificationError(
            f"unexpected CI completion status: {completion['status']}"
        )
    if completion_status == "success":
        verify_exact_head_ci(
            pr_number, head_sha, acquisition["workflow_run_id"],
            pr_snapshot=final_pr_snapshot or {"headRefOid": head_sha, "headRefName": branch},
        )
    return {
        "kind": "agent-orchestrator-ci-acquisition",
        "pr_number": int(pr_number),
        "head_sha": head_sha,
        "workflow_run_id": acquisition["workflow_run_id"],
        "source": acquisition.get("source"),
        "status": "completed" if completion_status == "success" else completion_status,
        "conclusion": completion.get("conclusion"),
        "duplicate_run_ids": acquisition.get("duplicate_run_ids", []),
        "observed_run_ids": acquisition.get("observed_run_ids", []),
        "selection_reason": acquisition.get("selection_reason"),
        "superseded_run_ids": acquisition.get("superseded_run_ids", []),
        "unsupported_run_ids": acquisition.get("unsupported_run_ids", []),
        "fallback_dispatched": acquisition.get("fallback_dispatched", False),
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
    try:
        expected_run_id = int(workflow_run_id)
    except (TypeError, ValueError) as exc:
        raise CIVerificationError("run_id_identity_invalid") from exc
    actual_run_id = run.get("databaseId")
    if actual_run_id is None:
        raise CIVerificationError("run_id_identity_missing")
    if str(actual_run_id) != str(expected_run_id):
        raise CIVerificationError("run_id_identity_mismatch")
    provider_identity = {"repository", "headRepository", "workflowId", "path"}
    production_identity = (
        os.environ.get("AGENT_CI_FIXTURE_MODE") != "true"
        and bool(os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_ACTIONS") == "true")
    )
    if production_identity:
        if not isinstance(pr_snapshot, dict) or not pr_snapshot.get("headRefName"):
            raise CIVerificationError("pr_branch_identity_missing")
        identity_failure = _validate_run_identity(
            run,
            expected_sha,
            pr_snapshot["headRefName"],
            pr_number,
        )
        if identity_failure:
            raise CIVerificationError(
                f"CI run identity rejected: {identity_failure}"
            )
    elif provider_identity & run.keys() and not _candidate_matches(
        run,
        str(run.get("headBranch", "")),
        expected_sha,
        requirements,
        pr_number,
    ):
        raise CIVerificationError("CI run identity does not match the expected PR and repository")
    if not run.get("workflowName"):
        raise CIVerificationError("workflow_name_identity_missing")
    if run.get("workflowName") != requirements["workflow_name"]:
        raise CIVerificationError("workflow name does not match canonical tests workflow")
    if not run.get("headSha"):
        raise CIVerificationError("head_identity_missing")
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
    # Prove each required job actually checked out and verified the claimed
    # commit.  A pull_request run that skips the exact-head step (or omits
    # step evidence) is not acceptable exact-head proof; the orchestrator
    # must fall back to a workflow_dispatch that requires expected_sha.
    _require_exact_head_checkout_evidence(by_name, required)
    if pr_snapshot is not None and pr_snapshot.get("headRefOid") != expected_sha:
        raise CIVerificationError("PR head moved while verifying CI")

    return {
        "kind": "agent-orchestrator-ci-state",
        "pr_number": pr_number,
        "head_sha": expected_sha,
        "checked_out_sha": expected_sha,
        "workflow_run_id": run.get("databaseId") or workflow_run_id,
        "workflow_name": requirements["workflow_name"],
        "required_jobs": required,
        "successful_jobs": required,
        "exact_head_verify_step": EXACT_HEAD_VERIFY_STEP,
        "status": "success",
        "created_at": run.get("createdAt"),
        "updated_at": run.get("updatedAt"),
    }


def _require_exact_head_checkout_evidence(
    by_name: dict[str, Any],
    required: list[str],
) -> None:
    """Fail closed unless every required job executed exact-head verification.

    Canonical evidence must show that the job checked out and tested the
    claimed commit.  Missing step payloads, an absent verification step, a
    skipped step, or a non-success conclusion are all rejections.
    """

    absent_steps: list[str] = []
    skipped: list[str] = []
    failed: list[str] = []
    for name in required:
        job = by_name[name]
        steps = job.get("steps")
        if not isinstance(steps, list) or not steps:
            absent_steps.append(name)
            continue
        matches = [
            step
            for step in steps
            if isinstance(step, dict) and step.get("name") == EXACT_HEAD_VERIFY_STEP
        ]
        if not matches:
            absent_steps.append(name)
            continue
        step = matches[0]
        conclusion = step.get("conclusion")
        status = step.get("status")
        if conclusion == "skipped" or status == "skipped":
            skipped.append(name)
            continue
        if status != "completed" or conclusion != "success":
            failed.append(name)
    if absent_steps:
        raise CIVerificationError(
            f"exact-head verification step evidence is absent: {absent_steps}"
        )
    if skipped:
        raise CIVerificationError(
            f"exact-head verification step was skipped: {skipped}"
        )
    if failed:
        raise CIVerificationError(
            f"exact-head verification step did not succeed: {failed}"
        )


def dispatch_exact_ci(branch: str, head_sha: str, timeout_seconds: int = 60) -> dict[str, Any]:
    acquisition = acquire_exact_run(0, branch, head_sha, 0, timeout_seconds)
    acquisition["workflow_name"] = load_requirements()["workflow_name"]
    return acquisition


def main() -> None:
    try:
        if len(sys.argv) == 4 and sys.argv[1] == "dispatch":
            result = dispatch_exact_ci(sys.argv[2], sys.argv[3])
        elif len(sys.argv) == 6 and sys.argv[1] == "verify-failed-run":
            result = verify_failed_run(sys.argv[2], sys.argv[3], sys.argv[4], int(sys.argv[5]))
        elif len(sys.argv) == 5 and sys.argv[1] == "acquire":
            result = acquire_exact_ci(int(sys.argv[2]), sys.argv[3], sys.argv[4])
        elif len(sys.argv) == 5 and sys.argv[1] == "acquire-run":
            result = acquire_exact_run(int(sys.argv[2]), sys.argv[3], sys.argv[4])
        else:
            raise SystemExit(
                "Usage: ci_verifier.py <dispatch branch sha|verify-failed-run run-id sha branch pr|acquire pr branch sha|acquire-run pr branch sha>"
            )
        print(json.dumps(result, sort_keys=True))
    except CIControlStopped as exc:
        observed = exc.observed_run
        ci_run_id = exc.ci_run_id if exc.ci_run_id is not None else 0
        if exc.dispatch_nonce:
            observed = dict(observed)
            observed["dispatch_nonce"] = exc.dispatch_nonce
        print(json.dumps({
            "status": "ci_control_stopped",
            "reason": str(exc),
            "ci_run_id": ci_run_id,
            "workflow_run_id": ci_run_id,
            "run_identity": "exact" if ci_run_id else "unavailable",
            "dispatch_nonce": exc.dispatch_nonce,
            "head_sha": exc.head_sha,
            "observed_status": str(observed.get("status") or "unknown"),
            "run": observed,
        }, sort_keys=True))
    except CIVerificationError as exc:
        print(f"CI_VERIFICATION_ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
