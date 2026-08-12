"""Controller-owned plan-packet lifecycle transitions on the Plan Execution Ledger.

A plan subject traverses ``claim -> dispatch -> handoff -> CI -> review ->
merge -> closeout``.  The existing CI monitor, review, repository-maintenance
merge, and canonical closeout owners perform the work; this module records
their verified receipts on the Plan Execution Ledger as controller-owned
transitions and reconstructs the lifecycle idempotently from the ledger plus
authoritative GitHub/PR/CI/review state.

No model self-report advances state.  Every recorder validates the exact
subject binding (packet id, attempt, dispatch id, PR number, exact head SHA)
against the durable ledger claim before writing; every write is idempotent;
every conflict or missing transition fails closed.  The CI and review
transitions are verified readbacks of the existing owners' own ledger
recordings; the merge and closeout transitions are written here only after
authoritative GitHub/PR state is verified by the caller.
"""

from __future__ import annotations

import json
import re
import uuid
from typing import Any

import local_loop
import plan_lane
import state_manager as sm

_ATTEMPT_PATTERN = re.compile(
    r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
)
_CLOSEOUT_REFERENCE_PATTERN = re.compile(
    r"^(?P<canonical>PR #(?P<pr>[1-9][0-9]*) exact head `(?P<head>[0-9a-f]{40})`; merge "
    r"`(?P<merge>[0-9a-f]{40})`; exact-head `PASS`; canonical workflow "
    r"`(?P<workflow>[1-9][0-9]*)`)(?:; (?P<detail>[^\r\n]{1,1800}))?$"
)
_LEGACY_CLOSEOUT_REFERENCE_PATTERN = re.compile(r"^PR #(?P<pr>[1-9][0-9]*)$")
_TERMINAL_PACKET_STATE_PATTERN = re.compile(r"^[a-z0-9_]{1,64}$")


def _normalized_attempt_id(value: object) -> str | None:
    """Return the canonical lowercase-hyphenated UUID attempt id, or None."""

    if not isinstance(value, str) or _ATTEMPT_PATTERN.fullmatch(value) is None:
        return None
    try:
        parsed = uuid.UUID(value)
    except ValueError:
        return None
    return value if value == str(parsed) else None


def canonical_closeout_reference(
    pr_number: object,
    head_sha: object,
    merge_commit_sha: object,
    workflow_run_id: object,
) -> str | None:
    """Render the one closeout reference accepted by routing and status rows."""

    if (
        type(pr_number) is not int
        or pr_number < 1
        or not isinstance(head_sha, str)
        or local_loop.HEX40.fullmatch(head_sha) is None
        or not isinstance(merge_commit_sha, str)
        or local_loop.HEX40.fullmatch(merge_commit_sha) is None
        or type(workflow_run_id) is not int
        or workflow_run_id < 1
    ):
        return None
    return (
        f"PR #{pr_number} exact head `{head_sha}`; merge `{merge_commit_sha}`; "
        f"exact-head `PASS`; canonical workflow `{workflow_run_id}`"
    )


def canonical_closeout_reference_match(value: object) -> re.Match[str] | None:
    """Parse a bounded canonical closeout reference and ignored status detail."""

    if not isinstance(value, str) or len(value.encode("utf-8")) > 2 * 1024:
        return None
    return _CLOSEOUT_REFERENCE_PATTERN.fullmatch(value)


