"""Phase 6B-3: BackupManager — Scheduled backups and restore for DurableStore."""

from __future__ import annotations

import hashlib
import json
import shutil
import sqlite3
import threading
import time
import uuid
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any

from .durable_store import DurableStore

BACKUP_MANAGER_SCHEMA_VERSION = "backup_manager.v1"
METADATA_FILENAME = "backup_metadata.json"


@dataclass(frozen=True)
class BackupRecord:
    backup_id: str
    created_at: str
    size_bytes: int
    label: str
    source_path: str
    backup_path: str
    checksum: str


@dataclass(frozen=True)
class RestoreResult:
    success: bool
    records_restored: int
    errors: list[str]
    duration_ms: float


def _compute_checksum(file_path: str | Path) -> str:
    h = hashlib.sha256()
    with open(file_path, "rb") as f:
        for chunk in iter(lambda: f.read(8192), b""):
            h.update(chunk)
    return h.hexdigest()


class BackupManager:
    """Manages backups and restores for DurableStore instances."""

    def __init__(self, backup_dir: str | Path = "backups") -> None:
        self._backup_dir = Path(backup_dir)
        self._backup_dir.mkdir(parents=True, exist_ok=True)
        self._lock = threading.Lock()

    def _metadata_path(self) -> Path:
        return self._backup_dir / METADATA_FILENAME

    def _load_metadata(self) -> dict[str, dict[str, Any]]:
        meta_path = self._metadata_path()
        if meta_path.exists():
            with open(meta_path, "r") as f:
                return json.load(f)
        return {}

    def _save_metadata(self, metadata: dict[str, dict[str, Any]]) -> None:
        meta_path = self._metadata_path()
        tmp_path = meta_path.with_suffix(".tmp")
        with open(tmp_path, "w") as f:
            json.dump(metadata, f, indent=2, sort_keys=True)
        tmp_path.replace(meta_path)

    def _get_store_path(self, store: DurableStore) -> str:
        return store.db_path

    def _checkpoint_wal(self, db_path: Path) -> None:
        conn = sqlite3.connect(str(db_path))
        try:
            conn.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        finally:
            conn.close()

    def _copy_sqlite_files(self, src: Path, dst: Path) -> None:
        shutil.copy2(str(src), str(dst))
        for suffix in (".wal", ".shm"):
            src_sidecar = src.parent / (src.name + suffix)
            if src_sidecar.exists():
                shutil.copy2(str(src_sidecar), str(dst.parent / (dst.name + suffix)))

    def _remove_sqlite_sidecars(self, base: Path) -> None:
        for suffix in (".wal", ".shm"):
            sidecar = base.parent / (base.name + suffix)
            if sidecar.exists():
                sidecar.unlink()

    def create_backup(self, store: DurableStore, label: str = "") -> BackupRecord:
        with self._lock:
            source_path = Path(self._get_store_path(store))
            if not source_path.exists():
                raise FileNotFoundError(f"Source database not found: {source_path}")

            backup_id = str(uuid.uuid4())
            created_at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
            backup_filename = f"{backup_id}.db"
            backup_path = self._backup_dir / backup_filename

            self._checkpoint_wal(source_path)
            self._copy_sqlite_files(source_path, backup_path)
            size_bytes = backup_path.stat().st_size
            checksum = _compute_checksum(backup_path)

            record = BackupRecord(
                backup_id=backup_id,
                created_at=created_at,
                size_bytes=size_bytes,
                label=label,
                source_path=str(source_path),
                backup_path=str(backup_path),
                checksum=checksum,
            )

            metadata = self._load_metadata()
            metadata[backup_id] = asdict(record)
            self._save_metadata(metadata)

            return record

    def list_backups(self) -> list[BackupRecord]:
        with self._lock:
            metadata = self._load_metadata()
        records = []
        for entry in metadata.values():
            records.append(BackupRecord(**entry))
        records.sort(key=lambda r: r.created_at)
        return records

    def get_backup(self, backup_id: str) -> BackupRecord | None:
        with self._lock:
            metadata = self._load_metadata()
        entry = metadata.get(backup_id)
        if entry is None:
            return None
        return BackupRecord(**entry)

    def restore_backup(self, backup_id: str, target_store: DurableStore) -> RestoreResult:
        start = time.monotonic()
        errors: list[str] = []

        with self._lock:
            metadata = self._load_metadata()
            entry = metadata.get(backup_id)
            if entry is None:
                errors.append(f"Backup not found: {backup_id}")
                return RestoreResult(
                    success=False,
                    records_restored=0,
                    errors=errors,
                    duration_ms=0.0,
                )

            backup_path = Path(entry["backup_path"])
            if not backup_path.exists():
                errors.append(f"Backup file missing: {backup_path}")
                return RestoreResult(
                    success=False,
                    records_restored=0,
                    errors=errors,
                    duration_ms=0.0,
                )

            checksum = _compute_checksum(backup_path)
            if checksum != entry["checksum"]:
                errors.append("Checksum mismatch — backup may be corrupted")
                return RestoreResult(
                    success=False,
                    records_restored=0,
                    errors=errors,
                    duration_ms=0.0,
                )

            target_path = Path(self._get_store_path(target_store))

            target_store.close()
            self._remove_sqlite_sidecars(target_path)

            # Atomic restore: copy to temp, then rename over target
            tmp_path = target_path.with_suffix(".restore_tmp")
            self._copy_sqlite_files(backup_path, tmp_path)
            tmp_path.replace(target_path)
            self._remove_sqlite_sidecars(tmp_path)

            # Reopen the store at the same path (constructor calls _ensure_schema)
            restored_store = DurableStore(db_path=str(target_path))
            try:
                stats = restored_store.stats()
                records_restored = stats["plans"] + stats["repos"] + stats["events"]
            finally:
                restored_store.close()

            duration_ms = (time.monotonic() - start) * 1000
            return RestoreResult(
                success=True,
                records_restored=records_restored,
                errors=[],
                duration_ms=duration_ms,
            )

    def delete_backup(self, backup_id: str) -> bool:
        with self._lock:
            metadata = self._load_metadata()
            if backup_id not in metadata:
                return False

            entry = metadata.pop(backup_id)
            backup_path = Path(entry["backup_path"])
            if backup_path.exists():
                backup_path.unlink()

            self._save_metadata(metadata)
            return True
