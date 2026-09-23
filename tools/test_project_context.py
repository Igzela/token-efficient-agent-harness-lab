from __future__ import annotations

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "project_context.py"
SPEC = importlib.util.spec_from_file_location("project_context_under_test", SCRIPT)
assert SPEC and SPEC.loader
project_context = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = project_context
SPEC.loader.exec_module(project_context)


def _success_checks(*, include_exact_head: bool = True) -> list[dict[str, str]]:
    names = list(project_context.REQUIRED_SOURCE_CI_CHECKS)
    if include_exact_head and "exact-head-check" not in names:
        names.append("exact-head-check")
    names.append("context-capsule")
    return [
        {"name": name, "status": "COMPLETED", "conclusion": "SUCCESS"}
        for name in names
    ]


def _proof(repository: str, pr_number: int, head: str) -> dict[str, object]:
    return {
        "kind": "exact-head-check-proof.v1",
        "status": "pass",
        "reason": "exact_head_match",
        "repository": repository,
        "pull_request": pr_number,
        "live_head": head,
        "expected_head": head,
        "pr_state": "open",
    }


class ProjectContextTests(unittest.TestCase):
    def test_capsule_is_a_status_view_and_does_not_select_work(self) -> None:
        capsule = project_context.build_capsule(
            offline=True, repository="owner/repo", event_name="push"
        )
        self.assertEqual(capsule["schema_version"], project_context.CAPSULE_SCHEMA_VERSION)
        self.assertIn("current_pr", capsule)
        self.assertIn("suggested_next_step", capsule)
        self.assertIn("START_HERE.md", capsule["suggested_next_step"])
        for removed in (
            "active_mission",
            "steward_continuity",
            "execution_authority",
            "owner_direct_repair",
            "canonical_routed_mission",
        ):
            self.assertNotIn(removed, capsule)
            self.assertNotIn(removed, capsule["binding"])
        self.assertIn("not an authority owner or task assignment", " ".join(capsule["notes"]))

    def test_current_checkout_pr_is_discovered_only_by_exact_branch(self) -> None:
        repository = "owner/repo"
        head = "a" * 40
        observer = mock.Mock()
        observer.list_open_pull_requests.return_value = [
            {
                "number": 42,
                "title": "Current branch",
                "head": {"ref": "agent/current", "sha": head},
                "draft": True,
            },
            {
                "number": 43,
                "title": "Unrelated",
                "head": {"ref": "agent/other", "sha": "b" * 40},
                "draft": False,
            },
        ]
        observer.pull_request.return_value = {
            "number": 42,
            "state": "open",
            "draft": True,
            "base": {"ref": "main", "sha": "c" * 40},
            "head": {"ref": "agent/current", "sha": head},
            "title": "Current branch",
            "html_url": "https://example.invalid/42",
            "user": {"login": "author"},
        }
        observer.pull_request_reviews.return_value = []
        observer.issue_comments.return_value = []
        observer.pull_request_comments.return_value = []
        observer.check_runs.return_value = []

        with (
            mock.patch.object(
                project_context,
                "accepted_baseline",
                return_value={"branch": "main", "sha": "d" * 40, "availability": "confirmed"},
            ),
            mock.patch.object(
                project_context,
                "canonical_documents",
                return_value={"availability": "confirmed", "source_sha": "d" * 40, "documents": {}},
            ),
            mock.patch.object(
                project_context,
                "local_checkout_state",
                return_value={"head_sha": head, "branch": "agent/current", "detached": False, "dirty": False},
            ),
            mock.patch.object(project_context, "workflow_run_identity", return_value={"availability": "unavailable"}),
            mock.patch.object(project_context, "session_binding", return_value={"availability": "unavailable"}),
        ):
            capsule = project_context.build_capsule(
                offline=False, repository=repository, event_name="pull_request", observer=observer
            )
        self.assertEqual(capsule["current_pr"]["number"], 42)
        observer.pull_request.assert_called_once_with(42)

    def test_ambiguous_open_prs_for_checkout_branch_are_not_selected(self) -> None:
        observer = mock.Mock()
        observer.list_open_pull_requests.return_value = [
            {"number": 42, "head": {"ref": "agent/current"}},
            {"number": 43, "head": {"ref": "agent/current"}},
        ]
        with (
            mock.patch.object(
                project_context,
                "accepted_baseline",
                return_value={"branch": "main", "sha": "d" * 40, "availability": "confirmed"},
            ),
            mock.patch.object(
                project_context,
                "canonical_documents",
                return_value={"availability": "confirmed", "source_sha": "d" * 40, "documents": {}},
            ),
            mock.patch.object(
                project_context,
                "local_checkout_state",
                return_value={"head_sha": "a" * 40, "branch": "agent/current", "detached": False, "dirty": False},
            ),
            mock.patch.object(project_context, "workflow_run_identity", return_value={"availability": "unavailable"}),
            mock.patch.object(project_context, "session_binding", return_value={"availability": "unavailable"}),
        ):
            capsule = project_context.build_capsule(
                offline=False, repository="owner/repo", event_name="pull_request", observer=observer
            )
        self.assertIsNone(capsule["current_pr"])
        self.assertEqual(capsule["observation_warning"], "multiple_open_prs_match_current_branch")
        observer.pull_request.assert_not_called()

    def test_explicit_pull_request_must_be_open_based_on_main(self) -> None:
        observer = mock.Mock()
        observer.pull_request.return_value = {
            "number": 42,
            "state": "closed",
            "base": {"ref": "main"},
            "head": {"ref": "agent/current", "sha": "a" * 40},
        }
        pr = project_context.load_pr("owner/repo", 42, offline=False, observer=observer)
        self.assertEqual(pr["availability"], "unavailable")
        self.assertEqual(pr["unavailable_reason"], "github_pull_request_not_open")

    def test_review_aggregate_alone_is_not_exact_head_acceptance(self) -> None:
        observation = project_context._build_review_observation(
            head_sha="a" * 40,
            base_sha="b" * 40,
            pr_author_identity="author",
            aggregate_review="APPROVED",
            reviews=[{"state": "APPROVED", "user": {"login": "reviewer"}, "body": ""}],
            comments=[],
            observation_time="2026-09-23T00:00:00Z",
        )
        self.assertEqual(observation["exact_head_review_state"], "unverified")
        self.assertEqual(observation["unresolved_objections_state"], "none_observed")

    def test_required_check_matrix_rejects_missing_or_failed_checks(self) -> None:
        checks = _success_checks()
        summary = project_context.summarize_checks(checks)
        matrix = project_context.source_required_check_matrix(summary, "pull_request")
        self.assertTrue(project_context.is_matrix_successful(matrix, event_name="pull_request"))
        self.assertEqual(len(matrix), len(project_context.REQUIRED_SOURCE_CI_CHECKS))

        checks[0] = {**checks[0], "conclusion": "FAILURE"}
        failed = project_context.source_required_check_matrix(
            project_context.summarize_checks(checks), "pull_request"
        )
        self.assertFalse(project_context.is_matrix_successful(failed, event_name="pull_request"))
        incomplete_checks = [
            item for item in _success_checks() if item["name"] != "rust-tests"
        ]
        incomplete = project_context.source_required_check_matrix(
            project_context.summarize_checks(incomplete_checks), "pull_request"
        )
        self.assertFalse(project_context.is_matrix_successful(incomplete, event_name="pull_request"))

    def test_success_binding_requires_checkout_and_requested_pr_exact_head(self) -> None:
        head = "a" * 40
        matrix = project_context.source_required_check_matrix(
            project_context.summarize_checks(_success_checks()), "pull_request"
        )
        capsule = {
            "schema_version": project_context.CAPSULE_SCHEMA_VERSION,
            "binding": {
                "workflow_run_identity": {
                    "availability": "confirmed",
                    "event_name": "pull_request",
                },
                "source_required_check_matrix": matrix,
                "expected_head_sha": head,
                "checked_out_sha": head,
                "requested_pr_exact_head": {"number": 42, "head_sha": head},
                "pr_exact_head": {"availability": "confirmed", "head_sha": head},
            },
        }
        with mock.patch.dict("os.environ", {"GITHUB_EVENT_NAME": "pull_request"}):
            self.assertTrue(project_context.has_valid_success_binding(capsule))
            capsule["binding"]["pr_exact_head"]["head_sha"] = "b" * 40
            self.assertFalse(project_context.has_valid_success_binding(capsule))

    def test_exact_head_proof_is_bound_to_repository_pr_and_head(self) -> None:
        head = "a" * 40
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "proof.json"
            path.write_text(json.dumps(_proof("owner/repo", 42, head)), encoding="utf-8")
            observed = project_context.load_exact_head_proof(
                path,
                repository="owner/repo",
                pr_number=42,
                expected_head_sha=head,
            )
            self.assertEqual(observed["number"], 42)
            self.assertEqual(observed["head_sha"], head)
            with self.assertRaisesRegex(ValueError, "does not confirm"):
                project_context.load_exact_head_proof(
                    path,
                    repository="owner/repo",
                    pr_number=42,
                    expected_head_sha="b" * 40,
                )

    def test_capsule_markdown_labels_status_as_informational(self) -> None:
        capsule = project_context.build_capsule(
            offline=True, repository="owner/repo", event_name="push"
        )
        rendered = project_context.markdown(capsule)
        self.assertIn("Suggested next step", rendered)
        self.assertIn("does not select work or grant authority", rendered)
        self.assertNotIn("Mission", rendered)
        self.assertNotIn("WorkCard", rendered)


if __name__ == "__main__":
    unittest.main()
