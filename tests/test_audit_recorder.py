"""Tests for audit_recorder.py — ProviderAuditEvent and ProviderAuditRecorder."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.provider.audit_recorder import (
    ProviderAuditEvent,
    ProviderAuditRecorder,
)


class ProviderAuditEventTests(unittest.TestCase):
    def test_create_event(self):
        event = ProviderAuditEvent(
            event_id="evt-001",
            dispatch_id="disp-001",
            provider_id="openai-1",
            event_type="request_sent",
        )
        self.assertEqual(event.event_type, "request_sent")
        self.assertIsNone(event.input_token_count)

    def test_to_dict_roundtrip(self):
        event = ProviderAuditEvent(
            event_id="evt-001",
            dispatch_id="disp-001",
            provider_id="openai-1",
            event_type="response_received",
            input_token_count=100,
            output_token_count=50,
        )
        d = event.to_dict()
        self.assertEqual(d["input_token_count"], 100)
        self.assertIn("schema_version", d)


class ProviderAuditRecorderTests(unittest.TestCase):
    def setUp(self):
        self.recorder = ProviderAuditRecorder()

    def test_record_event(self):
        event = ProviderAuditEvent(
            event_id="evt-001",
            dispatch_id="disp-001",
            provider_id="openai-1",
            event_type="request_sent",
        )
        self.recorder.record(event)
        self.assertEqual(self.recorder.count(), 1)

    def test_create_and_record(self):
        event = self.recorder.create_and_record(
            dispatch_id="disp-001",
            provider_id="openai-1",
            event_type="request_sent",
        )
        self.assertTrue(event.event_id.startswith("paudit-"))
        self.assertEqual(self.recorder.count(), 1)

    def test_list_events_by_dispatch(self):
        self.recorder.create_and_record(dispatch_id="disp-001", provider_id="p1", event_type="request_sent")
        self.recorder.create_and_record(dispatch_id="disp-001", provider_id="p1", event_type="response_received")
        self.recorder.create_and_record(dispatch_id="disp-002", provider_id="p1", event_type="request_sent")
        events = self.recorder.list_events("disp-001")
        self.assertEqual(len(events), 2)

    def test_list_all(self):
        self.recorder.create_and_record(dispatch_id="d1", provider_id="p1", event_type="request_sent")
        self.recorder.create_and_record(dispatch_id="d2", provider_id="p1", event_type="request_sent")
        self.assertEqual(len(self.recorder.list_all()), 2)


if __name__ == "__main__":
    unittest.main()
