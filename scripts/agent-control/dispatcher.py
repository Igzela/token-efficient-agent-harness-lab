"""The single GitHub-serialized dispatcher for all orchestrator workflow runs."""

from __future__ import annotations

import json
import os
import re
import sys
import uuid
from datetime import datetime, timedelta, timezone

import artifact_contract
import ci_verifier
import control_state
import local_loop
import plan_lane
import plan_lifecycle
import pr_binding
import state_manager as sm


MONITOR_WORKFLOW = "agent-ci-monitor.yml"
MONITOR_RECEIPT_ACTION = "ci-monitor"
LOCAL_UNKNOWN_OUTPUT_REASON = "local_unknown_output"
PLAN_LIFECYCLE_STAGES = frozenset({"ci", "review", "merge", "closeout"})
_SAFE_GITHUB_OPERATOR = re.compile(r"^[A-Za-z0-9](?:[A-Za-z0-9-]{0,37})$")
_MAX_ROUTE_PAYLOAD_BYTES = 8192
_ROUTE_T3_TRANSPORT_KEYS = frozenset({
    "schema_version", "accepted_main_sha", "candidate_digest", "action_digest",
    "scope_digest", "authority_receipt_digest", "outcome_receipt_digest",
    "authority_owner_digest", "decision_source", "decision_evidence_digest",
    "issued_at", "expires_at", "disposition",
})
_ROUTE_OWNER_TRANSPORT_KEYS = frozenset({
    "schema_version", "accepted_main_sha", "candidate_digest",
    "outcome_receipt_digest", "owner_evidence_digest",
})


class RoutePayloadError(ValueError):
    """A route receipt transport payload failed bounded structural validation."""

    def __init__(self, reason: str):
        super().__init__(reason)
        self.reason = reason


def _decode_route_payload(
    raw: object,
    *,
    schema_version: str,
    exact_keys: frozenset[str],
) -> dict[str, str]:
    """Decode one exact-key string-only payload without exposing its contents."""

    if not isinstance(raw, str) or not raw or "\x00" in raw:
        raise RoutePayloadError("route_payload_invalid")
    try:
        encoded = raw.encode("utf-8")
    except UnicodeEncodeError as exc:
        raise RoutePayloadError("route_payload_invalid") from exc
    if len(encoded) > _MAX_ROUTE_PAYLOAD_BYTES:
        raise RoutePayloadError("route_payload_too_large")

    def reject_duplicates(pairs: list[tuple[str, object]]) -> dict[str, object]:
        decoded: dict[str, object] = {}
        for key, value in pairs:
            if key in decoded:
                raise RoutePayloadError("route_payload_duplicate_key")
            decoded[key] = value
        return decoded

    try:
        decoded = json.loads(raw, object_pairs_hook=reject_duplicates)
    except RoutePayloadError:
        raise
    except (json.JSONDecodeError, RecursionError) as exc:
        raise RoutePayloadError("route_payload_invalid") from exc
    if not isinstance(decoded, dict) or set(decoded) != exact_keys:
        raise RoutePayloadError("route_payload_keys_invalid")
    if decoded.get("schema_version") != schema_version:
        raise RoutePayloadError("route_payload_schema_invalid")
    if any(not isinstance(value, str) or "\x00" in value for value in decoded.values()):
        raise RoutePayloadError("route_payload_value_invalid")
    return decoded  # type: ignore[return-value]


def dispatch_route_t3_payload(packet_id: str, raw_payload: object) -> dict[str, object]:
    """Validate the compact workflow transport before entering the T3 owner."""

    try:
        payload = _decode_route_payload(
            raw_payload,
            schema_version="route_t3_transport.v1",
            exact_keys=_ROUTE_T3_TRANSPORT_KEYS,
        )
    except RoutePayloadError as exc:
        return {"authorized": False, "reason": exc.reason}
    return record_route_t3_receipt(
        packet_id,
        payload["accepted_main_sha"],
        payload["candidate_digest"],
        payload["action_digest"],
        payload["scope_digest"],
        payload["authority_receipt_digest"],
        payload["outcome_receipt_digest"],
        payload["authority_owner_digest"],
        os.environ.get("GITHUB_ACTOR", ""),
        payload["decision_source"],
        payload["decision_evidence_digest"],
        payload["issued_at"],
        payload["expires_at"],
        payload["disposition"],
    )


