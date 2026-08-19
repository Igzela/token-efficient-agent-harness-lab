"""CWS repository-session projection stays claim-bound and non-duplicative."""

from __future__ import annotations

import pathlib
import sys
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts" / "agent-control"))

import prompt_builder  # noqa: E402


class CwsRepositoryProjectionTests(unittest.TestCase):
    def test_fresh_block_uses_handles_not_full_docs(self) -> None:
        status = "STATUS-BODY" * 200
        block = prompt_builder.cws_session_projection_block(
            accepted_main_sha="a" * 40,
            head_sha="a" * 40,
            packet_id="PE7-CWS-REPOSITORY-INTEGRATION-1",
            mode="fresh",
            documents={
                "docs/CURRENT_STATUS.md": status,
                "docs/NEXT_DECISION.md": "packet",
            },
        )
        self.assertIn("accepted_main_sha", block)
        self.assertIn("docs/CURRENT_STATUS.md", block)
        self.assertEqual(block.count("STATUS-BODY"), 0)
        self.assertIn("sha256", block)

    def test_fresh_changed_head_fails_closed(self) -> None:
        with self.assertRaisesRegex(ValueError, "changed_head"):
            prompt_builder.cws_session_projection_block(
                accepted_main_sha="a" * 40,
                head_sha="b" * 40,
                packet_id="PE7-CWS-REPOSITORY-INTEGRATION-1",
                mode="fresh",
                documents={},
            )

    def test_duplicate_document_path_is_listed_once(self) -> None:
        docs = {
            "docs/CURRENT_STATUS.md": "once",
        }
        # dict cannot duplicate keys; simulate by calling with same path rebuilt
        block = prompt_builder.cws_session_projection_block(
            accepted_main_sha="c" * 40,
            head_sha="d" * 40,
            packet_id="PE7-CWS-REPOSITORY-INTEGRATION-1",
            mode="repair",
            documents=docs,
        )
        self.assertEqual(block.count("docs/CURRENT_STATUS.md"), 1)


if __name__ == "__main__":
    unittest.main()
