"""Phase 6A: StorageMigrator — JSON/JSONL → SQLite migration."""

from __future__ import annotations

import json
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


STORAGE_MIGRATOR_SCHEMA_VERSION = "storage_migrator.v1"


@dataclass(frozen=True)
class MigrationReport:
    source: str
    target: str
    records_migrated: int
    errors: list[str]
    duration_ms: float


@dataclass(frozen=True)
class FullMigrationReport:
    plans: MigrationReport
    repos: MigrationReport
    events: MigrationReport
    total_duration_ms: float


def _read_json_file(path: Path) -> dict[str, Any] | None:
    if not path.exists():
        return None
    try:
        return json.loads(path.read_text())
    except (json.JSONDecodeError, OSError):
        return None


def _read_jsonl_file(path: Path) -> tuple[list[dict[str, Any]], list[str]]:
    if not path.exists():
        return [], []
    records = []
    errors = []
    for line_num, raw_line in enumerate(path.read_text().splitlines(), start=1):
        line = raw_line.strip()
        if not line:
            continue
        try:
            records.append(json.loads(line))
        except json.JSONDecodeError as e:
            errors.append(f"line {line_num}: {e.msg} (near {e.doc[max(0, e.pos-20):e.pos+20]!r})")
    return records, errors


def migrate_plans_json_to_sqlite(
    json_path: Path,
    store: Any,
) -> MigrationReport:
    """Migrate plans from a JSON file to a DurableStore."""
    start = time.monotonic()
    errors: list[str] = []
    migrated = 0

    data = _read_json_file(json_path)
    if data is None:
        return MigrationReport(
            source=str(json_path), target="sqlite",
            records_migrated=0, errors=["file not found or invalid"],
            duration_ms=(time.monotonic() - start) * 1000,
        )

    plans = data.get("plans", [])
    for plan in plans:
        plan_id = plan.get("plan_id")
        if not plan_id:
            errors.append(f"plan missing plan_id: {json.dumps(plan)[:100]}")
            continue
        try:
            store.save_plan(plan_id, plan, schema_version=plan.get("schema_version"), upsert=True)
            migrated += 1
        except Exception as e:
            errors.append(f"plan {plan_id}: {e}")

    return MigrationReport(
        source=str(json_path), target="sqlite",
        records_migrated=migrated, errors=errors,
        duration_ms=(time.monotonic() - start) * 1000,
    )


def migrate_repos_json_to_sqlite(
    json_path: Path,
    store: Any,
) -> MigrationReport:
    """Migrate repos from a JSON file to a DurableStore."""
    start = time.monotonic()
    errors: list[str] = []
    migrated = 0

    data = _read_json_file(json_path)
    if data is None:
        return MigrationReport(
            source=str(json_path), target="sqlite",
            records_migrated=0, errors=["file not found or invalid"],
            duration_ms=(time.monotonic() - start) * 1000,
        )

    repos = data.get("repos", [])
    for repo in repos:
        repo_id = repo.get("id")
        if not repo_id:
            errors.append(f"repo missing id: {json.dumps(repo)[:100]}")
            continue
        try:
            store.save_repo(repo_id, repo, schema_version=repo.get("schema_version"), upsert=True)
            migrated += 1
        except Exception as e:
            errors.append(f"repo {repo_id}: {e}")

    return MigrationReport(
        source=str(json_path), target="sqlite",
        records_migrated=migrated, errors=errors,
        duration_ms=(time.monotonic() - start) * 1000,
    )


def migrate_events_jsonl_to_sqlite(
    jsonl_path: Path,
    store: Any,
) -> MigrationReport:
    """Migrate events from a JSONL file to a DurableStore."""
    start = time.monotonic()
    errors: list[str] = []
    migrated = 0

    events, parse_errors = _read_jsonl_file(jsonl_path)
    errors.extend(parse_errors)
    if not events:
        return MigrationReport(
            source=str(jsonl_path), target="sqlite",
            records_migrated=0, errors=errors,
            duration_ms=(time.monotonic() - start) * 1000,
        )

    for event in events:
        event_id = event.get("event_id")
        if not event_id:
            errors.append(f"event missing event_id: {json.dumps(event)[:100]}")
            continue
        try:
            store.save_event(event_id, event, schema_version=event.get("schema_version"), upsert=True)
            migrated += 1
        except Exception as e:
            errors.append(f"event {event_id}: {e}")

    return MigrationReport(
        source=str(jsonl_path), target="sqlite",
        records_migrated=migrated, errors=errors,
        duration_ms=(time.monotonic() - start) * 1000,
    )


def full_migration(
    plans_json: Path,
    repos_json: Path,
    events_jsonl: Path,
    store: Any,
) -> FullMigrationReport:
    """Run all three migrations and produce a combined report."""
    start = time.monotonic()
    plans_report = migrate_plans_json_to_sqlite(plans_json, store)
    repos_report = migrate_repos_json_to_sqlite(repos_json, store)
    events_report = migrate_events_jsonl_to_sqlite(events_jsonl, store)
    return FullMigrationReport(
        plans=plans_report,
        repos=repos_report,
        events=events_report,
        total_duration_ms=(time.monotonic() - start) * 1000,
    )
