"""Tests for dispatch/backup_manager.py — Scheduled backups and restore."""

import json
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.backup_manager import (
    BACKUP_MANAGER_SCHEMA_VERSION,
    BackupManager,
    BackupRecord,
    RestoreResult,
    _compute_checksum,
)
from harness_core.dispatch.durable_store import DurableStore


def _make_temp_db() -> Path:
    f = tempfile.NamedTemporaryFile(suffix=".db", delete=False)
    f.close()
    return Path(f.name)


def _make_temp_backup_dir() -> Path:
    d = tempfile.mkdtemp()
    return Path(d)


class BackupRecordTests(unittest.TestCase):
    def test_fields(self):
        r = BackupRecord(
            backup_id="b1",
            created_at="2026-01-01T00:00:00Z",
            size_bytes=1024,
            label="test",
            source_path="/tmp/src.db",
            backup_path="/tmp/backup.db",
            checksum="abc123",
        )
        self.assertEqual(r.backup_id, "b1")
        self.assertEqual(r.label, "test")

    def test_immutable(self):
        r = BackupRecord(
            backup_id="b1",
            created_at="t",
            size_bytes=0,
            label="",
            source_path="",
            backup_path="",
            checksum="",
        )
        with self.assertRaises(AttributeError):
            r.backup_id = "b2"  # type: ignore[misc]


class RestoreResultTests(unittest.TestCase):
    def test_fields(self):
        r = RestoreResult(success=True, records_restored=5, errors=[], duration_ms=12.3)
        self.assertTrue(r.success)
        self.assertEqual(r.records_restored, 5)
        self.assertEqual(r.duration_ms, 12.3)

    def test_immutable(self):
        r = RestoreResult(success=False, records_restored=0, errors=[], duration_ms=0.0)
        with self.assertRaises(AttributeError):
            r.success = True  # type: ignore[misc]


class SchemaVersionTest(unittest.TestCase):
    def test_version_defined(self):
        self.assertEqual(BACKUP_MANAGER_SCHEMA_VERSION, "backup_manager.v1")


class ChecksumTests(unittest.TestCase):
    def test_checksum_deterministic(self):
        db = _make_temp_db()
        store = DurableStore(db)
        store.save_plan("p1", {"task": "test"})
        store.close()
        c1 = _compute_checksum(db)
        c2 = _compute_checksum(db)
        self.assertEqual(c1, c2)
        db.unlink(missing_ok=True)

    def test_checksum_hex_string(self):
        db = _make_temp_db()
        store = DurableStore(db)
        store.save_plan("p1", {"task": "test"})
        store.close()
        c = _compute_checksum(db)
        self.assertEqual(len(c), 64)
        int(c, 16)  # should not raise
        db.unlink(missing_ok=True)


class CreateBackupTests(unittest.TestCase):
    def setUp(self):
        self.backup_dir = _make_temp_backup_dir()
        self.bm = BackupManager(self.backup_dir)
        self.db = _make_temp_db()

    def tearDown(self):
        self.db.unlink(missing_ok=True)
        import shutil
        shutil.rmtree(self.backup_dir, ignore_errors=True)

    def test_creates_valid_record(self):
        store = DurableStore(self.db)
        record = self.bm.create_backup(store, label="test")
        store.close()
        self.assertIsInstance(record, BackupRecord)
        self.assertTrue(record.backup_id)
        self.assertEqual(record.label, "test")
        self.assertGreater(record.size_bytes, 0)
        self.assertEqual(len(record.checksum), 64)

    def test_backup_file_exists(self):
        store = DurableStore(self.db)
        record = self.bm.create_backup(store)
        store.close()
        self.assertTrue(Path(record.backup_path).exists())

    def test_checksum_matches_file(self):
        store = DurableStore(self.db)
        record = self.bm.create_backup(store)
        store.close()
        actual = _compute_checksum(record.backup_path)
        self.assertEqual(record.checksum, actual)

    def test_source_path_recorded(self):
        store = DurableStore(self.db)
        record = self.bm.create_backup(store)
        store.close()
        self.assertEqual(record.source_path, str(self.db))

    def test_backup_with_empty_label(self):
        store = DurableStore(self.db)
        record = self.bm.create_backup(store)
        store.close()
        self.assertEqual(record.label, "")

    def test_raises_on_missing_source(self):
        missing = self.backup_dir / "nonexistent.db"
        store = DurableStore(missing)
        missing.unlink(missing_ok=True)
        with self.assertRaises(FileNotFoundError):
            self.bm.create_backup(store)


