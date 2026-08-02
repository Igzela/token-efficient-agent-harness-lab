"""The single GitHub-serialized dispatcher for all orchestrator workflow runs."""

from __future__ import annotations

import json
import os
import sys
import uuid

import artifact_contract
import control_state
import state_manager as sm


def _repo() -> str:
    return os.environ.get("AGENT_REPO", os.environ.get("GITHUB_REPOSITORY", ""))


def _dispatch_id(action: str, *parts: object) -> str:
    return ":".join([action, *(str(part) for part in parts)])


def _claim(
    issue: int,
    target_label: str,
    dispatch_id: str,
    action: str,
    claim_details: dict[str, object] | None = None,
) -> tuple[bool, list[str], str]:
    repo = _repo()
    try:
        previous = sm.read_dispatch_state(issue, dispatch_id, repo)
    except sm.StateUnavailableError:
        return False, [], "dispatch_state_unavailable"
    if previous and previous.get("status") in {"claimed", "dispatched"}:
        # The claim_nonce is a per-attempt generation token, never a binding
        # field: a retry generates a fresh one, so comparing it here would
        # turn every legitimately-claimed retry into an unverifiable state.
        # The durable identity is the dispatch_id; the nonce binds the worker
        # run to the claim through the workflow input, not through this check.
        binding_details = {
            key: value
            for key, value in (claim_details or {}).items()
            if key != "claim_nonce"
        }
        if not binding_details or not sm.dispatch_state_binding_matches(
            previous, issue, action, binding_details, target_label
        ):
            return False, [], "dispatch_state_binding_unverified"
        if previous.get("status") == "dispatched":
            return False, [], "already_dispatched"
        return False, [], "dispatch_in_flight"
    labels = sm.get_issue_labels_checked(issue, repo)
    if labels is None:
        return False, [], "label_state_unavailable"
    task_scope = None
    task_binding = None
    if target_label == sm.LABEL_RUNNING:
        if sm.LABEL_READY not in labels or labels & (sm.ACTIVE_LABELS | sm.TERMINAL_LABELS):
            return False, [], "issue_not_ready"
        scope_valid, scope_binding = sm.read_task_scope_binding(issue, repo)
        if not scope_valid:
            return False, [], f"invalid_scope:{scope_binding}"
        task_binding = scope_binding
        task_scope = scope_binding["allowed_paths"]
        dependencies_ready, blocker = sm.check_dependencies_complete(issue, repo)
        if not dependencies_ready:
            return False, [], f"dependencies_not_ready:{blocker}"
        associated = sm.has_open_issue_pr(issue, repo)
        if associated is None:
            return False, [], "association_state_unavailable"
        if associated:
            return False, [], "issue_already_associated"
    elif target_label in {sm.LABEL_CI_REPAIRING, sm.LABEL_REVIEW_RUNNING}:
        if target_label in labels:
            # The external workflow may already have been dispatched even if
            # its final audit comment was unavailable.  Retain the active
            # claim and refuse a duplicate external mutation.
            try:
                matching_claim = claim_details and sm.has_inflight_ci_dispatch(
                    issue,
                    claim_details.get("pr_number"),
                    claim_details.get("head_sha"),
                    claim_details.get("ci_run_id"),
                    repo,
                )
            except sm.StateUnavailableError:
                return False, [], "dispatch_state_unavailable"
            if matching_claim:
                return False, [], "capacity_already_claimed"
            return False, [], "capacity_already_claimed_unverified"
        if labels & sm.TERMINAL_LABELS or not labels & {sm.LABEL_RUNNING, sm.LABEL_CI_REPAIRING, sm.LABEL_REVIEW_RUNNING}:
            return False, [], "issue_not_active"
    active = sm.get_active_issue_numbers(repo)
    if active is None:
        return False, [], "capacity_state_unavailable"
    if issue not in active and len(active) >= sm.MAX_ACTIVE:
        return False, [], "capacity_full"
    if target_label == sm.LABEL_RUNNING and issue not in active:
        active_scopes = sm.get_active_issue_scopes(active, repo)
        if active_scopes is None:
            return False, [], "active_scope_state_unavailable"
        for active_issue in sorted(active_scopes):
            if artifact_contract.scopes_overlap(
                task_scope or [], active_scopes[active_issue]
            ):
                return False, [], f"scope_conflict:{active_issue}"
    previous_known = sorted(labels & (sm.ACTIVE_LABELS | {sm.LABEL_READY, sm.LABEL_REVIEW_PASSED}))
    claim_details_payload = {
        "previous_labels": previous_known,
        "target_label": target_label,
        **(claim_details or {}),
    }
    # Bind the persisted claim to the exact scope and complete-body digest
    # read at claim time, before any label mutation.  A later retry keeps this
    # original binding and never re-reads a mutable Issue body.
    if task_binding is not None:
        claim_details_payload["allowed_paths"] = task_binding["allowed_paths"]
        claim_details_payload["task_body_sha256"] = task_binding["task_body_sha256"]
    # Persist the claim before changing the Issue label.  An emergency-stop
    # cancellation can interrupt the label mutation lane; the durable claim
    # then gives retry/compensation an exact identity instead of leaving an
    # active label with no state-owner record.
    if not sm.record_dispatch_state(
        issue,
        dispatch_id,
        action,
        "claimed",
        claim_details_payload,
        repo,
    ):
        return False, [], "claim_state_failed"
    if not sm.set_labels(issue, target_label, repo=repo):
        sm.record_dispatch_state(
            issue,
            dispatch_id,
            action,
            "failed",
            {"reason": "claim_label_failed", **claim_details_payload},
            repo,
        )
        return False, [], "claim_label_failed"
    return True, previous_known, "claimed"


