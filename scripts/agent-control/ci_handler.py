"""Process canonical tests workflow completions with exact-head evidence."""

from __future__ import annotations

import json
import os
import re
import sys

import ci_verifier
import state_manager as sm


MAX_REPAIR_ATTEMPTS = int(os.environ.get("AGENT_MAX_REPAIR_ATTEMPTS", "2"))
TERMINAL_UNSUPPORTED_CONCLUSIONS = {
    "action_required",
    "cancelled",
    "neutral",
    "skipped",
    "stale",
    "startup_failure",
    "timed_out",
}


def parse_workflow_run_event(event_path):
    with open(event_path, encoding="utf-8") as handle:
        event = json.load(handle)
    workflow_run = event.get("workflow_run", {})
    pull_requests = workflow_run.get("pull_requests", [])
    pr_data = pull_requests[0] if pull_requests else {}
    return {
        "workflow_name": workflow_run.get("name") or workflow_run.get("workflow_name", ""),
        "conclusion": workflow_run.get("conclusion"),
        "status": workflow_run.get("status"),
        "head_branch": workflow_run.get("head_branch", ""),
        "head_sha": workflow_run.get("head_sha", ""),
        "run_id": workflow_run.get("id"),
        "run_url": workflow_run.get("html_url", ""),
        "pr_number": pr_data.get("number"),
        "pr_head_sha": pr_data.get("head", {}).get("sha") if isinstance(pr_data, dict) else None,
        "repository": (event.get("repository") or {}).get("full_name", ""),
        "head_repository": (workflow_run.get("head_repository") or {}).get("full_name", ""),
        "workflow_id": workflow_run.get("workflow_id"),
        "workflow_path": workflow_run.get("path"),
    }


def _run_for_event(info):
    run = ci_verifier.run_info(info["run_id"])
    if not run:
        return None, "workflow_run_unavailable"
    if run.get("databaseId") is None:
        return None, "run_id_identity_missing"
    if str(run.get("databaseId")) != str(info.get("run_id")):
        return None, "run_id_identity_mismatch"
    requirements = ci_verifier.load_requirements()
    identity_failure = ci_verifier._validate_run_identity(
        run, info["head_sha"], info["head_branch"], info.get("pr_number"),
    )
    if identity_failure:
        return None, identity_failure
    if run.get("status") != "completed":
        return None, "workflow_run_not_completed"
    if info.get("workflow_id") is not None and run.get("workflowId") not in (None, info["workflow_id"]):
        return None, "workflow_id_mismatch"
    if info.get("workflow_path") and run.get("path") not in (None, info["workflow_path"]):
        return None, "workflow_path_mismatch"
    provider_identity = {"repository", "headRepository", "workflowId", "path"}
    production_run = (
        os.environ.get("GITHUB_ACTIONS") == "true"
        and os.environ.get("AGENT_CI_FIXTURE_MODE") != "true"
    )
    if (provider_identity & run.keys() or production_run) and not ci_verifier._candidate_matches(
        run,
        info["head_branch"],
        info["head_sha"],
        requirements,
        info.get("pr_number"),
    ):
        return None, "workflow_identity_or_head_mismatch"
    return run, None


def get_failed_jobs(run_id):
    run = ci_verifier.run_info(run_id)
    if not run:
        return []
    failed = []
    for job in run.get("jobs", []):
        if job.get("conclusion") != "success":
            failed.append({
                "name": job.get("name", ""),
                "failed_steps": [
                    step.get("name", "")
                    for step in job.get("steps", [])
                    if step.get("conclusion") not in ("success", "skipped")
                ],
            })
    return failed


def _find_issue_for_pr(pr_number):
    pr_info = sm.get_pr_info(pr_number)
    if not pr_info:
        return None
    marker = sm.parse_binding_marker(pr_info.get("body", ""))
    if not marker:
        return None
    issue = marker.get("issue_number")
    return int(issue) if isinstance(issue, int) or str(issue).isdigit() else None


def _find_pr_for_run(info):
    """Find the open PR for workflow_dispatch runs with no pull_requests payload."""
    raw = sm._gh(
        "pr", "list", "--state", "open", "--limit", "100",
        "--json", "number,headRefName,headRefOid",
    )
    if raw is None:
        return None
    try:
        candidates = json.loads(raw)
    except json.JSONDecodeError:
        return None
    for candidate in candidates:
        if (
            candidate.get("headRefName") == info["head_branch"]
            and candidate.get("headRefOid") == info["head_sha"]
        ):
            return int(candidate["number"])
    return None


