from __future__ import annotations

from pathlib import Path
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / ".github" / "workflows" / "tests.yml"


class CiWorkflowOptimizationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.source = WORKFLOW.read_text(encoding="utf-8")
        cls.parsed = yaml.safe_load(cls.source)

    def job_source(self, name: str) -> str:
        marker = f"  {name}:\n"
        start = self.source.index(marker)
        later = [
            self.source.find(f"  {candidate}:\n", start + len(marker))
            for candidate in self.parsed["jobs"]
            if candidate != name
        ]
        ends = [position for position in later if position >= 0]
        return self.source[start : min(ends) if ends else len(self.source)]

    def test_trusted_base_owns_ready_diff_classification(self) -> None:
        job = self.parsed["jobs"]["classify-change-impact"]
        self.assertEqual(job["outputs"]["docs_only"], "${{ steps.classify.outputs.docs_only }}")
        source = self.job_source("classify-change-impact")
        self.assertIn("path: trusted-base", source)
        self.assertIn("trusted-base/scripts/ci/classify_change_impact.py", source)
        self.assertIn("github.event.pull_request.base.sha", source)
        self.assertIn("git diff --raw --no-renames", source)
        self.assertIn('allowed_modes = {"000000", "100644"}', source)
        self.assertIn('docs_only={str(docs_only).lower()}', source)

    def test_required_jobs_keep_exact_head_and_docs_only_n_a_path(self) -> None:
        required = {
            "docker-build",
            "native-runtime",
            "pg-integration-tests",
            "python-tests",
            "rust-tests",
            "rust-typescript-cutover",
            "typescript-tests",
        }
        for name in required:
            with self.subTest(job=name):
                source = self.job_source(name)
                self.assertIn("needs: classify-change-impact", source)
                self.assertIn("name: Verify exact requested head", source)
                self.assertIn("name: Report documentation-only lane", source)
                self.assertIn("needs.classify-change-impact.outputs.docs_only", source)
        self.assertEqual(self.source.count("name: Verify exact requested head"), 8)
        self.assertEqual(self.source.count("ref: ${{ env.EXPECTED_SHA }}"), 8)

    def test_sccache_is_pinned_and_replaces_target_archive(self) -> None:
        action = "mozilla-actions/sccache-action@9e7fa8a12102821edf02ca5dbea1acd0f89a2696"
        self.assertEqual(self.source.count(action), 4)
        self.assertEqual(self.source.count('version: "v0.15.0"'), 4)
        self.assertEqual(self.source.count('SCCACHE_GHA_ENABLED: "true"'), 4)
        self.assertEqual(self.source.count("RUSTC_WRAPPER: sccache"), 4)
        self.assertEqual(self.source.count("disable_annotations: true"), 4)
        self.assertEqual(self.source.count("continue-on-error: true"), 4)
        self.assertEqual(self.source.count('${SCCACHE_PATH}'), 4)
        self.assertNotIn("Cache Rust target for rust-tests", self.source)
        self.assertNotIn("rust-target-2026-07-10", self.source)

    def test_expensive_steps_are_guarded_and_postgres_is_conditional(self) -> None:
        cheap = {
            "Check out repository",
            "Verify exact requested head",
            "Report documentation-only lane",
        }
        for name in (
            "rust-tests",
            "pg-integration-tests",
            "typescript-tests",
            "native-runtime",
            "rust-typescript-cutover",
            "docker-build",
        ):
            with self.subTest(job=name):
                for step in self.parsed["jobs"][name]["steps"]:
                    if step.get("name") in cheap:
                        continue
                    condition = str(step.get("if", ""))
                    self.assertIn("docs_only != 'true'", condition, step.get("name"))
        pg = self.job_source("pg-integration-tests")
        self.assertNotIn("services:", pg)
        self.assertIn("name: Start PostgreSQL service", pg)
        self.assertIn("name: Stop PostgreSQL service", pg)
        self.assertIn("docker run --detach", pg)

    def test_context_capsule_push_is_not_treated_as_a_pr_head(self) -> None:
        capsule = self.job_source("context-capsule")
        self.assertNotIn("NEEDS_CONTEXT_PATH:", capsule)
        self.assertNotIn("CAPSULE_DIR:", capsule)
        self.assertEqual(capsule.count('--expected-head-sha "${EXPECTED_SHA}"'), 1)
        self.assertGreater(
            capsule.index('--expected-head-sha "${EXPECTED_SHA}"'),
            capsule.index('if [ -n "${GITHUB_PR_NUMBER}" ]'),
        )
        self.assertIn('Path(os.environ["RUNNER_TEMP"]) / "needs-context.json"', capsule)
        self.assertIn('needs_context_path="${RUNNER_TEMP}/needs-context.json"', capsule)
        self.assertIn('capsule_dir="${RUNNER_TEMP}/context-capsule"', capsule)
        self.assertIn('path: ${{ runner.temp }}/context-capsule/', capsule)
        self.assertNotIn('open("needs-context.json"', capsule)
        self.assertNotIn('path: context-capsule/', capsule)

    def test_canonical_identity_and_context_capsule_are_unchanged(self) -> None:
        self.assertIn("name: tests", self.source)
        self.assertIn("types: [ready_for_review]", self.source)
        capsule = self.job_source("context-capsule")
        for required in (
            "python-tests",
            "rust-tests",
            "pg-integration-tests",
            "typescript-tests",
            "native-runtime",
            "docker-build",
            "rust-typescript-cutover",
        ):
            self.assertIn(f"      - {required}", capsule)
        self.assertIn("--require-success", capsule)


if __name__ == "__main__":
    unittest.main()
