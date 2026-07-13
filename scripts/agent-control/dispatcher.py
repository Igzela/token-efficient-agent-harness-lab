"""The single GitHub-serialized dispatcher for all orchestrator workflow runs."""

from __future__ import annotations

import json
import os
import sys
import uuid

import state_manager as sm


MAX_ACTIVE = 2


def _repo() -> str:
    return os.environ.get("AGENT_REPO", os.environ.get("GITHUB_REPOSITORY", ""))


def _dispatch_id(action: str, *parts: object) -> str:
    return ":".join([action, *(str(part) for part in parts)])


def _claim(issue: int, target_label: str, dispatch_id: str, action: str) -> tuple[bool, list[str], str]:
    repo = _repo()
    previous = sm.read_dispatch_state(issue, dispatch_id, repo)
    if previous and previous.get("status") == "dispatched":
        return False, [], "already_dispatched"
    labels = sm.get_issue_labels(issue, repo)
    if target_label == sm.LABEL_RUNNING:
        if sm.LABEL_READY not in labels or labels & (sm.ACTIVE_LABELS | sm.TERMINAL_LABELS):
            return False, [], "issue_not_ready"
        associated = sm.has_open_issue_pr(issue, repo)
        if associated is None:
            return False, [], "association_state_unavailable"
        if associated:
            return False, [], "issue_already_associated"
    elif target_label in {sm.LABEL_CI_REPAIRING, sm.LABEL_REVIEW_RUNNING}:
        if labels & sm.TERMINAL_LABELS or not labels & {sm.LABEL_RUNNING, sm.LABEL_CI_REPAIRING, sm.LABEL_REVIEW_RUNNING}:
            return False, [], "issue_not_active"
    active = sm.get_active_issue_numbers(repo)
    if active is None:
        return False, [], "capacity_state_unavailable"
    if issue not in active and len(active) >= MAX_ACTIVE:
        return False, [], "capacity_full"
    previous_known = sorted(labels & (sm.ACTIVE_LABELS | {sm.LABEL_READY, sm.LABEL_REVIEW_PASSED}))
    if not sm.set_labels(issue, target_label, repo=repo):
        return False, [], "claim_label_failed"
    if not sm.record_dispatch_state(
        issue,
        dispatch_id,
        action,
        "claimed",
        {"previous_labels": previous_known, "target_label": target_label},
        repo,
    ):
        sm.set_labels(issue, *(previous_known or [sm.LABEL_READY]), repo=repo)
        return False, [], "claim_state_failed"
    return True, previous_known, "claimed"


def _rollback(issue: int, dispatch_id: str, previous_labels: list[str], reason: str) -> None:
    repo = _repo()
    sm.set_labels(issue, *(previous_labels or [sm.LABEL_READY]), repo=repo)
    sm.record_dispatch_state(issue, dispatch_id, "rollback", "failed", {"reason": reason}, repo)


def _run_workflow(workflow: str, fields: dict[str, object]) -> bool:
    args = ["workflow", "run", workflow, "--ref", "main"]
    for key, value in fields.items():
        args.extend(["-f", f"{key}={value}"])
    return sm._gh(*args) is not None


def dispatch_ready(issue: int, dispatch_id: str | None = None) -> dict[str, object]:
    dispatch_id = dispatch_id or _dispatch_id("worker", issue)
    claimed, previous, reason = _claim(issue, sm.LABEL_RUNNING, dispatch_id, "worker")
    if not claimed:
        return {"dispatched": reason == "already_dispatched", "issue": issue, "reason": reason}
    if not _run_workflow("agent-worker.yml", {"issue": issue, "dry_run": "false"}):
        _rollback(issue, dispatch_id, previous, "workflow_dispatch_failed")
        return {"dispatched": False, "issue": issue, "reason": "workflow_dispatch_failed"}
    sm.record_dispatch_state(issue, dispatch_id, "worker", "dispatched", {"workflow": "agent-worker.yml"}, _repo())
    return {"dispatched": True, "issue": issue, "dispatch_id": dispatch_id}


def dispatch_repair(
    pr: int, issue: int, sha: str, run_id: str, repair_count: str, failed_jobs: str
) -> dict[str, object]:
    dispatch_id = _dispatch_id("repair", pr, sha, run_id, repair_count)
    claimed, previous, reason = _claim(issue, sm.LABEL_CI_REPAIRING, dispatch_id, "repair")
    if not claimed:
        return {"dispatched": reason == "already_dispatched", "reason": reason}
    fields = {
        "pr_number": pr,
        "issue_number": issue,
        "head_sha": sha,
        "failed_jobs": failed_jobs,
        "repair_count": repair_count,
        "ci_run_id": run_id,
    }
    if not _run_workflow("agent-ci-repair.yml", fields):
        _rollback(issue, dispatch_id, previous, "workflow_dispatch_failed")
        return {"dispatched": False, "reason": "workflow_dispatch_failed"}
    sm.record_dispatch_state(issue, dispatch_id, "repair", "dispatched", fields, _repo())
    return {"dispatched": True, "dispatch_id": dispatch_id}


