from __future__ import annotations

import hashlib
import json
import unittest

from acp_opencode_adapter.adapter import ADAPTER_VERSION, handle_request


def _profile():
    return {
        "approval_mode": "deny_by_default",
        "network_enabled": False,
        "mcp_enabled": False,
        "websearch": False,
        "webfetch": False,
        "remote_agents": False,
        "background_agents": False,
        "provider_fallback": False,
    }


def _profile_hash(profile=None):
    profile = profile or _profile()
    body = json.dumps(profile, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(body.encode("utf-8")).hexdigest()


def _base_request(**overrides):
    profile = _profile()
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
        "environment_allowlist": [
            "PATH",
            "HOME",
            "LANG",
            "LC_ALL",
            "TMPDIR",
            "PYTHONIOENCODING",
            "PYTHONDONTWRITEBYTECODE",
            "PYTHONPATH",
        ],
        "permission_profile": profile,
        "permission_profile_hash": _profile_hash(profile),
        "requested_capabilities": [],
        "adapter_version": ADAPTER_VERSION,
        "adapter_contract_version": "opencode_external_adapter.v1",
        "expected_opencode_version": "1.1.48",
        "expected_adapter_version": ADAPTER_VERSION,
    }
    req.update(overrides)
    return req


class TestOpenCodeFixtureAdapter(unittest.TestCase):
    def test_analysis_fixture_ok(self):
        result, code = handle_request(_base_request())
        self.assertEqual(code, 0)
        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["changed_paths"], [])
        self.assertEqual(result["task_input_hash"], "a" * 64)
        self.assertEqual(result["base_commit"], "b" * 40)
        self.assertEqual(result["worktree_id"], "wt-1")
        self.assertEqual(result["tool_summary"]["network_attempts"], 0)
        self.assertEqual(result["tool_summary"]["provider_attempts"], 0)
        self.assertEqual(result["tool_summary"]["mcp_attempts"], 0)
        self.assertEqual(result["tool_summary"]["process_attempts"], 0)

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
        result, code = handle_request(_base_request(allowed_paths=["../etc/passwd"]))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "allowed_paths_invalid")

    def test_rejects_network_capability(self):
        result, code = handle_request(_base_request(requested_capabilities=["websearch"]))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "capability_forbidden")

    def test_rejects_live_mode(self):
        result, code = handle_request(_base_request(mode="live"))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "mode_forbidden")

    def test_rejects_mcp_enabled_profile(self):
        profile = dict(_profile())
        profile["mcp_enabled"] = True
        result, code = handle_request(
            _base_request(
                permission_profile=profile,
                permission_profile_hash=_profile_hash(profile),
            )
        )
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "mcp_forbidden")

    def test_rejects_permission_profile_hash_mismatch(self):
        result, code = handle_request(
            _base_request(permission_profile_hash="0" * 64)
        )
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "permission_profile_hash_mismatch")

    def test_rejects_fixture_base_placeholder(self):
        result, code = handle_request(_base_request(base_commit="fixture-base"))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "base_commit_invalid")

    def test_rejects_fixture_worktree_placeholder(self):
        result, code = handle_request(_base_request(worktree_id="fixture-worktree"))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "worktree_invalid")

    def test_rejects_bad_task_input_hash(self):
        result, code = handle_request(_base_request(task_input_hash="not-a-hash"))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "task_input_hash_invalid")

    def test_rejects_wrong_adapter_version(self):
        result, code = handle_request(
            _base_request(expected_adapter_version="9.9.9", adapter_version="9.9.9")
        )
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "adapter_version_mismatch")

    def test_rejects_forbidden_env(self):
        result, code = handle_request(
            _base_request(environment_allowlist=["PATH", "OPENAI_API_KEY"])
        )
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "environment_forbidden")

    def test_rejects_extra_authority_field(self):
        result, code = handle_request(_base_request(merge_authority=True))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "request_extra_authority")

    def test_rejects_missing_requested_capabilities(self):
        req = _base_request()
        del req["requested_capabilities"]
        result, code = handle_request(req)
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "requested_capabilities_missing")


if __name__ == "__main__":
    unittest.main()
