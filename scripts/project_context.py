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


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REPOSITORY = "Igzela/token-efficient-agent-harness-lab"
PACKET_ID = r"(?:PE\d+|PR\d+|TOOL)(?:-[A-Z0-9]+)+"

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
    structured_pr = re.search(
        r"^\*\*(?:Owned PR|Review surface):\*\*\s*#(\d+)\s*$",
        block,
        re.MULTILINE | re.IGNORECASE,
    )
    fallback_pr = re.search(r"\bPR #(\d+)\b|(?<!\w)#(\d+)\b", block)
    pr_number = None
    if structured_pr:
        pr_number = structured_pr.group(1)
    elif fallback_pr:
        pr_number = fallback_pr.group(1) or fallback_pr.group(2)
    return {
        "packet": packet,
        "state": state_match.group(1) if state_match else None,
        "pr_number": pr_number,
    }


def parse_open_frontiers(status_text: str) -> list[dict[str, Any]]:
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


def load_pr(repository: str, pr_number: int, *, offline: bool) -> dict[str, Any]:
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
    fields = ",".join(
        [
            "number",
            "title",
            "headRefName",
            "headRefOid",
            "baseRefName",
            "isDraft",
            "mergeStateStatus",
            "reviewDecision",
            "statusCheckRollup",
            "url",
            "reviews",
            "comments",
        ]
    )
    result = run_command(
        ["gh", "pr", "view", str(pr_number), "--repo", repository, "--json", fields],
        timeout=20,
    )
    if not result.ok:
        unavailable["unavailable_reason"] = f"gh_pr_view_failed_exit_{result.returncode}"
        return unavailable
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError:
        unavailable["unavailable_reason"] = "gh_pr_view_invalid_json"
        return unavailable
    aggregate_review = payload.get("reviewDecision") or "REVIEW_REQUIRED"
    head_sha = payload.get("headRefOid")
    observation_time = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    review_observation = _build_review_observation(
        head_sha=head_sha,
        aggregate_review=aggregate_review,
        reviews=payload.get("reviews") or [],
        comments=payload.get("comments") or [],
        observation_time=observation_time,
    )
    return {
        "number": payload.get("number", pr_number),
        "availability": "confirmed",
        "head_sha": head_sha,
        "head_branch": payload.get("headRefName"),
        "base_branch": payload.get("baseRefName"),
        "title": payload.get("title"),
        "url": payload.get("url"),
        "draft": payload.get("isDraft"),
        "merge_state": payload.get("mergeStateStatus"),
        "review_decision": aggregate_review,
        "exact_head_review": {
            "state": "unverified",
            "reason": "aggregate_review_decision_is_not_exact_head_bound",
        },
        "review_observation": review_observation,
        "ci": summarize_checks(payload.get("statusCheckRollup") or []),
    }


def _build_review_observation(
    *,
    head_sha: str | None,
    aggregate_review: str | None,
    reviews: list[dict[str, Any]],
    comments: list[dict[str, Any]],
    observation_time: str,
) -> dict[str, Any]:
    """Build a fail-closed review observation from available GitHub evidence.

    Exact-head acceptance is never inferred from aggregate state or prose.
    """
    observation: dict[str, Any] = {
        "observed_head_sha": head_sha,
        "observation_time": observation_time,
        "aggregate_review_state": aggregate_review,
        "exact_head_review_state": "unverified",
        "unresolved_objections_state": "unavailable",
        "unavailable_reason": None,
    }
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
        return observation

    # Comments from the GitHub REST API do not expose resolved/unresolved state
    # reliably. We only flag explicit BLOCKING mentions so we do not hide them.
    explicit_blocking = [
        comment
        for comment in comments
        if "BLOCKING" in str(comment.get("body") or "")
    ]
    if explicit_blocking:
        observation["unresolved_objections_state"] = "explicit_blocking_comments_present"
        return observation

    if aggregate_review == "APPROVED" and reviews:
        # Aggregate approval exists, but we still do not treat it as exact-head
        # independent acceptance. Mark objections as none observed, not resolved.
        observation["unresolved_objections_state"] = "none_observed"
    else:
        observation["unresolved_objections_state"] = "unavailable"
        observation["unavailable_reason"] = "insufficient_review_evidence"
    return observation


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
    for required in REQUIRED_CI_CHECKS:
        raw_names = raw_by_canonical.get(required) or []
        if required in successful:
            conclusion = "success"
        elif required in failed:
            conclusion = "failed"
        elif required in pending:
            conclusion = "pending"
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


def is_matrix_successful(matrix: list[dict[str, Any]]) -> bool:
    """True only when every required check is success or not_applicable."""
    return all(item.get("conclusion") in {"success", "not_applicable"} for item in matrix)