def _record_ci(issue, pr, sha, run_id, status, run, repair_count=0):
    requirements = ci_verifier.load_requirements()
    jobs = run.get("jobs", []) if run else []
    successful = sorted(job.get("name") for job in jobs if job.get("conclusion") == "success")
    extra = {
        "workflow_name": requirements["workflow_name"],
        "required_jobs": requirements["required_jobs"],
        "successful_jobs": successful,
        "repair_count": repair_count,
        "workflow_run_id": run_id,
    }
    if not sm.record_ci_state(issue, pr, sha, run_id, status, extra=extra):
        raise RuntimeError("unable to persist exact-head CI state")


def _is_duplicate_exact_head_run(issue, pr, sha, run_id, branch):
    if run_id is None:
        return True
    try:
        run_number = int(run_id)
    except (TypeError, ValueError):
        return True
    acquisition = sm.read_ci_acquisition(issue, pr, sha)
    exact_runs = ci_verifier._acquirable_runs(ci_verifier.find_exact_runs(branch, sha, pr))
    selected = ci_verifier.select_canonical_run(exact_runs)
    if selected is not None:
        try:
            selected_number = int(selected.get("databaseId", 0))
        except (TypeError, ValueError):
            return True
        if selected_number != run_number:
            return True
    try:
        acquired_number = int(acquisition.get("workflow_run_id", 0)) if acquisition else None
    except (TypeError, ValueError):
        return True
    if selected is None and acquired_number is not None and acquired_number != run_number:
        return True
    state = sm.read_ci_state(issue)
    if state and state.get("pr_number") == int(pr) and state.get("head_sha") == sha:
        previous_run = state.get("workflow_run_id") or state.get("ci_run_id")
        status = str(state.get("status", ""))
        if str(previous_run) == str(run_number) and (
            status in {"success", "invalidated"}
            or status.startswith("terminal_")
        ):
            return True
        if str(previous_run) == str(run_number) and status.startswith("failure_repair_"):
            return True
    if acquired_number == run_number:
        return False
    return False


def _persist_canonical_acquisition(issue, pr, sha, branch, event_run):
    """Persist the current exact-head candidate set before acting on an event.

    Completion events can arrive out of order.  Re-reading the candidate set
    here makes the durable acquisition record reflect the same canonical run
    that gates repair/review decisions, including unsupported terminal runs.
    """

    requirements = ci_verifier.load_requirements()
    candidates = [
        run for run in ci_verifier.find_exact_runs(branch, sha, pr)
        if ci_verifier._candidate_matches(run, branch, sha, requirements, pr)
    ]
    acquirable = ci_verifier._acquirable_runs(candidates)
    selected = ci_verifier.select_canonical_run(acquirable)
    unsupported_ids = [
        run.get("databaseId") for run in candidates
        if run.get("databaseId") is not None and run not in acquirable
    ]
    if selected is None:
        selected = event_run
        status = "unsupported"
    else:
        status = "bound"
    if not selected or selected.get("databaseId") is None:
        return False
    selected_id = selected.get("databaseId")
    observed_ids = [
        run.get("databaseId") for run in candidates
        if run.get("databaseId") is not None
    ]
    if selected_id not in observed_ids:
        observed_ids.append(selected_id)
    superseded_ids = [
        run_id for run_id in observed_ids
        if run_id != selected_id and run_id not in unsupported_ids
    ]
    source = "workflow_dispatch" if selected.get("event") == "workflow_dispatch" else "pull_request"
    previous_acquisition = sm.read_ci_acquisition(issue, pr, sha)
    metadata = {
        "status": status,
        "observed_run_ids": observed_ids,
        "selection_reason": (
            ci_verifier._selection_reason(selected, acquirable)
            if status == "bound" else "unsupported_terminal_observed"
        ),
        "superseded_run_ids": superseded_ids,
        "unsupported_run_ids": unsupported_ids,
        "fallback_dispatched": bool((previous_acquisition or {}).get("fallback_dispatched", False)),
    }
    if not sm.record_ci_acquisition(
        issue, pr, sha, selected_id, source, superseded_ids, metadata=metadata
    ):
        raise RuntimeError("unable to persist canonical exact-head acquisition")
    return True


