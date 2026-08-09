#!/usr/bin/env python3
"""Generate a compact, fail-closed repository handoff capsule."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from typing import Any


SCRIPTS_DIR = Path(__file__).resolve().parent
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

from github_observer import (  # noqa: E402
    GitHubObservationError,
    GitHubObserver,
    token_from_environment,
)


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REPOSITORY = "Igzela/token-efficient-agent-harness-lab"
PACKET_ID = r"(?:PE\d+|PR\d+|TOOL|CI|PRODUCT)(?:-[A-Z0-9]+)+"

# Logical required check names. These are the canonical names used in the matrix.
REQUIRED_CI_CHECKS = (
    "python-tests",
    "rust-tests",
    "pg-integration-tests",
    "typescript-tests",
    "native-runtime",
    "docker-build",
    "rust-typescript-cutover",
    "exact-head-check",
    "context-capsule",
)
# The terminal context-capsule publisher consumes the source matrix, so it is
# required for canonical acceptance but must not be listed as its own input.
REQUIRED_SOURCE_CI_CHECKS = tuple(
    name for name in REQUIRED_CI_CHECKS if name != "context-capsule"
)

# Explicit aliases for known check-name representations.
# Every alias must canonicalize to exactly one logical required check.
# Do not use substring, fuzzy, or similarity matching.
CHECK_NAME_ALIASES = {
    "exact-head-check": "exact-head-check",
    "exact-head": "exact-head-check",
    "exact-head-check / exact-head": "exact-head-check",
    "exact-head / exact-head-check": "exact-head-check",
}

FAILED_CONCLUSIONS = {
    "ACTION_REQUIRED",
    "CANCELLED",
    "FAILURE",
    "SKIPPED",
    "STALE",
    "STARTUP_FAILURE",
    "TIMED_OUT",
}
PENDING_STATES = {"EXPECTED", "IN_PROGRESS", "PENDING", "QUEUED", "REQUESTED", "WAITING"}


@dataclass(frozen=True)
class CommandResult:
    ok: bool
    stdout: str
    stderr: str
    returncode: int


def run_command(command: list[str], *, timeout: int = 15) -> CommandResult:
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        return CommandResult(False, "", str(error), 127)
    return CommandResult(
        result.returncode == 0,
        result.stdout.strip(),
        result.stderr.strip(),
        result.returncode,
    )


def read_text(relative_path: str) -> str:
    path = ROOT / relative_path
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def git_show_text(ref: str, relative_path: str) -> str:
    result = run_command(["git", "show", f"{ref}:{relative_path}"])
    return result.stdout if result.ok else ""


def section(text: str, heading: str) -> str:
    start = text.find(heading)
    if start < 0:
        return ""
    start += len(heading)
    end = text.find("\n## ", start)
    return text[start:] if end < 0 else text[start:end]


def parse_first_routed_packet(next_text: str) -> dict[str, str | None]:
    routing = section(next_text, "## Active Routing")
    packet_match = re.search(PACKET_ID, routing)
    if not packet_match:
        return {"packet": None, "state": None, "pr_number": None}
    packet = packet_match.group(0)
    heading = re.search(
        rf"^#{{2,3}} Packet {re.escape(packet)}\b.*$",
        next_text,
        re.MULTILINE,
    )
    block = ""
    if heading:
        next_heading = re.search(r"^#{2,3} Packet ", next_text[heading.end() :], re.MULTILINE)
        end = heading.end() + next_heading.start() if next_heading else len(next_text)
        block = next_text[heading.start() : end]
    state_match = re.search(r"^\*\*State:\*\* `([A-Z_]+)`", block, re.MULTILINE)
    structured_owner = re.search(
        r"^\*\*(?:Owned PR|Review surface):\*\*\s*(?P<value>.*?)\s*$",
        block,
        re.MULTILINE | re.IGNORECASE,
    )
    pr_number = None
    if structured_owner:
        structured_pr = re.fullmatch(
            r"#(?P<number>\d+)", structured_owner.group("value").strip()
        )
        if structured_pr:
            pr_number = structured_pr.group("number")
    # Older in-progress packets used a prose review-surface line before the
    # structured owner field was introduced.  Keep those exact legacy forms
    # readable, but never infer a PR from prerequisite/history prose.  In
    # particular, READY packets may list accepted prerequisite PRs and must
    # remain unbound until an owner field exists.
    if pr_number is None and state_match and state_match.group(1) == "IN_PROGRESS":
        legacy_review = re.search(
            r"^\s*(?:Current review surface is )?PR #(?P<number>\d+)\.?\s*$",
            block,
            re.MULTILINE | re.IGNORECASE,
        )
        has_prerequisite_prose = re.search(
            r"\b(?:prerequisite|prerequisites|accepted by|satisfied by|depends on)\b",
            block,
            re.IGNORECASE,
        )
        if legacy_review and not has_prerequisite_prose:
            pr_number = legacy_review.group("number")
    return {
        "packet": packet,
        "state": state_match.group(1) if state_match else None,
        "pr_number": pr_number,
    }


def parse_open_frontiers(status_text: str) -> list[dict[str, Any]]:
    """Read the legacy status table; new capsules use live observations.

    Kept for backward-compatible tooling/tests while accepted branches move
    dynamic PR state out of ``CURRENT_STATUS.md``.
    """
    block = section(status_text, "## Open Review Surfaces")
    frontiers: list[dict[str, Any]] = []
    for line in block.splitlines():
        match = re.match(r"^\|\s*#(\d+)\s*\|\s*([^|]+?)\s*\|\s*([^|]+?)\s*\|$", line)
        if not match:
            continue
        frontiers.append(
            {
                "pr": int(match.group(1)),
                "purpose": match.group(2).strip(),
                "documented_status": match.group(3).strip(),
            }
        )
    return frontiers


def _packet_body_binding(body: str, packet: str) -> bool:
    """Return whether a PR body explicitly binds itself to one packet."""
    return bool(
        re.search(
            rf"(?im)^\s*(?:Packet|Packet ID|Packet-ID)\s*:\s*`?{re.escape(packet)}`?\s*$",
            body,
        )
    )


def observe_open_frontiers(
    repository: str,
    packet: dict[str, Any],
    *,
    offline: bool,
    observer: GitHubObserver | None = None,
) -> dict[str, Any]:
    """Discover bounded live PR routing without making it authoritative.

    Canonical ``Owned PR`` wins when present. Otherwise an exact structured
    packet binding in the PR body is preferred. The exact normalized packet
    branch is a temporary legacy bridge. Ambiguity fails closed.
    """
    unavailable = {
        "availability": "unavailable",
        "source": None,
        "active_pr_number": (
            int(packet["pr_number"]) if packet.get("pr_number") else None
        ),
        "binding": "canonical_owned_pr" if packet.get("pr_number") else None,
        "warning": "offline_observation_disabled" if offline else None,
        "open_frontiers": [],
    }
    if offline:
        return unavailable

    observer = observer or GitHubObserver(
        repository, token=token_from_environment()
    )
    try:
        pulls = observer.list_open_pull_requests(base="main")
    except (GitHubObservationError, ValueError) as error:
        unavailable["warning"] = getattr(
            error, "reason", "github_observation_invalid"
        )
        return unavailable

    frontiers = [
        {
            "pr": item.get("number"),
            "purpose": item.get("title"),
            "head_sha": (item.get("head") or {}).get("sha"),
            "head_branch": (item.get("head") or {}).get("ref"),
            "draft": item.get("draft"),
            "url": item.get("html_url"),
        }
        for item in pulls
        if isinstance(item.get("number"), int)
    ]
    canonical = unavailable["active_pr_number"]
    if canonical is not None:
        if canonical not in {frontier["pr"] for frontier in frontiers}:
            return {
                **unavailable,
                "availability": "conflict",
                "source": "accepted_next_decision_plus_github_rest",
                "warning": "canonical_owned_pr_is_not_open_against_main",
                "open_frontiers": frontiers,
            }
        return {
            **unavailable,
            "availability": "confirmed",
            "source": "accepted_next_decision_plus_github_rest",
            "warning": None,
            "open_frontiers": frontiers,
        }

    packet_id = packet.get("packet")
    if not packet_id:
        return {
            **unavailable,
            "availability": "confirmed",
            "source": "github_rest",
            "warning": "canonical_packet_unavailable",
            "open_frontiers": frontiers,
        }

    explicit = [
        item
        for item in pulls
        if _packet_body_binding(str(item.get("body") or ""), str(packet_id))
    ]
    normalized_branch = str(packet_id).lower()
    legacy = [
        item
        for item in pulls
        if (item.get("head") or {}).get("ref") == normalized_branch
    ]
    explicit_numbers = {
        int(item["number"])
        for item in explicit
        if isinstance(item.get("number"), int)
    }
    legacy_numbers = {
        int(item["number"])
        for item in legacy
        if isinstance(item.get("number"), int)
    }
    if explicit_numbers and legacy_numbers - explicit_numbers:
        return {
            **unavailable,
            "availability": "conflict",
            "source": "github_rest",
            "warning": "structured_and_legacy_packet_bindings_conflict",
            "open_frontiers": frontiers,
        }
    candidates = explicit or legacy
    distinct = {
        int(item["number"])
        for item in candidates
        if isinstance(item.get("number"), int)
    }
    if len(distinct) > 1:
        return {
            **unavailable,
            "availability": "conflict",
            "source": "github_rest",
            "warning": "multiple_open_prs_bind_active_packet",
            "open_frontiers": frontiers,
        }
    active = next(iter(distinct), None)
    return {
        **unavailable,
        "availability": "confirmed",
        "source": "github_rest",
        "active_pr_number": active,
        "binding": (
            "pr_body_packet" if explicit and active is not None
            else "legacy_exact_packet_branch" if active is not None
            else None
        ),
        "warning": (
            "legacy_exact_packet_branch_binding" if legacy and not explicit
            else None
        ),
        "open_frontiers": frontiers,
    }


def repository_from_git() -> str:
    result = run_command(["git", "remote", "get-url", "origin"])
    if not result.ok or not result.stdout:
        return DEFAULT_REPOSITORY
    raw = result.stdout.strip()
    ssh_match = re.match(r"git@[^:]+:(?P<path>.+?)(?:\.git)?$", raw)
    if ssh_match:
        return ssh_match.group("path").removesuffix(".git")
    url_match = re.match(
        r"^[A-Za-z][A-Za-z0-9+.-]*://[^/]+/(?P<path>[^?#]+?)(?:\.git)?/?$",
        raw,
    )
    if url_match:
        path = url_match.group("path").strip("/").removesuffix(".git")
        if path.count("/") == 1:
            return path
    return DEFAULT_REPOSITORY


def accepted_baseline(*, offline: bool) -> dict[str, Any]:
    if not offline:
        remote = run_command(["git", "ls-remote", "origin", "refs/heads/main"])
        if remote.ok and remote.stdout:
            sha = remote.stdout.split()[0]
            if re.fullmatch(r"[0-9a-f]{40}", sha):
                return {
                    "branch": "main",
                    "sha": sha,
                    "availability": "confirmed",
                    "source": "git ls-remote origin refs/heads/main",
                }
    for ref in ("origin/main", "main"):
        local = run_command(["git", "rev-parse", "--verify", ref])
        if local.ok and re.fullmatch(r"[0-9a-f]{40}", local.stdout):
            return {
                "branch": "main",
                "sha": local.stdout,
                "availability": "local_only" if offline or ref != "origin/main" else "confirmed",
                "source": f"git rev-parse {ref}",
            }
    return {
        "branch": "main",
        "sha": None,
        "availability": "unavailable",
        "source": None,
    }


def ensure_commit_available(sha: str, *, offline: bool) -> bool:
    present = run_command(["git", "cat-file", "-e", f"{sha}^{{commit}}"])
    if present.ok:
        return True
    if offline:
        return False
    fetched = run_command(
        ["git", "fetch", "--no-tags", "--depth=1", "origin", sha],
        timeout=30,
    )
    if not fetched.ok:
        return False
    return run_command(["git", "cat-file", "-e", f"{sha}^{{commit}}"]).ok


def canonical_documents(baseline: dict[str, Any], *, offline: bool) -> dict[str, Any]:
    sha = baseline.get("sha")
    unavailable = {
        "availability": "unavailable",
        "source_sha": sha,
        "current_status": "",
        "next_decision": "",
    }
    if not isinstance(sha, str) or not re.fullmatch(r"[0-9a-f]{40}", sha):
        return unavailable
    if not ensure_commit_available(sha, offline=offline):
        return unavailable
    status = git_show_text(sha, "docs/CURRENT_STATUS.md")
    next_text = git_show_text(sha, "docs/NEXT_DECISION.md")
    if not status or not next_text:
        return unavailable
    return {
        "availability": baseline.get("availability", "local_only"),
        "source_sha": sha,
        "current_status": status,
        "next_decision": next_text,
    }


def _canonical_check_name(name: str) -> str | None:
    """Return the logical required-check name for an observed check name.

    Only exact aliases and exact logical names are accepted. No substring,
    fuzzy, or similarity matching is used.
    """
    normalized = name.strip()
    if normalized in CHECK_NAME_ALIASES:
        return CHECK_NAME_ALIASES[normalized]
    if normalized in REQUIRED_CI_CHECKS:
        return normalized
    return None


def _review_state_projection_unavailable(reason: str) -> dict[str, Any]:
    return {
        "availability": "unavailable",
        "unavailable_reason": reason,
        "review_protocol_version": None,
        "review_mode": None,
        "review_round": None,
        "prior_reviewed_head": None,
        "reviewed_head": None,
        "finding_ledger_digest": None,
        "open_blocker_ids": [],
        "deferred_note_ids": [],
        "autonomous_repairs_remaining": None,
        "stop_reason": None,
        "review_state": "unavailable",
    }


def _review_state_projection_conflict(reason: str) -> dict[str, Any]:
    projection = _review_state_projection_unavailable(reason)
    projection["availability"] = "conflict"
    projection["review_state"] = "conflict"
    return projection


def _linked_issue_numbers(pr_body: str) -> list[int]:
    """Linked packet issue numbers from the PR body binding convention."""
    if not pr_body:
        return []
    numbers: list[int] = []
    for match in re.finditer(
        r"(?:Closes|Fixes|Resolves|Implements)\s+#?(\d+)\b",
        pr_body,
        re.IGNORECASE,
    ):
        number = int(match.group(1))
        if number not in numbers:
            numbers.append(number)
    return numbers


def _comment_author_identity(comment: dict[str, Any]) -> str | None:
    author = comment.get("user") or comment.get("author") or {}
    return author.get("login") if isinstance(author, dict) else None


def _load_review_state_projection(
    repository: str,
    payload: dict[str, Any],
    *,
    observer: GitHubObserver | None = None,
) -> dict[str, Any]:
    """Project only trusted bounded fields from the durable review state.

    The durable ReviewState lives in the linked packet Issue comments
    (written by the trusted orchestrator finalize step).  This projection is
    non-authoritative and never decides severity, disposition, repair, Ready,
    or merge.  Full finding text never enters the capsule; only bounded ids,
    counts, and the ledger digest are projected.
    """
    head_sha = payload.get("headRefOid")
    if not head_sha:
        return _review_state_projection_unavailable("pr_head_unavailable")
    candidates = _linked_issue_numbers(str(payload.get("body") or ""))
    if not candidates:
        return _review_state_projection_unavailable("linked_issue_not_found")
    if observer is None:
        return _review_state_projection_unavailable(
            "trusted_review_state_observer_required"
        )

    sys.path.insert(0, str(ROOT / "scripts" / "agent-control"))
    try:
        import review_convergence as rc
    except ImportError:
        return _review_state_projection_unavailable("convergence_owner_unavailable")

    found: dict[int, dict[str, Any]] = {}
    for issue_number in candidates:
        try:
            comments = observer.issue_comments(issue_number)
        except GitHubObservationError:
            continue
        trusted_comments = [
            comment
            for comment in comments
            if _comment_author_identity(comment) in TRUSTED_REVIEW_STATE_AUTHORS
            and "agent-orchestrator-review-state"
            in str(comment.get("body") or "")
        ]
        state: dict[str, Any] | None = None
        # GitHub's issue-comments endpoint is oldest-first.  Only the newest
        # trusted state is authoritative; never fall back to an older PASS if
        # the latest state is malformed or blocking.
        for comment in reversed(trusted_comments):
            body = str(comment.get("body") or "")
            try:
                candidate = json.loads(body)
            except (json.JSONDecodeError, TypeError):
                found[issue_number] = _review_state_projection_conflict(
                    "latest_durable_review_state_is_malformed"
                )
                break
            if (
                isinstance(candidate, dict)
                and candidate.get("kind") == "agent-orchestrator-review-state"
            ):
                state = candidate
            else:
                found[issue_number] = _review_state_projection_conflict(
                    "latest_durable_review_state_kind_is_invalid"
                )
            break
        if issue_number in found:
            continue
        if state is None:
            continue
        try:
            projection = rc.project_capsule_fields(state, expected_head=head_sha)
        except (rc.ConvergenceError, TypeError, ValueError):
            found[issue_number] = _review_state_projection_conflict(
                "latest_durable_review_state_projection_failed"
            )
            continue
        found[issue_number] = projection

    if not found:
        return _review_state_projection_unavailable("durable_review_state_not_found")
    projections = list(found.values())
    first = projections[0]
    for projection in projections[1:]:
        if projection != first:
            return {
                "availability": "conflict",
                "unavailable_reason": "multiple_linked_issues_with_conflicting_review_state",
                **{key: first.get(key) for key in (
                    "review_protocol_version", "review_mode", "review_round",
                    "prior_reviewed_head", "reviewed_head", "finding_ledger_digest",
                    "open_blocker_ids", "deferred_note_ids",
                    "autonomous_repairs_remaining", "stop_reason", "review_state",
                )},
            }
    return first


def load_pr(
    repository: str,
    pr_number: int,
    *,
    offline: bool,
    observer: GitHubObserver | None = None,
) -> dict[str, Any]:
    unavailable = {
        "number": pr_number,
        "availability": "unavailable",
        "head_sha": None,
        "head_branch": None,
        "base_branch": None,
        "title": None,
        "url": None,
        "draft": None,
        "merge_state": None,
        "review_decision": None,
        "exact_head_review": {
            "state": "unavailable",
            "reason": "remote_review_state_unavailable",
        },
        "review_observation": {
            "observed_head_sha": None,
            "observation_time": None,
            "aggregate_review_state": None,
            "exact_head_review_state": "unavailable",
            "unresolved_objections_state": "unavailable",
            "unavailable_reason": "remote_review_state_unavailable",
        },
        "review_state_projection": _review_state_projection_unavailable(
            "remote_review_state_unavailable"
        ),
        "ci": {
            "state": "unavailable",
            "successful": [],
            "failed": [],
            "pending": [],
            "missing_required": list(REQUIRED_CI_CHECKS),
        },
    }
    if offline:
        return unavailable
    observer = observer or GitHubObserver(
        repository, token=token_from_environment()
    )
    try:
        rest_payload = observer.pull_request(pr_number)
        if rest_payload.get("state") != "open":
            raise GitHubObservationError("github_pull_request_not_open")
        if (rest_payload.get("base") or {}).get("ref") != "main":
            raise GitHubObservationError("github_pull_request_base_not_main")
        head_sha = (rest_payload.get("head") or {}).get("sha")
        if not isinstance(head_sha, str) or not re.fullmatch(r"[0-9a-f]{40}", head_sha):
            raise GitHubObservationError("github_pull_request_head_invalid")
        reviews = observer.pull_request_reviews(pr_number)
        comments = (
            observer.issue_comments(pr_number)
            + observer.pull_request_comments(pr_number)
        )
        checks = observer.check_runs(head_sha)
    except (GitHubObservationError, ValueError) as error:
        unavailable["unavailable_reason"] = getattr(
            error, "reason", "github_pr_observation_invalid"
        )
        return unavailable

    review_states = {str(item.get("state") or "").upper() for item in reviews}
    if "CHANGES_REQUESTED" in review_states:
        aggregate_review = "CHANGES_REQUESTED"
    elif "APPROVED" in review_states:
        aggregate_review = "APPROVED"
    else:
        aggregate_review = "REVIEW_REQUIRED"
    base_sha = (rest_payload.get("base") or {}).get("sha")
    author = rest_payload.get("user") or {}
    observation_time = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    review_observation = _build_review_observation(
        head_sha=head_sha,
        base_sha=base_sha,
        pr_author_identity=author.get("login") if isinstance(author, dict) else None,
        aggregate_review=aggregate_review,
        reviews=reviews,
        comments=comments,
        observation_time=observation_time,
    )
    projection_payload = {
        "headRefOid": head_sha,
        "body": rest_payload.get("body") or "",
    }
    review_state_projection = _load_review_state_projection(
        repository, projection_payload, observer=observer
    )
    _reconcile_review_state_projection(
        review_observation, review_state_projection
    )
    exact_review_state = review_observation.get("exact_head_review_state")
    exact_head_review = {
        "state": "confirmed" if exact_review_state == "confirmed" else "unverified",
        "reason": None
        if exact_review_state == "confirmed"
        else review_observation.get("unavailable_reason")
        or "exact_head_review_receipt_not_confirmed",
    }
    merge_state = str(rest_payload.get("mergeable_state") or "unknown").upper()
    return {
        "number": rest_payload.get("number", pr_number),
        "availability": "confirmed",
        "head_sha": head_sha,
        "head_branch": (rest_payload.get("head") or {}).get("ref"),
        "base_branch": (rest_payload.get("base") or {}).get("ref"),
        "title": rest_payload.get("title"),
        "url": rest_payload.get("html_url"),
        "draft": rest_payload.get("draft"),
        "merge_state": merge_state,
        "review_decision": aggregate_review,
        "exact_head_review": exact_head_review,
        "review_observation": review_observation,
        "review_state_projection": review_state_projection,
        "ci": summarize_checks(checks),
    }


REVIEW_RECEIPT_MARKER = "EXACT-HEAD REVIEW RECEIPT"
REVIEW_RECEIPT_REQUIRED_AXES = {
    "architecture",
    "authority",
    "compatibility",
    "security",
    "audit",
    "rollback",
    "scope/path binding",
}
REVIEW_SESSION_ID_PATTERN = re.compile(
    r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}",
    re.IGNORECASE,
)
TRUSTED_REVIEW_STATE_AUTHORS = frozenset({"github-actions", "github-actions[bot]"})


def _receipt_field(
    body: str, label: str, errors: list[str] | None = None
) -> str | None:
    matches = re.findall(
        rf"(?im)^\s*{re.escape(label)}\s*:\s*(.*?)\s*$", body
    )
    if len(matches) > 1 and errors is not None:
        errors.append(
            "review_field_duplicated:"
            + re.sub(r"[^a-z0-9]+", "_", label.lower()).strip("_")
        )
    value = matches[0].strip() if len(matches) == 1 else ""
    return value or None


def _parse_review_receipt(
    comment: dict[str, Any],
    expected_head_sha: str | None,
    expected_base_sha: str | None,
    expected_pr_author_identity: str | None,
) -> dict[str, Any]:
    body = str(comment.get("body") or "")
    author = comment.get("author") or comment.get("user") or {}
    author_identity = (
        author.get("login")
        if isinstance(author, dict)
        else None
    )
    errors: list[str] = []
    if body.count(REVIEW_RECEIPT_MARKER) != 1:
        errors.append("review_receipt_marker_count_invalid")
    reviewer_identity = _receipt_field(body, "Reviewer session identity", errors)
    authenticated_identity = _receipt_field(
        body, "Reviewer authenticated identity", errors
    )
    transport = _receipt_field(body, "Review transport", errors)
    implementation_identity = _receipt_field(
        body, "Implementation session identity", errors
    )
    reviewed_sha = _receipt_field(body, "Reviewed SHA", errors)
    reviewed_range = _receipt_field(body, "Reviewed range", errors)
    observed_at = _receipt_field(body, "Observed at", errors)
    axes_value = _receipt_field(body, "Axes", errors)
    outcome = (_receipt_field(body, "Outcome", errors) or "").upper()
    unresolved = (
        _receipt_field(body, "Unresolved objections", errors) or ""
    ).lower()

    if not author_identity:
        errors.append("reviewer_author_identity_missing")
    if not authenticated_identity:
        errors.append("reviewer_authenticated_identity_missing")
    elif author_identity and authenticated_identity.lower() != author_identity.lower():
        errors.append("reviewer_authenticated_identity_mismatch")
    if not expected_pr_author_identity:
        errors.append("pull_request_author_identity_missing")
    if not reviewer_identity or reviewer_identity.lower() in {
        "self",
        "self-review",
        "implementation-agent",
        "unknown",
    }:
        errors.append("reviewer_session_identity_missing")
    elif transport == "parent-posted-on-behalf-of-independent-session" and not REVIEW_SESSION_ID_PATTERN.fullmatch(
        reviewer_identity
    ):
        errors.append("parent_reviewer_session_identity_is_not_a_uuid")
    if author_identity and expected_pr_author_identity and author_identity.lower() == expected_pr_author_identity.lower():
        if transport != "parent-posted-on-behalf-of-independent-session":
            errors.append("same-author-review-requires-independent-transport")
        if not implementation_identity:
            errors.append("implementation_session_identity_missing")
        elif implementation_identity.lower() == reviewer_identity.lower():
            errors.append("reviewer_and_implementation_sessions_match")
    elif transport != "direct-github-reviewer":
        errors.append("direct-review-requires-authenticated-reviewer-transport")
    if not expected_head_sha or not re.fullmatch(r"[0-9a-f]{40}", expected_head_sha):
        errors.append("current_head_sha_missing_or_invalid")
    if not reviewed_sha or not re.fullmatch(r"[0-9a-f]{40}", reviewed_sha):
        errors.append("reviewed_sha_missing_or_invalid")
    if reviewed_sha and reviewed_sha != expected_head_sha:
        errors.append("reviewed_sha_does_not_match_current_head")
    range_match = (
        re.fullmatch(r"([0-9a-f]{40})\.\.\.([0-9a-f]{40})", reviewed_range or "")
        if reviewed_range
        else None
    )
    if not range_match:
        errors.append("complete_diff_range_missing_or_invalid")
    elif expected_head_sha and range_match.group(2) != expected_head_sha:
        errors.append("complete_diff_range_does_not_match_current_head")
    if not expected_base_sha or not re.fullmatch(r"[0-9a-f]{40}", expected_base_sha):
        errors.append("accepted_base_sha_missing_or_invalid")
    elif range_match and range_match.group(1) != expected_base_sha:
        errors.append("complete_diff_range_does_not_match_accepted_base")
    if not observed_at:
        errors.append("observation_time_missing")
    else:
        try:
            parsed_observed_at = datetime.fromisoformat(observed_at.replace("Z", "+00:00"))
        except ValueError:
            parsed_observed_at = None
        if parsed_observed_at is None or parsed_observed_at.tzinfo is None:
            errors.append("observation_time_is_not_iso8601_with_timezone")
    axes_lower = (axes_value or "").lower()
    negative_review = r"\b(?:not|no|missing|excluded|unreviewed|unavailable|without|never)\b"
    negated_axes = sorted(
        axis
        for axis in REVIEW_RECEIPT_REQUIRED_AXES
        if re.search(
            rf"(?:{negative_review}[^,;\n]*{re.escape(axis)}|"
            rf"{re.escape(axis)}[^,;\n]*{negative_review})",
            axes_lower,
        )
    )
    if negated_axes:
        errors.append("review_axes_negated:" + ",".join(negated_axes))
    missing_axes = sorted(
        axis
        for axis in REVIEW_RECEIPT_REQUIRED_AXES
        if not re.search(
            rf"(?<![a-z0-9]){re.escape(axis)}(?![a-z0-9])", axes_lower
        )
    )
    if missing_axes:
        errors.append("review_axes_missing:" + ",".join(missing_axes))
    if outcome != "PASS":
        errors.append("review_outcome_is_not_exact_pass")
    if unresolved not in {"none", "none observed"}:
        errors.append("unresolved_objections_present_or_unspecified")

    state = "valid" if not errors else "invalid"
    return {
        "state": state,
        "observed_head_sha": reviewed_sha,
        "complete_diff_range": reviewed_range,
        "reviewer_session_identity": reviewer_identity,
        "reviewer_authenticated_identity": authenticated_identity,
        "review_transport": transport,
        "implementation_session_identity": implementation_identity,
        "reviewer_author_identity": author_identity,
        "observation_time": observed_at,
        "axes": axes_value,
        "outcome": outcome or None,
        "unresolved_objections": unresolved or None,
        "errors": errors,
    }


def _build_review_observation(
    *,
    head_sha: str | None,
    base_sha: str | None = None,
    pr_author_identity: str | None = None,
    aggregate_review: str | None,
    reviews: list[dict[str, Any]],
    comments: list[dict[str, Any]],
    observation_time: str,
) -> dict[str, Any]:
    """Build a fail-closed review observation from available GitHub evidence.

    Exact-head acceptance is never inferred from aggregate state or prose.
    A review receipt comment (``EXACT-HEAD REVIEW RECEIPT`` marker, see
    ``docs/REAL_WORLD_TESTING_PLAYBOOK.md``) is the only evidence that a
    complete-diff review was bound to a specific head; a receipt naming a
    different head is stale, not acceptance.
    """
    observation: dict[str, Any] = {
        "observed_head_sha": head_sha,
        "observation_time": observation_time,
        "aggregate_review_state": aggregate_review,
        "exact_head_review_state": "unverified",
        "unresolved_objections_state": "unavailable",
        "unavailable_reason": None,
        "review_receipt": {
            "state": "unavailable",
            "observed_head_sha": None,
            "outcome": None,
        },
    }
    receipt_comments = [
        comment
        for comment in comments
        if REVIEW_RECEIPT_MARKER in str(comment.get("body") or "")
    ]
    if receipt_comments:
        parsed_receipts = [
            _parse_review_receipt(
                receipt, head_sha, base_sha, pr_author_identity
            )
            for receipt in receipt_comments
        ]
        current_receipts = [
            receipt
            for receipt in parsed_receipts
            if receipt.get("observed_head_sha") == head_sha
        ]
        unbound_receipts = [
            receipt
            for receipt in parsed_receipts
            if not re.fullmatch(
                r"[0-9a-f]{40}", str(receipt.get("observed_head_sha") or "")
            )
        ]
        if len(current_receipts) == 1 and not unbound_receipts:
            parsed_receipt = current_receipts[0]
            observation["review_receipt"] = parsed_receipt
        elif len(current_receipts) > 1:
            parsed_receipt = {
                "state": "invalid",
                "observed_head_sha": head_sha,
                "outcome": None,
                "errors": ["multiple_current_head_review_receipts"],
            }
            observation["review_receipt"] = parsed_receipt
        elif unbound_receipts:
            parsed_receipt = {
                "state": "invalid",
                "observed_head_sha": head_sha,
                "outcome": None,
                "errors": ["unbound_review_receipt_present"],
            }
            observation["review_receipt"] = parsed_receipt
        else:
            parsed_receipt = {
                "state": "stale",
                "observed_head_sha": None,
                "outcome": None,
                "errors": ["review_receipt_not_for_current_head"],
            }
            observation["review_receipt"] = parsed_receipt
        if parsed_receipt["state"] == "valid":
            observation["exact_head_review_state"] = "receipt_observed"
        else:
            observation["exact_head_review_state"] = "receipt_invalid"
            observation["unavailable_reason"] = "review_receipt_is_invalid"
    if not reviews and not comments:
        observation["unresolved_objections_state"] = "unavailable"
        observation["unavailable_reason"] = "no_reviews_or_comments_exposed"
        return observation

    blocking_reviews = [
        review
        for review in reviews
        if str(review.get("state") or "").upper() == "CHANGES_REQUESTED"
    ]
    if blocking_reviews:
        observation["unresolved_objections_state"] = "blocking_reviews_present"
        observation["exact_head_review_state"] = "unverified"
        return observation

    # Comments from the GitHub REST API do not expose resolved/unresolved state
    # reliably. Recognize both the legacy literal and the convergence protocol's
    # structured open-blocker vocabulary so neither surface can hide objections.
    structured_block = re.compile(
        r"(?is)(?:disposition[\"']?\s*[:=]\s*[\"'`]?block_current_head[\"'`]?"
        r".*?status[\"']?\s*[:=]\s*[\"'`]?open[\"'`]?|"
        r"status[\"']?\s*[:=]\s*[\"'`]?open[\"'`]?.*?"
        r"disposition[\"']?\s*[:=]\s*[\"'`]?block_current_head[\"'`]?)"
    )
    explicit_blocking = [
        review
        for review in reviews
        if "BLOCKING" in str(review.get("body") or "").upper()
        or structured_block.search(str(review.get("body") or ""))
    ] + [
        comment
        for comment in comments
        if "BLOCKING" in str(comment.get("body") or "").upper()
        or structured_block.search(str(comment.get("body") or ""))
    ]
    if explicit_blocking:
        observation["unresolved_objections_state"] = "explicit_blocking_comments_present"
        observation["exact_head_review_state"] = "unverified"
        return observation

    if (
        observation["review_receipt"].get("state") == "valid"
        and observation["review_receipt"].get("unresolved_objections")
        in {"none", "none observed"}
    ):
        observation["unresolved_objections_state"] = "none_observed"
        observation["exact_head_review_state"] = "confirmed"
    elif aggregate_review == "APPROVED" and reviews:
        # Aggregate approval exists, but we still do not treat it as exact-head
        # independent acceptance. Mark objections as none observed, not resolved.
        observation["unresolved_objections_state"] = "none_observed"
    else:
        observation["unresolved_objections_state"] = "unavailable"
        if observation["unavailable_reason"] is None:
            observation["unavailable_reason"] = "insufficient_review_evidence"
    return observation


def _reconcile_review_state_projection(
    observation: dict[str, Any], projection: dict[str, Any]
) -> None:
    """Fail closed when trusted durable state contradicts a PASS receipt."""
    availability = projection.get("availability")
    if availability == "unavailable":
        return
    open_blockers = projection.get("open_blocker_ids") or []
    review_state = str(projection.get("review_state") or "").upper()
    if availability == "conflict":
        reason = "durable_review_state_conflict"
    elif open_blockers:
        reason = "durable_review_state_has_open_blockers"
    elif review_state != "PASS":
        reason = "durable_review_state_is_not_pass"
    else:
        return
    observation["exact_head_review_state"] = "unverified"
    observation["unresolved_objections_state"] = reason
    observation["unavailable_reason"] = reason


def summarize_checks(checks: list[dict[str, Any]]) -> dict[str, Any]:
    successful: list[str] = []
    failed: list[str] = []
    pending: list[str] = []
    observed_required: set[str] = set()
    successful_required: set[str] = set()
    raw_by_canonical: dict[str, list[str]] = {name: [] for name in REQUIRED_CI_CHECKS}

    for check in checks:
        name = str(
            check.get("name")
            or check.get("context")
            or check.get("workflowName")
            or "unnamed-check"
        )
        conclusion = str(check.get("conclusion") or "").upper()
        status = str(check.get("status") or check.get("state") or "").upper()
        required_name = _canonical_check_name(name)
        if required_name:
            observed_required.add(required_name)
            raw_by_canonical.setdefault(required_name, []).append(name)
        if conclusion == "SUCCESS":
            successful.append(name)
            if required_name:
                successful_required.add(required_name)
        elif conclusion in FAILED_CONCLUSIONS:
            failed.append(name)
        elif status in PENDING_STATES or not conclusion:
            pending.append(name)
        else:
            pending.append(name)
    missing_required = sorted(set(REQUIRED_CI_CHECKS) - observed_required)
    incomplete_required = sorted(observed_required - successful_required)
    if failed:
        state = "failed"
    elif pending or incomplete_required:
        state = "pending"
    elif missing_required:
        state = "incomplete"
    elif set(REQUIRED_CI_CHECKS).issubset(successful_required):
        state = "success"
    else:
        state = "unavailable"
    return {
        "state": state,
        "successful": sorted(set(successful)),
        "failed": sorted(set(failed)),
        "pending": sorted(set(pending)),
        "missing_required": missing_required,
        "incomplete_required": incomplete_required,
        "raw_by_canonical": {k: sorted(set(v)) for k, v in raw_by_canonical.items()},
    }


def source_required_check_matrix(
    ci_summary: dict[str, Any], event_name: str | None = None
) -> list[dict[str, Any]]:
    """Return a per-required-check view of observed matrix state.

    For push and workflow_dispatch events the PR-only `exact-head-check` is
    marked not_applicable rather than missing.
    """
    def canonical_names(outcomes: list[str]) -> set[str]:
        return {
            _canonical_check_name(str(name)) or str(name)
            for name in outcomes
        }

    successful = canonical_names(ci_summary.get("successful") or [])
    failed = canonical_names(ci_summary.get("failed") or [])
    pending = canonical_names(ci_summary.get("pending") or [])
    raw_by_canonical = ci_summary.get("raw_by_canonical") or {}
    matrix: list[dict[str, Any]] = []
    for required in REQUIRED_SOURCE_CI_CHECKS:
        raw_names = raw_by_canonical.get(required) or []
        if required in failed:
            conclusion = "failed"
        elif required in pending:
            conclusion = "pending"
        elif required in successful:
            conclusion = "success"
        elif raw_names:
            conclusion = "pending"
        else:
            conclusion = "missing"
        if (
            required == "exact-head-check"
            and conclusion == "missing"
            and event_name in {"push", "workflow_dispatch"}
        ):
            conclusion = "not_applicable"
        matrix.append(
            {
                "logical_name": required,
                "observed": bool(raw_names),
                "conclusion": conclusion,
                "raw_names": sorted(set(raw_names)),
            }
        )
    return matrix


def parse_checks_json(raw: str) -> list[dict[str, Any]]:
    """Convert a GitHub Actions `needs` JSON blob into a checks array.

    Expected shape: {"job-id": {"result": "success", "outputs": {}}, ...}
    """
    try:
        data = json.loads(raw)
    except json.JSONDecodeError:
        return []
    if not isinstance(data, dict):
        return []
    checks: list[dict[str, Any]] = []
    for job_id, info in data.items():
        if not isinstance(info, dict):
            continue
        result = str(info.get("result") or "").upper()
        checks.append(
            {
                "name": job_id,
                "status": "COMPLETED" if result else "IN_PROGRESS",
                "conclusion": result if result else None,
            }
        )
    return checks


def is_matrix_successful(
    matrix: list[dict[str, Any]], *, event_name: str | None = None
) -> bool:
    """True only for one complete, successful entry per required check."""
    if not isinstance(matrix, list) or len(matrix) != len(REQUIRED_SOURCE_CI_CHECKS):
        return False
    observed_required: set[str] = set()
    for item in matrix:
        if not isinstance(item, dict):
            return False
        required = item.get("logical_name")
        conclusion = item.get("conclusion")
        raw_names = item.get("raw_names")
        if (
            required not in REQUIRED_SOURCE_CI_CHECKS
            or required in observed_required
            or not isinstance(raw_names, list)
            or conclusion not in {"success", "not_applicable"}
            or (
                conclusion == "success"
                and (item.get("observed") is not True or not raw_names)
            )
            or (
                conclusion == "not_applicable"
                and (
                    required != "exact-head-check"
                    or event_name not in {"push", "workflow_dispatch"}
                    or item.get("observed") is not False
                    or raw_names
                )
            )
        ):
            return False
        observed_required.add(required)
    return observed_required == set(REQUIRED_SOURCE_CI_CHECKS)


def has_valid_success_binding(capsule: dict[str, Any]) -> bool:
    """Validate the minimal generated-capsule shape before accepting success."""
    if not isinstance(capsule, dict) or capsule.get("schema_version") != "project_context.v1":
        return False
    binding = capsule.get("binding")
    if not isinstance(binding, dict):
        return False
    run_identity = binding.get("workflow_run_identity")
    if (
        not isinstance(run_identity, dict)
        or run_identity.get("availability") != "confirmed"
    ):
        return False
    bound_event = run_identity.get("event_name")
    live_event = os.environ.get("GITHUB_EVENT_NAME")
    if live_event and bound_event != live_event:
        return False
    event_name = live_event or bound_event
    if event_name not in {"push", "pull_request", "workflow_dispatch"}:
        return False
    matrix = binding.get("source_required_check_matrix")
    if not is_matrix_successful(matrix, event_name=event_name):
        return False
    expected_head = binding.get("expected_head_sha")
    checked_out = binding.get("checked_out_sha")
    if (
        not isinstance(expected_head, str)
        or not re.fullmatch(r"[0-9a-f]{40}", expected_head)
        or checked_out != expected_head
    ):
        return False
    if event_name == "pull_request":
        requested = binding.get("requested_pr_exact_head")
        if (
            not isinstance(requested, dict)
            or not isinstance(requested.get("number"), int)
            or not isinstance(requested.get("head_sha"), str)
            or not re.fullmatch(r"[0-9a-f]{40}", requested["head_sha"])
        ):
            return False
        return is_requested_head_matched(capsule)
    return True


def is_requested_head_matched(capsule: dict[str, Any]) -> bool:
    """Require a requested exact head to match a confirmed PR observation."""
    binding = capsule.get("binding") or {}
    if not isinstance(binding, dict):
        return False
    expected_head = (binding.get("requested_pr_exact_head") or {}).get("head_sha")
    if not expected_head:
        return True
    observed = binding.get("pr_exact_head") or {}
    return (
        observed.get("availability") == "confirmed"
        and observed.get("head_sha") == expected_head
    )


def load_exact_head_proof(
    path: Path,
    *,
    repository: str,
    pr_number: int | None,
    expected_head_sha: str | None,
) -> dict[str, Any]:
    """Validate a trusted exact-head action proof into a bounded PR observation.

    The caller supplies this only from the trusted-base exact-head action. It
    carries no review text, logs, or token material. Invalid or mismatched
    proofs fail closed instead of becoming a caller-asserted success.
    """
    try:
        proof = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ValueError(f"exact-head proof is unreadable: {exc}") from exc
    if not isinstance(proof, dict):
        raise ValueError("exact-head proof must be a JSON object")

    actual_pr = proof.get("pull_request")
    actual_head = proof.get("live_head")
    expected = proof.get("expected_head")
    if (
        proof.get("kind") != "exact-head-check-proof.v1"
        or proof.get("status") != "pass"
        or proof.get("reason") != "exact_head_match"
        or proof.get("repository") != repository
        or not isinstance(actual_pr, int)
        or pr_number != actual_pr
        or not isinstance(actual_head, str)
        or not re.fullmatch(r"[0-9a-f]{40}", actual_head)
        or expected_head_sha != actual_head
        or expected != actual_head
        or proof.get("pr_state") != "open"
    ):
        raise ValueError("exact-head proof does not confirm the requested open PR head")
    return {
        "number": actual_pr,
        "head_sha": actual_head,
        "head_branch": None,
        "base_branch": None,
        "availability": "confirmed",
        "ci": {
            "state": "unavailable",
            "successful": [],
            "failed": [],
            "pending": [],
            "missing_required": list(REQUIRED_CI_CHECKS),
        },
        "review_observation": {
            "observed_head_sha": actual_head,
            "observation_time": None,
            "aggregate_review_state": None,
            "exact_head_review_state": "unavailable",
            "unresolved_objections_state": "unavailable",
            "unavailable_reason": "trusted_exact_head_proof_has_no_review_observation",
        },
        "review_state_projection": _review_state_projection_unavailable(
            "trusted_exact_head_proof_has_no_review_state"
        ),
    }


def local_checkout_state() -> dict[str, Any]:
    head = run_command(["git", "rev-parse", "--verify", "HEAD"])
    branch = run_command(["git", "symbolic-ref", "--short", "-q", "HEAD"])
    status = run_command(["git", "status", "--porcelain"])
    changes = status.stdout.splitlines() if status.ok and status.stdout else []
    return {
        "head_sha": head.stdout if head.ok and re.fullmatch(r"[0-9a-f]{40}", head.stdout) else None,
        "branch": branch.stdout if branch.ok and branch.stdout else None,
        "detached": not branch.ok or not branch.stdout,
        "dirty": bool(changes),
        "change_count": len(changes),
    }


def workflow_run_identity() -> dict[str, Any]:
    """Capture GitHub Actions workflow/run identity when available.

    Returns unavailable outside a workflow run.
    """
    run_id = os.environ.get("GITHUB_RUN_ID")
    if not run_id:
        return {
            "availability": "unavailable",
            "run_id": None,
            "run_attempt": None,
            "event_name": None,
            "repository": os.environ.get("GITHUB_REPOSITORY"),
            "workflow": None,
            "job": None,
        }
    return {
        "availability": "confirmed",
        "run_id": run_id,
        "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
        "event_name": os.environ.get("GITHUB_EVENT_NAME"),
        "repository": os.environ.get("GITHUB_REPOSITORY"),
        "workflow": os.environ.get("GITHUB_WORKFLOW"),
        "job": os.environ.get("GITHUB_JOB"),
    }


def session_binding() -> dict[str, Any]:
    """Capture live session/workflow binding without provider credentials."""
    return {
        "availability": "confirmed" if os.environ.get("GITHUB_RUN_ID") else "unavailable",
        "runner_os": os.environ.get("RUNNER_OS"),
        "runner_arch": os.environ.get("RUNNER_ARCH"),
        "github_actor": os.environ.get("GITHUB_ACTOR"),
        "github_ref": os.environ.get("GITHUB_REF"),
        "github_sha": os.environ.get("GITHUB_SHA"),
        "github_run_id": os.environ.get("GITHUB_RUN_ID"),
        "github_run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
        "github_event_name": os.environ.get("GITHUB_EVENT_NAME"),
    }


def staleness_conditions() -> list[str]:
    return [
        "accepted `main` SHA changes",
        "canonical active PR or workflow PR exact head changes",
        "any required CI check conclusion changes",
        "canonical documents change",
        "review state or unresolved objections change",
        "review receipt head or outcome changes",
        "workflow run identity changes",
        "local checkout becomes dirty or switches branch",
    ]


def compute_fingerprint(capsule: dict[str, Any]) -> str:
    """Stable fingerprint over immutable binding fields.

    This is transport-integrity evidence only, not authority. It intentionally
    excludes mutable fields such as observation time and next permitted action.
    """
    binding = capsule.get("binding", {})
    accepted = binding.get("accepted_baseline", {})
    canonical = binding.get("canonical_document_source", {})
    routed = binding.get("canonical_routed_packet", {})
    pr = binding.get("pr_exact_head", {})
    canonical_pr = binding.get("canonical_active_pr_exact_head", {})
    requested_pr = binding.get("requested_pr_exact_head", {})
    run = binding.get("workflow_run_identity", {})
    fingerprint_input = {
        "repository": capsule.get("repository"),
        "accepted_main_sha": accepted.get("sha"),
        "canonical_document_source_sha": canonical.get("source_sha"),
        "canonical_routed_packet": routed.get("packet"),
        "pr_number": pr.get("number"),
        "pr_exact_head_sha": pr.get("head_sha"),
        "canonical_active_pr_number": canonical_pr.get("number"),
        "canonical_active_pr_exact_head_sha": canonical_pr.get("head_sha"),
        "requested_pr_number": requested_pr.get("number"),
        "requested_pr_exact_head_sha": requested_pr.get("head_sha"),
        "checked_out_sha": binding.get("checked_out_sha"),
        "expected_head_sha": binding.get("expected_head_sha"),
        "workflow_run_id": run.get("run_id"),
        "workflow_run_attempt": run.get("run_attempt"),
    }
    canonical_json = json.dumps(fingerprint_input, sort_keys=True, ensure_ascii=True)
    return hashlib.sha256(canonical_json.encode("utf-8")).hexdigest()[:24]


def next_permitted_action(packet: dict[str, Any], active_pr: dict[str, Any] | None) -> str:
    packet_id = packet.get("packet") or "the earliest eligible packet"
    state = packet.get("state")
    if state == "BLOCKED_PREREQUISITE":
        return f"resolve the named prerequisite for {packet_id}; do not implement the blocked packet"
    if state == "COMPLETE":
        return f"{packet_id} is complete; refresh accepted main and select the next eligible packet"
    if not active_pr:
        if state == "READY_FOR_EXECUTION":
            return (
                f"confirm the documented prerequisites and bounded action for {packet_id}; "
                "do not infer an implementation PR or provider effect"
            )
        return f"inspect {packet_id}, confirm ownership, and create or continue one focused PR"
    number = active_pr.get("number")
    if active_pr.get("availability") != "confirmed":
        return f"refresh PR #{number} exact head, CI, and review state before acting"
    ci = active_pr.get("ci", {})
    ci_state = ci.get("state")
    if ci_state == "failed":
        return f"repair the failing exact-head checks for PR #{number} without weakening guards"
    exact_review = active_pr.get("exact_head_review", {})
    if active_pr.get("draft") is True:
        if exact_review.get("state") != "confirmed":
            return (
                f"stabilize Draft PR #{number} at exact head {active_pr.get('head_sha')}, "
                "complete focused checks, and obtain independent exact PASS; keep it Draft"
            )
        return (
            f"mark independently accepted PR #{number} Ready, then run canonical exact-head CI"
        )
    if exact_review.get("state") != "confirmed":
        return (
            f"obtain independent acceptance for PR #{number} at exact head "
            f"{active_pr.get('head_sha')} and verify unresolved objections"
        )
    if ci_state == "incomplete":
        missing = ", ".join(ci.get("missing_required") or [])
        return f"obtain the missing required exact-head checks for PR #{number}: {missing}"
    if ci_state in {"pending", "unavailable"}:
        return f"complete or verify all required exact-head CI for PR #{number}"
    return (
        f"confirm explicit merge authority and full merge eligibility for PR #{number}; "
        "do not merge automatically"
    )


def build_capsule(
    *,
    offline: bool,
    repository: str | None = None,
    checks_json: str | None = None,
    event_name: str | None = None,
    pr_number: int | None = None,
    expected_head_sha: str | None = None,
    exact_head_proof: Path | None = None,
) -> dict[str, Any]:
    repository = repository or repository_from_git()
    baseline = accepted_baseline(offline=offline)
    documents = canonical_documents(baseline, offline=offline)
    next_text = documents.get("next_decision", "")
    packet = parse_first_routed_packet(next_text)
    observer = None if offline else GitHubObserver(
        repository, token=token_from_environment()
    )
    frontier_observation = observe_open_frontiers(
        repository,
        packet,
        offline=offline,
        observer=observer,
    )

    event_name = event_name or os.environ.get("GITHUB_EVENT_NAME")
    provided_checks = parse_checks_json(checks_json) if checks_json else []

    routed_pr_number = int(packet["pr_number"]) if packet.get("pr_number") else None
    discovered_pr_number = frontier_observation.get("active_pr_number")
    canonical_pr_number = (
        routed_pr_number
        if routed_pr_number is not None
        else discovered_pr_number
    )
    workflow_pr_number = pr_number if pr_number is not None else canonical_pr_number
    if exact_head_proof:
        workflow_pr = load_exact_head_proof(
            exact_head_proof,
            repository=repository,
            pr_number=workflow_pr_number,
            expected_head_sha=expected_head_sha,
        )
        provided_checks.append(
            {
                "name": "exact-head-check",
                "status": "COMPLETED",
                "conclusion": "SUCCESS",
            }
        )
        active_pr = (
            workflow_pr
            if canonical_pr_number == workflow_pr_number
            else load_pr(
                repository,
                canonical_pr_number,
                offline=offline,
                observer=observer,
            )
            if canonical_pr_number
            else None
        )
    else:
        active_pr = (
            load_pr(
                repository,
                canonical_pr_number,
                offline=offline,
                observer=observer,
            )
            if canonical_pr_number
            else None
        )
        workflow_pr = (
            active_pr
            if workflow_pr_number == canonical_pr_number
            else load_pr(
                repository,
                workflow_pr_number,
                offline=offline,
                observer=observer,
            )
            if workflow_pr_number
            else None
        )
    frontiers = frontier_observation.get("open_frontiers") or []
    represented_numbers = {
        number
        for number in (canonical_pr_number, workflow_pr_number)
        if number is not None
    }
    blocked_frontiers = [
        frontier
        for frontier in frontiers
        if frontier["pr"] not in represented_numbers
    ]
    checkout = local_checkout_state()
    if (
        expected_head_sha
        and event_name in {"push", "workflow_dispatch"}
        and checkout.get("head_sha") != expected_head_sha
    ):
        raise ValueError("capsule checkout does not match expected exact head")
    checkout["matches_accepted_baseline"] = bool(
        checkout.get("head_sha")
        and baseline.get("sha")
        and checkout.get("head_sha") == baseline.get("sha")
    )
    checkout["matches_active_frontier"] = bool(
        checkout.get("head_sha")
        and active_pr
        and active_pr.get("head_sha")
        and checkout.get("head_sha") == active_pr.get("head_sha")
    )
    checkout["matches_workflow_frontier"] = bool(
        checkout.get("head_sha")
        and workflow_pr
        and workflow_pr.get("head_sha")
        and checkout.get("head_sha") == workflow_pr.get("head_sha")
    )

    if workflow_pr and workflow_pr.get("availability") == "confirmed":
        pr_ci_summary = workflow_pr.get("ci", {})
        pr_check_items = [
            {"name": name, "status": "COMPLETED", "conclusion": outcome.upper()}
            for outcome in ("successful", "failed", "pending")
            for name in (pr_ci_summary.get(outcome) or [])
        ]
        if provided_checks:
            # Merge provided checks (e.g., current workflow needs) with PR status
            # rollup so that PR-only exact-head-check is preserved while current
            # run results are available immediately.
            ci_summary = summarize_checks(provided_checks + pr_check_items)
        else:
            ci_summary = pr_ci_summary
        workflow_pr["ci"] = ci_summary
    elif provided_checks:
        ci_summary = summarize_checks(provided_checks)
        if workflow_pr:
            workflow_pr["ci"] = ci_summary
    else:
        ci_summary = {
            "state": "unavailable",
            "successful": [],
            "failed": [],
            "pending": [],
            "missing_required": list(REQUIRED_CI_CHECKS),
        }
    matrix = source_required_check_matrix(ci_summary, event_name=event_name)
    run_identity = workflow_run_identity()
    session = session_binding()

    review_observation = (
        workflow_pr.get("review_observation")
        if isinstance(workflow_pr, dict)
        and isinstance(workflow_pr.get("review_observation"), dict)
        else {
            "observed_head_sha": None,
            "observation_time": None,
            "aggregate_review_state": None,
            "exact_head_review_state": "unavailable",
            "unresolved_objections_state": "unavailable",
            "unavailable_reason": "no_workflow_pr_review_observation",
            "review_receipt": {
                "state": "unavailable",
                "observed_head_sha": None,
                "outcome": None,
            },
        }
    )
    binding = {
        "accepted_baseline": baseline,
        "canonical_document_source": {
            "availability": documents.get("availability"),
            "source_sha": documents.get("source_sha"),
        },
        "canonical_routed_packet": packet,
        "frontier_observation": frontier_observation,
        "session_binding": session,
        "pr_exact_head": {
            "number": workflow_pr.get("number") if workflow_pr else None,
            "head_sha": workflow_pr.get("head_sha") if workflow_pr else None,
            "head_branch": workflow_pr.get("head_branch") if workflow_pr else None,
            "base_branch": workflow_pr.get("base_branch") if workflow_pr else None,
            "availability": workflow_pr.get("availability") if workflow_pr else "unavailable",
        },
        "canonical_active_pr_exact_head": {
            "number": active_pr.get("number") if active_pr else None,
            "head_sha": active_pr.get("head_sha") if active_pr else None,
            "head_branch": active_pr.get("head_branch") if active_pr else None,
            "base_branch": active_pr.get("base_branch") if active_pr else None,
            "availability": active_pr.get("availability") if active_pr else "unavailable",
        },
        "requested_pr_exact_head": {
            "number": pr_number,
            "head_sha": expected_head_sha,
        },
        "checked_out_sha": checkout.get("head_sha"),
        "expected_head_sha": expected_head_sha,
        "workflow_run_identity": run_identity,
        "source_required_check_matrix": matrix,
        "review_observation": review_observation,
        "unresolved_objection_observation": review_observation[
            "unresolved_objections_state"
        ],
        "review_state_projection": (
            workflow_pr.get("review_state_projection")
            if isinstance(workflow_pr, dict)
            and isinstance(workflow_pr.get("review_state_projection"), dict)
            else _review_state_projection_unavailable("no_workflow_pr_review_state")
        ),
    }

    if documents.get("availability") == "unavailable":
        action = "obtain the accepted-main canonical documents before selecting or advancing work"
    else:
        action = next_permitted_action(packet, active_pr)

    capsule = {
        "schema_version": "project_context.v1",
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "repository": repository,
        "accepted_baseline": baseline,
        "canonical_document_source": {
            "availability": documents.get("availability"),
            "source_sha": documents.get("source_sha"),
        },
        "binding": binding,
        "local_checkout": checkout,
        "active_packet": packet,
        "active_frontier": active_pr,
        "workflow_frontier": workflow_pr,
        "frontier_observation": frontier_observation,
        "blocked_or_other_frontiers": blocked_frontiers,
        "next_permitted_action": action,
        "required_reading": [
            "START_HERE.md",
            "AGENTS.md when implementing or repairing code",
            "docs/CURRENT_STATUS.md from the accepted baseline",
            "docs/NEXT_DECISION.md from the accepted baseline",
            "docs/MODULE_MAP.md for ownership",
            "relevant ARCHITECTURE_BOOK or REAL_WORLD_TESTING_PLAYBOOK sections",
            "relevant code and tests",
        ],
        "hard_stops": [
            "no stale-head CI or review claims",
            "no success when a required CI check is failed, pending, skipped, or missing",
            "no aggregate approval treated as exact-head acceptance",
            "no downstream packet before its prerequisite is accepted",
            "no provider call, merge, release, deploy, or protected-branch write without explicit authority",
            "no caller-asserted authority, secret exposure, invented evidence, or weakened fail-closed behavior",
            "no second runtime, scheduler, store, evaluator, budget, approval, output, audit, or rollback owner",
        ],
        "staleness_conditions": staleness_conditions(),
        "notes": [
            "This capsule is a generated transport view, not an authority owner.",
            "Accepted truth and routing are read from the accepted baseline; live PR, CI, and review state are queried separately.",
            "The canonical active frontier remains packet-routed; an explicit workflow PR is a separate exact-head validation surface.",
            "Unavailable remote facts are reported as unavailable rather than inferred.",
            "The generator is on-demand; CI may publish a short-lived artifact and job summary.",
        ],
    }
    capsule["fingerprint"] = compute_fingerprint(capsule)
    return capsule


def markdown(capsule: dict[str, Any]) -> str:
    baseline = capsule["accepted_baseline"]
    document_source = capsule.get("canonical_document_source", {})
    checkout = capsule.get("local_checkout", {})
    packet = capsule["active_packet"]
    frontier = capsule.get("active_frontier")
    workflow_frontier = capsule.get("workflow_frontier")
    frontier_observation = capsule.get("frontier_observation", {})
    binding = capsule.get("binding", {})
    lines = [
        "# Project Context Capsule",
        "",
        f"- Repository: `{capsule['repository']}`",
        (
            f"- Accepted baseline: `{baseline.get('sha') or 'unavailable'}` "
            f"({baseline.get('availability')}, {baseline.get('source') or 'no source'})"
        ),
        (
            f"- Canonical documents: `{document_source.get('source_sha') or 'unavailable'}` "
            f"availability=`{document_source.get('availability') or 'unavailable'}`"
        ),
        (
            f"- Local checkout: head=`{checkout.get('head_sha') or 'unavailable'}` "
            f"branch=`{checkout.get('branch') or 'detached'}` dirty=`{checkout.get('dirty')}`"
        ),
        (
            f"- Active packet: `{packet.get('packet') or 'unavailable'}` "
            f"state=`{packet.get('state') or 'unavailable'}`"
        ),
        (
            f"- Live frontier observation: "
            f"availability=`{frontier_observation.get('availability') or 'unavailable'}` "
            f"binding=`{frontier_observation.get('binding') or 'none'}` "
            f"warning=`{frontier_observation.get('warning') or 'none'}`"
        ),
    ]
    if frontier:
        ci = frontier.get("ci", {})
        exact_review = frontier.get("exact_head_review", {})
        lines.extend(
            [
                (
                    f"- Active PR: `#{frontier.get('number')}` "
                    f"head=`{frontier.get('head_sha') or 'unavailable'}` "
                    f"availability=`{frontier.get('availability')}`"
                ),
                (
                    f"- CI: `{ci.get('state', 'unavailable')}`; "
                    f"missing_required=`{','.join(ci.get('missing_required') or []) or 'none'}`"
                ),
                (
                    f"- Review: aggregate=`{frontier.get('review_decision') or 'unavailable'}`; "
                    f"exact_head=`{exact_review.get('state') or 'unavailable'}`"
                ),
            ]
        )
    else:
        lines.append("- Active PR: `unavailable`")

    if (
        workflow_frontier
        and (
            not frontier
            or workflow_frontier.get("number") != frontier.get("number")
        )
    ):
        workflow_ci = workflow_frontier.get("ci", {})
        lines.extend(
            [
                (
                    f"- Workflow PR: `#{workflow_frontier.get('number')}` "
                    f"head=`{workflow_frontier.get('head_sha') or 'unavailable'}` "
                    f"availability=`{workflow_frontier.get('availability')}`"
                ),
                (
                    f"- Workflow PR CI: `{workflow_ci.get('state', 'unavailable')}`; "
                    f"missing_required=`{','.join(workflow_ci.get('missing_required') or []) or 'none'}`"
                ),
            ]
        )

    pr_exact = binding.get("pr_exact_head", {})
    run_identity = binding.get("workflow_run_identity", {})
    review_obs = binding.get("review_observation", {})
    review_projection = binding.get("review_state_projection", {})
    projection_line = (
        f"- Review convergence state: `{review_projection.get('review_state') or 'unavailable'}`; "
        f"mode=`{review_projection.get('review_mode') or 'unavailable'}` "
        f"round=`{review_projection.get('review_round') or 'unavailable'}` "
        f"availability=`{review_projection.get('availability') or 'unavailable'}`"
    )
    lines.extend(
        [
            f"- Fingerprint: `{capsule.get('fingerprint') or 'unavailable'}`",
            f"- Workflow run: `{run_identity.get('run_id') or 'unavailable'}` "
            f"(event=`{run_identity.get('event_name') or 'unavailable'}`)",
            f"- Workflow PR exact head binding: `{pr_exact.get('head_sha') or 'unavailable'}`",
            (
                f"- Unresolved objections: "
                f"`{review_obs.get('unresolved_objections_state') or 'unavailable'}`"
            ),
            projection_line,
            f"- Next permitted action: {capsule['next_permitted_action']}",
            "",
            "## Required reading",
        ]
    )
    lines.extend(f"- {item}" for item in capsule["required_reading"])
    lines.extend(["", "## Hard stops"])
    lines.extend(f"- {item}" for item in capsule["hard_stops"])
    lines.extend(["", "## Staleness conditions"])
    lines.extend(f"- {item}" for item in capsule["staleness_conditions"])
    lines.extend(["", "## Source required-check matrix"])
    matrix = binding.get("source_required_check_matrix", [])
    if matrix:
        for item in matrix:
            status = "✓" if item.get("conclusion") == "success" else item.get("conclusion", "?")
            lines.append(
                f"- `{item.get('logical_name')}`: {status} "
                f"(raw: {', '.join(item.get('raw_names') or []) or 'none'})"
            )
    else:
        lines.append("- unavailable")
    lines.extend(["", "## Other live-observed frontiers"])
    if capsule["blocked_or_other_frontiers"]:
        for item in capsule["blocked_or_other_frontiers"]:
            lines.append(
                f"- PR #{item['pr']}: {item.get('purpose') or 'untitled'} — "
                f"head=`{item.get('head_sha') or 'unavailable'}` "
                f"draft=`{item.get('draft')}`"
            )
    else:
        lines.append("- None observed, or live observation unavailable.")
    lines.extend(["", "*["])
    lines.append(f"schema_version: {capsule['schema_version']}")
    lines.append(f"generated_at: {capsule['generated_at']}")
    lines.append("*]")
    lines.extend(["", *[f"> {note}" for note in capsule["notes"]]])
    return "\n".join(lines) + "\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--format", choices=("markdown", "json"), default="markdown")
    parser.add_argument(
        "--offline",
        action="store_true",
        help="Do not contact the remote Git repository or GitHub CLI.",
    )
    parser.add_argument("--repo", help="Override owner/repository for GitHub lookups.")
    parser.add_argument(
        "--checks-json",
        help="GitHub Actions needs JSON to use as the source check matrix.",
    )
    parser.add_argument(
        "--event-name",
        help="GitHub event name (push, pull_request, workflow_dispatch, etc.).",
    )
    parser.add_argument(
        "--pr-number",
        type=int,
        help="Explicit PR number to observe instead of the routed canonical PR.",
    )
    parser.add_argument(
        "--expected-head-sha",
        help="Exact PR head expected by the caller or workflow session.",
    )
    parser.add_argument(
        "--exact-head-proof",
        type=Path,
        help="Trusted exact-head action proof for a PR workflow without exposing a token to rendering.",
    )
    parser.add_argument(
        "--capsule-json",
        type=Path,
        help="Render or validate an existing generated capsule without regenerating it.",
    )
    parser.add_argument(
        "--require-success",
        action="store_true",
        help="Exit non-zero if the source required-check matrix is not fully successful.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    capsule_json = getattr(args, "capsule_json", None)
    if capsule_json:
        try:
            capsule = json.loads(capsule_json.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            print(f"Context capsule snapshot is invalid: {exc}", file=sys.stderr)
            return 1
    else:
        try:
            capsule = build_capsule(
                offline=args.offline,
                repository=args.repo,
                checks_json=args.checks_json,
                event_name=args.event_name,
                pr_number=getattr(args, "pr_number", None),
                expected_head_sha=getattr(args, "expected_head_sha", None),
                exact_head_proof=getattr(args, "exact_head_proof", None),
            )
        except ValueError as exc:
            print(f"Context capsule cannot establish trusted exact-head evidence: {exc}", file=sys.stderr)
            return 1
    if args.format == "json":
        print(json.dumps(capsule, indent=2, sort_keys=True))
    else:
        print(markdown(capsule), end="")
    if args.require_success:
        if not has_valid_success_binding(capsule):
            print("\nSource required-check matrix is not fully successful.", file=sys.stderr)
            return 1
        event_name = ((capsule.get("binding") or {}).get("workflow_run_identity") or {}).get(
            "event_name"
        )
        if event_name == "pull_request" and not is_requested_head_matched(capsule):
            print("\nRequested exact PR head is unavailable or no longer current.", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
