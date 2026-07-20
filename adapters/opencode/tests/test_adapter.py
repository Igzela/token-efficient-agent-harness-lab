from __future__ import annotations

import hashlib
import json
import unittest

from acp_opencode_adapter.adapter import handle_request


def _base_request(**overrides):
    req = {
        "schema_version": "opencode_external_request.v1",
        "invocation_id": "inv-1",
        "run_id": "run-1",
        "node_id": "node-1",
        "lease_id": "lease-1",
        "runtime_kind": "opencode",
        "mode": "fixture",
        "task_kind": "analysis",
        "task_input_hash": "a" * 64,
        "base_commit": "b" * 40,
        "worktree_id": "wt-1",
        "allowed_paths": ["docs/fixture.md"],
        "environment_allowlist": ["PATH", "HOME", "LANG"],
        "permission_profile": {
            "approval_mode": "deny_by_default",
            "network_enabled": False,
            "mcp_enabled": False,
            "websearch": False,
            "webfetch": False,
            "remote_agents": False,
            "background_agents": False,
            "provider_fallback": False,
        },
        "requested_capabilities": [],
    }
    req.update(overrides)
    return req


class TestOpenCodeFixtureAdapter(unittest.TestCase):
    def test_analysis_fixture_ok(self):
        result, code = handle_request(_base_request())
        self.assertEqual(code, 0)
        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["changed_paths"], [])
        self.assertEqual(result["tool_summary"]["network_attempts"], 0)

    def test_patch_fixture_ok(self):
        result, code = handle_request(
            _base_request(task_kind="allowed_path_patch", allowed_paths=["docs/out.md"])
        )
        self.assertEqual(code, 0)
        self.assertEqual(result["changed_paths"], ["docs/out.md"])
        self.assertTrue(result["patch"].startswith("*** Begin Patch"))
        expected = hashlib.sha256(result["patch"].encode()).hexdigest()
        self.assertEqual(result["patch_sha256"], expected)

    def test_rejects_path_traversal(self):
        result, code = handle_request(
            _base_request(allowed_paths=["../etc/passwd"])
        )
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "allowed_paths_invalid")

    def test_rejects_network_capability(self):
        result, code = handle_request(
            _base_request(requested_capabilities=["websearch"])
        )
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "capability_forbidden")

    def test_rejects_live_mode(self):
        result, code = handle_request(_base_request(mode="live"))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "mode_forbidden")

    def test_rejects_mcp_enabled_profile(self):
        profile = _base_request()["permission_profile"]
        profile = dict(profile)
        profile["mcp_enabled"] = True
        result, code = handle_request(_base_request(permission_profile=profile))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "mcp_forbidden")


if __name__ == "__main__":
    unittest.main()