def _reselect_unsupported(issue, pr, sha, branch, event_run):
    """Give an unsupported terminal run one bounded exact-head fallback chance.

    The workflow-run event itself cannot authorise repair or review.  If the
    bounded acquisition finds a supported replacement, persist it and leave
    the replacement's own completion event to drive the next action.  Only an
    exhausted reselection attempt is allowed to become a terminal block.
    """

    previous = sm.read_ci_acquisition(issue, pr, sha)
    if previous and previous.get("fallback_dispatched"):
        candidates = ci_verifier.find_exact_runs(branch, sha, pr)
        acquirable = ci_verifier._acquirable_runs(candidates)
        selected = ci_verifier.select_canonical_run(acquirable)
        if not selected:
            return None
        selected_id = selected.get("databaseId")
        acquisition = {
            "workflow_run_id": selected_id,
            "source": "workflow_dispatch" if selected.get("event") == "workflow_dispatch" else "pull_request",
            "status": "bound",
            "selection_reason": ci_verifier._selection_reason(selected, acquirable),
            "observed_run_ids": [
                run.get("databaseId") for run in candidates if run.get("databaseId") is not None
            ],
            "superseded_run_ids": [
                run.get("databaseId") for run in candidates
                if run.get("databaseId") not in (None, selected_id)
                and run in acquirable
            ],
            "unsupported_run_ids": [
                run.get("databaseId") for run in candidates
                if run.get("databaseId") is not None and run not in acquirable
            ],
            "fallback_dispatched": True,
            "duplicate_run_ids": [],
        }
    else:
        try:
            acquisition = ci_verifier.acquire_exact_ci(
                pr, branch, sha, observe_seconds=0, dispatch_timeout_seconds=60
            )
        except ci_verifier.CIVerificationError:
            return None
    try:
        selected_id = int(acquisition["workflow_run_id"])
        event_id = int(event_run.get("databaseId", 0))
    except (KeyError, TypeError, ValueError):
        return None
    if selected_id == event_id:
        return None
    metadata = {
        "status": acquisition.get("status", "bound"),
        "observed_run_ids": acquisition.get("observed_run_ids", []),
        "selection_reason": acquisition.get("selection_reason", ""),
        "superseded_run_ids": acquisition.get("superseded_run_ids", []),
        "unsupported_run_ids": acquisition.get("unsupported_run_ids", []),
        "fallback_dispatched": acquisition.get("fallback_dispatched", False),
    }
    if not sm.record_ci_acquisition(
        issue,
        pr,
        sha,
        selected_id,
        acquisition.get("source", "workflow_dispatch"),
        acquisition.get("duplicate_run_ids", []),
        metadata=metadata,
    ):
        raise RuntimeError("unable to persist reselected exact-head acquisition")
    return acquisition


def _state_unavailable_result(issue, pr, sha, run_id, detail):
    safe_detail = re.sub(r"[^a-z0-9_.:/()-]+", "_", str(detail).lower())[:180].strip("_")
    reason = "ci_state_unavailable"
    if safe_detail:
        reason = f"{reason}:{safe_detail}"
    return {
        "action": "blocked",
        "pr_number": pr,
        "issue_number": issue,
        "head_sha": sha,
        "ci_run_id": run_id,
        "terminal_status": "terminal_ci_state_unavailable",
        "observed_status": "unknown",
        "reason": reason,
    }


def _typed_ci_verification_reason(error):
    """Keep a verifier's stable typed reason in terminal evidence."""
    message = str(error)
    prefix = "CI run identity rejected: "
    candidate = message[len(prefix):] if message.startswith(prefix) else message
    if re.fullmatch(r"[a-z0-9_]+", candidate):
        return candidate
    return "exact_head_ci_rejected"


def _noop_result(issue, pr_number, head_sha, ci_run_id, reason):
    """Return a bound no-op that the monitor can terminalize and release."""
    return {
        "action": "noop",
        "pr_number": int(pr_number),
        "issue_number": int(issue),
        "head_sha": head_sha,
        "ci_run_id": int(ci_run_id),
        "terminal_status": "terminal_noop",
        "observed_status": "unknown",
        "reason": f"ci_noop:{reason}",
    }


def _record_ci_noop(issue, pr_number, head_sha, ci_run_id, run, reason):
    """Persist a bound no-op before the monitor performs compensation."""
    return _record_ci_terminal(
        issue,
        pr_number,
        head_sha,
        ci_run_id,
        run,
        "noop",
        action="noop",
        reason=f"ci_noop:{reason}",
    )