def dispatch_route_owner_payload(packet_id: str, raw_payload: object) -> dict[str, object]:
    """Validate the compact workflow transport before entering the owner receipt."""

    try:
        payload = _decode_route_payload(
            raw_payload,
            schema_version="route_owner_outcome_transport.v1",
            exact_keys=_ROUTE_OWNER_TRANSPORT_KEYS,
        )
    except RoutePayloadError as exc:
        return {"recorded": False, "reason": exc.reason}
    return record_route_owner_outcome(
        packet_id,
        payload["accepted_main_sha"],
        payload["candidate_digest"],
        payload["outcome_receipt_digest"],
        payload["owner_evidence_digest"],
    )


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
    capacity = sm.get_active_capacity(repo)
    if capacity is None:
        return False, [], "capacity_state_unavailable"
    active = capacity["issues"]
    active_plans = capacity["plans"]
    if issue not in active and len(active) + len(active_plans) >= sm.MAX_ACTIVE:
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
        for active_plan in active_plans:
            if artifact_contract.scopes_overlap(
                task_scope or [], active_plan["allowed_paths"]
            ):
                return False, [], f"scope_conflict:{active_plan['subject_id']}"
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
    lease_deadline = (
        datetime.now(timezone.utc) + timedelta(hours=sm.LOCAL_CLAIM_LEASE_HOURS)
    ).isoformat().replace("+00:00", "Z")
    claimed, previous, reason = _claim(
        issue,
        sm.LABEL_RUNNING,
        dispatch_id,
        "worker",
        {
            "issue_number": issue,
            "claim_nonce": claim_nonce,
            "lease_deadline": lease_deadline,
        },
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
    recheck = sm.get_active_capacity(_repo())
    if recheck is None:
        rolled_back = _rollback(issue, dispatch_id, previous, "capacity_recheck_unavailable")
        reason = "capacity_recheck_unavailable" if rolled_back else "capacity_recheck_unavailable_rollback_failed"
        return {"dispatched": False, "issue": issue, "reason": reason}
    if len(recheck["issues"]) + len(recheck["plans"]) > sm.MAX_ACTIVE:
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
    # Resume a claim that was durable-written before labels/dispatch promotion
    # completed.  Only the exact attempt/token binding may finish the claim;
    # a mismatched caller still fails closed through ``_claim``.
    try:
        existing_claim = sm.read_dispatch_state(issue, dispatch_id, repo)
    except sm.StateUnavailableError:
        return {"dispatched": False, "issue": issue, "reason": "dispatch_state_unavailable"}
    if isinstance(existing_claim, dict) and existing_claim.get("status") == "claimed":
        existing_details = existing_claim.get("details")
        if isinstance(existing_details, dict):
            binding_ok, binding_reason = sm.local_claim_binding_valid(
                issue, existing_details, attempt, client_token
            )
            if not binding_ok:
                return {
                    "dispatched": False,
                    "issue": issue,
                    "reason": binding_reason,
                }
            # Resume only when the durable claim still matches live server
            # authority for this attempt.  A mismatched main/scope means the
            # original claim is stale and must not be promoted.
            if (
                existing_details.get("accepted_main_sha") != accepted_main
                or existing_details.get("canonical_branch") != canonical_branch
                or existing_details.get("allowed_paths") != scope_binding["allowed_paths"]
                or existing_details.get("task_body_sha256") != scope_binding["task_body_sha256"]
            ):
                return {
                    "dispatched": False,
                    "issue": issue,
                    "reason": "dispatch_state_binding_unverified",
                }
            labels = sm.get_issue_labels_checked(issue, repo)
            if labels is None:
                return {
                    "dispatched": False,
                    "issue": issue,
                    "reason": "label_state_unavailable",
                }
            previous_labels = sorted(
                labels & (sm.ACTIVE_LABELS | {sm.LABEL_READY, sm.LABEL_REVIEW_PASSED})
            )
            if sm.LABEL_RUNNING not in labels:
                if not sm.set_labels(issue, sm.LABEL_RUNNING, repo=repo):
                    return {
                        "dispatched": False,
                        "issue": issue,
                        "reason": "claim_label_failed",
                    }
            recheck = sm.get_active_capacity(repo)
            if recheck is None:
                rolled_back = _rollback(
                    issue, dispatch_id, previous_labels, "capacity_recheck_unavailable"
                )
                reason = (
                    "capacity_recheck_unavailable"
                    if rolled_back
                    else "capacity_recheck_unavailable_rollback_failed"
                )
                return {"dispatched": False, "issue": issue, "reason": reason}
            if len(recheck["issues"]) + len(recheck["plans"]) > sm.MAX_ACTIVE:
                rolled_back = _rollback(
                    issue, dispatch_id, previous_labels, "capacity_recheck_exceeded"
                )
                reason = (
                    "capacity_recheck_exceeded"
                    if rolled_back
                    else "capacity_recheck_exceeded_rollback_failed"
                )
                return {"dispatched": False, "issue": issue, "reason": reason}
            if not _record_dispatched(
                issue, dispatch_id, "local-run", {"workflow": "local-run"}
            ):
                return {
                    "dispatched": False,
                    "issue": issue,
                    "reason": "dispatch_state_failed_capacity_retained",
                }
            return {"dispatched": True, "issue": issue, "dispatch_id": dispatch_id}
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
    recheck = sm.get_active_capacity(repo)
    if recheck is None:
        rolled_back = _rollback(issue, dispatch_id, previous, "capacity_recheck_unavailable")
        reason = "capacity_recheck_unavailable" if rolled_back else "capacity_recheck_unavailable_rollback_failed"
        return {"dispatched": False, "issue": issue, "reason": reason}
    if len(recheck["issues"]) + len(recheck["plans"]) > sm.MAX_ACTIVE:
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


def _plan_dispatch_id(packet_id: str, source_main_sha: str, attempt: str) -> str:
    return f"plan-run:{packet_id}:{source_main_sha}:{attempt}"


def _read_live_plan(packet_id: str, repo: str) -> tuple[plan_lane.PlanCandidate | None, int | None, str | None]:
    """Derive a plan candidate and its ledger only from live accepted main."""

    if not plan_lane.PACKET_ID.fullmatch(packet_id):
        return None, None, "plan_packet_id_invalid"
    try:
        adapter = local_loop.GitHubAdapter(repo)
        metadata = adapter.repository_metadata()
        branch = metadata.get("default_branch")
        if not isinstance(branch, str) or not local_loop.BRANCH.fullmatch(branch):
            return None, None, "default_branch_unavailable"
        accepted_main = adapter.accepted_main_sha(branch)
        document = adapter.accepted_plan_document(accepted_main)
        status_document = adapter.accepted_status_document(accepted_main)
        candidate = plan_lane.parse(
            document,
            accepted_main,
            completed_packet_ids=plan_lane.accepted_completed_packet_ids(status_document),
        )
        if candidate.packet_id != packet_id:
            return None, None, "plan_packet_not_current"
        ledger = control_state.read_plan_ledger(repo)
        ledger_number = ledger.get("number") if isinstance(ledger, dict) else None
        if type(ledger_number) is not int or ledger_number <= 0:
            return None, None, "plan_execution_ledger_invalid"
        return candidate, ledger_number, None
    except plan_lane.PlanLaneError as exc:
        return None, None, exc.reason
    except (local_loop.LoopUnavailable, control_state.ControlStateError):
        return None, None, "plan_source_unavailable"


def claim_plan(packet_id: str, attempt_id: str) -> dict[str, object]:
    """Write one exact plan-run claim to the Plan Execution Ledger Issue."""

    attempt = _normalized_attempt_id(attempt_id)
    if attempt is None:
        return {"dispatched": False, "reason": "invalid_attempt_id"}
    repo = _repo()
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"dispatched": False, "reason": "disabled_or_emergency_stopped"}
    if not repo:
        return {"dispatched": False, "reason": "repository_unavailable"}
    candidate, ledger_issue, error = _read_live_plan(packet_id, repo)
    if candidate is None or ledger_issue is None:
        return {"dispatched": False, "reason": error or "plan_source_unavailable"}
    dispatch_id = _plan_dispatch_id(packet_id, candidate.source_main_sha, attempt)
    execution_token = local_loop.plan_execution_token(
        repo, packet_id, candidate.source_main_sha, attempt
    )
    try:
        previous = sm.read_dispatch_state(ledger_issue, dispatch_id, repo)
    except sm.StateUnavailableError:
        return {"dispatched": False, "reason": "dispatch_state_unavailable"}
    if previous and previous.get("status") in {"claimed", "dispatched"}:
        details = previous.get("details")
        if not isinstance(details, dict):
            return {"dispatched": False, "reason": "plan_claim_unverifiable"}
        if details.get("execution_token") != execution_token:
            return {"dispatched": False, "reason": "plan_claim_binding_unverified"}
        if previous.get("status") == "dispatched":
            return {
                "dispatched": True,
                "ledger_issue": ledger_issue,
                "dispatch_id": dispatch_id,
                "reason": "already_dispatched",
            }
        # Resume a claim that was durable-written before label/dispatch
        # promotion completed.  Only the exact attempt/token binding may
        # finish the claim; a mismatched or stale generation fails closed.
        valid, reason = sm.plan_claim_binding_valid(
            ledger_issue, details, packet_id, attempt, execution_token,
            candidate.source_main_sha, candidate.task_spec_sha256,
        )
        if not valid:
            return {"dispatched": False, "reason": reason}
        labels = sm.get_issue_labels_checked(ledger_issue, repo)
        if labels is None:
            return {"dispatched": False, "reason": "plan_ledger_state_unavailable"}
        if sm.LABEL_RUNNING not in labels:
            if not sm.set_labels(ledger_issue, sm.LABEL_RUNNING, repo=repo):
                return {"dispatched": False, "reason": "claim_label_failed"}
        recheck = sm.get_active_capacity(repo)
        if recheck is None or len(recheck["issues"]) + len(recheck["plans"]) > sm.MAX_ACTIVE:
            reason = "capacity_recheck_unavailable" if recheck is None else "capacity_recheck_exceeded"
            return {"dispatched": False, "reason": reason}
        if not sm.record_dispatch_state(
            ledger_issue, dispatch_id, "plan-run", "dispatched",
            {**details, "workflow": "plan-local-run"}, repo,
        ):
            return {"dispatched": False, "reason": "dispatch_state_failed_capacity_retained"}
        return {
            "dispatched": True,
            "ledger_issue": ledger_issue,
            "dispatch_id": dispatch_id,
            "reason": "claimed_resumed",
        }
    capacity = sm.get_active_capacity(repo)
    if capacity is None:
        return {"dispatched": False, "reason": "capacity_state_unavailable"}
    issues = capacity["issues"]
    plans = capacity["plans"]
    if len(issues) + len(plans) >= sm.MAX_ACTIVE:
        return {"dispatched": False, "reason": "capacity_full"}
    ledger_labels = sm.get_issue_labels_checked(ledger_issue, repo)
    if ledger_labels is None:
        return {"dispatched": False, "reason": "plan_ledger_state_unavailable"}
    if ledger_labels & sm.ACTIVE_LABELS:
        return {"dispatched": False, "reason": "plan_ledger_active_state_unavailable"}
    active_scopes = sm.get_active_issue_scopes(issues, repo)
    if active_scopes is None:
        return {"dispatched": False, "reason": "active_scope_state_unavailable"}
    for active_issue, paths in active_scopes.items():
        if artifact_contract.scopes_overlap(candidate.allowed_paths, paths):
            return {"dispatched": False, "reason": f"scope_conflict:{active_issue}"}
    for active_plan in plans:
        if artifact_contract.scopes_overlap(candidate.allowed_paths, active_plan["allowed_paths"]):
            return {"dispatched": False, "reason": f"scope_conflict:{active_plan['subject_id']}"}
    claim_nonce = _new_claim_nonce()
    lease_deadline = (
        datetime.now(timezone.utc) + timedelta(hours=sm.LOCAL_CLAIM_LEASE_HOURS)
    ).isoformat().replace("+00:00", "Z")
    details = {
        "ledger_issue_number": ledger_issue,
        "subject_kind": "plan-packet",
        "subject_id": packet_id,
        "source_main_sha": candidate.source_main_sha,
        "task_spec_sha256": candidate.task_spec_sha256,
        "allowed_paths": list(candidate.allowed_paths),
        "canonical_branch": candidate.branch,
        "attempt_id": attempt,
        "execution_token": execution_token,
        "claim_nonce": claim_nonce,
        "lease_deadline": lease_deadline,
        "previous_labels": sorted(ledger_labels & (sm.ACTIVE_LABELS | sm.TERMINAL_LABELS | {
            sm.LABEL_READY, sm.LABEL_REVIEW_PASSED,
        })),
        "target_label": sm.LABEL_RUNNING,
    }
    if not sm.record_dispatch_state(
        ledger_issue, dispatch_id, "plan-run", "claimed", details, repo
    ):
        return {"dispatched": False, "reason": "claim_state_failed"}
    try:
        persisted = sm.read_dispatch_state(ledger_issue, dispatch_id, repo)
    except sm.StateUnavailableError:
        persisted = None
    persisted_details = persisted.get("details") if isinstance(persisted, dict) else None
    persisted_valid, _ = sm.plan_claim_binding_valid(
        ledger_issue,
        persisted_details,
        packet_id,
        attempt,
        execution_token,
        candidate.source_main_sha,
        candidate.task_spec_sha256,
    )
    if (
        not isinstance(persisted, dict)
        or persisted.get("status") != "claimed"
        or persisted.get("action") != "plan-run"
        or not persisted_valid
    ):
        return {"dispatched": False, "reason": "claim_readback_unverified"}
    if not sm.set_labels(ledger_issue, sm.LABEL_RUNNING, repo=repo):
        sm.record_dispatch_state(
            ledger_issue,
            dispatch_id,
            "plan-run",
            "failed",
            {**details, "reason": "claim_label_failed"},
            repo,
        )
        return {"dispatched": False, "reason": "claim_label_failed"}
    recheck = sm.get_active_capacity(repo)
    if recheck is None or len(recheck["issues"]) + len(recheck["plans"]) > sm.MAX_ACTIVE:
        reason = "capacity_recheck_unavailable" if recheck is None else "capacity_recheck_exceeded"
        restored = sm.set_labels(ledger_issue, *details["previous_labels"], repo=repo)
        recorded = sm.record_dispatch_state(
            ledger_issue,
            dispatch_id,
            "plan-run",
            "failed",
            {**details, "reason": reason},
            repo,
        )
        suffix = "_rollback_failed" if not restored or not recorded else ""
        return {"dispatched": False, "reason": f"{reason}{suffix}"}
    if not sm.record_dispatch_state(
        ledger_issue,
        dispatch_id,
        "plan-run",
        "dispatched",
        {**details, "workflow": "plan-local-run"},
        repo,
    ):
        return {"dispatched": False, "reason": "dispatch_state_failed_capacity_retained"}
    return {
        "dispatched": True,
        "ledger_issue": ledger_issue,
        "dispatch_id": dispatch_id,
        "claim_nonce": claim_nonce,
        "source_main_sha": candidate.source_main_sha,
        "task_spec_sha256": candidate.task_spec_sha256,
    }


