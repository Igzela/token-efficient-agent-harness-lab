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

    def metrics(self) -> dict[str, Any]:
        return self._get("/api/v1/metrics")

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

    def plans(
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
        return self._get(_query_path("/api/v1/plans", params))

    def create_plan(
        self,
        raw_request: str,
        request_source: RequestSource = "api",
    ) -> dict[str, Any]:
        return self._post(
            "/api/v1/plans",
            {"raw_request": raw_request, "request_source": request_source},
        )

    def plan(self, plan_id: str) -> dict[str, Any]:
        return self._get(f"/api/v1/plans/{_quote_path_segment(plan_id)}")

    def workflow_runs(
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
        return self._get(_query_path("/api/v1/workflow-runs", params))

    def create_workflow_run(self, plan_id: str) -> dict[str, Any]:
        return self._post("/api/v1/workflow-runs", {"plan_id": plan_id})

    def workflow_run(self, run_id: str) -> dict[str, Any]:
        return self._get(f"/api/v1/workflow-runs/{_quote_path_segment(run_id)}")

    def workflow_run_events(self, run_id: str, limit: int | None = None) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        return self._get(
            _query_path(f"/api/v1/workflow-runs/{_quote_path_segment(run_id)}/events", params)
        )

    def record_workflow_run_event(
        self,
        run_id: str,
        event_type: str,
        node_id: str | None = None,
        details: Any | None = None,
    ) -> dict[str, Any]:
        payload: dict[str, Any] = {"event_type": event_type}
        if node_id is not None:
            payload["node_id"] = node_id
        if details is not None:
            payload["details"] = details
        return self._post(
            f"/api/v1/workflow-runs/{_quote_path_segment(run_id)}/events",
            payload,
        )

    def workflow_run_approvals(self, run_id: str, limit: int | None = None) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        return self._get(
            _query_path(f"/api/v1/workflow-runs/{_quote_path_segment(run_id)}/approvals", params)
        )

    def record_workflow_run_approval(
        self,
        run_id: str,
        node_id: str,
        decision: str,
        reason: str | None = None,
    ) -> dict[str, Any]:
        payload = {"node_id": node_id, "decision": decision}
        if reason is not None:
            payload["reason"] = reason
        return self._post(
            f"/api/v1/workflow-runs/{_quote_path_segment(run_id)}/approvals",
            payload,
        )

    def resume_workflow_run(self, run_id: str, reason: str | None = None) -> dict[str, Any]:
        payload: dict[str, Any] = {}
        if reason is not None:
            payload["reason"] = reason
        return self._post(f"/api/v1/workflow-runs/{_quote_path_segment(run_id)}/resume", payload)

    def cancel_workflow_run(self, run_id: str, reason: str | None = None) -> dict[str, Any]:
        payload: dict[str, Any] = {}
        if reason is not None:
            payload["reason"] = reason
        return self._post(f"/api/v1/workflow-runs/{_quote_path_segment(run_id)}/cancel", payload)

    def supervised_patch_workspaces(self, limit: int | None = None) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        return self._get(_query_path("/api/v1/supervised-patch/workspaces", params))

    def supervised_patch_workspace_detail(self, workspace_id: str) -> dict[str, Any]:
        return self._get(
            f"/api/v1/supervised-patch/workspaces/{_quote_path_segment(workspace_id)}"
        )

    def supervised_patch_artifacts(self, limit: int | None = None) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        return self._get(_query_path("/api/v1/supervised-patch/artifacts", params))

    def supervised_patch_artifact_detail(self, artifact_id: str) -> dict[str, Any]:
        return self._get(
            f"/api/v1/supervised-patch/artifacts/{_quote_path_segment(artifact_id)}"
        )

    def create_supervised_patch_workspace(
        self,
        run_id: str,
        target_id: str,
        target_repo_path: str,
        source_revision: str,
        plan_id: str | None = None,
        source_tree_hash: str | None = None,
    ) -> dict[str, Any]:
        body: dict[str, Any] = {
            "run_id": run_id,
            "target_id": target_id,
            "target_repo_path": target_repo_path,
            "source_revision": source_revision,
        }
        if plan_id is not None:
            body["plan_id"] = plan_id
        if source_tree_hash is not None:
            body["source_tree_hash"] = source_tree_hash
        return self._post("/api/v1/supervised-patch/workspaces", body)

    def cleanup_supervised_patch_workspace(self, workspace_id: str) -> dict[str, Any]:
        return self._post(
            f"/api/v1/supervised-patch/workspaces/{_quote_path_segment(workspace_id)}/cleanup",
            {},
        )

    def quarantine_supervised_patch_workspace(self, workspace_id: str) -> dict[str, Any]:
        return self._post(
            f"/api/v1/supervised-patch/workspaces/{_quote_path_segment(workspace_id)}/quarantine",
            {},
        )

    def capture_supervised_patch(self, workspace_id: str) -> dict[str, Any]:
        return self._post(
            f"/api/v1/supervised-patch/workspaces/{_quote_path_segment(workspace_id)}/capture",
            {},
        )

    def export_supervised_patch_artifact(
        self, artifact_id: str, run_id: str
    ) -> dict[str, Any]:
        return self._post(
            f"/api/v1/supervised-patch/artifacts/{_quote_path_segment(artifact_id)}/export",
            {"run_id": run_id},
        )

    def tick_workflow_run(
        self,
        run_id: str,
        actor: str | None = None,
        max_retries: int | None = None,
        executor: str | None = None,
        timeout_ms: int | None = None,
        command: str | None = None,
    ) -> dict[str, Any]:
        body: dict[str, Any] = {}
        if actor is not None:
            body["actor"] = actor
        if max_retries is not None:
            body["max_retries"] = max_retries
        if executor is not None:
            body["executor"] = executor
        if timeout_ms is not None:
            body["timeout_ms"] = timeout_ms
        if command is not None:
            body["command"] = command
        return self._post(
            f"/api/v1/workflow-runs/{_quote_path_segment(run_id)}/tick", body
        )

    def scheduler_status(self) -> dict[str, Any]:
        return self._get("/api/v1/scheduler/status")

    def fetch_executor_pool(self) -> dict[str, Any]:
        return self._get("/api/v1/executor-pool")

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
        search: str | None = None,
        redact: bool | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        if offset is not None:
            params["offset"] = offset
        if search:
            params["search"] = search
        if redact is not None:
            params["redact"] = "true" if redact else "false"
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

    def verify_backup(self, backup_id: str) -> dict[str, Any]:
        return self._get(f"/api/v1/backups/{_quote_path_segment(backup_id)}/verify")

    def restore_backup_dry_run(self, backup_id: str) -> dict[str, Any]:
        return self._post(
            f"/api/v1/backups/{_quote_path_segment(backup_id)}/restore/dry-run",
            {"confirm_restore_dry_run": True},
        )

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
