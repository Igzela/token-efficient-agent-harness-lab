"""Rebuildable SQLite journal for the provider-free Steward.

The journal is an operator-side projection.  It records bounded transition
facts so a service restart can reconstruct what it may inspect next; it is
not the product store, queue, lease, budget, approval, output, audit, or
rollback authority.  Every append is idempotent, hash chained, and checked
against the card state machine while the SQLite write lock is held.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import re
import sqlite3
import time
from typing import Any, Iterable


SCHEMA_VERSION = "steward_journal.v1"
MAX_ID_CHARS = 128
MAX_DETAIL_CHARS = 512
MAX_EVENTS = 100_000
IDENTIFIER = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$")
BRANCH = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._/-]{0,199}$")
SHA256 = re.compile(r"^[0-9a-f]{64}$")
REPOSITORY = re.compile(r"^[A-Za-z0-9_.-]{1,100}/[A-Za-z0-9_.-]{1,100}$")

CARD_STATES = frozenset(
    {
        "QUEUED",
        "RUNNING",
        "VERIFYING",
        "REVIEWING",
        "WAITING_FOR_MERGE",
        "RETRYING",
        "RECONCILED",
        "COMPLETE",
        "BLOCKED",
        "OUTCOME_UNKNOWN",
    }
)
MISSION_STATES = frozenset(
    {
        "IDLE",
        "PROPOSING",
        "WAITING_APPROVAL",
        "RUNNING",
        "VERIFYING",
        "INTEGRATING",
        "COMPLETE",
        "PAUSED_FOR_OWNER",
        "BLOCKED",
    }
)
ALL_STATES = CARD_STATES | MISSION_STATES | {"HEALTHY", "STOPPED"}
# OUTCOME_UNKNOWN is deliberately recoverable: it must remain visible to the
# read-only reconciliation loop and may never be replayed as if it succeeded.
TERMINAL_STATES = frozenset({"COMPLETE", "BLOCKED"})
_EDGES: dict[str | None, frozenset[str]] = {
    None: frozenset({"QUEUED", "BLOCKED"}),
    "QUEUED": frozenset({"RUNNING", "RETRYING", "BLOCKED", "OUTCOME_UNKNOWN"}),
    "RUNNING": frozenset(
        {"VERIFYING", "RETRYING", "BLOCKED", "OUTCOME_UNKNOWN"}
    ),
    "VERIFYING": frozenset(
        {"REVIEWING", "RETRYING", "BLOCKED", "OUTCOME_UNKNOWN"}
    ),
    "REVIEWING": frozenset(
        {
            "REVIEWING",
            "WAITING_FOR_MERGE",
            "COMPLETE",
            "RETRYING",
            "BLOCKED",
            "OUTCOME_UNKNOWN",
        }
    ),
    "WAITING_FOR_MERGE": frozenset(
        {"WAITING_FOR_MERGE", "REVIEWING", "COMPLETE", "BLOCKED"}
    ),
    "RETRYING": frozenset({"QUEUED", "RUNNING", "BLOCKED", "OUTCOME_UNKNOWN"}),
    "RECONCILED": frozenset({"QUEUED", "BLOCKED"}),
    "COMPLETE": frozenset(),
    "BLOCKED": frozenset(),
    "OUTCOME_UNKNOWN": frozenset(
        {
            "RECONCILED",
            "WAITING_FOR_MERGE",
            "COMPLETE",
            "BLOCKED",
            "OUTCOME_UNKNOWN",
        }
    ),
}


class JournalError(RuntimeError):
    """Base class for fail-closed journal errors."""


class JournalCorrupt(JournalError):
    """The journal cannot be replayed without guessing."""


class TransitionRejected(JournalError):
    """A requested state transition is not allowed from the live tail."""

    def __init__(self, current: str | None, observed: str):
        self.current = current
        self.observed = observed
        super().__init__(f"invalid steward transition {current!r} -> {observed!r}")


class IdempotencyConflict(JournalError):
    """An idempotency key was reused for different transition facts."""


@dataclass(frozen=True)
class JournalEvent:
    seq: int
    timestamp: str
    event: str
    idempotency_key: str
    mission_id: str
    stage_id: str
    card_id: str
    attempt: int
    state: str
    detail: str
    data: dict[str, Any]
    prev_sha256: str
    sha256: str

    def unsigned_wire(self) -> dict[str, Any]:
        return {
            "schema_version": SCHEMA_VERSION,
            "seq": self.seq,
            "timestamp": self.timestamp,
            "event": self.event,
            "idempotency_key": self.idempotency_key,
            "mission_id": self.mission_id,
            "stage_id": self.stage_id,
            "card_id": self.card_id,
            "attempt": self.attempt,
            "state": self.state,
            "detail": self.detail,
            "data": self.data,
            "prev_sha256": self.prev_sha256,
            "sha256": "",
        }

    def to_wire(self) -> dict[str, Any]:
        return {**self.unsigned_wire(), "sha256": self.sha256}

    @classmethod
    def from_wire(cls, value: object) -> "JournalEvent":
        if not isinstance(value, dict):
            raise JournalCorrupt("event is not an object")
        required = {
            "schema_version",
            "seq",
            "timestamp",
            "event",
            "idempotency_key",
            "mission_id",
            "stage_id",
            "card_id",
            "attempt",
            "state",
            "detail",
            "data",
            "prev_sha256",
            "sha256",
        }
        if set(value) != required or value["schema_version"] != SCHEMA_VERSION:
            raise JournalCorrupt("event schema is invalid")
        seq = value["seq"]
        attempt = value["attempt"]
        if type(seq) is not int or seq < 1 or type(attempt) is not int or attempt < 0:
            raise JournalCorrupt("event sequence or attempt is invalid")
        strings = (
            ("event", value["event"]),
            ("idempotency_key", value["idempotency_key"]),
            ("mission_id", value["mission_id"]),
            ("stage_id", value["stage_id"]),
            ("state", value["state"]),
            ("detail", value["detail"]),
            ("timestamp", value["timestamp"]),
            ("prev_sha256", value["prev_sha256"]),
            ("sha256", value["sha256"]),
        )
        for field, item in strings:
            if not isinstance(item, str) or len(item) > MAX_DETAIL_CHARS + MAX_ID_CHARS:
                raise JournalCorrupt(f"event {field} is invalid")
        for field in ("event", "idempotency_key", "mission_id", "stage_id", "state"):
            item = value[field]
            if not item or IDENTIFIER.fullmatch(item) is None:
                raise JournalCorrupt(f"event {field} is invalid")
        card_id = value["card_id"]
        if not isinstance(card_id, str) or len(card_id) > MAX_ID_CHARS:
            raise JournalCorrupt("event card_id is invalid")
        if card_id and IDENTIFIER.fullmatch(card_id) is None:
            raise JournalCorrupt("event card_id is invalid")
        if value["state"] not in ALL_STATES:
            raise JournalCorrupt("event state is invalid")
        if value["detail"] and IDENTIFIER.fullmatch(value["detail"]) is None:
            raise JournalCorrupt("event detail is invalid")
        try:
            clean_data = _validate_data(value["data"])
        except JournalError as exc:
            raise JournalCorrupt("event data is invalid") from exc
        if value["prev_sha256"] and SHA256.fullmatch(value["prev_sha256"]) is None:
            raise JournalCorrupt("event prev hash is invalid")
        if SHA256.fullmatch(value["sha256"]) is None:
            raise JournalCorrupt("event hash is invalid")
        return cls(
            seq,
            value["timestamp"],
            value["event"],
            value["idempotency_key"],
            value["mission_id"],
            value["stage_id"],
            card_id,
            attempt,
            value["state"],
            value["detail"],
            clean_data,
            value["prev_sha256"],
            value["sha256"],
        )


def transition_allowed(current: str | None, observed: str) -> bool:
    """Return whether a card may enter ``observed`` from the live tail."""

    return observed in _EDGES.get(current, frozenset())


def _canonical(value: object) -> str:
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"))


def _sha256(value: object) -> str:
    return hashlib.sha256(_canonical(value).encode("utf-8")).hexdigest()


def _now() -> str:
    return datetime.now(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def _validate_data(data: object | None) -> dict[str, Any]:
    if data is None:
        return {}
    if not isinstance(data, dict) or len(data) > 32:
        raise JournalError("journal_data_invalid")
    encoded = _canonical(data)
    if len(encoded.encode("utf-8")) > 16384:
        raise JournalError("journal_data_too_large")

    def _validate_item(val: Any, depth: int = 0) -> None:
        if depth > 5:
            raise JournalError("journal_data_too_deep")
        if val is None or isinstance(val, (bool, int, float, str)):
            return
        if isinstance(val, (list, tuple)):
            if len(val) > 128:
                raise JournalError("journal_data_value_invalid")
            for item in val:
                _validate_item(item, depth + 1)
            return
        if isinstance(val, dict):
            if len(val) > 32:
                raise JournalError("journal_data_value_invalid")
            for k, v in val.items():
                if not isinstance(k, str) or IDENTIFIER.fullmatch(k) is None:
                    raise JournalError("journal_data_key_invalid")
                if any(
                    marker in k.upper()
                    for marker in ("TOKEN", "SECRET", "PASSWORD", "API_KEY", "APIKEY", "CREDENTIAL", "AUTH")
                ):
                    raise JournalError("journal_data_key_credential_shaped")
                _validate_item(v, depth + 1)
            return
        raise JournalError("journal_data_value_invalid")

    for key, value in data.items():
        if not isinstance(key, str) or IDENTIFIER.fullmatch(key) is None:
            raise JournalError("journal_data_key_invalid")
        if any(
            marker in key.upper()
            for marker in ("TOKEN", "SECRET", "PASSWORD", "API_KEY", "APIKEY", "CREDENTIAL", "AUTH")
        ):
            raise JournalError("journal_data_key_credential_shaped")
        _validate_item(value, 0)
    return dict(data)


def _semantic(event: JournalEvent) -> tuple[object, ...]:
    return (
        event.event,
        event.idempotency_key,
        event.mission_id,
        event.stage_id,
        event.card_id,
        event.attempt,
        event.state,
        event.detail,
        _canonical(event.data),
    )


class StewardJournal:
    """Append-only SQLite journal whose projection can be rebuilt from rows."""

    def __init__(self, path: str | Path):
        self.path = Path(path)

    def _connect(self) -> sqlite3.Connection:
        if self.path.is_symlink():
            raise JournalError("journal_symlink_refused")
        self.path.parent.mkdir(parents=True, exist_ok=True)
        connection = sqlite3.connect(self.path, timeout=30.0, isolation_level=None)
        connection.row_factory = sqlite3.Row
        connection.execute("PRAGMA busy_timeout=30000")
        if str(self.path) != ":memory:":
            try:
                connection.execute("PRAGMA journal_mode=WAL")
                connection.execute("PRAGMA synchronous=NORMAL")
            except sqlite3.OperationalError:
                pass
        connection.execute(
            """CREATE TABLE IF NOT EXISTS steward_journal_events (
                seq INTEGER PRIMARY KEY,
                idempotency_key TEXT NOT NULL UNIQUE,
                record_json TEXT NOT NULL
            )"""
        )
        return connection

    def _read_locked(self, connection: sqlite3.Connection) -> list[JournalEvent]:
        rows = connection.execute(
            "SELECT seq, record_json FROM steward_journal_events ORDER BY seq"
        ).fetchall()
        events: list[JournalEvent] = []
        for row in rows:
            try:
                record = JournalEvent.from_wire(json.loads(row["record_json"]))
            except (TypeError, ValueError, json.JSONDecodeError) as exc:
                raise JournalCorrupt("journal record cannot be decoded") from exc
            if record.seq != row["seq"]:
                raise JournalCorrupt("journal row sequence mismatch")
            events.append(record)
        self._verify(events)
        return events

    @staticmethod
    def _verify(events: list[JournalEvent]) -> None:
        if len(events) > MAX_EVENTS:
            raise JournalCorrupt("journal exceeds bounded event count")
        previous = ""
        for index, event in enumerate(events, start=1):
            if event.seq != index or event.prev_sha256 != previous:
                raise JournalCorrupt("journal sequence or chain break")
            if _sha256(event.unsigned_wire()) != event.sha256:
                raise JournalCorrupt(f"journal hash mismatch at seq {event.seq}")
            previous = event.sha256

    def replay(self) -> list[JournalEvent]:
        connection = self._connect()
        try:
            connection.execute("BEGIN")
            events = self._read_locked(connection)
            connection.commit()
            return events
        except Exception:
            connection.rollback()
            raise
        finally:
            connection.close()

    def _latest_state(
        self,
        events: Iterable[JournalEvent],
        mission_id: str,
        stage_id: str,
        card_id: str,
    ) -> str | None:
        latest: str | None = None
        for event in events:
            if (
                event.mission_id == mission_id
                and event.stage_id == stage_id
                and event.card_id == card_id
                and event.state in CARD_STATES
            ):
                latest = event.state
        return latest

    def append(
        self,
        *,
        event: str,
        idempotency_key: str,
        mission_id: str,
        stage_id: str,
        card_id: str = "",
        attempt: int = 0,
        state: str,
        detail: str = "",
        data: dict[str, Any] | None = None,
        enforce_transition: bool = True,
    ) -> JournalEvent:
        """Append one bounded fact or return the identical idempotent fact."""

        values = (event, idempotency_key, mission_id, stage_id, state)
        if any(not isinstance(item, str) or IDENTIFIER.fullmatch(item) is None for item in values):
            raise JournalError("journal_identity_invalid")
        if not isinstance(card_id, str) or len(card_id) > MAX_ID_CHARS or (
            card_id and IDENTIFIER.fullmatch(card_id) is None
        ):
            raise JournalError("journal_card_id_invalid")
        if type(attempt) is not int or attempt < 0:
            raise JournalError("journal_attempt_invalid")
        if state not in ALL_STATES:
            raise JournalError("journal_state_invalid")
        if (
            not isinstance(detail, str)
            or len(detail) > MAX_DETAIL_CHARS
            or (detail and IDENTIFIER.fullmatch(detail) is None)
        ):
            raise JournalError("journal_detail_invalid")
        if not card_id and state not in (MISSION_STATES | {"HEALTHY", "STOPPED"}):
            raise JournalError("card_state_without_card_invalid")
        if state == "HEALTHY" and card_id:
            raise JournalError("heartbeat_card_invalid")
        clean_data = _validate_data(data)

        connection = self._connect()
        try:
            connection.execute("BEGIN IMMEDIATE")
            events = self._read_locked(connection)
            existing_row = connection.execute(
                "SELECT record_json FROM steward_journal_events WHERE idempotency_key = ?",
                (idempotency_key,),
            ).fetchone()
            if existing_row is not None:
                try:
                    existing = JournalEvent.from_wire(json.loads(existing_row["record_json"]))
                except (TypeError, ValueError, json.JSONDecodeError) as exc:
                    raise JournalCorrupt("idempotency record cannot be decoded") from exc
                if _semantic(existing) != (
                    event,
                    idempotency_key,
                    mission_id,
                    stage_id,
                    card_id,
                    attempt,
                    state,
                    detail,
                    _canonical(clean_data),
                ):
                    raise IdempotencyConflict("idempotency key has different transition facts")
                connection.commit()
                return existing
            current = (
                self._latest_state(events, mission_id, stage_id, card_id)
                if card_id
                else None
            )
            if enforce_transition and card_id and not transition_allowed(current, state):
                raise TransitionRejected(current, state)
            previous = events[-1].sha256 if events else ""
            candidate = JournalEvent(
                seq=len(events) + 1,
                timestamp=_now(),
                event=event,
                idempotency_key=idempotency_key,
                mission_id=mission_id,
                stage_id=stage_id,
                card_id=card_id,
                attempt=attempt,
                state=state,
                detail=detail,
                data=clean_data,
                prev_sha256=previous,
                sha256="",
            )
            record = JournalEvent(**{**candidate.__dict__, "sha256": _sha256(candidate.unsigned_wire())})
            connection.execute(
                "INSERT INTO steward_journal_events(seq, idempotency_key, record_json) VALUES (?, ?, ?)",
                (record.seq, record.idempotency_key, _canonical(record.to_wire())),
            )
            connection.commit()
            return record
        except sqlite3.IntegrityError as exc:
            connection.rollback()
            raise JournalError("journal_append_conflict") from exc
        except Exception:
            connection.rollback()
            raise
        finally:
            connection.close()

    def heartbeat(self, *, mission_id: str, idempotency_key: str, detail: str = "tick") -> JournalEvent:
        return self.append(
            event="HEARTBEAT",
            idempotency_key=idempotency_key,
            mission_id=mission_id,
            stage_id="heartbeat",
            state="HEALTHY",
            detail=detail,
            enforce_transition=False,
        )

    def consume_owner_approval(
        self,
        *,
        repository: str,
        mission_id: str,
        approval_id: str,
        proposal_sha256: str,
        accepted_main_sha: str,
    ) -> bool:
        """Record one authenticated approval consumption as replay evidence.

        GitHub remains the approval authority.  This journal entry is only a
        durable, hash-chained deny-on-replay fact, written under the same
        SQLite lock as the rest of the journal so a process restart cannot
        forget an already-consumed approval.
        """

        values = (mission_id, approval_id)
        if any(not isinstance(value, str) or IDENTIFIER.fullmatch(value) is None for value in values):
            raise JournalError("approval_identity_invalid")
        if REPOSITORY.fullmatch(repository) is None:
            raise JournalError("approval_repository_invalid")
        if SHA256.fullmatch(proposal_sha256) is None or re.fullmatch(r"[0-9a-f]{40}", accepted_main_sha) is None:
            raise JournalError("approval_digest_invalid")
        event = "MISSION_APPROVAL_CONSUMED"
        stage_id = "approval"
        detail = "owner_approval_consumed"
        idempotency_key = (
            f"approval-consumed:{hashlib.sha256(repository.encode('utf-8')).hexdigest()[:16]}:{approval_id}"
        )
        data = _validate_data(
            {
                "repository": repository,
                "approval_id": approval_id,
                "proposal_sha256": proposal_sha256,
                "accepted_main_sha": accepted_main_sha,
            }
        )
        semantic = (
            event,
            idempotency_key,
            mission_id,
            stage_id,
            "",
            0,
            "HEALTHY",
            detail,
            _canonical(data),
        )
        connection = self._connect()
        try:
            connection.execute("BEGIN IMMEDIATE")
            events = self._read_locked(connection)
            existing_row = connection.execute(
                "SELECT record_json FROM steward_journal_events WHERE idempotency_key = ?",
                (idempotency_key,),
            ).fetchone()
            if existing_row is not None:
                try:
                    existing = JournalEvent.from_wire(json.loads(existing_row["record_json"]))
                except (TypeError, ValueError, json.JSONDecodeError) as exc:
                    raise JournalCorrupt("approval replay record cannot be decoded") from exc
                if _semantic(existing) != semantic:
                    raise IdempotencyConflict("approval replay key has different facts")
                connection.commit()
                return False
            if len(events) >= MAX_EVENTS:
                raise JournalError("journal_event_limit_exceeded")
            previous = events[-1].sha256 if events else ""
            candidate = JournalEvent(
                seq=len(events) + 1,
                timestamp=_now(),
                event=event,
                idempotency_key=idempotency_key,
                mission_id=mission_id,
                stage_id=stage_id,
                card_id="",
                attempt=0,
                state="HEALTHY",
                detail=detail,
                data=data,
                prev_sha256=previous,
                sha256="",
            )
            record = JournalEvent(**{**candidate.__dict__, "sha256": _sha256(candidate.unsigned_wire())})
            connection.execute(
                "INSERT INTO steward_journal_events(seq, idempotency_key, record_json) VALUES (?, ?, ?)",
                (record.seq, record.idempotency_key, _canonical(record.to_wire())),
            )
            connection.commit()
            return True
        except sqlite3.IntegrityError as exc:
            connection.rollback()
            raise JournalError("approval_replay_append_conflict") from exc
        except Exception:
            connection.rollback()
            raise
        finally:
            connection.close()

    def latest_for_card(
        self,
        card_id: str,
        *,
        mission_id: str | None = None,
        stage_id: str | None = None,
    ) -> JournalEvent | None:
        events = self.replay()
        for event in reversed(events):
            if (
                event.card_id == card_id
                and (mission_id is None or event.mission_id == mission_id)
                and (stage_id is None or event.stage_id == stage_id)
            ):
                return event
        return None

    def stage_binding_for_card(
        self, card_id: str, *, mission_id: str | None = None, stage_id: str | None = None
    ) -> dict[str, Any] | None:
        """Return the latest exact Stage PR binding recorded for one card."""

        for event in reversed(self.replay()):
            if (
                event.card_id != card_id
                or event.event != "STAGE_PR_BOUND"
                or (mission_id is not None and event.mission_id != mission_id)
                or (stage_id is not None and event.stage_id != stage_id)
            ):
                continue
            data = event.data
            if (
                isinstance(data.get("repository"), str)
                and REPOSITORY.fullmatch(data["repository"])
                and type(data.get("pr_number")) is int
                and 1 <= data["pr_number"] <= 1_000_000_000
                and isinstance(data.get("base_sha"), str)
                and re.fullmatch(r"[0-9a-f]{40}", data["base_sha"])
                and isinstance(data.get("head_sha"), str)
                and re.fullmatch(r"[0-9a-f]{40}", data["head_sha"])
                and isinstance(data.get("base_branch"), str)
                and BRANCH.fullmatch(data["base_branch"])
                and ".." not in data["base_branch"]
                and not data["base_branch"].endswith("/")
                and isinstance(data.get("head_branch"), str)
                and BRANCH.fullmatch(data["head_branch"])
                and ".." not in data["head_branch"]
                and not data["head_branch"].endswith("/")
            ):
                return {
                    "repository": data["repository"],
                    "pr_number": data["pr_number"],
                    "base_sha": data["base_sha"],
                    "head_sha": data["head_sha"],
                    "base_branch": data["base_branch"],
                    "head_branch": data["head_branch"],
                }
        return None

    def projection(
        self, *, mission_id: str | None = None, stage_id: str | None = None
    ) -> dict[str, Any]:
        """Rebuild a bounded, non-authoritative view from the verified chain."""

        all_events = self.replay()
        events = [
            event
            for event in all_events
            if (mission_id is None or event.mission_id == mission_id)
            and (stage_id is None or event.stage_id == stage_id)
        ]
        latest: dict[tuple[str, str, str], JournalEvent] = {}
        for event in events:
            if event.card_id:
                latest[(event.mission_id, event.stage_id, event.card_id)] = event
        card_states: dict[str, str] = {}
        card_occurrences: dict[str, list[tuple[str, str, str]]] = {}
        for binding, event in latest.items():
            card_occurrences.setdefault(binding[2], []).append(binding)
        for binding, event in latest.items():
            card_id = binding[2]
            key = card_id if len(card_occurrences[card_id]) == 1 else ":".join(binding)
            card_states[key] = event.state
        return {
            "schema_version": SCHEMA_VERSION,
            "event_count": len(events),
            "card_states": dict(sorted(card_states.items())),
            "active_cards": sorted(
                {
                    binding[2]
                    for binding, event in latest.items()
                    if event.state not in TERMINAL_STATES
                }
            ),
            "active_bindings": [
                {
                    "mission_id": mission,
                    "stage_id": stage,
                    "card_id": card,
                    "state": event.state,
                }
                for (mission, stage, card), event in sorted(latest.items())
                if event.state not in TERMINAL_STATES
            ],
            "bindings": [
                {
                    "mission_id": mission,
                    "stage_id": stage,
                    "card_id": card,
                    "state": event.state,
                }
                for (mission, stage, card), event in sorted(latest.items())
            ],
            "last_heartbeat": next(
                (event.timestamp for event in reversed(events) if event.event == "HEARTBEAT"),
                None,
            ),
            "last_seq": events[-1].seq if events else 0,
            "tail_sha256": events[-1].sha256 if events else "",
        }

    def active_mission_record(self) -> JournalEvent | None:
        """Return the latest active, non-terminal mission activation event."""
        all_events = self.replay()
        for event in reversed(all_events):
            if event.event in {"MISSION_COMPLETED", "MISSION_STOPPED"}:
                return None
            if event.event == "MISSION_ACTIVATED":
                return event
        return None

    def record_mission_proposal(
        self,
        mission_id: str,
        proposal_sha256: str,
        mission_data: dict[str, Any] | None = None,
        *,
        idempotency_key: str | None = None,
    ) -> JournalEvent:
        key = idempotency_key or f"mission-proposed:{mission_id}:{proposal_sha256[:16]}"
        return self.append(
            event="MISSION_PROPOSED",
            idempotency_key=key,
            mission_id=mission_id,
            stage_id="mission",
            card_id="",
            state="PROPOSING",
            detail="mission_proposed",
            data=mission_data or {"proposal_sha256": proposal_sha256},
            enforce_transition=False,
        )

    def record_mission_activation(
        self,
        mission_id: str,
        proposal_sha256: str,
        mission_data: dict[str, Any] | None = None,
        *,
        idempotency_key: str | None = None,
    ) -> JournalEvent:
        key = idempotency_key or f"mission-activated:{mission_id}:{proposal_sha256[:16]}"
        return self.append(
            event="MISSION_ACTIVATED",
            idempotency_key=key,
            mission_id=mission_id,
            stage_id="mission",
            card_id="",
            state="RUNNING",
            detail="mission_activated",
            data=mission_data or {"proposal_sha256": proposal_sha256},
            enforce_transition=False,
        )

    def record_mission_completion(
        self,
        mission_id: str,
        summary: dict[str, Any] | None = None,
        *,
        idempotency_key: str | None = None,
    ) -> JournalEvent:
        key = idempotency_key or f"mission-completed:{mission_id}:{_now()}"
        return self.append(
            event="MISSION_COMPLETED",
            idempotency_key=key,
            mission_id=mission_id,
            stage_id="mission",
            card_id="",
            state="COMPLETE",
            detail="mission_completed",
            data=summary or {},
            enforce_transition=False,
        )

    def record_mission_stop(
        self,
        mission_id: str,
        reason: str = "emergency_stop",
        *,
        idempotency_key: str | None = None,
    ) -> JournalEvent:
        key = idempotency_key or f"mission-stopped:{mission_id}:{_now()}"
        return self.append(
            event="MISSION_STOPPED",
            idempotency_key=key,
            mission_id=mission_id,
            stage_id="mission",
            card_id="",
            state="BLOCKED",
            detail=reason,
            data={"reason": reason},
            enforce_transition=False,
        )


__all__ = [
    "ALL_STATES",
    "CARD_STATES",
    "MISSION_STATES",
    "JournalCorrupt",
    "JournalError",
    "IdempotencyConflict",
    "JournalEvent",
    "StewardJournal",
    "TERMINAL_STATES",
    "TransitionRejected",
    "transition_allowed",
]
