"""The single GitHub-serialized dispatcher for all orchestrator workflow runs."""

from __future__ import annotations

import json
import os
import sys
import uuid
from datetime import datetime, timedelta, timezone

import artifact_contract
import ci_verifier
import control_state
import local_loop
import pr_binding
import state_manager as sm


MONITOR_WORKFLOW = "agent-ci-monitor.yml"
MONITOR_RECEIPT_ACTION = "ci-monitor"


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


def _local_dispatch_id(issue: int, attempt: str) -> str:
    return _dispatch_id("local-run", issue, attempt)


def _read_local_claim(issue: int, dispatch_id: str, repo: str) -> tuple[dict[str, object] | None, str | None]:
    """Read the newest exact trusted local-run dispatch state.

    ``read_dispatch_state`` filters through the trusted-author comment
    filter, so a human-authored local comment can never become the claim.
    Returns ``(claim, None)`` or ``(None, reason)`` for every unreadable or
    unexpected state.
    """

    try:
        claim = sm.read_dispatch_state(issue, dispatch_id, repo)
    except sm.StateUnavailableError:
        return None, "dispatch_state_unavailable"
    if claim is None:
        return None, "claim_not_found"
    if claim.get("action") != "local-run":
        return None, "claim_action_mismatch"
    return claim, None


def _record_local_ci_state(
    issue: int, pr_number: int, head_sha: str, run_id: int, repo: str
) -> bool:
    """Persist the exact-head CI state for a bound run, or return False.

    The CI state is persisted before any monitor dispatch; a failed write
    never reaches the monitor.
    """

    try:
        requirements = ci_verifier.load_requirements()
    except ci_verifier.CIVerificationError:
        return False
    return sm.record_ci_state(
        issue,
        pr_number,
        head_sha,
        run_id,
        "dispatched",
        extra={
            "workflow_name": requirements["workflow_name"],
            "required_jobs": requirements["required_jobs"],
            "successful_jobs": [],
            "workflow_run_id": run_id,
        },
        repo=repo,
    )


def _acquire_local_ci(issue: int, pr_number: int, branch: str, head_sha: str, repo: str) -> int | None:
    """Reuse a trusted bound acquisition or acquire and persist exact-head CI.

    Returns the exact canonical run id, or None when the acquisition cannot
    be reused, acquired, or persisted.  The acquisition and CI state are
    persisted before any monitor dispatch; a conflicting existing acquisition
    fails closed instead of being overwritten.  A reused acquisition must
    also verify or persist the exact CI state: if exact CI state already
    exists it is reused without a duplicate write, if it is absent it is
    written, and conflicting or unreadable state fails closed.
    """

    try:
        existing = sm.read_ci_acquisition(issue, pr_number, head_sha, repo)
    except sm.StateUnavailableError:
        return None
    if existing is not None:
        run_id = existing.get("workflow_run_id")
        if existing.get("status") != "bound" or type(run_id) is not int or run_id < 1:
            return None
        outcome, ci_state = sm.read_exact_ci_state(
            issue, pr_number, head_sha, run_id, repo
        )
        if outcome == "matched":
            return run_id
        if outcome != "absent":
            return None
        if not _record_local_ci_state(issue, pr_number, head_sha, run_id, repo):
            return None
        return run_id
    try:
        acquisition = ci_verifier.acquire_exact_run(pr_number, branch, head_sha)
    except ci_verifier.CIVerificationError:
        return None
    run_id = acquisition.get("workflow_run_id")
    if type(run_id) is not int or run_id < 1:
        return None
    metadata = {
        "observed_run_ids": acquisition.get("observed_run_ids", []),
        "selection_reason": acquisition.get("selection_reason", ""),
        "superseded_run_ids": acquisition.get("superseded_run_ids", []),
        "unsupported_run_ids": acquisition.get("unsupported_run_ids", []),
        "fallback_dispatched": bool(acquisition.get("fallback_dispatched", False)),
        "bound_status": acquisition.get("bound_status", ""),
    }
    try:
        recorded = sm.record_ci_acquisition(
            issue,
            pr_number,
            head_sha,
            run_id,
            str(acquisition.get("source", "unknown")),
            duplicate_run_ids=acquisition.get("duplicate_run_ids", []),
            repo=repo,
            metadata=metadata,
        )
    except (ValueError, TypeError):
        return None
    if not recorded:
        return None
    if not _record_local_ci_state(issue, pr_number, head_sha, run_id, repo):
        return None
    return run_id


