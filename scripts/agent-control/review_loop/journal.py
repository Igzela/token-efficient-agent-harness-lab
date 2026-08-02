"""Append-only transport journal with sequence/hash chaining (pure logic).

The journal is a local operator record, never accepted project evidence.
A projection can be rebuilt from the journal; a tampered or truncated chain
is rejected rather than silently trusted.
"""

from __future__ import annotations

import hashlib
import json
import pathlib
import time
from typing import Iterable

from . import models


def _verify_chain(records: list[models.TransportEvent]) -> None:
    """Reject breakage between seq, prev_sha and each record sha."""
    for index, record in enumerate(records):
        if not record.sha:
            raise ValueError(f"seq {record.seq} has no sha; chain broken")
        if record.seq != index + 1:
            raise ValueError(
                f"sequence gap at seq {record.seq} (expected {index + 1})"
            )
        if index > 0:
            if record.prev_sha != records[index - 1].sha:
                raise ValueError(
                    f"chain break before seq {record.seq}: prev_sha mismatch"
                )
        elif record.prev_sha:
            raise ValueError(f"first record has unexpected prev_sha {record.prev_sha!r}")
        expected = hashlib.sha256(
            record.to_json().replace(
                f'"sha": "{record.sha}"',
                f'"sha": ""',
            ).encode("utf-8")
        ).hexdigest()
        if expected != record.sha:
            raise ValueError(f"seq {record.seq} sha does not verify")


class Journal:
    """Append-only JSONL journal.  The file path is caller-owned (operator side)."""

    def __init__(self, path: pathlib.Path):
        self.path = pathlib.Path(path)

    def _previous_tail(self) -> models.TransportEvent | None:
        if not self.path.exists():
            return None
        with open(self.path, encoding="utf-8") as handle:
            lines = [line for line in handle if line.strip()]
        if not lines:
            return None
        try:
            return models.TransportEvent.from_json(lines[-1])
        except Exception:
            raise ValueError("journal tail is corrupt; refusing to append")

    def _locked_read_all(self, handle) -> list[models.TransportEvent]:
        """Replay the chain while the append lock is held (R2-B5)."""
        handle.seek(0)
        lines = [line for line in handle if line.strip()]
        if not lines:
            return []
        records: list[models.TransportEvent] = []
        for line in lines:
            record = models.TransportEvent.from_json(line)
            records.append(record)
        return records

    def append(self, *, event: str, chat_key: str, request_text_sha256: str, detail: str = "") -> models.TransportEvent:
        # R2-B5: a global flock serializes every append across chats and
        # processes, so two writers can never observe the same tail and
        # produce a chain break.  The lock is held across read + append.
        import fcntl

        self.path.parent.mkdir(parents=True, exist_ok=True)
        with open(self.path, "a+", encoding="utf-8") as handle:
            fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
            try:
                existing = self._locked_read_all(handle)
                if existing:
                    _verify_chain(existing)
                previous = existing[-1] if existing else None
                seq = (previous.seq + 1) if previous else 1
                ts = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
                record = models.TransportEvent(
                    seq=seq,
                    ts=ts,
                    event=event,
                    chat_key=chat_key,
                    request_text_sha256=request_text_sha256,
                    detail=detail,
                    prev_sha=previous.sha if previous else "",
                )
                payload = record.to_json()
                record = models.TransportEvent(
                    seq=seq,
                    ts=ts,
                    event=event,
                    chat_key=chat_key,
                    request_text_sha256=request_text_sha256,
                    detail=detail,
                    prev_sha=record.prev_sha,
                    sha=hashlib.sha256(payload.encode("utf-8")).hexdigest(),
                )
                handle.write(record.to_json() + "\n")
                handle.flush()
                return record
            finally:
                fcntl.flock(handle.fileno(), fcntl.LOCK_UN)

    def replay(self) -> list[models.TransportEvent]:
        """Replay the chain, rejecting breakage between seq and prev_sha."""
        if not self.path.exists():
            return []
        records: list[models.TransportEvent] = []
        with open(self.path, encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if not line:
                    continue
                records.append(models.TransportEvent.from_json(line))
        _verify_chain(records)
        return records

    def projection(self) -> dict[str, object]:
        """Rebuildable non-authoritative projection: latest outcome per chat."""
        events = self.replay()
        latest: dict[str, str] = {}
        by_chat: dict[str, list[models.TransportEvent]] = {}
        for record in events:
            by_chat.setdefault(record.chat_key, []).append(record)
            latest[record.chat_key] = record.event
        return {
            "schema_version": models.JOURNAL_SCHEMA,
            "event_count": len(events),
            "latest_event_per_chat": latest,
            "event_count_per_chat": {key: len(value) for key, value in by_chat.items()},
        }


def serialize_projection(projection: dict[str, object]) -> str:
    return json.dumps(projection, sort_keys=True, indent=2)