def handoff_plan(
    packet_id: str, attempt_id: str, expected_head_sha: str, claim_nonce: str
) -> dict[str, object]:
    """Handoff one exact plan PR through the existing CI/monitor owners."""

    attempt = _normalized_attempt_id(attempt_id)
    if attempt is None:
        return {"handed_off": False, "reason": "invalid_attempt_id"}
    if not isinstance(expected_head_sha, str) or local_loop.HEX40.fullmatch(expected_head_sha) is None:
        return {"handed_off": False, "reason": "invalid_head_sha"}
    if not isinstance(claim_nonce, str) or sm.CLAIM_NONCE_PATTERN.fullmatch(claim_nonce) is None:
        return {"handed_off": False, "reason": "claim_nonce_invalid"}
    repo = _repo()
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"handed_off": False, "reason": "disabled_or_emergency_stopped"}
    candidate, ledger_issue, error = _read_live_plan(packet_id, repo)
    if candidate is None or ledger_issue is None:
        return {"handed_off": False, "reason": error or "plan_source_unavailable"}
    dispatch_id = _plan_dispatch_id(packet_id, candidate.source_main_sha, attempt)
    try:
        claim = sm.read_dispatch_state(ledger_issue, dispatch_id, repo)
    except sm.StateUnavailableError:
        claim = None
    if not isinstance(claim, dict):
        return {"handed_off": False, "reason": "plan_claim_not_found"}
    if claim.get("status") != "dispatched" or claim.get("action") != "plan-run":
        return {"handed_off": False, "reason": "plan_claim_state_unexpected"}
    details = claim.get("details")
    token = local_loop.plan_execution_token(repo, packet_id, candidate.source_main_sha, attempt)
    valid, reason = sm.plan_claim_binding_valid(
        ledger_issue, details, packet_id, attempt, token,
        candidate.source_main_sha, candidate.task_spec_sha256,
    )
    if not valid:
        return {"handed_off": False, "reason": reason}
    if details.get("claim_nonce") != claim_nonce:
        return {"handed_off": False, "reason": "claim_nonce_mismatch"}
    labels = sm.get_issue_labels_checked(ledger_issue, repo)
    if labels is None:
        return {"handed_off": False, "reason": "plan_ledger_state_unavailable"}
    if sm.LABEL_RUNNING not in labels:
        return {"handed_off": False, "reason": "plan_ledger_not_running"}
    try:
        pr = pr_binding.find_plan_pr(
            packet_id, candidate.branch, expected_head_sha, candidate.source_main_sha,
            candidate.task_spec_sha256, repo,
        )
    except pr_binding.PRBindingError as exc:
        return {"handed_off": False, "reason": f"pr_binding_rejected:{exc}"}
    pr_number = pr.get("number")
    if type(pr_number) is not int:
        return {"handed_off": False, "reason": "pr_number_invalid"}
    extra = {
        "subject_kind": "plan-packet",
        "subject_id": packet_id,
        "source_main_sha": candidate.source_main_sha,
        "task_spec_sha256": candidate.task_spec_sha256,
        "branch": candidate.branch,
        "attempt_id": attempt,
        "dispatch_id": dispatch_id,
        "claim_nonce": claim_nonce,
    }
    if not sm.record_worker_state(ledger_issue, pr_number, expected_head_sha, "plan-run", extra, repo):
        return {"handed_off": False, "reason": "worker_state_failed"}
    run_id = _acquire_local_ci(ledger_issue, pr_number, candidate.branch, expected_head_sha, repo)
    if run_id is None:
        return {"handed_off": False, "reason": "ci_acquisition_failed"}
    monitor_ok, monitor_reason = _dispatch_local_monitor(
        ledger_issue, pr_number, expected_head_sha, run_id, repo
    )
    if not monitor_ok:
        return {"handed_off": False, "reason": monitor_reason}
    return {
        "handed_off": True,
        "subject_kind": "plan-packet",
        "subject_id": packet_id,
        "ledger_issue": ledger_issue,
        "pr_number": pr_number,
        "head_sha": expected_head_sha,
        "ci_run_id": run_id,
        "dispatch_id": dispatch_id,
    }


def _verified_plan_pr(
    pr_number: int, head_sha: str, packet_id: str, repo: str
) -> dict[str, str] | None:
    """Verify one plan PR binding by number against authoritative GitHub state.

    Unlike the open-PR list in ``pr_binding.find_plan_pr``, this read also
    works after the maintainer merge: the PR must target ``main``, its head
    must equal the exact expected plan head, and its binding marker must
    carry the plan subject identity.  Any other state returns ``False``.
    """

    try:
        value = pr_binding._gh_json(
            "pr", "view", str(pr_number), "--repo", repo,
            "--json", "state,headRefOid,baseRefName,baseRefOid,body",
        )
    except pr_binding.PRBindingError:
        return None
    if not isinstance(value, dict):
        return None
    live_head = value.get("headRefOid")
    live_base = value.get("baseRefOid")
    if (
        not isinstance(live_head, str)
        or local_loop.HEX40.fullmatch(live_head) is None
        or not isinstance(live_base, str)
        or local_loop.HEX40.fullmatch(live_base) is None
        or live_head != head_sha
        or value.get("baseRefName") != "main"
    ):
        return None
    marker = sm.parse_binding_marker(str(value.get("body", "")))
    if not (
        isinstance(marker, dict)
        and marker.get("subject_kind") == "plan-packet"
        and marker.get("subject_id") == packet_id
    ):
        return None
    return {
        "base_sha": live_base,
        "head_sha": live_head,
        "reviewed_range": f"{live_base}...{live_head}",
    }


def _authoritative_plan_merge(pr_number: int, head_sha: str, repo: str) -> str | None:
    """Return the merge commit SHA only for a provably merged exact plan head.

    Reads authoritative GitHub PR state by number: the PR must be merged, its
    head must equal the exact expected plan head, and the merge commit SHA
    must be well formed.  Any other state (not merged, wrong head, unreadable
    or malformed evidence) returns ``None``.
    """

    try:
        value = pr_binding._gh_json(
            "pr", "view", str(pr_number), "--repo", repo,
            "--json", "state,merged,headRefOid,mergeCommit",
        )
    except pr_binding.PRBindingError:
        return None
    if not isinstance(value, dict):
        return None
    if str(value.get("state", "")).upper() != "MERGED" or value.get("merged") is not True:
        return None
    if value.get("headRefOid") != head_sha:
        return None
    merge_commit = value.get("mergeCommit")
    if not isinstance(merge_commit, dict):
        return None
    oid = merge_commit.get("oid")
    if not isinstance(oid, str) or local_loop.HEX40.fullmatch(oid) is None:
        return None
    return oid


_RECEIPT_MARKER = "EXACT-HEAD REVIEW RECEIPT"


def _receipt_field(body: str, label: str) -> str | None:
    matches = re.findall(rf"(?im)^\s*{re.escape(label)}\s*:\s*(.*?)\s*$", body)
    return matches[0].strip() if len(matches) == 1 else None


