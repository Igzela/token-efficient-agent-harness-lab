"""Tests for the CA-7 security baseline checker.

Uses tempfile directories to isolate each test from the real repository.
"""

from __future__ import annotations

import json
import os
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

# Add tools/ to path so we can import the checker
TOOLS_DIR = Path(__file__).resolve().parent.parent / "tools"
sys.path.insert(0, str(TOOLS_DIR))

import check_security_baseline as csb


class TestSecretScan(unittest.TestCase):
    """Tests for the secret scan check."""

    def test_clean_file_no_secrets(self):
        """A file with no credential patterns should pass."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "main.py").write_text('x = 1\nprint("hello")\n')
            findings = csb.check_secret_scan(repo, ["main.py"])
            self.assertEqual(findings, [])

    def test_detects_api_key(self):
        """A file with api_key = 'real-key' should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "config.py").write_text('api_key = "sk-abc123def456"\n')
            findings = csb.check_secret_scan(repo, ["config.py"])
            self.assertEqual(len(findings), 1)
            self.assertIn("api_key", findings[0])

    def test_detects_secret_token(self):
        """A file with secret = 'value' should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "auth.py").write_text('secret = "my-super-secret-value"\n')
            findings = csb.check_secret_scan(repo, ["auth.py"])
            self.assertEqual(len(findings), 1)

    def test_allows_placeholder_values(self):
        """Placeholder strings should not be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "config.py").write_text(
                'api_key = "your-api-key-here"\n'
                'secret = "changeme"\n'
            )
            findings = csb.check_secret_scan(repo, ["config.py"])
            self.assertEqual(findings, [])

    def test_skips_comments(self):
        """Lines starting with # should be skipped."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "config.py").write_text(
                '# api_key = "sk-real-key"\n'
                'x = 1\n'
            )
            findings = csb.check_secret_scan(repo, ["config.py"])
            self.assertEqual(findings, [])

    def test_detects_bearer_token(self):
        """Bearer tokens should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "headers.py").write_text(
                'Authorization = "Bearer sk-abc123"\n'
            )
            findings = csb.check_secret_scan(repo, ["headers.py"])
            self.assertEqual(len(findings), 1)
            self.assertIn("Bearer", findings[0])


class TestImportScan(unittest.TestCase):
    """Tests for the AST import scan check."""

    def test_clean_file_no_prohibited_imports(self):
        """Standard library imports should pass."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "utils.py").write_text(
                "import os\nimport json\nfrom pathlib import Path\n"
            )
            findings = csb.check_import_scan(repo, ["utils.py"])
            self.assertEqual(findings, [])

    def test_detects_requests_import(self):
        """import requests should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "http.py").write_text("import requests\n")
            findings = csb.check_import_scan(repo, ["http.py"])
            self.assertEqual(len(findings), 1)
            self.assertIn("requests", findings[0])

    def test_detects_openai_import(self):
        """import openai should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "llm.py").write_text("import openai\n")
            findings = csb.check_import_scan(repo, ["llm.py"])
            self.assertEqual(len(findings), 1)
            self.assertIn("openai", findings[0])

    def test_detects_anthropic_from_import(self):
        """from anthropic import Client should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "llm.py").write_text("from anthropic import Client\n")
            findings = csb.check_import_scan(repo, ["llm.py"])
            self.assertEqual(len(findings), 1)
            self.assertIn("anthropic", findings[0])

    def test_detects_urllib_submodule(self):
        """import urllib.request should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "fetch.py").write_text("import urllib.request\n")
            findings = csb.check_import_scan(repo, ["fetch.py"])
            self.assertEqual(len(findings), 1)
            self.assertIn("urllib", findings[0])

    def test_detects_socket_import(self):
        """import socket should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "net.py").write_text("import socket\n")
            findings = csb.check_import_scan(repo, ["net.py"])
            self.assertEqual(len(findings), 1)
            self.assertIn("socket", findings[0])


class TestActiveRoutingGuard(unittest.TestCase):
    """Tests for the active routing guard check."""

    def test_clean_json_no_active_routing(self):
        """JSON without active_routing_allowed should pass."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            data = {"routing": {"shadow": True}}
            (repo / "config.json").write_text(json.dumps(data))
            findings = csb.check_active_routing(repo, ["config.json"])
            self.assertEqual(findings, [])

    def test_detects_active_routing(self):
        """JSON with active_routing_allowed: true should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            data = {"routing": {"active_routing_allowed": True}}
            (repo / "config.json").write_text(json.dumps(data))
            findings = csb.check_active_routing(repo, ["config.json"])
            self.assertEqual(len(findings), 1)
            self.assertIn("active_routing_allowed", findings[0])

    def test_active_routing_false_is_ok(self):
        """active_routing_allowed: false should not be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            data = {"routing": {"active_routing_allowed": False}}
            (repo / "config.json").write_text(json.dumps(data))
            findings = csb.check_active_routing(repo, ["config.json"])
            self.assertEqual(findings, [])

    def test_nested_active_routing(self):
        """Deeply nested active_routing_allowed: true should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            data = {"a": {"b": {"c": {"active_routing_allowed": True}}}}
            (repo / "config.json").write_text(json.dumps(data))
            findings = csb.check_active_routing(repo, ["config.json"])
            self.assertEqual(len(findings), 1)


