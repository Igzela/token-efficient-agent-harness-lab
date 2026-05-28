"""Tests for dispatch/storage_migrator.py — JSON/JSONL → SQLite migration."""

import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.durable_store import DurableStore
from harness_core.dispatch.storage_migrator import (
    FullMigrationReport,
    MigrationReport,
    STORAGE_MIGRATOR_SCHEMA_VERSION,
    _read_json_file,
    _read_jsonl_file,
    full_migration,
    migrate_events_jsonl_to_sqlite,
    migrate_plans_json_to_sqlite,
    migrate_repos_json_to_sqlite,
)


class MigrationReportTests(unittest.TestCase):
    def test_fields(self):
        r = MigrationReport(
            source="a.json", target="sqlite",
            records_migrated=5, errors=["e1"], duration_ms=10.0,
        )
        self.assertEqual(r.records_migrated, 5)
        self.assertEqual(r.source, "a.json")

    def test_immutable(self):
        r = MigrationReport(source="a", target="b", records_migrated=0, errors=[], duration_ms=0.0)
        with self.assertRaises(AttributeError):
            r.records_migrated = 1  # type: ignore[misc]


class FullMigrationReportTests(unittest.TestCase):
    def test_fields(self):
        r = FullMigrationReport(
            plans=MigrationReport("a", "b", 1, [], 1.0),
            repos=MigrationReport("c", "d", 2, [], 2.0),
            events=MigrationReport("e", "f", 3, [], 3.0),
            total_duration_ms=6.0,
        )
        self.assertEqual(r.plans.records_migrated, 1)
        self.assertEqual(r.total_duration_ms, 6.0)


