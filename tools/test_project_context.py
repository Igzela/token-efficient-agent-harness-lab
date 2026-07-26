from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "project_context.py"
SPEC = importlib.util.spec_from_file_location("project_context", SCRIPT)
assert SPEC and SPEC.loader
project_context = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = project_context
SPEC.loader.exec_module(project_context)


class ProjectContextTests(unittest.TestCase):
    def test_parse_first_routed_packet_and_pr(self) -> None:
        text = """# Next Decision

## Active Routing

1. `PE7-PRODUCT-GOLDEN-PATH-1` — `IN_PROGRESS`.
2. `PE7-REAL-WORKLOAD-EVIDENCE-1` — `BLOCKED_PREREQUISITE`.

## Packet PE7-PRODUCT-GOLDEN-PATH-1 — authority board

**State:** `IN_PROGRESS`

Current review surface is PR #299.

## Packet PE7-REAL-WORKLOAD-EVIDENCE-1 — baseline

**State:** `BLOCKED_PREREQUISITE`
"""
        self.assertEqual(
            project_context.parse_first_routed_packet(text),
            {
                "packet": "PE7-PRODUCT-GOLDEN-PATH-1",
                "state": "IN_PROGRESS",
                "pr_number": "299",
            },
        )

    def test_structured_owned_pr_wins_over_other_numbers(self) -> None:
        text = """## Active Routing
1. `PE7-AUTHORITY-1` — `IN_PROGRESS`.

## Packet PE7-AUTHORITY-1
**State:** `IN_PROGRESS`
**Owned PR:** #302
Issue #208 remains stopped and PR #299 is a prerequisite.
"""
        self.assertEqual(project_context.parse_first_routed_packet(text)["pr_number"], "302")

    def test_parse_open_frontiers(self) -> None:
        text = """# Current Status

## Open Review Surfaces

| PR | Purpose | Current status |
|---|---|---|
| #299 | Authority | Open; review required |
| #300 | RWE | Blocked on #299 |

## Current Product Verdict
"""
        self.assertEqual(
            project_context.parse_open_frontiers(text),
            [
                {
                    "pr": 299,
                    "purpose": "Authority",
                    "documented_status": "Open; review required",
                },
                {
                    "pr": 300,
                    "purpose": "RWE",
                    "documented_status": "Blocked on #299",
                },
            ],
        )

    def test_summarize_checks_fails_closed(self) -> None:
        summary = project_context.summarize_checks(
            [
                {"name": "rust-tests", "status": "COMPLETED", "conclusion": "SUCCESS"},
                {"name": "pg-tests", "status": "COMPLETED", "conclusion": "FAILURE"},
                {"name": "review", "status": "IN_PROGRESS", "conclusion": None},
            ]
        )
        self.assertEqual(summary["state"], "failed")
        self.assertEqual(summary["successful"], ["rust-tests"])
        self.assertEqual(summary["failed"], ["pg-tests"])
        self.assertEqual(summary["pending"], ["review"])

    def test_success_requires_every_required_check(self) -> None:
        checks = [
            {"name": name, "status": "COMPLETED", "conclusion": "SUCCESS"}
            for name in project_context.REQUIRED_CI_CHECKS
        ]
        summary = project_context.summarize_checks(checks)
        self.assertEqual(summary["state"], "success")
        self.assertEqual(summary["missing_required"], [])

    def test_missing_required_check_is_incomplete(self) -> None:
        checks = [
            {"name": name, "status": "COMPLETED", "conclusion": "SUCCESS"}
            for name in project_context.REQUIRED_CI_CHECKS
            if name != "pg-integration-tests"
        ]
        summary = project_context.summarize_checks(checks)
        self.assertEqual(summary["state"], "incomplete")
        self.assertEqual(summary["missing_required"], ["pg-integration-tests"])

    def test_unavailable_pr_is_not_inferred(self) -> None:
        with mock.patch.object(
            project_context,
            "run_command",
            return_value=project_context.CommandResult(False, "", "gh unavailable", 127),
        ):
            result = project_context.load_pr("owner/repo", 299, offline=False)
        self.assertEqual(result["availability"], "unavailable")
        self.assertIsNone(result["head_sha"])
        self.assertEqual(result["ci"]["state"], "unavailable")
        self.assertEqual(result["unavailable_reason"], "gh_pr_view_failed_exit_127")
        self.assertNotIn("gh unavailable", json.dumps(result))

    def test_offline_baseline_never_uses_detached_head(self) -> None:
        calls: list[tuple[str, ...]] = []

        def fake_run(command: list[str], **_kwargs):
            calls.append(tuple(command))
            if command[-1] == "HEAD":
                return project_context.CommandResult(True, "c" * 40, "", 0)
            return project_context.CommandResult(False, "", "missing", 1)

        with mock.patch.object(project_context, "run_command", side_effect=fake_run):
            baseline = project_context.accepted_baseline(offline=True)
        self.assertEqual(baseline["availability"], "unavailable")
        self.assertNotIn(("git", "rev-parse", "--verify", "HEAD"), calls)

    def test_canonical_documents_never_fall_back_to_worktree(self) -> None:
        baseline = {
            "branch": "main",
            "sha": "b" * 40,
            "availability": "confirmed",
            "source": "remote",
        }
        with (
            mock.patch.object(project_context, "ensure_commit_available", return_value=False),
            mock.patch.object(project_context, "read_text") as read_worktree,
        ):
            documents = project_context.canonical_documents(baseline, offline=True)
        self.assertEqual(documents["availability"], "unavailable")
        read_worktree.assert_not_called()

    def test_next_action_requires_exact_head_review_after_green_ci(self) -> None:
        action = project_context.next_permitted_action(
            {
                "packet": "PE7-PRODUCT-GOLDEN-PATH-1",
                "state": "IN_PROGRESS",
                "pr_number": "299",
            },
            {
                "number": 299,
                "availability": "confirmed",
                "head_sha": "a" * 40,
                "ci": {"state": "success"},
                "review_decision": "APPROVED",
                "exact_head_review": {
                    "state": "unverified",
                    "reason": "aggregate_review_decision_is_not_exact_head_bound",
                },
            },
        )
        self.assertIn("obtain independent acceptance", action)
        self.assertIn("a" * 40, action)
        self.assertIn("unresolved objections", action)

    def test_confirmed_exact_head_review_reaches_merge_authority_gate(self) -> None:
        action = project_context.next_permitted_action(
            {
                "packet": "PE7-PRODUCT-GOLDEN-PATH-1",
                "state": "IN_PROGRESS",
                "pr_number": "299",
            },
            {
                "number": 299,
                "availability": "confirmed",
                "head_sha": "d" * 40,
                "ci": {"state": "success"},
                "exact_head_review": {"state": "confirmed"},
            },
        )
        self.assertIn("confirm explicit merge authority", action)
        self.assertIn("do not merge automatically", action)

    def test_offline_capsule_marks_remote_state(self) -> None:
        next_text = """## Active Routing
1. `PE7-PRODUCT-GOLDEN-PATH-1` — `IN_PROGRESS`.

## Packet PE7-PRODUCT-GOLDEN-PATH-1
**State:** `IN_PROGRESS`
PR #299
"""
        status_text = """## Open Review Surfaces
| PR | Purpose | Current status |
|---|---|---|
| #299 | Authority | Open |
| #300 | RWE | Blocked |
"""
        baseline = {
            "branch": "main",
            "sha": "b" * 40,
            "availability": "local_only",
            "source": "git rev-parse origin/main",
        }
        documents = {
            "availability": "local_only",
            "source_sha": "b" * 40,
            "current_status": status_text,
            "next_decision": next_text,
        }
        with (
            mock.patch.object(project_context, "accepted_baseline", return_value=baseline),
            mock.patch.object(project_context, "canonical_documents", return_value=documents),
            mock.patch.object(
                project_context,
                "local_checkout_state",
                return_value={
                    "head_sha": "c" * 40,
                    "branch": None,
                    "detached": True,
                    "dirty": False,
                    "change_count": 0,
                },
            ),
        ):
            capsule = project_context.build_capsule(
                offline=True, repository="owner/repository"
            )
        self.assertEqual(capsule["active_frontier"]["availability"], "unavailable")
        self.assertEqual(capsule["active_frontier"]["ci"]["state"], "unavailable")
        self.assertEqual(capsule["blocked_or_other_frontiers"][0]["pr"], 300)
        self.assertEqual(capsule["canonical_document_source"]["source_sha"], "b" * 40)
        rendered = project_context.markdown(capsule)
        self.assertIn("unavailable", rendered)
        self.assertNotIn("APPROVED", rendered)

    def test_json_output_is_serializable(self) -> None:
        capsule = {
            "schema_version": "project_context.v1",
            "generated_at": "2026-07-26T00:00:00Z",
            "repository": "owner/repo",
            "accepted_baseline": {
                "branch": "main",
                "sha": None,
                "availability": "unavailable",
                "source": None,
            },
            "canonical_document_source": {
                "availability": "unavailable",
                "source_sha": None,
            },
            "local_checkout": {
                "head_sha": None,
                "branch": None,
                "detached": True,
                "dirty": False,
                "change_count": 0,
            },
            "active_packet": {"packet": None, "state": None, "pr_number": None},
            "active_frontier": None,
            "blocked_or_other_frontiers": [],
            "next_permitted_action": "inspect",
            "required_reading": [],
            "hard_stops": [],
            "notes": [],
        }
        json.dumps(capsule)


if __name__ == "__main__":
    unittest.main()