def is_requested_head_matched(capsule: dict[str, Any]) -> bool:
    """Require a requested exact head to match a confirmed PR observation."""
    binding = capsule.get("binding") or {}
    expected_head = (binding.get("requested_pr_exact_head") or {}).get("head_sha")
    if not expected_head:
        return True
    observed = binding.get("pr_exact_head") or {}
    return (
        observed.get("availability") == "confirmed"
        and observed.get("head_sha") == expected_head
    )


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
        "active PR exact head changes",
        "any required CI check conclusion changes",
        "canonical documents change",
        "review state or unresolved objections change",
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
    requested_pr = binding.get("requested_pr_exact_head", {})
    run = binding.get("workflow_run_identity", {})
    fingerprint_input = {
        "repository": capsule.get("repository"),
        "accepted_main_sha": accepted.get("sha"),
        "canonical_document_source_sha": canonical.get("source_sha"),
        "canonical_routed_packet": routed.get("packet"),
        "pr_number": pr.get("number"),
        "pr_exact_head_sha": pr.get("head_sha"),
        "requested_pr_number": requested_pr.get("number"),
        "requested_pr_exact_head_sha": requested_pr.get("head_sha"),
        "checked_out_sha": binding.get("checked_out_sha"),
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
        return f"inspect {packet_id}, confirm ownership, and create or continue one focused PR"
    number = active_pr.get("number")
    if active_pr.get("availability") != "confirmed":
        return f"refresh PR #{number} exact head, CI, and review state before acting"
    ci = active_pr.get("ci", {})
    ci_state = ci.get("state")
    if ci_state == "failed":
        return f"repair the failing exact-head checks for PR #{number} without weakening guards"
    if ci_state == "incomplete":
        missing = ", ".join(ci.get("missing_required") or [])
        return f"obtain the missing required exact-head checks for PR #{number}: {missing}"
    if ci_state in {"pending", "unavailable"}:
        return f"complete or verify all required exact-head CI for PR #{number}, then obtain independent review"
    exact_review = active_pr.get("exact_head_review", {})
    if exact_review.get("state") != "confirmed":
        return (
            f"obtain independent acceptance for PR #{number} at exact head "
            f"{active_pr.get('head_sha')} and verify unresolved objections"
        )
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
) -> dict[str, Any]:
    repository = repository or repository_from_git()
    baseline = accepted_baseline(offline=offline)
    documents = canonical_documents(baseline, offline=offline)
    next_text = documents.get("next_decision", "")
    status_text = documents.get("current_status", "")
    packet = parse_first_routed_packet(next_text)
    frontiers = parse_open_frontiers(status_text)

    event_name = event_name or os.environ.get("GITHUB_EVENT_NAME")
    provided_checks = parse_checks_json(checks_json) if checks_json else []

    routed_pr_number = int(packet["pr_number"]) if packet.get("pr_number") else None
    target_pr_number = pr_number if pr_number is not None else routed_pr_number
    active_pr = load_pr(repository, target_pr_number, offline=offline) if target_pr_number else None
    blocked_frontiers = [
        frontier
        for frontier in frontiers
        if target_pr_number is None or frontier["pr"] != target_pr_number
    ]
    checkout = local_checkout_state()
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

    if active_pr and active_pr.get("availability") == "confirmed":
        pr_ci_summary = active_pr.get("ci", {})
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
        active_pr["ci"] = ci_summary
    elif provided_checks:
        ci_summary = summarize_checks(provided_checks)
        if active_pr is None:
            active_pr = {
                "number": None,
                "availability": "unavailable",
                "ci": ci_summary,
            }
        else:
            active_pr["ci"] = ci_summary
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

    binding = {
        "accepted_baseline": baseline,
        "canonical_document_source": {
            "availability": documents.get("availability"),
            "source_sha": documents.get("source_sha"),
        },
        "canonical_routed_packet": packet,
        "session_binding": session,
        "pr_exact_head": {
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
        "workflow_run_identity": run_identity,
        "source_required_check_matrix": matrix,
        "review_observation": (
            active_pr.get("review_observation")
            if active_pr
            else {
                "observed_head_sha": None,
                "observation_time": None,
                "aggregate_review_state": None,
                "exact_head_review_state": "unavailable",
                "unresolved_objections_state": "unavailable",
                "unavailable_reason": "no_active_pr",
            }
        ),
        "unresolved_objection_observation": (
            active_pr.get("review_observation", {}).get("unresolved_objections_state")
            if active_pr
            else "unavailable"
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
            "Current status and routing are read from the accepted baseline, not branch-local prose.",
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

    pr_exact = binding.get("pr_exact_head", {})
    run_identity = binding.get("workflow_run_identity", {})
    review_obs = binding.get("review_observation", {})
    lines.extend(
        [
            f"- Fingerprint: `{capsule.get('fingerprint') or 'unavailable'}`",
            f"- Workflow run: `{run_identity.get('run_id') or 'unavailable'}` "
            f"(event=`{run_identity.get('event_name') or 'unavailable'}`)",
            f"- PR exact head binding: `{pr_exact.get('head_sha') or 'unavailable'}`",
            (
                f"- Unresolved objections: "
                f"`{review_obs.get('unresolved_objections_state') or 'unavailable'}`"
            ),
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
    lines.extend(["", "## Other documented frontiers"])
    if capsule["blocked_or_other_frontiers"]:
        for item in capsule["blocked_or_other_frontiers"]:
            lines.append(
                f"- PR #{item['pr']}: {item['purpose']} — {item['documented_status']}"
            )
    else:
        lines.append("- None discovered in accepted `docs/CURRENT_STATUS.md`.")
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
        capsule = build_capsule(
            offline=args.offline,
            repository=args.repo,
            checks_json=args.checks_json,
            event_name=args.event_name,
            pr_number=getattr(args, "pr_number", None),
            expected_head_sha=getattr(args, "expected_head_sha", None),
        )
    if args.format == "json":
        print(json.dumps(capsule, indent=2, sort_keys=True))
    else:
        print(markdown(capsule), end="")
    if args.require_success:
        matrix = capsule.get("binding", {}).get("source_required_check_matrix", [])
        if not is_matrix_successful(matrix):
            print("\nSource required-check matrix is not fully successful.", file=sys.stderr)
            return 1
        if not is_requested_head_matched(capsule):
            print("\nRequested exact PR head is unavailable or no longer current.", file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