class TestGovernanceBoundaryGuard(unittest.TestCase):
    """Tests for the governance boundary guard check."""

    def test_valid_governance_fixtures(self):
        """Well-formed governance fixtures should pass."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            gov_dir = repo / "tests" / "fixtures" / "governance"
            gov_dir.mkdir(parents=True)

            fixture = {
                "schema_version": "governance_decision.v1",
                "decision_id": "gov-test",
                "candidate_id": "cand-test",
                "policy_id": "pol-test",
                "decision": "approve_activation",
                "gate_results": {
                    "evidence_gate": "pass",
                    "approval_gate": "pass",
                    "rollback_gate": "pass",
                    "scope_gate": "pass",
                    "unknown_error_gate": "pass",
                },
            }
            for name in [
                "valid_all_gates_pass.json",
                "gate_scope_fail.json",
                "gate_approval_fail.json",
                "gate_rollback_fail.json",
                "gate_unknown_error_fail.json",
            ]:
                (gov_dir / name).write_text(json.dumps(fixture))

            findings = csb.check_governance_boundary(repo)
            self.assertEqual(findings, [])

    def test_missing_governance_dir(self):
        """Missing governance directory should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "tests").mkdir(parents=True)
            findings = csb.check_governance_boundary(repo)
            self.assertEqual(len(findings), 1)
            self.assertIn("not found", findings[0])

    def test_missing_required_fixture(self):
        """Missing a required fixture file should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            gov_dir = repo / "tests" / "fixtures" / "governance"
            gov_dir.mkdir(parents=True)
            # Only create some fixtures, not all
            fixture = {
                "schema_version": "governance_decision.v1",
                "gate_results": {
                    "evidence_gate": "pass",
                    "approval_gate": "pass",
                    "rollback_gate": "pass",
                    "scope_gate": "pass",
                    "unknown_error_gate": "pass",
                },
            }
            (gov_dir / "valid_all_gates_pass.json").write_text(json.dumps(fixture))
            findings = csb.check_governance_boundary(repo)
            self.assertTrue(len(findings) >= 1)
            self.assertTrue(any("Missing" in f for f in findings))

    def test_invalid_json_fixture(self):
        """Invalid JSON in a fixture should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            gov_dir = repo / "tests" / "fixtures" / "governance"
            gov_dir.mkdir(parents=True)
            (gov_dir / "valid_all_gates_pass.json").write_text("NOT JSON")
            # Create the other required fixtures
            fixture = {
                "schema_version": "governance_decision.v1",
                "gate_results": {
                    "evidence_gate": "pass",
                    "approval_gate": "pass",
                    "rollback_gate": "pass",
                    "scope_gate": "pass",
                    "unknown_error_gate": "pass",
                },
            }
            for name in [
                "gate_scope_fail.json",
                "gate_approval_fail.json",
                "gate_rollback_fail.json",
                "gate_unknown_error_fail.json",
            ]:
                (gov_dir / name).write_text(json.dumps(fixture))
            findings = csb.check_governance_boundary(repo)
            self.assertTrue(any("Invalid JSON" in f for f in findings))

    def test_missing_gate_results(self):
        """Fixture missing gate_results should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            gov_dir = repo / "tests" / "fixtures" / "governance"
            gov_dir.mkdir(parents=True)
            fixture = {"schema_version": "governance_decision.v1"}
            (gov_dir / "valid_all_gates_pass.json").write_text(json.dumps(fixture))
            for name in [
                "gate_scope_fail.json",
                "gate_approval_fail.json",
                "gate_rollback_fail.json",
                "gate_unknown_error_fail.json",
            ]:
                full = {
                    "schema_version": "governance_decision.v1",
                    "gate_results": {
                        "evidence_gate": "pass",
                        "approval_gate": "pass",
                        "rollback_gate": "pass",
                        "scope_gate": "pass",
                        "unknown_error_gate": "pass",
                    },
                }
                (gov_dir / name).write_text(json.dumps(full))
            findings = csb.check_governance_boundary(repo)
            self.assertTrue(any("missing gate_results" in f for f in findings))


class TestStage0EventGuard(unittest.TestCase):
    """Tests for the stage-0 event guard check."""

    def test_valid_events_jsonl(self):
        """A valid events.jsonl should pass."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            events_dir = repo / "docs" / "stage0"
            events_dir.mkdir(parents=True)
            events = [
                json.dumps({"event": "task_started", "task_id": "t-001"}),
                json.dumps({"event": "task_completed", "task_id": "t-001"}),
            ]
            (events_dir / "events.jsonl").write_text("\n".join(events) + "\n")
            findings = csb.check_stage0_event_guard(repo)
            self.assertEqual(findings, [])

    def test_missing_events_file(self):
        """Missing events.jsonl should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "docs" / "stage0").mkdir(parents=True)
            findings = csb.check_stage0_event_guard(repo)
            self.assertEqual(len(findings), 1)
            self.assertIn("not found", findings[0])

    def test_empty_events_file(self):
        """Empty events.jsonl should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            events_dir = repo / "docs" / "stage0"
            events_dir.mkdir(parents=True)
            (events_dir / "events.jsonl").write_text("")
            findings = csb.check_stage0_event_guard(repo)
            self.assertEqual(len(findings), 1)
            self.assertIn("empty", findings[0])

    def test_no_valid_json_lines(self):
        """events.jsonl with no valid JSON should be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            events_dir = repo / "docs" / "stage0"
            events_dir.mkdir(parents=True)
            (events_dir / "events.jsonl").write_text("not json\nalso not json\n")
            findings = csb.check_stage0_event_guard(repo)
            self.assertEqual(len(findings), 1)
            self.assertIn("no valid JSON", findings[0])

    def test_blank_lines_only(self):
        """events.jsonl with only blank lines should be flagged as empty."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            events_dir = repo / "docs" / "stage0"
            events_dir.mkdir(parents=True)
            (events_dir / "events.jsonl").write_text("\n\n\n")
            findings = csb.check_stage0_event_guard(repo)
            self.assertEqual(len(findings), 1)
            self.assertIn("empty", findings[0])


if __name__ == "__main__":
    unittest.main()
