"""Bound verification-evidence records (H3).

A PASS evidence record is meaningful only when bound to the exact WorkCard,
verification scope, code state, command, and real success signal it was
produced from. A stale record from an old WorkCard, moved code, or mismatched
tests must never satisfy Stop acceptance. Both the PostToolUse recorder
(``session.py``) and the Stop verifier (``continuation.py``) share this
module so the binding cannot drift between the two ends.
"""

from __future__ import annotations

from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
from typing import Any

EVIDENCE_SCHEMA_VERSION = "hooks_verification_evidence.v1"


def read_focused_tests(env: Any = None) -> list[str]:
    """Parse the WorkCard-declared focused verification checks (may be empty)."""
    source = env if env is not None else os.environ
    raw = source.get("STEWARD_FOCUSED_TESTS", "") if hasattr(source, "get") else ""
    if not raw:
        return []
    try:
        focused = json.loads(raw)
    except Exception:
        return []
    if not isinstance(focused, list):
        return []
    return [str(t).strip() for t in focused if isinstance(t, str) and str(t).strip()]


def focused_tests_digest(focused_tests: list[str]) -> str:
    """Stable digest of the declared focused-test scope."""
    canonical = json.dumps(sorted(str(t) for t in focused_tests), sort_keys=True)
    return f"sha256:{hashlib.sha256(canonical.encode('utf-8')).hexdigest()}"


def workspace_state(worktree: Path | str) -> dict[str, str]:
    """Capture the code state a test result is bound to (best-effort).

    Returns head SHA + porcelain-status digest; empty strings when git is
    unavailable. The Stop verifier requires exact equality with the state
    recorded at test time, so unobservable state can never silently match.
    """
    head_sha = ""
    status_digest = ""
    try:
        proc = subprocess.run(
            ["git", "-C", str(worktree), "rev-parse", "--verify", "HEAD"],
            capture_output=True,
            text=True,
            timeout=5,
            check=False,
        )
        if proc.returncode == 0:
            head_sha = proc.stdout.strip()
    except Exception:
        head_sha = ""
    try:
        proc = subprocess.run(
            ["git", "-C", str(worktree), "status", "--porcelain"],
            capture_output=True,
            text=True,
            timeout=5,
            check=False,
        )
        if proc.returncode == 0:
            status_digest = "sha256:" + hashlib.sha256(proc.stdout.encode("utf-8")).hexdigest()
    except Exception:
        status_digest = ""
    return {"head_sha": head_sha, "status_digest": status_digest}


_SUCCESS_KEYS = ("success", "succeeded", "ok", "passed", "pass")
_FAILURE_KEYS = ("failed", "failure", "fail", "error", "errors")
_EXIT_KEYS = ("exit_code", "exitcode", "exitCode", "returncode", "return_code")
_STATUS_KEYS = ("status", "state", "result", "outcome")

_SUCCESS_STRINGS = {"pass", "passed", "success", "successful", "ok", "true", "0"}
_FAILURE_STRINGS = {"fail", "failed", "failure", "error", "false"}


def _signal_from_scalar(value: Any, key_class: str) -> bool | None:
    """Map a scalar success signal by key class.

    - exit keys use exit-code semantics: 0/true means success.
    - success keys use count/flag semantics: nonzero means success
      (e.g. {"passed": 5} means five tests passed).
    - status keys accept explicit success/failure words or booleans;
      numbers are NOT guessed.
    """
    if isinstance(value, bool):
        return value
    if isinstance(value, (int, float)):
        if key_class == "exit":
            return True if value == 0 else False
        if key_class == "success":
            return True if value != 0 else False
        return None
    if isinstance(value, str):
        lowered = value.strip().lower()
        if key_class == "success" and lowered == "0":
            return False  # e.g. {"passed": "0"}: zero passing tests is not success
        if lowered in _SUCCESS_STRINGS:
            return True
        if lowered in _FAILURE_STRINGS:
            return False
    return None