def _record_ci_terminal(
    issue,
    pr_number,
    head_sha,
    ci_run_id,
    run,
    terminal_code,
    *,
    action="blocked",
    reason=None,
):
    """Persist a typed terminal CI result before capacity compensation."""
    run = run or {}
    observed_status = str(run.get("status") or "unknown")
    terminal_status = f"terminal_{terminal_code}"
    evidence_reason = reason or terminal_code
    requirements = ci_verifier.load_requirements()
    extra = {
        "workflow_name": requirements["workflow_name"],
        "required_jobs": requirements["required_jobs"],
        "successful_jobs": sorted(
            job.get("name") for job in run.get("jobs", [])
            if isinstance(job, dict)
            and job.get("conclusion") == "success"
            and isinstance(job.get("name"), str)
        ),
        "workflow_run_id": ci_run_id,
        "observed_conclusion": str(run.get("conclusion") or ""),
        "run_attempt": run.get("attempt"),
    }
    if not sm.record_ci_terminal_state(
        issue, pr_number, head_sha, ci_run_id, terminal_status,
        observed_status, evidence_reason, extra=extra,
    ):
        return _state_unavailable_result(
            issue, pr_number, head_sha, ci_run_id,
            "unable to persist typed terminal CI evidence",
        )
    return {
        "action": action,
        "pr_number": int(pr_number),
        "issue_number": int(issue),
        "head_sha": head_sha,
        "ci_run_id": int(ci_run_id),
        "terminal_status": terminal_status,
        "observed_status": observed_status,
        "reason": evidence_reason,
    }


