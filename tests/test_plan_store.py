"""Tests for plan_store.py — append-only plan persistence."""

import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.plan_store import (
    PlanStoreError,
    get_plan,
    load_plans,
)


def _write_plan_file(path, plans):
    path.write_text(json.dumps({
        "schema_version": "app_plans.v1",
        "plans": plans,
    }, indent=2) + "\n", encoding="utf-8")


class LoadPlansTests(unittest.TestCase):
    def test_missing_file_returns_empty(self):
        with tempfile.TemporaryDirectory() as tmp:
            data = load_plans(Path(tmp) / "missing.json")
            self.assertEqual(data["schema_version"], "app_plans.v1")
            self.assertEqual(data["plans"], [])

    def test_valid_file_loads(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "plans.json"
            _write_plan_file(path, [{"plan_id": "p1"}])
            data = load_plans(path)
            self.assertEqual(len(data["plans"]), 1)

    def test_wrong_schema_version_raises(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "plans.json"
            path.write_text(json.dumps({
                "schema_version": "wrong",
                "plans": [],
            }), encoding="utf-8")
            with self.assertRaises(PlanStoreError):
                load_plans(path)

    def test_invalid_json_raises(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "plans.json"
            path.write_text("not json", encoding="utf-8")
            with self.assertRaises(PlanStoreError):
                load_plans(path)

    def test_plans_not_list_raises(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "plans.json"
            path.write_text(json.dumps({
                "schema_version": "app_plans.v1",
                "plans": "not a list",
            }), encoding="utf-8")
            with self.assertRaises(PlanStoreError):
                load_plans(path)


class GetPlanTests(unittest.TestCase):
    def test_found(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "plans.json"
            _write_plan_file(path, [{"plan_id": "p1", "task_id": "t1"}])
            result = get_plan(path, "p1")
            self.assertIsNotNone(result)
            self.assertEqual(result["plan_id"], "p1")

    def test_not_found(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "plans.json"
            _write_plan_file(path, [{"plan_id": "p1"}])
            result = get_plan(path, "missing")
            self.assertIsNone(result)

    def test_get_plan_from_multiple(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "plans.json"
            _write_plan_file(path, [
                {"plan_id": "p1"},
                {"plan_id": "p2"},
                {"plan_id": "p3"},
            ])
            result = get_plan(path, "p2")
            self.assertEqual(result["plan_id"], "p2")


if __name__ == "__main__":
    unittest.main()