def _authoritative_plan_ci(
    pr_number: int, head_sha: str, worker: dict[str, object], repo: str
) -> dict[str, object] | None:
    """Return verified exact-head canonical CI evidence, or None.

    Finds the newest supported exact-head ``tests`` run for the bound plan
    branch, then re-proves it through the existing CI verifier.  Any
    missing branch, unreadable run, or failed exact-head proof returns
    ``None``.
    """

    extra = worker.get("extra")
    branch = extra.get("branch") if isinstance(extra, dict) else None
    if not isinstance(branch, str) or not branch:
        return None
    try:
        selected = ci_verifier.select_canonical_run(
            ci_verifier.find_exact_runs(branch, head_sha, pr_number)
        )
    except (ci_verifier.CIVerificationError, ValueError, TypeError):
        return None
    if not isinstance(selected, dict):
        return None
    run_id = selected.get("databaseId")
    pr = sm.get_pr_info(pr_number, repo)
    if not isinstance(pr, dict):
        return None
    try:
        evidence = ci_verifier.verify_exact_head_ci(pr_number, head_sha, run_id, pr)
    except ci_verifier.CIVerificationError:
        return None
    workflow_run_id = evidence.get("workflow_run_id")
    if type(workflow_run_id) is not int:
        try:
            workflow_run_id = int(workflow_run_id)
        except (TypeError, ValueError):
            return None
    required = evidence.get("required_jobs")
    successful = evidence.get("successful_jobs")
    workflow_name = evidence.get("workflow_name")
    if (
        workflow_run_id <= 0
        or not isinstance(required, list)
        or not isinstance(successful, list)
        or not isinstance(workflow_name, str)
        or not workflow_name
    ):
        return None
    return {
        "workflow_run_id": workflow_run_id,
        "workflow_name": workflow_name,
        "required_jobs": required,
        "successful_jobs": successful,
    }


def _authoritative_plan_review(
    pr_number: int, head_sha: str, repo: str, expected_base_sha: str = ""
) -> dict[str, str] | None:
    """Return a live exact-head PASS review receipt, or None.

    Requires one playbook ``EXACT-HEAD REVIEW RECEIPT`` on the PR whose
    reviewed SHA, complete ``base...head`` range, and PASS outcome match
    the live binding, and whose reviewer session differs from the
    implementation session.  Absence, conflict, or a non-PASS outcome
    returns ``None``.
    """

    ok, _reason, binding = sm.resolve_live_review_binding(
        pr_number, head_sha, repo, expected_base_sha
    )
    if not ok or not isinstance(binding, dict):
        return None
    reviewed_range = binding.get("reviewed_range")
    base_sha = binding.get("base_sha")
    if (
        not isinstance(reviewed_range, str)
        or not isinstance(base_sha, str)
        or reviewed_range != f"{base_sha}...{head_sha}"
    ):
        return None
    try:
        comments = sm.get_issue_comments(pr_number, repo)
    except sm.StateUnavailableError:
        return None
    if not isinstance(comments, list):
        return None
    matches: list[dict[str, str]] = []
    for comment in comments:
        body = comment.get("body") if isinstance(comment, dict) else None
        if not isinstance(body, str) or body.count(_RECEIPT_MARKER) != 1:
            continue
        reviewed_sha = _receipt_field(body, "Reviewed SHA")
        comment_range = _receipt_field(body, "Reviewed range")
        outcome = (_receipt_field(body, "Outcome") or "").upper()
        unresolved = (_receipt_field(body, "Unresolved objections") or "").lower()
        reviewer = _receipt_field(body, "Reviewer session identity")
        implementation = _receipt_field(body, "Implementation session identity")
        if (
            reviewed_sha == head_sha
            and comment_range == reviewed_range
            and outcome == "PASS"
            and unresolved in {"none", "no"}
            and isinstance(reviewer, str)
            and reviewer
            and isinstance(implementation, str)
            and implementation
            and reviewer != implementation
        ):
            matches.append({
                "base_sha": base_sha,
                "reviewed_range": reviewed_range,
                "summary": "exact-head PASS review receipt verified from live PR",
            })
    if not matches:
        return None
    return matches[0]


def record_plan_lifecycle(packet_id: str, attempt_id: str, stage: str) -> dict[str, object]:
    """Record one controller-owned plan lifecycle transition on the ledger.

    The ``ci`` and ``review`` stages verify authoritative GitHub CI/review
    state for the exact plan head and then record the receipt through the
    existing CI-state and review-state owners — the same write path the
    ``merge`` stage already uses after verifying the maintainer merge.
    The ``closeout`` stage requires the CI, review, and merge receipts on
    the ledger and then records the canonical closeout receipt plus
    terminal packet state through the existing dispatch-state owner.
    Every stage validates the exact subject binding (packet id, attempt,
    dispatch id, PR number, exact head SHA) against the durable ledger
    claim; no model self-report advances state, and every write is
    idempotent.
    """

    attempt = _normalized_attempt_id(attempt_id)
    if attempt is None:
        return {"recorded": False, "stage": stage, "reason": "invalid_attempt_id"}
    if not plan_lane.PACKET_ID.fullmatch(packet_id):
        return {"recorded": False, "stage": stage, "reason": "plan_packet_id_invalid"}
    if stage not in PLAN_LIFECYCLE_STAGES:
        return {"recorded": False, "stage": stage, "reason": "stage_invalid"}
    repo = _repo()
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"recorded": False, "stage": stage, "reason": "disabled_or_emergency_stopped"}
    if not repo:
        return {"recorded": False, "stage": stage, "reason": "repository_unavailable"}
    candidate, ledger_issue, error = _read_live_plan(packet_id, repo)
    if candidate is None or ledger_issue is None:
        return {"recorded": False, "stage": stage, "reason": error or "plan_source_unavailable"}
    dispatch_id = _plan_dispatch_id(packet_id, candidate.source_main_sha, attempt)
    claim = plan_lifecycle._plan_claim(ledger_issue, dispatch_id, repo)
    if claim is None:
        return {"recorded": False, "stage": stage, "reason": "plan_claim_not_found"}
    if claim.get("status") == "closed_out":
        return {"recorded": True, "stage": stage, "reason": "already_closed_out"}
    if claim.get("status") != "dispatched":
        return {"recorded": False, "stage": stage, "reason": "plan_claim_state_unexpected"}
    details = claim.get("details")
    token = local_loop.plan_execution_token(repo, packet_id, candidate.source_main_sha, attempt)
    valid, reason = sm.plan_claim_binding_valid(
        ledger_issue, details, packet_id, attempt, token,
        candidate.source_main_sha, candidate.task_spec_sha256,
    )
    if not valid:
        return {"recorded": False, "stage": stage, "reason": reason}
    if not isinstance(details, dict):
        return {"recorded": False, "stage": stage, "reason": "plan_claim_details_invalid"}
    try:
        worker = sm.read_worker_state(ledger_issue, repo)
    except sm.StateUnavailableError:
        worker = None
    if not isinstance(worker, dict) or worker.get("worker_type") != "plan-run":
        return {"recorded": False, "stage": stage, "reason": "worker_state_unavailable"}
    pr_number = worker.get("pr_number")
    head_sha = worker.get("head_sha")
    if (
        type(pr_number) is not int or pr_number <= 0
        or not isinstance(head_sha, str) or local_loop.HEX40.fullmatch(head_sha) is None
    ):
        return {"recorded": False, "stage": stage, "reason": "worker_binding_invalid"}
    plan_binding = _verified_plan_pr(pr_number, head_sha, packet_id, repo)
    if not plan_binding:
        return {"recorded": False, "stage": stage, "reason": "plan_pr_binding_unverified"}
    if stage == "ci":
        receipt = plan_lifecycle.plan_ci_receipt(ledger_issue, pr_number, head_sha, repo)
        if receipt is not None:
            return {
                "recorded": True, "stage": stage, "reason": "verified",
                "ci_run_id": receipt.get("workflow_run_id"),
            }
        evidence = _authoritative_plan_ci(pr_number, head_sha, worker, repo)
        if evidence is None:
            return {"recorded": False, "stage": stage, "reason": "ci_evidence_unavailable"}
        outcome = plan_lifecycle.record_plan_ci_receipt(
            ledger_issue, packet_id, attempt, candidate.source_main_sha,
            pr_number, head_sha, int(evidence["workflow_run_id"]), repo,
            workflow_name=str(evidence["workflow_name"]),
            required_jobs=list(evidence["required_jobs"]),
            successful_jobs=list(evidence["successful_jobs"]),
        )
        return {
            **outcome, "stage": stage,
            "ci_run_id": evidence["workflow_run_id"],
        }
    if stage == "review":
        # Production _verified_plan_pr returns the controller-derived full
        # base/head range.  A bool True is retained only for legacy isolated
        # test doubles and cannot occur from the production verifier.
        expected_base = plan_binding["base_sha"] if isinstance(plan_binding, dict) else ""
        if isinstance(plan_binding, dict):
            receipt = plan_lifecycle.plan_review_receipt(
                ledger_issue, pr_number, head_sha, repo, expected_base
            )
        else:
            receipt = plan_lifecycle.plan_review_receipt(
                ledger_issue, pr_number, head_sha
            )
        if receipt is not None:
            return {
                "recorded": True, "stage": stage, "reason": "verified",
                "review_workflow_run_id": receipt.get("review_workflow_run_id"),
            }
        evidence = _authoritative_plan_review(
            pr_number, head_sha, repo, expected_base
        )
        if evidence is None:
            return {"recorded": False, "stage": stage, "reason": "review_evidence_unavailable"}
        outcome = plan_lifecycle.record_plan_review_receipt(
            ledger_issue, packet_id, attempt, candidate.source_main_sha,
            pr_number, head_sha, evidence["base_sha"], evidence["reviewed_range"],
            repo, evidence["summary"],
        )
        return {**outcome, "stage": stage}
    if stage == "merge":
        merge_commit_sha = _authoritative_plan_merge(pr_number, head_sha, repo)
        if merge_commit_sha is None:
            return {"recorded": False, "stage": stage, "reason": "merge_evidence_unavailable"}
        outcome = plan_lifecycle.record_plan_merge_receipt(
            ledger_issue, packet_id, attempt, candidate.source_main_sha,
            pr_number, head_sha, merge_commit_sha, repo,
        )
        return {**outcome, "stage": stage, "merge_commit_sha": merge_commit_sha}
    ci_receipt = plan_lifecycle.plan_ci_receipt(ledger_issue, pr_number, head_sha, repo)
    merge_receipt = plan_lifecycle.plan_merge_receipt(ledger_issue, pr_number, head_sha, repo)
    closeout_reference = plan_lifecycle.canonical_closeout_reference(
        pr_number,
        head_sha,
        merge_receipt.get("merge_commit_sha") if merge_receipt is not None else None,
        ci_receipt.get("workflow_run_id") if ci_receipt is not None else None,
    )
    if closeout_reference is None:
        return {"recorded": False, "stage": stage, "reason": "closeout_receipt_pending"}
    outcome = plan_lifecycle.record_plan_closeout_receipt(
        ledger_issue, packet_id, attempt, candidate.source_main_sha,
        pr_number, head_sha, "closed_out", closeout_reference, repo,
    )
    return {**outcome, "stage": stage}