def process_ci_completion(event_path):
    info = parse_workflow_run_event(event_path)
    expected_repo = os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY", "")
    if not expected_repo or info["repository"] != expected_repo:
        return {"action": "noop", "reason": "foreign_or_untrusted_repository"}
    if info["head_repository"] and info["head_repository"] != expected_repo:
        return {"action": "noop", "reason": "fork_or_foreign_head_repository"}
    supported_conclusions = {"success", "failure"} | TERMINAL_UNSUPPORTED_CONCLUSIONS
    if info["status"] != "completed" or info["conclusion"] not in supported_conclusions:
        return {"action": "noop", "reason": "non_terminal_or_unsupported_conclusion"}
    pr_number = info["pr_number"] or _find_pr_for_run(info)
    if not pr_number:
        return {"action": "noop", "reason": "no_pr"}
    pr_number = int(pr_number)
    issue_number = _find_issue_for_pr(pr_number)
    if not issue_number:
        return {"action": "noop", "reason": "no_canonical_issue_binding"}
    run, run_failure = _run_for_event(info)
    if not run:
        return _record_ci_terminal(
            issue_number, pr_number, info["head_sha"], info["run_id"],
            {"status": info["status"], "conclusion": info["conclusion"], "headBranch": info["head_branch"]},
            "ci_stale_binding", action="stale",
            reason=f"ci_stale_binding:{run_failure or 'workflow_identity_or_head_mismatch'}",
        )
    pr_info = sm.get_pr_info(pr_number)
    if not pr_info or pr_info.get("state") != "OPEN":
        return _record_ci_noop(
            issue_number, pr_number, info["head_sha"], info["run_id"], run,
            "pr_not_open",
        )
    current_head = pr_info.get("headRefOid", "")
    expected_head = info["pr_head_sha"] or info["head_sha"]
    expected_branch = pr_info.get("headRefName", "")
    if not expected_branch:
        return _record_ci_terminal(
            issue_number, pr_number, expected_head, info["run_id"], run,
            "ci_stale_binding", action="stale",
            reason="ci_stale_binding:pr_branch_identity_missing",
        )
    if (
        current_head != expected_head
        or run.get("headSha") != expected_head
        or info["head_branch"] != expected_branch
        or run.get("headBranch") != expected_branch
    ):
        return _record_ci_terminal(
            issue_number, pr_number, expected_head, info["run_id"], run,
            "ci_stale_binding", action="stale", reason="ci_stale_binding:head_sha_mismatch",
        )
    binding_ok, binding_reason = sm.verify_issue_pr_binding(issue_number, pr_number, current_head)
    if not binding_ok:
        return _record_ci_noop(
            issue_number, pr_number, current_head, info["run_id"], run,
            f"binding_rejected:{binding_reason}",
        )
    try:
        duplicate = _is_duplicate_exact_head_run(
            issue_number, pr_number, current_head, info["run_id"], info["head_branch"]
        )
    except sm.StateUnavailableError as exc:
        return _state_unavailable_result(
            issue_number, pr_number, current_head, info["run_id"], str(exc)
        )
    if duplicate:
        return _record_ci_noop(
            issue_number, pr_number, current_head, info["run_id"], run,
            "duplicate_exact_head_run",
        )

    try:
        _persist_canonical_acquisition(
            issue_number, pr_number, current_head, info["head_branch"], run
        )
    except (RuntimeError, sm.StateUnavailableError) as exc:
        return _state_unavailable_result(
            issue_number, pr_number, current_head, info["run_id"], str(exc)
        )

    if info["conclusion"] in TERMINAL_UNSUPPORTED_CONCLUSIONS:
        try:
            replacement = _reselect_unsupported(
                issue_number, pr_number, current_head, expected_branch, run
            )
        except (RuntimeError, sm.StateUnavailableError) as exc:
            return _state_unavailable_result(
                issue_number, pr_number, current_head, info["run_id"], str(exc)
            )
        if replacement is not None:
            return {
                "action": "noop",
                "pr_number": pr_number,
                "issue_number": issue_number,
                "head_sha": current_head,
                "ci_run_id": replacement.get("workflow_run_id"),
                "reason": "ci_reselected_after_unsupported_run",
            }
        conclusion = info["conclusion"]
        try:
            _record_ci(
                issue_number,
                pr_number,
                current_head,
                info["run_id"],
                f"terminal_{conclusion}",
                run,
            )
        except RuntimeError as exc:
            return _state_unavailable_result(
                issue_number, pr_number, current_head, info["run_id"], str(exc)
            )
        if not ci_verifier.control_is_live():
            return _record_ci_terminal(
                issue_number, pr_number, current_head, info["run_id"], run,
                "ci_control_stopped", reason="ci_control_stopped:after_ci_state_persistence",
            )
        return _record_ci_terminal(
            issue_number, pr_number, current_head, info["run_id"], run,
            conclusion, reason=f"ci_terminal_{conclusion}",
        )

    if info["conclusion"] == "success":
        try:
            evidence = ci_verifier.verify_exact_head_ci(pr_number, current_head, info["run_id"], pr_info)
        except ci_verifier.CIVerificationError as exc:
            return _record_ci_terminal(
                issue_number, pr_number, current_head, info["run_id"], run,
                "ci_stale_binding", action="stale",
                reason=f"ci_stale_binding:{_typed_ci_verification_reason(exc)}",
            )
        try:
            previous_state = sm.read_ci_state(issue_number)
        except sm.StateUnavailableError as exc:
            return _state_unavailable_result(
                issue_number, pr_number, current_head, info["run_id"], str(exc)
            )
        repair_count = int((previous_state or {}).get("extra", {}).get("repair_count", 0))
        try:
            _record_ci(
                issue_number, pr_number, current_head, info["run_id"],
                "success", run, repair_count,
            )
        except RuntimeError as exc:
            return _state_unavailable_result(
                issue_number, pr_number, current_head, info["run_id"], str(exc)
            )
        if not ci_verifier.control_is_live():
            return _record_ci_terminal(
                issue_number, pr_number, current_head, info["run_id"], run,
                "ci_control_stopped", reason="ci_control_stopped:after_ci_state_persistence",
            )
        labels = sm.get_issue_labels_checked(issue_number)
        if labels is None:
            return _state_unavailable_result(
                issue_number, pr_number, current_head, info["run_id"], "Issue label state is unavailable"
            )
        action = "merge_ready" if sm.LABEL_REVIEW_PASSED in labels else "trigger_review"
        return {
            "action": action,
            "pr_number": pr_number,
            "issue_number": issue_number,
            "head_sha": current_head,
            "ci_run_id": info["run_id"],
            "ci_evidence": evidence,
            "reason": "ci_green",
        }

    try:
        state = sm.read_ci_state(issue_number)
    except sm.StateUnavailableError as exc:
        return _state_unavailable_result(
            issue_number, pr_number, current_head, info["run_id"], str(exc)
        )
    repair_count = int((state or {}).get("extra", {}).get("repair_count", 0))
    next_count = repair_count + 1
    try:
        _record_ci(
            issue_number, pr_number, current_head, info["run_id"],
            f"failure_repair_{repair_count}", run, repair_count,
        )
    except RuntimeError as exc:
        return _state_unavailable_result(
            issue_number, pr_number, current_head, info["run_id"], str(exc)
        )
    if not ci_verifier.control_is_live():
        return _record_ci_terminal(
            issue_number, pr_number, current_head, info["run_id"], run,
            "ci_control_stopped", reason="ci_control_stopped:after_ci_state_persistence",
        )
    if next_count > MAX_REPAIR_ATTEMPTS:
        return _record_ci_terminal(
            issue_number, pr_number, current_head, info["run_id"], run,
            "max_repairs_exceeded", reason=f"max_repairs_exceeded:{next_count}/{MAX_REPAIR_ATTEMPTS}",
        )
    return {
        "action": "trigger_repair",
        "pr_number": pr_number,
        "issue_number": issue_number,
        "head_sha": current_head,
        "ci_run_id": info["run_id"],
        "repair_count": next_count,
        "reason": "ci_failure",
    }


