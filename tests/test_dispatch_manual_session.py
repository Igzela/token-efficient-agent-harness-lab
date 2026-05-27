"""Tests for manual_session.py — ManualExecutionSession schema and store."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.manual_session import (
    ManualExecutionSession,
    ManualSessionStore,
    MANUAL_SESSION_STATUSES,
)


class ManualExecutionSessionSchemaTests(unittest.TestCase):
    def test_to_dict_roundtrip(self):
        store = ManualSessionStore()
        session = store.create("disp-001", "pp-001")
        d = session.to_dict()
        self.assertIn("session_id", d)
        self.assertEqual(d["dispatch_id"], "disp-001")
        self.assertEqual(d["status"], "created")

    def test_has_required_fields(self):
        store = ManualSessionStore()
        session = store.create("disp-001", "pp-001")
        self.assertTrue(session.session_id)
        self.assertTrue(session.created_at)
        self.assertTrue(session.updated_at)


class ManualSessionStoreTests(unittest.TestCase):
    def setUp(self):
        self.store = ManualSessionStore()

    def test_create_session(self):
        session = self.store.create("disp-001", "pp-001")
        self.assertEqual(session.status, "created")
        self.assertEqual(session.dispatch_id, "disp-001")

    def test_get_session(self):
        session = self.store.create("disp-001", "pp-001")
        found = self.store.get(session.session_id)
        self.assertIsNotNone(found)
        self.assertEqual(found.session_id, session.session_id)

    def test_advance_status(self):
        session = self.store.create("disp-001", "pp-001")
        advanced = self.store.advance(session, "prompt_generated")
        self.assertEqual(advanced.status, "prompt_generated")

    def test_advance_with_submission(self):
        session = self.store.create("disp-001", "pp-001")
        advanced = self.store.advance(session, "result_submitted", submission_id="pb-001")
        self.assertEqual(advanced.submission_id, "pb-001")

    def test_advance_invalid_status_raises(self):
        session = self.store.create("disp-001", "pp-001")
        with self.assertRaises(ValueError):
            self.store.advance(session, "invalid_status")

    def test_get_by_dispatch(self):
        self.store.create("disp-001", "pp-001")
        self.store.create("disp-002", "pp-002")
        found = self.store.get_by_dispatch("disp-002")
        self.assertIsNotNone(found)
        self.assertEqual(found.dispatch_id, "disp-002")

    def test_list_sessions(self):
        self.store.create("disp-001", "pp-001")
        self.store.create("disp-002", "pp-002")
        sessions = self.store.list_sessions()
        self.assertEqual(len(sessions), 2)

    def test_full_lifecycle(self):
        session = self.store.create("disp-001", "pp-001")
        s1 = self.store.advance(session, "prompt_generated")
        s2 = self.store.advance(s1, "human_executing")
        s3 = self.store.advance(s2, "result_submitted", submission_id="pb-001")
        s4 = self.store.advance(s3, "evaluated", evaluation_id="meval-001")
        s5 = self.store.advance(s4, "recorded")
        self.assertEqual(s5.status, "recorded")


if __name__ == "__main__":
    unittest.main()
