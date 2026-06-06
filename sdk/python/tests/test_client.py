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
        )
        args, _ = mock_urlopen.call_args
        req = args[0]
        body = json.loads(req.data)
        self.assertEqual(body["plan_id"], "plan-0001")
        self.assertEqual(body["source_tree_hash"], "sha256:deadbeef")


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


if __name__ == "__main__":
    unittest.main()
