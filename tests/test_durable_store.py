"""Tests for dispatch/durable_store.py — SQLite-backed durable storage."""

import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.durable_store import (
    DURABLE_STORE_SCHEMA_VERSION,
    DurableStore,
    StoredRecord,
)


class StoredRecordTests(unittest.TestCase):
    def test_fields(self):
        r = StoredRecord(record_id="r1", created_at="2026-01-01T00:00:00Z",
                         schema_version="v1", data={"key": "val"})
        self.assertEqual(r.record_id, "r1")
        self.assertEqual(r.data["key"], "val")

    def test_immutable(self):
        r = StoredRecord(record_id="r1", created_at="t", schema_version=None, data={})
        with self.assertRaises(AttributeError):
            r.record_id = "r2"  # type: ignore[misc]


class DurableStoreInitTests(unittest.TestCase):
    def test_in_memory_store(self):
        store = DurableStore()
        self.assertEqual(store.stats()["plans"], 0)
        store.close()

    def test_file_store(self):
        with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as f:
            db_path = f.name
        store = DurableStore(db_path)
        store.save_plan("p1", {"task": "test"})
        store.close()
        store2 = DurableStore(db_path)
        plan = store2.get_plan("p1")
        self.assertIsNotNone(plan)
        store2.close()
        Path(db_path).unlink(missing_ok=True)

    def test_schema_version_defined(self):
        self.assertEqual(DURABLE_STORE_SCHEMA_VERSION, "durable_store.v1")


class PlanCRUDTests(unittest.TestCase):
    def setUp(self):
        self.store = DurableStore()

    def tearDown(self):
        self.store.close()

    def test_save_and_get_plan(self):
        self.store.save_plan("p1", {"task": "build feature"})
        plan = self.store.get_plan("p1")
        self.assertIsNotNone(plan)
        self.assertEqual(plan.record_id, "p1")  # type: ignore[union-attr]
        self.assertEqual(plan.data["task"], "build feature")  # type: ignore[union-attr]

    def test_get_nonexistent_plan(self):
        self.assertIsNone(self.store.get_plan("nope"))

    def test_list_plans_empty(self):
        self.assertEqual(self.store.list_plans(), [])

    def test_list_plans_multiple(self):
        self.store.save_plan("p1", {"task": "a"})
        self.store.save_plan("p2", {"task": "b"})
        plans = self.store.list_plans()
        self.assertEqual(len(plans), 2)

    def test_list_plans_ordered_by_created_at(self):
        self.store.save_plan("p1", {"task": "a"}, created_at="2026-01-01T00:00:00Z")
        self.store.save_plan("p2", {"task": "b"}, created_at="2026-01-02T00:00:00Z")
        plans = self.store.list_plans()
        self.assertEqual(plans[0].record_id, "p1")
        self.assertEqual(plans[1].record_id, "p2")

    def test_delete_plan(self):
        self.store.save_plan("p1", {"task": "a"})
        self.assertTrue(self.store.delete_plan("p1"))
        self.assertIsNone(self.store.get_plan("p1"))

    def test_delete_nonexistent_plan(self):
        self.assertFalse(self.store.delete_plan("nope"))

    def test_save_plan_upsert(self):
        self.store.save_plan("p1", {"task": "v1"})
        self.store.save_plan("p1", {"task": "v2"}, upsert=True)
        plan = self.store.get_plan("p1")
        self.assertEqual(plan.data["task"], "v2")  # type: ignore[union-attr]

    def test_save_plan_no_upsert_raises(self):
        self.store.save_plan("p1", {"task": "v1"})
        with self.assertRaises(Exception):
            self.store.save_plan("p1", {"task": "v2"})

    def test_plan_with_schema_version(self):
        self.store.save_plan("p1", {"task": "x"}, schema_version="resource_plan.v1")
        plan = self.store.get_plan("p1")
        self.assertEqual(plan.schema_version, "resource_plan.v1")  # type: ignore[union-attr]

    def test_plan_schema_version_from_data(self):
        self.store.save_plan("p1", {"schema_version": "app_plans.v1", "task": "x"})
        plan = self.store.get_plan("p1")
        self.assertEqual(plan.schema_version, "app_plans.v1")  # type: ignore[union-attr]

    def test_plan_created_at_auto(self):
        self.store.save_plan("p1", {"task": "x"})
        plan = self.store.get_plan("p1")
        self.assertIsNotNone(plan.created_at)  # type: ignore[union-attr]

    def test_plan_complex_nested_data(self):
        data = {"steps": [{"action": "analyze"}, {"action": "build"}], "meta": {"risk": "low"}}
        self.store.save_plan("p1", data)
        plan = self.store.get_plan("p1")
        self.assertEqual(len(plan.data["steps"]), 2)  # type: ignore[union-attr]


