"""Tests for pure Harness app API handlers."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from harness_core.app_api import handle_api_request


VALID_AGENTS = """# Agent Execution Policy

Claude Code operates as an execution adapter under the Token-Efficient Agent Harness.
It is not the governance authority.
All governance decisions require human authorisation.
Claude Code must pause before pushing directly to `main` or `master`.
Claude Code must not connect real LLM provider systems without approval.
Claude Code must pause before modifying active YAML or active user/project state.
"""


def write_instance(root: Path) -> None:
    harness = root / "docs" / "harness"
    harness.mkdir(parents=True)
    (root / "AGENTS.md").write_text(VALID_AGENTS, encoding="utf-8")
    (harness / "PROJECT_BOARD.md").write_text(
        "# Project Board\n\n## Task States\n\ntodo ready running done\n\nPhase 1 Final Gate: PASS\n",
        encoding="utf-8",
    )
    (harness / "TASK_QUEUE.md").write_text(
        "### P1-001\n\n**Status**: done\n**Goal**: test\n",
        encoding="utf-8",
    )
    (harness / "QUALITY_GATES.md").write_text(
        "unknown_error requires human review\nprovider boundary\nactive state mutation requires approval\nauto modification forbidden\nread-only boundary\n",
        encoding="utf-8",
    )
    (harness / "DECISION_RECORD.md").write_text("# Decision Record\n", encoding="utf-8")
    (harness / "RISK_REGISTER.md").write_text(
        "active risk\nmitigated risk\nprovider/LLM premature integration\nscope drift\nmutation active state\n",
        encoding="utf-8",
    )


class AppApiTests(unittest.TestCase):
    def test_health(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = handle_api_request("GET", "/api/health", None, Path(tmp) / "registry.json")

        self.assertEqual(200, response.status_code)
        self.assertEqual("ok", response.body_json["status"])

    def test_register_local_repo_and_audit_by_id(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            target = root / "target"
            target.mkdir()
            write_instance(target)
            registry_path = root / "registry.json"

            add_response = handle_api_request(
                "POST",
                "/api/repos",
                json.dumps(
                    {
                        "id": "target",
                        "name": "Target",
                        "kind": "local",
                        "path": str(target),
                    }
                ),
                registry_path,
            )
            audit_response = handle_api_request("GET", "/api/audit?repo_id=target", None, registry_path)

        self.assertEqual(201, add_response.status_code)
        self.assertEqual(200, audit_response.status_code)
        self.assertIn(audit_response.body_json["audit"]["verdict"], {"PASS", "PASS_WITH_NOTES", "BLOCKED"})
        self.assertEqual(str(target.resolve()), audit_response.body_json["repo"]["path"])

    def test_remote_repo_audit_is_unsupported(self):
        with tempfile.TemporaryDirectory() as tmp:
            registry_path = Path(tmp) / "registry.json"
            add_response = handle_api_request(
                "POST",
                "/api/repos",
                json.dumps(
                    {
                        "id": "remote",
                        "name": "Remote",
                        "kind": "remote",
                        "url": "https://github.com/example/repo.git",
                    }
                ),
                registry_path,
            )
            audit_response = handle_api_request("GET", "/api/audit?repo_id=remote", None, registry_path)

        self.assertEqual(201, add_response.status_code)
        self.assertEqual(409, audit_response.status_code)
        self.assertEqual("remote_audit_unsupported", audit_response.body_json["error"]["code"])

    def test_audit_requires_repo_id_not_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = handle_api_request("GET", "/api/audit?path=/tmp", None, Path(tmp) / "registry.json")

        self.assertEqual(400, response.status_code)
        self.assertEqual("missing_repo_id", response.body_json["error"]["code"])

    def test_errors_are_structured_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            response = handle_api_request("POST", "/api/repos", "{not-json", Path(tmp) / "registry.json")

        self.assertEqual(400, response.status_code)
        self.assertEqual("invalid_json", response.body_json["error"]["code"])
        self.assertNotIn("Traceback", json.dumps(response.body_json))


if __name__ == "__main__":
    unittest.main()
