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

# Import checker from same directory
TOOLS_DIR = Path(__file__).resolve().parent
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
                'Authorization = "Bearer sk-abc123def456"\n'
            )
            findings = csb.check_secret_scan(repo, ["headers.py"])
            self.assertEqual(len(findings), 1)
            self.assertIn("Bearer", findings[0])

    def test_allows_bearer_help_text(self):
        """CLI help text mentioning Bearer token is not a credential."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "cli.py").write_text(
                'parser.add_argument("--token", help="Bearer token; never printed.")\n'
            )
            findings = csb.check_secret_scan(repo, ["cli.py"])
            self.assertEqual(findings, [])

    def test_allows_indirect_provider_secret_env_lookup(self):
        """Shell indirection through a named environment variable is not a literal secret."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "start.sh").write_text('provider_secret="${!ACP_API_KEY:-}"\n')
            findings = csb.check_secret_scan(repo, ["start.sh"])
            self.assertEqual(findings, [])


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


class TestDormantAutomationGuard(unittest.TestCase):
    """Tests for the dormant automation guard check."""

    def test_clean_automation_file_passes(self):
        """A clean automation script should pass."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "scripts").mkdir(parents=True)
            (repo / "scripts" / "run_checks.sh").write_text(
                "#!/usr/bin/env bash\nset -euo pipefail\ncargo test -p engine\n"
            )
            findings = csb.check_dormant_automation_guard(
                repo, ["scripts/run_checks.sh"]
            )
            self.assertEqual(findings, [])

    def test_detects_dangerously_skip_permissions(self):
        """claude --dangerously-skip-permissions must be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "scripts").mkdir(parents=True)
            (repo / "scripts" / "loop.sh").write_text(
                'claude --dangerously-skip-permissions <<< "$PROMPT"\n'
            )
            findings = csb.check_dormant_automation_guard(
                repo, ["scripts/loop.sh"]
            )
            self.assertEqual(len(findings), 1)
            self.assertIn("dangerously-skip-permissions", findings[0])

    def test_detects_gh_run_list_limit_1(self):
        """gh run list --limit 1 (unbound CI judgment) must be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "scripts").mkdir(parents=True)
            (repo / "scripts" / "wait.sh").write_text(
                "status=$(gh run list --limit 1 --json status,conclusion \\\n"
                "  --jq '.[0] | \"\\(.status) \\(.conclusion)\"')\n"
            )
            findings = csb.check_dormant_automation_guard(
                repo, ["scripts/wait.sh"]
            )
            self.assertEqual(len(findings), 1)
            self.assertIn("gh run list --limit 1", findings[0])

    def test_detects_gh_run_watch_chained_to_unbound_list(self):
        """gh run watch $(gh run list --limit 1 ...) must be flagged twice."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / ".github" / "workflows").mkdir(parents=True)
            (repo / ".github" / "workflows" / "agent.yml").write_text(
                "run: gh run watch $(gh run list --limit 1 --json databaseId "
                "-q '.[0].databaseId') --exit-status\n"
            )
            findings = csb.check_dormant_automation_guard(
                repo, [".github/workflows/agent.yml"]
            )
            self.assertEqual(len(findings), 2)
            self.assertTrue(
                any("gh run watch" in f for f in findings)
            )
            self.assertTrue(any("gh run list --limit 1" in f for f in findings))

    def test_explicit_run_id_watch_passes(self):
        """gh run watch with an explicit run id must not be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "scripts").mkdir(parents=True)
            (repo / "scripts" / "wait.sh").write_text(
                "gh run watch 1234567890 --exit-status\n"
            )
            findings = csb.check_dormant_automation_guard(
                repo, ["scripts/wait.sh"]
            )
            self.assertEqual(findings, [])

    def test_commit_bound_list_passes(self):
        """gh run list bound to a head sha or branch is legitimate."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "scripts").mkdir(parents=True)
            (repo / "scripts" / "wait.sh").write_text(
                "gh run list --branch main --head "
                "ac50c3860ad1dccd5ef72a166cd609688c253a98 --json databaseId\n"
            )
            findings = csb.check_dormant_automation_guard(
                repo, ["scripts/wait.sh"]
            )
            self.assertEqual(findings, [])

    def test_informational_bounded_list_passes(self):
        """gh run list --limit >= 2 is an informational query, not a finding."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "scripts").mkdir(parents=True)
            (repo / "scripts" / "status.sh").write_text(
                "gh run list --limit 3 --json status,conclusion\n"
            )
            findings = csb.check_dormant_automation_guard(
                repo, ["scripts/status.sh"]
            )
            self.assertEqual(findings, [])

    def test_bare_watch_with_exit_status_flagged(self):
        """gh run watch --exit-status with no run id is unbound."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "scripts").mkdir(parents=True)
            (repo / "scripts" / "wait.sh").write_text(
                "gh run watch --exit-status\n"
            )
            findings = csb.check_dormant_automation_guard(
                repo, ["scripts/wait.sh"]
            )
            self.assertEqual(len(findings), 1)
            self.assertIn("gh run watch unbound", findings[0])

    def test_doc_prose_is_not_scanned(self):
        """The same strings in docs prose must not be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "docs").mkdir(parents=True)
            (repo / "docs" / "notes.md").write_text(
                "Do not run `gh run list --limit 1` and do not pass "
                "`--dangerously-skip-permissions`.\n"
            )
            findings = csb.check_dormant_automation_guard(
                repo, ["docs/notes.md"]
            )
            self.assertEqual(findings, [])

    def test_vendor_and_generated_dirs_are_not_scanned(self):
        """Vendored and generated files must not be scanned."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "vendor" / "lib").mkdir(parents=True)
            (repo / "vendor" / "lib" / "loop.sh").write_text(
                "claude --dangerously-skip-permissions\n"
            )
            (repo / "scripts" / "generated").mkdir(parents=True)
            (repo / "scripts" / "generated" / "gen.sh").write_text(
                "gh run list --limit 1\n"
            )
            findings = csb.check_dormant_automation_guard(
                repo, ["vendor/lib/loop.sh", "scripts/generated/gen.sh"]
            )
            self.assertEqual(findings, [])

    def test_allowlisted_pattern_passes(self):
        """An explicit allowlist entry with a reason suppresses the finding."""
        original = csb.AUTOMATION_GUARD_ALLOWLIST
        csb.AUTOMATION_GUARD_ALLOWLIST = {
            "scripts/special_loop.sh": {
                "gh run list --limit 1": "bounded maintenance fixture; reviewed"
            }
        }
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                repo = Path(tmpdir)
                (repo / "scripts").mkdir(parents=True)
                (repo / "scripts" / "special_loop.sh").write_text(
                    "gh run list --limit 1\n"
                )
                findings = csb.check_dormant_automation_guard(
                    repo, ["scripts/special_loop.sh"]
                )
                self.assertEqual(findings, [])
        finally:
            csb.AUTOMATION_GUARD_ALLOWLIST = original

    def test_allowlist_only_matches_exact_file_and_pattern(self):
        """An allowlist entry must not leak to other files or patterns."""
        original = csb.AUTOMATION_GUARD_ALLOWLIST
        csb.AUTOMATION_GUARD_ALLOWLIST = {
            "scripts/special_loop.sh": {
                "gh run list --limit 1": "bounded maintenance fixture; reviewed"
            }
        }
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                repo = Path(tmpdir)
                (repo / "scripts").mkdir(parents=True)
                (repo / "scripts" / "other_loop.sh").write_text(
                    "gh run list --limit 1\n"
                )
                (repo / "scripts" / "special_loop.sh").write_text(
                    "gh run watch --exit-status\n"
                )
                findings = csb.check_dormant_automation_guard(
                    repo,
                    ["scripts/other_loop.sh", "scripts/special_loop.sh"],
                )
                self.assertEqual(len(findings), 2)
        finally:
            csb.AUTOMATION_GUARD_ALLOWLIST = original