class RepoCRUDTests(unittest.TestCase):
    def setUp(self):
        self.store = DurableStore()

    def tearDown(self):
        self.store.close()

    def test_save_and_get_repo(self):
        self.store.save_repo("r1", {"name": "my-repo", "kind": "local"})
        repo = self.store.get_repo("r1")
        self.assertIsNotNone(repo)
        self.assertEqual(repo.data["name"], "my-repo")  # type: ignore[union-attr]

    def test_get_nonexistent_repo(self):
        self.assertIsNone(self.store.get_repo("nope"))

    def test_list_repos_empty(self):
        self.assertEqual(self.store.list_repos(), [])

    def test_list_repos_multiple(self):
        self.store.save_repo("r1", {"name": "a"})
        self.store.save_repo("r2", {"name": "b"})
        self.assertEqual(len(self.store.list_repos()), 2)

    def test_delete_repo(self):
        self.store.save_repo("r1", {"name": "a"})
        self.assertTrue(self.store.delete_repo("r1"))
        self.assertIsNone(self.store.get_repo("r1"))

    def test_delete_nonexistent_repo(self):
        self.assertFalse(self.store.delete_repo("nope"))

    def test_repo_upsert(self):
        self.store.save_repo("r1", {"name": "v1"})
        self.store.save_repo("r1", {"name": "v2"}, upsert=True)
        repo = self.store.get_repo("r1")
        self.assertEqual(repo.data["name"], "v2")  # type: ignore[union-attr]

    def test_save_repo_no_upsert_raises(self):
        self.store.save_repo("r1", {"name": "v1"})
        with self.assertRaises(Exception):
            self.store.save_repo("r1", {"name": "v2"})


class EventCRUDTests(unittest.TestCase):
    def setUp(self):
        self.store = DurableStore()

    def tearDown(self):
        self.store.close()

    def test_save_and_get_event(self):
        self.store.save_event("e1", {"event_type": "state_changed", "payload": {}})
        event = self.store.get_event("e1")
        self.assertIsNotNone(event)
        self.assertEqual(event.data["event_type"], "state_changed")  # type: ignore[union-attr]

    def test_get_nonexistent_event(self):
        self.assertIsNone(self.store.get_event("nope"))

    def test_get_events_all(self):
        self.store.save_event("e1", {"event_type": "a"})
        self.store.save_event("e2", {"event_type": "b"})
        events = self.store.get_events()
        self.assertEqual(len(events), 2)

    def test_get_events_by_type(self):
        self.store.save_event("e1", {"event_type": "deploy"})
        self.store.save_event("e2", {"event_type": "build"})
        self.store.save_event("e3", {"event_type": "deploy"})
        events = self.store.get_events(event_type="deploy")
        self.assertEqual(len(events), 2)

    def test_get_events_limit(self):
        for i in range(10):
            self.store.save_event(f"e{i}", {"event_type": "a"})
        events = self.store.get_events(limit=3)
        self.assertEqual(len(events), 3)

    def test_delete_event(self):
        self.store.save_event("e1", {"event_type": "a"})
        self.assertTrue(self.store.delete_event("e1"))
        self.assertIsNone(self.store.get_event("e1"))

    def test_delete_nonexistent_event(self):
        self.assertFalse(self.store.delete_event("nope"))

    def test_event_upsert(self):
        self.store.save_event("e1", {"event_type": "v1"})
        self.store.save_event("e1", {"event_type": "v2"}, upsert=True)
        event = self.store.get_event("e1")
        self.assertEqual(event.data["event_type"], "v2")  # type: ignore[union-attr]

    def test_save_event_no_upsert_raises(self):
        self.store.save_event("e1", {"event_type": "v1"})
        with self.assertRaises(Exception):
            self.store.save_event("e1", {"event_type": "v2"})


class MigrationLogTests(unittest.TestCase):
    def setUp(self):
        self.store = DurableStore()

    def tearDown(self):
        self.store.close()

    def test_log_migration(self):
        mid = self.store.log_migration_start("json", "sqlite")
        self.store.log_migration_finish(mid, records_migrated=42)
        log = self.store.get_migration_log()
        self.assertEqual(len(log), 1)
        self.assertEqual(log[0]["source"], "json")
        self.assertEqual(log[0]["records_migrated"], 42)
        self.assertEqual(log[0]["status"], "completed")

    def test_log_migration_failure(self):
        mid = self.store.log_migration_start("json", "sqlite")
        self.store.log_migration_finish(mid, records_migrated=0, status="failed")
        log = self.store.get_migration_log()
        self.assertEqual(log[0]["status"], "failed")

    def test_multiple_migrations(self):
        self.store.log_migration_start("json", "sqlite")
        self.store.log_migration_start("json", "sqlite")
        log = self.store.get_migration_log()
        self.assertEqual(len(log), 2)


class StatsTests(unittest.TestCase):
    def setUp(self):
        self.store = DurableStore()

    def tearDown(self):
        self.store.close()

    def test_empty_stats(self):
        stats = self.store.stats()
        self.assertEqual(stats["plans"], 0)
        self.assertEqual(stats["repos"], 0)
        self.assertEqual(stats["events"], 0)
        self.assertEqual(stats["migrations"], 0)

    def test_stats_after_saves(self):
        self.store.save_plan("p1", {"task": "a"})
        self.store.save_plan("p2", {"task": "b"})
        self.store.save_repo("r1", {"name": "repo"})
        self.store.save_event("e1", {"event_type": "x"})
        stats = self.store.stats()
        self.assertEqual(stats["plans"], 2)
        self.assertEqual(stats["repos"], 1)
        self.assertEqual(stats["events"], 1)


class ThreadSafetyTests(unittest.TestCase):
    def test_concurrent_saves(self):
        import threading
        with tempfile.NamedTemporaryFile(suffix=".db", delete=False) as f:
            db_path = f.name
        store = DurableStore(db_path)
        errors: list[Exception] = []

        def save_plan(i: int) -> None:
            try:
                store.save_plan(f"p{i}", {"task": f"task {i}"})
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=save_plan, args=(i,)) for i in range(20)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        self.assertEqual(errors, [])
        self.assertEqual(store.stats()["plans"], 20)
        store.close()
        Path(db_path).unlink(missing_ok=True)


if __name__ == "__main__":
    unittest.main()