def reconcile_legacy_closeout_reference(
    ledger_issue: int,
    packet_id: str,
    attempt_id: str,
    closeout_reference: object,
    repo: str = "",
) -> str | None:
    """Rebuild one pre-canonical ``PR #n`` receipt from trusted ledger state.

    The function is read-only.  It accepts no generic legacy prose: the legacy
    PR number must match the exact subject's current worker binding, and all
    CI/review/merge/closeout transitions must be independently readable before
    it returns a canonical reference for the route compiler.
    """

    canonical = canonical_closeout_reference_match(closeout_reference)
    legacy = (
        _LEGACY_CLOSEOUT_REFERENCE_PATTERN.fullmatch(closeout_reference)
        if isinstance(closeout_reference, str)
        else None
    )
    if canonical is None and legacy is None:
        return None
    lifecycle = read_plan_lifecycle(ledger_issue, packet_id, attempt_id, repo)
    if not isinstance(lifecycle, dict):
        return None
    stages = lifecycle.get("stages")
    if (
        lifecycle.get("claim_status") != "closed_out"
        or (
            legacy is not None
            and lifecycle.get("pr_number") != int(legacy.group("pr"))
        )
        or not isinstance(stages, dict)
        or not all(stages.get(stage) is True for stage in ("ci", "review", "merge", "closeout"))
    ):
        return None
    transitions = lifecycle.get("transitions")
    if not isinstance(transitions, dict):
        return None
    ci = transitions.get("ci")
    merge = transitions.get("merge")
    head_sha = lifecycle.get("head_sha")
    if not isinstance(ci, dict) or not isinstance(merge, dict):
        return None
    rebuilt = canonical_closeout_reference(
        lifecycle.get("pr_number"),
        head_sha,
        merge.get("merge_commit_sha"),
        ci.get("workflow_run_id"),
    )
    if rebuilt is None:
        return None
    if canonical is not None and canonical.group("canonical") != rebuilt:
        return None
    return rebuilt


def _plan_claim(ledger_issue: int, dispatch_id: str, repo: str) -> dict[str, Any] | None:
    """Read one exact plan-run ledger claim, failing closed on unreadable state."""

    try:
        claim = sm.read_dispatch_state(ledger_issue, dispatch_id, repo)
    except sm.StateUnavailableError:
        return None
    if not isinstance(claim, dict) or claim.get("action") != "plan-run":
        return None
    return claim


def _exact_plan_claim(ledger_issue: int, packet_id: str, attempt: str, repo: str) -> dict[str, Any] | None:
    """Resolve the single plan-run claim bound to one packet/attempt generation.

    Mirrors the supervisor's exact-match rule: exactly one trusted claim with
    a valid dispatch id and a matching subject/attempt binding may exist.
    Any ambiguity or malformed state fails closed to ``None``.
    """

    try:
        comments = sm.get_issue_comments(ledger_issue, repo)
    except sm.StateUnavailableError:
        return None
    matches: list[dict[str, Any]] = []
    seen_dispatch_ids: set[str] = set()
    for comment in comments:
        if (comment.get("author") or {}).get("login") not in sm.TRUSTED_STATE_AUTHORS:
            continue
        body = comment.get("body", "")
        if not isinstance(body, str) or "agent-orchestrator-dispatch-state" not in body:
            continue
        try:
            state = json.loads(body)
        except (json.JSONDecodeError, TypeError):
            return None
        if (
            not isinstance(state, dict)
            or state.get("kind") != "agent-orchestrator-dispatch-state"
            or state.get("version") != 1
            or state.get("issue_number") != ledger_issue
            or state.get("action") != "plan-run"
        ):
            continue
        dispatch_id = state.get("dispatch_id")
        details = state.get("details")
        if (
            not isinstance(dispatch_id, str)
            or sm.PLAN_DISPATCH_ID_PATTERN.fullmatch(dispatch_id) is None
            or not isinstance(details, dict)
            or details.get("subject_kind") != "plan-packet"
            or details.get("subject_id") != packet_id
            or details.get("attempt_id") != attempt
        ):
            continue
        if dispatch_id in seen_dispatch_ids:
            continue
        seen_dispatch_ids.add(dispatch_id)
        matches.append(state)
    if len(matches) != 1:
        return None
    return matches[0]


def plan_ci_receipt(ledger_issue: int, pr_number: int, head_sha: str, repo: str = "") -> dict[str, Any] | None:
    """Return the exact-head terminal CI receipt recorded on the ledger, or None.

    The CI receipt is the newest trusted CI state on the ledger bound to the
    exact ``(pr_number, head_sha)`` with a terminal status; anything else
    (absent, conflicting binding, or non-terminal) reads as ``None``.
    """

    state = sm.read_ci_state(ledger_issue, repo)
    if not isinstance(state, dict):
        return None
    if state.get("pr_number") != int(pr_number) or state.get("head_sha") != head_sha:
        return None
    if state.get("status") != "terminal_success":
        return None
    return state