class TestRemovedPluginSurfaceGuard(unittest.TestCase):
    """Tests for the removed plugin-surface guard check."""

    def test_clean_engine_source_passes(self):
        """Engine source without plugin trust tokens should pass."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "scheduler.rs").write_text(
                "pub fn tick() -> u32 { 1 }\n"
            )
            findings = csb.check_removed_plugin_surface_guard(
                repo, ["engine/src/scheduler.rs"]
            )
            self.assertEqual(findings, [])

    def test_single_legacy_token_is_allowed(self):
        """A single legacy-named constant is business code, not a revival."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "plugin.rs").write_text(
                'pub const TRUST_LEVEL_OFFICIAL: &str = "official";\n'
            )
            findings = csb.check_removed_plugin_surface_guard(
                repo, ["engine/src/plugin.rs"]
            )
            self.assertEqual(findings, [])

    def test_two_legacy_tokens_flagged(self):
        """Two or more legacy tokens in one file signal a revival attempt."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "plugin.rs").write_text(
                "pub const TRUST_LEVEL_OFFICIAL: &str = \"official\";\n"
                "pub const ALL_KNOWN_PERMISSIONS: &[&str] = &[];\n"
            )
            findings = csb.check_removed_plugin_surface_guard(
                repo, ["engine/src/plugin.rs"]
            )
            self.assertEqual(len(findings), 1)
            self.assertIn("legacy plugin trust tokens", findings[0])

    def test_detects_unrestricted_comment(self):
        """The 'empty = unrestricted' trust semantic must be flagged."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "trust.rs").write_text(
                "TRUST_LEVEL_OFFICIAL => HashSet::new(), // empty = unrestricted\n"
            )
            findings = csb.check_removed_plugin_surface_guard(
                repo, ["engine/src/trust.rs"]
            )
            self.assertEqual(len(findings), 1)
            self.assertIn("empty = unrestricted", findings[0])

    def test_resurrected_plugin_path_flagged(self):
        """Re-creating the deleted plugin files is always a finding."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src" / "infrastructure").mkdir(parents=True)
            (repo / "engine" / "src" / "infrastructure" / "plugin_system.rs").write_text(
                "pub fn load() -> Result<(), String> { Ok(()) }\n"
            )
            findings = csb.check_removed_plugin_surface_guard(
                repo, ["engine/src/infrastructure/plugin_system.rs"]
            )
            self.assertEqual(len(findings), 1)
            self.assertIn("resurrected", findings[0])

    def test_generic_verified_identifier_passes(self):
        """Generic verified/official identifiers are legal business names."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "trust.rs").write_text(
                "pub fn is_verified(sha: &str) -> bool { sha.len() == 64 }\n"
            )
            findings = csb.check_removed_plugin_surface_guard(
                repo, ["engine/src/trust.rs"]
            )
            self.assertEqual(findings, [])

    def test_engine_tests_are_not_scanned(self):
        """engine/tests is outside the production crate source scan scope."""
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "tests").mkdir(parents=True)
            (repo / "engine" / "tests" / "test_plugin.rs").write_text(
                'pub const TRUST_LEVEL_OFFICIAL: &str = "official";\n'
            )
            findings = csb.check_removed_plugin_surface_guard(
                repo, ["engine/tests/test_plugin.rs"]
            )
            self.assertEqual(findings, [])


