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
AGENT_CONTROL_DIR = SCRIPTS_DIR / "agent-control"
if str(AGENT_CONTROL_DIR) not in sys.path:
    sys.path.insert(0, str(AGENT_CONTROL_DIR))

from github_observer import (  # noqa: E402
    GitHubObservationError,
    GitHubObserver,
    token_from_environment,
)
import review_convergence  # noqa: E402


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_REPOSITORY = "Igzela/token-efficient-agent-harness-lab"
CAPSULE_SCHEMA_VERSION = "project_context.v2"
CANONICAL_DOCUMENT_PATHS = (
    "START_HERE.md",
    "AGENTS.md",
    "README.md",
    "docs/ARCHITECTURE.md",
    "docs/AUTONOMY.md",
    "docs/ROADMAP.md",
    "docs/RUNBOOK.md",
)
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
        "documents": {},
    }
    if not isinstance(sha, str) or not re.fullmatch(r"[0-9a-f]{40}", sha):
        return unavailable
    if not ensure_commit_available(sha, offline=offline):
        return unavailable
    documents: dict[str, str] = {}
    for path in CANONICAL_DOCUMENT_PATHS:
        content = git_show_text(sha, path)
        documents[path] = content
    if any(not content for content in documents.values()):
        return unavailable
    return {
        "availability": baseline.get("availability", "local_only"),
        "source_sha": sha,
        "documents": documents,
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
        "ci": summarize_checks(checks),
    }


REVIEW_RECEIPT_MARKER = review_convergence.REVIEW_RECEIPT_MARKER
BLOCKING_TOKEN = re.compile(r"\bBLOCKING\b", re.IGNORECASE)
NEGATED_BLOCKING_PREFIX = re.compile(r"(?:\bNON|\bNOT)[\s-]+$", re.IGNORECASE)


def _parse_review_receipt(
    comment: dict[str, Any],
    expected_head_sha: str | None,
    expected_base_sha: str | None,
    expected_pr_author_identity: str | None,
) -> dict[str, Any]:
    """Compatibility projection over the canonical review-convergence owner."""
    return review_convergence.observe_exact_head_receipt(
        comment,
        expected_head_sha=expected_head_sha,
        expected_base_sha=expected_base_sha,
        expected_pr_author_identity=expected_pr_author_identity,
    ).to_dict()


