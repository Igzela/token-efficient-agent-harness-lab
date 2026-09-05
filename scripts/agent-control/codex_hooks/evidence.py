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

# Work-product status must not bind hook-ephemeral files: receipts, telemetry,
# and evidence records record the run itself, so including them would make
# every digest self-invalidating. Shared with continuation's edit check.
HOOK_EPHEMERAL_PATH_MARKERS = (
    "hooks_state",
    ".codex",
    "failure_reason.json",
    "telemetry.json",
    "verification_evidence.json",
    "compaction_state.json",
    "continuation_state.json",
    "completion_status.json",
)


def is_hooks_ephemeral_status_path(path_part: str) -> bool:
    """True when a porcelain path belongs to hook-ephemeral state, not work product."""
    base = path_part.rsplit("/", 1)[-1]
    if base.startswith("receipt_"):
        return True
    return any(marker in path_part for marker in HOOK_EPHEMERAL_PATH_MARKERS)


def porcelain_work_product_lines(porcelain_output: str) -> list[str]:
    """Filter `git status --porcelain` output down to work-product entries."""
    entries: list[str] = []
    for line in porcelain_output.splitlines():
        stripped = line.strip()
        if not stripped:
            continue
        parts = stripped.split(maxsplit=1)
        if len(parts) < 2:
            continue
        path_part = parts[1].split("->")[-1].strip().strip('"')
        if is_hooks_ephemeral_status_path(path_part):
            continue
        entries.append(stripped)
    return entries


def _read_env_list(key: str, env: Any = None) -> list[str]:
    """Parse a list of strings from an environment variable."""
    source = env if env is not None else os.environ
    raw = source.get(key, "") if hasattr(source, "get") else ""
    if not raw:
        return []
    try:
        items = json.loads(raw)
    except Exception:
        return []
    if not isinstance(items, list):
        return []
    return [str(t).strip() for t in items if isinstance(t, str) and str(t).strip()]


def read_focused_tests(env: Any = None) -> list[str]:
    """Parse the WorkCard-declared focused verification checks (may be empty)."""
    return _read_env_list("STEWARD_FOCUSED_TESTS", env)


def read_negative_checks(env: Any = None) -> list[str]:
    """Parse the WorkCard-declared negative checks (may be empty)."""
    return _read_env_list("STEWARD_NEGATIVE_CHECKS", env)


def read_expected_evidence(env: Any = None) -> list[str]:
    """Parse the WorkCard-declared expected evidence descriptors (may be empty)."""
    return _read_env_list("STEWARD_EXPECTED_EVIDENCE", env)


def read_allowed_paths(env: Any = None) -> list[str]:
    """Parse the WorkCard-declared allowed paths (may be empty)."""
    return _read_env_list("STEWARD_ALLOWED_PATHS", env)


