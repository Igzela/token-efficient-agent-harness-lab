from __future__ import annotations

from pathlib import Path


def replace_once(source: str, old: str, new: str, label: str) -> str:
    count = source.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one match, got {count}")
    return source.replace(old, new, 1)


def update_tests_workflow() -> None:
    path = Path(".github/workflows/tests.yml")
    source = path.read_text(encoding="utf-8")
    source = replace_once(
        source,
        """      - name: Check out trusted base classifier
        if: github.event_name == 'pull_request'
        uses: actions/checkout@df4cb1c069e1874edd31b4311f1884172cec0e10 # v6
        with:
          ref: ${{ github.event.pull_request.base.sha }}
          path: trusted-base
          persist-credentials: false
""",
        """      - name: Check out trusted base classifier
        if: github.event_name == 'pull_request' || (github.event_name == 'push' && github.event.before != '0000000000000000000000000000000000000000' && github.event.forced != true)
        uses: actions/checkout@df4cb1c069e1874edd31b4311f1884172cec0e10 # v6
        with:
          ref: ${{ github.event.pull_request.base.sha || github.event.before }}
          path: trusted-base
          persist-credentials: false
""",
        "trusted-base checkout",
    )
    source = replace_once(
        source,
        """        env:
          EVENT_NAME: ${{ github.event_name }}
          BASE_SHA: ${{ github.event.pull_request.base.sha || '' }}
        run: |
          set -euo pipefail
          if [ "${EVENT_NAME}" != "pull_request" ]; then
            echo "docs_only=false" >> "$GITHUB_OUTPUT"
            echo "mode=full" >> "$GITHUB_OUTPUT"
            exit 0
          fi
          test -n "${BASE_SHA}"
          git cat-file -e "${BASE_SHA}^{commit}" 2>/dev/null || git fetch --no-tags --depth=1 origin "${BASE_SHA}"
          git diff --name-only "${BASE_SHA}" "${EXPECTED_SHA}" > changed-files.txt
""",
        """        env:
          EVENT_NAME: ${{ github.event_name }}
          BASE_SHA: ${{ github.event.pull_request.base.sha || github.event.before || '' }}
          PUSH_FORCED: ${{ github.event.forced || false }}
        run: |
          set -euo pipefail
          full_mode() {
            echo "docs_only=false" >> "$GITHUB_OUTPUT"
            echo "mode=full" >> "$GITHUB_OUTPUT"
          }
          if [ "${EVENT_NAME}" = "workflow_dispatch" ]; then
            full_mode
            exit 0
          fi
          if [ "${EVENT_NAME}" != "pull_request" ] && [ "${EVENT_NAME}" != "push" ]; then
            full_mode
            exit 0
          fi
          if [ "${EVENT_NAME}" = "push" ]; then
            if [ "${PUSH_FORCED}" = "true" ] || [ -z "${BASE_SHA}" ] || [ "${BASE_SHA}" = "0000000000000000000000000000000000000000" ]; then
              full_mode
              exit 0
            fi
          fi
          test -n "${BASE_SHA}"
          git cat-file -e "${BASE_SHA}^{commit}" 2>/dev/null || git fetch --no-tags --depth=1 origin "${BASE_SHA}"
          if ! git merge-base --is-ancestor "${BASE_SHA}" "${EXPECTED_SHA}"; then
            full_mode
            exit 0
          fi
          git diff --name-only "${BASE_SHA}" "${EXPECTED_SHA}" > changed-files.txt
""",
        "classifier event contract",
    )
    source = replace_once(
        source,
        "run: uv run --no-project --with pyyaml python -m unittest tools.test_ci_workflow_optimization\n",
        "run: uv run --no-project --with pyyaml python -m unittest tools.test_ci_workflow_optimization tools.test_ci_lane_contract\n",
        "canonical workflow tests",
    )
    path.write_text(source, encoding="utf-8")


def update_fast_workflow() -> None:
    path = Path(".github/workflows/pr-fast-checks.yml")
    source = path.read_text(encoding="utf-8")
    source = replace_once(
        source,
        """env:
  EXPECTED_SHA: ${{ github.event.pull_request.head.sha }}
  BASE_SHA: ${{ github.event.pull_request.base.sha }}
""",
        """env:
  EXPECTED_SHA: ${{ github.event.pull_request.head.sha }}
  BASE_SHA: ${{ github.event.pull_request.base.sha }}
  PR_IS_DRAFT: ${{ github.event.pull_request.draft }}
""",
        "fast workflow env",
    )
    source = replace_once(
        source,
        """    steps:
      - name: Check out exact PR head
""",
        """    steps:
      - name: Enforce Draft lane
        run: |
          set -euo pipefail
          if [ "${PR_IS_DRAFT}" != "true" ]; then
            echo "Changing PR heads must remain Draft. Convert this PR to Draft, publish the stabilized head, then mark it Ready once to trigger canonical tests."
            exit 1
          fi
      - name: Check out exact PR head
""",
        "fast workflow guard",
    )
    source = replace_once(
        source,
        "          uv run --no-project python -m unittest tools.test_classify_change_impact\n",
        "          uv run --no-project python -m unittest tools.test_classify_change_impact\n          uv run --no-project --with pyyaml python -m unittest tools.test_ci_lane_contract\n",
        "fast workflow tests",
    )
    path.write_text(source, encoding="utf-8")


