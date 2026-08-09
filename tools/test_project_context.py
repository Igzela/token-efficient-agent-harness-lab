from __future__ import annotations

import argparse
import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
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

    def test_explicit_non_numeric_owner_does_not_fallback_to_history(self) -> None:
        for owner_label in ("Owned PR", "Review surface"):
            with self.subTest(owner_label=owner_label):
                text = f"""## Active Routing
1. `PE7-AUTHORITY-1` — `IN_PROGRESS`.

## Packet PE7-AUTHORITY-1
**State:** `IN_PROGRESS`
**{owner_label}:** TBD
Phase one was accepted through PR #302.
"""
                self.assertIsNone(
                    project_context.parse_first_routed_packet(text)["pr_number"]
                )

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

    def test_live_frontier_prefers_structured_packet_binding(self) -> None:
        observer = mock.Mock()
        observer.list_open_pull_requests.return_value = [
            {
                "number": 370,
                "title": "Refreeze",
                "body": "Packet: `PE7-RWE-V2-REFREEZE-1`",
                "head": {"ref": "unrelated", "sha": "a" * 40},
                "draft": True,
                "html_url": "https://example.invalid/370",
            },
            {
                "number": 225,
                "title": "Dashboard",
                "body": "",
                "head": {"ref": "dashboard", "sha": "b" * 40},
                "draft": False,
                "html_url": "https://example.invalid/225",
            },
        ]
        result = project_context.observe_open_frontiers(
            "owner/repo",
            {
                "packet": "PE7-RWE-V2-REFREEZE-1",
                "state": "READY_FOR_EXECUTION",
                "pr_number": None,
            },
            offline=False,
            observer=observer,
        )
        self.assertEqual(result["active_pr_number"], 370)
        self.assertEqual(result["binding"], "pr_body_packet")
        self.assertEqual(len(result["open_frontiers"]), 2)

    def test_canonical_owned_pr_must_still_be_open_against_main(self) -> None:
        observer = mock.Mock()
        observer.list_open_pull_requests.return_value = [
            {
                "number": 371,
                "title": "Different work",
                "body": "",
                "head": {"ref": "maintenance", "sha": "a" * 40},
                "draft": True,
                "html_url": "https://example.invalid/371",
            }
        ]
        result = project_context.observe_open_frontiers(
            "owner/repo",
            {
                "packet": "PE7-RWE-V2-REFREEZE-1",
                "state": "IN_PROGRESS",
                "pr_number": "370",
            },
            offline=False,
            observer=observer,
        )
        self.assertEqual(result["availability"], "conflict")
        self.assertEqual(
            result["warning"], "canonical_owned_pr_is_not_open_against_main"
        )

    def test_live_frontier_supports_exact_legacy_packet_branch(self) -> None:
        observer = mock.Mock()
        observer.list_open_pull_requests.return_value = [
            {
                "number": 370,
                "title": "Refreeze",
                "body": "",
                "head": {
                    "ref": "pe7-rwe-v2-refreeze-1",
                    "sha": "a" * 40,
                },
                "draft": True,
                "html_url": "https://example.invalid/370",
            }
        ]
        result = project_context.observe_open_frontiers(
            "owner/repo",
            {
                "packet": "PE7-RWE-V2-REFREEZE-1",
                "state": "READY_FOR_EXECUTION",
                "pr_number": None,
            },
            offline=False,
            observer=observer,
        )
        self.assertEqual(result["active_pr_number"], 370)
        self.assertEqual(result["binding"], "legacy_exact_packet_branch")
        self.assertEqual(result["warning"], "legacy_exact_packet_branch_binding")

    def test_live_frontier_conflict_fails_closed(self) -> None:
        observer = mock.Mock()
        observer.list_open_pull_requests.return_value = [
            {
                "number": number,
                "title": "Conflicting owner",
                "body": "Packet: PE7-RWE-V2-REFREEZE-1",
                "head": {"ref": f"candidate-{number}", "sha": str(number)[0] * 40},
                "draft": True,
                "html_url": f"https://example.invalid/{number}",
            }
            for number in (370, 371)
        ]
        result = project_context.observe_open_frontiers(
            "owner/repo",
            {
                "packet": "PE7-RWE-V2-REFREEZE-1",
                "state": "READY_FOR_EXECUTION",
                "pr_number": None,
            },
            offline=False,
            observer=observer,
        )
        self.assertEqual(result["availability"], "conflict")
        self.assertIsNone(result["active_pr_number"])

    def test_structured_and_legacy_bindings_cannot_disagree(self) -> None:
        observer = mock.Mock()
        observer.list_open_pull_requests.return_value = [
            {
                "number": 370,
                "title": "Legacy owner",
                "body": "",
                "head": {"ref": "pe7-rwe-v2-refreeze-1", "sha": "a" * 40},
                "draft": True,
                "html_url": "https://example.invalid/370",
            },
            {
                "number": 371,
                "title": "Structured owner",
                "body": "Packet: PE7-RWE-V2-REFREEZE-1",
                "head": {"ref": "candidate", "sha": "b" * 40},
                "draft": True,
                "html_url": "https://example.invalid/371",
            },
        ]
        result = project_context.observe_open_frontiers(
            "owner/repo",
            {
                "packet": "PE7-RWE-V2-REFREEZE-1",
                "state": "READY_FOR_EXECUTION",
                "pr_number": None,
            },
            offline=False,
            observer=observer,
        )
        self.assertEqual(result["availability"], "conflict")
        self.assertEqual(
            result["warning"],
            "structured_and_legacy_packet_bindings_conflict",
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

    def test_source_matrix_excludes_terminal_context_capsule_input(self) -> None:
        checks = [
            {"name": name, "status": "COMPLETED", "conclusion": "SUCCESS"}
            for name in project_context.REQUIRED_SOURCE_CI_CHECKS
        ]
        matrix = project_context.source_required_check_matrix(
            project_context.summarize_checks(checks), event_name="workflow_dispatch"
        )
        self.assertEqual(
            [item["logical_name"] for item in matrix],
            list(project_context.REQUIRED_SOURCE_CI_CHECKS),
        )
        self.assertNotIn("context-capsule", [item["logical_name"] for item in matrix])
        self.assertTrue(project_context.is_matrix_successful(matrix, event_name="workflow_dispatch"))

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
        observer = mock.Mock()
        observer.pull_request.side_effect = project_context.GitHubObservationError(
            "github_transport_unavailable"
        )
        result = project_context.load_pr(
            "owner/repo", 299, offline=False, observer=observer
        )
        self.assertEqual(result["availability"], "unavailable")
        self.assertIsNone(result["head_sha"])
        self.assertEqual(result["ci"]["state"], "unavailable")
        self.assertEqual(result["unavailable_reason"], "github_transport_unavailable")

    def test_closed_or_non_main_pull_request_is_not_an_active_frontier(self) -> None:
        for state, base, reason in (
            ("closed", "main", "github_pull_request_not_open"),
            ("open", "release", "github_pull_request_base_not_main"),
        ):
            with self.subTest(state=state, base=base):
                observer = mock.Mock()
                observer.pull_request.return_value = {
                    "number": 299,
                    "state": state,
                    "head": {"sha": "a" * 40},
                    "base": {"ref": base, "sha": "b" * 40},
                }
                result = project_context.load_pr(
                    "owner/repo", 299, offline=False, observer=observer
                )
                self.assertEqual(result["availability"], "unavailable")
                self.assertEqual(result["unavailable_reason"], reason)
                observer.pull_request_reviews.assert_not_called()

    def test_durable_projection_uses_latest_trusted_state_only(self) -> None:
        head = "a" * 40
        older = {
            "kind": "agent-orchestrator-review-state",
            "version": 3,
            "issue_number": 42,
            "pr_number": 299,
            "head_sha": head,
            "verdict": "PASS",
            "open_blocker_ids": [],
        }
        newer = {
            "kind": "agent-orchestrator-review-state",
            "version": 3,
            "issue_number": 42,
            "pr_number": 299,
            "head_sha": head,
            "verdict": "BLOCKED",
            "open_blocker_ids": ["F-1"],
        }
        observer = mock.Mock()
        observer.issue_comments.return_value = [
            {
                "user": {"login": "github-actions[bot]"},
                "body": json.dumps(older),
            },
            {"user": {"login": "untrusted"}, "body": json.dumps(older)},
            {
                "user": {"login": "github-actions[bot]"},
                "body": json.dumps(newer),
            },
        ]
        projection = project_context._load_review_state_projection(
            "owner/repo",
            {"number": 299, "headRefOid": head, "body": "Closes #42"},
            observer=observer,
        )
        self.assertEqual(projection["availability"], "confirmed")
        self.assertEqual(projection["review_state"], "BLOCKED")
        self.assertEqual(projection["open_blocker_ids"], ["F-1"])

    def test_malformed_latest_trusted_state_is_a_conflict(self) -> None:
        head = "a" * 40
        observer = mock.Mock()
        observer.issue_comments.return_value = [
            {
                "user": {"login": "github-actions[bot]"},
                "body": json.dumps(
                    {
                        "kind": "agent-orchestrator-review-state",
                        "version": 3,
                        "issue_number": 42,
                        "pr_number": 299,
                        "head_sha": head,
                        "verdict": "PASS",
                    }
                ),
            },
            {
                "user": {"login": "github-actions[bot]"},
                "body": '{"kind":"agent-orchestrator-review-state",',
            },
        ]
        projection = project_context._load_review_state_projection(
            "owner/repo",
            {"number": 299, "headRefOid": head, "body": "Closes #42"},
            observer=observer,
        )
        self.assertEqual(projection["availability"], "conflict")
        self.assertEqual(
            projection["unavailable_reason"],
            "latest_durable_review_state_is_malformed",
        )

    def test_load_pr_binds_durable_review_state_to_requested_pr(self) -> None:
        head = "a" * 40
        base = "b" * 40
        observer = mock.Mock()
        observer.pull_request.return_value = {
            "number": 299,
            "state": "open",
            "head": {"sha": head, "ref": "candidate"},
            "base": {"ref": "main", "sha": base},
            "body": "Closes #42",
            "user": {"login": "implementation-agent"},
        }
        observer.pull_request_reviews.return_value = []
        observer.pull_request_comments.return_value = []
        observer.issue_comments.side_effect = lambda number: (
            []
            if number == 299
            else [
                {
                    "user": {"login": "github-actions[bot]"},
                    "body": json.dumps(
                        {
                            "kind": "agent-orchestrator-review-state",
                            "version": 3,
                            "issue_number": 42,
                            "pr_number": 300,
                            "head_sha": head,
                            "verdict": "PASS",
                            "open_blocker_ids": [],
                        }
                    ),
                }
            ]
        )
        observer.check_runs.return_value = []

        result = project_context.load_pr(
            "owner/repo", 299, offline=False, observer=observer
        )

        self.assertEqual(result["review_state_projection"]["availability"], "conflict")
        self.assertEqual(
            result["review_state_projection"]["unavailable_reason"],
            "latest_durable_review_state_binding_mismatch",
        )

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

    def test_draft_routes_to_review_before_canonical_ci(self) -> None:
        action = project_context.next_permitted_action(
            {
                "packet": "PE7-RWE-V2-REFREEZE-1",
                "state": "IN_PROGRESS",
                "pr_number": "370",
            },
            {
                "number": 370,
                "availability": "confirmed",
                "head_sha": "a" * 40,
                "draft": True,
                "ci": {"state": "incomplete", "missing_required": ["rust-tests"]},
                "exact_head_review": {"state": "unverified"},
            },
        )
        self.assertIn("keep it Draft", action)
        self.assertIn("independent exact PASS", action)
        self.assertNotIn("missing required", action)

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
        self.assertEqual(capsule["blocked_or_other_frontiers"], [])
        self.assertEqual(
            capsule["frontier_observation"]["warning"],
            "offline_observation_disabled",
        )
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

    # -----------------------------------------------------------------------
    # Exact-head alias map
    # -----------------------------------------------------------------------

    def test_exact_head_aliases_canonicalize(self) -> None:
        for alias in ("exact-head-check", "exact-head", "exact-head-check / exact-head", "exact-head / exact-head-check"):
            self.assertEqual(
                project_context._canonical_check_name(alias),
                "exact-head-check",
                f"alias {alias!r} should canonicalize to exact-head-check",
            )

    def test_exact_head_near_matches_rejected(self) -> None:
        for near in (
            "exact-head-checks",
            "exact-head-",
            "exact_head",
            "Exact-Head",
            "exact head",
            "head-exact",
            "exact-head-check / exact-head / extra",
        ):
            self.assertIsNone(
                project_context._canonical_check_name(near),
                f"near match {near!r} must not canonicalize",
            )

    def test_duplicate_exact_head_aliases_do_not_count_twice(self) -> None:
        checks = [
            {"name": "exact-head", "status": "COMPLETED", "conclusion": "SUCCESS"},
            {"name": "exact-head-check", "status": "COMPLETED", "conclusion": "SUCCESS"},
            {"name": "rust-tests", "status": "COMPLETED", "conclusion": "SUCCESS"},
        ]
        summary = project_context.summarize_checks(checks)
        self.assertEqual(summary["state"], "incomplete")
        # The successful list preserves raw observed names; duplicates are canonicalized.
        self.assertEqual(summary["successful"], ["exact-head", "exact-head-check", "rust-tests"])
        raw = (summary.get("raw_by_canonical") or {}).get("exact-head-check") or []
        self.assertEqual(sorted(set(raw)), ["exact-head", "exact-head-check"])
        # exact-head-check must not appear in missing_required despite two aliases.
        self.assertNotIn("exact-head-check", summary["missing_required"])

    def test_exact_head_alias_marks_matrix_successful(self) -> None:
        checks = [
            {"name": name, "status": "COMPLETED", "conclusion": "SUCCESS"}
            for name in project_context.REQUIRED_CI_CHECKS
            if name != "exact-head-check"
        ]
        checks.append(
            {
                "name": "exact-head-check / exact-head",
                "status": "COMPLETED",
                "conclusion": "SUCCESS",
            }
        )
        matrix = project_context.source_required_check_matrix(
            project_context.summarize_checks(checks), event_name="pull_request"
        )
        exact = next(item for item in matrix if item["logical_name"] == "exact-head-check")
        self.assertEqual(exact["conclusion"], "success")
        self.assertTrue(project_context.is_matrix_successful(matrix, event_name="push"))

    # -----------------------------------------------------------------------
    # Check-matrix states
    # -----------------------------------------------------------------------

    def test_failure_pending_cancelled_skipped_missing_states(self) -> None:
        checks = [
            {"name": "rust-tests", "status": "COMPLETED", "conclusion": "FAILURE"},
            {"name": "python-tests", "status": "IN_PROGRESS", "conclusion": None},
            {"name": "docker-build", "status": "COMPLETED", "conclusion": "CANCELLED"},
            {"name": "typescript-tests", "status": "COMPLETED", "conclusion": "SKIPPED"},
        ]
        summary = project_context.summarize_checks(checks)
        self.assertEqual(summary["state"], "failed")
        self.assertEqual(summary["failed"], ["docker-build", "rust-tests", "typescript-tests"])
        self.assertEqual(summary["pending"], ["python-tests"])
        matrix = project_context.source_required_check_matrix(summary)
        missing = [item for item in matrix if item["conclusion"] == "missing"]
        self.assertEqual(
            sorted(item["logical_name"] for item in missing),
            [
                "exact-head-check",
                "native-runtime",
                "pg-integration-tests",
                "rust-typescript-cutover",
            ],
        )

    def test_successful_matrix_reports_all_required(self) -> None:
        checks = [
            {"name": name, "status": "COMPLETED", "conclusion": "SUCCESS"}
            for name in project_context.REQUIRED_CI_CHECKS
        ]
        summary = project_context.summarize_checks(checks)
        matrix = project_context.source_required_check_matrix(summary)
        self.assertTrue(all(item["conclusion"] == "success" for item in matrix))
        self.assertEqual(
            [item["logical_name"] for item in matrix],
            list(project_context.REQUIRED_SOURCE_CI_CHECKS),
        )

    def test_conflicting_required_check_outcomes_fail_the_matrix(self) -> None:
        checks = [
            {"name": name, "status": "COMPLETED", "conclusion": "SUCCESS"}
            for name in project_context.REQUIRED_CI_CHECKS
        ]
        checks.extend(
            [
                {"name": "python-tests", "status": "COMPLETED", "conclusion": "FAILURE"},
                {"name": "rust-tests", "status": "IN_PROGRESS", "conclusion": None},
            ]
        )
        matrix = project_context.source_required_check_matrix(
            project_context.summarize_checks(checks), event_name="pull_request"
        )
        conclusions = {item["logical_name"]: item["conclusion"] for item in matrix}
        self.assertEqual(conclusions["python-tests"], "failed")
        self.assertEqual(conclusions["rust-tests"], "pending")
        self.assertFalse(project_context.is_matrix_successful(matrix))

    # -----------------------------------------------------------------------
    # Fingerprint and binding
    # -----------------------------------------------------------------------

    def test_fingerprint_is_stable_and_changes_with_binding(self) -> None:
        base = {
            "schema_version": "project_context.v1",
            "repository": "owner/repo",
            "binding": {
                "accepted_baseline": {"sha": "a" * 40},
                "canonical_document_source": {"source_sha": "b" * 40},
                "canonical_routed_packet": {"packet": "PE7-CONTEXT-CAPSULE-AUTOMATION-1"},
                "pr_exact_head": {"number": 306, "head_sha": "c" * 40},
                "requested_pr_exact_head": {"number": 306, "head_sha": "c" * 40},
                "checked_out_sha": "c" * 40,
                "workflow_run_identity": {"run_id": "123", "run_attempt": "1"},
            },
        }
        fp1 = project_context.compute_fingerprint(base)
        fp2 = project_context.compute_fingerprint(base)
        self.assertEqual(fp1, fp2)
        self.assertEqual(len(fp1), 24)
        base["binding"]["accepted_baseline"]["sha"] = "d" * 40
        fp3 = project_context.compute_fingerprint(base)
        self.assertNotEqual(fp1, fp3)
        base["binding"]["requested_pr_exact_head"]["head_sha"] = "e" * 40
        fp4 = project_context.compute_fingerprint(base)
        self.assertNotEqual(fp3, fp4)

    def test_fingerprint_ignores_mutable_fields(self) -> None:
        base = {
            "schema_version": "project_context.v1",
            "repository": "owner/repo",
            "generated_at": "2026-07-26T00:00:00Z",
            "next_permitted_action": "action-a",
            "binding": {
                "accepted_baseline": {"sha": "a" * 40},
                "canonical_document_source": {"source_sha": "b" * 40},
                "canonical_routed_packet": {"packet": "PE7-CONTEXT-CAPSULE-AUTOMATION-1"},
                "pr_exact_head": {"number": 306, "head_sha": "c" * 40},
                "requested_pr_exact_head": {"number": 306, "head_sha": "c" * 40},
                "checked_out_sha": "c" * 40,
                "workflow_run_identity": {"run_id": "123", "run_attempt": "1"},
            },
        }
        fp1 = project_context.compute_fingerprint(base)
        base["generated_at"] = "2026-07-27T00:00:00Z"
        base["next_permitted_action"] = "action-b"
        fp2 = project_context.compute_fingerprint(base)
        self.assertEqual(fp1, fp2)

    # -----------------------------------------------------------------------
    # Review observation
    # -----------------------------------------------------------------------

    def test_blocking_review_is_observed_unresolved(self) -> None:
        obs = project_context._build_review_observation(
            head_sha="h" * 40,
            aggregate_review="REVIEW_REQUIRED",
            reviews=[{"state": "CHANGES_REQUESTED", "body": "blocking"}],
            comments=[],
            observation_time="2026-07-28T00:00:00Z",
        )
        self.assertEqual(obs["unresolved_objections_state"], "blocking_reviews_present")

    def test_explicit_blocking_comment_is_observed(self) -> None:
        obs = project_context._build_review_observation(
            head_sha="h" * 40,
            aggregate_review="APPROVED",
            reviews=[{"state": "APPROVED", "body": "lgtm"}],
            comments=[{"body": "This is a BLOCKING issue."}],
            observation_time="2026-07-28T00:00:00Z",
        )
        self.assertEqual(obs["unresolved_objections_state"], "explicit_blocking_comments_present")

    def test_lowercase_blocking_comment_is_observed(self) -> None:
        obs = project_context._build_review_observation(
            head_sha="h" * 40,
            aggregate_review="APPROVED",
            reviews=[{"state": "APPROVED", "body": "lgtm"}],
            comments=[{"body": "this is a blocking issue."}],
            observation_time="2026-07-28T00:00:00Z",
        )
        self.assertEqual(obs["unresolved_objections_state"], "explicit_blocking_comments_present")

    def test_valid_structured_receipt_confirms_exact_head(self) -> None:
        head = "a" * 40
        base = "b" * 40
        body = """EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: independent-session-1
Reviewer authenticated identity: reviewer
Review transport: direct-github-reviewer
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Unresolved objections: none
""".format(head=head, base=base)
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=[{"author": {"login": "reviewer"}, "body": body}],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertEqual(observation["review_receipt"]["state"], "valid")
        self.assertEqual(observation["exact_head_review_state"], "confirmed")
        self.assertEqual(observation["unresolved_objections_state"], "none_observed")

    def test_duplicate_outcome_field_invalidates_receipt(self) -> None:
        head = "a" * 40
        base = "b" * 40
        body = f"""EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: independent-session-1
Reviewer authenticated identity: reviewer
Review transport: direct-github-reviewer
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Outcome: BLOCKED
Unresolved objections: none
"""
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=[{"user": {"login": "reviewer"}, "body": body}],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertEqual(observation["review_receipt"]["state"], "invalid")
        self.assertIn(
            "review_field_duplicated:outcome",
            observation["review_receipt"]["errors"],
        )
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_conflicting_current_head_receipts_never_confirm(self) -> None:
        head = "a" * 40
        base = "b" * 40
        template = """EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: independent-session-{suffix}
Reviewer authenticated identity: reviewer
Review transport: direct-github-reviewer
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: {outcome}
Unresolved objections: none
"""
        comments = [
            {
                "user": {"login": "reviewer"},
                "body": template.format(
                    head=head, base=base, suffix="blocked", outcome="BLOCKED"
                ),
            },
            {
                "user": {"login": "reviewer"},
                "body": template.format(
                    head=head, base=base, suffix="pass", outcome="PASS"
                ),
            },
        ]
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=comments,
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertEqual(
            observation["review_receipt"]["errors"],
            ["multiple_current_head_review_receipts"],
        )
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_structured_open_blocker_never_confirms_receipt(self) -> None:
        head = "a" * 40
        base = "b" * 40
        receipt = f"""EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: independent-session-1
Reviewer authenticated identity: reviewer
Review transport: direct-github-reviewer
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Unresolved objections: none
"""
        blocker = json.dumps(
            {
                "findings": [
                    {
                        "id": "F-1",
                        "disposition": "block_current_head",
                        "status": "open",
                    }
                ]
            }
        )
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=[
                {"user": {"login": "reviewer"}, "body": receipt},
                {"user": {"login": "reviewer"}, "body": blocker},
            ],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertEqual(
            observation["unresolved_objections_state"],
            "explicit_blocking_comments_present",
        )
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_durable_review_state_contradiction_revokes_receipt_confirmation(self) -> None:
        observation = {
            "exact_head_review_state": "confirmed",
            "unresolved_objections_state": "none_observed",
            "unavailable_reason": None,
        }
        project_context._reconcile_review_state_projection(
            observation,
            {
                "availability": "confirmed",
                "review_state": "BLOCKED",
                "open_blocker_ids": ["F-1"],
            },
        )
        self.assertEqual(observation["exact_head_review_state"], "unverified")
        self.assertEqual(
            observation["unresolved_objections_state"],
            "durable_review_state_has_open_blockers",
        )

    def test_pass_with_notes_is_not_exact_acceptance(self) -> None:
        head = "a" * 40
        base = "b" * 40
        body = f"""EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: independent-session-1
Reviewer authenticated identity: reviewer
Review transport: direct-github-reviewer
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS_WITH_NOTES
Unresolved objections: none
"""
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=[{"author": {"login": "reviewer"}, "body": body}],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertEqual(observation["review_receipt"]["state"], "invalid")
        self.assertIn(
            "review_outcome_is_not_exact_pass",
            observation["review_receipt"]["errors"],
        )
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_receipt_without_current_head_never_confirms(self) -> None:
        head = "a" * 40
        base = "b" * 40
        body = """EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: independent-session-1
Reviewer authenticated identity: reviewer
Review transport: direct-github-reviewer
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Unresolved objections: none
""".format(head=head, base=base)
        observation = project_context._build_review_observation(
            head_sha=None,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=[{"author": {"login": "reviewer"}, "body": body}],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_receipt_bound_to_wrong_base_never_confirms(self) -> None:
        head = "a" * 40
        expected_base = "b" * 40
        wrong_base = "c" * 40
        body = """EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {wrong_base}...{head}
Reviewer session identity: independent-session-1
Reviewer authenticated identity: reviewer
Review transport: direct-github-reviewer
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Unresolved objections: none
""".format(head=head, wrong_base=wrong_base)
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=expected_base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=[{"author": {"login": "reviewer"}, "body": body}],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_self_review_identity_never_confirms(self) -> None:
        head = "a" * 40
        base = "b" * 40
        body = """EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: self-review
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Unresolved objections: none
""".format(head=head, base=base)
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=[{"author": {"login": "reviewer"}, "body": body}],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_receipt_authored_by_pull_request_author_never_confirms(self) -> None:
        head = "a" * 40
        base = "b" * 40
        body = f"""EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: fabricated-independent-session
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Unresolved objections: none
"""
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=[
                {
                    "author": {"login": "implementation-agent"},
                    "body": body,
                }
            ],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_parent_transport_requires_authenticated_independent_session(self) -> None:
        head = "a" * 40
        base = "b" * 40
        body = f"""EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: 019fbc08-e766-7930-ade1-1019221d2d43
Reviewer authenticated identity: implementation-agent
Review transport: parent-posted-on-behalf-of-independent-session
Implementation session identity: parent-implementation-session-338
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Unresolved objections: none
"""
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=[
                {"author": {"login": "implementation-agent"}, "body": body}
            ],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertEqual(observation["exact_head_review_state"], "confirmed")

    def test_receipt_authenticated_reviewer_identity_must_match_comment_author(self) -> None:
        head = "a" * 40
        base = "b" * 40
        body = f"""EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: independent-session-1
Reviewer authenticated identity: another-reviewer
Review transport: direct-github-reviewer
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Unresolved objections: none
"""
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=[{"author": {"login": "reviewer"}, "body": body}],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_blocking_review_body_never_confirms_receipt(self) -> None:
        head = "a" * 40
        base = "b" * 40
        body = f"""EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: independent-session-1
Reviewer authenticated identity: reviewer
Review transport: direct-github-reviewer
Observed at: 2026-08-01T06:00:00Z
Axes: architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Unresolved objections: none
"""
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[
                {
                    "state": "COMMENTED",
                    "body": "BLOCKING: unresolved review objection",
                }
            ],
            comments=[{"author": {"login": "reviewer"}, "body": body}],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertEqual(
            observation["unresolved_objections_state"],
            "explicit_blocking_comments_present",
        )
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_receipt_requires_valid_timestamp_and_exact_axis_tokens(self) -> None:
        head = "a" * 40
        base = "b" * 40
        body = f"""EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: independent-session-1
Observed at: yesterday
Axes: not architecture, authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Unresolved objections: none
"""
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=[{"author": {"login": "reviewer"}, "body": body}],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_parent_receipt_rejects_non_uuid_and_embedded_negative_axes(self) -> None:
        head = "a" * 40
        base = "b" * 40
        body = f"""EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Reviewed range: {base}...{head}
Reviewer session identity: fabricated-session
Reviewer authenticated identity: implementation-agent
Review transport: parent-posted-on-behalf-of-independent-session
Implementation session identity: parent-session
Observed at: 2026-08-01T06:00:00Z
Axes: architecture (not reviewed), authority, compatibility, security, audit, rollback, scope/path binding
Outcome: PASS
Unresolved objections: none
"""
        observation = project_context._build_review_observation(
            head_sha=head,
            base_sha=base,
            pr_author_identity="implementation-agent",
            aggregate_review="REVIEW_REQUIRED",
            reviews=[],
            comments=[{"author": {"login": "implementation-agent"}, "body": body}],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_dispatch_success_binding_requires_expected_checkout_identity(self) -> None:
        checks = {
            name: {"result": "success"}
            for name in project_context.REQUIRED_CI_CHECKS
            if name != "exact-head-check"
        }
        sha = "a" * 40
        capsule = {
            "schema_version": "project_context.v1",
            "binding": {
                "workflow_run_identity": {
                    "availability": "confirmed",
                    "event_name": "workflow_dispatch",
                },
                "expected_head_sha": sha,
                "checked_out_sha": "b" * 40,
                "source_required_check_matrix": project_context.source_required_check_matrix(
                    project_context.summarize_checks(
                        project_context.parse_checks_json(json.dumps(checks))
                    ),
                    event_name="workflow_dispatch",
                ),
            },
        }
        with mock.patch.dict(os.environ, {"GITHUB_EVENT_NAME": "workflow_dispatch"}, clear=False):
            self.assertFalse(project_context.has_valid_success_binding(capsule))

    def test_require_success_accepts_a_complete_dispatch_snapshot(self) -> None:
        checks = {
            name: {"result": "success"}
            for name in project_context.REQUIRED_CI_CHECKS
            if name != "exact-head-check"
        }
        sha = "a" * 40
        capsule = {
            "schema_version": "project_context.v1",
            "binding": {
                "workflow_run_identity": {
                    "availability": "confirmed",
                    "event_name": "workflow_dispatch",
                },
                "expected_head_sha": sha,
                "checked_out_sha": sha,
                "source_required_check_matrix": project_context.source_required_check_matrix(
                    project_context.summarize_checks(
                        project_context.parse_checks_json(json.dumps(checks))
                    ),
                    event_name="workflow_dispatch",
                ),
            },
        }
        with tempfile.TemporaryDirectory() as directory:
            snapshot = Path(directory) / "capsule.json"
            snapshot.write_text(json.dumps(capsule), encoding="utf-8")
            with (
                mock.patch.object(
                    project_context,
                    "parse_args",
                    return_value=argparse.Namespace(
                        format="json",
                        capsule_json=snapshot,
                        require_success=True,
                    ),
                ),
                mock.patch.dict(
                    os.environ, {"GITHUB_EVENT_NAME": "workflow_dispatch"}, clear=False
                ),
            ):
                self.assertEqual(project_context.main(), 0)

    def test_dispatch_capsule_generation_rejects_checkout_drift(self) -> None:
        with mock.patch.object(
            project_context,
            "local_checkout_state",
            return_value={"head_sha": "b" * 40, "branch": "main", "dirty": False},
        ):
            with self.assertRaisesRegex(ValueError, "does not match expected exact head"):
                project_context.build_capsule(
                    offline=True,
                    repository="owner/repo",
                    event_name="workflow_dispatch",
                    expected_head_sha="a" * 40,
                )

    def test_failed_or_incomplete_receipt_never_confirms_exact_head(self) -> None:
        head = "c" * 40
        body = f"""EXACT-HEAD REVIEW RECEIPT
Reviewed SHA: {head}
Outcome: FAIL
Unresolved objections: none
"""
        observation = project_context._build_review_observation(
            head_sha=head,
            aggregate_review="APPROVED",
            reviews=[{"state": "APPROVED", "body": "aggregate"}],
            comments=[{"author": {"login": "reviewer"}, "body": body}],
            observation_time="2026-08-01T06:00:00Z",
        )
        self.assertEqual(observation["review_receipt"]["state"], "invalid")
        self.assertNotEqual(observation["exact_head_review_state"], "confirmed")

    def test_new_repair_packet_id_routes_with_owned_pr(self) -> None:
        text = """## Active Routing
1. `CI-EVIDENCE-AND-GOVERNANCE-CLOSEOUT-REPAIR-1` — `IN_PROGRESS`.

## Packet CI-EVIDENCE-AND-GOVERNANCE-CLOSEOUT-REPAIR-1
**State:** `IN_PROGRESS`
**Owned PR:** #338
"""
        self.assertEqual(
            project_context.parse_first_routed_packet(text),
            {
                "packet": "CI-EVIDENCE-AND-GOVERNANCE-CLOSEOUT-REPAIR-1",
                "state": "IN_PROGRESS",
                "pr_number": "338",
            },
        )

    def test_approved_aggregate_not_treated_as_exact_head_acceptance(self) -> None:
        obs = project_context._build_review_observation(
            head_sha="h" * 40,
            aggregate_review="APPROVED",
            reviews=[{"state": "APPROVED", "body": "lgtm"}],
            comments=[],
            observation_time="2026-07-28T00:00:00Z",
        )
        self.assertEqual(obs["unresolved_objections_state"], "none_observed")

    # -----------------------------------------------------------------------
    # Packet state routing
    # -----------------------------------------------------------------------

    def test_complete_packet_routes_to_next_eligible(self) -> None:
        action = project_context.next_permitted_action(
            {"packet": "PE7-CONTEXT-CAPSULE-AUTOMATION-1", "state": "COMPLETE", "pr_number": "306"},
            None,
        )
        self.assertIn("complete", action)
        self.assertIn("next eligible", action)

    def test_blocked_prerequisite_action_forbids_implementation(self) -> None:
        action = project_context.next_permitted_action(
            {"packet": "PE7-PRODUCT-GOLDEN-PATH-1", "state": "BLOCKED_PREREQUISITE", "pr_number": None},
            None,
        )
        self.assertIn("do not implement the blocked packet", action)

    # -----------------------------------------------------------------------
    # Event-aware source matrix and provided checks
    # -----------------------------------------------------------------------

    def test_push_event_marks_exact_head_check_not_applicable(self) -> None:
        checks = {
            "python-tests": {"result": "success"},
            "rust-tests": {"result": "success"},
            "pg-integration-tests": {"result": "success"},
            "typescript-tests": {"result": "success"},
            "native-runtime": {"result": "success"},
            "docker-build": {"result": "success"},
            "rust-typescript-cutover": {"result": "success"},
            "context-capsule": {"result": "success"},
        }
        capsule = project_context.build_capsule(
            offline=True,
            repository="owner/repo",
            checks_json=json.dumps(checks),
            event_name="push",
        )
        matrix = capsule.get("binding", {}).get("source_required_check_matrix", [])
        exact = next(item for item in matrix if item["logical_name"] == "exact-head-check")
        self.assertEqual(exact["conclusion"], "not_applicable")
        self.assertTrue(project_context.is_matrix_successful(matrix, event_name="push"))

    def test_successful_push_without_routed_pr_renders_an_unavailable_review(self) -> None:
        next_text = """## Active Routing
1. `PE7-CONTEXT-CAPSULE-AUTOMATION-1` — `IN_PROGRESS`.

## Packet PE7-CONTEXT-CAPSULE-AUTOMATION-1
**State:** `IN_PROGRESS`
"""
        documents = {
            "availability": "local_only",
            "source_sha": "b" * 40,
            "current_status": "## Open Review Surfaces\n",
            "next_decision": next_text,
        }
        checks = {
            name: {"result": "success"}
            for name in project_context.REQUIRED_CI_CHECKS
            if name != "exact-head-check"
        }
        with (
            mock.patch.object(
                project_context,
                "accepted_baseline",
                return_value={
                    "branch": "main",
                    "sha": "b" * 40,
                    "availability": "local_only",
                    "source": "git rev-parse origin/main",
                },
            ),
            mock.patch.object(project_context, "canonical_documents", return_value=documents),
            mock.patch.object(
                project_context,
                "local_checkout_state",
                return_value={
                    "head_sha": "c" * 40,
                    "branch": "main",
                    "detached": False,
                    "dirty": False,
                    "change_count": 0,
                },
            ),
        ):
            capsule = project_context.build_capsule(
                offline=True,
                repository="owner/repo",
                checks_json=json.dumps(checks),
                event_name="push",
            )
        self.assertIsNone(capsule["active_frontier"])
        self.assertEqual(
            capsule["binding"]["review_observation"]["unresolved_objections_state"],
            "unavailable",
        )
        self.assertTrue(project_context.is_matrix_successful(
            capsule["binding"]["source_required_check_matrix"], event_name="push"
        ))
        rendered = project_context.markdown(capsule)
        self.assertIn("Unresolved objections: `unavailable`", rendered)
        self.assertIn("inspect PE7-CONTEXT-CAPSULE-AUTOMATION-1", rendered)

    def test_push_source_matrix_excludes_canonical_active_pr_checks(self) -> None:
        main_sha = "c" * 40
        documents = {
            "availability": "confirmed",
            "source_sha": main_sha,
            "current_status": "",
            "next_decision": """## Active Routing
1. `PE7-RWE-V2-REFREEZE-1` — `READY_FOR_EXECUTION`.
## Packet PE7-RWE-V2-REFREEZE-1
**State:** `READY_FOR_EXECUTION`
**Owned PR:** #370
""",
        }
        active_pr = {
            "number": 370,
            "availability": "confirmed",
            "head_sha": "a" * 40,
            "head_branch": "pe7-rwe-v2-refreeze-1",
            "base_branch": "main",
            "draft": True,
            "ci": {
                "state": "pending",
                "successful": [],
                "failed": [],
                "pending": ["exact-head"],
                "missing_required": [],
                "raw_by_canonical": {"exact-head-check": ["exact-head"]},
            },
            "exact_head_review": {"state": "unverified"},
        }
        checks = {
            name: {"result": "success"}
            for name in project_context.REQUIRED_CI_CHECKS
            if name != "exact-head-check"
        }
        with (
            mock.patch.object(
                project_context,
                "accepted_baseline",
                return_value={
                    "branch": "main",
                    "sha": main_sha,
                    "availability": "confirmed",
                    "source": "origin/main",
                },
            ),
            mock.patch.object(
                project_context, "canonical_documents", return_value=documents
            ),
            mock.patch.object(project_context, "load_pr", return_value=active_pr),
            mock.patch.object(
                project_context,
                "local_checkout_state",
                return_value={
                    "head_sha": main_sha,
                    "branch": "main",
                    "detached": False,
                    "dirty": False,
                    "change_count": 0,
                },
            ),
        ):
            capsule = project_context.build_capsule(
                offline=True,
                repository="owner/repo",
                checks_json=json.dumps(checks),
                event_name="push",
                expected_head_sha=main_sha,
            )

        self.assertEqual(capsule["active_frontier"]["number"], 370)
        self.assertIsNone(capsule["workflow_frontier"])
        self.assertEqual(capsule["active_frontier"]["ci"]["state"], "pending")
        matrix = capsule["binding"]["source_required_check_matrix"]
        exact = next(item for item in matrix if item["logical_name"] == "exact-head-check")
        self.assertEqual(exact["conclusion"], "not_applicable")
        self.assertTrue(project_context.is_matrix_successful(matrix, event_name="push"))

    def test_pull_request_event_preserves_exact_head_check_required(self) -> None:
        checks = {
            "python-tests": {"result": "success"},
            "rust-tests": {"result": "success"},
            "pg-integration-tests": {"result": "success"},
            "typescript-tests": {"result": "success"},
            "native-runtime": {"result": "success"},
            "docker-build": {"result": "success"},
            "rust-typescript-cutover": {"result": "success"},
        }
        capsule = project_context.build_capsule(
            offline=True,
            repository="owner/repo",
            checks_json=json.dumps(checks),
            event_name="pull_request",
        )
        matrix = capsule.get("binding", {}).get("source_required_check_matrix", [])
        exact = next(item for item in matrix if item["logical_name"] == "exact-head-check")
        self.assertEqual(exact["conclusion"], "missing")
        self.assertFalse(project_context.is_matrix_successful(matrix))

    def test_not_applicable_exact_head_check_is_rejected_for_a_pr_snapshot(self) -> None:
        checks = {
            name: {"result": "success"}
            for name in project_context.REQUIRED_CI_CHECKS
            if name != "exact-head-check"
        }
        matrix = project_context.source_required_check_matrix(
            project_context.summarize_checks(project_context.parse_checks_json(json.dumps(checks))),
            event_name="push",
        )
        sha = "a" * 40
        capsule = {
            "schema_version": "project_context.v1",
            "binding": {
                "workflow_run_identity": {
                    "availability": "confirmed",
                    "event_name": "pull_request",
                },
                "source_required_check_matrix": matrix,
                "requested_pr_exact_head": {"number": 777, "head_sha": sha},
                "pr_exact_head": {"availability": "confirmed", "head_sha": sha},
            },
        }
        with mock.patch.dict(os.environ, {"GITHUB_EVENT_NAME": "pull_request"}, clear=False):
            self.assertFalse(project_context.has_valid_success_binding(capsule))

    def test_complete_push_snapshot_is_accepted(self) -> None:
        checks = {
            name: {"result": "success"}
            for name in project_context.REQUIRED_CI_CHECKS
            if name != "exact-head-check"
        }
        capsule = {
            "schema_version": "project_context.v1",
            "binding": {
                "workflow_run_identity": {
                    "availability": "confirmed",
                    "event_name": "push",
                },
                "expected_head_sha": "a" * 40,
                "checked_out_sha": "a" * 40,
                "source_required_check_matrix": project_context.source_required_check_matrix(
                    project_context.summarize_checks(
                        project_context.parse_checks_json(json.dumps(checks))
                    ),
                    event_name="push",
                ),
            },
        }
        with mock.patch.dict(os.environ, {"GITHUB_EVENT_NAME": "push"}, clear=False):
            self.assertTrue(project_context.has_valid_success_binding(capsule))

    def test_complete_pr_snapshot_requires_and_accepts_trusted_head_binding(self) -> None:
        sha = "a" * 40
        checks = [
            {"name": name, "status": "COMPLETED", "conclusion": "SUCCESS"}
            for name in project_context.REQUIRED_CI_CHECKS
        ]
        capsule = {
            "schema_version": "project_context.v1",
            "binding": {
                "workflow_run_identity": {
                    "availability": "confirmed",
                    "event_name": "pull_request",
                },
                "expected_head_sha": sha,
                "checked_out_sha": sha,
                "source_required_check_matrix": project_context.source_required_check_matrix(
                    project_context.summarize_checks(checks), event_name="pull_request"
                ),
                "requested_pr_exact_head": {"number": 777, "head_sha": sha},
                "pr_exact_head": {"availability": "confirmed", "head_sha": sha},
            },
        }
        with mock.patch.dict(os.environ, {"GITHUB_EVENT_NAME": "pull_request"}, clear=False):
            self.assertTrue(project_context.has_valid_success_binding(capsule))

    def test_pr_success_binding_requires_requested_exact_head(self) -> None:
        checks = [
            {"name": name, "status": "COMPLETED", "conclusion": "SUCCESS"}
            for name in project_context.REQUIRED_CI_CHECKS
        ]
        capsule = {
            "schema_version": "project_context.v1",
            "binding": {
                "workflow_run_identity": {
                    "availability": "confirmed",
                    "event_name": "pull_request",
                },
                "source_required_check_matrix": project_context.source_required_check_matrix(
                    project_context.summarize_checks(checks), event_name="pull_request"
                ),
                "pr_exact_head": {"availability": "confirmed", "head_sha": "a" * 40},
            },
        }
        with mock.patch.dict(os.environ, {"GITHUB_EVENT_NAME": "pull_request"}, clear=False):
            self.assertFalse(project_context.has_valid_success_binding(capsule))

    def test_pull_request_event_accepts_verified_exact_head_check(self) -> None:
        checks = {
            name: {"result": "success"}
            for name in project_context.REQUIRED_CI_CHECKS
        }
        capsule = project_context.build_capsule(
            offline=True,
            repository="owner/repo",
            checks_json=json.dumps(checks),
            event_name="pull_request",
            pr_number=777,
            expected_head_sha="a" * 40,
        )
        matrix = capsule["binding"]["source_required_check_matrix"]
        self.assertTrue(project_context.is_matrix_successful(matrix))
        self.assertEqual(capsule["binding"]["pr_exact_head"]["number"], 777)
        self.assertEqual(
            capsule["binding"]["requested_pr_exact_head"],
            {"number": 777, "head_sha": "a" * 40},
        )

    def test_trusted_exact_head_proof_binds_the_pr_and_matrix(self) -> None:
        checks = {
            name: {"result": "success"}
            for name in project_context.REQUIRED_CI_CHECKS
            if name != "exact-head-check"
        }
        proof = {
            "kind": "exact-head-check-proof.v1",
            "status": "pass",
            "reason": "exact_head_match",
            "repository": "owner/repo",
            "pull_request": 777,
            "expected_head": "a" * 40,
            "live_head": "a" * 40,
            "pr_state": "open",
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "proof.json"
            path.write_text(json.dumps(proof), encoding="utf-8")
            capsule = project_context.build_capsule(
                offline=True,
                repository="owner/repo",
                checks_json=json.dumps(checks),
                event_name="pull_request",
                pr_number=777,
                expected_head_sha="a" * 40,
                exact_head_proof=path,
            )
        self.assertTrue(project_context.is_matrix_successful(
            capsule["binding"]["source_required_check_matrix"]
        ))
        self.assertTrue(project_context.is_requested_head_matched(capsule))

    def test_workflow_pr_does_not_replace_canonical_packet_frontier(self) -> None:
        workflow_head = "a" * 40
        baseline = {
            "branch": "main",
            "sha": "b" * 40,
            "availability": "confirmed",
            "source": "origin/main",
        }
        documents = {
            "availability": "confirmed",
            "source_sha": baseline["sha"],
            "current_status": "",
            "next_decision": """## Active Routing
1. `PE7-RWE-V2-REFREEZE-1` — `READY_FOR_EXECUTION`.
## Packet PE7-RWE-V2-REFREEZE-1
**State:** `READY_FOR_EXECUTION`
**Owned PR:** #370
""",
        }
        proof = {
            "kind": "exact-head-check-proof.v1",
            "status": "pass",
            "reason": "exact_head_match",
            "repository": "owner/repo",
            "pull_request": 371,
            "expected_head": workflow_head,
            "live_head": workflow_head,
            "pr_state": "open",
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "proof.json"
            path.write_text(json.dumps(proof), encoding="utf-8")
            with (
                mock.patch.object(
                    project_context, "accepted_baseline", return_value=baseline
                ),
                mock.patch.object(
                    project_context, "canonical_documents", return_value=documents
                ),
                mock.patch.object(
                    project_context,
                    "local_checkout_state",
                    return_value={
                        "head_sha": workflow_head,
                        "branch": "candidate",
                        "detached": False,
                        "dirty": False,
                        "change_count": 0,
                    },
                ),
            ):
                capsule = project_context.build_capsule(
                    offline=True,
                    repository="owner/repo",
                    event_name="pull_request",
                    pr_number=371,
                    expected_head_sha=workflow_head,
                    exact_head_proof=path,
                )
        self.assertEqual(capsule["active_frontier"]["number"], 370)
        self.assertEqual(capsule["active_frontier"]["availability"], "unavailable")
        self.assertEqual(capsule["workflow_frontier"]["number"], 371)
        self.assertEqual(capsule["workflow_frontier"]["head_sha"], workflow_head)
        self.assertEqual(capsule["binding"]["pr_exact_head"]["number"], 371)
        self.assertEqual(
            capsule["binding"]["canonical_active_pr_exact_head"]["number"], 370
        )
        self.assertIn("refresh PR #370", capsule["next_permitted_action"])
        self.assertNotIn("PR #371", capsule["next_permitted_action"])

    def test_trusted_exact_head_proof_rejects_unconfirmed_or_mismatched_state(self) -> None:
        proof = {
            "kind": "exact-head-check-proof.v1",
            "status": "fail",
            "reason": "head_moved",
            "repository": "owner/repo",
            "pull_request": 777,
            "expected_head": "a" * 40,
            "live_head": "b" * 40,
            "pr_state": "open",
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "proof.json"
            path.write_text(json.dumps(proof), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "does not confirm"):
                project_context.load_exact_head_proof(
                    path,
                    repository="owner/repo",
                    pr_number=777,
                    expected_head_sha="a" * 40,
                )

    def test_requested_head_requires_a_matching_confirmed_pr(self) -> None:
        capsule = {
            "binding": {
                "requested_pr_exact_head": {"head_sha": "a" * 40},
                "pr_exact_head": {
                    "availability": "confirmed",
                    "head_sha": "b" * 40,
                },
            }
        }
        self.assertFalse(project_context.is_requested_head_matched(capsule))
        capsule["binding"]["pr_exact_head"]["head_sha"] = "a" * 40
        self.assertTrue(project_context.is_requested_head_matched(capsule))

    def test_failed_provided_check_marks_matrix_not_successful(self) -> None:
        checks = {
            "python-tests": {"result": "failure"},
            "rust-tests": {"result": "success"},
            "pg-integration-tests": {"result": "success"},
            "typescript-tests": {"result": "success"},
            "native-runtime": {"result": "success"},
            "docker-build": {"result": "success"},
            "rust-typescript-cutover": {"result": "success"},
        }
        capsule = project_context.build_capsule(
            offline=True,
            repository="owner/repo",
            checks_json=json.dumps(checks),
            event_name="push",
        )
        matrix = capsule.get("binding", {}).get("source_required_check_matrix", [])
        self.assertFalse(project_context.is_matrix_successful(matrix))
        python = next(item for item in matrix if item["logical_name"] == "python-tests")
        self.assertEqual(python["conclusion"], "failed")

    def test_require_success_exits_nonzero_on_failed_matrix(self) -> None:
        checks = {
            "python-tests": {"result": "failure"},
            "rust-tests": {"result": "success"},
            "pg-integration-tests": {"result": "success"},
            "typescript-tests": {"result": "success"},
            "native-runtime": {"result": "success"},
            "docker-build": {"result": "success"},
            "rust-typescript-cutover": {"result": "success"},
        }
        with mock.patch.object(
            project_context,
            "parse_args",
            return_value=argparse.Namespace(
                format="json",
                offline=True,
                repo="owner/repo",
                checks_json=json.dumps(checks),
                event_name="push",
                require_success=True,
            ),
        ):
            self.assertEqual(project_context.main(), 1)

    def test_require_success_rejects_a_malformed_snapshot(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            snapshot = Path(directory) / "capsule.json"
            snapshot.write_text(json.dumps({"binding": {}}), encoding="utf-8")
            with mock.patch.object(
                project_context,
                "parse_args",
                return_value=argparse.Namespace(
                    format="json",
                    capsule_json=snapshot,
                    require_success=True,
                ),
            ):
                self.assertEqual(project_context.main(), 1)


if __name__ == "__main__":
    unittest.main()