def plan_review_receipt(
    ledger_issue: int,
    pr_number: int,
    head_sha: str,
    repo: str = "",
    expected_base_sha: str = "",
) -> dict[str, Any] | None:
    """Return the exact-head PASS review receipt recorded on the ledger, or None.

    The review receipt is the newest trusted review state on the ledger bound
    to the exact ``(pr_number, head_sha)`` with a PASS verdict; anything else
    reads as ``None``.
    """

    live_binding: dict[str, Any] | None = None
    if repo:
        binding_ok, _binding_reason, live_binding = sm.resolve_live_review_binding(
            pr_number, head_sha, repo, expected_base_sha
        )
        if not binding_ok or live_binding["head_sha"] != head_sha:
            return None
    if repo:
        state = sm.read_review_state(ledger_issue, repo)
    else:
        body = sm.get_issue_comment_bodies(
            ledger_issue, "agent-orchestrator-review-state", repo
        )
        try:
            state = json.loads(body) if body else None
        except json.JSONDecodeError:
            state = None
    if not isinstance(state, dict):
        return None
    if state.get("pr_number") != int(pr_number) or state.get("head_sha") != head_sha:
        return None
    if state.get("verdict") not in {"PASS", "pass"}:
        return None
    if live_binding is not None:
        if (
            state.get("base_sha") != live_binding["base_sha"]
            or state.get("reviewed_range") != live_binding["reviewed_range"]
        ):
            return None
        rebound_ok, _rebound_reason, rebound = sm.resolve_live_review_binding(
            pr_number, head_sha, repo, live_binding["base_sha"]
        )
        if not rebound_ok or rebound["reviewed_range"] != live_binding["reviewed_range"]:
            return None
    return state


def plan_merge_receipt(ledger_issue: int, pr_number: int, head_sha: str, repo: str = "") -> dict[str, Any] | None:
    """Return the exact-head maintainer merge receipt on the ledger, or None.

    The merge receipt is the newest trusted merge state on the ledger bound
    to the exact ``(pr_number, head_sha)`` with status ``confirmed`` and a
    well-formed merge commit SHA; anything else reads as ``None``.
    """

    body = sm.get_issue_comment_bodies(ledger_issue, "agent-orchestrator-merge-state", repo)
    if not body:
        return None
    try:
        state = json.loads(body)
    except json.JSONDecodeError:
        return None
    if (
        not isinstance(state, dict)
        or state.get("kind") != "agent-orchestrator-merge-state"
        or state.get("pr_number") != int(pr_number)
        or state.get("expected_head_sha") != head_sha
        or state.get("status") != "confirmed"
    ):
        return None
    if not isinstance(state.get("merge_commit_sha"), str) or local_loop.HEX40.fullmatch(state["merge_commit_sha"]) is None:
        return None
    return state


def record_plan_merge_receipt(
    ledger_issue: int,
    packet_id: str,
    attempt_id: str,
    source_main_sha: str,
    pr_number: int,
    head_sha: str,
    merge_commit_sha: str,
    repo: str = "",
) -> dict[str, Any]:
    """Record the verified maintainer merge receipt on the ledger idempotently.

    The caller must already have verified the merge against authoritative
    GitHub state; this recorder only validates the subject binding and the
    bounded identity fields, then persists through the existing merge-state
    owner.  An identical existing receipt is an idempotent success; any
    conflicting receipt, unprovable claim, or malformed identity fails closed.
    """

    attempt = _normalized_attempt_id(attempt_id)
    if (
        not isinstance(packet_id, str)
        or plan_lane.PACKET_ID.fullmatch(packet_id) is None
        or attempt is None
        or local_loop.HEX40.fullmatch(source_main_sha) is None
        or type(pr_number) is not int
        or pr_number <= 0
        or local_loop.HEX40.fullmatch(head_sha) is None
        or local_loop.HEX40.fullmatch(merge_commit_sha) is None
    ):
        return {"recorded": False, "reason": "merge_receipt_identity_invalid"}
    dispatch_id = f"plan-run:{packet_id}:{source_main_sha}:{attempt}"
    claim = _plan_claim(ledger_issue, dispatch_id, repo)
    if claim is None:
        return {"recorded": False, "reason": "plan_claim_not_found"}
    if claim.get("status") == "closed_out":
        existing = plan_merge_receipt(ledger_issue, pr_number, head_sha, repo)
        if existing is not None and existing.get("merge_commit_sha") == merge_commit_sha:
            return {"recorded": True, "reason": "already_verified"}
        return {"recorded": False, "reason": "conflicting_terminal_state"}
    if claim.get("status") != "dispatched":
        return {"recorded": False, "reason": "plan_claim_state_unexpected"}
    existing = plan_merge_receipt(ledger_issue, pr_number, head_sha, repo)
    if existing is not None:
        if existing.get("merge_commit_sha") == merge_commit_sha:
            return {"recorded": True, "reason": "already_recorded"}
        return {"recorded": False, "reason": "conflicting_merge_receipt"}
    if not sm.record_merge_state(ledger_issue, pr_number, head_sha, merge_commit_sha, repo):
        return {"recorded": False, "reason": "merge_receipt_write_failed"}
    return {"recorded": True, "reason": "recorded"}


