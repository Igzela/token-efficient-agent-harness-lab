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


if __name__ == "__main__":
    unittest.main()
