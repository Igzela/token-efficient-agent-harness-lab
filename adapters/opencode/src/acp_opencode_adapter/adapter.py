"""Deterministic OpenCode fixture adapter — no network, no real binary.

Identity and confinement are validated before any fixture work. Real OpenCode
binary admission is intentionally not implemented here
(PE7-OPENCODE-BINARY-ADMISSION-1).
"""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
import time
from typing import Any

REQUEST_SCHEMA = "opencode_external_request.v1"
RESULT_SCHEMA = "opencode_external_result.v1"
ERROR_SCHEMA = "opencode_external_error.v1"
ADAPTER_CONTRACT = "opencode_external_adapter.v1"
ADAPTER_VERSION = "0.1.0"
PINNED_OPENCODE_VERSION = "1.1.48"
RUNTIME_KIND = "opencode"

_ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:@/-]{0,255}$")
_SHA_RE = re.compile(r"^[0-9a-f]{64}$")
_BASE_COMMIT_RE = re.compile(r"^[0-9a-f]{40}$|^[0-9a-f]{64}$")

# Exact declared fixture environment allowlist (order-insensitive equality).
_EXACT_ENV_ALLOWLIST = (
    "PATH",
    "HOME",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "PYTHONIOENCODING",
    "PYTHONDONTWRITEBYTECODE",
    "PYTHONNOUSERSITE",
    "PYTHONPATH",
)

_EXACT_PERMISSION_KEYS = frozenset(
    {
        "approval_mode",
        "network_enabled",
        "mcp_enabled",
        "websearch",
        "webfetch",
        "remote_agents",
        "background_agents",
        "provider_fallback",
    }
)

# Exact allowed request-envelope keys (unknown keys fail closed).
_ALLOWED_REQUEST_KEYS = frozenset(
    {
        "schema_version",
        "invocation_id",
        "run_id",
        "node_id",
        "workflow_id",
        "execution_attempt",
        "scheduler_claim_id",
        "runtime_kind",
        "mode",
        "task_kind",
        "task_input_hash",
        "base_commit",
        "worktree_id",
        "allowed_paths",
        "environment_allowlist",
        "permission_profile",
        "permission_profile_hash",
        "requested_capabilities",
        "adapter_version",
        "adapter_contract_version",
        "expected_opencode_version",
        "expected_adapter_version",
    }
)


def _err(code: str, message: str) -> dict[str, Any]:
    return {
        "schema_version": ERROR_SCHEMA,
        "code": code,
        "message": message,
    }


def _sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def _canonical_json(value: Any) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def _empty_tool_summary(*, tool_call_count: int = 0) -> dict[str, int]:
    return {
        "tool_call_count": tool_call_count,
        "network_attempts": 0,
        "provider_attempts": 0,
        "mcp_attempts": 0,
        "web_attempts": 0,
        "remote_agent_attempts": 0,
        "background_agent_attempts": 0,
        "process_attempts": 0,
    }


def _runtime_block() -> dict[str, str]:
    return {
        "runtime_kind": RUNTIME_KIND,
        "runtime_version": PINNED_OPENCODE_VERSION,
        "adapter_version": ADAPTER_VERSION,
        "adapter_contract_version": ADAPTER_CONTRACT,
        "mode": "fixture",
    }


def _bound_result(request: dict[str, Any], **fields: Any) -> dict[str, Any]:
    result = {
        "schema_version": RESULT_SCHEMA,
        "invocation_id": request["invocation_id"],
        "run_id": request["run_id"],
        "node_id": request["node_id"],
        "workflow_id": request["workflow_id"],
        "scheduler_claim_id": request["scheduler_claim_id"],
        "execution_attempt": request["execution_attempt"],
        "task_kind": request["task_kind"],
        "task_input_hash": request["task_input_hash"],
        "base_commit": request["base_commit"],
        "worktree_id": request["worktree_id"],
        "status": "ok",
        "runtime": _runtime_block(),
    }
    result.update(fields)
    return result


