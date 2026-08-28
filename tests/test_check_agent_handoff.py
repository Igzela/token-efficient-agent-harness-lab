"""Provider-free tests for the handoff guard's Mission compatibility read."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
sys.path.insert(0, str(ROOT / "scripts" / "agent-control"))

import check_agent_handoff as handoff  # noqa: E402


class HandoffMissionCompatibilityTests(unittest.TestCase):
    def test_current_accepted_capsule_passes_the_legacy_projection(self):
        next_text = (ROOT / "docs" / "NEXT_DECISION.md").read_text(encoding="utf-8")
        failures: list[str] = []
        packets = handoff.parse_packet_contracts(next_text, failures)
        self.assertEqual(failures, [])
        self.assertEqual(
            handoff.weak_agent_dispatch_failures(next_text, packets), []
        )
        packet = packets["PE7-AUTONOMOUS-STEWARD-PR3"]
        heading = handoff.PACKET_HEADING_RE.search(next_text)
        self.assertIsNotNone(heading)
        self.assertEqual(packet["source_path"], "docs/NEXT_DECISION.md")
        self.assertEqual(
            packet["packet_sha256"],
            hashlib.sha256(next_text[heading.start() :].encode("utf-8")).hexdigest(),
        )

    def test_changed_capsule_identity_is_rejected_by_the_new_compatibility_read(self):
        next_text = (ROOT / "docs" / "NEXT_DECISION.md").read_text(encoding="utf-8")
        failures: list[str] = []
        packets = handoff.parse_packet_contracts(next_text, failures)
        marker = handoff.WEAK_AGENT_DISPATCH_RE.search(next_text)
        self.assertIsNotNone(marker)
        payload = json.loads(marker.group("payload"))
        payload["packet_id"] = "PE7-AUTONOMOUS-STEWARD-PR2"
        forged = next_text[: marker.start("payload")] + json.dumps(payload, sort_keys=True) + next_text[marker.end("payload") :]
        compatibility_failures = handoff.weak_agent_dispatch_failures(forged, packets)
        self.assertTrue(
            any("legacy Mission compatibility invalid" in item for item in compatibility_failures)
        )


if __name__ == "__main__":
    unittest.main()
