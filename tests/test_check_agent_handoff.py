"""Provider-free tests for the handoff guard's Mission compatibility read."""

from __future__ import annotations

from pathlib import Path
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "scripts" / "agent-control"))

import check_agent_handoff as handoff  # noqa: E402


class HandoffMissionCompatibilityTests(unittest.TestCase):
    def test_current_blocked_route_exposes_no_dispatch_capsule(self):
        next_text = (ROOT / "docs" / "NEXT_DECISION.md").read_text(encoding="utf-8")
        failures: list[str] = []
        packets = handoff.parse_packet_contracts(next_text, failures)
        self.assertEqual(failures, [])
        self.assertEqual(
            handoff.weak_agent_dispatch_failures(next_text, packets), []
        )
        packet = packets["PE7-AUTONOMOUS-STEWARD-PR4"]
        self.assertEqual(packet["state"], "BLOCKED_PREREQUISITE")
        self.assertFalse(packet["checkpoint_allowed"])
        self.assertNotIn("weak-agent-dispatch:v1", next_text)

    def test_blocked_route_never_authorizes_legacy_compatibility(self):
        next_text = (ROOT / "docs" / "NEXT_DECISION.md").read_text(encoding="utf-8")
        failures: list[str] = []
        packets = handoff.parse_packet_contracts(next_text, failures)
        self.assertEqual(
            handoff.weak_agent_dispatch_failures(next_text, packets), []
        )
        self.assertEqual(
            packets["PE7-AUTONOMOUS-STEWARD-PR4"]["execution_authorized"],
            False,
        )


if __name__ == "__main__":
    unittest.main()
