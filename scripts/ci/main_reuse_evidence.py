#!/usr/bin/env python3
"""Prove that a main push may reuse an already-accepted PR exact head.

This verifier is run from the trusted pre-push main tree. Any uncertainty exits
non-zero so the workflow falls back to the complete matrix. It never mutates
GitHub or treats reuse as new acceptance evidence.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
from typing import Any


SCRIPTS_DIR = Path(__file__).resolve().parents[1]
if str(SCRIPTS_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPTS_DIR))

from github_observer import (  # noqa: E402
    GitHubObservationError,
    GitHubObserver,
    token_from_environment,
)
import project_context  # noqa: E402


HEX40 = re.compile(r"[0-9a-f]{40}")
SCHEMA_VERSION = "main_ci_reuse.v1"
CANONICAL_WORKFLOW_PATH = ".github/workflows/tests.yml"
REQUIRED_CANONICAL_JOBS = (
    "python-tests",
    "rust-tests",
    "pg-integration-tests",
    "typescript-tests",
    "native-runtime",
    "docker-build",
    "rust-typescript-cutover",
    "context-capsule",
)
EXACT_HEAD_CHECK_NAMES = {
    "exact-head",
    "exact-head-check",
    "exact-head-check / exact-head",
    "exact-head / exact-head-check",
}

# Changes to the mechanism that decides reuse must prove themselves on main.
PROTECTED_PREFIXES = (
    ".github/",
    "actions/exact-head-check/",
    "scripts/agent-control/",
)
PROTECTED_PATHS = {
    "AGENTS.md",
    "START_HERE.md",
    "docs/REAL_WORLD_TESTING_PLAYBOOK.md",
    "scripts/check_agent_handoff.py",
    "scripts/github_observer.py",
    "scripts/project_context.py",
    "scripts/ci/main_reuse_evidence.py",
    "tools/check_security_baseline.py",
    "tools/test_ci_workflow_optimization.py",
    "tools/test_github_observer.py",
    "tools/test_project_context.py",
    "tools/test_workflow_capsule.py",
}


class ReuseEvidenceError(RuntimeError):
    """Trusted equivalence or acceptance evidence is incomplete."""


def _require_sha(value: str, label: str) -> str:
    if not HEX40.fullmatch(value):
        raise ReuseEvidenceError(f"{label}_invalid")
    return value


def _tree_sha(commit: dict[str, Any], label: str) -> str:
    value = ((commit.get("commit") or {}).get("tree") or {}).get("sha")
    if not isinstance(value, str) or not HEX40.fullmatch(value):
        raise ReuseEvidenceError(f"{label}_tree_invalid")
    return value


def _is_protected(path: str) -> bool:
    return path in PROTECTED_PATHS or any(
        path.startswith(prefix) for prefix in PROTECTED_PREFIXES
    )


def changed_paths(repository_root: Path, before: str, after: str) -> list[str]:
    result = subprocess.run(
        [
            "git",
            "-C",
            str(repository_root),
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            before,
            after,
        ],
        capture_output=True,
        check=False,
    )
    if result.returncode != 0:
        raise ReuseEvidenceError("local_exact_diff_unavailable")
    try:
        paths = [item.decode("utf-8") for item in result.stdout.split(b"\0") if item]
    except UnicodeDecodeError as error:
        raise ReuseEvidenceError("local_exact_diff_path_invalid") from error
    if not paths:
        raise ReuseEvidenceError("main_push_diff_empty")
    if any(path.startswith("/") or "\x00" in path for path in paths):
        raise ReuseEvidenceError("local_exact_diff_path_invalid")
    return paths


def _associated_merged_pr(
    observer: GitHubObserver, after: str
) -> dict[str, Any]:
    candidates = [
        item
        for item in observer.commit_pull_requests(after)
        if item.get("merge_commit_sha") == after
        and item.get("merged_at")
        and (item.get("base") or {}).get("ref") == "main"
    ]
    numbers = {
        int(item["number"])
        for item in candidates
        if isinstance(item.get("number"), int)
    }
    if len(numbers) != 1:
        raise ReuseEvidenceError("associated_merged_pr_not_unique")
    return observer.pull_request(next(iter(numbers)))


def _canonical_run(
    observer: GitHubObserver, head_sha: str, pr_number: int
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    runs = [
        run
        for run in observer.workflow_runs(head_sha=head_sha, event="pull_request")
        if run.get("path") == CANONICAL_WORKFLOW_PATH
        and run.get("head_sha") == head_sha
        and any(
            isinstance(linked_pr, dict)
            and linked_pr.get("number") == pr_number
            for linked_pr in (run.get("pull_requests") or [])
        )
        and run.get("status") == "completed"
        and run.get("conclusion") == "success"
    ]
    if not runs:
        raise ReuseEvidenceError("canonical_pr_workflow_success_missing")
    run = max(runs, key=lambda item: int(item.get("id") or 0))
    run_id = run.get("id")
    if not isinstance(run_id, int):
        raise ReuseEvidenceError("canonical_pr_workflow_id_invalid")
    jobs = observer.workflow_jobs(run_id)
    outcomes = {
        name: [
            job
            for job in jobs
            if job.get("name") == name
            and job.get("status") == "completed"
            and job.get("conclusion") == "success"
        ]
        for name in REQUIRED_CANONICAL_JOBS
    }
    missing = sorted(name for name, matches in outcomes.items() if not matches)
    if missing:
        raise ReuseEvidenceError(
            "canonical_pr_jobs_missing_or_unsuccessful:" + ",".join(missing)
        )
    return run, jobs


def _exact_head_check(observer: GitHubObserver, head_sha: str) -> dict[str, Any]:
    checks = [
        check
        for check in observer.check_runs(head_sha)
        if check.get("name") in EXACT_HEAD_CHECK_NAMES
        and check.get("status") == "completed"
        and check.get("conclusion") == "success"
    ]
    if not checks:
        raise ReuseEvidenceError("exact_head_check_success_missing")
    return max(checks, key=lambda item: int(item.get("id") or 0))


def _review_digest(
    observer: GitHubObserver, pr: dict[str, Any], head_sha: str
) -> str:
    number = pr.get("number")
    if not isinstance(number, int):
        raise ReuseEvidenceError("pull_request_number_invalid")
    reviews = observer.pull_request_reviews(number)
    comments = (
        observer.issue_comments(number)
        + observer.pull_request_comments(number)
    )
    states = {str(item.get("state") or "").upper() for item in reviews}
    aggregate = (
        "CHANGES_REQUESTED"
        if "CHANGES_REQUESTED" in states
        else "APPROVED"
        if "APPROVED" in states
        else "REVIEW_REQUIRED"
    )
    observation = project_context._build_review_observation(
        head_sha=head_sha,
        base_sha=(pr.get("base") or {}).get("sha"),
        pr_author_identity=(pr.get("user") or {}).get("login"),
        aggregate_review=aggregate,
        reviews=reviews,
        comments=comments,
        observation_time="main-reuse-verification",
    )
    if observation.get("exact_head_review_state") != "confirmed":
        raise ReuseEvidenceError("exact_pass_review_receipt_missing")
    if observation.get("unresolved_objections_state") != "none_observed":
        raise ReuseEvidenceError("review_objections_not_clear")
    receipt = observation.get("review_receipt") or {}
    bounded = {
        "reviewed_head": receipt.get("observed_head_sha"),
        "reviewed_range": receipt.get("complete_diff_range"),
        "outcome": receipt.get("outcome"),
        "unresolved_objections": receipt.get("unresolved_objections"),
        "axes": receipt.get("axes"),
        "observed_at": receipt.get("observation_time"),
    }
    encoded = json.dumps(bounded, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(encoded.encode("utf-8")).hexdigest()


def build_reuse_receipt(
    observer: GitHubObserver,
    *,
    before: str,
    after: str,
    paths: list[str],
) -> dict[str, Any]:
    _require_sha(before, "before")
    _require_sha(after, "after")
    if before == after:
        raise ReuseEvidenceError("before_after_equal")
    protected = sorted(path for path in paths if _is_protected(path))
    if protected:
        raise ReuseEvidenceError("ci_authority_changed:" + ",".join(protected))

    pr = _associated_merged_pr(observer, after)
    if pr.get("state") != "closed" or not pr.get("merged_at"):
        raise ReuseEvidenceError("associated_pull_request_not_merged")
    if pr.get("merge_commit_sha") != after:
        raise ReuseEvidenceError("associated_pull_request_merge_sha_mismatch")
    if (pr.get("base") or {}).get("ref") != "main":
        raise ReuseEvidenceError("associated_pull_request_base_not_main")
    head_sha = (pr.get("head") or {}).get("sha")
    if not isinstance(head_sha, str):
        raise ReuseEvidenceError("pull_request_head_invalid")
    _require_sha(head_sha, "pull_request_head")

    after_tree = _tree_sha(observer.commit(after), "main_after")
    head_tree = _tree_sha(observer.commit(head_sha), "pull_request_head")
    if after_tree != head_tree:
        raise ReuseEvidenceError("main_and_pull_request_trees_differ")

    number = pr.get("number")
    if not isinstance(number, int):
        raise ReuseEvidenceError("pull_request_number_invalid")
    run, _jobs = _canonical_run(observer, head_sha, number)
    exact_check = _exact_head_check(observer, head_sha)
    review_digest = _review_digest(observer, pr, head_sha)

    return {
        "schema_version": SCHEMA_VERSION,
        "repository": observer.repository,
        "before_sha": before,
        "after_sha": after,
        "tree_sha": after_tree,
        "pull_request": number,
        "pull_request_head_sha": head_sha,
        "canonical_workflow_run_id": run.get("id"),
        "canonical_required_jobs": list(REQUIRED_CANONICAL_JOBS),
        "exact_head_check_id": exact_check.get("id"),
        "review_receipt_sha256": review_digest,
        "changed_path_count": len(paths),
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--before", required=True)
    parser.add_argument("--after", required=True)
    parser.add_argument("--repository-root", type=Path, default=Path.cwd())
    parser.add_argument("--receipt", type=Path, required=True)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        before = _require_sha(args.before, "before")
        after = _require_sha(args.after, "after")
        paths = changed_paths(args.repository_root.resolve(), before, after)
        observer = GitHubObserver(
            args.repository, token=token_from_environment()
        )
        receipt = build_reuse_receipt(
            observer, before=before, after=after, paths=paths
        )
        args.receipt.parent.mkdir(parents=True, exist_ok=True)
        args.receipt.write_text(
            json.dumps(receipt, sort_keys=True, indent=2) + "\n",
            encoding="utf-8",
        )
    except (GitHubObservationError, ReuseEvidenceError, ValueError, OSError) as error:
        reason = getattr(error, "reason", str(error) or "reuse_evidence_unavailable")
        print(f"main CI reuse unavailable: {reason}", file=sys.stderr)
        return 1
    print(
        f"main CI reuse confirmed for PR #{receipt['pull_request']} "
        f"at {receipt['pull_request_head_sha']}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
