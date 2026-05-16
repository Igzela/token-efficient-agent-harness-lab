"""Sandbox Manager for Stage 4 — file-level isolation with claim tracking."""

from __future__ import annotations

from dataclasses import dataclass, field


@dataclass(frozen=True)
class Sandbox:
    sandbox_id: str
    task_id: str
    status: str  # "created" | "active" | "released" | "failed"
    claimed_files: tuple[str, ...]
    created_at: str
    released_at: str | None = None


@dataclass(frozen=True)
class FileClaim:
    claim_id: str
    sandbox_id: str
    file_path: str
    claimed_at: str
    released: bool = False


@dataclass(frozen=True)
class ConflictReport:
    has_conflict: bool
    conflicting_sandbox_id: str | None
    conflicting_file: str | None
    message: str


class SandboxManager:
    """Track sandbox file claims and detect conflicts."""

    def __init__(self) -> None:
        self._sandboxes: dict[str, Sandbox] = {}
        self._file_owners: dict[str, str] = {}  # file_path -> sandbox_id
        self._claims: dict[str, FileClaim] = {}  # claim_id -> FileClaim
        self._claim_counter: int = 0

    def create_sandbox(
        self, task_id: str, files: tuple[str, ...], timestamp: str = "2026-01-01T00:00:00Z"
    ) -> Sandbox:
        sandbox_id = f"sbx_{task_id}_{len(self._sandboxes)}"
        sandbox = Sandbox(
            sandbox_id=sandbox_id,
            task_id=task_id,
            status="created",
            claimed_files=(),
            created_at=timestamp,
        )
        self._sandboxes[sandbox_id] = sandbox
        if files:
            report = self.claim_files(sandbox_id, files, timestamp)
            if report.has_conflict:
                raise ValueError(f"initial claim failed: {report.message}")
        return self._sandboxes[sandbox_id]

    def claim_files(
        self,
        sandbox_id: str,
        files: tuple[str, ...],
        timestamp: str = "2026-01-01T00:00:00Z",
    ) -> ConflictReport:
        sandbox = self._sandboxes.get(sandbox_id)
        if sandbox is None:
            return ConflictReport(True, None, None, f"unknown sandbox: {sandbox_id}")
        if sandbox.status not in ("created", "active"):
            return ConflictReport(
                True, None, None, f"sandbox {sandbox_id} is {sandbox.status}"
            )
        for file_path in files:
            owner = self._file_owners.get(file_path)
            if owner is not None and owner != sandbox_id:
                return ConflictReport(
                    True, owner, file_path, f"file {file_path} claimed by {owner}"
                )
        for file_path in files:
            if file_path not in self._file_owners:
                self._file_owners[file_path] = sandbox_id
                self._claim_counter += 1
                claim_id = f"claim_{self._claim_counter}"
                self._claims[claim_id] = FileClaim(
                    claim_id=claim_id,
                    sandbox_id=sandbox_id,
                    file_path=file_path,
                    claimed_at=timestamp,
                )
        existing = set(sandbox.claimed_files)
        existing.update(files)
        self._sandboxes[sandbox_id] = Sandbox(
            sandbox_id=sandbox.sandbox_id,
            task_id=sandbox.task_id,
            status="active",
            claimed_files=tuple(sorted(existing)),
            created_at=sandbox.created_at,
            released_at=sandbox.released_at,
        )
        return ConflictReport(False, None, None, "ok")

    def release_sandbox(
        self, sandbox_id: str, timestamp: str = "2026-01-01T00:00:00Z"
    ) -> Sandbox:
        sandbox = self._sandboxes.get(sandbox_id)
        if sandbox is None:
            raise ValueError(f"unknown sandbox: {sandbox_id}")
        for file_path in sandbox.claimed_files:
            if self._file_owners.get(file_path) == sandbox_id:
                del self._file_owners[file_path]
        for claim in self._claims.values():
            if claim.sandbox_id == sandbox_id and not claim.released:
                self._claims[claim.claim_id] = FileClaim(
                    claim_id=claim.claim_id,
                    sandbox_id=claim.sandbox_id,
                    file_path=claim.file_path,
                    claimed_at=claim.claimed_at,
                    released=True,
                )
        released = Sandbox(
            sandbox_id=sandbox.sandbox_id,
            task_id=sandbox.task_id,
            status="released",
            claimed_files=sandbox.claimed_files,
            created_at=sandbox.created_at,
            released_at=timestamp,
        )
        self._sandboxes[sandbox_id] = released
        return released

    def get_sandbox(self, sandbox_id: str) -> Sandbox | None:
        return self._sandboxes.get(sandbox_id)

    def list_active(self) -> tuple[Sandbox, ...]:
        return tuple(
            s for s in self._sandboxes.values() if s.status in ("created", "active")
        )

    def list_all(self) -> tuple[Sandbox, ...]:
        return tuple(self._sandboxes.values())

    def is_file_claimed(self, file_path: str) -> str | None:
        return self._file_owners.get(file_path)

    def get_claims(self, sandbox_id: str) -> tuple[FileClaim, ...]:
        return tuple(
            c
            for c in self._claims.values()
            if c.sandbox_id == sandbox_id
        )