def workcard_acceptance_digest(
    *,
    workcard_id: str,
    focused_tests: list[str] | None = None,
    negative_checks: list[str] | None = None,
    expected_evidence: list[str] | None = None,
    allowed_paths: list[str] | None = None,
    env: Any = None,
) -> str:
    """Stable canonical digest of the full WorkCard acceptance contract.

    Binds workcard_id, focused_tests, negative_checks, expected_evidence,
    and allowed_paths. PostToolUse and Stop share this canonical helper so
    neither side can drift. Any change to these acceptance descriptors
    invalidates stored verification receipts.
    """
    if focused_tests is None:
        focused_tests = read_focused_tests(env)
    if negative_checks is None:
        negative_checks = read_negative_checks(env)
    if expected_evidence is None:
        expected_evidence = read_expected_evidence(env)
    if allowed_paths is None:
        allowed_paths = read_allowed_paths(env)

    canonical = {
        "allowed_paths": sorted(str(p).strip() for p in allowed_paths if str(p).strip()),
        "expected_evidence": sorted(str(e).strip() for e in expected_evidence if str(e).strip()),
        "focused_tests": sorted(str(t).strip() for t in focused_tests if str(t).strip()),
        "negative_checks": sorted(str(n).strip() for n in negative_checks if str(n).strip()),
        "workcard_id": str(workcard_id).strip(),
    }
    canonical_json = json.dumps(canonical, sort_keys=True, separators=(",", ":"))
    return f"sha256:{hashlib.sha256(canonical_json.encode('utf-8')).hexdigest()}"


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
            entries = porcelain_work_product_lines(proc.stdout)
            status_digest = "sha256:" + hashlib.sha256(
                "\n".join(entries).encode("utf-8")
            ).hexdigest()
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
    focused_tests: list[str] | None = None,
    negative_checks: list[str] | None = None,
    expected_evidence: list[str] | None = None,
    allowed_paths: list[str] | None = None,
    command: str,
    success: bool,
    worktree: Path | str,
    receipt_id: int,
    env: Any = None,
) -> dict[str, Any]:
    """Build a bound PASS evidence record. Call only with proven ``success``."""
    if focused_tests is None:
        focused_tests = read_focused_tests(env)
    if negative_checks is None:
        negative_checks = read_negative_checks(env)
    if expected_evidence is None:
        expected_evidence = read_expected_evidence(env)
    if allowed_paths is None:
        allowed_paths = read_allowed_paths(env)

    acc_digest = workcard_acceptance_digest(
        workcard_id=workcard_id,
        focused_tests=focused_tests,
        negative_checks=negative_checks,
        expected_evidence=expected_evidence,
        allowed_paths=allowed_paths,
        env=env,
    )
    state = workspace_state(worktree)
    return {
        "schema_version": EVIDENCE_SCHEMA_VERSION,
        "status": "passed",
        "workcard_id": workcard_id,
        "acceptance_digest": acc_digest,
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
    focused_tests: list[str] | None = None,
    negative_checks: list[str] | None = None,
    expected_evidence: list[str] | None = None,
    allowed_paths: list[str] | None = None,
    worktree: Path | str,
    env: Any = None,
) -> tuple[bool, str]:
    """Verify a stored evidence record is bound to the current WorkCard state.

    Stored PASS receipt is accepted only when:
    - schema/version is correct
    - WorkCard identity matches
    - acceptance digest matches exactly (binding workcard_id, focused_tests,
      negative_checks, expected_evidence descriptors, allowed_paths)
    - workspace state (head SHA + work product status digest) matches exactly
    - verification command is present
    - result is proven success.

    Empty-vs-empty comparisons never count as a match: an unbound record
    (missing card id) or unobservable code state (no git HEAD) is rejected
    outright instead of passing trivially. Callers fall through to fresh
    focused-test execution in that case.
    """
    if not isinstance(evidence, dict):
        return False, "evidence_not_a_record"
    if evidence.get("schema_version") != EVIDENCE_SCHEMA_VERSION:
        return False, "evidence_schema_mismatch"
    if evidence.get("status") != "passed" or evidence.get("result") != "success":
        return False, "evidence_not_a_pass"
    if not workcard_id:
        return False, "evidence_workcard_missing"
    if (evidence.get("workcard_id") or "") != workcard_id:
        return False, "evidence_workcard_mismatch"

    if focused_tests is None:
        focused_tests = read_focused_tests(env)
    if negative_checks is None:
        negative_checks = read_negative_checks(env)
    if expected_evidence is None:
        expected_evidence = read_expected_evidence(env)
    if allowed_paths is None:
        allowed_paths = read_allowed_paths(env)

    expected_acc_digest = workcard_acceptance_digest(
        workcard_id=workcard_id,
        focused_tests=focused_tests,
        negative_checks=negative_checks,
        expected_evidence=expected_evidence,
        allowed_paths=allowed_paths,
        env=env,
    )
    recorded_acc_digest = evidence.get("acceptance_digest")
    if not recorded_acc_digest:
        return False, "evidence_acceptance_digest_missing"
    if recorded_acc_digest != expected_acc_digest:
        return False, "evidence_acceptance_digest_mismatch"

    if evidence.get("focused_tests_digest") != focused_tests_digest(focused_tests):
        return False, "evidence_focused_tests_mismatch"

    current = workspace_state(worktree)
    if not current["head_sha"]:
        return False, "evidence_code_state_unobservable"
    if (evidence.get("head_sha") or "") != current["head_sha"]:
        return False, "evidence_code_state_moved"
    if not current["status_digest"]:
        return False, "evidence_workspace_state_unobservable"
    if (evidence.get("status_digest") or "") != current["status_digest"]:
        return False, "evidence_workspace_state_moved"
    if not evidence.get("command"):
        return False, "evidence_command_missing"
    return True, ""
