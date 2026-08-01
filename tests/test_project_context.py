import importlib.util
from pathlib import Path
import sys
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts" / "project_context.py"
SPEC = importlib.util.spec_from_file_location("project_context", SCRIPT)
assert SPEC and SPEC.loader
project_context = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = project_context
SPEC.loader.exec_module(project_context)


class TestProjectContextRouting(unittest.TestCase):
    def test_ready_live_packet_does_not_infer_pr_from_prerequisites(self):
        text = """\
## Active Routing

1. `PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1` — `READY_FOR_EXECUTION`: satisfied by PRs #339/#340 and #342.

## Packet PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1

**State:** `READY_FOR_EXECUTION`

**Prerequisite:** Packets A and B are accepted by PR #342.
"""

        parsed = project_context.parse_first_routed_packet(text)

        self.assertEqual(
            parsed,
            {
                "packet": "PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1",
                "state": "READY_FOR_EXECUTION",
                "pr_number": None,
            },
        )

    def test_explicit_owned_pr_remains_active_pr_binding(self):
        text = """\
## Active Routing

1. `PE7-TEST-1` — `IN_PROGRESS`

## Packet PE7-TEST-1

**State:** `IN_PROGRESS`

**Owned PR:** #342
"""

        parsed = project_context.parse_first_routed_packet(text)

        self.assertEqual(parsed["packet"], "PE7-TEST-1")
        self.assertEqual(parsed["state"], "IN_PROGRESS")
        self.assertEqual(parsed["pr_number"], "342")

    def test_ready_packet_without_pr_does_not_infer_implementation_pr(self):
        action = project_context.next_permitted_action(
            {
                "packet": "PE7-PRODUCT-GOLDEN-PATH-DEEPSEEK-LIVE-SEAL-1",
                "state": "READY_FOR_EXECUTION",
            },
            None,
        )

        self.assertIn("documented prerequisites", action)
        self.assertNotIn("create or continue one focused PR", action)


if __name__ == "__main__":
    unittest.main()