def write_contract_test() -> None:
    Path("tools/test_ci_lane_contract.py").write_text(
        '''from __future__ import annotations

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
            self.tests_source.index("  classify-change-impact:\\n") :
            self.tests_source.index("  python-tests:\\n")
        ]
        self.assertIn("github.event.pull_request.base.sha || github.event.before", classifier)
        self.assertIn("git merge-base --is-ancestor", classifier)
        self.assertIn('git diff --name-only "${BASE_SHA}" "${EXPECTED_SHA}"', classifier)
        self.assertIn("trusted-base/scripts/ci/classify_change_impact.py", classifier)
        self.assertNotIn('if [ "${EVENT_NAME}" != "pull_request" ]; then', classifier)

    def test_uncertain_push_and_dispatch_fail_closed_to_full(self) -> None:
        classifier = self.tests_source[
            self.tests_source.index("  classify-change-impact:\\n") :
            self.tests_source.index("  python-tests:\\n")
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
''',
        encoding="utf-8",
    )


def update_agents() -> None:
    path = Path("AGENTS.md")
    source = path.read_text(encoding="utf-8")
    source = replace_once(
        source,
        """Keep an implementation PR in Draft while its diff is changing. Draft pushes run the separate `pr-fast-checks` workflow for exact-head governance feedback only. That workflow is deliberately non-canonical: it cannot authorize review, merge, release, deployment, or acceptance.

Run focused and applicable full checks locally, finish the complete-diff review, collect all known findings into one repair batch, and only then mark the PR Ready for review. The `ready_for_review` event triggers the single canonical `tests` workflow. Its accepted-base classifier selects either the complete source-test matrix or the strict documentation-only mode; pushes to `main` and explicit exact-head fallback dispatches always use the complete matrix.
""",
        """Keep an implementation PR in Draft while its diff is changing. The `pr-fast-checks` workflow enforces that `opened`, `synchronize`, and `reopened` heads are Draft; a non-Draft mutating event fails immediately and must be corrected by converting the PR to Draft. Draft pushes provide exact-head governance feedback only and cannot authorize review, merge, release, deployment, or acceptance.

Run focused and applicable full checks locally, finish the complete-diff review, collect all known findings into one repair batch, and only then mark the PR Ready once. The `ready_for_review` event triggers the single canonical `tests` workflow. Its trusted classifier selects either the complete source-test matrix or strict documentation-only mode. A normal `main` push may use documentation-only mode only from the previous-main `before...after` diff and accepted-before classifier; forced, zero-base, non-ancestor, mixed, or uncertain pushes fail closed to the complete matrix. Explicit exact-head fallback dispatches always use the complete matrix.
""",
        "AGENTS CI discipline",
    )
    path.write_text(source, encoding="utf-8")


def update_playbook() -> None:
    path = Path("docs/REAL_WORLD_TESTING_PLAYBOOK.md")
    source = path.read_text(encoding="utf-8")
    source = replace_once(
        source,
        """- `tests` is the sole canonical source-test workflow. For pull requests it starts only on `ready_for_review`; pushes to `main` and explicit exact-head `workflow_dispatch` fallback runs remain complete-matrix paths. Every required source job must execute its exact-head verification step successfully.

For a Ready pull request, `tests` checks out the accepted base separately and uses that trusted classifier to inspect the exact `base...head` path and file-mode diff. A strictly documentation-only result selects canonical `docs_only` mode: all required jobs still check out and verify the exact head and finish successfully, while compiler, runtime, database-test, TypeScript, Docker-build, and other non-applicable source-test steps report not applicable. Empty, mixed, executable, symlink, submodule, workflow, script, test, configuration, dependency, schema, migration, generated, or uncertain diffs fail closed to the complete matrix. Candidate-controlled classifier code cannot grant itself the documentation-only mode.

Keep a changing PR in Draft. Before marking it Ready, batch all known repairs, run focused and applicable full local checks, and review the complete diff. If a Ready candidate needs another commit, convert it back to Draft before publishing the replacement head, then mark it Ready again after the repair batch stabilizes. A new head invalidates all prior CI and review conclusions.
""",
        """- `tests` is the sole canonical source-test workflow. For pull requests it starts only on `ready_for_review`; normal pushes to `main` may use the same strict documentation-only mode, while explicit exact-head `workflow_dispatch` fallback runs remain complete-matrix paths. Every required source job must execute its exact-head verification step successfully.

For a Ready pull request, `tests` checks out the accepted base separately and uses that trusted classifier to inspect the exact `base...head` path and file-mode diff. For a normal `main` push it instead binds the accepted-before commit and classifies the complete `before...after` range. A strictly documentation-only result selects canonical `docs_only` mode: all required jobs still check out and verify the exact head and finish successfully, while compiler, runtime, database-test, TypeScript, Docker-build, and other non-applicable source-test steps report not applicable. Empty, mixed, executable, symlink, submodule, workflow, script, test, configuration, dependency, schema, migration, generated, forced, zero-base, non-ancestor, or otherwise uncertain diffs fail closed to the complete matrix. Candidate-controlled classifier code cannot grant itself documentation-only mode.

Keep a changing PR in Draft. `pr-fast-checks` enforces Draft state on `opened`, `synchronize`, and `reopened`; directly opening or updating a Ready PR fails the lane guard. Before marking a PR Ready once, batch all known repairs, run focused and applicable full local checks, and review the complete diff. If a Ready candidate needs another commit, convert it back to Draft before publishing the replacement head, then mark it Ready again after the repair batch stabilizes. A new head invalidates all prior CI and review conclusions.
""",
        "playbook CI contract",
    )
    path.write_text(source, encoding="utf-8")


def main() -> None:
    update_tests_workflow()
    update_fast_workflow()
    write_contract_test()
    update_agents()
    update_playbook()


if __name__ == "__main__":
    main()
