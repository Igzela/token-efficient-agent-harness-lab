import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core import generate_batch_digest, replay_all


SANITIZED_FIXTURE = (
    Path(__file__).resolve().parent / "fixtures" / "stage0_events_sanitized.jsonl"
)


class DigestTests(unittest.TestCase):
    def test_digest_stub_summarizes_projection_bundle(self):
        projections = replay_all(SANITIZED_FIXTURE)

        digest = generate_batch_digest(projections)

        self.assertEqual(
            ("item_001", "item_002", "item_003", "item_004", "item_005"),
            digest.completed_items,
        )
        self.assertEqual(3, digest.handoff_count)
        self.assertEqual(2, digest.resolved_dependency_count)
        self.assertEqual((), digest.blocked_or_waiting_approval)
        self.assertEqual((), digest.failed_items)


if __name__ == "__main__":
    unittest.main()