def extract_tool_success(tool_response: Any, _depth: int = 0) -> bool | None:
    """Extract a real success signal from a structured tool response.

    Returns True only on an explicit success indicator (e.g. exit_code 0,
    success true), False on any explicit failure indicator, and None when no
    machine-readable signal exists. String output blobs without structure
    yield None: absence of "failed"/"error" substrings is NOT success.
    Explicit failure anywhere dominates (fail-closed precedence).
    """
    if _depth > 4:
        return None
    if isinstance(tool_response, dict):
        found_success = False
        for key, value in tool_response.items():
            lowered = str(key).lower()
            if lowered in _EXIT_KEYS:
                signal = _signal_from_scalar(value, "exit")
                if signal is False:
                    return False
                if signal is True:
                    found_success = True
            elif lowered in _SUCCESS_KEYS:
                signal = _signal_from_scalar(value, "success")
                if signal is False:
                    return False
                if signal is True:
                    found_success = True
            elif lowered in _FAILURE_KEYS:
                if isinstance(value, (int, float)) and not isinstance(value, bool):
                    if value != 0:
                        return False
                elif isinstance(value, bool):
                    if value:
                        return False
                elif isinstance(value, str):
                    if value.strip():
                        return False
                elif isinstance(value, (list, dict)) and value:
                    return False
            elif lowered in _STATUS_KEYS:
                signal = _signal_from_scalar(value, "status")
                if signal is False:
                    return False
                if signal is True:
                    found_success = True
            elif isinstance(value, (dict, list)):
                nested = extract_tool_success(value, _depth + 1)
                if nested is False:
                    return False
                if nested is True:
                    found_success = True
        return True if found_success else None
    if isinstance(tool_response, (list, tuple)):
        found_success = False
        for item in tool_response:
            nested = extract_tool_success(item, _depth + 1)
            if nested is False:
                return False
            if nested is True:
                found_success = True
        return True if found_success else None
    return None


def build_evidence_record(
    *,
    workcard_id: str,
    focused_tests: list[str],
    command: str,
    success: bool,
    worktree: Path | str,
    receipt_id: int,
) -> dict[str, Any]:
    """Build a bound PASS evidence record. Call only with proven ``success``."""
    state = workspace_state(worktree)
    return {
        "schema_version": EVIDENCE_SCHEMA_VERSION,
        "status": "passed",
        "workcard_id": workcard_id,
        "focused_tests_digest": focused_tests_digest(focused_tests),
        "command": command,
        "result": "success" if success else "failure",
        "head_sha": state["head_sha"],
        "status_digest": state["status_digest"],
        "receipt_id": receipt_id,
        "recorded_at": datetime.now(timezone.utc).isoformat(),
    }


def evidence_binding_matches(
    evidence: Any,
    *,
    workcard_id: str,
    focused_tests: list[str],
    worktree: Path | str,
) -> tuple[bool, str]:
    """Verify a stored evidence record is bound to the current WorkCard state."""
    if not isinstance(evidence, dict):
        return False, "evidence_not_a_record"
    if evidence.get("schema_version") != EVIDENCE_SCHEMA_VERSION:
        return False, "evidence_schema_mismatch"
    if evidence.get("status") != "passed" or evidence.get("result") != "success":
        return False, "evidence_not_a_pass"
    if (evidence.get("workcard_id") or "") != (workcard_id or ""):
        return False, "evidence_workcard_mismatch"
    if evidence.get("focused_tests_digest") != focused_tests_digest(focused_tests):
        return False, "evidence_focused_tests_mismatch"
    current = workspace_state(worktree)
    if (evidence.get("head_sha") or "") != current["head_sha"]:
        return False, "evidence_code_state_moved"
    if (evidence.get("status_digest") or "") != current["status_digest"]:
        return False, "evidence_workspace_state_moved"
    if not evidence.get("command"):
        return False, "evidence_command_missing"
    return True, ""
