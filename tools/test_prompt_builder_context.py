from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest


SCRIPT = (
    Path(__file__).resolve().parents[1]
    / "scripts"
    / "agent-control"
    / "prompt_builder.py"
)
SPEC = importlib.util.spec_from_file_location("prompt_builder_context", SCRIPT)
assert SPEC and SPEC.loader
prompt_builder = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = prompt_builder
SPEC.loader.exec_module(prompt_builder)


class PromptBuilderContextTests(unittest.TestCase):
    def test_active_packet_excerpt_excludes_future_packet(self) -> None:
        text = """# Next Decision
## Active Routing
1. `PE7-CURRENT-1` — `READY_FOR_EXECUTION`.

## Packet PE7-CURRENT-1
**State:** `READY_FOR_EXECUTION`
Current contract.

## Packet PE7-FUTURE-1
**State:** `BLOCKED_PREREQUISITE`
Future contract.
"""
        excerpt = prompt_builder._active_packet_context(text)
        self.assertIn("Current contract", excerpt)
        self.assertNotIn("Future contract", excerpt)

    def test_governance_context_includes_current_packet_once(self) -> None:
        next_text = """## Active Routing
1. `CI-CONTROL-1` — `READY_FOR_EXECUTION`.
## Packet CI-CONTROL-1
**State:** `READY_FOR_EXECUTION`
Unique current contract marker.
"""
        context = prompt_builder._build_task_context(
            "Change .github/workflows/tests.yml CI policy",
            "# Agent Instructions\nRepository rules",
            "# Current Status\nAccepted facts",
            next_text,
            "# Module Map\nCI owner",
        )
        self.assertEqual(context.count("Unique current contract marker"), 1)
        self.assertIn("### Current packet", context)


if __name__ == "__main__":
    unittest.main()
