"""Phase 6A: DurableStore — SQLite-backed local durable storage."""

from __future__ import annotations

import json
import sqlite3
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any


DURABLE_STORE_SCHEMA_VERSION = "durable_store.v1"

_DDL = """\
CREATE TABLE IF NOT EXISTS plans (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    schema_version TEXT,
    data TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS repos (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    schema_version TEXT,
    data TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS events (
    id TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    schema_version TEXT,
    event_type TEXT,
    data TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS migration_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at TEXT NOT NULL,
    finished_at TEXT,
    source TEXT NOT NULL,
    target TEXT NOT NULL,
    records_migrated INTEGER DEFAULT 0,
    status TEXT DEFAULT 'running'
);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_created ON events(created_at);
"""


@dataclass(frozen=True)
class StoredRecord:
    record_id: str
    created_at: str
    schema_version: str | None
    data: dict[str, Any]


class DurableStore:
    """SQLite-backed durable store for plans, repos, and events."""

    def __init__(self, db_path: str | Path = ":memory:") -> None:
        self._db_path = str(db_path)
        self._lock = threading.Lock()
        self._conn: sqlite3.Connection | None = None
        self._ensure_schema()

    def _get_conn(self) -> sqlite3.Connection:
        if self._conn is None:
            self._conn = sqlite3.connect(self._db_path, check_same_thread=False)
            self._conn.row_factory = sqlite3.Row
            self._conn.execute("PRAGMA journal_mode=WAL")
            self._conn.execute("PRAGMA foreign_keys=ON")
        return self._conn

    def _ensure_schema(self) -> None:
        conn = self._get_conn()
        conn.executescript(_DDL)
        conn.commit()

    def close(self) -> None:
        if self._conn is not None:
            self._conn.close()
            self._conn = None

    # ── Plans ───────────────────────────────────────────────────────────

    def save_plan(self, plan_id: str, data: dict[str, Any],
                  schema_version: str | None = None,
                  created_at: str | None = None) -> StoredRecord:
        ts = created_at or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        sv = schema_version or data.get("schema_version")
        blob = json.dumps(data, sort_keys=True, default=str)
        with self._lock:
            conn = self._get_conn()
            conn.execute(
                "INSERT OR REPLACE INTO plans (id, created_at, schema_version, data) VALUES (?, ?, ?, ?)",
                (plan_id, ts, sv, blob),
            )
            conn.commit()
        return StoredRecord(record_id=plan_id, created_at=ts, schema_version=sv, data=data)

    def get_plan(self, plan_id: str) -> StoredRecord | None:
        with self._lock:
            conn = self._get_conn()
            row = conn.execute("SELECT * FROM plans WHERE id = ?", (plan_id,)).fetchone()
        if row is None:
            return None
        return StoredRecord(
            record_id=row["id"],
            created_at=row["created_at"],
            schema_version=row["schema_version"],
            data=json.loads(row["data"]),
        )

    def list_plans(self) -> list[StoredRecord]:
        with self._lock:
            conn = self._get_conn()
            rows = conn.execute("SELECT * FROM plans ORDER BY created_at").fetchall()
        return [
            StoredRecord(
                record_id=r["id"],
                created_at=r["created_at"],
                schema_version=r["schema_version"],
                data=json.loads(r["data"]),
            )
            for r in rows
        ]

    def delete_plan(self, plan_id: str) -> bool:
        with self._lock:
            conn = self._get_conn()
            cursor = conn.execute("DELETE FROM plans WHERE id = ?", (plan_id,))
            conn.commit()
        return cursor.rowcount > 0

    # ── Repos ───────────────────────────────────────────────────────────

    def save_repo(self, repo_id: str, data: dict[str, Any],
                  schema_version: str | None = None,
                  created_at: str | None = None) -> StoredRecord:
        ts = created_at or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        sv = schema_version or data.get("schema_version")
        blob = json.dumps(data, sort_keys=True, default=str)
        with self._lock:
            conn = self._get_conn()
            conn.execute(
                "INSERT OR REPLACE INTO repos (id, created_at, schema_version, data) VALUES (?, ?, ?, ?)",
                (repo_id, ts, sv, blob),
            )
            conn.commit()
        return StoredRecord(record_id=repo_id, created_at=ts, schema_version=sv, data=data)

    def get_repo(self, repo_id: str) -> StoredRecord | None:
        with self._lock:
            conn = self._get_conn()
            row = conn.execute("SELECT * FROM repos WHERE id = ?", (repo_id,)).fetchone()
        if row is None:
            return None
        return StoredRecord(
            record_id=row["id"],
            created_at=row["created_at"],
            schema_version=row["schema_version"],
            data=json.loads(row["data"]),
        )

    def list_repos(self) -> list[StoredRecord]:
        with self._lock:
            conn = self._get_conn()
            rows = conn.execute("SELECT * FROM repos ORDER BY created_at").fetchall()
        return [
            StoredRecord(
                record_id=r["id"],
                created_at=r["created_at"],
                schema_version=r["schema_version"],
                data=json.loads(r["data"]),
            )
            for r in rows
        ]

    def delete_repo(self, repo_id: str) -> bool:
        with self._lock:
            conn = self._get_conn()
            cursor = conn.execute("DELETE FROM repos WHERE id = ?", (repo_id,))
            conn.commit()
        return cursor.rowcount > 0

    # ── Events ──────────────────────────────────────────────────────────

    def save_event(self, event_id: str, data: dict[str, Any],
                   schema_version: str | None = None,
                   created_at: str | None = None) -> StoredRecord:
        ts = created_at or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        sv = schema_version or data.get("schema_version")
        event_type = data.get("event_type", "")
        blob = json.dumps(data, sort_keys=True, default=str)
        with self._lock:
            conn = self._get_conn()
            conn.execute(
                "INSERT OR REPLACE INTO events (id, created_at, schema_version, event_type, data) VALUES (?, ?, ?, ?, ?)",
                (event_id, ts, sv, event_type, blob),
            )
            conn.commit()
        return StoredRecord(record_id=event_id, created_at=ts, schema_version=sv, data=data)

    def get_event(self, event_id: str) -> StoredRecord | None:
        with self._lock:
            conn = self._get_conn()
            row = conn.execute("SELECT * FROM events WHERE id = ?", (event_id,)).fetchone()
        if row is None:
            return None
        return StoredRecord(
            record_id=row["id"],
            created_at=row["created_at"],
            schema_version=row["schema_version"],
            data=json.loads(row["data"]),
        )

    def get_events(self, event_type: str | None = None,
                   limit: int = 100) -> list[StoredRecord]:
        with self._lock:
            conn = self._get_conn()
            if event_type is not None:
                rows = conn.execute(
                    "SELECT * FROM events WHERE event_type = ? ORDER BY created_at DESC LIMIT ?",
                    (event_type, limit),
                ).fetchall()
            else:
                rows = conn.execute(
                    "SELECT * FROM events ORDER BY created_at DESC LIMIT ?",
                    (limit,),
                ).fetchall()
        return [
            StoredRecord(
                record_id=r["id"],
                created_at=r["created_at"],
                schema_version=r["schema_version"],
                data=json.loads(r["data"]),
            )
            for r in rows
        ]

    def delete_event(self, event_id: str) -> bool:
        with self._lock:
            conn = self._get_conn()
            cursor = conn.execute("DELETE FROM events WHERE id = ?", (event_id,))
            conn.commit()
        return cursor.rowcount > 0

    # ── Migration Log ───────────────────────────────────────────────────

    def log_migration_start(self, source: str, target: str) -> int:
        with self._lock:
            conn = self._get_conn()
            cursor = conn.execute(
                "INSERT INTO migration_log (started_at, source, target) VALUES (?, ?, ?)",
                (time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), source, target),
            )
            conn.commit()
        return cursor.lastrowid  # type: ignore[return-value]

    def log_migration_finish(self, migration_id: int, records_migrated: int,
                             status: str = "completed") -> None:
        with self._lock:
            conn = self._get_conn()
            conn.execute(
                "UPDATE migration_log SET finished_at = ?, records_migrated = ?, status = ? WHERE id = ?",
                (time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), records_migrated, status, migration_id),
            )
            conn.commit()

    def get_migration_log(self) -> list[dict[str, Any]]:
        with self._lock:
            conn = self._get_conn()
            rows = conn.execute("SELECT * FROM migration_log ORDER BY started_at").fetchall()
        return [dict(r) for r in rows]

    # ── Stats ───────────────────────────────────────────────────────────

    def stats(self) -> dict[str, int]:
        with self._lock:
            conn = self._get_conn()
            return {
                "plans": conn.execute("SELECT COUNT(*) FROM plans").fetchone()[0],
                "repos": conn.execute("SELECT COUNT(*) FROM repos").fetchone()[0],
                "events": conn.execute("SELECT COUNT(*) FROM events").fetchone()[0],
                "migrations": conn.execute("SELECT COUNT(*) FROM migration_log").fetchone()[0],
            }
