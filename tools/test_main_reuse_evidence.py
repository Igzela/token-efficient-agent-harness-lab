from __future__ import annotations

import hashlib
import importlib.util
import json
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


def durable_review_state(
    verdict: str = "PASS",
    *,
    open_blocker_ids: list[str] | None = None,
    head_sha: str = HEAD,
    issue_number: int = 42,
    pr_number: int = 41,
) -> str:
    findings = [
        {
            "id": blocker_id,
            "axis": "correctness",
            "evidence": blocker_id,
            "severity": "blocker",
            "disposition": "block_current_head",
            "scope_relation": "in_packet",
            "origin_head": head_sha,
            "acceptance_condition": "repair",
            "status": "open",
        }
        for blocker_id in (open_blocker_ids or [])
    ]
    rows = [
        {
            "acceptance_condition": finding["acceptance_condition"],
            "disposition": finding["disposition"],
            "id": finding["id"],
            "origin_head": finding["origin_head"],
            "severity": finding["severity"],
            "status": finding["status"],
        }
        for finding in sorted(findings, key=lambda item: item["id"])
    ]
    finding_ledger_digest = hashlib.sha256(
        json.dumps(
            rows, sort_keys=True, separators=(",", ":"), ensure_ascii=False
        ).encode()
    ).hexdigest()
    base_sha = BASE
    return json.dumps(
        {
            "kind": "agent-orchestrator-review-state",
            "version": 3,
            "issue_number": issue_number,
            "pr_number": pr_number,
            "review_protocol_version": "review-convergence.v1",
            "review_mode": "repair_verification",
            "review_round": 2,
            "prior_reviewed_head": "c" * 40,
            "base_sha": base_sha,
            "head_sha": head_sha,
            "reviewed_range": f"{base_sha}...{head_sha}",
            "findings": findings,
            "finding_ledger_digest": finding_ledger_digest,
            "open_blocker_ids": open_blocker_ids or [],
            "deferred_note_ids": [],
            "decision_required_ids": [],
            "autonomous_repairs_remaining": 0,
            "stop_reason": "" if verdict == "PASS" else "decision_required",
            "artifact_sha256": "",
            "review_workflow_run_id": None,
            "summary": verdict.lower(),
            "blockers": [finding["evidence"] for finding in findings],
            "major_notes": [],
            "minor_notes": [],
            "verdict": verdict,
        },
        sort_keys=True,
    )


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
        "body": "Closes #42",
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
    observer.issue_comments_by_number = {
        41: [{"user": {"login": "reviewer"}, "body": review_body()}],
        42: [
            {
                "user": {"login": "github-actions[bot]"},
                "body": durable_review_state(),
            }
        ],
    }
    observer.issue_comments.side_effect = (
        lambda number: observer.issue_comments_by_number.get(number, [])
    )
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
        self.assertEqual(receipt["linked_issue_numbers"], [42])
        self.assertEqual(len(receipt["linked_review_state_sha256"]), 64)
        self.assertNotIn("reviewer", str(receipt))

    def test_ci_authority_change_forces_full_matrix(self) -> None:
        for path in (
            ".github/workflows/tests.yml",
            "scripts/ci/run_rust_tests.py",
            "scripts/ci/classify_change_impact.py",
            "scripts/ci/future_control.py",
            "scripts/check_future_guard.py",
            "scripts/verify_rust_typescript_stack.sh",
            "scripts/check_wire_codegen_drift.sh",
            "tools/check_future_guard.py",
            "engine/tests/test_http_server.rs",
            "engine/tests/http_server/auth.rs",
            "engine/tests/http_server/common.rs",
        ):
            with self.subTest(path=path), self.assertRaisesRegex(
                main_reuse.ReuseEvidenceError, "ci_authority_changed"
            ):
                main_reuse.build_reuse_receipt(
                    accepted_observer(),
                    before=BEFORE,
                    after=AFTER,
                    paths=[path],
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
            "canonical_pr_workflow_missing",
        ):
            main_reuse.build_reuse_receipt(
                observer,
                before=BEFORE,
                after=AFTER,
                paths=["engine/src/lib.rs"],
            )

    def test_later_failed_or_pending_canonical_run_forces_full_matrix(self) -> None:
        for status, conclusion, expected in (
            ("completed", "failure", "canonical_pr_workflow_latest_not_successful"),
            ("in_progress", None, "canonical_pr_workflow_nonterminal_conflict"),
        ):
            with self.subTest(status=status, conclusion=conclusion):
                observer = accepted_observer()
                observer.workflow_runs.return_value.append(
                    {
                        "id": 9002,
                        "path": ".github/workflows/tests.yml",
                        "head_sha": HEAD,
                        "pull_requests": [{"number": 41}],
                        "status": status,
                        "conclusion": conclusion,
                    }
                )
                with self.assertRaisesRegex(
                    main_reuse.ReuseEvidenceError,
                    expected,
                ):
                    main_reuse.build_reuse_receipt(
                        observer,
                        before=BEFORE,
                        after=AFTER,
                        paths=["engine/src/lib.rs"],
                    )

    def test_duplicate_required_job_forces_full_matrix(self) -> None:
        observer = accepted_observer()
        observer.workflow_jobs.return_value.append(
            {"name": "python-tests", "status": "completed", "conclusion": "success"}
        )
        with self.assertRaisesRegex(
            main_reuse.ReuseEvidenceError,
            "canonical_pr_jobs_missing_or_unsuccessful:python-tests",
        ):
            main_reuse.build_reuse_receipt(
                observer,
                before=BEFORE,
                after=AFTER,
                paths=["engine/src/lib.rs"],
            )

    def test_later_failed_or_pending_exact_head_check_forces_full_matrix(self) -> None:
        for status, conclusion, expected in (
            ("completed", "failure", "exact_head_check_latest_not_successful"),
            ("queued", None, "exact_head_check_nonterminal_conflict"),
        ):
            with self.subTest(status=status, conclusion=conclusion):
                observer = accepted_observer()
                observer.check_runs.return_value.append(
                    {
                        "id": 7002,
                        "name": "exact-head-check",
                        "status": status,
                        "conclusion": conclusion,
                    }
                )
                with self.assertRaisesRegex(
                    main_reuse.ReuseEvidenceError,
                    expected,
                ):
                    main_reuse.build_reuse_receipt(
                        observer,
                        before=BEFORE,
                        after=AFTER,
                        paths=["engine/src/lib.rs"],
                    )

    def test_duplicate_latest_exact_head_check_forces_full_matrix(self) -> None:
        observer = accepted_observer()
        observer.check_runs.return_value.append(
            {
                "id": 7001,
                "name": "exact-head-check",
                "status": "completed",
                "conclusion": "success",
            }
        )
        with self.assertRaisesRegex(
            main_reuse.ReuseEvidenceError,
            "exact_head_check_latest_not_unique",
        ):
            main_reuse.build_reuse_receipt(
                observer,
                before=BEFORE,
                after=AFTER,
                paths=["engine/src/lib.rs"],
            )

    def test_pass_with_notes_cannot_authorize_reuse(self) -> None:
        observer = accepted_observer()
        observer.issue_comments_by_number[41] = [
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

    def test_linked_issue_blocked_review_state_prevents_reuse(self) -> None:
        observer = accepted_observer()
        observer.issue_comments_by_number[42] = [
            {
                "user": {"login": "github-actions[bot]"},
                "body": durable_review_state(
                    "DECISION_REQUIRED", open_blocker_ids=["REVIEW-AUDIT-001"]
                ),
            }
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

    def test_latest_trusted_linked_issue_state_controls_reuse(self) -> None:
        observer = accepted_observer()
        observer.issue_comments_by_number[42] = [
            {
                "user": {"login": "github-actions[bot]"},
                "body": durable_review_state(
                    "DECISION_REQUIRED", open_blocker_ids=["REVIEW-AUDIT-001"]
                ),
            },
            {
                "user": {"login": "github-actions[bot]"},
                "body": durable_review_state(),
            },
        ]
        receipt = main_reuse.build_reuse_receipt(
            observer,
            before=BEFORE,
            after=AFTER,
            paths=["engine/src/lib.rs"],
        )
        self.assertEqual(receipt["linked_issue_numbers"], [42])

    def test_untrusted_pass_cannot_shadow_linked_issue_blocker(self) -> None:
        observer = accepted_observer()
        observer.issue_comments_by_number[42] = [
            {
                "user": {"login": "github-actions[bot]"},
                "body": durable_review_state(
                    "DECISION_REQUIRED", open_blocker_ids=["REVIEW-AUDIT-001"]
                ),
            },
            {"user": {"login": "attacker"}, "body": durable_review_state()},
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

    def test_missing_or_malformed_linked_issue_state_forces_full_matrix(self) -> None:
        for comments, expected in (
            ([], "linked_issue_review_state_unavailable"),
            (
                [
                    {
                        "user": {"login": "github-actions[bot]"},
                        "body": "agent-orchestrator-review-state: malformed",
                    }
                ],
                "linked_issue_review_state_conflict",
            ),
            (
                [
                    {
                        "user": {"login": "github-actions[bot]"},
                        "body": durable_review_state(head_sha="9" * 40),
                    }
                ],
                "linked_issue_review_state_conflict",
            ),
            (
                [
                    {
                        "user": {"login": "github-actions[bot]"},
                        "body": durable_review_state(pr_number=99),
                    }
                ],
                "linked_issue_review_state_conflict",
            ),
        ):
            with self.subTest(expected=expected):
                observer = accepted_observer()
                observer.issue_comments_by_number[42] = comments
                with self.assertRaisesRegex(
                    main_reuse.ReuseEvidenceError,
                    expected,
                ):
                    main_reuse.build_reuse_receipt(
                        observer,
                        before=BEFORE,
                        after=AFTER,
                        paths=["engine/src/lib.rs"],
                    )

    def test_multiple_linked_issues_force_full_matrix(self) -> None:
        observer = accepted_observer()
        observer.pull_request.return_value["body"] = "Closes #42\nFixes #43"
        with self.assertRaisesRegex(
            main_reuse.ReuseEvidenceError,
            "linked_issue_binding_not_unique",
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