def _make_poll_validator(issue, pr_number, head_sha, branch):
    """Return a validator callback for wait_for_run_completion.

    Re-reads the current PR state, exact head, and Issue/PR binding on
    every call.  Returns None when the state is still current, or a
    string reason when the binding is stale.
    """

    def validate() -> str | None:
        pr_info = sm.get_pr_info(pr_number)
        if not pr_info:
            return "pr_unavailable"
        if pr_info.get("state") != "OPEN":
            return "pr_closed"
        if pr_info.get("headRefOid") != head_sha:
            return "pr_head_moved"
        if not pr_info.get("headRefName"):
            return "pr_branch_identity_missing"
        if pr_info.get("headRefName") != branch:
            return "pr_branch_changed"
        binding_ok, binding_reason = sm.verify_issue_pr_binding(issue, pr_number, head_sha)
        if not binding_ok:
            return f"binding_rejected:{binding_reason}"
        return None

    return validate


def _refresh_terminal_binding(issue, pr_number, head_sha, expected_branch):
    """Re-read all authoritative bindings immediately before terminal action."""
    if not ci_verifier.control_is_live():
        return None, "ci_control_stopped:control_emergency_stop_activated"
    pr_info = sm.get_pr_info(pr_number)
    if not pr_info:
        return None, "ci_stale_binding:pr_unavailable"
    if pr_info.get("state") != "OPEN":
        return pr_info, "ci_stale_binding:pr_closed"
    if pr_info.get("headRefOid") != head_sha:
        return pr_info, "ci_stale_binding:pr_head_moved"
    if not pr_info.get("headRefName"):
        return pr_info, "ci_stale_binding:pr_branch_identity_missing"
    if pr_info.get("headRefName") != expected_branch:
        return pr_info, "ci_stale_binding:pr_branch_changed"
    binding_ok, binding_reason = sm.verify_issue_pr_binding(issue, pr_number, head_sha)
    if not binding_ok:
        return pr_info, f"ci_stale_binding:binding_rejected:{binding_reason}"
    if not ci_verifier.control_is_live():
        return pr_info, "ci_control_stopped:control_emergency_stop_activated"
    return pr_info, None