def _rollback(issue: int, dispatch_id: str, previous_labels: list[str], reason: str) -> bool:
    repo = _repo()
    if not sm.set_labels(issue, *(previous_labels or [sm.LABEL_READY]), repo=repo):
        # The active label could not be restored, so the Issue still appears
        # active.  Do not write a terminal rollback here: it would orphan the
        # still-active label from any claimed state and make capacity release
        # impossible.  Leave the original claimed state untouched so the
        # reconcile path can still compensate it.
        return False
    try:
        previous = sm.read_dispatch_state(issue, dispatch_id, repo)
    except sm.StateUnavailableError:
        previous = None
    details = _claim_details_copy(previous) or {}
    details["reason"] = reason
    return sm.record_dispatch_state(
        issue, dispatch_id, "rollback", "failed", details, repo
    )


def _claim_details_copy(state: dict[str, object] | None) -> dict[str, object] | None:
    """Copy a persisted claim's details only when they are a valid object.

    ``read_dispatch_state`` does not validate ``details``; a corrupted or
    edited trusted comment could carry a non-object value.  Fail closed to
    ``None`` instead of raising, and record only the truthful minimal fields.
    """

    if not isinstance(state, dict):
        return None
    details = state.get("details")
    if not isinstance(details, dict):
        return None
    return dict(details)


def _record_dispatched(
    issue: int,
    dispatch_id: str,
    action: str,
    details: dict[str, object],
) -> bool:
    try:
        previous = sm.read_dispatch_state(issue, dispatch_id, _repo())
    except sm.StateUnavailableError:
        return False
    merged_details = _claim_details_copy(previous) or {}
    merged_details.update(details)
    return sm.record_dispatch_state(
        issue, dispatch_id, action, "dispatched", merged_details, _repo()
    )


def _run_workflow(workflow: str, fields: dict[str, object]) -> bool:
    try:
        control_state.require_live(_repo() or None)
    except control_state.ControlStateError:
        return False
    args = ["workflow", "run", workflow, "--ref", "main"]
    for key, value in fields.items():
        args.extend(["-f", f"{key}={value}"])
    return sm._gh(*args) is not None


def _new_claim_nonce() -> str:
    """Return one unpredictable per-claim nonce for a genuinely new claim.

    ``dispatch_id`` values are deterministic and may be reused after a claim
    is released, so an old worker run could otherwise terminalize a newer
    claim that shares its dispatch-id.  The nonce is the per-generation
    discriminator: it is persisted with the claim, carried into the worker
    workflow input, and required again for terminalization.
    """

    return uuid.uuid4().hex