class TestCheckNumbering(unittest.TestCase):
    """The main() progress labels must be sequential 1/N..N/N."""

    def test_labels_are_sequential(self):
        for index, label in enumerate(csb.CHECK_LABELS, start=1):
            self.assertEqual(label, csb.CHECK_LABELS[index - 1])
        self.assertGreaterEqual(len(csb.CHECK_LABELS), 7)

    def test_label_count_matches_checks(self):
        self.assertEqual(len(csb.CHECK_LABELS), 8)


class TestDormantSurfaceHeuristics(unittest.TestCase):
    """Tests for the dormant surface heuristic gate."""

    def test_module_level_dead_code_blanket_flagged(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "dormant.rs").write_text(
                "#![allow(dead_code)]\n\npub fn helper() -> u8 { 1 }\n"
            )
            findings = csb.check_dormant_surface_heuristics(
                repo, ["engine/src/dormant.rs"]
            )
            self.assertTrue(
                any("allow(dead_code)" in f for f in findings)
            )

    def test_clean_source_passes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "wired.rs").write_text(
                "pub fn helper() -> u8 { 1 }\n"
            )
            findings = csb.check_dormant_surface_heuristics(
                repo, ["engine/src/wired.rs"]
            )
            self.assertEqual(findings, [])

    def test_module_island_flagged(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "lib.rs").write_text(
                "pub mod ghost_module;\n"
            )
            (repo / "engine" / "src" / "ghost_module.rs").write_text(
                "pub fn nothing() {}\n"
            )
            findings = csb.check_dormant_surface_heuristics(
                repo, ["engine/src/lib.rs", "engine/src/ghost_module.rs"]
            )
            self.assertTrue(any("ghost_module" in f for f in findings))

    def test_self_described_placeholder_flagged(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "future.rs").write_text(
                "//! This module is a placeholder for the future workstream.\n"
                "pub fn stub_thing() {}\n"
            )
            findings = csb.check_dormant_surface_heuristics(
                repo, ["engine/src/future.rs"]
            )
            self.assertTrue(any("placeholder" in f for f in findings))

    def test_quality_gate_style_header_passes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "gate.rs").write_text(
                "// Local type stubs for removed placeholder modules.\n"
                "pub fn gate() -> bool { true }\n"
            )
            findings = csb.check_dormant_surface_heuristics(
                repo, ["engine/src/gate.rs"]
            )
            self.assertEqual(findings, [])

    def test_empty_executor_fn_flagged(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "executor.rs").write_text(
                "use serde_json::{json, Value};\n"
                "pub fn execute_task(_input: &Value) -> Value {\n"
                "    json!({})\n"
                "}\n"
            )
            findings = csb.check_dormant_surface_heuristics(
                repo, ["engine/src/executor.rs"]
            )
            self.assertTrue(any("execute_task" in f for f in findings))

    def test_non_empty_executor_passes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "executor.rs").write_text(
                "pub fn execute_task(input: u32) -> u32 { input + 1 }\n"
            )
            findings = csb.check_dormant_surface_heuristics(
                repo, ["engine/src/executor.rs"]
            )
            self.assertEqual(findings, [])

    def test_cfg_test_empty_executor_not_flagged(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "executor.rs").write_text(
                "use serde_json::json;\n"
                "#[cfg(test)]\n"
                "fn execute_test_stub() -> serde_json::Value {\n"
                "    json!({})\n"
                "}\n"
            )
            findings = csb.check_dormant_surface_heuristics(
                repo, ["engine/src/executor.rs"]
            )
            self.assertEqual(findings, [])

    def test_conflicting_owner_claims_flagged(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = Path(tmpdir)
            (repo / "engine" / "src").mkdir(parents=True)
            (repo / "engine" / "src" / "alpha.rs").write_text(
                "//! Alpha is the sole owner of the session store.\n"
            )
            (repo / "engine" / "src" / "beta.rs").write_text(
                "//! Beta is the sole owner of the session store.\n"
            )
            findings = csb.check_dormant_surface_heuristics(
                repo, ["engine/src/alpha.rs", "engine/src/beta.rs"]
            )
            self.assertTrue(any("session store" in f for f in findings))

    def test_classification_allowlist_suppresses(self):
        original = csb.DORMANT_SURFACE_CLASSIFICATION_ALLOWLIST
        csb.DORMANT_SURFACE_CLASSIFICATION_ALLOWLIST = original + [
            {
                "path": "engine/src/executor.rs",
                "classification": "wired",
                "owner": "test",
                "reason": "fixture",
                "review_condition": "test",
                "expiry_or_recheck_condition": "test",
            }
        ]
        try:
            with tempfile.TemporaryDirectory() as tmpdir:
                repo = Path(tmpdir)
                (repo / "engine" / "src").mkdir(parents=True)
                (repo / "engine" / "src" / "executor.rs").write_text(
                    "pub fn execute_task() -> serde_json::Value { "
                    "serde_json::json!({}) }\n"
                )
                findings = csb.check_dormant_surface_heuristics(
                    repo, ["engine/src/executor.rs"]
                )
                self.assertEqual(findings, [])
        finally:
            csb.DORMANT_SURFACE_CLASSIFICATION_ALLOWLIST = original


if __name__ == "__main__":
    unittest.main()
