"""Tests for dispatch/health_checker.py — health and readiness probes."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.durable_store import DurableStore
from harness_core.dispatch.health_checker import (
    HEALTH_CHECKER_SCHEMA_VERSION,
    HealthCheck,
    HealthChecker,
    HealthReport,
)


class HealthCheckTests(unittest.TestCase):
    def test_fields(self):
        hc = HealthCheck(name="storage", status="healthy", message="ok", latency_ms=1.5)
        self.assertEqual(hc.name, "storage")
        self.assertEqual(hc.latency_ms, 1.5)

    def test_immutable(self):
        hc = HealthCheck(name="x", status="y")
        with self.assertRaises(AttributeError):
            hc.status = "z"  # type: ignore[misc]


class HealthReportTests(unittest.TestCase):
    def test_fields(self):
        hr = HealthReport(status="healthy", checks=[])
        self.assertEqual(hr.status, "healthy")
        self.assertGreater(hr.timestamp, 0)


class CheckStorageTests(unittest.TestCase):
    def test_no_store(self):
        checker = HealthChecker()
        result = checker.check_storage()
        self.assertEqual(result.status, "unhealthy")
        self.assertIn("no store", result.message)

    def test_healthy_storage(self):
        store = DurableStore()
        checker = HealthChecker(store)
        result = checker.check_storage()
        self.assertEqual(result.status, "healthy")
        self.assertIn("plans=", result.message)
        store.close()

    def test_store_with_data(self):
        store = DurableStore()
        store.save_plan("p1", {"task": "a"})
        store.save_repo("r1", {"name": "repo"})
        checker = HealthChecker(store)
        result = checker.check_storage()
        self.assertEqual(result.status, "healthy")
        self.assertIn("plans=1", result.message)
        self.assertIn("repos=1", result.message)
        store.close()

    def test_latency_recorded(self):
        store = DurableStore()
        checker = HealthChecker(store)
        result = checker.check_storage()
        self.assertGreaterEqual(result.latency_ms, 0)
        store.close()


class CheckEventsTests(unittest.TestCase):
    def test_no_store(self):
        checker = HealthChecker()
        result = checker.check_events()
        self.assertEqual(result.status, "unhealthy")

    def test_healthy_events(self):
        store = DurableStore()
        checker = HealthChecker(store)
        result = checker.check_events()
        self.assertEqual(result.status, "healthy")
        self.assertIn("accessible", result.message)
        store.close()

    def test_events_with_data(self):
        store = DurableStore()
        store.save_event("e1", {"event_type": "deploy"})
        checker = HealthChecker(store)
        result = checker.check_events()
        self.assertEqual(result.status, "healthy")
        store.close()


class CheckPlansTests(unittest.TestCase):
    def test_no_store(self):
        checker = HealthChecker()
        result = checker.check_plans()
        self.assertEqual(result.status, "unhealthy")

    def test_healthy_plans(self):
        store = DurableStore()
        checker = HealthChecker(store)
        result = checker.check_plans()
        self.assertEqual(result.status, "healthy")
        self.assertIn("accessible", result.message)
        store.close()

    def test_plans_with_data(self):
        store = DurableStore()
        store.save_plan("p1", {"task": "a"})
        store.save_plan("p2", {"task": "b"})
        checker = HealthChecker(store)
        result = checker.check_plans()
        self.assertIn("count=2", result.message)
        store.close()


class HealthTests(unittest.TestCase):
    def test_all_healthy(self):
        store = DurableStore()
        checker = HealthChecker(store)
        report = checker.health()
        self.assertEqual(report.status, "healthy")
        self.assertEqual(len(report.checks), 3)
        store.close()

    def test_no_store_unhealthy(self):
        checker = HealthChecker()
        report = checker.health()
        self.assertEqual(report.status, "unhealthy")

    def test_health_dict_format(self):
        store = DurableStore()
        checker = HealthChecker(store)
        d = checker.health_dict()
        self.assertEqual(d["status"], "healthy")
        self.assertIn("storage", d["checks"])
        self.assertIn("events", d["checks"])
        self.assertIn("plans", d["checks"])
        self.assertIn("timestamp", d)
        store.close()


class ReadinessTests(unittest.TestCase):
    def test_ready_when_all_healthy(self):
        store = DurableStore()
        checker = HealthChecker(store)
        report = checker.readiness()
        self.assertEqual(report.status, "ready")
        store.close()

    def test_not_ready_when_no_store(self):
        checker = HealthChecker()
        report = checker.readiness()
        self.assertEqual(report.status, "not_ready")

    def test_readiness_dict_format(self):
        store = DurableStore()
        checker = HealthChecker(store)
        d = checker.readiness_dict()
        self.assertTrue(d["ready"])
        self.assertEqual(d["status"], "ready")
        self.assertIn("checks", d)
        store.close()

    def test_readiness_dict_not_ready(self):
        checker = HealthChecker()
        d = checker.readiness_dict()
        self.assertFalse(d["ready"])
        self.assertEqual(d["status"], "not_ready")


class SchemaVersionTests(unittest.TestCase):
    def test_schema_version_defined(self):
        self.assertEqual(HEALTH_CHECKER_SCHEMA_VERSION, "health_checker.v1")


if __name__ == "__main__":
    unittest.main()
