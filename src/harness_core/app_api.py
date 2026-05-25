"""Pure API handlers for the local Harness app control plane."""

from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
from typing import Any

from harness_core.app_registry import (
    AppRegistry,
    AppRegistryError,
    RepoRef,
    registry_to_dict,
)
from harness_core.instance_audit import audit_instance


@dataclass(frozen=True)
class AppApiResponse:
    """HTTP-shaped response returned by pure API handlers."""

    status_code: int
    body_json: dict[str, Any]
    headers: dict[str, str] | None = None

    def body_bytes(self) -> bytes:
        return json.dumps(self.body_json, ensure_ascii=False, sort_keys=True).encode("utf-8")

    def response_headers(self) -> dict[str, str]:
        base = {"Content-Type": "application/json; charset=utf-8"}
        if self.headers:
            base.update(self.headers)
        return base


def handle_api_request(method: str, path: str, body: bytes | str | None, registry_path: str | Path) -> AppApiResponse:
    """Handle one API request without starting a server."""

    route, query_string = _split_path(path)
    method = method.upper()

    try:
        if route == "/api/health" and method == "GET":
            return _json(200, {"status": "ok", "mode": "local_read_only_control_plane"})
        if route == "/api/repos" and method == "GET":
            return _list_repos(registry_path)
        if route == "/api/repos" and method == "POST":
            return _add_repo(body, registry_path)
        if route == "/api/audit" and method == "GET":
            query = _parse_query(query_string)
            repo_id = _single_query_value(query, "repo_id")
            return _audit_repo(repo_id, registry_path)
        if route.startswith("/api/"):
            return _error(404, "not_found", "API route not found")
        return _error(404, "not_found", "Route not found")
    except AppRegistryError as exc:
        return _error(400, "invalid_registry_request", str(exc))
    except json.JSONDecodeError:
        return _error(400, "invalid_json", "Request body must be valid JSON")
    except OSError:
        return _error(500, "registry_io_error", "Registry file could not be read or written")


def _list_repos(registry_path: str | Path) -> AppApiResponse:
    registry = AppRegistry.load(registry_path)
    return _json(200, registry_to_dict(registry))


def _add_repo(body: bytes | str | None, registry_path: str | Path) -> AppApiResponse:
    if body is None:
        return _error(400, "missing_body", "Request body is required")
    if isinstance(body, bytes):
        body_text = body.decode("utf-8")
    else:
        body_text = body

    data = json.loads(body_text)
    if not isinstance(data, dict):
        return _error(400, "invalid_json", "Request body must be an object")

    registry = AppRegistry.load(registry_path)
    updated = registry.add_repo(RepoRef.from_dict(data))
    updated.save(registry_path)
    repo = updated.get_repo(data["id"])
    return _json(201, {"repo": repo.to_dict() if repo else data})


def _audit_repo(repo_id: str | None, registry_path: str | Path) -> AppApiResponse:
    if not repo_id:
        return _error(400, "missing_repo_id", "repo_id query parameter is required")

    registry = AppRegistry.load(registry_path)
    repo = registry.get_repo(repo_id)
    if repo is None:
        return _error(404, "invalid_repo_id", "Repo id not found")

    if repo.kind == "remote":
        return _json(
            409,
            {
                "error": {
                    "code": "remote_audit_unsupported",
                    "message": "Remote repositories are metadata-only until registered as a local path.",
                },
                "repo": repo.to_dict(),
            },
        )

    if not repo.path:
        return _error(400, "invalid_repo", "Local repo is missing path")
    report = audit_instance(repo.path)
    return _json(200, {"repo": repo.to_dict(), "audit": report.to_dict()})


def _single_query_value(query: dict[str, list[str]], key: str) -> str | None:
    values = query.get(key)
    if not values:
        return None
    return values[0]


def _split_path(path: str) -> tuple[str, str]:
    if "?" not in path:
        return path, ""
    route, query_string = path.split("?", 1)
    return route, query_string


def _parse_query(query_string: str) -> dict[str, list[str]]:
    result: dict[str, list[str]] = {}
    if not query_string:
        return result
    for part in query_string.split("&"):
        if not part or "=" not in part:
            continue
        key, value = part.split("=", 1)
        if not key or not value:
            continue
        result.setdefault(key, []).append(value)
    return result


def _json(status_code: int, body: dict[str, Any]) -> AppApiResponse:
    return AppApiResponse(status_code=status_code, body_json=body)


def _error(status_code: int, code: str, message: str) -> AppApiResponse:
    return _json(status_code, {"error": {"code": code, "message": message}})
