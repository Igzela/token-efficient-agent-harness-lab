"""The single GitHub-serialized dispatcher for all orchestrator workflow runs."""

from __future__ import annotations

import json
import os
import sys
import uuid
from datetime import datetime, timedelta, timezone

import artifact_contract
import control_state
import local_loop
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
    *,
    scope_binding: dict[str, object] | None = None,
    dependencies_ready: tuple[bool, object] | None = None,
    require_claim_readback: bool = False,
) -> tuple[bool, list[str], str]:
    """Claim one Issue through the existing durable owners.

    ``scope_binding`` and ``dependencies_ready`` are narrow precomputed
    inputs: a caller that already read the Issue body exactly once (the
    trusted local-run gateway) hands over the derived binding and dependency
    outcome so ``_claim`` never re-reads the mutable body.  Ordinary Actions
    dispatchers pass nothing and keep the previous read-from-GitHub behavior.

    ``require_claim_readback`` makes the trusted readback mandatory for the
    caller's own claim generation: after the claimed comment is persisted and
    before any label mutation, the claim is re-read through the
    trusted-author-filtered ``read_dispatch_state`` and its exact dispatch id,
    binding, and nonce are verified.  A local human-authored direct invocation
    writes an untrusted comment that the readback filters out, so it fails
    closed here without ever changing a label.
    """

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
        # lease_deadline is a derived time bound recomputed on every retry, so
        # it is likewise excluded: the dispatch_id plus the client-bound
        # details are what make a retry verifiable, and a retry that arrives
        # after the label write must not be denied merely because its clock
        # differs from the original claim's.
        binding_details = {
            key: value
            for key, value in (claim_details or {}).items()
            if key not in {"claim_nonce", "lease_deadline"}
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
        if scope_binding is None:
            scope_valid, scope_binding = sm.read_task_scope_binding(issue, repo)
            if not scope_valid:
                return False, [], f"invalid_scope:{scope_binding}"
        task_binding = scope_binding
        task_scope = scope_binding["allowed_paths"]
        if dependencies_ready is None:
            dependencies_ready, blocker = sm.check_dependencies_complete(issue, repo)
        else:
            dependencies_ready, blocker = dependencies_ready
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
    if require_claim_readback:
        try:
            persisted_claim = sm.read_dispatch_state(issue, dispatch_id, repo)
        except sm.StateUnavailableError:
            persisted_claim = None
        if not sm.dispatch_state_binding_matches(
            persisted_claim, issue, action, claim_details_payload, target_label
        ):
            # The persisted claim could not be verified through the trusted
            # author filter, so no label mutation is authorized.  Leave the
            # claimed comment in place: a genuine retry resolves through the
            # existing binding, and a human-authored direct invocation (which
            # `read_dispatch_state` never trusts) ends here without touching
            # the Issue label.
            return False, [], "claim_readback_unverified"
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


def _normalized_attempt_id(value: object) -> str | None:
    """Return the canonical lowercase hyphenated UUID text, or None.

    The canonical attempt id is exactly lowercase hyphenated UUID text
    (``123e4567-e89b-12d3-a456-426614174000``) and is persisted exactly as
    such.  Any other spelling -- uppercase, undashed hex, URN, braces, or a
    malformed string -- fails closed to ``None``.
    """

    if not isinstance(value, str):
        return None
    try:
        parsed = uuid.UUID(value)
    except ValueError:
        return None
    return value if value == str(parsed) else None


def claim_local(issue: int, attempt_id: str, client_token: str) -> dict[str, object]:
    """Claim one GitHub-serialized local-run attempt for a trusted local process.

    The repository, default branch, accepted main SHA, issue author, and
    canonical branch are all derived server-side from GitHub; a local process
    supplies only the Issue, a canonical lowercase-hyphenated UUID attempt id,
    and a 32-lower-hex client token, and never any SHA, branch, scope, or
    lease.  The Issue body is read exactly once and both the repo-agent-task
    main binding and the issue-scope binding are derived from that same body;
    the scope binding and dependency outcome are precomputed into ``_claim``
    so the mutable body is never re-read.  The claim is persisted before label
    mutation, the claimed comment is re-read and verified through the trusted
    author filter before any label change, then the authoritative capacity is
    rechecked; a dispatched state is recorded without ever starting a GitHub
    workflow (the local process executes the task itself).
    """

    repo = _repo()
    attempt = _normalized_attempt_id(attempt_id)
    if attempt is None:
        return {"dispatched": False, "issue": issue, "reason": "invalid_attempt_id"}
    if not isinstance(client_token, str) or sm.CLAIM_NONCE_PATTERN.fullmatch(client_token) is None:
        return {"dispatched": False, "issue": issue, "reason": "invalid_client_token"}
    dispatch_id = _dispatch_id("local-run", issue, attempt)
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"dispatched": False, "issue": issue, "reason": "disabled_or_emergency_stopped"}
    if not repo:
        return {"dispatched": False, "issue": issue, "reason": "repository_unavailable"}
    try:
        adapter = local_loop.GitHubAdapter(repo)
    except ValueError:
        return {"dispatched": False, "issue": issue, "reason": "repository_malformed"}
    try:
        metadata = adapter.repository_metadata()
    except local_loop.LoopUnavailable:
        return {"dispatched": False, "issue": issue, "reason": "repository_state_unavailable"}
    owner = metadata.get("owner")
    branch = metadata.get("default_branch")
    if not isinstance(owner, str) or not owner:
        return {"dispatched": False, "issue": issue, "reason": "repository_state_unavailable"}
    if not isinstance(branch, str) or not local_loop.BRANCH.fullmatch(branch):
        return {"dispatched": False, "issue": issue, "reason": "default_branch_unavailable"}
    try:
        accepted_main = adapter.accepted_main_sha(branch)
    except local_loop.LoopUnavailable:
        return {"dispatched": False, "issue": issue, "reason": "accepted_main_unavailable"}
    if not local_loop.HEX40.fullmatch(accepted_main):
        return {"dispatched": False, "issue": issue, "reason": "accepted_main_unavailable"}
    # The canonical branch is derived from the Issue number only; it is never
    # required to exist yet.  An existing Issue PR is rejected through the
    # existing owner inside ``_claim``.
    canonical_branch = f"agent/issue-{issue}"
    author = sm.get_issue_author(issue, repo)
    if author is None:
        return {"dispatched": False, "issue": issue, "reason": "issue_state_unavailable"}
    if author.casefold() != owner.casefold():
        return {"dispatched": False, "issue": issue, "reason": "untrusted_author"}
    body = sm.get_issue_body(issue, repo)
    if not isinstance(body, str):
        return {"dispatched": False, "issue": issue, "reason": "task_body_unavailable"}
    try:
        task_main = local_loop.task_main_sha(body)
    except ValueError:
        return {"dispatched": False, "issue": issue, "reason": "invalid_task_binding"}
    if task_main != accepted_main:
        return {"dispatched": False, "issue": issue, "reason": "accepted_main_mismatch"}
    # The scope binding and the dependency outcome come from the same single
    # body read; any failure here writes no dispatch state, no label, and
    # starts no workflow.
    try:
        scope_binding = artifact_contract.build_issue_scope_binding(body)
    except (artifact_contract.ArtifactContractError, ValueError, TypeError) as exc:
        return {"dispatched": False, "issue": issue, "reason": f"invalid_scope:{exc}"}
    dependencies_ready = sm.check_dependencies_complete(issue, repo, body=body)
    lease_deadline = (
        datetime.now(timezone.utc) + timedelta(hours=sm.LOCAL_CLAIM_LEASE_HOURS)
    ).isoformat().replace("+00:00", "Z")
    claimed, previous, reason = _claim(
        issue,
        sm.LABEL_RUNNING,
        dispatch_id,
        "local-run",
        {
            "issue_number": issue,
            "attempt_id": attempt,
            "client_token": client_token,
            "accepted_main_sha": accepted_main,
            "canonical_branch": canonical_branch,
            "lease_deadline": lease_deadline,
            "claim_nonce": _new_claim_nonce(),
        },
        scope_binding=scope_binding,
        dependencies_ready=dependencies_ready,
        require_claim_readback=True,
    )
    if not claimed:
        return {"dispatched": reason == "already_dispatched", "issue": issue, "reason": reason}
    # Defensive post-label capacity recheck, identical to dispatch_ready: the
    # agent-dispatch-global group normally serializes claims, so a breach here
    # means a bypass or stale read.  Re-read the authoritative active union
    # after the label write; if it is unreadable or exceeds the canonical K,
    # compensate by restoring the previous labels and terminalizing the claim
    # (preserving its binding and nonce) without ever handing the attempt to
    # the local process.
    recheck = sm.get_active_issue_numbers(repo)
    if recheck is None:
        rolled_back = _rollback(issue, dispatch_id, previous, "capacity_recheck_unavailable")
        reason = "capacity_recheck_unavailable" if rolled_back else "capacity_recheck_unavailable_rollback_failed"
        return {"dispatched": False, "issue": issue, "reason": reason}
    if len(recheck) > sm.MAX_ACTIVE:
        rolled_back = _rollback(issue, dispatch_id, previous, "capacity_recheck_exceeded")
        reason = "capacity_recheck_exceeded" if rolled_back else "capacity_recheck_exceeded_rollback_failed"
        return {"dispatched": False, "issue": issue, "reason": reason}
    if not _record_dispatched(issue, dispatch_id, "local-run", {"workflow": "local-run"}):
        # The claim and active label stand; a retry must resolve through the
        # existing binding instead of dispatching a second local process.
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
        raise SystemExit(
            "Usage: dispatcher.py <dispatch-ready|dispatch-repair|dispatch-review|"
            "retry-review|dispatch-merge|dispatch-next|claim-local> ..."
        )
    command = sys.argv[1]
    if command == "dispatch-ready" and len(sys.argv) in {3, 4}:
        result = dispatch_ready(int(sys.argv[2]), sys.argv[3] if len(sys.argv) == 4 else None)
    elif command == "claim-local" and len(sys.argv) == 5:
        result = claim_local(int(sys.argv[2]), sys.argv[3], sys.argv[4])
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