def _live_routing_document(repo: str) -> tuple[str, str, str] | None:
    """Return accepted main plus plan/status documents from authoritative routing."""

    try:
        adapter = local_loop.GitHubAdapter(repo)
        metadata = adapter.repository_metadata()
        branch = metadata.get("default_branch")
        if not isinstance(branch, str) or local_loop.BRANCH.fullmatch(branch) is None:
            return None
        accepted_main = adapter.accepted_main_sha(branch)
        if local_loop.HEX40.fullmatch(accepted_main) is None:
            return None
        document = adapter.accepted_plan_document(accepted_main)
        status_document = adapter.accepted_status_document(accepted_main)
    except local_loop.LoopUnavailable:
        return None
    return accepted_main, document, status_document


def promote_plan(packet_id: str, attempt_id: str) -> dict[str, object]:
    """Record exactly-one successor-promotion or bounded escalation receipt.

    After an accepted plan closeout, reads the live accepted routing on the
    current accepted main and records either the bounded promotion receipt
    (successor packet id + capsule digest) or a bounded escalation pause.
    This controller transport never reconstructs current ownership from
    FUTURE_ROUTE prose: when no already-accepted successor exists, the
    evidence-backed route planner owns candidate generation and this command
    records only its eventual accepted binding.  The successor is never
    executed here; every write is idempotent, conflicts fail closed, and no
    model self-report advances routing.
    """

    attempt = _normalized_attempt_id(attempt_id)
    if attempt is None:
        return {"promoted": False, "reason": "invalid_attempt_id"}
    if not plan_lane.PACKET_ID.fullmatch(packet_id):
        return {"promoted": False, "reason": "plan_packet_id_invalid"}
    repo = _repo()
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"promoted": False, "reason": "disabled_or_emergency_stopped"}
    if not repo:
        return {"promoted": False, "reason": "repository_unavailable"}
    try:
        ledger = control_state.read_plan_ledger(repo)
    except control_state.ControlStateError:
        return {"promoted": False, "reason": "plan_ledger_unavailable"}
    ledger_issue = ledger.get("number")
    if type(ledger_issue) is not int or ledger_issue <= 0:
        return {"promoted": False, "reason": "plan_execution_ledger_invalid"}
    claim = plan_lifecycle._exact_plan_claim(ledger_issue, packet_id, attempt, repo)
    if claim is None:
        return {"promoted": False, "reason": "plan_claim_not_found"}
    if claim.get("status") != "closed_out":
        return {"promoted": False, "reason": "plan_claim_not_closed_out"}
    details = claim.get("details")
    if not isinstance(details, dict) or not isinstance(details.get("source_main_sha"), str) or local_loop.HEX40.fullmatch(details["source_main_sha"]) is None:
        return {"promoted": False, "reason": "plan_claim_binding_invalid"}
    source_main_sha = details["source_main_sha"]
    routing = _live_routing_document(repo)
    if routing is None:
        return {"promoted": False, "reason": "routing_unavailable"}
    accepted_main, document, status_document = routing
    compiled: dict[str, object] | None = None
    try:
        successor_id, capsule_digest = plan_lane.successor_binding(
            document,
            packet_id,
            accepted_main,
            completed_packet_ids=plan_lane.accepted_completed_packet_ids(status_document),
        )
    except plan_lane.PlanLaneError as exc:
        if exc.reason not in {"plan_packet_absent", "successor_still_current", "multiple_plan_packets"}:
            return {"promoted": False, "reason": f"routing_invalid:{exc.reason}"}
        compiled = _compile_promotion(repo, packet_id, attempt)
        if compiled is None:
            return {"promoted": False, "reason": "route_compile_unavailable"}
        if "escalated" in compiled:
            return compiled
        successor_id = compiled["successor_id"]
        capsule_digest = compiled["capsule_digest"]
    receipt_id = f"plan-promote:{packet_id}:{attempt}"
    receipt_details = {
        "subject_kind": "plan-packet",
        "subject_id": packet_id,
        "attempt_id": attempt,
        "source_main_sha": source_main_sha,
        "routing_main_sha": accepted_main,
        "successor_id": successor_id,
        "capsule_digest": capsule_digest,
    }
    if compiled is not None:
        receipt_details["compiled"] = True
        if isinstance(compiled.get("manifest_sha256"), str):
            receipt_details["manifest_sha256"] = compiled["manifest_sha256"]
    try:
        previous = sm.read_dispatch_state(ledger_issue, receipt_id, repo)
    except sm.StateUnavailableError:
        return {"promoted": False, "reason": "promotion_receipt_unavailable"}
    if isinstance(previous, dict) and previous.get("status") == "promoted":
        if previous.get("details") == receipt_details:
            return {"promoted": True, "reason": "already_promoted", **receipt_details}
        return {"promoted": False, "reason": "conflicting_promotion_receipt"}
    if not sm.record_dispatch_state(
        ledger_issue, receipt_id, "plan-promote", "promoted", receipt_details, repo
    ):
        return {"promoted": False, "reason": "promotion_receipt_write_failed"}
    return {"promoted": True, "reason": "promoted", **receipt_details}