def process_ci_dispatch(issue_number, pr_number, head_sha, ci_run_id):
    """Process a CI completion from explicit workflow_dispatch monitor inputs.

    This is the dispatch-monitor equivalent of process_ci_completion() for the
    trusted monitor path (agent-worker dispatch -> agent-ci-monitor via
    workflow_dispatch).  It derives all state from the GitHub API directly
    rather than from a workflow_run event payload.  When the bound run is
    still active at the moment of dispatch, this function boundedly waits
    for it to reach a terminal state via
    :func:`ci_verifier.wait_for_run_completion` before continuing.

    Unlike process_ci_completion(), every terminal outcome including
    completion timeout is explicitly recorded as a typed CI state and
    produces an action the workflow handles.
    """
    expected_repo = os.environ.get("AGENT_REPO") or os.environ.get("GITHUB_REPOSITORY", "")
    if not expected_repo:
        return _noop_result(issue_number, pr_number, head_sha, ci_run_id, "repository_unavailable")

    initial_run = ci_verifier.run_info(ci_run_id)
    if not initial_run:
        return _noop_result(issue_number, pr_number, head_sha, ci_run_id, "ci_run_not_found")
    if initial_run.get("databaseId") is None:
        return _record_ci_terminal(
            issue_number, pr_number, head_sha, ci_run_id, initial_run,
            "ci_stale_binding", action="stale",
            reason="ci_stale_binding:run_id_identity_missing",
        )
    if str(initial_run.get("databaseId")) != str(ci_run_id):
        return _record_ci_terminal(
            issue_number, pr_number, head_sha, ci_run_id, initial_run,
            "ci_stale_binding", action="stale",
            reason="ci_stale_binding:run_id_identity_mismatch",
        )

    pr_info = sm.get_pr_info(pr_number)
    if not pr_info:
        return _noop_result(issue_number, pr_number, head_sha, ci_run_id, "pr_unavailable")
    issue = _find_issue_for_pr(pr_number)
    if not issue or issue != int(issue_number):
        return _noop_result(issue_number, pr_number, head_sha, ci_run_id, "issue_binding_mismatch")

    if pr_info.get("state") != "OPEN":
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, initial_run,
            "ci_stale_binding", action="stale", reason="ci_stale_binding:pr_closed",
        )
    if pr_info.get("headRefOid") != head_sha:
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, initial_run,
            "ci_stale_binding", action="stale", reason="ci_stale_binding:pr_head_mismatch",
        )
    expected_branch = pr_info.get("headRefName", "")
    if not expected_branch:
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, initial_run,
            "ci_stale_binding", action="stale",
            reason="ci_stale_binding:pr_branch_identity_missing",
        )
    identity_failure = ci_verifier._validate_run_identity(
        initial_run, head_sha, expected_branch, int(pr_number),
    )
    if identity_failure:
        if identity_failure in {"foreign_repository", "fork_head_repository"}:
            return _record_ci_noop(
                issue, pr_number, head_sha, ci_run_id, initial_run, identity_failure,
            )
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, initial_run,
            "ci_stale_binding", action="stale",
            reason=f"ci_stale_binding:{identity_failure}",
        )

    initial_binding_ok, initial_binding_reason = sm.verify_issue_pr_binding(issue, pr_number, head_sha)
    if not initial_binding_ok:
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, initial_run,
            "ci_stale_binding", action="stale",
            reason=f"ci_stale_binding:initial_binding_rejected:{initial_binding_reason}",
        )

    run = initial_run
    completion = None
    if run.get("status") != "completed":
        poll_validator = _make_poll_validator(issue, pr_number, head_sha, expected_branch)
        completion = ci_verifier.wait_for_run_completion(
            ci_run_id,
            expected_head=head_sha,
            expected_branch=expected_branch,
            pr_number=pr_number,
            validator=poll_validator,
        )
        run = completion.get("run") or ci_verifier.run_info(ci_run_id) or {}

    refreshed_pr, refresh_failure = _refresh_terminal_binding(
        issue, pr_number, head_sha, expected_branch,
    )
    if refresh_failure:
        if refresh_failure.startswith("ci_control_stopped:"):
            return _record_ci_terminal(
                issue, pr_number, head_sha, ci_run_id, run,
                "ci_control_stopped", reason=refresh_failure,
            )
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run,
            "ci_stale_binding", action="stale", reason=refresh_failure,
        )
    pr_info = refreshed_pr

    if completion is not None and completion["status"] == "ci_completion_timeout":
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run,
            "ci_completion_timeout", reason="ci_completion_timeout",
        )
    if completion is not None and completion["status"] == "ci_control_stopped":
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run,
            "ci_control_stopped",
            reason=completion.get("reason") or "ci_control_stopped",
        )
    if completion is not None and completion["status"] == "ci_stale_binding":
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run,
            "ci_stale_binding", action="stale",
            reason=f"ci_stale_binding:{completion.get('reason')}",
        )
    if completion is not None and completion["status"] not in {"success", "failure"}:
        conclusion = completion.get("conclusion") or completion.get("status", "")
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run,
            f"ci_terminal_{conclusion}",
            reason=f"ci_terminal_{conclusion}",
        )

    if run.get("headBranch") != expected_branch:
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run,
            "ci_stale_binding", action="stale", reason="ci_stale_binding:run_branch_mismatch",
        )

    ci_conclusion = run.get("conclusion", "")
    supported_conclusions = {"success", "failure"} | TERMINAL_UNSUPPORTED_CONCLUSIONS
    if ci_conclusion not in supported_conclusions:
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run,
            "ci_stale_binding", action="stale",
            reason=f"ci_stale_binding:unsupported_conclusion:{ci_conclusion}",
        )

    branch = run.get("headBranch", "")

    binding_ok, binding_reason = sm.verify_issue_pr_binding(issue, pr_number, head_sha)
    if not binding_ok:
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run,
            "ci_stale_binding", action="stale",
            reason=f"ci_stale_binding:binding_rejected:{binding_reason}",
        )

    try:
        duplicate = _is_duplicate_exact_head_run(
            issue, pr_number, head_sha, ci_run_id, branch
        )
    except sm.StateUnavailableError as exc:
        return _state_unavailable_result(issue, pr_number, head_sha, ci_run_id, str(exc))
    if duplicate:
        return _record_ci_noop(
            issue, pr_number, head_sha, ci_run_id, run, "duplicate_exact_head_run",
        )

    if not ci_verifier.control_is_live():
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run,
            "ci_control_stopped", reason="ci_control_stopped:before_terminal_persistence",
        )

    try:
        _persist_canonical_acquisition(issue, pr_number, head_sha, branch, run)
    except (RuntimeError, sm.StateUnavailableError) as exc:
        return _state_unavailable_result(
            issue, pr_number, head_sha, ci_run_id, str(exc)
        )

    if not ci_verifier.control_is_live():
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run,
            "ci_control_stopped", reason="ci_control_stopped:after_terminal_persistence",
        )

    if ci_conclusion in TERMINAL_UNSUPPORTED_CONCLUSIONS:
        try:
            replacement = _reselect_unsupported(
                issue, pr_number, head_sha, branch, run
            )
        except (RuntimeError, sm.StateUnavailableError) as exc:
            return _state_unavailable_result(
                issue, pr_number, head_sha, ci_run_id, str(exc)
            )
        if replacement is not None:
            return _noop_result(
                issue,
                pr_number,
                head_sha,
                replacement.get("workflow_run_id", ci_run_id),
                "ci_reselected_after_unsupported_run",
            )
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run, ci_conclusion,
        )

    if ci_conclusion == "success":
        if not ci_verifier.control_is_live():
            return _record_ci_terminal(
                issue, pr_number, head_sha, ci_run_id, run,
                "ci_control_stopped", reason="ci_control_stopped:before_ci_verification",
            )
        try:
            evidence = ci_verifier.verify_exact_head_ci(
                pr_number, head_sha, ci_run_id, pr_info
            )
        except ci_verifier.CIVerificationError as exc:
            return _record_ci_terminal(
                issue, pr_number, head_sha, ci_run_id, run,
                "ci_stale_binding", action="stale",
                reason=f"ci_stale_binding:{_typed_ci_verification_reason(exc)}",
            )
        try:
            previous_state = sm.read_ci_state(issue)
        except sm.StateUnavailableError as exc:
            return _state_unavailable_result(
                issue, pr_number, head_sha, ci_run_id, str(exc)
            )
        repair_count = int((previous_state or {}).get("extra", {}).get("repair_count", 0))
        if not ci_verifier.control_is_live():
            return _record_ci_terminal(
                issue, pr_number, head_sha, ci_run_id, run,
                "ci_control_stopped", reason="ci_control_stopped:before_ci_state_persistence",
            )
        try:
            _record_ci(
                issue, pr_number, head_sha, ci_run_id,
                "success", run, repair_count,
            )
        except RuntimeError as exc:
            return _state_unavailable_result(
                issue, pr_number, head_sha, ci_run_id, str(exc)
            )
        if not ci_verifier.control_is_live():
            return _record_ci_terminal(
                issue, pr_number, head_sha, ci_run_id, run,
                "ci_control_stopped", reason="ci_control_stopped:after_ci_state_persistence",
            )
        labels = sm.get_issue_labels_checked(issue)
        if labels is None:
            return _state_unavailable_result(
                issue, pr_number, head_sha, ci_run_id,
                "Issue label state is unavailable",
            )
        action = "merge_ready" if sm.LABEL_REVIEW_PASSED in labels else "trigger_review"
        return {
            "action": action,
            "pr_number": pr_number,
            "issue_number": issue,
            "head_sha": head_sha,
            "ci_run_id": ci_run_id,
            "reason": "ci_green",
        }

    try:
        state = sm.read_ci_state(issue)
    except sm.StateUnavailableError as exc:
        return _state_unavailable_result(
            issue, pr_number, head_sha, ci_run_id, str(exc)
        )
    repair_count = int((state or {}).get("extra", {}).get("repair_count", 0))
    if not ci_verifier.control_is_live():
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run,
            "ci_control_stopped", reason="ci_control_stopped:before_ci_state_persistence",
        )
    next_count = repair_count + 1
    try:
        _record_ci(
            issue, pr_number, head_sha, ci_run_id,
            f"failure_repair_{repair_count}", run, repair_count,
        )
    except RuntimeError as exc:
        return _state_unavailable_result(
            issue, pr_number, head_sha, ci_run_id, str(exc)
        )
    if next_count > MAX_REPAIR_ATTEMPTS:
        return _record_ci_terminal(
            issue, pr_number, head_sha, ci_run_id, run,
            "max_repairs_exceeded", reason=f"max_repairs_exceeded:{next_count}/{MAX_REPAIR_ATTEMPTS}",
        )
    return {
        "action": "trigger_repair",
        "pr_number": pr_number,
        "issue_number": issue,
        "head_sha": head_sha,
        "ci_run_id": ci_run_id,
        "repair_count": next_count,
        "reason": "ci_failure",
    }


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "process-dispatch":
        if len(sys.argv) != 6:
            print(
                "Usage: ci_handler.py process-dispatch <issue> <pr> <head_sha> <ci_run_id>",
                file=sys.stderr,
            )
            sys.exit(1)
        result = process_ci_dispatch(
            int(sys.argv[2]), int(sys.argv[3]), sys.argv[4], int(sys.argv[5]),
        )
        print(json.dumps(result, sort_keys=True))
        return
    event_path = os.environ.get("GITHUB_EVENT_PATH")
    if not event_path:
        print("GITHUB_EVENT_PATH not set", file=sys.stderr)
        sys.exit(1)
    print(json.dumps(process_ci_completion(event_path), sort_keys=True))


if __name__ == "__main__":
    main()