def dispatch_ready(issue: int, dispatch_id: str | None = None) -> dict[str, object]:
    dispatch_id = dispatch_id or _dispatch_id("worker", issue)
    try:
        control_state.require_live(_repo() or None)
    except control_state.ControlStateError:
        return {"dispatched": False, "issue": issue, "reason": "disabled_or_emergency_stopped"}
    claim_nonce = _new_claim_nonce()
    claimed, previous, reason = _claim(
        issue,
        sm.LABEL_RUNNING,
        dispatch_id,
        "worker",
        {"issue_number": issue, "claim_nonce": claim_nonce},
    )
    if not claimed:
        if reason.startswith("invalid_scope:"):
            sm.record_dispatch_state(
                issue, dispatch_id, "worker", "rejected",
                {"reason": "invalid_scope", "detail": reason.removeprefix("invalid_scope:")}, _repo(),
            )
        return {"dispatched": reason == "already_dispatched", "issue": issue, "reason": reason}
    # Defensive post-label capacity recheck.  New implementation claims are
    # normally serialized by the agent-dispatch-global workflow group, so a
    # breach here means a bypass or stale read.  Re-read the authoritative
    # active union after the label write and before any external dispatch:
    # if it is unreadable or exceeds the canonical K, compensate by
    # restoring the previous labels and terminalizing the claim (preserving
    # its binding and nonce) without ever starting a workflow run.
    recheck = sm.get_active_issue_numbers(_repo())
    if recheck is None:
        rolled_back = _rollback(issue, dispatch_id, previous, "capacity_recheck_unavailable")
        reason = "capacity_recheck_unavailable" if rolled_back else "capacity_recheck_unavailable_rollback_failed"
        return {"dispatched": False, "issue": issue, "reason": reason}
    if len(recheck) > sm.MAX_ACTIVE:
        rolled_back = _rollback(issue, dispatch_id, previous, "capacity_recheck_exceeded")
        reason = "capacity_recheck_exceeded" if rolled_back else "capacity_recheck_exceeded_rollback_failed"
        return {"dispatched": False, "issue": issue, "reason": reason}
    if not _run_workflow(
        "agent-worker.yml",
        {
            "issue": issue,
            "dry_run": "false",
            "dispatch_id": dispatch_id,
            "claim_nonce": claim_nonce,
        },
    ):
        rolled_back = _rollback(issue, dispatch_id, previous, "workflow_dispatch_failed")
        reason = "workflow_dispatch_failed" if rolled_back else "workflow_dispatch_failed_rollback_failed"
        return {"dispatched": False, "issue": issue, "reason": reason}
    if not _record_dispatched(
        issue, dispatch_id, "worker", {"workflow": "agent-worker.yml"}
    ):
        # The external workflow has already been accepted.  Keep the claimed
        # state and active label so a retry cannot dispatch it a second time.
        return {
            "dispatched": False,
            "issue": issue,
            "reason": "dispatch_state_failed_capacity_retained",
        }
    return {"dispatched": True, "issue": issue, "dispatch_id": dispatch_id}


def dispatch_repair(pr: int, issue: int, sha: str, run_id: str, repair_count: str) -> dict[str, object]:
    dispatch_id = _dispatch_id("repair", pr, sha, run_id, repair_count)
    try:
        control_state.require_live(_repo() or None)
    except control_state.ControlStateError:
        return {"dispatched": False, "reason": "disabled_or_emergency_stopped"}
    fields = {
        "pr_number": pr,
        "issue_number": issue,
        "head_sha": sha,
        "repair_count": repair_count,
        "ci_run_id": run_id,
    }
    claimed, previous, reason = _claim(
        issue, sm.LABEL_CI_REPAIRING, dispatch_id, "repair", fields
    )
    if not claimed:
        return {"dispatched": reason == "already_dispatched", "reason": reason}
    if not _run_workflow("agent-ci-repair.yml", fields):
        rolled_back = _rollback(issue, dispatch_id, previous, "workflow_dispatch_failed")
        reason = "workflow_dispatch_failed" if rolled_back else "workflow_dispatch_failed_rollback_failed"
        return {"dispatched": False, "reason": reason}
    if not _record_dispatched(issue, dispatch_id, "repair", fields):
        # Do not roll back a claim after the external repair has started.
        return {"dispatched": False, "reason": "dispatch_state_failed_capacity_retained"}
    return {"dispatched": True, "dispatch_id": dispatch_id}


