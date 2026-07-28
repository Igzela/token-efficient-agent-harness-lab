"""Tests for context-capsule publisher job in .github/workflows/tests.yml."""

from __future__ import annotations

from pathlib import Path
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW_PATH = ROOT / ".github" / "workflows" / "tests.yml"

REQUIRED_TEST_JOBS = {
    "python-tests",
    "rust-tests",
    "pg-integration-tests",
    "typescript-tests",
    "native-runtime",
    "docker-build",
    "rust-typescript-cutover",
}


def _load_yaml(path: Path) -> dict:
    try:
        import yaml
    except ImportError as exc:
        raise unittest.SkipTest("pyyaml not available") from exc
    return yaml.safe_load(path.read_text(encoding="utf-8"))


class WorkflowCapsuleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        if not WORKFLOW_PATH.exists():
            raise unittest.SkipTest("tests.yml not found")
        cls.workflow = _load_yaml(WORKFLOW_PATH)

    def test_context_capsule_job_exists(self) -> None:
        jobs = self.workflow.get("jobs", {})
        self.assertIn("context-capsule", jobs)
        job = jobs["context-capsule"]
        self.assertEqual(job.get("name"), "context-capsule")

    def test_context_capsule_depends_on_all_test_jobs(self) -> None:
        job = self.workflow["jobs"]["context-capsule"]
        needs = set(job.get("needs") or [])
        self.assertGreaterEqual(needs, REQUIRED_TEST_JOBS)

    def test_context_capsule_uses_always(self) -> None:
        job = self.workflow["jobs"]["context-capsule"]
        self.assertEqual(str(job.get("if")).lower(), "always()")

    def test_context_capsule_minimum_permissions(self) -> None:
        job = self.workflow["jobs"]["context-capsule"]
        perms = job.get("permissions", {})
        self.assertEqual(perms.get("contents"), "read")
        self.assertEqual(perms.get("actions"), "write")
        self.assertEqual(perms.get("pull-requests"), "read")

    def test_context_capsule_checks_out_expected_sha(self) -> None:
        job = self.workflow["jobs"]["context-capsule"]
        steps = job.get("steps", [])
        checkout = next(
            (s for s in steps if s.get("name") == "Check out repository"), None
        )
        self.assertIsNotNone(checkout)
        self.assertEqual(checkout.get("with", {}).get("ref"), "${{ env.EXPECTED_SHA }}")
        verify = next(
            (s for s in steps if s.get("name") == "Verify exact requested head"), None
        )
        self.assertIsNotNone(verify)

    def test_context_capsule_uploads_short_lived_artifact(self) -> None:
        job = self.workflow["jobs"]["context-capsule"]
        steps = job.get("steps", [])
        upload = next(
            (s for s in steps if s.get("name") == "Upload context capsule artifact"),
            None,
        )
        self.assertIsNotNone(upload)
        with_clause = upload.get("with", {})
        self.assertIn("context-capsule-", with_clause.get("name", ""))
        self.assertEqual(with_clause.get("retention-days"), 1)
        self.assertEqual(upload.get("if"), "always()")

    def test_context_capsule_validates_matrix(self) -> None:
        job = self.workflow["jobs"]["context-capsule"]
        steps = job.get("steps", [])
        validate = next(
            (s for s in steps if s.get("name") == "Validate source-check matrix is fully successful"),
            None,
        )
        self.assertIsNotNone(validate)
        run = validate.get("run", "")
        self.assertIn("--require-success", run)

    def test_context_capsule_binds_event_pr_with_trusted_proof_and_renders_one_snapshot(self) -> None:
        job = self.workflow["jobs"]["context-capsule"]
        self.assertEqual(
            job.get("env", {}).get("GITHUB_PR_NUMBER"),
            "${{ github.event.pull_request.number || '' }}",
        )
        steps = job.get("steps", [])
        trusted_checkout = next(
            (step for step in steps if step.get("name") == "Check out trusted exact-head verifier"),
            None,
        )
        self.assertIsNotNone(trusted_checkout)
        self.assertEqual(
            trusted_checkout.get("with", {}).get("ref"),
            "${{ github.event.pull_request.base.sha }}",
        )
        trusted_verify = next(
            (step for step in steps if step.get("name") == "Verify live PR head with trusted base action"),
            None,
        )
        self.assertIsNotNone(trusted_verify)
        self.assertEqual(trusted_verify.get("uses"), "./trusted-base/actions/exact-head-check")
        self.assertEqual(
            trusted_verify.get("with", {}).get("github-token"),
            "${{ github.token }}",
        )
        generate = next(
            (step for step in steps if step.get("name") == "Generate context capsule"),
            None,
        )
        self.assertIsNotNone(generate)
        self.assertNotIn("GH_TOKEN", generate.get("env", {}))
        self.assertIn("--exact-head-proof trusted-exact-head-proof.json", generate.get("run", ""))
        self.assertIn("env -u GH_TOKEN -u GITHUB_TOKEN", generate.get("run", ""))
        self.assertIn("--capsule-json", generate.get("run", ""))

    def test_context_capsule_not_in_required_checks(self) -> None:
        from tools import test_project_context as tpc

        importlib = __import__("importlib")
        spec = importlib.util.spec_from_file_location(
            "project_context", ROOT / "scripts" / "project_context.py"
        )
        assert spec and spec.loader
        project_context = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = project_context
        spec.loader.exec_module(project_context)

        self.assertNotIn(
            "context-capsule",
            set(project_context.REQUIRED_CI_CHECKS),
        )


if __name__ == "__main__":
    unittest.main()