def _dispatch_local_monitor(
    issue: int, pr_number: int, head_sha: str, run_id: int, repo: str
) -> tuple[bool, str]:
    """Dispatch the trusted agent-ci-monitor exactly once per bound run.

    The durable ``ci-monitor`` receipt has a three-state lifecycle so a
    retry can never report success for an unproven dispatch and can never
    issue a second monitor run:

    * ``pending`` is persisted before the external dispatch request.  A
      receipt in this state means no dispatch outcome is known: a retry
      fails closed with ``monitor_dispatch_outcome_unknown`` and never
      dispatches again.
    * ``outcome_unknown`` is persisted (or retained) when the external
      dispatch failed or its outcome is ambiguous.  A retry fails closed
      with the same reason and never dispatches again.
    * ``dispatched`` is persisted only after a proven successful workflow
      run request.  An exact already-``dispatched`` trusted receipt is an
      idempotent success; a conflicting receipt fails closed.

    If the post-success ``dispatched`` receipt write fails, the outcome is
    reported unknown and never a success: the retry reads ``pending`` and
    fails closed without a second dispatch.  Every dispatch attempt is at
    most one per bound run.
    """

    receipt = f"ci-monitor:{pr_number}:{head_sha}:{run_id}"
    expected = {
        "issue_number": issue,
        "pr_number": pr_number,
        "head_sha": head_sha,
        "ci_run_id": run_id,
        "workflow": MONITOR_WORKFLOW,
    }
    try:
        previous = sm.read_dispatch_state(issue, receipt, repo)
    except sm.StateUnavailableError:
        return False, "monitor_receipt_unavailable"
    if previous is not None:
        if (
            previous.get("action") != MONITOR_RECEIPT_ACTION
            or not sm.dispatch_state_binding_matches(
                previous, issue, MONITOR_RECEIPT_ACTION, expected
            )
        ):
            return False, "monitor_receipt_conflict"
        status = previous.get("status")
        if status == "dispatched":
            return True, "already_dispatched"
        if status in {"pending", "outcome_unknown"}:
            return False, "monitor_dispatch_outcome_unknown"
        return False, "monitor_receipt_conflict"
    if not sm.record_dispatch_state(
        issue, receipt, MONITOR_RECEIPT_ACTION, "pending", expected, repo
    ):
        return False, "monitor_receipt_failed"
    if not _run_workflow(
        MONITOR_WORKFLOW,
        {"issue": issue, "pr": pr_number, "head_sha": head_sha, "ci_run_id": run_id},
    ):
        sm.record_dispatch_state(
            issue, receipt, MONITOR_RECEIPT_ACTION, "outcome_unknown", expected, repo
        )
        return False, "monitor_dispatch_outcome_unknown"
    if not sm.record_dispatch_state(
        issue, receipt, MONITOR_RECEIPT_ACTION, "dispatched", expected, repo
    ):
        # The proven dispatch happened but the success receipt could not be
        # persisted.  Report unknown; the pending receipt makes any retry
        # fail closed instead of dispatching a second monitor run.
        sm.record_dispatch_state(
            issue, receipt, MONITOR_RECEIPT_ACTION, "outcome_unknown", expected, repo
        )
        return False, "monitor_dispatch_outcome_unknown"
    return True, "dispatched"


def _claim_structure_error(claim: object, issue: int, dispatch_id: str) -> str | None:
    """Return a fail-closed reason when a read claim's top-level identity is invalid.

    ``read_dispatch_state`` normally filters trusted dispatch-state comments,
    but the gateways validate the top-level ``kind``/``version``/
    ``issue_number``/``dispatch_id`` identity themselves so a mocked or
    unusually-formatted read can never be treated as the exact claim.
    """

    if not isinstance(claim, dict):
        return "claim_malformed"
    if claim.get("kind") != "agent-orchestrator-dispatch-state":
        return "claim_malformed"
    if claim.get("version") != 1:
        return "claim_malformed"
    if claim.get("issue_number") != int(issue):
        return "claim_malformed"
    if claim.get("dispatch_id") != dispatch_id:
        return "claim_malformed"
    return None


