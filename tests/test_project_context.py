from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "project_context.py"
SPEC = importlib.util.spec_from_file_location("project_context_repo_smoke", SCRIPT)
assert SPEC and SPEC.loader
project_context = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = project_context
SPEC.loader.exec_module(project_context)


class TestProjectContextRouting(unittest.TestCase):
    def test_offline_capsule_does_not_require_assignment_state(self) -> None:
        capsule = project_context.build_capsule(
            offline=True,
            repository="Igzela/token-efficient-agent-harness-lab",
        )
        self.assertEqual(capsule["schema_version"], project_context.CAPSULE_SCHEMA_VERSION)
        self.assertIsNone(capsule["current_pr"])
        self.assertIn("START_HERE.md", capsule["suggested_next_step"])
        self.assertNotIn("active_mission", capsule)
        self.assertNotIn("execution_authority", capsule)

    def test_capsule_fingerprint_is_stable_for_same_binding(self) -> None:
        capsule = {
            "repository": "owner/repo",
            "binding": {
                "accepted_baseline": {"sha": "a" * 40},
                "canonical_document_source": {"source_sha": "b" * 40},
                "pr_exact_head": {"number": 5, "head_sha": "c" * 40},
                "requested_pr_exact_head": {"number": 5, "head_sha": "c" * 40},
                "checked_out_sha": "c" * 40,
                "expected_head_sha": "c" * 40,
                "workflow_run_identity": {"run_id": "123", "run_attempt": "1"},
            },
        }
        first = project_context.compute_fingerprint(capsule)
        capsule["suggested_next_step"] = "mutable text"
        self.assertEqual(first, project_context.compute_fingerprint(capsule))

    def test_required_checks_are_exact_not_fuzzy(self) -> None:
        matrix = project_context.source_required_check_matrix(
            project_context.summarize_checks(
                [{"name": name, "status": "COMPLETED", "conclusion": "SUCCESS"}
                 for name in project_context.REQUIRED_CI_CHECKS]
            ),
            "pull_request",
        )
        self.assertTrue(project_context.is_matrix_successful(matrix, event_name="pull_request"))
        self.assertFalse(
            project_context.is_matrix_successful(
                [{"logical_name": "python-test", "conclusion": "success",
                  "observed": True, "raw_names": ["python-test"]}],
                event_name="pull_request",
            )
        )


if __name__ == "__main__":
    unittest.main()
