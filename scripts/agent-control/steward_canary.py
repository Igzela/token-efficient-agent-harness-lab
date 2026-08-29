"""Deterministic provider-free PR4B canary child operations."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys


TARGET = Path("docs/CURRENT_STATUS.md")
ANCHORS = {
    "leaf-a": "The repository owner approved the Autonomous Steward migration direction",
    "leaf-b": "The existing controller remains the only lifecycle writer",
}
RECEIPT_PREFIX = "PR4B canary WorkCard"


def _git(*args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *args],
        capture_output=True,
        text=True,
        timeout=60,
        check=False,
    )
    if check and result.returncode != 0:
        raise RuntimeError("git operation failed")
    return result.stdout.strip()


def _suffix(card_id: str) -> str:
    suffix = card_id.rsplit(":", 1)[-1]
    if suffix not in ANCHORS:
        raise ValueError("canary card is not recognized")
    return suffix


def run_worker(card_id: str, attempt: int, session_id: str) -> int:
    suffix = _suffix(card_id)
    if type(attempt) is not int or attempt != 1:
        raise ValueError("canary attempt is not the initial bounded attempt")
    if not TARGET.is_file() or TARGET.is_symlink():
        raise RuntimeError("canary target is unavailable")
    lines = TARGET.read_text(encoding="utf-8").splitlines(keepends=True)
    anchor = ANCHORS[suffix]
    if sum(anchor in line for line in lines) != 1:
        raise RuntimeError("canary anchor is ambiguous")
    marker = (
        f"- {RECEIPT_PREFIX} `{suffix}` executed by the bounded provider-free runtime; "
        "this is an execution receipt, not final cutover acceptance.\n"
    )
    if marker in lines:
        raise RuntimeError("canary receipt already present")
    index = next(index for index, line in enumerate(lines) if anchor in line)
    lines.insert(index + 1, marker)
    TARGET.write_text("".join(lines), encoding="utf-8")
    _git("add", str(TARGET))
    _git("commit", "-m", f"chore: record PR4B canary {suffix}")
    head = _git("rev-parse", "HEAD")
    changed = _git("diff", "--name-only", "HEAD^", "HEAD").splitlines()
    if changed != [str(TARGET)] or not head:
        raise RuntimeError("canary commit footprint is invalid")
    print(json.dumps({
        "schema_version": "steward_worker_outcome.v1",
        "status": "PASS",
        "session_id": session_id,
        "head_sha": head,
        "changed_paths": [str(TARGET)],
        "detail": f"canary_receipt_{suffix}",
    }, separators=(",", ":")))
    return 0


def run_review(
    card_id: str,
    base_sha: str,
    head_sha: str,
    implementation_session_id: str,
    reviewer_session_id: str,
) -> int:
    suffix = _suffix(card_id)
    if _git("diff", "--check", f"{base_sha}...{head_sha}", check=False):
        status = "FAIL"
        blockers = ["whitespace_error"]
    else:
        changed = _git("diff", "--name-only", f"{base_sha}...{head_sha}").splitlines()
        diff = _git("diff", f"{base_sha}...{head_sha}")
        expected = f"{RECEIPT_PREFIX} `{suffix}`"
        if changed != [str(TARGET)] or expected not in diff:
            status = "FAIL"
            blockers = ["canary_diff_footprint_invalid"]
        else:
            status = "PASS"
            blockers = []
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import steward_workers as workers

    range_sha = workers.review_range_digest(base_sha, head_sha, worktree=Path.cwd())
    payload = {
        "schema_version": "steward_review_outcome.v1",
        "status": status,
        "reviewer_session_id": reviewer_session_id,
        "implementation_session_id": implementation_session_id,
        "reviewed_head_sha": head_sha,
        "blockers": blockers,
        "detail": "deterministic_canary_diff_review",
        "reviewed_base_sha": base_sha,
        "reviewed_range_sha256": range_sha,
        "review_axes": ["standards", "spec"],
        "review_round": 1,
        "review_mode": "full",
        "review_receipt_sha256": "",
        "summary": "bounded independent canary review",
        "findings": None,
        "security_ok": True,
        "rollback_ok": True,
        "observed_ci_status": "unknown",
        "finding_ledger_digest": "",
    }
    print(json.dumps(workers.seal_review_outcome_wire(payload), separators=(",", ":")))
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="operation", required=True)
    worker = subparsers.add_parser("worker")
    worker.add_argument("card_id")
    worker.add_argument("attempt", type=int)
    worker.add_argument("session_id")
    review = subparsers.add_parser("review")
    review.add_argument("card_id")
    review.add_argument("base_sha")
    review.add_argument("head_sha")
    review.add_argument("implementation_session_id")
    review.add_argument("reviewer_session_id")
    args = parser.parse_args(argv)
    if args.operation == "worker":
        return run_worker(args.card_id, args.attempt, args.session_id)
    return run_review(
        args.card_id,
        args.base_sha,
        args.head_sha,
        args.implementation_session_id,
        args.reviewer_session_id,
    )


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as exc:
        print(str(exc), file=sys.stderr)
        raise SystemExit(2)