class ListBackupsTests(unittest.TestCase):
    def setUp(self):
        self.backup_dir = _make_temp_backup_dir()
        self.bm = BackupManager(self.backup_dir)
        self.db = _make_temp_db()

    def tearDown(self):
        self.db.unlink(missing_ok=True)
        import shutil
        shutil.rmtree(self.backup_dir, ignore_errors=True)

    def test_empty_when_no_backups(self):
        self.assertEqual(self.bm.list_backups(), [])

    def test_returns_all_backups(self):
        store = DurableStore(self.db)
        self.bm.create_backup(store, label="first")
        self.bm.create_backup(store, label="second")
        store.close()
        backups = self.bm.list_backups()
        self.assertEqual(len(backups), 2)

    def test_sorted_by_created_at(self):
        store = DurableStore(self.db)
        self.bm.create_backup(store, label="a")
        self.bm.create_backup(store, label="b")
        store.close()
        backups = self.bm.list_backups()
        self.assertEqual(len(backups), 2)
        self.assertTrue(backups[0].created_at <= backups[1].created_at)


class GetBackupTests(unittest.TestCase):
    def setUp(self):
        self.backup_dir = _make_temp_backup_dir()
        self.bm = BackupManager(self.backup_dir)
        self.db = _make_temp_db()

    def tearDown(self):
        self.db.unlink(missing_ok=True)
        import shutil
        shutil.rmtree(self.backup_dir, ignore_errors=True)

    def test_returns_correct_record(self):
        store = DurableStore(self.db)
        created = self.bm.create_backup(store, label="findme")
        store.close()
        found = self.bm.get_backup(created.backup_id)
        self.assertIsNotNone(found)
        self.assertEqual(found.backup_id, created.backup_id)
        self.assertEqual(found.label, "findme")

    def test_returns_none_for_missing(self):
        self.assertIsNone(self.bm.get_backup("nonexistent-id"))


class RestoreBackupTests(unittest.TestCase):
    def setUp(self):
        self.backup_dir = _make_temp_backup_dir()
        self.bm = BackupManager(self.backup_dir)

    def tearDown(self):
        import shutil
        shutil.rmtree(self.backup_dir, ignore_errors=True)

    def test_restore_recovers_data(self):
        src_db = _make_temp_db()
        store = DurableStore(src_db)
        store.save_plan("p1", {"task": "important"})
        record = self.bm.create_backup(store)
        store.close()

        store.save_plan("p2", {"task": "added later"})
        store.close()

        result = self.bm.restore_backup(record.backup_id, DurableStore(src_db))
        self.assertTrue(result.success)
        self.assertGreater(result.records_restored, 0)

        restored = DurableStore(src_db)
        plan = restored.get_plan("p1")
        restored.close()
        self.assertIsNotNone(plan)
        self.assertEqual(plan.data["task"], "important")
        src_db.unlink(missing_ok=True)

    def test_restore_returns_error_for_missing_id(self):
        target = DurableStore(_make_temp_db())
        result = self.bm.restore_backup("no-such-id", target)
        target.close()
        self.assertFalse(result.success)
        self.assertEqual(len(result.errors), 1)
        self.assertIn("Backup not found", result.errors[0])

    def test_restore_checksum_mismatch(self):
        db = _make_temp_db()
        store = DurableStore(db)
        record = self.bm.create_backup(store)
        store.close()

        bp = Path(record.backup_path)
        bp.write_bytes(b"corrupted data here, definitely not a valid db")

        result = self.bm.restore_backup(record.backup_id, DurableStore(db))
        self.assertFalse(result.success)
        self.assertTrue(any("Checksum" in e for e in result.errors))
        db.unlink(missing_ok=True)

    def test_restore_result_has_duration(self):
        db = _make_temp_db()
        store = DurableStore(db)
        record = self.bm.create_backup(store)
        store.close()
        result = self.bm.restore_backup(record.backup_id, DurableStore(db))
        self.assertGreater(result.duration_ms, 0.0)
        db.unlink(missing_ok=True)


