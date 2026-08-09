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
        self.assertEqual(
            job["permissions"],
            {
                "actions": "read",
                "contents": "read",
                "pull-requests": "read",
            },
        )
        self.assertNotIn("actions", self.parsed["permissions"])
        self.assertNotIn("pull-requests", self.parsed["permissions"])
        self.assertEqual(job["outputs"]["docs_only"], "${{ steps.classify.outputs.docs_only }}")
        self.assertEqual(job["outputs"]["mode"], "${{ steps.classify.outputs.mode }}")
        self.assertEqual(job["outputs"]["reused_pr"], "${{ steps.classify.outputs.reused_pr }}")
        source = self.job_source("classify-change-impact")
        self.assertIn("path: trusted-base", source)
        self.assertIn("trusted-base/scripts/ci/classify_change_impact.py", source)
        self.assertIn("github.event.pull_request.base.sha", source)
        self.assertIn("git diff --raw --no-renames", source)
        self.assertIn('allowed_modes = {"000000", "100644"}', source)
        self.assertIn('docs_only={str(docs_only).lower()}', source)
        self.assertIn("trusted-base/scripts/ci/main_reuse_evidence.py", source)
        self.assertIn("mode=reused_pr", source)
        self.assertIn("Upload main CI reuse receipt", source)

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
                needs = self.parsed["jobs"][name]["needs"]
                if isinstance(needs, str):
                    needs = [needs]
                self.assertIn("classify-change-impact", needs)
                self.assertIn("name: Verify exact requested head", source)
                self.assertIn("name: Report documentation-only lane", source)
                self.assertIn("name: Report accepted PR reuse lane", source)
                self.assertIn("needs.classify-change-impact.outputs.docs_only", source)
        self.assertEqual(self.source.count("name: Verify exact requested head"), 8)
        self.assertEqual(self.source.count("ref: ${{ env.EXPECTED_SHA }}"), 8)

    def test_sccache_is_pinned_and_replaces_target_archive(self) -> None:
        action = "mozilla-actions/sccache-action@9e7fa8a12102821edf02ca5dbea1acd0f89a2696"
        self.assertEqual(self.source.count(action), 3)
        self.assertEqual(self.source.count('version: "v0.15.0"'), 3)
        self.assertEqual(self.source.count('SCCACHE_GHA_ENABLED: "true"'), 3)
        self.assertEqual(self.source.count("RUSTC_WRAPPER: sccache"), 3)
        self.assertEqual(self.source.count("disable_annotations: true"), 3)
        self.assertEqual(self.source.count("continue-on-error: true"), 3)
        self.assertEqual(self.source.count('${SCCACHE_PATH}'), 3)
        self.assertNotIn("Cache Rust target for rust-tests", self.source)
        self.assertNotIn("rust-target-2026-07-10", self.source)

    def test_full_code_lane_does_not_repeat_unrelated_rust_targets(self) -> None:
        pg = self.job_source("pg-integration-tests")
        self.assertIn("bash scripts/ci/run_postgres_tests.sh", pg)
        self.assertNotIn(
            "cargo test -p engine --features pg-tests -- --test-threads=1",
            pg,
        )

        cutover = self.job_source("rust-typescript-cutover")
        self.assertIn("needs:\n      - classify-change-impact\n      - native-runtime", cutover)
        self.assertIn("Download Rust + TypeScript cutover evidence", cutover)
        self.assertIn("Validate Rust + TypeScript cutover evidence", cutover)
        self.assertNotIn("Install Rust toolchain", cutover)

        native = self.job_source("native-runtime")
        self.assertIn(
            "bash scripts/verify_rust_typescript_stack.sh --runtime-only --evidence-path",
            native,
        )

        runner = (ROOT / "scripts" / "ci" / "run_postgres_tests.sh").read_text(
            encoding="utf-8"
        )
        for target in (
            "test_pe6_fault_drills",
            "test_pg_integration",
            "test_product_golden_path_g2",
            "test_product_golden_path_recovery",
        ):
            self.assertIn(target, runner)
        self.assertIn("actual_targets", runner)
        self.assertIn("cargo test -p engine --features pg-tests --no-run", runner)
        self.assertIn("CREATE DATABASE", runner)
        self.assertIn('pids+=("$!")', runner)
        self.assertIn("trap cleanup EXIT", runner)

        rust = self.job_source("rust-tests")
        self.assertIn("run: scripts/ci/run_rust_tests.py", rust)

    def test_cargo_audit_install_is_versioned_and_cached(self) -> None:
        rust = self.job_source("rust-tests")
        self.assertIn("name: Cache cargo-audit binary", rust)
        self.assertIn("cargo-audit-0.22.2", rust)
        self.assertIn("name: Ensure cargo-audit", rust)
        self.assertIn(
            "cargo install cargo-audit --version 0.22.2 --locked --force",
            rust,
        )
        self.assertEqual(
            rust.count(
                "CARGO_TERM_COLOR=never cargo audit --version"
            ),
            2,
        )
        self.assertIn(
            "CARGO_TERM_COLOR=never cargo audit --version"
            " | grep -F 'cargo-audit-audit 0.22.2'",
            rust,
        )

    def test_docker_build_uses_pinned_scoped_buildkit_caches(self) -> None:
        docker = self.job_source("docker-build")
        self.assertIn(
            "docker/setup-buildx-action@bb05f3f5519dd87d3ba754cc423b652a5edd6d2c",
            docker,
        )
        self.assertIn(
            "docker/bake-action@d3418bd7d0e9324001bca92fa8ba175ea7e6dc9b",
            docker,
        )
        self.assertIn("files: ./docker-compose.yml", docker)
        self.assertIn("targets: |\n            engine\n            dashboard", docker)
        self.assertIn(
            "engine.cache-to=type=gha,mode=max,scope=docker-engine",
            docker,
        )
        self.assertIn(
            "dashboard.cache-to=type=gha,mode=max,scope=docker-dashboard",
            docker,
        )
        self.assertNotIn("docker compose build", docker)
        for relative in ("deploy/Dockerfile.engine", "deploy/Dockerfile.combined"):
            dockerfile = (ROOT / relative).read_text(encoding="utf-8")
            self.assertIn("dependency_cache_anchor", dockerfile)
            self.assertIn("COPY engine/Cargo.toml engine/Cargo.toml", dockerfile)
            self.assertEqual(
                dockerfile.count(
                    "cargo build --release -p engine --bin agent-control-plane"
                ),
                2,
            )

    def test_expensive_steps_are_guarded_and_postgres_is_conditional(self) -> None:
        cheap = {
            "Check out repository",
            "Verify exact requested head",
            "Report documentation-only lane",
            "Report accepted PR reuse lane",
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
                    self.assertIn("mode == 'full'", condition, step.get("name"))
        pg = self.job_source("pg-integration-tests")
        self.assertNotIn("services:", pg)
        self.assertIn("name: Start PostgreSQL service", pg)
        self.assertIn("name: Stop PostgreSQL service", pg)
        self.assertIn("docker run --detach", pg)

    def test_context_capsule_push_is_not_treated_as_a_pr_head(self) -> None:
        capsule = self.job_source("context-capsule")
        self.assertEqual(capsule.count('--expected-head-sha "${EXPECTED_SHA}"'), 1)
        self.assertGreater(
            capsule.index('--expected-head-sha "${EXPECTED_SHA}"'),
            capsule.index('if [ -n "${GITHUB_PR_NUMBER}" ]'),
        )
        self.assertEqual(capsule.count("working-directory: ${{ runner.temp }}"), 4)
        self.assertIn('open("needs-context.json"', capsule)
        self.assertIn('--capsule-json context-capsule/context-capsule.json', capsule)
        self.assertIn(
            'cp "${GITHUB_WORKSPACE}/trusted-exact-head-proof.json" trusted-exact-head-proof.json',
            capsule,
        )
        self.assertIn('--exact-head-proof trusted-exact-head-proof.json', capsule)
        self.assertIn('python "${GITHUB_WORKSPACE}/scripts/project_context.py"', capsule)
        self.assertIn('path: ${{ runner.temp }}/context-capsule/', capsule)
        self.assertNotIn("NEEDS_CONTEXT_PATH:", capsule)
        self.assertNotIn("CAPSULE_DIR:", capsule)
        self.assertNotIn('path: context-capsule/', capsule)
        self.assertIn("Revalidate trusted main CI reuse receipt", capsule)
        self.assertIn("main-ci-reuse-recomputed.json", capsule)

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
