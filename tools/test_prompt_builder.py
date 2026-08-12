"""Tests for scripts/agent-control/prompt_builder.py capsule injection."""

from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "agent-control" / "prompt_builder.py"
SPEC = importlib.util.spec_from_file_location("prompt_builder", SCRIPT)
assert SPEC and SPEC.loader
prompt_builder = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = prompt_builder
SPEC.loader.exec_module(prompt_builder)


class PromptBuilderCapsuleTests(unittest.TestCase):
    def _minimal_capsule(self) -> str:
        return "# Project Context Capsule\n\n- Test capsule\n"

    def _live_review_binding(self) -> tuple[bool, str, dict[str, str]]:
        return True, "ok", {
            "head_sha": "a" * 40,
            "base_sha": "c" * 40,
            "reviewed_range": f"{'c' * 40}...{'a' * 40}",
        }

    def test_implementation_prompt_prepends_capsule(self) -> None:
        with mock.patch.object(
            prompt_builder,
            "generate_fresh_capsule",
            return_value=self._minimal_capsule(),
        ) as gen:
            prompt = prompt_builder.build_implementation_prompt(123)
        gen.assert_called_once()
        self.assertIn("## Fresh Repository Context Capsule", prompt)
        self.assertIn("Issue #123", prompt)
        # Capsule must appear before task-specific content.
        capsule_pos = prompt.find("## Fresh Repository Context Capsule")
        task_pos = prompt.find("## Task")
        self.assertGreaterEqual(capsule_pos, 0)
        self.assertGreater(task_pos, capsule_pos)

    def test_ci_repair_prompt_prepends_capsule(self) -> None:
        with mock.patch.object(
            prompt_builder,
            "generate_fresh_capsule",
            return_value=self._minimal_capsule(),
        ) as gen:
            prompt = prompt_builder.build_ci_repair_prompt(301, "a" * 40, "[]", "logs", 0)
        gen.assert_called_once_with(
            offline=True,
            required_pr_number=301,
            required_head_sha="a" * 40,
            require_local_checkout=True,
        )
        self.assertIn("## Fresh Repository Context Capsule", prompt)
        self.assertIn("CI repair for PR #301", prompt)
        capsule_pos = prompt.find("## Fresh Repository Context Capsule")
        task_pos = prompt.find("## CI Repair Task")
        self.assertGreater(task_pos, capsule_pos)

    def test_review_prompt_prepends_capsule(self) -> None:
        with mock.patch.object(
            prompt_builder,
            "generate_fresh_capsule",
            return_value=self._minimal_capsule(),
        ) as gen:
            with mock.patch(
                "state_manager.resolve_live_review_binding",
                return_value=self._live_review_binding(),
            ), mock.patch.object(
                prompt_builder,
                "_gh",
                return_value="diff content",
            ):
                prompt = prompt_builder.build_review_prompt(301, "a" * 40)
        gen.assert_called_once_with(
            offline=False,
            required_pr_number=301,
            required_head_sha="a" * 40,
        )
        self.assertIn("## Fresh Repository Context Capsule", prompt)
        self.assertIn("Review of PR #301", prompt)
        capsule_pos = prompt.find("## Fresh Repository Context Capsule")
        task_pos = prompt.find("## Final Review Task")
        self.assertGreater(task_pos, capsule_pos)

    def test_head_mismatch_refuses_prompt(self) -> None:
        with mock.patch.object(
            prompt_builder,
            "generate_fresh_capsule",
            side_effect=ValueError("head mismatch"),
        ):
            with self.assertRaises(ValueError):
                prompt_builder.build_ci_repair_prompt(301, "a" * 40, "[]", "", 0)

    def test_review_binding_failure_refuses_diff_acquisition(self) -> None:
        with mock.patch(
            "state_manager.resolve_live_review_binding",
            return_value=(False, "head_mismatch", None),
        ), mock.patch.object(prompt_builder, "_gh") as gh:
            with self.assertRaisesRegex(ValueError, "review binding rejected: head_mismatch"):
                prompt_builder.build_review_prompt(301, "a" * 40)
        gh.assert_not_called()

    def test_review_head_move_during_prompt_build_is_rejected(self) -> None:
        with mock.patch(
            "state_manager.resolve_live_review_binding",
            side_effect=[
                self._live_review_binding(),
                (False, "head_mismatch", None),
            ],
        ), mock.patch.object(
            prompt_builder, "_gh", return_value="diff content"
        ), mock.patch.object(
            prompt_builder, "generate_fresh_capsule"
        ) as capsule:
            with self.assertRaisesRegex(
                ValueError, "review binding changed during prompt build: head_mismatch"
            ):
                prompt_builder.build_review_prompt(301, "a" * 40)
        capsule.assert_not_called()

    def test_capsule_size_bound_is_enforced(self) -> None:
        huge = "x" * (prompt_builder.MAX_CAPSULE_CHARS + 1)
        with mock.patch.object(
            prompt_builder,
            "generate_fresh_capsule",
            return_value=huge,
        ):
            with self.assertRaises(ValueError):
                prompt_builder.build_implementation_prompt(1)

    def test_capsule_does_not_embed_diff_or_logs(self) -> None:
        """Capsule content must remain separate from task-specific material."""
        # A real capsule should never contain diff or log content. We verify the
        # helper enforces separation by returning a capsule without those.
        capsule = "# Project Context Capsule\n- accepted baseline: abc123\n"
        with mock.patch.object(
            prompt_builder,
            "generate_fresh_capsule",
            return_value=capsule,
        ):
            with mock.patch(
                "state_manager.resolve_live_review_binding",
                return_value=self._live_review_binding(),
            ), mock.patch.object(
                prompt_builder,
                "_gh",
                return_value="sensitive diff content",
            ):
                prompt = prompt_builder.build_review_prompt(301, "a" * 40)
        # Diff belongs in the task section after the separator, not in the capsule.
        capsule_end = prompt.find("---\n\n")
        self.assertGreater(capsule_end, 0)
        self.assertNotIn("sensitive diff", prompt[:capsule_end])
        self.assertIn("sensitive diff", prompt[capsule_end:])

    def test_regenerate_on_every_invocation(self) -> None:
        with mock.patch.object(
            prompt_builder,
            "generate_fresh_capsule",
            return_value=self._minimal_capsule(),
        ) as gen:
            prompt_builder.build_implementation_prompt(1)
            prompt_builder.build_implementation_prompt(2)
        self.assertEqual(gen.call_count, 2)

    def test_plan_prompt_carries_every_claim_bound_contract_field(self) -> None:
        prompt = prompt_builder.build_claim_bound_plan_implementation_prompt(
            "PE7-RWE-PREFLIGHT-1",
            "Prepare the provider-free preflight.",
            ["engine/src/rwe/**", "docs/NEXT_DECISION.md"],
            "a" * 40,
            "agent/packet-pe7-rwe-preflight-1",
            prerequisites=["PE7-RWE-REFREEZE-1"],
            forbidden_changes=["No provider request", "No schema change"],
            verification=["cargo test -p engine rwe", "git diff --check"],
            rollback=["Revert the packet", "Preserve accepted evidence"],
            repo_root=Path(__file__).resolve().parents[1],
        )

        self.assertIn(
            '- prerequisites: `["PE7-RWE-REFREEZE-1"]`', prompt
        )
        self.assertIn(
            '- forbidden_changes: `["No provider request","No schema change"]`',
            prompt,
        )
        self.assertIn(
            '- verification: `["cargo test -p engine rwe","git diff --check"]`',
            prompt,
        )
        self.assertIn(
            '- rollback: `["Revert the packet","Preserve accepted evidence"]`',
            prompt,
        )

    def test_copied_prompt_builder_uses_invoking_working_tree(self) -> None:
        """A copied control script must not derive the repository from __file__."""
        with tempfile.TemporaryDirectory() as temp_dir:
            copied_script = Path(temp_dir) / "prompt_builder.py"
            shutil.copy2(SCRIPT, copied_script)
            copied_spec = importlib.util.spec_from_file_location(
                "copied_prompt_builder", copied_script
            )
            assert copied_spec and copied_spec.loader
            copied_builder = importlib.util.module_from_spec(copied_spec)
            sys.modules[copied_spec.name] = copied_builder
            copied_spec.loader.exec_module(copied_builder)

            with mock.patch.dict(
                os.environ,
                {"GITHUB_EVENT_NAME": "pull_request", "GITHUB_SHA": "a" * 40},
                clear=False,
            ):
                capsule = copied_builder.generate_fresh_capsule(offline=True)

        self.assertIn("# Project Context Capsule", capsule)

    def test_capsule_markdown_renders_the_validated_json_snapshot(self) -> None:
        commands: list[list[str]] = []

        def run(command, **_kwargs):
            commands.append(command)
            if "--capsule-json" in command:
                return subprocess.CompletedProcess(command, 0, "# Project Context Capsule\n", "")
            return subprocess.CompletedProcess(command, 0, "{}", "")

        with mock.patch.object(prompt_builder.subprocess, "run", side_effect=run):
            capsule = prompt_builder.generate_fresh_capsule(offline=True)

        self.assertEqual(capsule, "# Project Context Capsule\n")
        self.assertEqual(sum("--capsule-json" not in command for command in commands), 1)
        self.assertIn("--capsule-json", commands[1])

    def test_unavailable_frontier_does_not_break_required_pr_transport(self) -> None:
        capsule_json = json.dumps(
            {
                "local_checkout": {"head_sha": "a" * 40},
                "binding": {"pr_exact_head": None},
                "active_frontier": None,
                "active_packet": None,
            }
        )

        def run(command, **_kwargs):
            output = (
                "# Project Context Capsule\n"
                if "--capsule-json" in command
                else capsule_json
            )
            return subprocess.CompletedProcess(command, 0, output, "")

        with mock.patch.dict(os.environ, {"GITHUB_SHA": "a" * 40}, clear=False), mock.patch.object(
            prompt_builder.subprocess, "run", side_effect=run
        ):
            capsule = prompt_builder.generate_fresh_capsule(
                offline=True,
                required_pr_number=301,
                required_head_sha="a" * 40,
        )
        self.assertIn("Project Context Capsule", capsule)

    def test_required_pr_binds_workflow_surface_not_canonical_frontier(self) -> None:
        sha = "a" * 40
        capsule_json = json.dumps(
            {
                "local_checkout": {"head_sha": sha},
                "binding": {"pr_exact_head": {"head_sha": sha}},
                "active_frontier": {
                    "number": 370,
                    "availability": "confirmed",
                },
                "workflow_frontier": {
                    "number": 371,
                    "availability": "confirmed",
                },
                "active_packet": {"packet": "PE7-RWE-V2-REFREEZE-1"},
            }
        )

        def run(command, **_kwargs):
            output = "# Project Context Capsule\n" if "--capsule-json" in command else capsule_json
            return subprocess.CompletedProcess(command, 0, output, "")

        with mock.patch.object(prompt_builder.subprocess, "run", side_effect=run):
            capsule = prompt_builder.generate_fresh_capsule(
                required_pr_number=371,
                required_head_sha=sha,
            )
        self.assertIn("Project Context Capsule", capsule)

    def test_ci_repair_prefers_checked_out_head_over_dispatch_sha(self) -> None:
        sha = "a" * 40
        capsule_json = json.dumps(
            {
                "local_checkout": {"head_sha": sha},
                "binding": {"pr_exact_head": None},
                "active_frontier": None,
                "active_packet": None,
            }
        )

        def run(command, **_kwargs):
            output = "# Project Context Capsule\n" if "--capsule-json" in command else capsule_json
            return subprocess.CompletedProcess(command, 0, output, "")

        with mock.patch.dict(
            os.environ,
            {
                "GITHUB_EVENT_NAME": "workflow_dispatch",
                "GITHUB_SHA": "b" * 40,
                "AGENT_CONTEXT_EXPECTED_HEAD_SHA": sha,
            },
            clear=False,
        ), mock.patch.object(prompt_builder.subprocess, "run", side_effect=run):
            capsule = prompt_builder.generate_fresh_capsule(
                offline=True,
                required_pr_number=301,
                required_head_sha=sha,
                require_local_checkout=True,
            )
        self.assertIn("Project Context Capsule", capsule)

    def test_ci_repair_rejects_mismatched_checked_out_head(self) -> None:
        expected = "a" * 40
        capsule_json = json.dumps(
            {
                "local_checkout": {"head_sha": "b" * 40},
                "binding": {"pr_exact_head": None},
                "active_frontier": None,
                "active_packet": None,
            }
        )

        def run(command, **_kwargs):
            return subprocess.CompletedProcess(command, 0, capsule_json, "")

        with mock.patch.dict(
            os.environ,
            {"AGENT_CONTEXT_EXPECTED_HEAD_SHA": expected},
            clear=False,
        ), mock.patch.object(prompt_builder.subprocess, "run", side_effect=run), self.assertRaisesRegex(
            ValueError, "Checked-out SHA"
        ):
            prompt_builder.generate_fresh_capsule(
                offline=True,
                required_pr_number=301,
                required_head_sha=expected,
                require_local_checkout=True,
            )

    def test_pr_head_takes_precedence_over_workflow_merge_sha(self) -> None:
        sha = "a" * 40
        capsule_json = json.dumps(
            {
                "local_checkout": {"head_sha": sha},
                "binding": {"pr_exact_head": {"head_sha": sha}},
                "active_frontier": None,
                "active_packet": None,
            }
        )

        def run(command, **_kwargs):
            output = "# Project Context Capsule\n" if "--capsule-json" in command else capsule_json
            return subprocess.CompletedProcess(command, 0, output, "")

        with mock.patch.dict(
            os.environ,
            {"GITHUB_SHA": "b" * 40},
            clear=False,
        ), mock.patch.object(prompt_builder.subprocess, "run", side_effect=run):
            capsule = prompt_builder.generate_fresh_capsule(
                required_pr_number=301,
                required_head_sha=sha,
            )
        self.assertIn("Project Context Capsule", capsule)


if __name__ == "__main__":
    unittest.main()
