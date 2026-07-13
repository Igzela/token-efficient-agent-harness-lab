"""Process canonical tests workflow completions with exact-head evidence."""

from __future__ import annotations

import json
import os
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
        return None
    requirements = ci_verifier.load_requirements()
    if run.get("workflowName") != requirements["workflow_name"]:
        return None
    if run.get("headSha") != info["head_sha"] or run.get("status") != "completed":
        return None
    if info.get("workflow_id") is not None and run.get("workflowId") not in (None, info["workflow_id"]):
        return None
    if info.get("workflow_path") and run.get("path") not in (None, info["workflow_path"]):
        return None
    return run


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
    exact_runs = ci_verifier._acquirable_runs(ci_verifier.find_exact_runs(branch, sha))
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
        run for run in ci_verifier.find_exact_runs(branch, sha)
        if ci_verifier._candidate_matches(run, branch, sha, requirements)
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
    metadata = {
        "status": status,
        "observed_run_ids": observed_ids,
        "selection_reason": (
            ci_verifier._selection_reason(selected, acquirable)
            if status == "bound" else "unsupported_terminal_observed"
        ),
        "superseded_run_ids": superseded_ids,
        "unsupported_run_ids": unsupported_ids,
        "fallback_dispatched": False,
    }
    if not sm.record_ci_acquisition(
        issue, pr, sha, selected_id, source, superseded_ids, metadata=metadata
    ):
        raise RuntimeError("unable to persist canonical exact-head acquisition")
    return True


def _state_unavailable_result(issue, pr, sha, run_id, detail):
    return {
        "action": "blocked",
        "pr_number": pr,
        "issue_number": issue,
        "head_sha": sha,
        "ci_run_id": run_id,
        "reason": f"ci_state_unavailable:{detail}",
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
    run = _run_for_event(info)
    if not run:
        return {"action": "stale", "reason": "workflow_identity_or_head_mismatch"}
    pr_number = int(pr_number)
    pr_info = sm.get_pr_info(pr_number)
    if not pr_info or pr_info.get("state") != "OPEN":
        return {"action": "noop", "reason": "pr_not_open"}
    current_head = pr_info.get("headRefOid", "")
    expected_head = info["pr_head_sha"] or info["head_sha"]
    expected_branch = pr_info.get("headRefName", "")
    if (
        current_head != expected_head
        or run.get("headSha") != expected_head
        or info["head_branch"] != expected_branch
        or run.get("headBranch") != expected_branch
    ):
        return {"action": "stale", "reason": "head_sha_mismatch"}
    issue_number = _find_issue_for_pr(pr_number)
    if not issue_number:
        return {"action": "noop", "reason": "no_canonical_issue_binding"}
    binding_ok, binding_reason = sm.verify_issue_pr_binding(issue_number, pr_number, current_head)
    if not binding_ok:
        return {"action": "noop", "reason": f"binding_rejected:{binding_reason}"}
    try:
        duplicate = _is_duplicate_exact_head_run(
            issue_number, pr_number, current_head, info["run_id"], info["head_branch"]
        )
    except sm.StateUnavailableError as exc:
        return _state_unavailable_result(
            issue_number, pr_number, current_head, info["run_id"], str(exc)
        )
    if duplicate:
        return {"action": "noop", "reason": "duplicate_exact_head_run"}

    try:
        _persist_canonical_acquisition(
            issue_number, pr_number, current_head, info["head_branch"], run
        )
    except (RuntimeError, sm.StateUnavailableError) as exc:
        return _state_unavailable_result(
            issue_number, pr_number, current_head, info["run_id"], str(exc)
        )

    if info["conclusion"] in TERMINAL_UNSUPPORTED_CONCLUSIONS:
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
        return {
            "action": "blocked",
            "pr_number": pr_number,
            "issue_number": issue_number,
            "head_sha": current_head,
            "ci_run_id": info["run_id"],
            "reason": f"ci_terminal_{conclusion}",
        }

    if info["conclusion"] == "success":
        try:
            evidence = ci_verifier.verify_exact_head_ci(pr_number, current_head, info["run_id"], pr_info)
        except ci_verifier.CIVerificationError as exc:
            return {"action": "stale", "reason": f"exact_head_ci_rejected:{exc}"}
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
    if next_count > MAX_REPAIR_ATTEMPTS:
        return {
            "action": "blocked",
            "pr_number": pr_number,
            "issue_number": issue_number,
            "head_sha": current_head,
            "repair_count": next_count,
            "reason": f"max_repairs_exceeded ({next_count}/{MAX_REPAIR_ATTEMPTS})",
        }
    return {
        "action": "trigger_repair",
        "pr_number": pr_number,
        "issue_number": issue_number,
        "head_sha": current_head,
        "ci_run_id": info["run_id"],
        "repair_count": next_count,
        "reason": "ci_failure",
    }


def main():
    event_path = os.environ.get("GITHUB_EVENT_PATH")
    if not event_path:
        print("GITHUB_EVENT_PATH not set", file=sys.stderr)
        sys.exit(1)
    print(json.dumps(process_ci_completion(event_path), sort_keys=True))


if __name__ == "__main__":
    main()
