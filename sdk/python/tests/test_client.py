"""Tests for AgentControlPlaneClient."""

import json
import unittest
from io import BytesIO
from unittest.mock import MagicMock, patch
from urllib.error import HTTPError

from agent_control_plane_sdk.client import AgentControlPlaneClient, AgentControlPlaneError


def mock_response(data: dict, status: int = 200) -> MagicMock:
    body = json.dumps(data).encode("utf-8")
    resp = MagicMock()
    resp.read.return_value = body
    resp.status = status
    resp.__enter__ = MagicMock(return_value=resp)
    resp.__exit__ = MagicMock(return_value=False)
    return resp


def mock_http_error(status: int, body: dict) -> HTTPError:
    fp = BytesIO(json.dumps(body).encode("utf-8"))
    return HTTPError("http://test", status, "error", {}, fp)


class ClientHealthTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_health_sends_get(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "axum_api.v1", "status": "healthy"})
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.health()
        self.assertEqual(result["status"], "healthy")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertIn("/api/v1/health", req.full_url)


class ClientReadyTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_ready_sends_get(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "axum_api.v1", "status": "ready"})
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.ready()
        self.assertEqual(result["status"], "ready")


class ClientDispatchTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_dispatch_sends_post(self, mock_urlopen):
        bundle = {"record": {}, "analysis": {}, "decision": {}, "execution_result": {}, "evaluation_result": {}}
        mock_urlopen.return_value = mock_response(bundle)
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.dispatch("test request")
        self.assertEqual(result, bundle)
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        body = json.loads(req.data)
        self.assertEqual(body["raw_request"], "test request")
        self.assertEqual(body["request_source"], "api")

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_dispatch_preserves_cli_execution_result_fields(self, mock_urlopen):
        bundle = {
            "record": {},
            "analysis": {},
            "decision": {},
            "execution_result": {
                "executor_type": "codex_cli",
                "status": "cli_completed",
                "output": "codex ok",
                "input_tokens": 11,
                "output_tokens": 7,
                "estimated_cost": 0.000138,
                "usage_source": "codex_cli",
            },
            "evaluation_result": {},
        }
        mock_urlopen.return_value = mock_response(bundle)
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.dispatch("generate a Rust function")

        self.assertEqual(result["execution_result"]["executor_type"], "codex_cli")
        self.assertEqual(result["execution_result"]["status"], "cli_completed")
        self.assertEqual(result["execution_result"]["input_tokens"], 11)
        self.assertEqual(result["execution_result"]["usage_source"], "codex_cli")


class ClientLocalStateTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_dashboard_reads_local_dashboard_endpoint(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "local_dashboard.v1"})
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.dashboard()
        self.assertEqual(result["schema_version"], "local_dashboard.v1")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/dashboard")

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_local_state_readers_use_product_endpoints(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"ok": True})
        client = AgentControlPlaneClient("http://localhost:8080")
        client.dispatches()
        client.config()
        client.team()
        client.costs()
        client.export_state()
        client.audit()

        urls = [call.args[0].full_url for call in mock_urlopen.call_args_list]
        self.assertEqual(
            urls,
            [
                "http://localhost:8080/api/v1/dispatches",
                "http://localhost:8080/api/v1/config",
                "http://localhost:8080/api/v1/team",
                "http://localhost:8080/api/v1/costs",
                "http://localhost:8080/api/v1/export",
                "http://localhost:8080/api/v1/audit",
            ],
        )

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_regression_readers_use_bounded_endpoints(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"read_only": True})
        client = AgentControlPlaneClient("http://localhost:8080")

        client.regressions(scenario_id="scenario one", limit=25)
        client.regression("artifact/one")
        client.regression_trend("scenario/one", limit=10)

        urls = [call.args[0].full_url for call in mock_urlopen.call_args_list]
        self.assertEqual(
            urls,
            [
                "http://localhost:8080/api/v1/regressions?scenario_id=scenario+one&limit=25",
                "http://localhost:8080/api/v1/regressions/artifact%2Fone",
                "http://localhost:8080/api/v1/regressions/trends/scenario%2Fone?limit=10",
            ],
        )

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_budget_evidence_readers_encode_artifact_ids(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"read_only": True})
        client = AgentControlPlaneClient("http://localhost:8080")

        client.budget_evidence(kind="anomaly", limit=25, offset=5)
        client.budget_evidence_artifact("budget/anomaly one")

        urls = [call.args[0].full_url for call in mock_urlopen.call_args_list]
        self.assertEqual(
            urls,
            [
                "http://localhost:8080/api/v1/budget-evidence?kind=anomaly&limit=25&offset=5",
                "http://localhost:8080/api/v1/budget-evidence/budget%2Fanomaly%20one",
            ],
        )

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_memory_and_production_evidence_methods_preserve_requests(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({})
        client = AgentControlPlaneClient("http://localhost:8080")
        scope = {"tenant_id": "local", "workspace_id": "ws", "agent_id": "agent-1"}
        client.create_memory({"scope": scope, "run_id": "run-1", "source_id": "source-1"})
        client.memory("memory/one", "run-1")
        client.revise_memory("memory/one", {"run_id": "run-1", "scope": scope, "expected_version": 1})
        client.invalidate_memory("memory/one", {"expected_version": 2, "run_id": "run-1", "scope": scope})
        client.forget_memory("memory/one", {"expected_version": 3, "run_id": "run-1", "scope": scope})
        client.supersede_memory(
            "memory/one",
            {
                "winner_expected_version": 4,
                "run_id": "run-1",
                "scope": scope,
                "loser_memory_id": "memory/two",
                "loser_expected_version": 2,
                "confirm_supersede": True,
            },
        )
        client.prune_memories({"scope": scope, "run_id": "run-1", "confirm_prune": True})
        client.retrieve_memories({"scope": scope, "run_id": "run-1"})
        client.usage_observations("run/one", 20)
        client.recompute_budget_evidence({"run_id": "run-1", "confirm_recompute": True})
        client.generate_offline_replay({"replay": {}, "confirm_generation": True})
        client.replay_production_profile()
        client.configure_replay_production_profile(
            {"profile": {"enabled": False}, "confirm_profile": True}
        )
        client.promote_adaptive_policy_with_evidence({"replay_artifact_id": "replay-1", "confirm_promotion": True})

        requests = [call.args[0] for call in mock_urlopen.call_args_list]
        self.assertEqual(
            [(request.method, request.full_url) for request in requests],
            [
                ("POST", "http://localhost:8080/api/v1/memories"),
                ("GET", "http://localhost:8080/api/v1/memories/memory%2Fone?run_id=run-1"),
                ("POST", "http://localhost:8080/api/v1/memories/memory%2Fone/revise"),
                ("POST", "http://localhost:8080/api/v1/memories/memory%2Fone/invalidate"),
                ("POST", "http://localhost:8080/api/v1/memories/memory%2Fone/forget"),
                ("POST", "http://localhost:8080/api/v1/memories/memory%2Fone/supersede"),
                ("POST", "http://localhost:8080/api/v1/memories/prune"),
                ("POST", "http://localhost:8080/api/v1/memories/retrieve"),
                ("GET", "http://localhost:8080/api/v1/usage-observations?run_id=run%2Fone&limit=20"),
                ("POST", "http://localhost:8080/api/v1/budget-evidence/recompute"),
                ("POST", "http://localhost:8080/api/v1/offline-replays/generate"),
                ("GET", "http://localhost:8080/api/v1/offline-replays/production-profile"),
                ("PUT", "http://localhost:8080/api/v1/offline-replays/production-profile"),
                ("POST", "http://localhost:8080/api/v1/adaptive-fusion/policies/promote-with-evidence"),
            ],
        )
        self.assertEqual(json.loads(requests[3].data), {"expected_version": 2, "run_id": "run-1", "scope": scope})
        self.assertTrue(json.loads(requests[5].data)["confirm_supersede"])
        self.assertTrue(json.loads(requests[6].data)["confirm_prune"])
        self.assertTrue(json.loads(requests[9].data)["confirm_recompute"])
        self.assertTrue(json.loads(requests[10].data)["confirm_generation"])
        self.assertTrue(json.loads(requests[12].data)["confirm_profile"])
        self.assertTrue(json.loads(requests[13].data)["confirm_promotion"])

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_offline_replay_readers_send_filters_and_encode_artifact_ids(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "offline_replay_read.v1"})
        client = AgentControlPlaneClient("http://localhost:8080")

        client.offline_replay_artifacts(
            status="insufficient evidence", limit=25, offset=5
        )
        client.offline_replay_artifact("offline/replay one")

        urls = [call.args[0].full_url for call in mock_urlopen.call_args_list]
        self.assertEqual(
            urls,
            [
                "http://localhost:8080/api/v1/offline-replays?status=insufficient+evidence&limit=25&offset=5",
                "http://localhost:8080/api/v1/offline-replays/offline%2Freplay%20one",
            ],
        )

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_operator_decisions_sends_bounded_queue_query(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"read_only": True})
        client = AgentControlPlaneClient("http://localhost:8080")
        client.operator_decisions("2026-07-11T00:01:00Z", 300, 25, 5)
        self.assertEqual(
            mock_urlopen.call_args[0][0].full_url,
            "http://localhost:8080/api/v1/operator/decisions?generated_at=2026-07-11T00%3A01%3A00Z&maximum_freshness_seconds=300&limit=25&offset=5",
        )

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_apply_operator_decision_posts_hash_bound_explicit_action(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "operator_decision_action_result.v1"})
        client = AgentControlPlaneClient("http://localhost:8080")

        client.apply_operator_decision(
            "decision/one",
            queue_sha256="a" * 64,
            generated_at="2026-07-11T00:01:00Z",
            maximum_freshness_seconds=300,
            limit=25,
            offset=5,
            action="approve",
            confirm_action=True,
            reason="reviewed",
        )

        request = mock_urlopen.call_args[0][0]
        self.assertEqual(
            request.full_url,
            "http://localhost:8080/api/v1/operator/decisions/decision%2Fone/actions",
        )
        self.assertEqual(request.method, "POST")
        self.assertEqual(
            json.loads(request.data),
            {
                "queue_sha256": "a" * 64,
                "generated_at": "2026-07-11T00:01:00Z",
                "maximum_freshness_seconds": 300,
                "limit": 25,
                "offset": 5,
                "action": "approve",
                "confirm_action": True,
                "reason": "reviewed",
                "budget_policy": None,
            },
        )

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_dispatches_sends_pagination_and_search_query_params(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "axum_api.v1", "dispatches": []})
        client = AgentControlPlaneClient("http://localhost:8080")
        client.dispatches(limit=25, offset=50, search="alpha parser&owner=bad")

        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(
            req.full_url,
            "http://localhost:8080/api/v1/dispatches?limit=25&offset=50&search=alpha+parser%26owner%3Dbad",
        )

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_planner_methods_call_read_only_plan_endpoints(self, mock_urlopen):
        advisory = {
            "schema_version": "plan_advisory.v1",
            "mode": "recommendation_only",
            "status": "recommendation_ready",
            "blockers": [],
            "recommendations": [],
            "quality": {},
            "routing": {},
            "retry": {},
            "observability": {},
            "decision": {"execution_allowed": False},
        }
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "plan": {"plan_id": "plan-0001", "advisory": advisory},
            "plans": [{"plan_id": "plan-0001", "advisory": advisory}],
        })
        client = AgentControlPlaneClient("http://localhost:8080")

        created = client.create_plan("Plan docs", request_source="api")
        listed = client.plans(limit=25, offset=50, search="docs plan")
        client.plan("plan/0001")

        self.assertEqual(created["plan"]["advisory"]["schema_version"], "plan_advisory.v1")
        self.assertFalse(listed["plans"][0]["advisory"]["decision"]["execution_allowed"])

        calls = [call.args[0] for call in mock_urlopen.call_args_list]
        self.assertEqual(calls[0].method, "POST")
        self.assertEqual(calls[0].full_url, "http://localhost:8080/api/v1/plans")
        self.assertEqual(json.loads(calls[0].data), {"raw_request": "Plan docs", "request_source": "api"})
        self.assertEqual(calls[1].method, "GET")
        self.assertEqual(calls[1].full_url, "http://localhost:8080/api/v1/plans?limit=25&offset=50&search=docs+plan")
        self.assertEqual(calls[2].method, "GET")
        self.assertIn("/api/v1/plans/plan%2F0001", calls[2].full_url)

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_create_agent_step_plan_sends_typed_request(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "plan": {"plan_id": "plan-agent-0001"},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        client.create_plan(
            "Review the bounded mailbox",
            agent_steps=[{
                "agent_id": "agent-1",
                "role": "reviewer",
                "capability_profile": ["mailbox", "review"],
                "profile_id": "reviewer-profile",
                "model": "fixture-model",
            }],
            confirm_agent_runtime_plan=True,
        )
        request = mock_urlopen.call_args.args[0]
        self.assertEqual(json.loads(request.data), {
            "raw_request": "Review the bounded mailbox",
            "request_source": "api",
            "agent_steps": [{
                "agent_id": "agent-1",
                "role": "reviewer",
                "capability_profile": ["mailbox", "review"],
                "profile_id": "reviewer-profile",
                "model": "fixture-model",
            }],
            "confirm_agent_runtime_plan": True,
        })

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_workflow_run_methods_call_inert_runtime_state_endpoints(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "run": {"run_id": "run-0001"},
            "runs": [],
            "event": {"event_id": "workflow-event-0001"},
            "events": [],
            "approval": {"approval_id": "workflow-approval-0001"},
            "approvals": [],
        })
        client = AgentControlPlaneClient("http://localhost:8080")

        client.create_workflow_run("plan-0001")
        client.workflow_runs(limit=25, offset=50, search="run plan")
        client.workflow_run("run/0001")
        client.record_workflow_run_event(
            "run/0001",
            event_type="node_status_observed",
            node_id="node-a",
            details={"status": "ready"},
        )
        client.workflow_run_events("run/0001", limit=10)
        client.record_workflow_run_approval(
            "run/0001",
            node_id="node-a",
            decision="approved",
            reason="metadata only",
        )
        client.workflow_run_approvals("run/0001", limit=10)
        client.resume_workflow_run("run/0001", reason="metadata resume")
        client.cancel_workflow_run("run/0001", reason="metadata cancel")

        calls = [call.args[0] for call in mock_urlopen.call_args_list]
        self.assertEqual(calls[0].method, "POST")
        self.assertEqual(calls[0].full_url, "http://localhost:8080/api/v1/workflow-runs")
        self.assertEqual(json.loads(calls[0].data), {"plan_id": "plan-0001"})
        self.assertEqual(calls[1].method, "GET")
        self.assertEqual(calls[1].full_url, "http://localhost:8080/api/v1/workflow-runs?limit=25&offset=50&search=run+plan")
        self.assertEqual(calls[2].method, "GET")
        self.assertIn("/api/v1/workflow-runs/run%2F0001", calls[2].full_url)
        self.assertEqual(calls[3].method, "POST")
        self.assertIn("/api/v1/workflow-runs/run%2F0001/events", calls[3].full_url)
        self.assertEqual(json.loads(calls[3].data), {
            "node_id": "node-a",
            "event_type": "node_status_observed",
            "details": {"status": "ready"},
        })
        self.assertIn("/api/v1/workflow-runs/run%2F0001/events?limit=10", calls[4].full_url)
        self.assertEqual(calls[5].method, "POST")
        self.assertIn("/api/v1/workflow-runs/run%2F0001/approvals", calls[5].full_url)
        self.assertEqual(json.loads(calls[5].data), {
            "node_id": "node-a",
            "decision": "approved",
            "reason": "metadata only",
        })
        self.assertIn("/api/v1/workflow-runs/run%2F0001/approvals?limit=10", calls[6].full_url)
        self.assertIn("/api/v1/workflow-runs/run%2F0001/resume", calls[7].full_url)
        self.assertEqual(json.loads(calls[7].data), {"reason": "metadata resume"})
        self.assertIn("/api/v1/workflow-runs/run%2F0001/cancel", calls[8].full_url)
        self.assertEqual(json.loads(calls[8].data), {"reason": "metadata cancel"})

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_supervised_patch_methods_call_read_only_metadata_endpoints(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "metadata_only": True,
            "execution_authority": "disabled",
            "workspace": {"workspace_id": "patch-workspace-0001"},
            "workspaces": [],
            "artifact": {"artifact_id": "patch-artifact-0001"},
            "artifacts": [],
        })
        client = AgentControlPlaneClient("http://localhost:8080")

        workspaces = client.supervised_patch_workspaces(limit=25)
        workspace = client.supervised_patch_workspace_detail("workspace/0001")
        artifacts = client.supervised_patch_artifacts(limit=10)
        artifact = client.supervised_patch_artifact_detail("artifact/0001")

        self.assertTrue(workspaces["metadata_only"])
        self.assertEqual(workspace["execution_authority"], "disabled")
        self.assertTrue(artifacts["metadata_only"])
        self.assertEqual(artifact["execution_authority"], "disabled")
        calls = [call.args[0] for call in mock_urlopen.call_args_list]
        self.assertEqual(
            [call.full_url for call in calls],
            [
                "http://localhost:8080/api/v1/supervised-patch/workspaces?limit=25",
                "http://localhost:8080/api/v1/supervised-patch/workspaces/workspace%2F0001",
                "http://localhost:8080/api/v1/supervised-patch/artifacts?limit=10",
                "http://localhost:8080/api/v1/supervised-patch/artifacts/artifact%2F0001",
            ],
        )
        self.assertTrue(all(call.method == "GET" for call in calls))

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_audit_sends_pagination_query_params(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "axum_api.v1", "events": []})
        client = AgentControlPlaneClient("http://localhost:8080")
        client.audit(limit=25, offset=50, search="provider key", redact=True)

        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(
            req.full_url,
            "http://localhost:8080/api/v1/audit?limit=25&offset=50&search=provider+key&redact=true",
        )

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_metrics_sends_get(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "axum_api.v1", "dispatch_count": 0})
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.metrics()

        self.assertEqual(result["dispatch_count"], 0)
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/metrics")

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_dispatch_metrics_sends_limit(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "axum_api.v1", "metrics": []})
        client = AgentControlPlaneClient("http://localhost:8080")
        client.dispatch_metrics(limit=30)

        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/dispatch-metrics?limit=30")

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_feedback_readers_send_filters(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "axum_api.v1", "traces": [], "rows": []})
        client = AgentControlPlaneClient("http://localhost:8080")
        client.feedback_traces(
            limit=25,
            offset=50,
            task_class="docs cleanup",
            tier="standard",
            status="passed",
        )
        client.feedback_cost_of_pass(task_class="docs cleanup", tier="standard")

        urls = [call.args[0].full_url for call in mock_urlopen.call_args_list]
        self.assertEqual(
            urls,
            [
                "http://localhost:8080/api/v1/feedback/traces?limit=25&offset=50&task_class=docs+cleanup&tier=standard&status=passed",
                "http://localhost:8080/api/v1/feedback/cost-of-pass?task_class=docs+cleanup&tier=standard",
            ],
        )

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_simulation_report_sends_limit(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "axum_api.v1", "report": []})
        client = AgentControlPlaneClient("http://localhost:8080")
        client.simulation_report(limit=12)

        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/simulation/report?limit=12")

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_proposal_methods_call_controlled_loop_endpoints(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "proposal": {"proposal_id": "proposal-0001", "status": "pending"},
            "proposals": [],
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        client.proposals(limit=20, offset=40, status="pending")
        client.create_proposal(
            title="Tune docs routing",
            summary="Use standard tier for docs cleanup",
            task_class="docs cleanup",
            tier="standard",
            payload={"selected_tier": "standard"},
            evidence={"samples": 10},
        )
        client.proposal("proposal/0001")
        client.approve_proposal("proposal/0001", actor="human", reason="reviewed")
        client.reject_proposal("proposal/0001", reason="insufficient evidence")
        client.rollback_proposal("proposal/0001", reason="regression")
        client.deactivate_proposal("proposal/0001", reason="superseded")

        calls = [call.args[0] for call in mock_urlopen.call_args_list]
        self.assertEqual(calls[0].method, "GET")
        self.assertEqual(calls[0].full_url, "http://localhost:8080/api/v1/proposals?limit=20&offset=40&status=pending")
        self.assertEqual(calls[1].method, "POST")
        self.assertEqual(calls[1].full_url, "http://localhost:8080/api/v1/proposals")
        self.assertEqual(json.loads(calls[1].data), {
            "payload": {"selected_tier": "standard"},
            "title": "Tune docs routing",
            "summary": "Use standard tier for docs cleanup",
            "task_class": "docs cleanup",
            "tier": "standard",
            "evidence": {"samples": 10},
        })
        self.assertEqual(calls[2].method, "GET")
        self.assertIn("/api/v1/proposals/proposal%2F0001", calls[2].full_url)
        self.assertIn("/api/v1/proposals/proposal%2F0001/approve", calls[3].full_url)
        self.assertEqual(json.loads(calls[3].data), {"actor": "human", "reason": "reviewed", "confirm_policy_override": True})
        self.assertIn("/api/v1/proposals/proposal%2F0001/reject", calls[4].full_url)
        self.assertEqual(json.loads(calls[4].data), {"reason": "insufficient evidence"})
        self.assertIn("/api/v1/proposals/proposal%2F0001/rollback", calls[5].full_url)
        self.assertEqual(json.loads(calls[5].data), {"reason": "regression", "confirm_policy_override": True})
        self.assertIn("/api/v1/proposals/proposal%2F0001/deactivate", calls[6].full_url)
        self.assertEqual(json.loads(calls[6].data), {"reason": "superseded", "confirm_policy_override": True})

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_provider_audit_sends_pagination_query_params(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "axum_api.v1", "events": []})
        client = AgentControlPlaneClient("http://localhost:8080")
        client.provider_audit(limit=25, offset=50)

        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/provider/audit?limit=25&offset=50")

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_cost_details_sends_limit_query_param(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"schema_version": "local_dispatch_cost_detail.v1", "dispatches": []})
        client = AgentControlPlaneClient("http://localhost:8080")
        client.cost_details(limit=25)

        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/costs/dispatches?limit=25")

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_create_backup_posts_confirmation(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"backup": {"backup_id": "backup-0001"}})
        client = AgentControlPlaneClient("http://localhost:8080", api_key="test")
        result = client.create_backup(label="manual", confirm_local_backup=True)
        self.assertEqual(result["backup"]["backup_id"], "backup-0001")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/backups")
        self.assertEqual(req.get_header("Authorization"), " ".join(["Bearer", "test"]))
        self.assertEqual(
            json.loads(req.data),
            {"label": "manual", "confirm_local_backup": True},
        )


class ClientAuthTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_bearer_token_included(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"status": "ok"})
        local_key = "tok_abc"
        client = AgentControlPlaneClient("http://localhost:8080", api_key=local_key)
        client.health()
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.get_header("Authorization"), " ".join(["Bearer", "tok_abc"]))

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_no_auth_header_without_key(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"status": "ok"})
        client = AgentControlPlaneClient("http://localhost:8080")
        client.health()
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertIsNone(req.get_header("Authorization"))


class ClientErrorTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_http_error_with_json_body(self, mock_urlopen):
        mock_urlopen.side_effect = mock_http_error(401, {"error": "unauthorized"})
        client = AgentControlPlaneClient("http://localhost:8080")
        with self.assertRaises(AgentControlPlaneError) as ctx:
            client.health()
        self.assertIn("unauthorized", str(ctx.exception))

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_http_error_without_json(self, mock_urlopen):
        fp = BytesIO(b"not json")
        err = HTTPError("http://test", 500, "Internal Server Error", {}, fp)
        mock_urlopen.side_effect = err
        client = AgentControlPlaneClient("http://localhost:8080")
        with self.assertRaises(AgentControlPlaneError):
            client.health()


class ClientBaseUrlTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_trailing_slash_stripped(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"status": "ok"})
        client = AgentControlPlaneClient("http://localhost:8080///")
        client.health()
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/health")


class ClientDispatchDetailTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_dispatch_detail_sends_get(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"dispatch": {"dispatch_id": "d/1 needs review"}})
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.dispatch_detail("d/1 needs review")
        self.assertEqual(result["dispatch"]["dispatch_id"], "d/1 needs review")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertIn("/api/v1/dispatches/d%2F1%20needs%20review", req.full_url)


class ClientListBackupsTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_list_backups_sends_get(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"backups": []})
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.list_backups()
        self.assertEqual(result["backups"], [])
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/backups")


class ClientDeleteBackupTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_delete_backup_sends_delete(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"ok": True, "backup_id": "backup/0001"})
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.delete_backup("backup/0001")
        self.assertEqual(result["ok"], True)
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "DELETE")
        self.assertIn("/api/v1/backups/backup%2F0001", req.full_url)


class ClientVerifyBackupTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_verify_backup_sends_get(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"verification": {"backup_id": "backup/0001", "success": True}})
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.verify_backup("backup/0001")
        self.assertEqual(result["verification"]["success"], True)
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertIn("/api/v1/backups/backup%2F0001/verify", req.full_url)


class ClientRestoreBackupDryRunTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_restore_backup_dry_run_sends_post(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"restore_dry_run": {"backup_id": "backup/0001", "dry_run": True}})
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.restore_backup_dry_run("backup/0001")
        self.assertEqual(result["restore_dry_run"]["dry_run"], True)
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertIn("/api/v1/backups/backup%2F0001/restore/dry-run", req.full_url)
        self.assertEqual(json.loads(req.data), {"confirm_restore_dry_run": True})


class ClientStorageIntegrityTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_storage_integrity_sends_get(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"status": "ok"})
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.storage_integrity()
        self.assertEqual(result["status"], "ok")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/storage/integrity")


class ClientImportSnapshotTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_import_snapshot_sends_post(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"imported": 5})
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.import_snapshot({"config": {}, "team": []})
        self.assertEqual(result["imported"], 5)
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/import")
        self.assertEqual(
            json.loads(req.data),
            {"snapshot": {"config": {}, "team": []}, "confirm_import": True},
        )


class ClientRestoreBackupTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_restore_backup_sends_post(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"success": True})
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.restore_backup("backup/0001")
        self.assertEqual(result["success"], True)
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertIn("/api/v1/backups/backup%2F0001/restore", req.full_url)
        self.assertEqual(json.loads(req.data), {"confirm_restore": True})


class ClientEncodedPathTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_api_key_paths_encode_ids(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"ok": True})
        client = AgentControlPlaneClient("http://localhost:8080")

        client.revoke_api_key("key/1")
        client.rotate_api_key("key/1")
        client.delete_api_key("key/1")
        client.update_key_scopes("key/1", ["health:read"])

        urls = [call.args[0].full_url for call in mock_urlopen.call_args_list]
        self.assertEqual(
            urls,
            [
                "http://localhost:8080/api/v1/keys/key%2F1/revoke",
                "http://localhost:8080/api/v1/keys/key%2F1/rotate",
                "http://localhost:8080/api/v1/keys/key%2F1",
                "http://localhost:8080/api/v1/keys/key%2F1/scopes",
            ],
        )

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_team_paths_encode_ids(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({"ok": True})
        client = AgentControlPlaneClient("http://localhost:8080")

        client.update_member_role("user/1", "admin")
        client.delete_member("user/1")

        urls = [call.args[0].full_url for call in mock_urlopen.call_args_list]
        self.assertEqual(
            urls,
            [
                "http://localhost:8080/api/v1/team/user%2F1",
                "http://localhost:8080/api/v1/team/user%2F1",
            ],
        )


class ClientCreateSupervisedPatchWorkspaceTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_create_workspace_posts_request(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "workspace": {"workspace_id": "ws-0001", "status": "workspace_created"},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.create_supervised_patch_workspace(
            run_id="run-0001",
            target_id="target-a",
            target_repo_path="/tmp/repo",
            source_revision="abc123",
        )
        self.assertEqual(result["workspace"]["workspace_id"], "ws-0001")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/supervised-patch/workspaces")
        self.assertEqual(json.loads(req.data), {
            "run_id": "run-0001",
            "target_id": "target-a",
            "target_repo_path": "/tmp/repo",
            "source_revision": "abc123",
        })

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_create_workspace_includes_optional_fields(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "workspace": {"workspace_id": "ws-0002"},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        client.create_supervised_patch_workspace(
            run_id="run-0001",
            target_id="target-a",
            target_repo_path="/tmp/repo",
            source_revision="abc123",
            plan_id="plan-0001",
            source_tree_hash="sha256:deadbeef",
            workspace_mode="git_worktree",
        )
        args, _ = mock_urlopen.call_args
        req = args[0]
        body = json.loads(req.data)
        self.assertEqual(body["plan_id"], "plan-0001")
        self.assertEqual(body["source_tree_hash"], "sha256:deadbeef")
        self.assertEqual(body["workspace_mode"], "git_worktree")


class ClientCleanupSupervisedPatchWorkspaceTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_cleanup_workspace_posts_action(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "workspace": {"workspace_id": "ws-0001", "status": "cleaned"},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.cleanup_supervised_patch_workspace("ws/0001")
        self.assertEqual(result["workspace"]["status"], "cleaned")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertIn("/api/v1/supervised-patch/workspaces/ws%2F0001/cleanup", req.full_url)


class ClientQuarantineSupervisedPatchWorkspaceTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_quarantine_workspace_posts_action(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "workspace": {"workspace_id": "ws-0001", "status": "quarantined"},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.quarantine_supervised_patch_workspace("ws-0001")
        self.assertEqual(result["workspace"]["status"], "quarantined")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertIn("/api/v1/supervised-patch/workspaces/ws-0001/quarantine", req.full_url)


class ClientVerifySupervisedPatchWorkspaceTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_verify_workspace_posts_request(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "verification": {"status": "evidence_recorded", "command": ["cargo", "test"]},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.verify_supervised_patch_workspace(
            "ws/0001",
            command="cargo test",
            confirm_verification=True,
            timeout_ms=600000,
            repair_executor="codex_cli",
            max_repair_attempts=2,
        )
        self.assertEqual(result["verification"]["status"], "evidence_recorded")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertIn("/api/v1/supervised-patch/workspaces/ws%2F0001/verify", req.full_url)
        self.assertEqual(json.loads(req.data), {
            "command": "cargo test",
            "confirm_verification": True,
            "timeout_ms": 600000,
            "repair_executor": "codex_cli",
            "max_repair_attempts": 2,
        })


class ClientCaptureSupervisedPatchTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_capture_patch_posts_to_workspace(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "artifact": {"artifact_id": "art-0001", "artifact_type": "patch_diff"},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.capture_supervised_patch("ws-0001")
        self.assertEqual(result["artifact"]["artifact_id"], "art-0001")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertIn("/api/v1/supervised-patch/workspaces/ws-0001/capture", req.full_url)


class ClientExportSupervisedPatchArtifactTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_export_artifact_posts_with_run_id(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "export": {"artifact_id": "art-0001", "exported_by": "key-1"},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.export_supervised_patch_artifact("art/0001", run_id="run-0001")
        self.assertEqual(result["export"]["artifact_id"], "art-0001")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertIn("/api/v1/supervised-patch/artifacts/art%2F0001/export", req.full_url)
        self.assertEqual(json.loads(req.data), {"run_id": "run-0001"})


class ClientTargetRepoOutputTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_target_repo_output_posts_guarded_request(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "output": {
                "schema_version": "target_repo_output.v1",
                "branch_name": "acp/art-0001",
                "patch_hash": "sha256:abc",
            },
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.target_repo_output(
            "art/0001",
            run_id="run-0001",
            mode="push_branch",
            confirm_target_output=True,
            branch_name="acp/art-0001",
            remote="origin",
            commit_message="feat: apply artifact",
            pr_title="Apply artifact",
            create_pull_request=True,
        )
        self.assertEqual(result["output"]["patch_hash"], "sha256:abc")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertIn(
            "/api/v1/supervised-patch/artifacts/art%2F0001/output",
            req.full_url,
        )
        self.assertEqual(json.loads(req.data), {
            "run_id": "run-0001",
            "mode": "push_branch",
            "confirm_target_output": True,
            "branch_name": "acp/art-0001",
            "remote": "origin",
            "commit_message": "feat: apply artifact",
            "pr_title": "Apply artifact",
            "create_pull_request": True,
        })


class ClientTickWorkflowRunTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_tick_workflow_run_posts(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "tick": {"node_id": "node-a", "status": "completed"},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.tick_workflow_run("run/0001")
        self.assertEqual(result["tick"]["node_id"], "node-a")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertIn("/api/v1/workflow-runs/run%2F0001/tick", req.full_url)
        self.assertEqual(json.loads(req.data), {})

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_tick_workflow_run_passes_optional_params(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "tick": {"status": "completed"},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        client.tick_workflow_run(
            "run-0001",
            actor="user-1",
            max_retries=3,
            executor="command",
            timeout_ms=60000,
        )
        args, _ = mock_urlopen.call_args
        req = args[0]
        body = json.loads(req.data)
        self.assertEqual(body["actor"], "user-1")
        self.assertEqual(body["max_retries"], 3)
        self.assertEqual(body["executor"], "command")
        self.assertEqual(body["timeout_ms"], 60000)

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_tick_workflow_run_passes_command_override(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "tick": {"status": "completed"},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        client.tick_workflow_run(
            "run-0001",
            executor="command",
            command="echo hello",
            timeout_ms=5000,
        )
        args, _ = mock_urlopen.call_args
        req = args[0]
        body = json.loads(req.data)
        self.assertEqual(body["executor"], "command")
        self.assertEqual(body["command"], "echo hello")
        self.assertEqual(body["timeout_ms"], 5000)

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_scheduler_status_gets(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "scheduler": {"enabled": True, "running": True},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.scheduler_status()
        self.assertEqual(result["scheduler"]["enabled"], True)
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertIn("/api/v1/scheduler/status", req.full_url)

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_scheduler_control_posts_confirmed_action(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "scheduler": {"running": True, "paused": True},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.control_scheduler("pause", actor="operator")
        self.assertTrue(result["scheduler"]["paused"])
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "POST")
        self.assertIn("/api/v1/scheduler/control", req.full_url)
        self.assertEqual(
            json.loads(req.data),
            {
                "action": "pause",
                "actor": "operator",
                "confirm_control": True,
            },
        )


class ClientFetchExecutorPoolTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_fetch_executor_pool_sends_get(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "executor_pool.v1",
            "executors": [
                {
                    "executor_type": "mock",
                    "capabilities": {
                        "supported_task_types": ["generate", "review"],
                        "supported_task_domains": ["code", "docs"],
                        "requires_auth": False,
                        "requires_cli": False,
                        "max_timeout_ms": 30000,
                    },
                    "available": True,
                    "active_count": 0,
                    "concurrency_limit": 4,
                    "cooldown_until": None,
                    "failure_score": 0.0,
                    "cost_per_execution_usd": None,
                    "daily_cost_usd": 0.0,
                    "daily_cost_limit_usd": None,
                    "total_executions": 42,
                    "success_rate": 1.0,
                    "avg_latency_ms": 150,
                    "last_executed_at": "2026-06-07T00:00:00Z",
                },
            ],
            "total_active": 0,
            "total_capacity": 4,
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.fetch_executor_pool()
        self.assertEqual(result["schema_version"], "executor_pool.v1")
        self.assertEqual(len(result["executors"]), 1)
        self.assertEqual(result["executors"][0]["executor_type"], "mock")
        self.assertFalse(result["executors"][0]["capabilities"]["requires_auth"])
        self.assertTrue(result["executors"][0]["available"])
        self.assertEqual(result["total_active"], 0)
        self.assertEqual(result["total_capacity"], 4)
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/executor-pool")


class ClientDecisionsTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_decisions_sends_get_with_query_params(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "decisions": [{"decision_id": "d-0001", "action": "dispatch", "confidence": 0.95}],
            "total": 1,
            "limit": 25,
            "offset": 0,
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.decisions(limit=25, offset=10, search="parser", run_id="run-0001")
        self.assertEqual(result["decisions"][0]["decision_id"], "d-0001")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertIn("/api/v1/decisions?", req.full_url)
        self.assertIn("limit=25", req.full_url)
        self.assertIn("offset=10", req.full_url)
        self.assertIn("search=parser", req.full_url)
        self.assertIn("run_id=run-0001", req.full_url)

    @patch("agent_control_plane_sdk.client.urlopen")
    def test_decisions_encodes_special_chars_in_search(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "decisions": [],
            "total": 0,
            "limit": 100,
            "offset": 0,
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        client.decisions(search="foo&bar=baz")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertIn("search=foo%26bar%3Dbaz", req.full_url)


class ClientDecisionDetailTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_decision_detail_sends_get(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "decision": {"decision_id": "d/1 needs review", "action": "dispatch"},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.decision_detail("d/1 needs review")
        self.assertEqual(result["decision"]["decision_id"], "d/1 needs review")
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertIn("/api/v1/decisions/d%2F1%20needs%20review", req.full_url)


class ClientDecisionStatsTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_decision_stats_sends_get(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "stats": {"total_decisions": 42, "by_action": {"dispatch": 30, "defer": 12}, "avg_confidence": 0.87},
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        result = client.decision_stats()
        self.assertEqual(result["stats"]["total_decisions"], 42)
        self.assertEqual(result["stats"]["avg_confidence"], 0.87)
        args, _ = mock_urlopen.call_args
        req = args[0]
        self.assertEqual(req.method, "GET")
        self.assertEqual(req.full_url, "http://localhost:8080/api/v1/decisions/stats")


class ClientToolPolicyTest(unittest.TestCase):
    @patch("agent_control_plane_sdk.client.urlopen")
    def test_configure_allowlist_preserves_confirmation_and_current_hash(self, mock_urlopen):
        mock_urlopen.return_value = mock_response({
            "schema_version": "axum_api.v1",
            "resource": {
                "schema_version": "tool_policy_resource.v1",
                "resource_kind": "allowlist",
                "resource_id": "review/profile",
                "resource_sha256": "a" * 64,
                "changed": True,
                "value": {"profile_id": "review/profile", "tool_names": ["echo"]},
            },
        })
        client = AgentControlPlaneClient("http://localhost:8080")
        client.configure_tool_allowlist_policy(
            "review/profile",
            tool_names=["echo"],
            expected_current_sha256="b" * 64,
            confirm_tool_policy=True,
        )
        args, _ = mock_urlopen.call_args
        request = args[0]
        self.assertEqual(request.method, "PUT")
        self.assertIn("/tool-policy/profiles/review%2Fprofile/allowlist", request.full_url)
        self.assertEqual(json.loads(request.data), {
            "tool_names": ["echo"],
            "expected_current_sha256": "b" * 64,
            "confirm_tool_policy": True,
        })


if __name__ == "__main__":
    unittest.main()