def record_plan_closeout_receipt(
    ledger_issue: int,
    packet_id: str,
    attempt_id: str,
    source_main_sha: str,
    pr_number: int,
    head_sha: str,
    terminal_packet_state: str,
    closeout_reference: str,
    repo: str = "",
) -> dict[str, Any]:
    """Record the canonical closeout receipt and terminal packet state.

    The closeout requires all three prior terminal receipts (CI, review,
    merge) bound to the exact head on the ledger; any missing transition
    fails closed without writing.  On success the plan-run claim becomes
    terminal ``closed_out`` with the bounded closeout evidence, and the
    ledger label moves ``agent-running`` -> ``agent-complete``.  Re-entry
    after a partial write is idempotent.
    """

    attempt = _normalized_attempt_id(attempt_id)
    closeout_match = canonical_closeout_reference_match(closeout_reference)
    if (
        not isinstance(packet_id, str)
        or plan_lane.PACKET_ID.fullmatch(packet_id) is None
        or attempt is None
        or local_loop.HEX40.fullmatch(source_main_sha) is None
        or type(pr_number) is not int
        or pr_number <= 0
        or local_loop.HEX40.fullmatch(head_sha) is None
        or _TERMINAL_PACKET_STATE_PATTERN.fullmatch(terminal_packet_state) is None
        or closeout_match is None
    ):
        return {"recorded": False, "reason": "closeout_identity_invalid"}
    receipts = {}
    for stage in ("ci", "review", "merge"):
        present = {
            "ci": plan_ci_receipt,
            "review": plan_review_receipt,
            "merge": plan_merge_receipt,
        }[stage](ledger_issue, pr_number, head_sha, repo)
        if present is None:
            return {"recorded": False, "reason": f"missing_transition:{stage}"}
        receipts[stage] = present
    assert closeout_match is not None
    ci_receipt = receipts["ci"]
    merge_receipt = receipts["merge"]
    if (
        closeout_match.group("pr") != str(pr_number)
        or closeout_match.group("head") != head_sha
        or closeout_match.group("merge") != merge_receipt.get("merge_commit_sha")
        or closeout_match.group("workflow") != str(ci_receipt.get("workflow_run_id"))
    ):
        return {"recorded": False, "reason": "closeout_reference_binding_invalid"}
    dispatch_id = f"plan-run:{packet_id}:{source_main_sha}:{attempt}"
    claim = _plan_claim(ledger_issue, dispatch_id, repo)
    if claim is None:
        return {"recorded": False, "reason": "plan_claim_not_found"}
    if claim.get("status") == "closed_out":
        if not _closeout_labels(ledger_issue, repo):
            return {"recorded": False, "reason": "closeout_label_failed"}
        return {"recorded": True, "reason": "already_closed_out"}
    if claim.get("status") != "dispatched":
        return {"recorded": False, "reason": "plan_claim_state_unexpected"}
    details = claim.get("details")
    if not isinstance(details, dict):
        return {"recorded": False, "reason": "plan_claim_details_invalid"}
    terminal_details = {
        **details,
        "terminal_packet_state": terminal_packet_state,
        "closeout_reference": closeout_reference,
    }
    if not sm.record_dispatch_state(
        ledger_issue, dispatch_id, "plan-run", "closed_out", terminal_details, repo
    ):
        return {"recorded": False, "reason": "closeout_state_write_failed"}
    if not _closeout_labels(ledger_issue, repo):
        return {"recorded": False, "reason": "closeout_label_failed"}
    return {"recorded": True, "reason": "closed_out"}