class DeleteBackupTests(unittest.TestCase):
    def setUp(self):
        self.backup_dir = _make_temp_backup_dir()
        self.bm = BackupManager(self.backup_dir)
        self.db = _make_temp_db()

    def tearDown(self):
        self.db.unlink(missing_ok=True)
        import shutil
        shutil.rmtree(self.backup_dir, ignore_errors=True)

    def test_removes_backup_file(self):
        store = DurableStore(self.db)
        record = self.bm.create_backup(store)
        store.close()
        self.assertTrue(self.bm.delete_backup(record.backup_id))
        self.assertFalse(Path(record.backup_path).exists())

    def test_removes_from_metadata(self):
        store = DurableStore(self.db)
        record = self.bm.create_backup(store)
        store.close()
        self.bm.delete_backup(record.backup_id)
        self.assertIsNone(self.bm.get_backup(record.backup_id))

    def test_returns_false_for_missing(self):
        self.assertFalse(self.bm.delete_backup("nonexistent"))


class MultipleBackupsTests(unittest.TestCase):
    def setUp(self):
        self.backup_dir = _make_temp_backup_dir()
        self.bm = BackupManager(self.backup_dir)

    def tearDown(self):
        import shutil
        shutil.rmtree(self.backup_dir, ignore_errors=True)

    def test_independent_backups(self):
        db1 = _make_temp_db()
        db2 = _make_temp_db()

        store1 = DurableStore(db1)
        store1.save_plan("p1", {"source": "db1"})
        r1 = self.bm.create_backup(store1, label="db1-backup")
        store1.close()

        store2 = DurableStore(db2)
        store2.save_plan("p2", {"source": "db2"})
        r2 = self.bm.create_backup(store2, label="db2-backup")
        store2.close()

        backups = self.bm.list_backups()
        self.assertEqual(len(backups), 2)
        self.assertNotEqual(r1.backup_id, r2.backup_id)

        found1 = self.bm.get_backup(r1.backup_id)
        found2 = self.bm.get_backup(r2.backup_id)
        self.assertEqual(found1.label, "db1-backup")
        self.assertEqual(found2.label, "db2-backup")

        db1.unlink(missing_ok=True)
        db2.unlink(missing_ok=True)


class MetadataPersistenceTests(unittest.TestCase):
    def setUp(self):
        self.backup_dir = _make_temp_backup_dir()

    def tearDown(self):
        import shutil
        shutil.rmtree(self.backup_dir, ignore_errors=True)

    def test_metadata_survives_reinitialization(self):
        db = _make_temp_db()
        store = DurableStore(db)
        bm1 = BackupManager(self.backup_dir)
        record = bm1.create_backup(store, label="persist")
        store.close()

        bm2 = BackupManager(self.backup_dir)
        found = bm2.get_backup(record.backup_id)
        self.assertIsNotNone(found)
        self.assertEqual(found.label, "persist")
        db.unlink(missing_ok=True)

    def test_metadata_json_structure(self):
        db = _make_temp_db()
        store = DurableStore(db)
        bm = BackupManager(self.backup_dir)
        record = bm.create_backup(store, label="json-check")
        store.close()

        meta_path = self.backup_dir / "backup_metadata.json"
        self.assertTrue(meta_path.exists())
        with open(meta_path) as f:
            data = json.load(f)
        self.assertIn(record.backup_id, data)
        entry = data[record.backup_id]
        self.assertEqual(entry["label"], "json-check")
        self.assertEqual(entry["size_bytes"], record.size_bytes)
        db.unlink(missing_ok=True)