def dispatch_review(pr: int, issue: int, sha: str) -> dict[str, object]:
    dispatch_id = _dispatch_id("review", pr, sha)
    try:
        control_state.require_live(_repo() or None)
    except control_state.ControlStateError:
        return {"dispatched": False, "reason": "disabled_or_emergency_stopped"}
    fields = {"pr_number": pr, "issue_number": issue, "head_sha": sha}
    claimed, previous, reason = _claim(
        issue, sm.LABEL_REVIEW_RUNNING, dispatch_id, "review", fields
    )
    if not claimed:
        return {"dispatched": reason == "already_dispatched", "reason": reason}
    if not _run_workflow("agent-review.yml", fields):
        rolled_back = _rollback(issue, dispatch_id, previous, "workflow_dispatch_failed")
        reason = "workflow_dispatch_failed" if rolled_back else "workflow_dispatch_failed_rollback_failed"
        return {"dispatched": False, "reason": reason}
    if not _record_dispatched(issue, dispatch_id, "review", fields):
        # Do not roll back a claim after the external review has started.
        return {"dispatched": False, "reason": "dispatch_state_failed_capacity_retained"}
    return {"dispatched": True, "dispatch_id": dispatch_id}


def retry_review(issue: int) -> dict[str, object]:
    """Retry review only after deriving and revalidating the live Issue binding."""

    repo = _repo()
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"dispatched": False, "reason": "disabled_or_emergency_stopped"}
    labels = sm.get_issue_labels_checked(issue, repo)
    if labels is None:
        return {"dispatched": False, "reason": "label_state_unavailable"}
    disallowed_labels = sm.ACTIVE_LABELS | (
        sm.TERMINAL_LABELS - {sm.LABEL_REVIEW_BLOCKED}
    )
    if sm.LABEL_REVIEW_BLOCKED not in labels or labels & disallowed_labels:
        return {"dispatched": False, "reason": "issue_not_review_blocked"}
    try:
        worker = sm.read_worker_state(issue, repo)
    except sm.StateUnavailableError:
        return {"dispatched": False, "reason": "worker_state_unavailable"}
    if not worker:
        return {"dispatched": False, "reason": "worker_state_unavailable"}
    try:
        pr = int(worker["pr_number"])
        sha = str(worker["head_sha"])
    except (KeyError, TypeError, ValueError):
        return {"dispatched": False, "reason": "worker_state_invalid"}
    binding_ok, binding_reason = sm.verify_issue_pr_binding(issue, pr, sha, repo)
    if not binding_ok:
        return {"dispatched": False, "reason": f"binding_rejected:{binding_reason}"}
    dispatch_id = _dispatch_id("retry-review", pr, sha)
    try:
        previous = sm.read_dispatch_state(issue, dispatch_id, repo)
    except sm.StateUnavailableError:
        return {"dispatched": False, "reason": "dispatch_state_unavailable"}
    fields = {"pr_number": pr, "issue_number": issue, "head_sha": sha}
    if previous and previous.get("status") in {"claimed", "dispatched"}:
        if not sm.dispatch_state_binding_matches(
            previous, issue, "retry-review", fields, sm.LABEL_REVIEW_RUNNING
        ):
            return {"dispatched": False, "reason": "dispatch_state_binding_unverified"}
        if previous.get("status") == "dispatched":
            return {
                "dispatched": True,
                "already_dispatched": True,
                "dispatch_id": dispatch_id,
            }
        return {"dispatched": False, "reason": "dispatch_in_flight"}
    active = sm.get_active_issue_numbers(repo)
    if active is None:
        return {"dispatched": False, "reason": "capacity_state_unavailable"}
    if issue not in active and len(active) >= sm.MAX_ACTIVE:
        return {"dispatched": False, "reason": "capacity_full"}
    previous_labels = [sm.LABEL_REVIEW_BLOCKED]
    if not sm.record_dispatch_state(
        issue,
        dispatch_id,
        "retry-review",
        "claimed",
        {
            "previous_labels": previous_labels,
            "target_label": sm.LABEL_REVIEW_RUNNING,
            **fields,
        },
        repo,
    ):
        return {"dispatched": False, "reason": "claim_state_failed"}
    if not sm.set_labels(issue, sm.LABEL_REVIEW_RUNNING, repo=repo):
        sm.record_dispatch_state(
            issue,
            dispatch_id,
            "retry-review",
            "failed",
            {"reason": "claim_label_failed", **fields},
            repo,
        )
        return {"dispatched": False, "reason": "claim_label_failed"}
    if not _run_workflow("agent-review.yml", fields):
        rolled_back = _rollback(
            issue, dispatch_id, previous_labels, "workflow_dispatch_failed"
        )
        reason = "workflow_dispatch_failed" if rolled_back else "workflow_dispatch_failed_rollback_failed"
        return {"dispatched": False, "reason": reason}
    if not _record_dispatched(issue, dispatch_id, "retry-review", fields):
        # Do not roll back a claim after the external retry has started.
        return {"dispatched": False, "reason": "dispatch_state_failed_capacity_retained"}
    return {"dispatched": True, "dispatch_id": dispatch_id}


