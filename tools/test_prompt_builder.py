"""Tests for scripts/agent-control/prompt_builder.py capsule injection."""

from __future__ import annotations

import importlib.util
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
            offline=False,
            required_pr_number=301,
            required_head_sha="a" * 40,
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
            with mock.patch.object(
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
            with mock.patch.object(
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


if __name__ == "__main__":
    unittest.main()