class ReadJsonFileTests(unittest.TestCase):
    def test_read_valid(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump({"key": "value"}, f)
            f.flush()
            result = _read_json_file(Path(f.name))
        self.assertEqual(result, {"key": "value"})
        Path(f.name).unlink(missing_ok=True)

    def test_read_nonexistent(self):
        self.assertIsNone(_read_json_file(Path("/nonexistent/file.json")))

    def test_read_invalid_json(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            f.write("not json{{{")
            f.flush()
            result = _read_json_file(Path(f.name))
        self.assertIsNone(result)
        Path(f.name).unlink(missing_ok=True)


class ReadJsonlFileTests(unittest.TestCase):
    def test_read_valid(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.write('{"id": "1"}\n{"id": "2"}\n')
            f.flush()
            records, errors = _read_jsonl_file(Path(f.name))
        self.assertEqual(len(records), 2)
        self.assertEqual(errors, [])
        Path(f.name).unlink(missing_ok=True)

    def test_read_nonexistent(self):
        records, errors = _read_jsonl_file(Path("/nonexistent/file.jsonl"))
        self.assertEqual(records, [])
        self.assertEqual(errors, [])

    def test_read_with_blank_lines(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.write('{"id": "1"}\n\n{"id": "2"}\n')
            f.flush()
            records, errors = _read_jsonl_file(Path(f.name))
        self.assertEqual(len(records), 2)
        self.assertEqual(errors, [])
        Path(f.name).unlink(missing_ok=True)

    def test_read_with_invalid_line(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.write('{"id": "1"}\nnot json\n{"id": "2"}\n')
            f.flush()
            records, errors = _read_jsonl_file(Path(f.name))
        self.assertEqual(len(records), 2)
        self.assertEqual(len(errors), 1)
        self.assertIn("line 2", errors[0])
        Path(f.name).unlink(missing_ok=True)

    def test_read_multiple_invalid_lines(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.write('not json 1\n{"id": "1"}\nnot json 2\n')
            f.flush()
            records, errors = _read_jsonl_file(Path(f.name))
        self.assertEqual(len(records), 1)
        self.assertEqual(len(errors), 2)
        self.assertIn("line 1", errors[0])
        self.assertIn("line 3", errors[1])
        Path(f.name).unlink(missing_ok=True)


class MigratePlansTests(unittest.TestCase):
    def setUp(self):
        self.store = DurableStore()

    def tearDown(self):
        self.store.close()

    def test_migrate_valid_plans(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump({
                "schema_version": "app_plans.v1",
                "plans": [
                    {"plan_id": "p1", "task": {"objective": "build"}, "schema_version": "resource_plan.v1"},
                    {"plan_id": "p2", "task": {"objective": "test"}},
                ],
            }, f)
            f.flush()
            report = migrate_plans_json_to_sqlite(Path(f.name), self.store)
        self.assertEqual(report.records_migrated, 2)
        self.assertEqual(report.errors, [])
        self.assertIsNotNone(self.store.get_plan("p1"))
        self.assertIsNotNone(self.store.get_plan("p2"))
        Path(f.name).unlink(missing_ok=True)

    def test_migrate_missing_plan_id(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump({"plans": [{"task": "no id"}]}, f)
            f.flush()
            report = migrate_plans_json_to_sqlite(Path(f.name), self.store)
        self.assertEqual(report.records_migrated, 0)
        self.assertEqual(len(report.errors), 1)
        Path(f.name).unlink(missing_ok=True)

    def test_migrate_nonexistent_file(self):
        report = migrate_plans_json_to_sqlite(Path("/nonexistent.json"), self.store)
        self.assertEqual(report.records_migrated, 0)
        self.assertIn("file not found", report.errors[0])

    def test_migrate_preserves_schema_version(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump({"plans": [{"plan_id": "p1", "schema_version": "custom.v2"}]}, f)
            f.flush()
            migrate_plans_json_to_sqlite(Path(f.name), self.store)
        plan = self.store.get_plan("p1")
        self.assertEqual(plan.schema_version, "custom.v2")  # type: ignore[union-attr]
        Path(f.name).unlink(missing_ok=True)


class MigrateReposTests(unittest.TestCase):
    def setUp(self):
        self.store = DurableStore()

    def tearDown(self):
        self.store.close()

    def test_migrate_valid_repos(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump({
                "repos": [
                    {"id": "r1", "name": "repo-1", "kind": "local"},
                    {"id": "r2", "name": "repo-2", "kind": "remote"},
                ],
            }, f)
            f.flush()
            report = migrate_repos_json_to_sqlite(Path(f.name), self.store)
        self.assertEqual(report.records_migrated, 2)
        self.assertEqual(report.errors, [])
        self.assertIsNotNone(self.store.get_repo("r1"))
        Path(f.name).unlink(missing_ok=True)

    def test_migrate_missing_repo_id(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
            json.dump({"repos": [{"name": "no id"}]}, f)
            f.flush()
            report = migrate_repos_json_to_sqlite(Path(f.name), self.store)
        self.assertEqual(report.records_migrated, 0)
        Path(f.name).unlink(missing_ok=True)

    def test_migrate_nonexistent_file(self):
        report = migrate_repos_json_to_sqlite(Path("/nonexistent.json"), self.store)
        self.assertEqual(report.records_migrated, 0)


class MigrateEventsTests(unittest.TestCase):
    def setUp(self):
        self.store = DurableStore()

    def tearDown(self):
        self.store.close()

    def test_migrate_valid_events(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.write(json.dumps({"event_id": "e1", "event_type": "deploy", "schema_version": "event.v1"}) + "\n")
            f.write(json.dumps({"event_id": "e2", "event_type": "build"}) + "\n")
            f.flush()
            report = migrate_events_jsonl_to_sqlite(Path(f.name), self.store)
        self.assertEqual(report.records_migrated, 2)
        self.assertEqual(report.errors, [])
        self.assertIsNotNone(self.store.get_event("e1"))
        Path(f.name).unlink(missing_ok=True)

    def test_migrate_missing_event_id(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.write(json.dumps({"event_type": "deploy"}) + "\n")
            f.flush()
            report = migrate_events_jsonl_to_sqlite(Path(f.name), self.store)
        self.assertEqual(report.records_migrated, 0)
        Path(f.name).unlink(missing_ok=True)

    def test_migrate_nonexistent_file(self):
        report = migrate_events_jsonl_to_sqlite(Path("/nonexistent.jsonl"), self.store)
        self.assertEqual(report.records_migrated, 0)

    def test_migrate_empty_file(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.flush()
            report = migrate_events_jsonl_to_sqlite(Path(f.name), self.store)
        self.assertEqual(report.records_migrated, 0)
        self.assertEqual(report.errors, [])
        Path(f.name).unlink(missing_ok=True)

    def test_migrate_with_malformed_lines_reports_errors(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".jsonl", delete=False) as f:
            f.write('{"event_id": "e1"}\n')
            f.write('not valid json {{{\n')
            f.write('{"event_id": "e2"}\n')
            f.flush()
            report = migrate_events_jsonl_to_sqlite(Path(f.name), self.store)
        self.assertEqual(report.records_migrated, 2)
        self.assertEqual(len(report.errors), 1)
        self.assertIn("line 2", report.errors[0])
        Path(f.name).unlink(missing_ok=True)


class FullMigrationTests(unittest.TestCase):
    def test_full_migration(self):
        store = DurableStore()
        with tempfile.TemporaryDirectory() as tmpdir:
            plans_path = Path(tmpdir) / "plans.json"
            repos_path = Path(tmpdir) / "repos.json"
            events_path = Path(tmpdir) / "events.jsonl"

            plans_path.write_text(json.dumps({
                "plans": [{"plan_id": "p1", "task": {"obj": "a"}}],
            }))
            repos_path.write_text(json.dumps({
                "repos": [{"id": "r1", "name": "repo"}],
            }))
            events_path.write_text(json.dumps({
                "event_id": "e1", "event_type": "created",
            }) + "\n")

            report = full_migration(plans_path, repos_path, events_path, store)

        self.assertIsInstance(report, FullMigrationReport)
        self.assertEqual(report.plans.records_migrated, 1)
        self.assertEqual(report.repos.records_migrated, 1)
        self.assertEqual(report.events.records_migrated, 1)
        self.assertGreater(report.total_duration_ms, 0)
        store.close()

    def test_full_migration_with_errors(self):
        store = DurableStore()
        with tempfile.TemporaryDirectory() as tmpdir:
            plans_path = Path(tmpdir) / "plans.json"
            repos_path = Path(tmpdir) / "repos.json"
            events_path = Path(tmpdir) / "events.jsonl"

            plans_path.write_text(json.dumps({
                "plans": [{"task": "no plan_id"}],
            }))
            repos_path.write_text(json.dumps({"repos": []}))
            events_path.write_text("")

            report = full_migration(plans_path, repos_path, events_path, store)

        self.assertEqual(report.plans.records_migrated, 0)
        self.assertTrue(len(report.plans.errors) > 0)
        store.close()

    def test_full_migration_nonexistent_files(self):
        store = DurableStore()
        report = full_migration(
            Path("/no/plans.json"),
            Path("/no/repos.json"),
            Path("/no/events.jsonl"),
            store,
        )
        self.assertEqual(report.plans.records_migrated, 0)
        self.assertEqual(report.repos.records_migrated, 0)
        self.assertEqual(report.events.records_migrated, 0)
        store.close()


class SchemaVersionTests(unittest.TestCase):
    def test_schema_version_defined(self):
        self.assertEqual(STORAGE_MIGRATOR_SCHEMA_VERSION, "storage_migrator.v1")


if __name__ == "__main__":
    unittest.main()
