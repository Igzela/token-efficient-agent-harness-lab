"""Tests for dispatch_ledger.py."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_ledger import DispatchLedger


class DispatchLedgerTests(unittest.TestCase):
    def setUp(self):
        self.ledger = DispatchLedger()

    def test_create_record(self):
        r = self.ledger.create_record("disp-001", "test request", "a-001", "dec-001")
        self.assertEqual(r.dispatch_id, "disp-001")
        self.assertEqual(r.final_status, "dispatched")

    def test_get_record(self):
        self.ledger.create_record("disp-001", "req", "a-001", "dec-001")
        r = self.ledger.get_record("disp-001")
        self.assertIsNotNone(r)
        self.assertEqual(r.dispatch_id, "disp-001")

    def test_get_nonexistent_record(self):
        r = self.ledger.get_record("nonexistent")
        self.assertIsNone(r)

    def test_list_records(self):
        self.ledger.create_record("disp-001", "req1", "a-001", "dec-001")
        self.ledger.create_record("disp-002", "req2", "a-002", "dec-002")
        records = self.ledger.list_records()
        self.assertEqual(len(records), 2)

    def test_update_record(self):
        r = self.ledger.create_record("disp-001", "req", "a-001", "dec-001")
        updated = self.ledger.update_record(r, final_status="completed", execution_result_id="exec-001")
        self.assertEqual(updated.final_status, "completed")
        self.assertEqual(updated.execution_result_id, "exec-001")
        self.assertNotEqual(updated.updated_at, r.created_at)

    def test_update_persists(self):
        r = self.ledger.create_record("disp-001", "req", "a-001", "dec-001")
        self.ledger.update_record(r, final_status="completed")
        stored = self.ledger.get_record("disp-001")
        self.assertEqual(stored.final_status, "completed")

    def test_replay(self):
        self.ledger.create_record("disp-001", "req", "a-001", "dec-001")
        r = self.ledger.replay("disp-001")
        self.assertIsNotNone(r)
        self.assertEqual(r.dispatch_id, "disp-001")

    def test_to_dict(self):
        r = self.ledger.create_record("disp-001", "req", "a-001", "dec-001")
        d = r.to_dict()
        self.assertIn("dispatch_id", d)
        self.assertIn("final_status", d)
        self.assertIn("schema_version", d)


if __name__ == "__main__":
    unittest.main()
