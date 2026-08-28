"""Provider-free tests for the handoff guard's Mission compatibility read."""

from __future__ import annotations

import json
from pathlib import Path
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "scripts" / "agent-control"))

import check_agent_handoff as handoff  # noqa: E402


class HandoffMissionCompatibilityTests(unittest.TestCase):
    def test_codegraph_fallback_policy_is_explicit(self):
        agents = (ROOT / "AGENTS.md").read_text(encoding="utf-8")
        policy = agents[agents.index("## Reading and Verification") :]
        self.assertIn("at most one bounded local repair attempt", policy)
        self.assertIn("immediately fall back to `rg`, raw source, compiler, and tests", policy)
        self.assertIn("not `DECISION_REQUIRED`", policy)
        self.assertIn("must never be committed", policy)

    def test_current_blocked_route_exposes_no_dispatch_capsule(self):
        next_text = (ROOT / "docs" / "NEXT_DECISION.md").read_text(encoding="utf-8")
        failures: list[str] = []
        packets = handoff.parse_packet_contracts(next_text, failures)
        self.assertEqual(failures, [])
        self.assertEqual(
            handoff.weak_agent_dispatch_failures(next_text, packets), []
        )
        packet = packets["PE7-AUTONOMOUS-STEWARD-PR4B"]
        self.assertEqual(packet["state"], "BLOCKED_PREREQUISITE")
        self.assertFalse(packet["checkpoint_allowed"])
        self.assertNotIn("weak-agent-dispatch:v1", next_text)

    def test_changed_dispatch_identity_is_rejected(self):
        next_text = (
            "# Next Decision\n\n"
            "## Active Routing\n\n"
            "1. `TOOL-SESSION-CONTEXT-1` — `READY_FOR_EXECUTION`\n\n"
            "## Packet TOOL-SESSION-CONTEXT-1\n\n"
            "**State:** `READY_FOR_EXECUTION`\n\n"
            "**Class:** `IMPLEMENT`\n\n"
            "**Allowed delta:** scripts/.\n\n"
            "### 11. Bounded Autonomous Worker Dispatch Capsule\n\n"
            "<!-- weak-agent-dispatch:v1\n"
            + json.dumps(
                {
                    "schema_version": "weak_agent_dispatch.v1",
                    "packet_id": "TOOL-OTHER-1",
                    "packet_state": "READY_FOR_EXECUTION",
                    "dispatch_lane": "provider_free_local",
                    "external_effect_limit": 0,
                    "authority_consumption_allowed": False,
                    "secret_values_allowed": False,
                    "private_paths_allowed": False,
                },
                sort_keys=True,
            )
            + "\n-->\n"
        )
        failures: list[str] = []
        packets = handoff.parse_packet_contracts(next_text, failures)
        compatibility_failures = handoff.weak_agent_dispatch_failures(
            next_text, packets
        )
        self.assertTrue(
            any("packet_id must equal" in item for item in compatibility_failures)
        )


if __name__ == "__main__":
    unittest.main()