class RestoreToNewStoreTests(unittest.TestCase):
    def setUp(self):
        self.backup_dir = _make_temp_backup_dir()
        self.bm = BackupManager(self.backup_dir)

    def tearDown(self):
        import shutil
        shutil.rmtree(self.backup_dir, ignore_errors=True)

    def test_restore_preserves_all_data(self):
        src_db = _make_temp_db()
        store = DurableStore(src_db)
        for i in range(5):
            store.save_plan(f"plan-{i}", {"idx": i})
        for i in range(3):
            store.save_event(f"ev-{i}", {"event_type": "test", "idx": i})
        store.close()

        src_store = DurableStore(src_db)
        record = self.bm.create_backup(src_store)
        src_store.close()

        result = self.bm.restore_backup(record.backup_id, DurableStore(src_db))
        self.assertTrue(result.success)
        self.assertEqual(result.records_restored, 8)

        restored = DurableStore(src_db)
        stats = restored.stats()
        restored.close()
        self.assertEqual(stats["plans"], 5)
        self.assertEqual(stats["events"], 3)
        src_db.unlink(missing_ok=True)


class ThreadSafetyTest(unittest.TestCase):
    def test_concurrent_creates(self):
        backup_dir = _make_temp_backup_dir()
        bm = BackupManager(backup_dir)
        db = _make_temp_db()
        store = DurableStore(db)
        results = []

        def create_one():
            r = bm.create_backup(store, label="concurrent")
            results.append(r)

        threads = [__import__("threading").Thread(target=create_one) for _ in range(5)]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        self.assertEqual(len(results), 5)
        ids = {r.backup_id for r in results}
        self.assertEqual(len(ids), 5)
        store.close()
        db.unlink(missing_ok=True)
        import shutil
        shutil.rmtree(backup_dir, ignore_errors=True)


class AtomicRestoreTests(unittest.TestCase):
    def setUp(self):
        self.backup_dir = _make_temp_backup_dir()
        self.bm = BackupManager(self.backup_dir)

    def tearDown(self):
        import shutil
        shutil.rmtree(self.backup_dir, ignore_errors=True)

    def test_restore_uses_atomic_copy(self):
        db = _make_temp_db()
        store = DurableStore(db)
        store.save_plan("p1", {"task": "original"})
        record = self.bm.create_backup(store)
        store.close()

        # Modify the target after backup
        store2 = DurableStore(db)
        store2.save_plan("p2", {"task": "added"})
        store2.close()

        # Restore should atomically replace — no partial state
        result = self.bm.restore_backup(record.backup_id, DurableStore(db))
        self.assertTrue(result.success)

        restored = DurableStore(db)
        plan = restored.get_plan("p1")
        p2 = restored.get_plan("p2")
        restored.close()
        self.assertIsNotNone(plan)
        self.assertIsNone(p2)  # p2 should be gone (restored to backup state)
        db.unlink(missing_ok=True)

    def test_metadata_atomic_write(self):
        db = _make_temp_db()
        store = DurableStore(db)
        self.bm.create_backup(store, label="atomic-meta")
        store.close()

        meta_path = self.backup_dir / "backup_metadata.json"
        tmp_path = self.backup_dir / "backup_metadata.tmp"
        self.assertTrue(meta_path.exists())
        self.assertFalse(tmp_path.exists())  # tmp should be cleaned up after write
        db.unlink(missing_ok=True)

    def test_restore_no_temp_files_left(self):
        db = _make_temp_db()
        store = DurableStore(db)
        store.save_plan("p1", {"task": "test"})
        record = self.bm.create_backup(store)
        store.close()

        result = self.bm.restore_backup(record.backup_id, DurableStore(db))
        self.assertTrue(result.success)

        # No .restore_tmp files should remain
        tmp_files = list(db.parent.glob("*.restore_tmp"))
        self.assertEqual(len(tmp_files), 0)
        db.unlink(missing_ok=True)


