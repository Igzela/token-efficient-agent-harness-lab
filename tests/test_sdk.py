"""Tests for harness_core/sdk.py — HarnessSDK programmatic API."""

import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.sdk import SDK_SCHEMA_VERSION, HarnessSDK


class HarnessSDKCreationTests(unittest.TestCase):
    def test_schema_version(self):
        self.assertEqual(SDK_SCHEMA_VERSION, "sdk.v1")

    def test_in_memory_creation(self):
        sdk = HarnessSDK()
        self.assertIsNotNone(sdk._store)
        self.assertIsNotNone(sdk._engine)
        sdk.close()

    def test_file_backed_creation(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            db_path = str(Path(tmpdir) / "test.db")
            sdk = HarnessSDK(store_path=db_path)
            self.assertIsNotNone(sdk._store)
            sdk.close()

    def test_components_are_independent(self):
        sdk1 = HarnessSDK()
        sdk2 = HarnessSDK()
        result1 = sdk1.create_dispatch({"raw_request": "hello"})
        result2 = sdk2.create_dispatch({"raw_request": "world"})
        self.assertNotEqual(
            result1["dispatch_id"], result2["dispatch_id"],
        )
        sdk1.close()
        sdk2.close()


class CreateDispatchTests(unittest.TestCase):
    def setUp(self):
        self.sdk = HarnessSDK()

    def tearDown(self):
        self.sdk.close()

    def test_returns_dict_with_required_keys(self):
        result = self.sdk.create_dispatch({"raw_request": "test task"})
        self.assertIn("dispatch_id", result)
        self.assertIn("decision", result)
        self.assertIn("record", result)
        self.assertIn("execution_status", result)
        self.assertIn("evaluation_status", result)

    def test_dispatch_id_prefix(self):
        result = self.sdk.create_dispatch({"raw_request": "test"})
        self.assertTrue(result["dispatch_id"].startswith("disp-"))

    def test_decision_has_schema_version(self):
        result = self.sdk.create_dispatch({"raw_request": "test"})
        self.assertEqual(result["decision"]["schema_version"], "dispatch_decision.v1")

    def test_execution_status_is_string(self):
        result = self.sdk.create_dispatch({"raw_request": "test"})
        self.assertIsInstance(result["execution_status"], str)

    def test_evaluation_status_is_string(self):
        result = self.sdk.create_dispatch({"raw_request": "test"})
        self.assertIsInstance(result["evaluation_status"], str)

    def test_empty_raw_request(self):
        result = self.sdk.create_dispatch({"raw_request": ""})
        self.assertIn("dispatch_id", result)

    def test_missing_raw_request_key(self):
        result = self.sdk.create_dispatch({})
        self.assertIn("dispatch_id", result)

    def test_request_source_in_analysis(self):
        result = self.sdk.create_dispatch({"raw_request": "test"})
        self.assertIn("request_source", result["decision"]["analysis_snapshot"])

    def test_custom_request_source(self):
        result = self.sdk.create_dispatch({
            "raw_request": "test",
            "request_source": "agent",
        })
        self.assertEqual(
            result["decision"]["analysis_snapshot"]["request_source"], "agent",
        )


class ListPlansTests(unittest.TestCase):
    def setUp(self):
        self.sdk = HarnessSDK()

    def tearDown(self):
        self.sdk.close()

    def test_empty_store(self):
        plans = self.sdk.list_plans()
        self.assertEqual(plans, [])

    def test_after_saving_plan(self):
        self.sdk._store.save_plan("p1", {"task": "test"})
        plans = self.sdk.list_plans()
        self.assertEqual(len(plans), 1)
        self.assertEqual(plans[0]["id"], "p1")
        self.assertIn("created_at", plans[0])
        self.assertIn("data", plans[0])

    def test_multiple_plans(self):
        self.sdk._store.save_plan("p1", {"task": "a"})
        self.sdk._store.save_plan("p2", {"task": "b"})
        plans = self.sdk.list_plans()
        self.assertEqual(len(plans), 2)


class GetPlanTests(unittest.TestCase):
    def setUp(self):
        self.sdk = HarnessSDK()

    def tearDown(self):
        self.sdk.close()

    def test_nonexistent_plan(self):
        result = self.sdk.get_plan("nonexistent")
        self.assertIsNone(result)

    def test_existing_plan(self):
        self.sdk._store.save_plan("p1", {"task": "test", "priority": "high"})
        result = self.sdk.get_plan("p1")
        self.assertIsNotNone(result)
        self.assertEqual(result["id"], "p1")
        self.assertEqual(result["data"]["task"], "test")

    def test_plan_has_metadata(self):
        self.sdk._store.save_plan("p1", {"task": "test"})
        result = self.sdk.get_plan("p1")
        self.assertIn("created_at", result)
        self.assertIn("schema_version", result)


class ValidateEventsTests(unittest.TestCase):
    def setUp(self):
        self.sdk = HarnessSDK()

    def tearDown(self):
        self.sdk.close()

    def test_nonexistent_file(self):
        result = self.sdk.validate_events("/nonexistent/path.jsonl")
        self.assertFalse(result["ok"])
        self.assertEqual(result["total"], 0)
        self.assertIn("file not found", result["errors"][0])

    def test_empty_file(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.write("")
            path = f.name
        result = self.sdk.validate_events(path)
        self.assertTrue(result["ok"])
        self.assertEqual(result["total"], 0)
        Path(path).unlink()

    def test_valid_event(self):
        event = {
            "event_id": "e1",
            "schema_version": "event.v1",
            "event_type": "test",
            "timestamp": "2026-01-01T00:00:00Z",
            "producer": {"component_id": "c1", "component_type": "test"},
            "correlation": {},
            "severity": "info",
            "payload": {},
            "idempotency_key": "ik1",
            "parent_event_id": None,
        }
        import json
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.write(json.dumps(event) + "\n")
            path = f.name
        result = self.sdk.validate_events(path)
        self.assertTrue(result["ok"])
        self.assertEqual(result["total"], 1)
        self.assertEqual(result["valid"], 1)
        self.assertEqual(result["invalid"], 0)
        Path(path).unlink()

    def test_invalid_json(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.write("not json at all\n")
            path = f.name
        result = self.sdk.validate_events(path)
        self.assertFalse(result["ok"])
        self.assertEqual(result["total"], 1)
        self.assertEqual(result["invalid"], 1)
        self.assertTrue(any("JSON parse error" in e for e in result["errors"]))
        Path(path).unlink()

    def test_mixed_valid_invalid(self):
        event = {
            "event_id": "e1",
            "schema_version": "event.v1",
            "event_type": "test",
            "timestamp": "2026-01-01T00:00:00Z",
            "producer": {"component_id": "c1", "component_type": "test"},
            "correlation": {},
            "severity": "info",
            "payload": {},
            "idempotency_key": "ik1",
            "parent_event_id": None,
        }
        import json
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.write(json.dumps(event) + "\n")
            f.write("not json\n")
            f.write(json.dumps({"broken": True}) + "\n")
            path = f.name
        result = self.sdk.validate_events(path)
        self.assertFalse(result["ok"])
        self.assertEqual(result["total"], 3)
        self.assertEqual(result["valid"], 1)
        self.assertEqual(result["invalid"], 2)
        Path(path).unlink()


class HealthCheckTests(unittest.TestCase):
    def setUp(self):
        self.sdk = HarnessSDK()

    def tearDown(self):
        self.sdk.close()

    def test_returns_dict(self):
        result = self.sdk.health_check()
        self.assertIsInstance(result, dict)

    def test_has_status_key(self):
        result = self.sdk.health_check()
        self.assertIn("status", result)

    def test_has_checks_key(self):
        result = self.sdk.health_check()
        self.assertIn("checks", result)


class GetStatusTests(unittest.TestCase):
    def setUp(self):
        self.sdk = HarnessSDK()

    def tearDown(self):
        self.sdk.close()

    def test_returns_dict(self):
        result = self.sdk.get_status()
        self.assertIsInstance(result, dict)

    def test_has_schema_version(self):
        result = self.sdk.get_status()
        self.assertEqual(result["schema_version"], SDK_SCHEMA_VERSION)

    def test_has_health(self):
        result = self.sdk.get_status()
        self.assertIn("health", result)

    def test_has_storage_stats(self):
        result = self.sdk.get_status()
        self.assertIn("storage", result)
        self.assertIn("plans", result["storage"])
        self.assertIn("events", result["storage"])

    def test_has_timestamp(self):
        result = self.sdk.get_status()
        self.assertIn("timestamp", result)
        self.assertIsInstance(result["timestamp"], float)


class CloseTests(unittest.TestCase):
    def test_close_twice(self):
        sdk = HarnessSDK()
        sdk.close()
        sdk.close()

    def test_close_in_memory(self):
        sdk = HarnessSDK()
        sdk.close()


if __name__ == "__main__":
    unittest.main()