def dispatch_merge(pr: int, issue: int, sha: str) -> dict[str, object]:
    dispatch_id = _dispatch_id("merge", pr, sha)
    fields = {"pr_number": pr, "issue_number": issue, "head_sha": sha}
    try:
        control_state.require_auto_merge(_repo() or None)
    except control_state.ControlStateError:
        return {"dispatched": False, "reason": "disabled_or_emergency_stopped"}
    try:
        existing = sm.read_dispatch_state(issue, dispatch_id, _repo())
    except sm.StateUnavailableError:
        return {"dispatched": False, "reason": "dispatch_state_unavailable"}
    if existing and existing.get("status") in {"claimed", "dispatched"}:
        if not sm.dispatch_state_binding_matches(
            existing, issue, "merge", fields
        ):
            return {"dispatched": False, "reason": "dispatch_state_binding_unverified"}
        if existing.get("status") == "dispatched":
            return {"dispatched": True, "already_dispatched": True, "dispatch_id": dispatch_id}
        return {"dispatched": False, "reason": "dispatch_in_flight"}
    labels = sm.get_issue_labels_checked(issue, _repo())
    if labels is None:
        return {"dispatched": False, "reason": "label_state_unavailable"}
    if sm.LABEL_MERGE_READY not in labels or sm.LABEL_REVIEW_PASSED not in labels:
        return {"dispatched": False, "reason": "issue_not_merge_ready"}
    try:
        review = sm.read_review_state(issue, _repo())
    except sm.StateUnavailableError:
        return {"dispatched": False, "reason": "review_state_unavailable"}
    if not review or review.get("pr_number") != int(pr) or review.get("head_sha") != sha or review.get("verdict") != "PASS":
        return {"dispatched": False, "reason": "review_head_mismatch"}
    if not sm.record_dispatch_state(issue, dispatch_id, "merge", "claimed", fields, _repo()):
        return {"dispatched": False, "reason": "claim_state_failed"}
    if not _run_workflow("agent-merge.yml", fields):
        audited = sm.record_dispatch_state(
            issue, dispatch_id, "merge", "failed",
            {"reason": "workflow_dispatch_failed"}, _repo(),
        )
        reason = "workflow_dispatch_failed" if audited else "workflow_dispatch_failed_audit_failed"
        return {"dispatched": False, "reason": reason}
    if not _record_dispatched(issue, dispatch_id, "merge", fields):
        # The merge workflow has already been accepted.  Retain the claim so
        # a later monitor retry cannot issue a second merge mutation.
        return {"dispatched": False, "reason": "dispatch_state_failed_capacity_retained"}
    return {"dispatched": True, "dispatch_id": dispatch_id}


def dispatch_next(source_issue: str | None = None) -> dict[str, object]:
    repo = _repo()
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"dispatched": False, "reason": "disabled_or_emergency_stopped"}
    active = sm.get_active_issue_numbers(repo)
    if active is None:
        return {"dispatched": False, "reason": "capacity-state-unavailable"}
    if len(active) >= sm.MAX_ACTIVE:
        return {"dispatched": False, "reason": "capacity-full"}
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
        raise SystemExit("Usage: dispatcher.py <dispatch-ready|dispatch-repair|dispatch-review|retry-review|dispatch-merge|dispatch-next> ...")
    command = sys.argv[1]
    if command == "dispatch-ready" and len(sys.argv) in {3, 4}:
        result = dispatch_ready(int(sys.argv[2]), sys.argv[3] if len(sys.argv) == 4 else None)
    elif command == "dispatch-repair" and len(sys.argv) == 7:
        result = dispatch_repair(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4], sys.argv[5], sys.argv[6])
    elif command == "dispatch-review" and len(sys.argv) == 5:
        result = dispatch_review(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4])
    elif command == "retry-review" and len(sys.argv) == 3:
        result = retry_review(int(sys.argv[2]))
    elif command == "dispatch-merge" and len(sys.argv) == 5:
        result = dispatch_merge(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4])
    elif command == "dispatch-next" and len(sys.argv) in {2, 3}:
        result = dispatch_next(sys.argv[2] if len(sys.argv) == 3 else None)
    else:
        raise SystemExit("invalid dispatcher command arity")
    print(json.dumps(result, sort_keys=True))
    if result.get("dispatched") is False:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
