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
    def test_codegraph_fallback_policy_is_explicit(self):
        agents = (ROOT / "AGENTS.md").read_text(encoding="utf-8")
        policy = agents[agents.index("## Reading and Verification") :]
        self.assertIn("at most one bounded local repair attempt", policy)
        self.assertIn("immediately fall back to `rg`, raw source, compiler, and tests", policy)
        self.assertIn("not `DECISION_REQUIRED`", policy)
        self.assertIn("must never be committed", policy)


if __name__ == "__main__":
    unittest.main()