def _has_explicit_blocking_semantics(body: object) -> bool:
    """Recognize blocking wording without treating negated notes as blockers."""
    text = str(body or "")
    return any(
        NEGATED_BLOCKING_PREFIX.search(text[:match.start()]) is None
        for match in BLOCKING_TOKEN.finditer(text)
    )

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
    ``docs/AUTONOMY.md``) is the only evidence that a
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
        if _has_explicit_blocking_semantics(review.get("body"))
        or structured_block.search(str(review.get("body") or ""))
    ] + [
        comment
        for comment in comments
        if _has_explicit_blocking_semantics(comment.get("body"))
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
    if not isinstance(capsule, dict) or capsule.get("schema_version") != CAPSULE_SCHEMA_VERSION:
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
    """Fingerprint the observed repository, checkout, PR, and workflow binding."""
    binding = capsule.get("binding", {})
    accepted = binding.get("accepted_baseline", {})
    canonical = binding.get("canonical_document_source", {})
    pr = binding.get("pr_exact_head", {})
    requested = binding.get("requested_pr_exact_head", {})
    run = binding.get("workflow_run_identity", {})
    fields = {
        "repository": capsule.get("repository"),
        "accepted_main_sha": accepted.get("sha"),
        "canonical_document_source_sha": canonical.get("source_sha"),
        "pr_number": pr.get("number"),
        "pr_head_sha": pr.get("head_sha"),
        "requested_pr_number": requested.get("number"),
        "requested_pr_head_sha": requested.get("head_sha"),
        "checked_out_sha": binding.get("checked_out_sha"),
        "expected_head_sha": binding.get("expected_head_sha"),
        "workflow_run_id": run.get("run_id"),
        "workflow_run_attempt": run.get("run_attempt"),
    }
    encoded = json.dumps(fields, sort_keys=True, ensure_ascii=True)
    return hashlib.sha256(encoded.encode("utf-8")).hexdigest()[:24]


def suggested_next_step(current_pr: dict[str, Any] | None) -> str:
    """Summarize current delivery state; never select work or grant authority."""
    if not current_pr:
        return (
            "Continue the current user-requested product or maintenance work "
            "through START_HERE.md; this status capsule does not assign a task."
        )
    number = current_pr.get("number")
    if current_pr.get("availability") != "confirmed":
        return f"Refresh live PR #{number} and its exact head before relying on this projection."
    ci = current_pr.get("ci", {})
    if ci.get("state") == "failed":
        return f"Inspect and repair failed exact-head checks for PR #{number} without weakening guards."
    review = current_pr.get("exact_head_review", {})
    if current_pr.get("draft") is True:
        if review.get("state") != "confirmed":
            return f"Stabilize Draft PR #{number}, verify locally, then request independent exact-head review."
        return f"After review and local verification, use the documented Ready and canonical CI path for PR #{number}."
    if review.get("state") != "confirmed":
        return f"Obtain independent exact-head review for PR #{number}; aggregate approval is not enough."
    if ci.get("state") != "success":
        return f"Complete and refresh the required canonical CI checks for PR #{number}."
    return f"Check live GitHub merge eligibility for PR #{number}; this projection does not authorize a merge."
def build_capsule(
    *,
    offline: bool,
    repository: str | None = None,
    checks_json: str | None = None,
    event_name: str | None = None,
    pr_number: int | None = None,
    expected_head_sha: str | None = None,
    exact_head_proof: Path | None = None,
    observer: GitHubObserver | None = None,
) -> dict[str, Any]:
    """Project accepted documents and the current checkout/PR without assigning work."""
    repository = repository or repository_from_git()
    baseline = accepted_baseline(offline=offline)
    documents = canonical_documents(baseline, offline=offline)
    event_name = event_name or os.environ.get("GITHUB_EVENT_NAME")
    checkout = local_checkout_state()
    remote = observer if observer is not None else (
        None if offline else GitHubObserver(repository, token=token_from_environment())
    )

    observed_pr_number = pr_number
    observation_warning = None
    if (
        observed_pr_number is None
        and event_name not in {"push", "workflow_dispatch"}
        and checkout.get("branch")
        and remote is not None
    ):
        try:
            pulls = remote.list_open_pull_requests(base="main")
            matches = [
                item for item in pulls
                if (item.get("head") or {}).get("ref") == checkout["branch"]
                and isinstance(item.get("number"), int)
            ]
            if len(matches) == 1:
                observed_pr_number = matches[0]["number"]
            elif len(matches) > 1:
                observation_warning = "multiple_open_prs_match_current_branch"
        except (GitHubObservationError, ValueError) as error:
            observation_warning = getattr(error, "reason", "github_observation_invalid")

    current_pr: dict[str, Any] | None = None
    if exact_head_proof is not None:
        if pr_number is None:
            raise ValueError("exact-head proof requires an explicit PR number")
        current_pr = load_exact_head_proof(
            exact_head_proof,
            repository=repository,
            pr_number=pr_number,
            expected_head_sha=expected_head_sha,
        )
    elif observed_pr_number is not None:
        current_pr = load_pr(
            repository,
            observed_pr_number,
            offline=offline,
            observer=remote,
        )

    if expected_head_sha:
        if (
            not re.fullmatch(r"[0-9a-f]{40}", expected_head_sha)
            or checkout.get("head_sha") != expected_head_sha
        ):
            raise ValueError("capsule checkout does not match expected exact head")
    if (
        current_pr
        and expected_head_sha
        and current_pr.get("availability") == "confirmed"
        and current_pr.get("head_sha") != expected_head_sha
    ):
        raise ValueError("current PR head does not match expected exact head")

    checkout["matches_accepted_baseline"] = bool(
        checkout.get("head_sha")
        and baseline.get("sha")
        and checkout.get("head_sha") == baseline.get("sha")
    )
    checkout["matches_current_pr"] = bool(
        checkout.get("head_sha")
        and current_pr
        and current_pr.get("head_sha")
        and checkout.get("head_sha") == current_pr.get("head_sha")
    )

    provided_checks = parse_checks_json(checks_json) if checks_json else []
    if exact_head_proof is not None:
        provided_checks.append(
            {"name": "exact-head-check", "status": "COMPLETED", "conclusion": "SUCCESS"}
        )
    if current_pr and current_pr.get("availability") == "confirmed":
        pr_ci = current_pr.get("ci", {})
        pr_checks = [
            {"name": name, "status": "COMPLETED", "conclusion": outcome.upper()}
            for outcome in ("successful", "failed", "pending")
            for name in (pr_ci.get(outcome) or [])
        ]
        ci_summary = summarize_checks(provided_checks + pr_checks) if provided_checks else pr_ci
    elif provided_checks:
        ci_summary = summarize_checks(provided_checks)
    else:
        ci_summary = {
            "state": "unavailable",
            "successful": [],
            "failed": [],
            "pending": [],
            "missing_required": list(REQUIRED_CI_CHECKS),
        }
    if current_pr is not None:
        current_pr["ci"] = ci_summary

    matrix = source_required_check_matrix(ci_summary, event_name=event_name)
    review_observation = (
        current_pr.get("review_observation")
        if isinstance(current_pr, dict)
        and isinstance(current_pr.get("review_observation"), dict)
        else {
            "observed_head_sha": None,
            "observation_time": None,
            "aggregate_review_state": None,
            "exact_head_review_state": "unavailable",
            "unresolved_objections_state": "unavailable",
            "unavailable_reason": "no_current_pr_review_observation",
        }
    )
    session = session_binding()
    current_pr_binding = {
        "number": current_pr.get("number") if current_pr else None,
        "head_sha": current_pr.get("head_sha") if current_pr else None,
        "head_branch": current_pr.get("head_branch") if current_pr else None,
        "base_branch": current_pr.get("base_branch") if current_pr else None,
        "availability": current_pr.get("availability") if current_pr else "unavailable",
    }
    binding = {
        "accepted_baseline": baseline,
        "canonical_document_source": {
            "availability": documents.get("availability"),
            "source_sha": documents.get("source_sha"),
        },
        "session_binding": session,
        "pr_exact_head": current_pr_binding,
        "requested_pr_exact_head": {
            "number": pr_number,
            "head_sha": expected_head_sha,
        },
        "checked_out_sha": checkout.get("head_sha"),
        "expected_head_sha": expected_head_sha,
        "workflow_run_identity": workflow_run_identity(),
        "source_required_check_matrix": matrix,
        "review_observation": review_observation,
        "unresolved_objection_observation": review_observation[
            "unresolved_objections_state"
        ],
    }
    action = suggested_next_step(current_pr)
    capsule = {
        "schema_version": CAPSULE_SCHEMA_VERSION,
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "repository": repository,
        "accepted_baseline": baseline,
        "canonical_document_source": binding["canonical_document_source"],
        "binding": binding,
        "local_checkout": checkout,
        "current_pr": current_pr,
        "suggested_next_step": action,
        "required_reading": [
            "START_HERE.md",
            "AGENTS.md when implementing or repairing code",
            "docs/ARCHITECTURE.md for architecture and module ownership",
            "docs/AUTONOMY.md for autonomy, testing, and merge rules",
            "relevant code and tests",
        ],
        "hard_stops": [
            "no stale-head CI or review claims",
            "no success when a required CI check is failed, pending, skipped, or missing",
            "no aggregate approval treated as exact-head acceptance",
            "no provider call, merge, release, deploy, or protected-branch write without explicit authority",
            "no caller-asserted authority, secret exposure, invented evidence, or weakened fail-closed behavior",
            "no second runtime, scheduler, store, evaluator, budget, approval, output, audit, or rollback owner",
        ],
        "staleness_conditions": staleness_conditions(),
        "notes": [
            "This capsule is a generated status view, not an authority owner or task assignment.",
            "Accepted canonical documents, current checkout, and live PR/CI/review facts are observed separately.",
            "Unavailable or conflicting remote facts are reported rather than inferred.",
            "The generator is on-demand; CI may publish a short-lived artifact and job summary.",
        ],
    }
    if observation_warning:
        capsule["observation_warning"] = observation_warning
    capsule["fingerprint"] = compute_fingerprint(capsule)
    return capsule



def markdown(capsule: dict[str, Any]) -> str:
    baseline = capsule.get("accepted_baseline", {})
    source = capsule.get("canonical_document_source", {})
    checkout = capsule.get("local_checkout", {})
    current_pr = capsule.get("current_pr")
    binding = capsule.get("binding", {})
    lines = [
        "# Project Context Capsule",
        "",
        f"- Repository: `{capsule.get('repository') or 'unavailable'}`",
        (
            f"- Accepted baseline: `{baseline.get('sha') or 'unavailable'}` "
            f"({baseline.get('availability') or 'unavailable'})"
        ),
        (
            f"- Canonical documents: `{source.get('source_sha') or 'unavailable'}` "
            f"({source.get('availability') or 'unavailable'})"
        ),
        (
            f"- Current checkout: head=`{checkout.get('head_sha') or 'unavailable'}` "
            f"branch=`{checkout.get('branch') or 'detached'}` dirty=`{checkout.get('dirty')}`"
        ),
    ]
    if current_pr:
        ci = current_pr.get("ci", {})
        review = current_pr.get("exact_head_review", {})
        lines.extend(
            [
                (
                    f"- Current branch PR: `#{current_pr.get('number')}` "
                    f"head=`{current_pr.get('head_sha') or 'unavailable'}` "
                    f"availability=`{current_pr.get('availability') or 'unavailable'}`"
                ),
                (
                    f"- PR checks: `{ci.get('state', 'unavailable')}`; "
                    f"missing=`{','.join(ci.get('missing_required') or []) or 'none'}`"
                ),
                (
                    f"- Exact-head review: `{review.get('state', 'unavailable')}`; "
                    f"aggregate=`{current_pr.get('review_decision') or 'unavailable'}`"
                ),
            ]
        )
    else:
        lines.append("- Current branch PR: none observed or remote observation unavailable.")
    review_observation = binding.get("review_observation", {})
    run = binding.get("workflow_run_identity", {})
    lines.extend(
        [
            f"- Workflow run: `{run.get('run_id') or 'unavailable'}` "
            f"(event=`{run.get('event_name') or 'unavailable'}`)",
            (
                f"- Unresolved objections: "
                f"`{review_observation.get('unresolved_objections_state') or 'unavailable'}`"
            ),
            f"- Suggested next step: {capsule.get('suggested_next_step') or 'unavailable'}",
        ]
    )
    if capsule.get("observation_warning"):
        lines.append(f"- Observation warning: `{capsule['observation_warning']}`")
    lines.extend(["", "## Required reading"])
    lines.extend(f"- {item}" for item in capsule.get("required_reading", []))
    lines.extend(["", "## Hard stops"])
    lines.extend(f"- {item}" for item in capsule.get("hard_stops", []))
    lines.extend(["", "## Staleness conditions"])
    lines.extend(f"- {item}" for item in capsule.get("staleness_conditions", []))
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
    lines.extend(
        [
            "",
            "*[",
            f"schema_version: {capsule.get('schema_version', 'unavailable')}",
            f"generated_at: {capsule.get('generated_at', 'unavailable')}",
            f"fingerprint: {capsule.get('fingerprint', 'unavailable')}",
            "*]",
            "",
            "> This generated view is informational; it does not select work or grant authority.",
            "",
        ]
    )
    return "\n".join(lines)



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
        except (ValueError, GitHubObservationError) as exc:
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