class RestoreFailureAtomicityTests(unittest.TestCase):
    def setUp(self):
        self.backup_dir = _make_temp_backup_dir()
        self.bm = BackupManager(self.backup_dir)

    def tearDown(self):
        import shutil
        shutil.rmtree(self.backup_dir, ignore_errors=True)

    def test_copy_failure_leaves_original_unchanged(self):
        db = _make_temp_db()
        store = DurableStore(db)
        store.save_plan("p1", {"task": "original"})
        record = self.bm.create_backup(store)
        store.close()

        original_checksum = _compute_checksum(db)

        # Monkeypatch _copy_sqlite_files to fail on the restore call
        original_copy = self.bm._copy_sqlite_files
        def failing_copy(src, dst):
            raise OSError("Simulated copy failure")
        self.bm._copy_sqlite_files = failing_copy

        result = self.bm.restore_backup(record.backup_id, DurableStore(db))
        self.assertFalse(result.success)

        # Original DB should be unchanged
        current_checksum = _compute_checksum(db)
        self.assertEqual(original_checksum, current_checksum)

        # Original data should still be readable
        store2 = DurableStore(db)
        plan = store2.get_plan("p1")
        store2.close()
        self.assertIsNotNone(plan)
        self.assertEqual(plan.data["task"], "original")
        db.unlink(missing_ok=True)

    def test_copy_failure_cleans_temp_files(self):
        db = _make_temp_db()
        store = DurableStore(db)
        store.save_plan("p1", {"task": "test"})
        record = self.bm.create_backup(store)
        store.close()

        def failing_copy(src, dst):
            # Create a partial temp file before "failing"
            dst.touch()
            raise OSError("Simulated copy failure")
        self.bm._copy_sqlite_files = failing_copy

        result = self.bm.restore_backup(record.backup_id, DurableStore(db))
        self.assertFalse(result.success)

        # Temp files should be cleaned up
        tmp_files = list(db.parent.glob("*.restore_tmp*"))
        self.assertEqual(len(tmp_files), 0)
        db.unlink(missing_ok=True)

    def test_checksum_mismatch_fails_before_target_mutation(self):
        db = _make_temp_db()
        store = DurableStore(db)
        store.save_plan("p1", {"task": "original"})
        record = self.bm.create_backup(store)
        store.close()

        # Modify store after backup so original has different data
        store2 = DurableStore(db)
        store2.save_plan("p2", {"task": "added"})
        store2.close()

        original_checksum = _compute_checksum(db)

        # Monkeypatch _copy_sqlite_files to write garbage to temp (simulating corruption during copy)
        def corrupting_copy(src, dst):
            import shutil as _shutil
            _shutil.copy2(str(src), str(dst))
            # Corrupt the temp file after copy
            with open(str(dst), "wb") as f:
                f.write(b"corrupted data")
        self.bm._copy_sqlite_files = corrupting_copy

        result = self.bm.restore_backup(record.backup_id, DurableStore(db))
        self.assertFalse(result.success)
        self.assertTrue(any("checksum" in e.lower() for e in result.errors))

        # Target should be untouched
        current_checksum = _compute_checksum(db)
        self.assertEqual(original_checksum, current_checksum)

        # Data should still be there
        store3 = DurableStore(db)
        plan = store3.get_plan("p1")
        p2 = store3.get_plan("p2")
        store3.close()
        self.assertIsNotNone(plan)
        self.assertIsNotNone(p2)  # p2 still exists — target wasn't replaced
        db.unlink(missing_ok=True)

    def test_replace_failure_after_sidecar_removal_preserves_target(self):
        db = _make_temp_db()
        store = DurableStore(db)
        store.save_plan("p1", {"task": "original"})
        record = self.bm.create_backup(store)
        store.close()

        # Add post-backup data
        store2 = DurableStore(db)
        store2.save_plan("p2", {"task": "added later"})
        store2.close()

        original_checksum = _compute_checksum(db)

        # Monkeypatch Path.replace to fail after sidecars are removed
        original_replace = Path.replace
        def failing_replace(self_path, target):
            raise OSError("Simulated replace failure")
        Path.replace = failing_replace

        try:
            result = self.bm.restore_backup(record.backup_id, DurableStore(db))
            self.assertFalse(result.success)
        finally:
            Path.replace = original_replace

        # Target DB should still be readable (checkpoint preserved data into main DB)
        store3 = DurableStore(db)
        plan = store3.get_plan("p1")
        p2 = store3.get_plan("p2")
        store3.close()
        self.assertIsNotNone(plan)
        self.assertEqual(plan.data["task"], "original")
        self.assertIsNotNone(p2)  # Post-backup data preserved via WAL checkpoint
        db.unlink(missing_ok=True)


if __name__ == "__main__":
    unittest.main()