def record_route_t3_receipt(
    packet_id: str,
    accepted_main_sha: str,
    candidate_digest: str,
    action_digest: str,
    scope_digest: str,
    authority_receipt_digest: str,
    outcome_receipt_digest: str,
    authority_owner_digest: str,
    operator: str,
    decision_source: str,
    decision_evidence_digest: str,
    issued_at: str,
    expires_at: str,
    disposition: str,
) -> dict[str, object]:
    """Record a finite source-authoritative T3 decision on the existing ledger.

    This command is intentionally absent from every worker-facing route path.
    The GitHub workflow dispatch records the finite decision handoff under the
    existing Plan Execution Ledger. Its transport identity is derived from the
    authenticated Actions actor; the declared source is one of the accepted
    human/local-Sol/GPT-web sources, and the authority owner is bound by the
    accepted current-main T3 request rather than supplied by the caller. The
    established product effect owner remains outside this controller and
    supplies only its redacted outcome digest here; the routed CLOSEOUT packet
    independently validates that owner-held evidence. This command does not
    issue product authority or invoke an effect; a model source can decide
    only the finite disposition already accepted by the route.
    """

    import route_driver

    repo = _repo()
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"authorized": False, "reason": "disabled_or_emergency_stopped"}
    if not repo or plan_lane.PACKET_ID.fullmatch(packet_id) is None:
        return {"authorized": False, "reason": "route_t3_identity_invalid"}
    if local_loop.HEX40.fullmatch(accepted_main_sha) is None:
        return {"authorized": False, "reason": "route_t3_identity_invalid"}
    authenticated_actor = os.environ.get("GITHUB_ACTOR")
    if (
        os.environ.get("GITHUB_ACTIONS") != "true"
        or not isinstance(authenticated_actor, str)
        or authenticated_actor != operator
        or _SAFE_GITHUB_OPERATOR.fullmatch(authenticated_actor) is None
        or authenticated_actor.endswith("[bot]")
    ):
        return {"authorized": False, "reason": "route_t3_operator_unproved"}
    # A receipt is meaningful only for the one currently accepted typed T3
    # pause.  Never let a workflow input create an orphan authority binding or
    # substitute hashes for the request that the accepted route actually
    # prepared.
    try:
        adapter = local_loop.GitHubAdapter(repo)
        metadata = adapter.repository_metadata()
        branch = metadata.get("default_branch")
        if not isinstance(branch, str) or not local_loop.BRANCH.fullmatch(branch):
            return {"authorized": False, "reason": "route_t3_request_unavailable"}
        current_main = adapter.accepted_main_sha(branch)
        request = route_driver.current_t3_request(
            adapter.accepted_plan_document(current_main), current_main
        )
    except (local_loop.LoopUnavailable, route_driver.RouteDriverError):
        return {"authorized": False, "reason": "route_t3_request_unavailable"}
    if request is None:
        return {"authorized": False, "reason": "route_t3_request_not_current"}
    if (
        request.packet_id != packet_id
        or request.accepted_main_sha != accepted_main_sha
        or request.candidate_digest != candidate_digest
        or request.action_digest != action_digest
        or request.scope_digest != scope_digest
        or request.authority_owner_digest != authority_owner_digest
    ):
        return {"authorized": False, "reason": "route_t3_request_binding_mismatch"}
    decision_digest = route_driver.t3_decision_digest(
        request,
        decision_source,
        decision_evidence_digest,
        disposition,
    )
    receipt = {
        "schema_version": "route_t3_receipt.v1",
        "packet_id": packet_id,
        "accepted_main_sha": accepted_main_sha,
        "candidate_digest": candidate_digest,
        "action_digest": action_digest,
        "scope_digest": scope_digest,
        "authority_receipt_digest": authority_receipt_digest,
        "outcome_receipt_digest": outcome_receipt_digest,
        "authority_owner_digest": authority_owner_digest,
        "operator": operator,
        "decision_source": decision_source,
        "decision_evidence_digest": decision_evidence_digest,
        "decision_digest": decision_digest,
        "issued_at": issued_at,
        "expires_at": expires_at,
        "disposition": disposition,
    }
    parsed, reason = route_driver.validate_t3_receipt(receipt, request)
    if parsed is None:
        return {"authorized": False, "reason": reason}
    try:
        ledger = control_state.read_plan_ledger(repo)
    except control_state.ControlStateError:
        return {"authorized": False, "reason": "plan_ledger_unavailable"}
    ledger_issue = ledger.get("number") if isinstance(ledger, dict) else None
    if type(ledger_issue) is not int or ledger_issue <= 0:
        return {"authorized": False, "reason": "plan_execution_ledger_invalid"}
    dispatch_id = f"route-t3:{packet_id}:{candidate_digest}"
    try:
        existing = sm.read_dispatch_state(ledger_issue, dispatch_id, repo)
    except sm.StateUnavailableError:
        return {"authorized": False, "reason": "t3_receipt_state_unavailable"}
    if isinstance(existing, dict):
        if (
            existing.get("action") == "route-t3-receipt"
            and existing.get("status") == "authorized"
            and existing.get("details") == receipt
        ):
            return {"authorized": True, "reason": "already_recorded", **receipt}
        return {"authorized": False, "reason": "conflicting_t3_receipt"}
    if not sm.record_dispatch_state(
        ledger_issue, dispatch_id, "route-t3-receipt", "authorized", receipt, repo
    ):
        return {"authorized": False, "reason": "t3_receipt_write_failed"}
    return {"authorized": True, "reason": "recorded", **receipt}


def record_route_owner_outcome(
    packet_id: str,
    accepted_main_sha: str,
    candidate_digest: str,
    outcome_receipt_digest: str,
    owner_evidence_digest: str,
) -> dict[str, object]:
    """Record an existing product owner's independent outcome evidence.

    This is a separate authenticated transport and ledger action from the T3
    decision receipt.  It only records a redacted evidence binding; it never
    executes an effect or supplies authority to one.
    """

    import route_driver

    repo = _repo()
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"recorded": False, "reason": "disabled_or_emergency_stopped"}
    actor = os.environ.get("GITHUB_ACTOR")
    if (
        os.environ.get("GITHUB_ACTIONS") != "true"
        or not isinstance(actor, str)
        or _SAFE_GITHUB_OPERATOR.fullmatch(actor) is None
        or actor.endswith("[bot]")
        or not repo
        or plan_lane.PACKET_ID.fullmatch(packet_id) is None
        or local_loop.HEX40.fullmatch(accepted_main_sha) is None
        or plan_lane.SHA256.fullmatch(candidate_digest) is None
        or plan_lane.SHA256.fullmatch(outcome_receipt_digest) is None
        or plan_lane.SHA256.fullmatch(owner_evidence_digest) is None
    ):
        return {"recorded": False, "reason": "route_owner_outcome_identity_invalid"}
    try:
        adapter = local_loop.GitHubAdapter(repo)
        metadata = adapter.repository_metadata()
        branch = metadata.get("default_branch")
        if not isinstance(branch, str) or not local_loop.BRANCH.fullmatch(branch):
            return {"recorded": False, "reason": "route_t3_request_unavailable"}
        repository_owner = metadata.get("owner")
        if (
            not isinstance(repository_owner, str)
            or _SAFE_GITHUB_OPERATOR.fullmatch(repository_owner) is None
            or actor.casefold() != repository_owner.casefold()
        ):
            return {"recorded": False, "reason": "route_owner_outcome_owner_unproved"}
        current_main = adapter.accepted_main_sha(branch)
        request = route_driver.current_t3_request(
            adapter.accepted_plan_document(current_main), current_main
        )
    except (local_loop.LoopUnavailable, route_driver.RouteDriverError):
        return {"recorded": False, "reason": "route_t3_request_unavailable"}
    if request is None or (
        request.packet_id != packet_id
        or request.accepted_main_sha != accepted_main_sha
        or request.candidate_digest != candidate_digest
    ):
        return {"recorded": False, "reason": "route_owner_outcome_binding_mismatch"}
    try:
        ledger = control_state.read_plan_ledger(repo)
    except control_state.ControlStateError:
        return {"recorded": False, "reason": "plan_ledger_unavailable"}
    ledger_issue = ledger.get("number") if isinstance(ledger, dict) else None
    if type(ledger_issue) is not int or ledger_issue <= 0:
        return {"recorded": False, "reason": "plan_execution_ledger_invalid"}
    t3_id = f"route-t3:{packet_id}:{candidate_digest}"
    owner_id = f"route-t3-owner-outcome:{packet_id}:{candidate_digest}"
    try:
        t3_state = sm.read_dispatch_state(ledger_issue, t3_id, repo)
        existing = sm.read_dispatch_state(ledger_issue, owner_id, repo)
    except sm.StateUnavailableError:
        return {"recorded": False, "reason": "route_owner_outcome_state_unavailable"}
    t3_details = t3_state.get("details") if isinstance(t3_state, dict) else None
    if (
        not isinstance(t3_state, dict)
        or t3_state.get("action") != "route-t3-receipt"
        or t3_state.get("status") != "authorized"
        or not isinstance(t3_details, dict)
        or t3_details.get("outcome_receipt_digest") != outcome_receipt_digest
        or t3_details.get("operator") == actor
    ):
        return {"recorded": False, "reason": "route_owner_outcome_t3_binding_unproved"}
    details = {
        "schema_version": "route_t3_owner_outcome.v1",
        "packet_id": packet_id,
        "accepted_main_sha": accepted_main_sha,
        "candidate_digest": candidate_digest,
        "outcome_receipt_digest": outcome_receipt_digest,
        "owner_actor": actor,
        "owner_evidence_digest": owner_evidence_digest,
        "owner_receipt_digest": route_driver.owner_outcome_receipt_digest(
            packet_id, accepted_main_sha, candidate_digest,
            outcome_receipt_digest, actor, owner_evidence_digest,
        ),
    }
    if isinstance(existing, dict):
        if existing.get("action") == "route-t3-owner-outcome" and existing.get("status") == "validated" and existing.get("details") == details:
            return {"recorded": True, "reason": "already_recorded", **details}
        return {"recorded": False, "reason": "conflicting_owner_outcome_receipt"}
    if not sm.record_dispatch_state(ledger_issue, owner_id, "route-t3-owner-outcome", "validated", details, repo):
        return {"recorded": False, "reason": "owner_outcome_receipt_write_failed"}
    return {"recorded": True, "reason": "recorded", **details}


