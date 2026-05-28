from __future__ import annotations

import json
import unittest
from unittest.mock import patch

from agent_control_plane_sdk import AgentControlPlaneClient, DispatchRequest


class FakeResponse:
    def __init__(self, body: dict):
        self._body = json.dumps(body).encode("utf-8")

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return None

    def read(self):
        return self._body


class ClientTests(unittest.TestCase):
    def test_dispatch_posts_rest_request(self) -> None:
        seen = {}

        def fake_urlopen(request, timeout):
            seen["url"] = request.full_url
            seen["body"] = json.loads(request.data.decode("utf-8"))
            seen["timeout"] = timeout
            return FakeResponse({"record": {"dispatch_id": "disp-1"}})

        with patch("agent_control_plane_sdk.client.urlopen", fake_urlopen):
            client = AgentControlPlaneClient("http://localhost:8080", timeout=2.5)
            result = client.dispatch("Summarize docs")

        self.assertEqual(seen["url"], "http://localhost:8080/api/v1/dispatch")
        self.assertEqual(seen["body"]["raw_request"], "Summarize docs")
        self.assertEqual(seen["body"]["request_source"], "api")
        self.assertEqual(seen["timeout"], 2.5)
        self.assertEqual(result["record"]["dispatch_id"], "disp-1")

    def test_dispatch_request_wire_shape(self) -> None:
        request = DispatchRequest("Review config", "cli")
        self.assertEqual(request.to_json()["schema_version"], "dispatch_request.v1")
        self.assertEqual(request.to_json()["request_source"], "cli")


if __name__ == "__main__":
    unittest.main()
