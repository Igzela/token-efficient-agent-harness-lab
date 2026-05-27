"""ManualExecutionSession schema and store for tracking manual execution lifecycle."""

from __future__ import annotations

import uuid
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

MANUAL_SESSION_SCHEMA_VERSION = "manual_execution_session.v1"

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

MANUAL_SESSION_STATUSES: tuple[str, ...] = (
    "created", "prompt_generated", "human_executing",
    "result_submitted", "evaluated", "recorded",
)


# ---------------------------------------------------------------------------
# Schema
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class ManualExecutionSession:
    session_id: str
    dispatch_id: str
    prompt_pack_id: str
    status: str  # from MANUAL_SESSION_STATUSES
    created_at: str
    updated_at: str
    submission_id: str | None = None
    evaluation_id: str | None = None
    schema_version: str = MANUAL_SESSION_SCHEMA_VERSION

    def to_dict(self) -> dict[str, Any]:
        return {
            "schema_version": self.schema_version,
            "session_id": self.session_id,
            "dispatch_id": self.dispatch_id,
            "prompt_pack_id": self.prompt_pack_id,
            "status": self.status,
            "submission_id": self.submission_id,
            "evaluation_id": self.evaluation_id,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        }


# ---------------------------------------------------------------------------
# Store
# ---------------------------------------------------------------------------


class ManualSessionStore:
    """In-memory store for ManualExecutionSessions."""

    def __init__(self) -> None:
        self._sessions: dict[str, ManualExecutionSession] = {}

    def create(
        self, dispatch_id: str, prompt_pack_id: str
    ) -> ManualExecutionSession:
        now = datetime.now(timezone.utc).isoformat()
        session = ManualExecutionSession(
            session_id=f"msess-{uuid.uuid4().hex[:12]}",
            dispatch_id=dispatch_id,
            prompt_pack_id=prompt_pack_id,
            status="created",
            created_at=now,
            updated_at=now,
        )
        self._sessions[session.session_id] = session
        return session

    def advance(
        self,
        session: ManualExecutionSession,
        new_status: str,
        submission_id: str | None = None,
        evaluation_id: str | None = None,
    ) -> ManualExecutionSession:
        if new_status not in MANUAL_SESSION_STATUSES:
            raise ValueError(f"Invalid status: {new_status}")
        updated = ManualExecutionSession(
            session_id=session.session_id,
            dispatch_id=session.dispatch_id,
            prompt_pack_id=session.prompt_pack_id,
            status=new_status,
            submission_id=submission_id or session.submission_id,
            evaluation_id=evaluation_id or session.evaluation_id,
            created_at=session.created_at,
            updated_at=datetime.now(timezone.utc).isoformat(),
        )
        self._sessions[session.session_id] = updated
        return updated

    def get(self, session_id: str) -> ManualExecutionSession | None:
        return self._sessions.get(session_id)

    def list_sessions(self) -> list[ManualExecutionSession]:
        return list(self._sessions.values())

    def get_by_dispatch(self, dispatch_id: str) -> ManualExecutionSession | None:
        for s in self._sessions.values():
            if s.dispatch_id == dispatch_id:
                return s
        return None
