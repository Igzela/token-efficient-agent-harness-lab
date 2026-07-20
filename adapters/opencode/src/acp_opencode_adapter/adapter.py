"""Deterministic OpenCode fixture adapter — no network, no real binary."""

from __future__ import annotations

import hashlib
import json
import re
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
_FORBIDDEN_MODES = frozenset(
    {
        "live",
        "network",
        "auto",
        "mcp",
        "websearch",
        "webfetch",
        "remote_agent",
        "background_agent",
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


def handle_request(request: dict[str, Any]) -> tuple[dict[str, Any], int]:
    if not isinstance(request, dict):
        return _err("request_invalid", "request must be an object"), 2
    if request.get("schema_version") != REQUEST_SCHEMA:
        return _err("request_schema_invalid", "unsupported request schema"), 2
    mode = request.get("mode")
    if mode != "fixture":
        return _err("mode_forbidden", "only fixture mode is accepted"), 2
    if request.get("runtime_kind") != RUNTIME_KIND:
        return _err("runtime_kind_invalid", "runtime_kind must be opencode"), 2
    for field in (
        "invocation_id",
        "run_id",
        "node_id",
        "lease_id",
        "task_kind",
        "base_commit",
        "worktree_id",
    ):
        value = request.get(field)
        if not isinstance(value, str) or not _ID_RE.match(value):
            return _err("request_field_invalid", f"{field} is invalid"), 2
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
    for name in env_allowlist:
        if not isinstance(name, str) or name in {
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
        }:
            return _err("environment_forbidden", f"undeclared or forbidden env: {name}"), 2
    profile = request.get("permission_profile")
    if not isinstance(profile, dict):
        return _err("permission_profile_invalid", "permission_profile required"), 2
    if profile.get("approval_mode") != "deny_by_default":
        return _err("permission_escalation", "approval_mode must be deny_by_default"), 2
    if profile.get("network_enabled") is not False:
        return _err("network_forbidden", "network must be disabled"), 2
    if profile.get("mcp_enabled") is not False:
        return _err("mcp_forbidden", "mcp must be disabled"), 2
    for flag in ("websearch", "webfetch", "remote_agents", "background_agents", "provider_fallback"):
        if profile.get(flag) is not False:
            return _err("capability_forbidden", f"{flag} must be false"), 2
    if any(mode in _FORBIDDEN_MODES for mode in (request.get("requested_capabilities") or [])):
        return _err("capability_forbidden", "requested capability is forbidden"), 2

    task_kind = request["task_kind"]
    if task_kind == "analysis":
        evidence = {
            "summary_digest": _sha256_text(request["task_input_hash"]),
            "findings_count": 1,
            "scope_paths": allowed_paths,
        }
        result = {
            "schema_version": RESULT_SCHEMA,
            "invocation_id": request["invocation_id"],
            "run_id": request["run_id"],
            "node_id": request["node_id"],
            "lease_id": request["lease_id"],
            "task_kind": "analysis",
            "status": "ok",
            "runtime": {
                "runtime_kind": RUNTIME_KIND,
                "runtime_version": PINNED_OPENCODE_VERSION,
                "adapter_version": ADAPTER_VERSION,
                "adapter_contract_version": ADAPTER_CONTRACT,
                "mode": "fixture",
            },
            "changed_paths": [],
            "patch": None,
            "patch_sha256": None,
            "analysis": evidence,
            "tool_summary": {"tool_call_count": 0, "network_attempts": 0, "mcp_attempts": 0},
            "reason_code": "fixture_analysis_ok",
        }
        return result, 0

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
        result = {
            "schema_version": RESULT_SCHEMA,
            "invocation_id": request["invocation_id"],
            "run_id": request["run_id"],
            "node_id": request["node_id"],
            "lease_id": request["lease_id"],
            "task_kind": "allowed_path_patch",
            "status": "ok",
            "runtime": {
                "runtime_kind": RUNTIME_KIND,
                "runtime_version": PINNED_OPENCODE_VERSION,
                "adapter_version": ADAPTER_VERSION,
                "adapter_contract_version": ADAPTER_CONTRACT,
                "mode": "fixture",
            },
            "changed_paths": [target],
            "patch": patch_body,
            "patch_sha256": patch_sha,
            "analysis": None,
            "tool_summary": {"tool_call_count": 1, "network_attempts": 0, "mcp_attempts": 0},
            "reason_code": "fixture_patch_ok",
        }
        return result, 0

    if task_kind == "path_escape":
        return _err("path_escape_rejected", "path traversal rejected"), 2
    if task_kind == "network_attempt":
        return _err("network_forbidden", "network attempt rejected"), 2

    return _err("task_kind_unsupported", f"unsupported task_kind: {task_kind}"), 2
