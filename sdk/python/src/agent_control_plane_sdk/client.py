from __future__ import annotations

import json
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from .wire_types import ApiStatus, DispatchBundle, DispatchRequest, LocalCostSummary, LocalDispatchCostDetail, RequestSource


class AgentControlPlaneError(RuntimeError):
    pass


class AgentControlPlaneClient:
    def __init__(self, base_url: str, api_key: str | None = None, timeout: float = 10.0):
        self.base_url = base_url.rstrip("/")
        self.api_key = api_key
        self.timeout = timeout

    def health(self) -> ApiStatus:
        return self._get("/api/v1/health")

    def ready(self) -> ApiStatus:
        return self._get("/api/v1/ready")

    def openapi(self) -> dict[str, Any]:
        return self._get("/api/v1/openapi.json")

    def dashboard(self) -> dict[str, Any]:
        return self._get("/api/v1/dashboard")

    def dispatches(
        self,
        limit: int | None = None,
        offset: int | None = None,
        search: str | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        if offset is not None:
            params["offset"] = offset
        if search:
            params["search"] = search
        return self._get(_query_path("/api/v1/dispatches", params))

    def config(self) -> dict[str, Any]:
        return self._get("/api/v1/config")

    def team(self) -> dict[str, Any]:
        return self._get("/api/v1/team")

    def costs(self) -> LocalCostSummary:
        return self._get("/api/v1/costs")

    def cost_details(self, limit: int | None = None) -> LocalDispatchCostDetail:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        return self._get(_query_path("/api/v1/costs/dispatches", params))

    def export_state(self) -> dict[str, Any]:
        return self._get("/api/v1/export")

    def audit(
        self,
        limit: int | None = None,
        offset: int | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        if offset is not None:
            params["offset"] = offset
        return self._get(_query_path("/api/v1/audit", params))

    def provider_health(self) -> dict[str, Any]:
        return self._get("/api/v1/provider/health")

    def provider_audit(
        self,
        limit: int | None = None,
        offset: int | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        if offset is not None:
            params["offset"] = offset
        return self._get(_query_path("/api/v1/provider/audit", params))

    def dispatch(
        self,
        raw_request: str,
        request_source: RequestSource = "api",
    ) -> DispatchBundle:
        request = DispatchRequest(raw_request=raw_request, request_source=request_source)
        return self._post("/api/v1/dispatch", request.to_json())

    def create_backup(
        self,
        label: str | None = None,
        confirm_local_backup: bool = False,
    ) -> dict[str, Any]:
        return self._post(
            "/api/v1/backups",
            {
                "label": label,
                "confirm_local_backup": confirm_local_backup,
            },
        )

    def list_api_keys(self) -> dict[str, Any]:
        return self._get("/api/v1/keys")

    def create_api_key(
        self,
        user_id: str,
        role: str,
        scopes: list[str],
        expires_at: float | None = None,
    ) -> dict[str, Any]:
        body: dict[str, Any] = {"user_id": user_id, "role": role, "scopes": scopes}
        if expires_at is not None:
            body["expires_at"] = expires_at
        return self._post("/api/v1/keys", body)

    def revoke_api_key(self, key_id: str) -> dict[str, Any]:
        return self._post(f"/api/v1/keys/{_quote_path_segment(key_id)}/revoke", {})

    def rotate_api_key(self, key_id: str) -> dict[str, Any]:
        return self._post(f"/api/v1/keys/{_quote_path_segment(key_id)}/rotate", {})

    def delete_api_key(self, key_id: str) -> dict[str, Any]:
        url = f"{self.base_url}/api/v1/keys/{_quote_path_segment(key_id)}"
        req = Request(url, method="DELETE")
        for k, v in self._headers().items():
            req.add_header(k, v)
        return self._send(req)

    def update_key_scopes(self, key_id: str, scopes: list[str]) -> dict[str, Any]:
        return self._post(f"/api/v1/keys/{_quote_path_segment(key_id)}/scopes", {"scopes": scopes})

    def create_team_member(
        self, user_id: str, display_name: str, role: str
    ) -> dict[str, Any]:
        return self._post(
            "/api/v1/team",
            {"user_id": user_id, "display_name": display_name, "role": role},
        )

    def update_member_role(self, user_id: str, role: str) -> dict[str, Any]:
        url = f"{self.base_url}/api/v1/team/{_quote_path_segment(user_id)}"
        data = json.dumps({"role": role}).encode("utf-8")
        req = Request(url, data=data, method="PUT")
        req.add_header("content-type", "application/json")
        for k, v in self._headers().items():
            req.add_header(k, v)
        return self._send(req)

    def delete_member(self, user_id: str) -> dict[str, Any]:
        url = f"{self.base_url}/api/v1/team/{_quote_path_segment(user_id)}"
        req = Request(url, method="DELETE")
        for k, v in self._headers().items():
            req.add_header(k, v)
        return self._send(req)

    def dispatch_detail(self, dispatch_id: str) -> dict[str, Any]:
        return self._get(f"/api/v1/dispatches/{_quote_path_segment(dispatch_id)}")

    def list_backups(self) -> dict[str, Any]:
        return self._get("/api/v1/backups")

    def delete_backup(self, backup_id: str) -> dict[str, Any]:
        url = f"{self.base_url}/api/v1/backups/{_quote_path_segment(backup_id)}"
        req = Request(url, method="DELETE")
        for k, v in self._headers().items():
            req.add_header(k, v)
        return self._send(req)

    def storage_integrity(self) -> dict[str, Any]:
        return self._get("/api/v1/storage/integrity")

    def import_snapshot(self, snapshot: dict[str, Any]) -> dict[str, Any]:
        return self._post("/api/v1/import", {"snapshot": snapshot, "confirm_import": True})

    def restore_backup(self, backup_id: str) -> dict[str, Any]:
        return self._post(f"/api/v1/backups/{_quote_path_segment(backup_id)}/restore", {"confirm_restore": True})

    def _get(self, path: str) -> Any:
        request = Request(f"{self.base_url}{path}", headers=self._headers(), method="GET")
        return self._send(request)

    def _post(self, path: str, body: dict[str, Any]) -> Any:
        data = json.dumps(body).encode("utf-8")
        headers = {**self._headers(), "content-type": "application/json"}
        request = Request(f"{self.base_url}{path}", data=data, headers=headers, method="POST")
        return self._send(request)

    def _headers(self) -> dict[str, str]:
        if self.api_key is None:
            return {}
        return {"authorization": f"Bearer {self.api_key}"}

    def _send(self, request: Request) -> Any:
        try:
            with urlopen(request, timeout=self.timeout) as response:
                payload = response.read().decode("utf-8")
        except HTTPError as exc:
            payload = exc.read().decode("utf-8")
            try:
                body = json.loads(payload)
                message = body.get("error", exc.reason)
            except json.JSONDecodeError:
                message = exc.reason
            raise AgentControlPlaneError(str(message)) from exc
        return json.loads(payload) if payload else None


def _query_path(path: str, params: dict[str, Any]) -> str:
    query = "&".join(
        f"{_quote_query_component(key)}={_quote_query_component(str(value))}"
        for key, value in params.items()
    )
    return f"{path}?{query}" if query else path


def _quote_query_component(value: str) -> str:
    return _quote_component(value, space_as_plus=True)


def _quote_path_segment(value: str) -> str:
    return _quote_component(value, space_as_plus=False)


def _quote_component(value: str, space_as_plus: bool) -> str:
    encoded = []
    for byte in value.encode("utf-8"):
        if (
            ord("a") <= byte <= ord("z")
            or ord("A") <= byte <= ord("Z")
            or ord("0") <= byte <= ord("9")
            or byte in b"-._~"
        ):
            encoded.append(chr(byte))
        elif byte == ord(" ") and space_as_plus:
            encoded.append("+")
        else:
            encoded.append(f"%{byte:02X}")
    return "".join(encoded)