def _record_plan_escalation(
    ledger_issue: int, packet_id: str, attempt: str, reason: str, repo: str
) -> dict[str, object]:
    """Record a bounded escalation pause receipt for the planning owner."""

    receipt_id = f"plan-escalate:{packet_id}:{attempt}"
    receipt_details = {
        "subject_kind": "plan-packet",
        "subject_id": packet_id,
        "attempt_id": attempt,
        "reason": reason,
        "pause_owner": "planning",
    }
    try:
        previous = sm.read_dispatch_state(ledger_issue, receipt_id, repo)
    except sm.StateUnavailableError:
        return {"promoted": False, "reason": "escalation_receipt_unavailable"}
    if isinstance(previous, dict) and previous.get("status") == "escalated":
        if previous.get("details") == receipt_details:
            return {"promoted": False, "escalated": True, **receipt_details}
        return {"promoted": False, "reason": "conflicting_escalation_receipt"}
    if not sm.record_dispatch_state(
        ledger_issue, receipt_id, "plan-escalate", "escalated", receipt_details, repo
    ):
        return {"promoted": False, "reason": "escalation_receipt_write_failed"}
    return {"promoted": False, "escalated": True, **receipt_details}


def _compile_promotion(
    repo: str,
    packet_id: str,
    attempt: str,
) -> dict[str, object] | None:
    """Record a typed pause until the evidence-backed planner supplies a candidate.

    The controller owns receipt transport, not promotion planning.  In
    particular it must not reconstruct current ownership from FUTURE_ROUTE
    prose, so an absent successor remains a durable ``DECISION_REQUIRED``
    escalation until a separately validated current-main candidate exists.
    """

    try:
        ledger = control_state.read_plan_ledger(repo)
    except control_state.ControlStateError:
        return None
    ledger_issue = ledger.get("number")
    if type(ledger_issue) is not int or ledger_issue <= 0:
        return None

    def escalate(reason: str) -> dict[str, object]:
        return _record_plan_escalation(ledger_issue, packet_id, attempt, reason, repo)

    return escalate("promotion_current_main_evidence_missing")


def _terminal_plan(
    packet_id: str,
    attempt_id: str,
    source_main_sha: str,
    reason_code: str,
    claim_nonce: str,
    *,
    unknown: bool,
) -> dict[str, object]:
    attempt = _normalized_attempt_id(attempt_id)
    if attempt is None:
        return {"released": False, "blocked": False, "reason": "invalid_attempt_id"}
    if not plan_lane.PACKET_ID.fullmatch(packet_id) or not local_loop.HEX40.fullmatch(source_main_sha):
        return {"released": False, "blocked": False, "reason": "plan_identity_invalid"}
    if not sm.CLAIM_NONCE_PATTERN.fullmatch(claim_nonce):
        return {"released": False, "blocked": False, "reason": "claim_nonce_invalid"}
    repo = _repo()
    try:
        control_state.require_live(repo or None)
        ledger = control_state.read_plan_ledger(repo)
    except control_state.ControlStateError:
        return {"released": False, "blocked": False, "reason": "plan_ledger_unavailable"}
    ledger_issue = ledger["number"]
    dispatch_id = _plan_dispatch_id(packet_id, source_main_sha, attempt)
    try:
        claim = sm.read_dispatch_state(ledger_issue, dispatch_id, repo)
    except sm.StateUnavailableError:
        claim = None
    if not isinstance(claim, dict):
        return {"released": False, "blocked": False, "reason": "claim_not_found"}
    if (
        claim.get("issue_number") != ledger_issue
        or claim.get("dispatch_id") != dispatch_id
        or claim.get("action") != "plan-run"
    ):
        return {"released": False, "blocked": False, "reason": "plan_claim_state_unexpected"}
    status = claim.get("status")
    if status not in {"claimed", "dispatched", "failed", "failed_unknown_output"}:
        return {"released": False, "blocked": False, "reason": "plan_claim_state_unexpected"}
    details = claim.get("details")
    token = local_loop.plan_execution_token(repo, packet_id, source_main_sha, attempt)
    valid, reason = sm.plan_claim_binding_valid(
        ledger_issue, details, packet_id, attempt, token, source_main_sha,
        details.get("task_spec_sha256") if isinstance(details, dict) else "",
        require_lease_live=status != "failed_unknown_output",
    )
    if not valid or not isinstance(details, dict) or details.get("claim_nonce") != claim_nonce:
        return {"released": False, "blocked": False, "reason": reason or "claim_nonce_mismatch"}
    outcome, payload = sm.release_local_claim_outcome(
        ledger_issue, dispatch_id, claim_nonce, repo
    )
    if outcome not in {"own-active", "own-terminal"}:
        return {"released": False, "blocked": False, "reason": "superseded" if outcome == "superseded" else str(payload)}
    terminal_status = "failed_unknown_output" if unknown else "failed"
    if unknown and status == "failed":
        return {"released": False, "blocked": False, "reason": "conflicting_terminal_state"}
    if not unknown and status == "failed_unknown_output":
        return {"released": False, "blocked": False, "reason": "conflicting_terminal_state"}
    if status != terminal_status:
        terminal_details = {**details, "reason": reason_code}
        if not sm.record_dispatch_state(
            ledger_issue, dispatch_id, "plan-run", terminal_status, terminal_details, repo
        ):
            return {"released": False, "blocked": False, "reason": "claim_state_failed_write"}
    released, release_reason = sm.release_failed_capacity(
        ledger_issue, sm.LABEL_RUNNING, sm.LABEL_BLOCKED, repo=repo
    )
    if not released:
        return {"released": False, "blocked": False, "reason": f"capacity_release_failed:{release_reason}"}
    return {
        "released": not unknown,
        "blocked": unknown,
        "ledger_issue": ledger_issue,
        "dispatch_id": dispatch_id,
        "reason": reason_code,
    }


def release_plan(packet_id, attempt_id, source_main_sha, reason_code, claim_nonce):
    if reason_code not in sm.LOCAL_RELEASE_REASONS:
        return {"released": False, "reason": "invalid_reason_code"}
    return _terminal_plan(packet_id, attempt_id, source_main_sha, reason_code, claim_nonce, unknown=False)


def block_plan(packet_id, attempt_id, source_main_sha, claim_nonce):
    return _terminal_plan(packet_id, attempt_id, source_main_sha, LOCAL_UNKNOWN_OUTPUT_REASON, claim_nonce, unknown=True)


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
    issue: int,
    attempt_id: str,
    client_token: str,
    expected_head_sha: str,
    claim_nonce: str | None = None,
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
    if not isinstance(details, dict):
        return {"handed_off": False, "issue": issue, "reason": "claim_details_invalid"}
    if claim_nonce is not None:
        if not sm.CLAIM_NONCE_PATTERN.fullmatch(claim_nonce):
            return {"handed_off": False, "issue": issue, "reason": "claim_nonce_invalid"}
        if details.get("claim_nonce") != claim_nonce:
            return {"handed_off": False, "issue": issue, "reason": "claim_nonce_mismatch"}
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
    issue: int,
    attempt_id: str,
    client_token: str,
    reason_code: str,
    claim_nonce: str | None = None,
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
    if not isinstance(details, dict):
        return {"released": False, "issue": issue, "reason": "claim_details_invalid"}
    if claim_nonce is not None:
        if not sm.CLAIM_NONCE_PATTERN.fullmatch(claim_nonce):
            return {"released": False, "issue": issue, "reason": "claim_nonce_invalid"}
        if details.get("claim_nonce") != claim_nonce:
            return {"released": False, "issue": issue, "reason": "claim_nonce_mismatch"}
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