def handoff_local(
    issue: int, attempt_id: str, client_token: str, expected_head_sha: str
) -> dict[str, object]:
    """Trusted server-side handoff of a claimed local-run attempt to its PR.

    The local process supplies only the Issue, canonical attempt id, client
    token, and the exact head sha it pushed; the server re-derives
    repository/default-branch/accepted-main authority, revalidates the
    newest exact trusted local-run claim binding, revalidates live control,
    the active label, the current Issue body digest/scope, the unchanged
    accepted main head, and the single open Draft PR for the canonical
    branch through the ``pr_binding`` owner's authoritative final view
    (number/base/head/isDraft/same-repo/marker/closing-link, exact head
    branch and head sha), then idempotently persists the local-run worker
    state, reuses or acquires exact-head CI, and dispatches the trusted
    agent-ci-monitor under a durable receipt.  Every validation precedes
    every worker-state/CI/monitor write; no provider call is ever made by a
    local process.
    """

    attempt = _normalized_attempt_id(attempt_id)
    if attempt is None:
        return {"handed_off": False, "issue": issue, "reason": "invalid_attempt_id"}
    if not isinstance(client_token, str) or sm.CLAIM_NONCE_PATTERN.fullmatch(client_token) is None:
        return {"handed_off": False, "issue": issue, "reason": "invalid_client_token"}
    if not isinstance(expected_head_sha, str) or local_loop.HEX40.fullmatch(expected_head_sha) is None:
        return {"handed_off": False, "issue": issue, "reason": "invalid_head_sha"}
    dispatch_id = _local_dispatch_id(issue, attempt)
    repo = _repo()
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"handed_off": False, "issue": issue, "reason": "disabled_or_emergency_stopped"}
    if not repo:
        return {"handed_off": False, "issue": issue, "reason": "repository_unavailable"}
    try:
        adapter = local_loop.GitHubAdapter(repo)
    except ValueError:
        return {"handed_off": False, "issue": issue, "reason": "repository_malformed"}
    try:
        metadata = adapter.repository_metadata()
        branch = metadata.get("default_branch")
        if not isinstance(branch, str) or not local_loop.BRANCH.fullmatch(branch):
            return {"handed_off": False, "issue": issue, "reason": "default_branch_unavailable"}
        claim_main = adapter.accepted_main_sha(branch)
    except local_loop.LoopUnavailable:
        return {"handed_off": False, "issue": issue, "reason": "repository_state_unavailable"}
    if not local_loop.HEX40.fullmatch(claim_main):
        return {"handed_off": False, "issue": issue, "reason": "accepted_main_unavailable"}
    canonical_branch = f"agent/issue-{issue}"
    claim, read_reason = _read_local_claim(issue, dispatch_id, repo)
    if claim is None:
        return {"handed_off": False, "issue": issue, "reason": read_reason}
    structure_error = _claim_structure_error(claim, issue, dispatch_id)
    if structure_error is not None:
        return {"handed_off": False, "issue": issue, "reason": structure_error}
    status = claim.get("status")
    if status == "claimed":
        return {"handed_off": False, "issue": issue, "reason": "dispatch_in_flight"}
    if status != "dispatched":
        return {"handed_off": False, "issue": issue, "reason": "claim_state_unexpected"}
    details = claim.get("details")
    binding_ok, binding_reason = sm.local_claim_binding_valid(
        issue, details, attempt, client_token
    )
    if not binding_ok:
        return {"handed_off": False, "issue": issue, "reason": binding_reason}
    if details.get("accepted_main_sha") != claim_main:
        return {"handed_off": False, "issue": issue, "reason": "accepted_main_moved"}
    # Live server-side revalidation; every check below must pass before any
    # worker-state, CI, or monitor write.
    labels = sm.get_issue_labels_checked(issue, repo)
    if labels is None:
        return {"handed_off": False, "issue": issue, "reason": "label_state_unavailable"}
    if sm.LABEL_RUNNING not in labels:
        return {"handed_off": False, "issue": issue, "reason": "issue_not_running"}
    body = sm.get_issue_body(issue, repo)
    if not isinstance(body, str):
        return {"handed_off": False, "issue": issue, "reason": "task_body_unavailable"}
    try:
        current_binding = artifact_contract.build_issue_scope_binding(body)
    except (artifact_contract.ArtifactContractError, ValueError, TypeError) as exc:
        return {"handed_off": False, "issue": issue, "reason": f"invalid_scope:{exc}"}
    if current_binding["task_body_sha256"] != details["task_body_sha256"]:
        return {"handed_off": False, "issue": issue, "reason": "task_body_changed"}
    if current_binding["allowed_paths"] != details["allowed_paths"]:
        return {"handed_off": False, "issue": issue, "reason": "scope_changed"}
    try:
        pr = pr_binding.find_issue_pr(issue, canonical_branch, expected_head_sha, repo)
    except pr_binding.PRBindingError as exc:
        return {"handed_off": False, "issue": issue, "reason": f"pr_binding_rejected:{exc}"}
    # The returned final verify view is the only authoritative source for the
    # PR number, base/head refs, Draft state, and head repository; the
    # discovery list snapshot is never trusted for these fields.  The
    # pr_binding owner has already required the exact canonical head branch
    # and head sha; the checks below are defense in depth on the verified
    # view and its repository defaults.
    if not isinstance(pr.get("number"), int):
        return {"handed_off": False, "issue": issue, "reason": "pr_binding_rejected:bound PR number is invalid"}
    if pr.get("baseRefName") != branch:
        return {"handed_off": False, "issue": issue, "reason": "pr_base_mismatch"}
    pr_number = pr.get("number")
    head_sha = pr.get("headRefOid")
    if not isinstance(head_sha, str) or not local_loop.HEX40.fullmatch(head_sha):
        return {"handed_off": False, "issue": issue, "reason": "pr_head_unavailable"}
    if head_sha != expected_head_sha:
        return {"handed_off": False, "issue": issue, "reason": "pr_head_mismatch"}
    if pr.get("isDraft") is not True:
        return {"handed_off": False, "issue": issue, "reason": "pr_binding_rejected:bound PR is not a Draft"}
    head_repo = pr.get("headRepository")
    if not isinstance(head_repo, dict) or head_repo.get("nameWithOwner") != repo:
        return {"handed_off": False, "issue": issue, "reason": "pr_binding_rejected:PR head repository is not the target repository"}
    recorded, record_reason = sm.record_local_worker_state(
        issue,
        pr_number,
        head_sha,
        branch=canonical_branch,
        attempt_id=attempt,
        dispatch_id=dispatch_id,
        claim_nonce=details["claim_nonce"],
        repo=repo,
    )
    if not recorded:
        return {"handed_off": False, "issue": issue, "reason": f"worker_state_failed:{record_reason}"}
    run_id = _acquire_local_ci(issue, pr_number, canonical_branch, head_sha, repo)
    if run_id is None:
        return {"handed_off": False, "issue": issue, "reason": "ci_acquisition_failed"}
    monitor_ok, monitor_reason = _dispatch_local_monitor(
        issue, pr_number, head_sha, run_id, repo
    )
    if not monitor_ok:
        return {"handed_off": False, "issue": issue, "reason": monitor_reason}
    return {
        "handed_off": True,
        "issue": issue,
        "pr_number": pr_number,
        "head_sha": head_sha,
        "ci_run_id": run_id,
        "dispatch_id": dispatch_id,
    }


