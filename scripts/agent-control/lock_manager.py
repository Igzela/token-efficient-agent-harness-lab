"""Concurrency locking for the agent orchestrator.

Uses a simple lock-file mechanism only as a same-host secondary safeguard.
Repository-wide capacity is authoritative in the serialized GitHub dispatcher;
the diagnostic capacity command must never be used for dispatch decisions.
"""

import hashlib
import json
import os
import pathlib
import subprocess
import sys
import time

LOCK_DIR = pathlib.Path("/tmp/agent-orchestrator-locks")
MAX_WORKERS = 2


def _lock_path(lock_key):
    h = hashlib.sha256(lock_key.encode()).hexdigest()[:16]
    LOCK_DIR.mkdir(parents=True, exist_ok=True)
    return LOCK_DIR / f"{h}.lock"


def acquire_lock(lock_key, timeout_secs=30, wait_interval=2):
    lock_file = _lock_path(lock_key)
    deadline = time.monotonic() + timeout_secs
    while time.monotonic() < deadline:
        try:
            fd = os.open(str(lock_file), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
            with os.fdopen(fd, "w") as f:
                f.write(f"{os.getpid()}\n{lock_key}\n")
            return True
        except FileExistsError:
            stale = _check_stale(lock_file)
            if stale:
                lock_file.unlink(missing_ok=True)
                continue
            time.sleep(wait_interval)
    return False


def release_lock(lock_key):
    lock_file = _lock_path(lock_key)
    lock_file.unlink(missing_ok=True)


def _check_stale(lock_file):
    try:
        content = lock_file.read_text().strip()
        pid_str = content.split("\n")[0].strip()
        pid = int(pid_str)
        try:
            os.kill(pid, 0)
            return False
        except ProcessLookupError:
            return True
        except PermissionError:
            return False
    except (ValueError, IndexError, OSError):
        return True


def count_active_locks(prefix=""):
    count = 0
    if not LOCK_DIR.exists():
        return 0
    for lock_file in LOCK_DIR.iterdir():
        if not lock_file.name.endswith(".lock"):
            continue
        if _check_stale(lock_file):
            lock_file.unlink(missing_ok=True)
            continue
        if prefix:
            try:
                content = lock_file.read_text()
                if prefix in content:
                    count += 1
            except OSError:
                pass
        else:
            count += 1
    return count


def check_repo_capacity():
    """Legacy same-host diagnostic; not a repository-wide capacity authority."""
    return count_active_locks() < MAX_WORKERS


def gh_concurrency_group(issue_number, pr_number=None, head_sha=None):
    parts = [f"issue-{issue_number}"]
    if pr_number:
        parts.append(f"pr-{pr_number}")
    if head_sha:
        parts.append(f"sha-{head_sha[:12]}")
    return ":".join(parts)


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    usage = (
        "Usage: lock_manager.py acquire <key> [timeout] | release <key> | "
        "check <key> | count [prefix] | capacity"
    )

    if command == "acquire":
        if len(sys.argv) not in (3, 4):
            print(usage, file=sys.stderr)
            sys.exit(1)
        lock_key = sys.argv[2]
        try:
            timeout = int(sys.argv[3]) if len(sys.argv) == 4 else 30
        except ValueError:
            print("timeout must be an integer", file=sys.stderr)
            sys.exit(1)
        if acquire_lock(lock_key, timeout):
            print("acquired")
        else:
            print("failed")
            sys.exit(1)

    elif command == "release":
        if len(sys.argv) != 3:
            print(usage, file=sys.stderr)
            sys.exit(1)
        lock_key = sys.argv[2]
        release_lock(lock_key)
        print("released")

    elif command == "check":
        if len(sys.argv) != 3:
            print(usage, file=sys.stderr)
            sys.exit(1)
        lock_key = sys.argv[2]
        lock_file = _lock_path(lock_key)
        if lock_file.exists() and not _check_stale(lock_file):
            print("locked")
        else:
            print("free")

    elif command == "count":
        if len(sys.argv) not in (2, 3):
            print(usage, file=sys.stderr)
            sys.exit(1)
        prefix = sys.argv[2] if len(sys.argv) == 3 else ""
        print(count_active_locks(prefix))

    elif command == "capacity":
        if len(sys.argv) != 2:
            print(usage, file=sys.stderr)
            sys.exit(1)
        if check_repo_capacity():
            print("available")
        else:
            print("full")
            sys.exit(1)

    else:
        print(usage, file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
