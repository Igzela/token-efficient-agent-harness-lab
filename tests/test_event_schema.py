"""Tests for event_schema.py — event.v1 validation and canonical serialization."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.event_schema import (
    canonical_event_json,
    stable_idempotency_hash,
    validate_event,
)
from harness_core.errors import SchemaViolationError


def _valid_event(**overrides):
    event = {
        "event_id": "evt-001",
        "schema_version": "event.v1",
        "event_type": "project_item_state_changed",
        "timestamp": "2026-01-01T00:00:00Z",
        "producer": {"component_id": "test", "component_type": "unit_test"},
        "correlation": {},
        "severity": "info",
        "payload": {"item_id": "item_1"},
        "idempotency_key": "idem-001",
        "parent_event_id": None,
    }
    event.update(overrides)
    return event


class ValidateEventTests(unittest.TestCase):
    def test_valid_event_passes(self):
        validate_event(_valid_event())

    def test_non_dict_raises(self):
        with self.assertRaises(SchemaViolationError):
            validate_event("not a dict")

    def test_missing_required_field_raises(self):
        event = _valid_event()
        del event["event_id"]
        with self.assertRaises(SchemaViolationError):
            validate_event(event)

    def test_wrong_schema_version_raises(self):
        with self.assertRaises(SchemaViolationError):
            validate_event(_valid_event(schema_version="event.v2"))

    def test_invalid_severity_raises(self):
        with self.assertRaises(SchemaViolationError):
            validate_event(_valid_event(severity="critical"))

    def test_non_dict_producer_raises(self):
        with self.assertRaises(SchemaViolationError):
            validate_event(_valid_event(producer="not a dict"))

    def test_missing_producer_field_raises(self):
        with self.assertRaises(SchemaViolationError):
            validate_event(_valid_event(producer={"component_id": "x"}))

    def test_empty_string_field_raises(self):
        with self.assertRaises(SchemaViolationError):
            validate_event(_valid_event(event_id=""))

    def test_non_dict_correlation_raises(self):
        with self.assertRaises(SchemaViolationError):
            validate_event(_valid_event(correlation="not a dict"))

    def test_non_dict_payload_raises(self):
        with self.assertRaises(SchemaViolationError):
            validate_event(_valid_event(payload="not a dict"))

    def test_parent_event_id_none_ok(self):
        validate_event(_valid_event(parent_event_id=None))

    def test_parent_event_id_string_ok(self):
        validate_event(_valid_event(parent_event_id="evt-000"))

    def test_parent_event_id_invalid_type_raises(self):
        with self.assertRaises(SchemaViolationError):
            validate_event(_valid_event(parent_event_id=123))


class StableIdempotencyHashTests(unittest.TestCase):
    def test_same_event_same_hash(self):
        event = _valid_event()
        h1 = stable_idempotency_hash(event)
        h2 = stable_idempotency_hash(event)
        self.assertEqual(h1, h2)

    def test_different_payload_different_hash(self):
        e1 = _valid_event(payload={"a": 1})
        e2 = _valid_event(payload={"a": 2})
        self.assertNotEqual(stable_idempotency_hash(e1), stable_idempotency_hash(e2))

    def test_excludes_event_id_and_timestamp(self):
        e1 = _valid_event(event_id="evt-1", timestamp="2026-01-01T00:00:00Z")
        e2 = _valid_event(event_id="evt-2", timestamp="2026-12-31T23:59:59Z")
        self.assertEqual(stable_idempotency_hash(e1), stable_idempotency_hash(e2))


class CanonicalEventJsonTests(unittest.TestCase):
    def test_deterministic_output(self):
        event = _valid_event()
        j1 = canonical_event_json(event)
        j2 = canonical_event_json(event)
        self.assertEqual(j1, j2)

    def test_is_valid_json(self):
        import json
        event = _valid_event()
        result = json.loads(canonical_event_json(event))
        self.assertEqual(result["event_id"], "evt-001")


if __name__ == "__main__":
    unittest.main()
