from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest
from unittest import mock


SCRIPT = (
    Path(__file__).resolve().parents[1]
    / "scripts"
    / "ci"
    / "main_reuse_evidence.py"
)
SPEC = importlib.util.spec_from_file_location("main_reuse_evidence", SCRIPT)
assert SPEC and SPEC.loader
main_reuse = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = main_reuse
SPEC.loader.exec_module(main_reuse)


BEFORE = "d" * 40
AFTER = "e" * 40
HEAD = "a" * 40
BASE = "b" * 40
TREE = "f" * 40


def review_body(outcome: str = "PASS") -> str:
    return f"""EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {HEAD}
Reviewed range: {BASE}...{HEAD}
Reviewer session identity: independent-session-1
Reviewer authenticated identity: reviewer
Review transport: direct-github-reviewer
Observed at: 2026-08-09T00:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: {outcome}
Unresolved objections: none
"""


def accepted_observer() -> mock.Mock:
    observer = mock.Mock()
    observer.repository = "owner/repository"
    summary = {
        "number": 41,
        "merge_commit_sha": AFTER,
        "merged_at": "2026-08-09T00:10:00Z",
        "base": {"ref": "main"},
    }
    pull = {
        **summary,
        "state": "closed",
        "head": {"sha": HEAD},
        "base": {"ref": "main", "sha": BASE},
        "user": {"login": "implementer"},
    }
    observer.commit_pull_requests.return_value = [summary]
    observer.pull_request.return_value = pull
    observer.commit.side_effect = lambda sha: {
        "sha": sha,
        "commit": {"tree": {"sha": TREE}},
    }
    observer.workflow_runs.return_value = [
        {
            "id": 9001,
            "path": ".github/workflows/tests.yml",
            "head_sha": HEAD,
            "pull_requests": [{"number": 41}],
            "status": "completed",
            "conclusion": "success",
        }
    ]
    observer.workflow_jobs.return_value = [
        {"name": name, "status": "completed", "conclusion": "success"}
        for name in main_reuse.REQUIRED_CANONICAL_JOBS
    ]
    observer.check_runs.return_value = [
        {
            "id": 7001,
            "name": "exact-head",
            "status": "completed",
            "conclusion": "success",
        }
    ]
    observer.pull_request_reviews.return_value = []
    observer.pull_request_comments.return_value = []
    observer.issue_comments.return_value = [
        {"user": {"login": "reviewer"}, "body": review_body()}
    ]
    return observer


class MainReuseEvidenceTests(unittest.TestCase):
    def test_builds_bounded_receipt_for_equivalent_accepted_head(self) -> None:
        receipt = main_reuse.build_reuse_receipt(
            accepted_observer(),
            before=BEFORE,
            after=AFTER,
            paths=["engine/src/lib.rs"],
        )
        self.assertEqual(receipt["schema_version"], "main_ci_reuse.v1")
        self.assertEqual(receipt["tree_sha"], TREE)
        self.assertEqual(receipt["pull_request"], 41)
        self.assertEqual(receipt["pull_request_head_sha"], HEAD)
        self.assertEqual(len(receipt["review_receipt_sha256"]), 64)
        self.assertNotIn("reviewer", str(receipt))

    def test_ci_authority_change_forces_full_matrix(self) -> None:
        with self.assertRaisesRegex(
            main_reuse.ReuseEvidenceError, "ci_authority_changed"
        ):
            main_reuse.build_reuse_receipt(
                accepted_observer(),
                before=BEFORE,
                after=AFTER,
                paths=[".github/workflows/tests.yml"],
            )

    def test_tree_mismatch_forces_full_matrix(self) -> None:
        observer = accepted_observer()
        observer.commit.side_effect = [
            {"commit": {"tree": {"sha": TREE}}},
            {"commit": {"tree": {"sha": "c" * 40}}},
        ]
        with self.assertRaisesRegex(
            main_reuse.ReuseEvidenceError,
            "main_and_pull_request_trees_differ",
        ):
            main_reuse.build_reuse_receipt(
                observer,
                before=BEFORE,
                after=AFTER,
                paths=["engine/src/lib.rs"],
            )

    def test_missing_required_job_forces_full_matrix(self) -> None:
        observer = accepted_observer()
        observer.workflow_jobs.return_value = observer.workflow_jobs.return_value[:-1]
        with self.assertRaisesRegex(
            main_reuse.ReuseEvidenceError,
            "canonical_pr_jobs_missing_or_unsuccessful",
        ):
            main_reuse.build_reuse_receipt(
                observer,
                before=BEFORE,
                after=AFTER,
                paths=["engine/src/lib.rs"],
            )

    def test_workflow_run_for_another_pr_forces_full_matrix(self) -> None:
        observer = accepted_observer()
        observer.workflow_runs.return_value[0]["pull_requests"] = [{"number": 42}]
        with self.assertRaisesRegex(
            main_reuse.ReuseEvidenceError,
            "canonical_pr_workflow_success_missing",
        ):
            main_reuse.build_reuse_receipt(
                observer,
                before=BEFORE,
                after=AFTER,
                paths=["engine/src/lib.rs"],
            )

    def test_pass_with_notes_cannot_authorize_reuse(self) -> None:
        observer = accepted_observer()
        observer.issue_comments.return_value = [
            {"user": {"login": "reviewer"}, "body": review_body("PASS_WITH_NOTES")}
        ]
        with self.assertRaisesRegex(
            main_reuse.ReuseEvidenceError,
            "exact_pass_review_receipt_missing",
        ):
            main_reuse.build_reuse_receipt(
                observer,
                before=BEFORE,
                after=AFTER,
                paths=["engine/src/lib.rs"],
            )

    def test_inline_blocking_comment_prevents_reuse(self) -> None:
        observer = accepted_observer()
        observer.pull_request_comments.return_value = [
            {"user": {"login": "reviewer-two"}, "body": "BLOCKING: unresolved"}
        ]
        with self.assertRaisesRegex(
            main_reuse.ReuseEvidenceError,
            "exact_pass_review_receipt_missing",
        ):
            main_reuse.build_reuse_receipt(
                observer,
                before=BEFORE,
                after=AFTER,
                paths=["engine/src/lib.rs"],
            )


if __name__ == "__main__":
    unittest.main()
