from __future__ import annotations

import argparse
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
        self.assertTrue(project_context.is_matrix_successful(matrix))

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
            ["exact-head-check", "native-runtime", "pg-integration-tests", "rust-typescript-cutover"],
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
            list(project_context.REQUIRED_CI_CHECKS),
        )

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
        self.assertTrue(project_context.is_matrix_successful(matrix))

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


if __name__ == "__main__":
    unittest.main()
