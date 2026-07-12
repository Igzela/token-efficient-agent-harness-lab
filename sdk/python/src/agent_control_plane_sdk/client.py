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

    def regressions(
        self, scenario_id: str | None = None, limit: int | None = None
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if scenario_id is not None:
            params["scenario_id"] = scenario_id
        if limit is not None:
            params["limit"] = limit
        return self._get(_query_path("/api/v1/regressions", params))

    def regression(self, artifact_id: str) -> dict[str, Any]:
        return self._get(f"/api/v1/regressions/{_quote_path_segment(artifact_id)}")

    def regression_trend(
        self, scenario_id: str, limit: int | None = None
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        path = f"/api/v1/regressions/trends/{_quote_path_segment(scenario_id)}"
        return self._get(_query_path(path, params))

    def budget_evidence(
        self, kind: str | None = None, limit: int | None = None, offset: int | None = None
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if kind is not None:
            params["kind"] = kind
        if limit is not None:
            params["limit"] = limit
        if offset is not None:
            params["offset"] = offset
        return self._get(_query_path("/api/v1/budget-evidence", params))

    def budget_evidence_artifact(self, artifact_id: str) -> dict[str, Any]:
        return self._get(
            f"/api/v1/budget-evidence/{_quote_path_segment(artifact_id)}"
        )

    def offline_replay_artifacts(
        self,
        status: str | None = None,
        limit: int | None = None,
        offset: int | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if status is not None:
            params["status"] = status
        if limit is not None:
            params["limit"] = limit
        if offset is not None:
            params["offset"] = offset
        return self._get(_query_path("/api/v1/offline-replays", params))

    def offline_replay_artifact(self, artifact_id: str) -> dict[str, Any]:
        return self._get(
            f"/api/v1/offline-replays/{_quote_path_segment(artifact_id)}"
        )

    def operator_decisions(
        self,
        generated_at: str | None = None,
        maximum_freshness_seconds: int | None = None,
        limit: int | None = None,
        offset: int | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        for key, value in {
            "generated_at": generated_at,
            "maximum_freshness_seconds": maximum_freshness_seconds,
            "limit": limit,
            "offset": offset,
        }.items():
            if value is not None:
                params[key] = value
        return self._get(_query_path("/api/v1/operator/decisions", params))

    def apply_operator_decision(
        self,
        decision_id: str,
        *,
        queue_sha256: str,
        generated_at: str,
        maximum_freshness_seconds: int,
        limit: int,
        offset: int,
        action: str,
        confirm_action: bool,
        reason: str | None = None,
        budget_policy: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        return self._post(
            f"/api/v1/operator/decisions/{_quote_path_segment(decision_id)}/actions",
            {
                "queue_sha256": queue_sha256,
                "generated_at": generated_at,
                "maximum_freshness_seconds": maximum_freshness_seconds,
                "limit": limit,
                "offset": offset,
                "action": action,
                "confirm_action": confirm_action,
                "reason": reason,
                "budget_policy": budget_policy,
            },
        )

    def metrics(self) -> dict[str, Any]:
        return self._get("/api/v1/metrics")

    def dispatch_metrics(self, limit: int | None = None) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        return self._get(_query_path("/api/v1/dispatch-metrics", params))

    def feedback_traces(
        self,
        limit: int | None = None,
        offset: int | None = None,
        task_class: str | None = None,
        tier: str | None = None,
        status: str | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        if offset is not None:
            params["offset"] = offset
        if task_class:
            params["task_class"] = task_class
        if tier:
            params["tier"] = tier
        if status:
            params["status"] = status
        return self._get(_query_path("/api/v1/feedback/traces", params))

    def feedback_cost_of_pass(
        self,
        task_class: str | None = None,
        tier: str | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if task_class:
            params["task_class"] = task_class
        if tier:
            params["tier"] = tier
        return self._get(_query_path("/api/v1/feedback/cost-of-pass", params))

    def feedback_patterns(
        self,
        task_class: str | None = None,
        tier: str | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if task_class:
            params["task_class"] = task_class
        if tier:
            params["tier"] = tier
        return self._get(_query_path("/api/v1/feedback/patterns", params))

    def simulation_report(self, limit: int | None = None) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        return self._get(_query_path("/api/v1/simulation/report", params))

    def policy_simulation_report(
        self, limit: int | None = None, policy: str | None = None
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        if policy is not None:
            params["policy"] = policy
        return self._get(_query_path("/api/v1/simulation/policy-delta", params))

    def proposals(
        self,
        limit: int | None = None,
        offset: int | None = None,
        status: str | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        if offset is not None:
            params["offset"] = offset
        if status:
            params["status"] = status
        return self._get(_query_path("/api/v1/proposals", params))

    def generated_proposals(self, limit: int = 50) -> dict[str, Any]:
        """Get auto-generated policy proposal candidates from feedback and simulation."""
        return self._get(f"/api/v1/proposals/generated?limit={limit}")

    def create_proposal(
        self,
        payload: dict[str, Any],
        title: str | None = None,
        summary: str | None = None,
        task_class: str | None = None,
        task_domain: str | None = None,
        task_intent: str | None = None,
        tier: str | None = None,
        target_tier: str | None = None,
        evidence: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        body: dict[str, Any] = {"payload": payload}
        if title is not None:
            body["title"] = title
        if summary is not None:
            body["summary"] = summary
        if task_class is not None:
            body["task_class"] = task_class
        if task_domain is not None:
            body["task_domain"] = task_domain
        if task_intent is not None:
            body["task_intent"] = task_intent
        if tier is not None:
            body["tier"] = tier
        if target_tier is not None:
            body["target_tier"] = target_tier
        if evidence is not None:
            body["evidence"] = evidence
        return self._post("/api/v1/proposals", body)

    def proposal(self, proposal_id: str) -> dict[str, Any]:
        return self._get(f"/api/v1/proposals/{_quote_path_segment(proposal_id)}")

    def approve_proposal(
        self,
        proposal_id: str,
        actor: str | None = None,
        reason: str | None = None,
        confirm_policy_override: bool = True,
    ) -> dict[str, Any]:
        return self._proposal_action(
            proposal_id,
            "approve",
            actor=actor,
            reason=reason,
            confirm_policy_override=confirm_policy_override,
        )

    def reject_proposal(
        self,
        proposal_id: str,
        actor: str | None = None,
        reason: str | None = None,
    ) -> dict[str, Any]:
        return self._proposal_action(proposal_id, "reject", actor=actor, reason=reason)

    def rollback_proposal(
        self,
        proposal_id: str,
        actor: str | None = None,
        reason: str | None = None,
        confirm_policy_override: bool = True,
    ) -> dict[str, Any]:
        return self._proposal_action(
            proposal_id,
            "rollback",
            actor=actor,
            reason=reason,
            confirm_policy_override=confirm_policy_override,
        )

    def deactivate_proposal(
        self,
        proposal_id: str,
        actor: str | None = None,
        reason: str | None = None,
        confirm_policy_override: bool = True,
    ) -> dict[str, Any]:
        return self._proposal_action(
            proposal_id,
            "deactivate",
            actor=actor,
            reason=reason,
            confirm_policy_override=confirm_policy_override,
        )

    def _proposal_action(
        self,
        proposal_id: str,
        action: str,
        actor: str | None = None,
        reason: str | None = None,
        confirm_policy_override: bool | None = None,
    ) -> dict[str, Any]:
        body: dict[str, Any] = {}
        if actor is not None:
            body["actor"] = actor
        if reason is not None:
            body["reason"] = reason
        if confirm_policy_override is not None:
            body["confirm_policy_override"] = confirm_policy_override
        return self._post(
            f"/api/v1/proposals/{_quote_path_segment(proposal_id)}/{action}",
            body,
        )

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
        workspace_mode: str | None = None,
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
        if workspace_mode is not None:
            body["workspace_mode"] = workspace_mode
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

    def verify_supervised_patch_workspace(
        self,
        workspace_id: str,
        command: str,
        confirm_verification: bool,
        timeout_ms: int | None = None,
        attempt: int | None = None,
        repair_executor: str | None = None,
        max_repair_attempts: int | None = None,
    ) -> dict[str, Any]:
        body: dict[str, Any] = {
            "command": command,
            "confirm_verification": confirm_verification,
        }
        if timeout_ms is not None:
            body["timeout_ms"] = timeout_ms
        if attempt is not None:
            body["attempt"] = attempt
        if repair_executor is not None:
            body["repair_executor"] = repair_executor
        if max_repair_attempts is not None:
            body["max_repair_attempts"] = max_repair_attempts
        return self._post(
            f"/api/v1/supervised-patch/workspaces/{_quote_path_segment(workspace_id)}/verify",
            body,
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

    def target_repo_output(
        self,
        artifact_id: str,
        run_id: str,
        mode: str,
        confirm_target_output: bool,
        branch_name: str | None = None,
        remote: str | None = None,
        commit_message: str | None = None,
        pr_title: str | None = None,
        create_pull_request: bool | None = None,
    ) -> dict[str, Any]:
        body: dict[str, Any] = {
            "run_id": run_id,
            "mode": mode,
            "confirm_target_output": confirm_target_output,
        }
        if branch_name is not None:
            body["branch_name"] = branch_name
        if remote is not None:
            body["remote"] = remote
        if commit_message is not None:
            body["commit_message"] = commit_message
        if pr_title is not None:
            body["pr_title"] = pr_title
        if create_pull_request is not None:
            body["create_pull_request"] = create_pull_request
        return self._post(
            f"/api/v1/supervised-patch/artifacts/{_quote_path_segment(artifact_id)}/output",
            body,
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

    def control_scheduler(
        self,
        action: str,
        actor: str | None = None,
        confirm_control: bool = True,
    ) -> dict[str, Any]:
        if action not in {"pause", "resume", "kill"}:
            raise ValueError("action must be pause, resume, or kill")
        body: dict[str, Any] = {
            "action": action,
            "confirm_control": confirm_control,
        }
        if actor is not None:
            body["actor"] = actor
        return self._post("/api/v1/scheduler/control", body)

    def fetch_executor_pool(self) -> dict[str, Any]:
        return self._get("/api/v1/executor-pool")

    def fetch_queue_status(self) -> dict[str, Any]:
        return self._get("/api/v1/queue/status")

    def fetch_queue_runs(
        self,
        limit: int | None = None,
        offset: int | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        if offset is not None:
            params["offset"] = offset
        return self._get(_query_path("/api/v1/queue/runs", params))

    def update_run_priority(self, run_id: str, priority: int) -> dict[str, Any]:
        return self._put(
            f"/api/v1/queue/runs/{_quote_path_segment(run_id)}/priority",
            {"priority": priority},
        )

    def pause_run(self, run_id: str, reason: str | None = None) -> dict[str, Any]:
        return self._put(
            f"/api/v1/queue/runs/{_quote_path_segment(run_id)}/pause",
            {"reason": reason},
        )

    def fetch_queue_tenants(self) -> dict[str, Any]:
        return self._get("/api/v1/queue/tenants")

    def decisions(
        self,
        limit: int | None = None,
        offset: int | None = None,
        search: str | None = None,
        run_id: str | None = None,
    ) -> dict[str, Any]:
        params: dict[str, Any] = {}
        if limit is not None:
            params["limit"] = limit
        if offset is not None:
            params["offset"] = offset
        if search:
            params["search"] = search
        if run_id:
            params["run_id"] = run_id
        return self._get(_query_path("/api/v1/decisions", params))

    def decision_detail(self, decision_id: str) -> dict[str, Any]:
        return self._get(f"/api/v1/decisions/{_quote_path_segment(decision_id)}")

    def decision_stats(self) -> dict[str, Any]:
        return self._get("/api/v1/decisions/stats")

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

    def _put(self, path: str, body: dict[str, Any]) -> Any:
        data = json.dumps(body).encode("utf-8")
        headers = {**self._headers(), "content-type": "application/json"}
        request = Request(f"{self.base_url}{path}", data=data, headers=headers, method="PUT")
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