def dispatch_review(pr: int, issue: int, sha: str) -> dict[str, object]:
    dispatch_id = _dispatch_id("review", pr, sha)
    claimed, previous, reason = _claim(issue, sm.LABEL_REVIEW_RUNNING, dispatch_id, "review")
    if not claimed:
        return {"dispatched": reason == "already_dispatched", "reason": reason}
    fields = {"pr_number": pr, "issue_number": issue, "head_sha": sha}
    if not _run_workflow("agent-review.yml", fields):
        _rollback(issue, dispatch_id, previous, "workflow_dispatch_failed")
        return {"dispatched": False, "reason": "workflow_dispatch_failed"}
    sm.record_dispatch_state(issue, dispatch_id, "review", "dispatched", fields, _repo())
    return {"dispatched": True, "dispatch_id": dispatch_id}


def dispatch_merge(pr: int, issue: int, sha: str) -> dict[str, object]:
    dispatch_id = _dispatch_id("merge", pr, sha)
    existing = sm.read_dispatch_state(issue, dispatch_id, _repo())
    if existing and existing.get("status") == "dispatched":
        return {"dispatched": True, "already_dispatched": True, "dispatch_id": dispatch_id}
    fields = {"pr_number": pr, "issue_number": issue, "head_sha": sha}
    if not sm.record_dispatch_state(issue, dispatch_id, "merge", "claimed", fields, _repo()):
        return {"dispatched": False, "reason": "claim_state_failed"}
    if not _run_workflow("agent-merge.yml", fields):
        sm.record_dispatch_state(issue, dispatch_id, "merge", "failed", {"reason": "workflow_dispatch_failed"}, _repo())
        return {"dispatched": False, "reason": "workflow_dispatch_failed"}
    sm.record_dispatch_state(issue, dispatch_id, "merge", "dispatched", fields, _repo())
    return {"dispatched": True, "dispatch_id": dispatch_id}


def dispatch_next(source_issue: str | None = None) -> dict[str, object]:
    repo = _repo()
    active = sm.get_active_issue_numbers(repo)
    if active is None or len(active) >= MAX_ACTIVE:
        return {"dispatched": False, "reason": "capacity_state_unavailable_or_full"}
    args = ["issue", "list", "--label", sm.LABEL_READY, "--state", "open", "--limit", "100", "--json", "number"]
    if repo:
        args.extend(["--repo", repo])
    raw = sm._gh(*args)
    if raw is None:
        return {"dispatched": False, "reason": "ready_issue_query_failed"}
    try:
        candidates = sorted(int(item["number"]) for item in json.loads(raw))
    except (json.JSONDecodeError, KeyError, TypeError, ValueError):
        return {"dispatched": False, "reason": "ready_issue_query_invalid"}
    for issue in candidates:
        if issue in active:
            continue
        ok, _ = sm.check_dependencies_complete(issue, repo)
        if not ok:
            continue
        result = dispatch_ready(issue, _dispatch_id("next", source_issue or "none", issue))
        if result.get("dispatched"):
            return {"selected_issue": issue, **result}
    return {"dispatched": False, "reason": "no_dependency_ready_task"}


def main() -> None:
    if len(sys.argv) < 2:
        raise SystemExit("Usage: dispatcher.py <dispatch-ready|dispatch-repair|dispatch-review|dispatch-merge|dispatch-next> ...")
    command = sys.argv[1]
    if command == "dispatch-ready" and len(sys.argv) in {3, 4}:
        result = dispatch_ready(int(sys.argv[2]), sys.argv[3] if len(sys.argv) == 4 else None)
    elif command == "dispatch-repair" and len(sys.argv) == 8:
        result = dispatch_repair(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4], sys.argv[5], sys.argv[6], sys.argv[7])
    elif command == "dispatch-review" and len(sys.argv) == 5:
        result = dispatch_review(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4])
    elif command == "dispatch-merge" and len(sys.argv) == 5:
        result = dispatch_merge(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4])
    elif command == "dispatch-next" and len(sys.argv) in {2, 3}:
        result = dispatch_next(sys.argv[2] if len(sys.argv) == 3 else None)
    else:
        raise SystemExit("invalid dispatcher command arity")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    main()