def _closeout_labels(ledger_issue: int, repo: str) -> bool:
    """Move the ledger label ``agent-running`` -> ``agent-complete``."""

    labels = sm.get_issue_labels_checked(ledger_issue, repo)
    if labels is None:
        return False
    if sm.LABEL_RUNNING not in labels:
        return sm.LABEL_COMPLETE in labels
    if not sm.set_labels(ledger_issue, sm.LABEL_COMPLETE, repo=repo):
        return False
    return sm.remove_labels(ledger_issue, sm.LABEL_RUNNING, repo=repo)


def read_plan_lifecycle(ledger_issue: int, packet_id: str, attempt_id: str, repo: str = "") -> dict[str, Any]:
    """Reconstruct one plan subject's lifecycle idempotently from the ledger.

    Resolves the single exact claim, derives the bound PR/head from the
    worker state, and reads back the four terminal transitions (CI, review,
    merge, closeout).  Any ambiguity, malformed trusted state, or unreadable
    ledger fails closed: stages read as not done rather than as success.
    """

    attempt = _normalized_attempt_id(attempt_id)
    if (
        not isinstance(packet_id, str)
        or plan_lane.PACKET_ID.fullmatch(packet_id) is None
        or attempt is None
    ):
        return {"packet_id": packet_id, "attempt_id": str(attempt_id), "stages": {
            "ci": False, "review": False, "merge": False, "closeout": False,
        }, "reason": "lifecycle_identity_invalid"}
    claim = _exact_plan_claim(ledger_issue, packet_id, attempt, repo)
    if claim is None:
        return {"packet_id": packet_id, "attempt_id": attempt, "stages": {
            "ci": False, "review": False, "merge": False, "closeout": False,
        }, "claim": None, "reason": "plan_claim_not_found"}
    try:
        worker = sm.read_worker_state(ledger_issue, repo)
    except sm.StateUnavailableError:
        worker = None
    if not isinstance(worker, dict) or worker.get("worker_type") != "plan-run":
        return {"packet_id": packet_id, "attempt_id": attempt, "claim": claim, "stages": {
            "ci": False, "review": False, "merge": False, "closeout": False,
        }, "reason": "worker_state_unavailable"}
    pr_number = worker.get("pr_number")
    head_sha = worker.get("head_sha")
    if type(pr_number) is not int or pr_number <= 0 or not isinstance(head_sha, str) or local_loop.HEX40.fullmatch(head_sha) is None:
        return {"packet_id": packet_id, "attempt_id": attempt, "claim": claim, "stages": {
            "ci": False, "review": False, "merge": False, "closeout": False,
        }, "reason": "worker_binding_invalid"}
    ci = plan_ci_receipt(ledger_issue, pr_number, head_sha, repo)
    review = plan_review_receipt(ledger_issue, pr_number, head_sha, repo)
    merge = plan_merge_receipt(ledger_issue, pr_number, head_sha, repo)
    closeout = None
    if claim.get("status") == "closed_out":
        details = claim.get("details")
        closeout = {
            "status": "closed_out",
            "terminal_packet_state": details.get("terminal_packet_state") if isinstance(details, dict) else None,
            "closeout_reference": details.get("closeout_reference") if isinstance(details, dict) else None,
        }
    return {
        "packet_id": packet_id,
        "attempt_id": attempt,
        "ledger_issue": ledger_issue,
        "pr_number": pr_number,
        "head_sha": head_sha,
        "dispatch_id": claim.get("dispatch_id"),
        "claim_status": claim.get("status"),
        "claim": claim,
        "transitions": {"ci": ci, "review": review, "merge": merge, "closeout": closeout},
        "stages": {
            "ci": ci is not None,
            "review": review is not None,
            "merge": merge is not None,
            "closeout": closeout is not None,
        },
    }
