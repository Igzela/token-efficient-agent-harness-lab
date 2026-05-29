from __future__ import annotations

import json
from typing import Any
from urllib.error import HTTPError
from urllib.request import Request, urlopen

from .wire_types import ApiStatus, DispatchBundle, DispatchRequest, RequestSource


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

    def dispatches(self) -> dict[str, Any]:
        return self._get("/api/v1/dispatches")

    def config(self) -> dict[str, Any]:
        return self._get("/api/v1/config")

    def team(self) -> dict[str, Any]:
        return self._get("/api/v1/team")

    def costs(self) -> dict[str, Any]:
        return self._get("/api/v1/costs")

    def export_state(self) -> dict[str, Any]:
        return self._get("/api/v1/export")

    def audit(self) -> dict[str, Any]:
        return self._get("/api/v1/audit")

    def provider_health(self) -> dict[str, Any]:
        return self._get("/api/v1/provider/health")

    def provider_audit(self) -> dict[str, Any]:
        return self._get("/api/v1/provider/audit")

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
