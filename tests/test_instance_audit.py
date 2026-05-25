"""Tests for the read-only harness instance auditor."""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from harness_core.instance_audit import audit_instance, format_report


VALID_AGENTS = """# Agent Execution Policy

Claude Code operates as an execution adapter under the Token-Efficient Agent Harness.
It is not the governance authority.
All governance decisions require human authorisation.
Claude Code must pause before pushing directly to `main` or `master`.
Claude Code must not connect real LLM provider systems without approval.
Claude Code must pause before modifying active YAML or active user/project state.
"""

BROAD_AUTOMATION_AGENTS = VALID_AGENTS + """

## Automation Authorisation

The human operator has authorised a default fully automated workflow for ordinary engineering work.
Claude Code must still pause for explicit human confirmation before deleting project state,
modifying active YAML under `alters/current/**`, or pushing directly to `main` or `master`.
"""

UNSAFE_AGENTS = """# Agent Execution Policy

Claude Code is the governance authority.
Claude Code may push to main without approval.
Claude Code may connect providers and modify active state.
"""

VALID_BOARD = """# Project Board

## Task States

```
todo → ready → running → review → done/failed
```

## Phase 1

| ID | Title | Status |
|----|-------|--------|
| P1-001 | First slice | done |
| P2-000 | Future scope | blocked |

Phase 1 Final Gate: PASS
Closeout report: docs/harness/PHASE1_CLOSEOUT_REPORT.md
"""

MALFORMED_BOARD = VALID_BOARD + """
 P1-002 | Missing leading pipe | done |
"""

FUTURE_DONE_BOARD = """# Project Board

## Task States

todo ready running review done

| ID | Title | Status |
|----|-------|--------|
| P5-000 | Future productization | done |
"""

VALID_QUEUE = """# Task Queue

### P1-001: First slice

**Status**: done
**Goal**: Do a controlled task.
**Notes**: Evidence produced.

### P2-000: Future scope

**Status**: blocked
**Goal**: Future work requiring approval.
"""

VALID_GATES = """# Quality Gates

- unknown_error requires human review.
- No provider or LLM calls before approved phase.
- Active state mutation requires human approval.
- Rubric cannot auto modify itself.
- Read-only and evidence-only boundaries must remain clear.
"""

VALID_DECISIONS = """# Decision Record

## Decisions

### Decision D-001

**Status**: accepted
**Context**: test
**Decision**: test
"""

VALID_RISKS = """# Risk Register

| ID | Risk | Likelihood | Impact | Mitigation | Status |
|----|------|------------|--------|------------|--------|
| R-001 | Scope drift beyond project | Medium | High | Slice boundaries | Active |
| R-002 | Provider/LLM premature integration | Medium | High | Approval gate | Active |
| R-003 | Mutation of active state | Medium | High | Human approval | Mitigated |
"""

VALID_CLOSEOUT = """# Phase 1 Closeout Report

**Status**: PASS
**Test count**: 12
**Sealed baseline candidate**: True

## Boundary Confirmations

- provider_used: False
"""


def write_instance(root: Path, *, agents: str = VALID_AGENTS, board: str = VALID_BOARD, queue: str = VALID_QUEUE) -> None:
    (root / "docs" / "harness").mkdir(parents=True)
    (root / "AGENTS.md").write_text(agents, encoding="utf-8")
    (root / "docs" / "harness" / "PROJECT_BOARD.md").write_text(board, encoding="utf-8")
    (root / "docs" / "harness" / "TASK_QUEUE.md").write_text(queue, encoding="utf-8")
    (root / "docs" / "harness" / "QUALITY_GATES.md").write_text(VALID_GATES, encoding="utf-8")
    (root / "docs" / "harness" / "DECISION_RECORD.md").write_text(VALID_DECISIONS, encoding="utf-8")
    (root / "docs" / "harness" / "RISK_REGISTER.md").write_text(VALID_RISKS, encoding="utf-8")
    (root / "docs" / "harness" / "PHASE1_CLOSEOUT_REPORT.md").write_text(VALID_CLOSEOUT, encoding="utf-8")


class TestInstanceAudit(unittest.TestCase):
    def test_valid_instance_passes_or_passes_with_notes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_instance(root)
            report = audit_instance(root)
            self.assertIn(report.verdict, {"PASS", "PASS_WITH_NOTES"})
            self.assertFalse(report.blockers)

    def test_missing_project_board_blocks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_instance(root)
            (root / "docs" / "harness" / "PROJECT_BOARD.md").unlink()
            report = audit_instance(root)
            self.assertEqual(report.verdict, "BLOCKED")
            self.assertTrue(any("PROJECT_BOARD" in item for item in report.blockers))

    def test_missing_agents_blocks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_instance(root)
            (root / "AGENTS.md").unlink()
            report = audit_instance(root)
            self.assertEqual(report.verdict, "BLOCKED")
            self.assertTrue(any("AGENTS" in item for item in report.blockers))

    def test_unsafe_agents_policy_blocks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_instance(root, agents=UNSAFE_AGENTS)
            report = audit_instance(root)
            self.assertEqual(report.verdict, "BLOCKED")
            self.assertTrue(any("AGENTS" in item or "main" in item for item in report.blockers))

    def test_future_phase_done_without_evidence_blocks(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_instance(root, board=FUTURE_DONE_BOARD)
            report = audit_instance(root)
            self.assertEqual(report.verdict, "BLOCKED")
            self.assertTrue(any("Future phase" in item for item in report.blockers))

    def test_broad_automation_with_pause_conditions_warns(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_instance(root, agents=BROAD_AUTOMATION_AGENTS)
            report = audit_instance(root)
            self.assertIn(report.verdict, {"PASS_WITH_NOTES", "PASS"})
            self.assertTrue(any("broad automation" in item.lower() for item in report.warnings))

    def test_malformed_board_table_warns(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_instance(root, board=MALFORMED_BOARD)
            report = audit_instance(root)
            self.assertIn(report.verdict, {"PASS_WITH_NOTES", "PASS"})
            self.assertTrue(any("PROJECT_BOARD" in item for item in report.warnings))

    def test_json_output_is_valid(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_instance(root)
            report = audit_instance(root)
            data = json.loads(report.to_json())
            self.assertIn("verdict", data)
            self.assertIn("checks", data)
            self.assertIn("warnings", data)
            self.assertIn("blockers", data)

    def test_format_report_contains_verdict(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_instance(root)
            report = audit_instance(root)
            rendered = format_report(report)
            self.assertIn("Verdict:", rendered)
            self.assertIn(str(root.resolve()), rendered)

    def test_auditor_does_not_write_to_target_repo(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            write_instance(root)
            before = {
                path.relative_to(root).as_posix(): path.read_text(encoding="utf-8")
                for path in root.rglob("*")
                if path.is_file()
            }
            audit_instance(root)
            after = {
                path.relative_to(root).as_posix(): path.read_text(encoding="utf-8")
                for path in root.rglob("*")
                if path.is_file()
            }
            self.assertEqual(before, after)


if __name__ == "__main__":
    unittest.main()
