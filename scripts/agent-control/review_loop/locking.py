"""Per-chat single-process lock (repository-owned protocol, operator-owned dirs).

The lock file location is caller-provided (operator directory); the protocol
for acquire/owner-identity/release/stale-owner is repository-owned so two
concurrent processes can never both hold the same chat.  An owner token binds
pid + start time; only a lock whose recorded owner is provably dead may be
taken over.
"""

from __future__ import annotations

import hashlib
import json
import os
import pathlib
import time


class LockBusy(RuntimeError):
    pass


class ChatLock:
    """Advisory flock with a JSON owner token (pid, start, host)."""

    def __init__(self, lock_dir: pathlib.Path, chat_key: str):
        digest = hashlib.sha256(chat_key.encode("utf-8")).hexdigest()
        self.path = pathlib.Path(lock_dir) / f"{digest}.lock"
        self.chat_key = chat_key
        self._held = False

    def _owner_token(self) -> dict[str, object]:
        return {
            "pid": os.getpid(),
            "start": int(time.time()),
            "host": os.uname().nodename,
        }

    def _owner_alive(self, token: dict[str, object]) -> bool:
        pid = token.get("pid")
        if not isinstance(pid, int):
            return False
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
        return True

    def acquire(self, *, stale_after_s: int = 3600) -> None:
        """Acquire the flock; fail with LockBusy when another process holds it.

        A lock file whose recorded owner is provably dead (or older than
        stale_after_s with a dead pid) may be reclaimed; otherwise the
        acquire fails closed.
        """
        import fcntl

        self.path.parent.mkdir(parents=True, exist_ok=True)
        fp = open(self.path, "a+")
        try:
            fcntl.flock(fp.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            token = self._read_token()
            if token and not self._owner_alive(token) and time.time() - int(token.get("start", 0)) > stale_after_s:
                fp.seek(0)
                fp.truncate()
                fp.write(json.dumps(self._owner_token()))
                fp.flush()
                self._fp = fp
                self._held = True
                return
            fp.close()
            raise LockBusy(f"chat lock held: {self.path}")
        fp.seek(0)
        fp.truncate()
        fp.write(json.dumps(self._owner_token()))
        fp.flush()
        self._fp = fp
        self._held = True

    def _read_token(self) -> dict[str, object] | None:
        try:
            self._fp.seek(0)
            raw = self._fp.read()
        except (AttributeError, OSError):
            return None
        if not raw.strip():
            return None
        try:
            data = json.loads(raw)
            return data if isinstance(data, dict) else None
        except Exception:
            return None

    def release(self) -> None:
        import fcntl

        if not self._held:
            return
        try:
            fcntl.flock(self._fp.fileno(), fcntl.LOCK_UN)
        finally:
            self._fp.close()
            self._held = False

    def __enter__(self) -> "ChatLock":
        self.acquire()
        return self

    def __exit__(self, *exc) -> None:
        self.release()