def block_local(
    issue: int,
    attempt_id: str,
    client_token: str,
    reason_code: str,
    claim_nonce: str | None = None,
) -> dict[str, object]:
    """Terminalize a local attempt whose external result cannot be proven.

    This is deliberately not ``release-local``: an unknown push, PR, or
    process outcome is never retryable capacity.  The exact claim is first
    recorded as ``failed_unknown_output`` and only then moved to
    ``agent-blocked`` so a later poll cannot create a duplicate effect.
    """

    attempt = _normalized_attempt_id(attempt_id)
    if attempt is None:
        return {"blocked": False, "issue": issue, "reason": "invalid_attempt_id"}
    if not isinstance(client_token, str) or sm.CLAIM_NONCE_PATTERN.fullmatch(client_token) is None:
        return {"blocked": False, "issue": issue, "reason": "invalid_client_token"}
    if reason_code != LOCAL_UNKNOWN_OUTPUT_REASON:
        return {"blocked": False, "issue": issue, "reason": "invalid_reason_code"}
    dispatch_id = _local_dispatch_id(issue, attempt)
    repo = _repo()
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"blocked": False, "issue": issue, "reason": "disabled_or_emergency_stopped"}
    if not repo:
        return {"blocked": False, "issue": issue, "reason": "repository_unavailable"}
    claim, read_reason = _read_local_claim(issue, dispatch_id, repo)
    if claim is None:
        return {"blocked": False, "issue": issue, "reason": read_reason}
    structure_error = _claim_structure_error(claim, issue, dispatch_id)
    if structure_error is not None:
        return {"blocked": False, "issue": issue, "reason": structure_error}
    status = claim.get("status")
    if status not in {"claimed", "dispatched", "failed_unknown_output"}:
        return {"blocked": False, "issue": issue, "reason": "claim_state_unexpected"}
    details = claim.get("details")
    if not isinstance(details, dict):
        return {"blocked": False, "issue": issue, "reason": "claim_details_invalid"}
    if claim_nonce is not None:
        if not sm.CLAIM_NONCE_PATTERN.fullmatch(claim_nonce):
            return {"blocked": False, "issue": issue, "reason": "claim_nonce_invalid"}
        if details.get("claim_nonce") != claim_nonce:
            return {"blocked": False, "issue": issue, "reason": "claim_nonce_mismatch"}
    binding_ok, binding_reason = sm.local_claim_binding_valid(
        issue,
        details,
        attempt,
        client_token,
        require_lease_live=status != "failed_unknown_output",
    )
    if not binding_ok:
        return {"blocked": False, "issue": issue, "reason": binding_reason}
    outcome, payload = sm.release_local_claim_outcome(
        issue, dispatch_id, details.get("claim_nonce"), repo
    )
    if outcome == "superseded":
        return {"blocked": False, "issue": issue, "reason": "superseded"}
    if outcome == "unverifiable":
        return {"blocked": False, "issue": issue, "reason": payload}
    if outcome not in {"own-active", "own-terminal"}:
        return {"blocked": False, "issue": issue, "reason": "claim_not_found"}
    if status == "failed_unknown_output":
        if details.get("reason") != reason_code:
            return {"blocked": False, "issue": issue, "reason": "conflicting_terminal_state"}
    else:
        terminal_details = dict(details)
        terminal_details["reason"] = reason_code
        if not sm.record_dispatch_state(
            issue, dispatch_id, "local-run", "failed_unknown_output", terminal_details, repo
        ):
            return {"blocked": False, "issue": issue, "reason": "claim_state_failed_write"}
    blocked, block_reason = sm.release_failed_capacity(
        issue, sm.LABEL_RUNNING, sm.LABEL_BLOCKED, repo=repo
    )
    if not blocked:
        return {"blocked": False, "issue": issue, "reason": f"capacity_block_failed:{block_reason}"}
    return {"blocked": True, "issue": issue, "dispatch_id": dispatch_id, "reason": reason_code}


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
    repo = _repo()
    try:
        control_state.require_live(repo or None)
    except control_state.ControlStateError:
        return {"dispatched": False, "reason": "disabled_or_emergency_stopped"}
    binding_ok, binding_reason, live_binding = sm.verify_review_issue_pr_binding(
        issue, pr, sha, repo
    )
    if not binding_ok:
        return {"dispatched": False, "reason": f"binding_rejected:{binding_reason}"}
    sha = live_binding["head_sha"]
    dispatch_id = _dispatch_id("review", pr, sha)
    fields = {"pr_number": pr, "issue_number": issue, "head_sha": sha}
    claimed, previous, reason = _claim(
        issue, sm.LABEL_REVIEW_RUNNING, dispatch_id, "review", fields
    )
    if not claimed:
        return {"dispatched": reason == "already_dispatched", "reason": reason}
    rebound_ok, rebound_reason, rebound = sm.verify_review_issue_pr_binding(
        issue, pr, sha, repo
    )
    if not rebound_ok or rebound["head_sha"] != sha:
        rolled_back = _rollback(
            issue, dispatch_id, previous, "binding_changed_before_dispatch"
        )
        reason = (
            f"binding_rejected:{rebound_reason}"
            if rolled_back
            else "binding_changed_before_dispatch_rollback_failed"
        )
        return {"dispatched": False, "reason": reason}
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
    binding_ok, binding_reason, _live_binding = sm.verify_review_issue_pr_binding(
        issue, pr, sha, repo
    )
    if not binding_ok:
        return {"dispatched": False, "reason": f"binding_rejected:{binding_reason}"}
    try:
        import review_convergence as rc

        previous_review = sm.read_review_state(issue, repo)
        attempt = rc.derive_next_review_attempt(previous_review, sha)
    except sm.StateUnavailableError:
        return {"dispatched": False, "reason": "review_state_unavailable"}
    if not attempt.get("allowed"):
        return {
            "dispatched": False,
            "reason": f"review_budget_denied:{attempt.get('deny_reason') or 'not_allowed'}",
            "review_mode": attempt.get("review_mode"),
            "review_round": attempt.get("review_round"),
            "open_blocker_ids": attempt.get("open_blocker_ids") or [],
            "finding_ledger_digest": attempt.get("finding_ledger_digest") or "",
        }
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
    capacity = sm.get_active_capacity(repo)
    if capacity is None:
        return {"dispatched": False, "reason": "capacity_state_unavailable"}
    active = capacity["issues"]
    plans = capacity["plans"]
    if issue not in active and len(active) + len(plans) >= sm.MAX_ACTIVE:
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
    rebound_ok, rebound_reason, rebound = sm.verify_review_issue_pr_binding(
        issue, pr, sha, repo
    )
    if not rebound_ok or rebound["head_sha"] != sha:
        rolled_back = _rollback(
            issue, dispatch_id, previous_labels, "binding_changed_before_dispatch"
        )
        reason = (
            f"binding_rejected:{rebound_reason}"
            if rolled_back
            else "binding_changed_before_dispatch_rollback_failed"
        )
        return {"dispatched": False, "reason": reason}
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
    binding_ok, binding_reason, live_binding = sm.verify_review_issue_pr_binding(
        issue, pr, sha, _repo()
    )
    if not binding_ok:
        return {"dispatched": False, "reason": f"binding_rejected:{binding_reason}"}
    sha = live_binding["head_sha"]
    dispatch_id = _dispatch_id("merge", pr, sha)
    fields = {"pr_number": pr, "issue_number": issue, "head_sha": sha}
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
    rebound_ok, rebound_reason, rebound = sm.verify_review_issue_pr_binding(
        issue, pr, sha, _repo()
    )
    if not rebound_ok or rebound["head_sha"] != sha:
        return {"dispatched": False, "reason": f"binding_rejected:{rebound_reason}"}
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
    capacity = sm.get_active_capacity(repo)
    if capacity is None:
        return {"dispatched": False, "reason": "capacity-state-unavailable"}
    active = capacity["issues"]
    plans = capacity["plans"]
    if len(active) + len(plans) >= sm.MAX_ACTIVE:
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
    return {
        "dispatched": False,
        "reason": "no_dependency_ready_task",
        "active_plan_subject_ids": sorted(
            item["subject_id"] for item in plans if isinstance(item.get("subject_id"), str)
        ),
    }


def main() -> None:
    if len(sys.argv) < 2:
        raise SystemExit(
            "Usage: dispatcher.py <dispatch-ready|dispatch-repair|dispatch-review|"
            "retry-review|dispatch-merge|dispatch-next|claim-local|handoff-local|"
            "release-local|block-local|claim-plan|handoff-plan|lifecycle-plan|"
            "promote-plan|record-route-t3-receipt|record-route-owner-outcome|"
            "release-plan|block-plan> ..."
        )
    command = sys.argv[1]
    if command == "dispatch-ready" and len(sys.argv) in {3, 4}:
        result = dispatch_ready(int(sys.argv[2]), sys.argv[3] if len(sys.argv) == 4 else None)
    elif command == "claim-local" and len(sys.argv) == 5:
        result = claim_local(int(sys.argv[2]), sys.argv[3], sys.argv[4])
    elif command == "handoff-local" and len(sys.argv) == 7:
        result = handoff_local(int(sys.argv[2]), sys.argv[3], sys.argv[4], sys.argv[5], sys.argv[6])
    elif command == "release-local" and len(sys.argv) == 7:
        result = release_local(int(sys.argv[2]), sys.argv[3], sys.argv[4], sys.argv[5], sys.argv[6])
    elif command == "block-local" and len(sys.argv) == 7:
        result = block_local(int(sys.argv[2]), sys.argv[3], sys.argv[4], sys.argv[5], sys.argv[6])
    elif command == "claim-plan" and len(sys.argv) == 4:
        result = claim_plan(sys.argv[2], sys.argv[3])
    elif command == "handoff-plan" and len(sys.argv) == 6:
        result = handoff_plan(sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5])
    elif command == "lifecycle-plan" and len(sys.argv) == 5:
        result = record_plan_lifecycle(sys.argv[2], sys.argv[3], sys.argv[4])
    elif command == "promote-plan" and len(sys.argv) == 4:
        result = promote_plan(sys.argv[2], sys.argv[3])
    elif command == "record-route-t3-receipt" and len(sys.argv) == 4:
        result = dispatch_route_t3_payload(sys.argv[2], sys.argv[3])
    elif command == "record-route-owner-outcome" and len(sys.argv) == 4:
        result = dispatch_route_owner_payload(sys.argv[2], sys.argv[3])
    elif command == "release-plan" and len(sys.argv) == 7:
        result = release_plan(sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5], sys.argv[6])
    elif command == "block-plan" and len(sys.argv) == 6:
        result = block_plan(sys.argv[2], sys.argv[3], sys.argv[4], sys.argv[5])
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
        or result.get("blocked") is False
        or result.get("authorized") is False
        or result.get("recorded") is False
    ):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