def handle_request(request: dict[str, Any]) -> tuple[dict[str, Any], int]:
    if not isinstance(request, dict):
        return _err("request_invalid", "request must be an object"), 2

    unknown = set(request.keys()) - _ALLOWED_REQUEST_KEYS
    if unknown:
        return _err(
            "request_unknown_field",
            f"unknown request fields: {','.join(sorted(unknown))}",
        ), 2

    if request.get("schema_version") != REQUEST_SCHEMA:
        return _err("request_schema_invalid", "unsupported request schema"), 2
    if request.get("mode") != "fixture":
        return _err("mode_forbidden", "only fixture mode is accepted"), 2
    if request.get("runtime_kind") != RUNTIME_KIND:
        return _err("runtime_kind_invalid", "runtime_kind must be opencode"), 2

    for field, expected in (
        ("adapter_version", ADAPTER_VERSION),
        ("expected_adapter_version", ADAPTER_VERSION),
        ("adapter_contract_version", ADAPTER_CONTRACT),
        ("expected_opencode_version", PINNED_OPENCODE_VERSION),
    ):
        value = request.get(field)
        if value is None:
            return _err("request_field_missing", f"{field} is required"), 2
        if value != expected:
            return _err(
                "adapter_version_mismatch" if "adapter" in field else "opencode_version_mismatch",
                f"{field} mismatch",
            ), 2

    for field in (
        "invocation_id",
        "run_id",
        "node_id",
        "workflow_id",
        "scheduler_claim_id",
        "task_kind",
        "worktree_id",
    ):
        value = request.get(field)
        if not isinstance(value, str) or not _ID_RE.match(value):
            return _err("request_field_invalid", f"{field} is invalid"), 2
    if request.get("worktree_id") == "fixture-worktree":
        return _err("worktree_invalid", "placeholder fixture-worktree is forbidden"), 2

    attempt = request.get("execution_attempt")
    if not isinstance(attempt, int) or isinstance(attempt, bool) or attempt < 1:
        return _err("execution_attempt_invalid", "execution_attempt must be a positive integer"), 2

    base_commit = request.get("base_commit")
    if not isinstance(base_commit, str) or not _BASE_COMMIT_RE.match(base_commit):
        return _err("base_commit_invalid", "base_commit must be 40- or 64-char hex"), 2
    if base_commit == "fixture-base":
        return _err("base_commit_invalid", "placeholder fixture-base is forbidden"), 2

    task_input_hash = request.get("task_input_hash")
    if not isinstance(task_input_hash, str) or not _SHA_RE.match(task_input_hash):
        return _err("task_input_hash_invalid", "task_input_hash must be sha256 hex"), 2

    allowed_paths = request.get("allowed_paths")
    if not isinstance(allowed_paths, list) or not allowed_paths:
        return _err("allowed_paths_invalid", "allowed_paths required"), 2
    if len(allowed_paths) > 32:
        return _err("allowed_paths_oversized", "too many allowed paths"), 2
    for path in allowed_paths:
        if not isinstance(path, str) or not path or ".." in path or path.startswith("/"):
            return _err("allowed_paths_invalid", "path traversal or absolute path rejected"), 2
        if path.startswith("\\") or "\x00" in path:
            return _err("allowed_paths_invalid", "forbidden path form"), 2

    env_allowlist = request.get("environment_allowlist")
    if not isinstance(env_allowlist, list):
        return _err("environment_allowlist_invalid", "environment_allowlist required"), 2
    if any(not isinstance(name, str) for name in env_allowlist):
        return _err("environment_allowlist_invalid", "env names must be strings"), 2
    if len(env_allowlist) != len(set(env_allowlist)):
        return _err("environment_allowlist_invalid", "duplicate env names forbidden"), 2
    if set(env_allowlist) != set(_EXACT_ENV_ALLOWLIST):
        return _err(
            "environment_allowlist_mismatch",
            "environment_allowlist must equal the declared fixture allowlist exactly",
        ), 2

    profile = request.get("permission_profile")
    if not isinstance(profile, dict):
        return _err("permission_profile_invalid", "permission_profile required"), 2
    if set(profile.keys()) != _EXACT_PERMISSION_KEYS:
        return _err(
            "permission_profile_keys_invalid",
            "permission_profile must contain exactly the deny-by-default key set",
        ), 2
    if profile.get("approval_mode") != "deny_by_default":
        return _err("permission_escalation", "approval_mode must be deny_by_default"), 2
    for flag in (
        "network_enabled",
        "mcp_enabled",
        "websearch",
        "webfetch",
        "remote_agents",
        "background_agents",
        "provider_fallback",
    ):
        if profile.get(flag) is not False:
            return _err("capability_forbidden", f"{flag} must be false"), 2

    declared_hash = request.get("permission_profile_hash")
    if not isinstance(declared_hash, str) or not _SHA_RE.match(declared_hash):
        return _err("permission_profile_hash_invalid", "permission_profile_hash required"), 2
    actual_hash = _sha256_text(_canonical_json(profile))
    if declared_hash != actual_hash:
        return _err(
            "permission_profile_hash_mismatch",
            "permission_profile_hash does not match profile body",
        ), 2

    requested = request.get("requested_capabilities")
    if requested is None:
        return _err("requested_capabilities_missing", "requested_capabilities required"), 2
    if not isinstance(requested, list):
        return _err("requested_capabilities_invalid", "requested_capabilities must be a list"), 2
    if requested:
        return _err("capability_forbidden", "fixture mode rejects non-empty capability requests"), 2

    task_kind = request["task_kind"]
    if task_kind == "analysis":
        evidence = {
            "summary_digest": _sha256_text(request["task_input_hash"]),
            "findings_count": 1,
            "scope_paths": allowed_paths,
        }
        return (
            _bound_result(
                request,
                changed_paths=[],
                patch=None,
                patch_sha256=None,
                analysis=evidence,
                tool_summary=_empty_tool_summary(tool_call_count=0),
                reason_code="fixture_analysis_ok",
            ),
            0,
        )

    if task_kind == "allowed_path_patch":
        target = allowed_paths[0]
        if not target.endswith(".md") and not target.endswith(".txt"):
            return _err("patch_target_invalid", "fixture patch target must be text path"), 2
        patch_body = (
            f"*** Begin Patch\n*** Add File: {target}\n"
            f"+# OpenCode fixture patch\n"
            f"+generated_by=acp_opencode_adapter fixture\n"
            f"*** End Patch\n"
        )
        patch_sha = _sha256_text(patch_body)
        if len(patch_body.encode("utf-8")) > 64 * 1024:
            return _err("patch_oversized", "patch exceeds bound"), 2
        return (
            _bound_result(
                request,
                changed_paths=[target],
                patch=patch_body,
                patch_sha256=patch_sha,
                analysis=None,
                tool_summary=_empty_tool_summary(tool_call_count=1),
                reason_code="fixture_patch_ok",
            ),
            0,
        )

    if task_kind == "path_escape":
        return _err("path_escape_rejected", "path traversal rejected"), 2
    if task_kind == "network_attempt":
        return _err("network_forbidden", "network attempt rejected"), 2

    if task_kind == "descendant_spawn":
        child = subprocess.Popen(  # noqa: S603 — fixture-only local sleep
            ["sleep", "120"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=False,
        )
        try:
            time.sleep(60)
        finally:
            if child.poll() is None:
                try:
                    child.kill()
                except OSError:
                    pass
        return _err("descendant_spawn_should_timeout", "descendant fixture should not complete"), 2

    if task_kind == "early_exit_with_descendant":
        # Parent returns immediately while an un-reaped child remains in the session.
        # Rust must kill the process group and report containment failure if needed.
        subprocess.Popen(  # noqa: S603 — fixture-only local sleep
            ["sleep", "120"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=False,
        )
        return (
            _err(
                "early_exit_with_descendant",
                "parent exited while descendant remains for containment test",
            ),
            2,
        )

    if task_kind == "stdin_close_early":
        # Exit before consuming full stdin so the parent write path can observe EPIPE.
        try:
            sys.stdin.close()
        except Exception:
            pass
        return _err("stdin_closed_early", "adapter closed stdin without consuming request"), 2

    return _err("task_kind_unsupported", f"unsupported task_kind: {task_kind}"), 2
