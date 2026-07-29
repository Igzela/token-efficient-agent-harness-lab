from __future__ import annotations

from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]
TESTS = ROOT / ".github" / "workflows" / "tests.yml"
FAST = ROOT / ".github" / "workflows" / "pr-fast-checks.yml"


class CiLaneContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.tests_source = TESTS.read_text(encoding="utf-8")
        cls.tests = yaml.safe_load(cls.tests_source)
        cls.fast_source = FAST.read_text(encoding="utf-8")
        cls.fast = yaml.safe_load(cls.fast_source)

    def test_normal_main_push_uses_previous_main_as_trusted_base(self) -> None:
        classifier = self.tests_source[
            self.tests_source.index("  classify-change-impact:\n") :
            self.tests_source.index("  python-tests:\n")
        ]
        self.assertIn("github.event.pull_request.base.sha || github.event.before", classifier)
        self.assertIn("git merge-base --is-ancestor", classifier)
        self.assertIn('git diff --name-only "${BASE_SHA}" "${EXPECTED_SHA}"', classifier)
        self.assertIn("trusted-base/scripts/ci/classify_change_impact.py", classifier)
        self.assertNotIn('if [ "${EVENT_NAME}" != "pull_request" ]; then', classifier)

    def test_uncertain_push_and_dispatch_fail_closed_to_full(self) -> None:
        classifier = self.tests_source[
            self.tests_source.index("  classify-change-impact:\n") :
            self.tests_source.index("  python-tests:\n")
        ]
        for required in (
            'EVENT_NAME}" = "workflow_dispatch',
            'PUSH_FORCED}" = "true',
            '0000000000000000000000000000000000000000',
            'git merge-base --is-ancestor',
            'full_mode',
        ):
            self.assertIn(required, classifier)
        self.assertIn("git diff --raw --no-renames", classifier)
        self.assertIn('allowed_modes = {"000000", "100644"}', classifier)

    def test_required_job_shells_and_terminal_capsule_remain(self) -> None:
        required = {
            "python-tests",
            "rust-tests",
            "pg-integration-tests",
            "typescript-tests",
            "native-runtime",
            "rust-typescript-cutover",
            "docker-build",
        }
        self.assertTrue(required.issubset(self.tests["jobs"]))
        capsule = self.tests["jobs"]["context-capsule"]
        self.assertTrue(required.issubset(set(capsule["needs"])))
        self.assertIn("--require-success", capsule["steps"][-1]["run"])

    def test_pr_fast_checks_enforce_draft_for_every_mutating_event(self) -> None:
        events = self.fast[True]["pull_request"]["types"]
        self.assertEqual(events, ["opened", "synchronize", "reopened", "converted_to_draft"])
        self.assertTrue(self.fast["concurrency"]["cancel-in-progress"])
        self.assertIn("PR_IS_DRAFT", self.fast["env"])
        guard = self.fast["jobs"]["fast-pr-checks"]["steps"][0]
        self.assertEqual(guard["name"], "Enforce Draft lane")
        self.assertIn('PR_IS_DRAFT}" != "true', guard["run"])
        self.assertIn("Convert this PR to Draft", guard["run"])

    def test_canonical_pr_workflow_starts_only_on_ready_transition(self) -> None:
        self.assertEqual(self.tests[True]["pull_request"]["types"], ["ready_for_review"])
        self.assertNotIn("synchronize", self.tests[True]["pull_request"]["types"])
        self.assertIn("tools.test_ci_lane_contract", self.tests_source)
        self.assertIn("tools.test_ci_lane_contract", self.fast_source)


if __name__ == "__main__":
    unittest.main()