def release_local(
    issue: int, attempt_id: str, client_token: str, reason_code: str
) -> dict[str, object]:
    """Terminalize a trusted local-run claim for a known pre-handoff failure.

    Only an exact claim match (canonical attempt id, client token, and the
    complete persisted binding) may release.  An active claimed/dispatched
    release requires the full binding including an unexpired lease; an exact
    already-persisted failed terminal retry stays idempotent after lease
    expiry (immutable binding syntax, attempt/token/reason) and returns
    already-released without rewriting when the Issue is already
    agent-blocked.  The trusted terminal failed local-run state is persisted
    with the entire binding and the bounded allowlisted reason before the
    Issue moves from agent-running to agent-blocked.  Exact retries are
    idempotent; wrong attempt/token, conflicting terminal, or unreadable
    state fails closed before any mutation.  This slice performs no lease
    recovery and never queries Actions jobs or dispatches any workflow.
    """

    attempt = _normalized_attempt_id(attempt_id)
    if attempt is None:
        return {"released": False, "issue": issue, "reason": "invalid_attempt_id"}
    if not isinstance(client_token, str) or sm.CLAIM_NONCE_PATTERN.fullmatch(client_token) is None:
        return {"released": False, "issue": issue, "reason": "invalid_client_token"}
    if reason_code not in sm.LOCAL_RELEASE_REASONS:
        return {"released": False, "issue": issue, "reason": "invalid_reason_code"}
    dispatch_id = _local_dispatch_id(issue, attempt)
    repo = _repo()
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"released": False, "issue": issue, "reason": "disabled_or_emergency_stopped"}
    if not repo:
        return {"released": False, "issue": issue, "reason": "repository_unavailable"}
    claim, read_reason = _read_local_claim(issue, dispatch_id, repo)
    if claim is None:
        return {"released": False, "issue": issue, "reason": read_reason}
    structure_error = _claim_structure_error(claim, issue, dispatch_id)
    if structure_error is not None:
        return {"released": False, "issue": issue, "reason": structure_error}
    status = claim.get("status")
    if status not in {"claimed", "dispatched", "failed"}:
        return {"released": False, "issue": issue, "reason": "claim_state_unexpected"}
    details = claim.get("details")
    # An active claimed/dispatched release requires the full binding including
    # an unexpired lease.  An exact already-persisted failed terminal retry
    # must stay idempotent after lease expiry: the immutable binding syntax,
    # attempt/token/reason still validate, but the lease no longer needs to
    # be live.
    binding_ok, binding_reason = sm.local_claim_binding_valid(
        issue, details, attempt, client_token,
        require_lease_live=status != "failed",
    )
    if not binding_ok:
        return {"released": False, "issue": issue, "reason": binding_reason}
    # Generation guard: a stale release retry for terminal attempt A must
    # never release the active capacity of a newer local claim generation B.
    # The exact-``dispatch_id`` read above validates A's binding but would
    # still authorize a release while B's claim owns the Issue.  Reclassify
    # the newest relevant local-run dispatch state (``dispatch_id`` plus the
    # per-claim ``claim_nonce`` discriminator) here, before any terminal
    # write or label mutation: only ``own-active``/``own-terminal`` (the
    # caller's exact generation is the newest) proceed; a newer generation,
    # absent, or unverifiable/API-unavailable state fails closed with no
    # mutation at all.
    outcome, payload = sm.release_local_claim_outcome(
        issue, dispatch_id, details.get("claim_nonce"), repo
    )
    if outcome == "superseded":
        return {"released": False, "issue": issue, "reason": "superseded"}
    if outcome == "unverifiable":
        return {"released": False, "issue": issue, "reason": payload}
    if outcome != "own-active" and outcome != "own-terminal":
        return {"released": False, "issue": issue, "reason": "claim_not_found"}
    if status == "failed":
        if details.get("reason") != reason_code:
            return {"released": False, "issue": issue, "reason": "conflicting_terminal_state"}
    else:
        # The terminal failed state preserving the entire binding is
        # persisted before any label change, so a crash can never leave an
        # un-owned agent-running label behind.
        terminal_details = dict(details)
        terminal_details["reason"] = reason_code
        if not sm.record_dispatch_state(
            issue, dispatch_id, "local-run", "failed", terminal_details, repo
        ):
            return {"released": False, "issue": issue, "reason": "claim_state_failed_write"}
    released, release_reason = sm.release_failed_capacity(
        issue, sm.LABEL_RUNNING, sm.LABEL_BLOCKED, repo=repo
    )
    if not released:
        return {"released": False, "issue": issue, "reason": f"capacity_release_failed:{release_reason}"}
    return {"released": True, "issue": issue, "dispatch_id": dispatch_id, "reason": reason_code}


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
            "retry-review|dispatch-merge|dispatch-next|claim-local|handoff-local|"
            "release-local> ..."
        )
    command = sys.argv[1]
    if command == "dispatch-ready" and len(sys.argv) in {3, 4}:
        result = dispatch_ready(int(sys.argv[2]), sys.argv[3] if len(sys.argv) == 4 else None)
    elif command == "claim-local" and len(sys.argv) == 5:
        result = claim_local(int(sys.argv[2]), sys.argv[3], sys.argv[4])
    elif command == "handoff-local" and len(sys.argv) == 6:
        result = handoff_local(int(sys.argv[2]), sys.argv[3], sys.argv[4], sys.argv[5])
    elif command == "release-local" and len(sys.argv) == 6:
        result = release_local(int(sys.argv[2]), sys.argv[3], sys.argv[4], sys.argv[5])
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
    if (
        result.get("dispatched") is False
        or result.get("handed_off") is False
        or result.get("released") is False
    ):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
