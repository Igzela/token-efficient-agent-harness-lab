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
        "workflow_id": "wf-1",
        "execution_attempt": 1,
        "scheduler_claim_id": "workflow:run-1:node-1:1",
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
            "PYTHONNOUSERSITE",
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
        self.assertEqual(result["scheduler_claim_id"], "workflow:run-1:node-1:1")
        self.assertEqual(result["execution_attempt"], 1)
        self.assertEqual(result["workflow_id"], "wf-1")
        self.assertIsNone(result["patch"])
        self.assertIsNone(result["patch_sha256"])
        self.assertEqual(result["changed_paths"], [])
        self.assertEqual(result["reason_code"], "fixture_analysis_ok")
        self.assertEqual(result["analysis"]["findings_count"], 1)
        self.assertEqual(result["analysis"]["scope_paths"], ["docs/fixture.md"])

    def test_patch_fixture_ok(self):
        result, code = handle_request(
            _base_request(task_kind="allowed_path_patch", allowed_paths=["docs/out.md"])
        )
        self.assertEqual(code, 0)
        self.assertEqual(result["changed_paths"], ["docs/out.md"])

    def test_rejects_missing_adapter_version(self):
        req = _base_request()
        del req["adapter_version"]
        result, code = handle_request(req)
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "request_field_missing")

    def test_rejects_missing_expected_adapter_version(self):
        req = _base_request()
        del req["expected_adapter_version"]
        result, code = handle_request(req)
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "request_field_missing")

    def test_rejects_missing_contract_version(self):
        req = _base_request()
        del req["adapter_contract_version"]
        result, code = handle_request(req)
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "request_field_missing")

    def test_rejects_missing_opencode_version(self):
        req = _base_request()
        del req["expected_opencode_version"]
        result, code = handle_request(req)
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "request_field_missing")

    def test_rejects_env_subset(self):
        result, code = handle_request(_base_request(environment_allowlist=["PATH", "HOME"]))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "environment_allowlist_mismatch")

    def test_rejects_duplicate_env(self):
        env = list(_base_request()["environment_allowlist"]) + ["PATH"]
        result, code = handle_request(_base_request(environment_allowlist=env))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "environment_allowlist_invalid")

    def test_rejects_extra_env(self):
        env = list(_base_request()["environment_allowlist"]) + ["FOO"]
        result, code = handle_request(_base_request(environment_allowlist=env))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "environment_allowlist_mismatch")

    def test_rejects_extra_permission_key(self):
        profile = _profile()
        profile["extra_flag"] = False
        result, code = handle_request(
            _base_request(
                permission_profile=profile,
                permission_profile_hash=_profile_hash(profile),
            )
        )
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "permission_profile_keys_invalid")

    def test_rejects_unknown_request_field(self):
        result, code = handle_request(_base_request(merge_authority=True))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "request_unknown_field")

    def test_rejects_fixture_base_placeholder(self):
        result, code = handle_request(_base_request(base_commit="fixture-base"))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "base_commit_invalid")

    def test_rejects_path_traversal(self):
        result, code = handle_request(_base_request(allowed_paths=["../etc/passwd"]))
        self.assertNotEqual(code, 0)
        self.assertEqual(result["code"], "allowed_paths_invalid")


if __name__ == "__main__":
    unittest.main()
