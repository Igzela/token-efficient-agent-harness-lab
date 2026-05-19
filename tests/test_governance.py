"""Tests for Governance Approval Path Enforcement track.

Covers governance_decision.v1 schema validation, 5-gate evaluation,
decision logic, cross-track references, and fixture loading.
"""

import json
import unittest
from pathlib import Path

from harness_core.governance import (
    GOVERNANCE_DECISION_VERSION,
    DECISION_VALUES,
    GATE_RESULTS,
    ROLLBACK_SCOPES_HARNESS_LEVEL,
    USER_PROJECT_INDICATORS,
    GOVERNANCE_DECISION_REQUIRED,
    DECISION_BASIS_REQUIRED,
    GATE_RESULTS_REQUIRED,
    validate_governance_decision,
    evaluate_evidence_gate,
    evaluate_approval_gate,
    evaluate_rollback_gate,
    evaluate_scope_gate,
    evaluate_unknown_error_gate,
    decide_policy_activation,
    governance_allows_activation,
    governance_blocks_activation,
    explain_blocked_reasons,
    load_fixture,
    load_all_fixtures,
)

FIXTURE_DIR = Path(__file__).parent / "fixtures" / "governance"


class TestSchemaVersion(unittest.TestCase):
    """G1: Schema version constant is correct."""

    def test_version_value(self):
        self.assertEqual(GOVERNANCE_DECISION_VERSION, "governance_decision.v1")


class TestDecisionEnum(unittest.TestCase):
    """G2: Decision enum contains exactly 4 values."""

    def test_decision_count(self):
        self.assertEqual(len(DECISION_VALUES), 4)

    def test_decision_values(self):
        expected = {
            "approve_activation",
            "reject_activation",
            "defer_activation",
            "require_more_evidence",
        }
        self.assertEqual(set(DECISION_VALUES), expected)


class TestGateResultsEnum(unittest.TestCase):
    """G3: Gate results enum contains pass and fail."""

    def test_gate_results_count(self):
        self.assertEqual(len(GATE_RESULTS), 2)

    def test_gate_results_values(self):
        self.assertEqual(set(GATE_RESULTS), {"pass", "fail"})


class TestRollbackScopes(unittest.TestCase):
    """G4: ROLLBACK_SCOPES_HARNESS_LEVEL contains expected harness-only scopes."""

    def test_harness_scopes(self):
        expected = {
            "docs_only", "config", "schema", "profile",
            "skill", "eval_gate", "runtime_guard",
        }
        self.assertEqual(ROLLBACK_SCOPES_HARNESS_LEVEL, expected)


class TestUserProjectIndicators(unittest.TestCase):
    """G5: USER_PROJECT_INDICATORS contains expected project path indicators."""

    def test_indicators(self):
        expected = ("/src/", "/lib/", "/app/", "/home/", "/Users/", "/workspace/")
        self.assertEqual(USER_PROJECT_INDICATORS, expected)


class TestRequiredFields(unittest.TestCase):
    """G6: Required field lists are non-empty and correct."""

    def test_governance_decision_required(self):
        self.assertEqual(len(GOVERNANCE_DECISION_REQUIRED), 12)
        self.assertIn("schema_version", GOVERNANCE_DECISION_REQUIRED)
        self.assertIn("decision", GOVERNANCE_DECISION_REQUIRED)
        self.assertIn("gate_results", GOVERNANCE_DECISION_REQUIRED)

    def test_decision_basis_required(self):
        self.assertEqual(len(DECISION_BASIS_REQUIRED), 5)
        self.assertIn("admitted_evidence_refs", DECISION_BASIS_REQUIRED)
        self.assertIn("approval_ref", DECISION_BASIS_REQUIRED)

    def test_gate_results_required(self):
        self.assertEqual(len(GATE_RESULTS_REQUIRED), 5)
        expected_gates = {
            "evidence_gate", "approval_gate", "rollback_gate",
            "scope_gate", "unknown_error_gate",
        }
        self.assertEqual(set(GATE_RESULTS_REQUIRED), expected_gates)


class TestValidateGovernanceDecision(unittest.TestCase):
    """G7: validate_governance_decision catches missing fields, bad enums, wrong types."""

    def test_valid_decision(self):
        data = {
            "schema_version": GOVERNANCE_DECISION_VERSION,
            "decision_id": "gov-test-001",
            "candidate_id": "cand-test-001",
            "policy_id": "pol-test-001",
            "decision": "approve_activation",
            "decision_basis": {
                "admitted_evidence_refs": ["evidence:001"],
                "diagnostic_evidence_refs": [],
                "approval_ref": "approval:001",
                "rollback_plan_ref": "rbp-001",
                "registry_entry_ref": "pol-001",
            },
            "gate_results": {
                "evidence_gate": "pass",
                "approval_gate": "pass",
                "rollback_gate": "pass",
                "scope_gate": "pass",
                "unknown_error_gate": "pass",
            },
            "blocked_reasons": [],
            "allowed_next_actions": ["activate_policy"],
            "forbidden_next_actions": [],
            "decided_by": "governance_engine",
            "decided_at": "2026-05-19T10:00:00Z",
        }
        violations = validate_governance_decision(data)
        self.assertEqual(violations, [])

    def test_missing_required_field(self):
        data = {
            "schema_version": GOVERNANCE_DECISION_VERSION,
            "decision_id": "gov-test-002",
            # missing candidate_id
            "policy_id": "pol-test-002",
            "decision": "approve_activation",
            "decision_basis": {
                "admitted_evidence_refs": [],
                "diagnostic_evidence_refs": [],
                "approval_ref": "",
                "rollback_plan_ref": "",
                "registry_entry_ref": "",
            },
            "gate_results": {
                "evidence_gate": "pass",
                "approval_gate": "pass",
                "rollback_gate": "pass",
                "scope_gate": "pass",
                "unknown_error_gate": "pass",
            },
            "blocked_reasons": [],
            "allowed_next_actions": [],
            "forbidden_next_actions": [],
            "decided_by": "governance_engine",
            "decided_at": "2026-05-19T10:00:00Z",
        }
        violations = validate_governance_decision(data)
        self.assertTrue(any("candidate_id" in v for v in violations))

    def test_bad_schema_version(self):
        data = {
            "schema_version": "governance_decision.v2",
            "decision_id": "gov-test-003",
            "candidate_id": "cand-test-003",
            "policy_id": "pol-test-003",
            "decision": "approve_activation",
            "decision_basis": {
                "admitted_evidence_refs": [],
                "diagnostic_evidence_refs": [],
                "approval_ref": "",
                "rollback_plan_ref": "",
                "registry_entry_ref": "",
            },
            "gate_results": {
                "evidence_gate": "pass",
                "approval_gate": "pass",
                "rollback_gate": "pass",
                "scope_gate": "pass",
                "unknown_error_gate": "pass",
            },
            "blocked_reasons": [],
            "allowed_next_actions": [],
            "forbidden_next_actions": [],
            "decided_by": "governance_engine",
            "decided_at": "2026-05-19T10:00:00Z",
        }
        violations = validate_governance_decision(data)
        self.assertTrue(any("schema_version" in v for v in violations))

    def test_bad_decision_enum(self):
        data = {
            "schema_version": GOVERNANCE_DECISION_VERSION,
            "decision_id": "gov-test-004",
            "candidate_id": "cand-test-004",
            "policy_id": "pol-test-004",
            "decision": "maybe_activation",
            "decision_basis": {
                "admitted_evidence_refs": [],
                "diagnostic_evidence_refs": [],
                "approval_ref": "",
                "rollback_plan_ref": "",
                "registry_entry_ref": "",
            },
            "gate_results": {
                "evidence_gate": "pass",
                "approval_gate": "pass",
                "rollback_gate": "pass",
                "scope_gate": "pass",
                "unknown_error_gate": "pass",
            },
            "blocked_reasons": [],
            "allowed_next_actions": [],
            "forbidden_next_actions": [],
            "decided_by": "governance_engine",
            "decided_at": "2026-05-19T10:00:00Z",
        }
        violations = validate_governance_decision(data)
        self.assertTrue(any("decision" in v for v in violations))

    def test_bad_gate_result_value(self):
        data = {
            "schema_version": GOVERNANCE_DECISION_VERSION,
            "decision_id": "gov-test-005",
            "candidate_id": "cand-test-005",
            "policy_id": "pol-test-005",
            "decision": "approve_activation",
            "decision_basis": {
                "admitted_evidence_refs": [],
                "diagnostic_evidence_refs": [],
                "approval_ref": "",
                "rollback_plan_ref": "",
                "registry_entry_ref": "",
            },
            "gate_results": {
                "evidence_gate": "pass",
                "approval_gate": "pass",
                "rollback_gate": "pass",
                "scope_gate": "pass",
                "unknown_error_gate": "maybe",
            },
            "blocked_reasons": [],
            "allowed_next_actions": [],
            "forbidden_next_actions": [],
            "decided_by": "governance_engine",
            "decided_at": "2026-05-19T10:00:00Z",
        }
        violations = validate_governance_decision(data)
        self.assertTrue(any("gate_results" in v for v in violations))

    def test_bad_decision_basis_type(self):
        data = {
            "schema_version": GOVERNANCE_DECISION_VERSION,
            "decision_id": "gov-test-006",
            "candidate_id": "cand-test-006",
            "policy_id": "pol-test-006",
            "decision": "approve_activation",
            "decision_basis": "not_a_dict",
            "gate_results": {
                "evidence_gate": "pass",
                "approval_gate": "pass",
                "rollback_gate": "pass",
                "scope_gate": "pass",
                "unknown_error_gate": "pass",
            },
            "blocked_reasons": [],
            "allowed_next_actions": [],
            "forbidden_next_actions": [],
            "decided_by": "governance_engine",
            "decided_at": "2026-05-19T10:00:00Z",
        }
        violations = validate_governance_decision(data)
        self.assertTrue(any("decision_basis" in v for v in violations))

    def test_bad_blocked_reasons_type(self):
        data = {
            "schema_version": GOVERNANCE_DECISION_VERSION,
            "decision_id": "gov-test-007",
            "candidate_id": "cand-test-007",
            "policy_id": "pol-test-007",
            "decision": "reject_activation",
            "decision_basis": {
                "admitted_evidence_refs": [],
                "diagnostic_evidence_refs": [],
                "approval_ref": "",
                "rollback_plan_ref": "",
                "registry_entry_ref": "",
            },
            "gate_results": {
                "evidence_gate": "pass",
                "approval_gate": "pass",
                "rollback_gate": "pass",
                "scope_gate": "pass",
                "unknown_error_gate": "pass",
            },
            "blocked_reasons": "not_a_list",
            "allowed_next_actions": [],
            "forbidden_next_actions": [],
            "decided_by": "governance_engine",
            "decided_at": "2026-05-19T10:00:00Z",
        }
        violations = validate_governance_decision(data)
        self.assertTrue(any("blocked_reasons" in v for v in violations))


class TestEvaluateEvidenceGate(unittest.TestCase):
    """G8: evidence_gate passes only when admitted_evidence_refs is non-empty."""

    def test_pass_with_admitted(self):
        result, reason = evaluate_evidence_gate({
            "admitted_evidence_refs": ["evidence:001"],
            "diagnostic_evidence_refs": [],
        })
        self.assertEqual(result, "pass")
        self.assertIn("admitted", reason)

    def test_fail_diagnostic_only(self):
        result, reason = evaluate_evidence_gate({
            "admitted_evidence_refs": [],
            "diagnostic_evidence_refs": ["diagnostic:001"],
        })
        self.assertEqual(result, "fail")
        self.assertIn("diagnostic-only", reason)

    def test_fail_empty(self):
        result, reason = evaluate_evidence_gate({
            "admitted_evidence_refs": [],
            "diagnostic_evidence_refs": [],
        })
        self.assertEqual(result, "fail")


class TestEvaluateApprovalGate(unittest.TestCase):
    """G9: approval_gate passes only when decision=approved."""

    def test_pass_approved(self):
        result, reason = evaluate_approval_gate({"decision": "approved"})
        self.assertEqual(result, "pass")
        self.assertIn("approved", reason)

    def test_fail_rejected(self):
        result, reason = evaluate_approval_gate({"decision": "rejected"})
        self.assertEqual(result, "fail")
        self.assertIn("rejected", reason)

    def test_fail_deferred(self):
        result, reason = evaluate_approval_gate({"decision": "deferred"})
        self.assertEqual(result, "fail")
        self.assertIn("deferred", reason)

    def test_fail_none(self):
        result, reason = evaluate_approval_gate(None)
        self.assertEqual(result, "fail")
        self.assertIn("no approval_record", reason)


class TestEvaluateRollbackGate(unittest.TestCase):
    """G10: rollback_gate passes only when status=approved and steps non-empty."""

    def test_pass_approved_with_steps(self):
        result, reason = evaluate_rollback_gate({
            "status": "approved",
            "rollback_steps": [{"step": 1, "action": "revert"}],
        })
        self.assertEqual(result, "pass")

    def test_fail_proposed(self):
        result, reason = evaluate_rollback_gate({
            "status": "proposed",
            "rollback_steps": [{"step": 1, "action": "revert"}],
        })
        self.assertEqual(result, "fail")
        self.assertIn("proposed", reason)

    def test_fail_empty_steps(self):
        result, reason = evaluate_rollback_gate({
            "status": "approved",
            "rollback_steps": [],
        })
        self.assertEqual(result, "fail")
        self.assertIn("empty", reason)

    def test_fail_none(self):
        result, reason = evaluate_rollback_gate(None)
        self.assertEqual(result, "fail")
        self.assertIn("no rollback_plan", reason)


class TestEvaluateScopeGate(unittest.TestCase):
    """G11: scope_gate fails when impacted_refs contain user project file paths."""

    def test_pass_harness_only(self):
        result, reason = evaluate_scope_gate(
            {"policy_id": "pol-001"},
            {"impacted_refs": [{"path_or_registry_key": "docs/context_pack_v2.md"}]},
        )
        self.assertEqual(result, "pass")

    def test_fail_user_project(self):
        result, reason = evaluate_scope_gate(
            {"policy_id": "pol-002"},
            {"impacted_refs": [{"path_or_registry_key": "/src/main.py"}]},
        )
        self.assertEqual(result, "fail")
        self.assertIn("user project", reason)

    def test_fail_home_path(self):
        result, reason = evaluate_scope_gate(
            {"policy_id": "pol-003"},
            {"impacted_refs": [{"path_or_registry_key": "/home/user/project/main.py"}]},
        )
        self.assertEqual(result, "fail")
        self.assertIn("user project", reason)

    def test_fail_none_rollback(self):
        result, reason = evaluate_scope_gate(
            {"policy_id": "pol-004"},
            None,
        )
        self.assertEqual(result, "fail")
        self.assertIn("no rollback_plan", reason)

    def test_pass_empty_impacted(self):
        result, reason = evaluate_scope_gate(
            {"policy_id": "pol-005"},
            {"impacted_refs": []},
        )
        self.assertEqual(result, "pass")


class TestEvaluateUnknownErrorGate(unittest.TestCase):
    """G12: unknown_error_gate fails when unknown_error evidence has no human review."""

    def test_pass_no_unknown(self):
        result, reason = evaluate_unknown_error_gate({
            "diagnostic_evidence_refs": ["diagnostic:001:lint"],
            "human_review_refs": [],
        })
        self.assertEqual(result, "pass")

    def test_pass_unknown_with_human_review(self):
        result, reason = evaluate_unknown_error_gate({
            "diagnostic_evidence_refs": ["diagnostic:001:unknown_error"],
            "human_review_refs": ["review:001"],
        })
        self.assertEqual(result, "pass")

    def test_fail_unknown_no_human_review(self):
        result, reason = evaluate_unknown_error_gate({
            "diagnostic_evidence_refs": ["diagnostic:001:unknown_error"],
            "human_review_refs": [],
        })
        self.assertEqual(result, "fail")
        self.assertIn("human review", reason)


class TestDecidePolicyActivation(unittest.TestCase):
    """G13: decide_policy_activation orchestrates all gates and produces correct decision."""

    def _make_candidate(self, cid="cand-001", pid="pol-001"):
        return {"candidate_id": cid, "policy_id": pid}

    def _make_registry(self, pid="pol-001"):
        return {"policy_id": pid}

    def test_all_pass_approve(self):
        decision = decide_policy_activation(
            self._make_candidate(),
            {"admitted_evidence_refs": ["evidence:001"], "diagnostic_evidence_refs": []},
            {"decision": "approved"},
            {"status": "approved", "rollback_steps": [{"step": 1}]},
            self._make_registry(),
        )
        self.assertEqual(decision["decision"], "approve_activation")
        self.assertEqual(decision["gate_results"]["evidence_gate"], "pass")
        self.assertEqual(decision["gate_results"]["approval_gate"], "pass")
        self.assertEqual(decision["gate_results"]["rollback_gate"], "pass")
        self.assertEqual(decision["gate_results"]["scope_gate"], "pass")
        self.assertEqual(decision["gate_results"]["unknown_error_gate"], "pass")
        self.assertEqual(decision["blocked_reasons"], [])
        self.assertIn("activate_policy", decision["allowed_next_actions"])

    def test_evidence_fail(self):
        decision = decide_policy_activation(
            self._make_candidate(),
            {"admitted_evidence_refs": [], "diagnostic_evidence_refs": ["diag:001"]},
            {"decision": "approved"},
            {"status": "approved", "rollback_steps": [{"step": 1}]},
            self._make_registry(),
        )
        self.assertEqual(decision["decision"], "require_more_evidence")
        self.assertEqual(decision["gate_results"]["evidence_gate"], "fail")
        self.assertIn("activate_policy", decision["forbidden_next_actions"])

    def test_approval_fail(self):
        decision = decide_policy_activation(
            self._make_candidate(),
            {"admitted_evidence_refs": ["evidence:001"], "diagnostic_evidence_refs": []},
            {"decision": "rejected"},
            {"status": "approved", "rollback_steps": [{"step": 1}]},
            self._make_registry(),
        )
        self.assertEqual(decision["decision"], "reject_activation")
        self.assertEqual(decision["gate_results"]["approval_gate"], "fail")

    def test_rollback_fail(self):
        decision = decide_policy_activation(
            self._make_candidate(),
            {"admitted_evidence_refs": ["evidence:001"], "diagnostic_evidence_refs": []},
            {"decision": "approved"},
            {"status": "proposed", "rollback_steps": [{"step": 1}]},
            self._make_registry(),
        )
        self.assertEqual(decision["decision"], "reject_activation")
        self.assertEqual(decision["gate_results"]["rollback_gate"], "fail")

    def test_scope_fail_user_project(self):
        decision = decide_policy_activation(
            self._make_candidate(),
            {"admitted_evidence_refs": ["evidence:001"], "diagnostic_evidence_refs": []},
            {"decision": "approved"},
            {
                "status": "approved",
                "rollback_steps": [{"step": 1}],
                "impacted_refs": [{"path_or_registry_key": "/src/main.py"}],
            },
            self._make_registry(),
        )
        self.assertEqual(decision["decision"], "reject_activation")
        self.assertEqual(decision["gate_results"]["scope_gate"], "fail")

    def test_unknown_error_no_human_review(self):
        decision = decide_policy_activation(
            self._make_candidate(),
            {
                "admitted_evidence_refs": ["evidence:001"],
                "diagnostic_evidence_refs": ["diagnostic:001:unknown_error"],
                "human_review_refs": [],
            },
            {"decision": "approved"},
            {"status": "approved", "rollback_steps": [{"step": 1}]},
            self._make_registry(),
        )
        self.assertEqual(decision["decision"], "require_more_evidence")
        self.assertEqual(decision["gate_results"]["unknown_error_gate"], "fail")
        self.assertIn("collect_human_review", decision["allowed_next_actions"])

    def test_unknown_error_with_human_review_passes(self):
        decision = decide_policy_activation(
            self._make_candidate(),
            {
                "admitted_evidence_refs": ["evidence:001"],
                "diagnostic_evidence_refs": ["diagnostic:001:unknown_error"],
                "human_review_refs": ["review:001"],
            },
            {"decision": "approved"},
            {"status": "approved", "rollback_steps": [{"step": 1}]},
            self._make_registry(),
        )
        self.assertEqual(decision["decision"], "approve_activation")

    def test_schema_version_is_set(self):
        decision = decide_policy_activation(
            self._make_candidate(),
            {"admitted_evidence_refs": [], "diagnostic_evidence_refs": []},
            None,
            None,
            self._make_registry(),
        )
        self.assertEqual(decision["schema_version"], GOVERNANCE_DECISION_VERSION)

    def test_decision_id_format(self):
        decision = decide_policy_activation(
            self._make_candidate(cid="cand-xyz"),
            {"admitted_evidence_refs": [], "diagnostic_evidence_refs": []},
            None,
            None,
            self._make_registry(),
        )
        self.assertIn("cand-xyz", decision["decision_id"])

    def test_decided_by_is_engine(self):
        decision = decide_policy_activation(
            self._make_candidate(),
            {"admitted_evidence_refs": [], "diagnostic_evidence_refs": []},
            None,
            None,
            self._make_registry(),
        )
        self.assertEqual(decision["decided_by"], "governance_engine")


class TestGovernanceAllowsActivation(unittest.TestCase):
    """G14: governance_allows_activation returns True only for approve_activation."""

    def test_allows_approve(self):
        allows, reason = governance_allows_activation({"decision": "approve_activation"})
        self.assertTrue(allows)
        self.assertIn("approve_activation", reason)

    def test_blocks_reject(self):
        allows, reason = governance_allows_activation({"decision": "reject_activation"})
        self.assertFalse(allows)

    def test_blocks_defer(self):
        allows, reason = governance_allows_activation({"decision": "defer_activation"})
        self.assertFalse(allows)

    def test_blocks_require(self):
        allows, reason = governance_allows_activation({"decision": "require_more_evidence"})
        self.assertFalse(allows)


class TestGovernanceBlocksActivation(unittest.TestCase):
    """G15: governance_blocks_activation returns True for non-approve decisions."""

    def test_blocks_reject(self):
        blocks, reason = governance_blocks_activation({
            "decision": "reject_activation",
            "blocked_reasons": ["gate failed"],
        })
        self.assertTrue(blocks)
        self.assertIn("gate failed", reason)

    def test_blocks_require(self):
        blocks, reason = governance_blocks_activation({
            "decision": "require_more_evidence",
            "blocked_reasons": ["evidence_gate failed"],
        })
        self.assertTrue(blocks)

    def test_does_not_block_approve(self):
        blocks, reason = governance_blocks_activation({"decision": "approve_activation"})
        self.assertFalse(blocks)


class TestExplainBlockedReasons(unittest.TestCase):
    """G16: explain_blocked_reasons returns blocked_reasons for non-approve, empty for approve."""

    def test_approve_empty(self):
        reasons = explain_blocked_reasons({"decision": "approve_activation"})
        self.assertEqual(reasons, [])

    def test_reject_returns_reasons(self):
        reasons = explain_blocked_reasons({
            "decision": "reject_activation",
            "blocked_reasons": ["rollback_gate failed"],
        })
        self.assertEqual(len(reasons), 1)
        self.assertIn("rollback_gate failed", reasons[0])

    def test_multiple_reasons(self):
        reasons = explain_blocked_reasons({
            "decision": "reject_activation",
            "blocked_reasons": ["reason1", "reason2", "reason3"],
        })
        self.assertEqual(len(reasons), 3)


class TestFixtureLoading(unittest.TestCase):
    """G17: Fixture loading validates all governance_decision fixtures."""

    def test_all_fixtures_valid(self):
        if not FIXTURE_DIR.exists():
            self.skipTest("fixtures directory not found")
        results = load_all_fixtures(FIXTURE_DIR)
        self.assertGreater(len(results), 0)
        for name, data, violations in results:
            self.assertEqual(
                violations, [],
                f"Fixture {name} has validation violations: {violations}",
            )

    def test_fixture_count(self):
        if not FIXTURE_DIR.exists():
            self.skipTest("fixtures directory not found")
        results = load_all_fixtures(FIXTURE_DIR)
        self.assertGreaterEqual(len(results), 10)


class TestFixtureGateScenarios(unittest.TestCase):
    """G18: Fixtures cover all gate pass/fail scenarios."""

    def _load_fixture(self, name):
        path = FIXTURE_DIR / name
        return load_fixture(path)

    def test_all_gates_pass_fixture(self):
        data = self._load_fixture("valid_all_gates_pass.json")
        self.assertEqual(data["decision"], "approve_activation")
        for gate, result in data["gate_results"].items():
            self.assertEqual(result, "pass", f"{gate} should be pass")

    def test_evidence_gate_fail_fixture(self):
        data = self._load_fixture("gate_evidence_fail.json")
        self.assertEqual(data["gate_results"]["evidence_gate"], "fail")
        self.assertEqual(data["decision"], "require_more_evidence")

    def test_approval_gate_fail_fixture(self):
        data = self._load_fixture("gate_approval_fail.json")
        self.assertEqual(data["gate_results"]["approval_gate"], "fail")
        self.assertEqual(data["decision"], "reject_activation")

    def test_rollback_gate_fail_fixture(self):
        data = self._load_fixture("gate_rollback_fail.json")
        self.assertEqual(data["gate_results"]["rollback_gate"], "fail")
        self.assertEqual(data["decision"], "reject_activation")

    def test_scope_gate_fail_fixture(self):
        data = self._load_fixture("gate_scope_fail.json")
        self.assertEqual(data["gate_results"]["scope_gate"], "fail")
        self.assertEqual(data["decision"], "reject_activation")

    def test_unknown_error_gate_fail_fixture(self):
        data = self._load_fixture("gate_unknown_error_fail.json")
        self.assertEqual(data["gate_results"]["unknown_error_gate"], "fail")
        self.assertEqual(data["decision"], "require_more_evidence")


class TestCrossTrackReferences(unittest.TestCase):
    """G19: Fixtures cross-reference policy_candidate lifecycle, error_taxonomy, model_profiles."""

    def _load_fixture(self, name):
        path = FIXTURE_DIR / name
        return load_fixture(path)

    def test_policy_candidate_lifecycle_reference(self):
        data = self._load_fixture("cross_reference_policy_candidate_lifecycle.json")
        self.assertEqual(data["candidate_id"], "cand-ledger-001")
        refs = data["decision_basis"]["admitted_evidence_refs"]
        self.assertTrue(any("usage_ledger" in r for r in refs))

    def test_unknown_error_cross_reference(self):
        data = self._load_fixture("cross_reference_unknown_error.json")
        diag = data["decision_basis"]["diagnostic_evidence_refs"]
        self.assertTrue(any("unknown_error" in r for r in diag))
        self.assertEqual(data["gate_results"]["unknown_error_gate"], "fail")

    def test_shadow_routing_cross_reference(self):
        data = self._load_fixture("cross_reference_shadow_routing.json")
        self.assertEqual(data["decision"], "reject_activation")
        self.assertIn("activate_policy", data["forbidden_next_actions"])


class TestDecisionLogic(unittest.TestCase):
    """G20: decide_policy_activation decision enum matches gate results."""

    def test_all_pass_gives_approve(self):
        decision = decide_policy_activation(
            {"candidate_id": "cand-001", "policy_id": "pol-001"},
            {"admitted_evidence_refs": ["evidence:001"], "diagnostic_evidence_refs": []},
            {"decision": "approved"},
            {"status": "approved", "rollback_steps": [{"step": 1}]},
            {"policy_id": "pol-001"},
        )
        self.assertEqual(decision["decision"], "approve_activation")
        self.assertEqual(decision["allowed_next_actions"], ["activate_policy"])
        self.assertEqual(decision["forbidden_next_actions"], [])

    def test_any_fail_gives_non_approve(self):
        decision = decide_policy_activation(
            {"candidate_id": "cand-002", "policy_id": "pol-002"},
            {"admitted_evidence_refs": [], "diagnostic_evidence_refs": ["diag:001"]},
            {"decision": "rejected"},
            {"status": "proposed", "rollback_steps": []},
            {"policy_id": "pol-002"},
        )
        self.assertIn(decision["decision"], [
            "reject_activation", "require_more_evidence",
        ])
        self.assertIn("activate_policy", decision["forbidden_next_actions"])

    def test_evidence_only_fail_gives_require_more(self):
        decision = decide_policy_activation(
            {"candidate_id": "cand-003", "policy_id": "pol-003"},
            {"admitted_evidence_refs": [], "diagnostic_evidence_refs": ["diag:001"]},
            {"decision": "approved"},
            {"status": "approved", "rollback_steps": [{"step": 1}]},
            {"policy_id": "pol-003"},
        )
        self.assertEqual(decision["decision"], "require_more_evidence")
        self.assertIn("collect_admitted_evidence", decision["allowed_next_actions"])


class TestAllowedForbiddenActions(unittest.TestCase):
    """G21: allowed_next_actions and forbidden_next_actions are consistent."""

    def test_approve_forbids_nothing(self):
        decision = decide_policy_activation(
            {"candidate_id": "cand-001", "policy_id": "pol-001"},
            {"admitted_evidence_refs": ["evidence:001"], "diagnostic_evidence_refs": []},
            {"decision": "approved"},
            {"status": "approved", "rollback_steps": [{"step": 1}]},
            {"policy_id": "pol-001"},
        )
        self.assertEqual(decision["forbidden_next_actions"], [])
        self.assertIn("activate_policy", decision["allowed_next_actions"])

    def test_reject_forbids_activate(self):
        decision = decide_policy_activation(
            {"candidate_id": "cand-004", "policy_id": "pol-004"},
            {"admitted_evidence_refs": ["evidence:001"], "diagnostic_evidence_refs": []},
            {"decision": "rejected"},
            {"status": "approved", "rollback_steps": [{"step": 1}]},
            {"policy_id": "pol-004"},
        )
        self.assertIn("activate_policy", decision["forbidden_next_actions"])
        self.assertNotIn("activate_policy", decision["allowed_next_actions"])

    def test_unknown_error_allows_human_review(self):
        decision = decide_policy_activation(
            {"candidate_id": "cand-005", "policy_id": "pol-005"},
            {
                "admitted_evidence_refs": ["evidence:001"],
                "diagnostic_evidence_refs": ["diagnostic:001:unknown_error"],
                "human_review_refs": [],
            },
            {"decision": "approved"},
            {"status": "approved", "rollback_steps": [{"step": 1}]},
            {"policy_id": "pol-005"},
        )
        self.assertIn("collect_human_review", decision["allowed_next_actions"])
        self.assertIn("activate_policy", decision["forbidden_next_actions"])


class TestGateEvaluatorIsolation(unittest.TestCase):
    """Each gate evaluator returns (result, reason) tuple with correct types."""

    def test_evidence_gate_returns_tuple(self):
        result, reason = evaluate_evidence_gate({
            "admitted_evidence_refs": ["evidence:001"],
            "diagnostic_evidence_refs": [],
        })
        self.assertIsInstance(result, str)
        self.assertIsInstance(reason, str)
        self.assertIn(result, GATE_RESULTS)

    def test_approval_gate_returns_tuple(self):
        result, reason = evaluate_approval_gate({"decision": "approved"})
        self.assertIsInstance(result, str)
        self.assertIn(result, GATE_RESULTS)

    def test_rollback_gate_returns_tuple(self):
        result, reason = evaluate_rollback_gate({
            "status": "approved",
            "rollback_steps": [{"step": 1}],
        })
        self.assertIsInstance(result, str)
        self.assertIn(result, GATE_RESULTS)

    def test_scope_gate_returns_tuple(self):
        result, reason = evaluate_scope_gate(
            {"policy_id": "pol-001"},
            {"impacted_refs": []},
        )
        self.assertIsInstance(result, str)
        self.assertIn(result, GATE_RESULTS)

    def test_unknown_error_gate_returns_tuple(self):
        result, reason = evaluate_unknown_error_gate({
            "diagnostic_evidence_refs": [],
            "human_review_refs": [],
        })
        self.assertIsInstance(result, str)
        self.assertIn(result, GATE_RESULTS)


class TestMultipleGateFailures(unittest.TestCase):
    """Multiple simultaneous gate failures produce correct blocked_reasons."""

    def test_two_gates_fail_evidence_priority(self):
        decision = decide_policy_activation(
            {"candidate_id": "cand-multi", "policy_id": "pol-multi"},
            {"admitted_evidence_refs": [], "diagnostic_evidence_refs": ["diag:001"]},
            {"decision": "rejected"},
            {"status": "proposed", "rollback_steps": [{"step": 1}]},
            {"policy_id": "pol-multi"},
        )
        # evidence_gate fail takes priority -> require_more_evidence
        self.assertEqual(decision["decision"], "require_more_evidence")
        self.assertGreaterEqual(len(decision["blocked_reasons"]), 2)

    def test_three_gates_fail_evidence_priority(self):
        decision = decide_policy_activation(
            {"candidate_id": "cand-three", "policy_id": "pol-three"},
            {"admitted_evidence_refs": [], "diagnostic_evidence_refs": []},
            {"decision": "rejected"},
            {"status": "proposed", "rollback_steps": []},
            {"policy_id": "pol-three"},
        )
        # evidence_gate fail takes priority -> require_more_evidence
        self.assertEqual(decision["decision"], "require_more_evidence")
        self.assertGreaterEqual(len(decision["blocked_reasons"]), 3)


class TestGovernanceNeverModifiesRegistry(unittest.TestCase):
    """Governance decision does not directly modify policy_registry."""

    def test_decision_is_separate_from_registry(self):
        decision = decide_policy_activation(
            {"candidate_id": "cand-001", "policy_id": "pol-001"},
            {"admitted_evidence_refs": ["evidence:001"], "diagnostic_evidence_refs": []},
            {"decision": "approved"},
            {"status": "approved", "rollback_steps": [{"step": 1}]},
            {"policy_id": "pol-001"},
        )
        # Decision has decision_id, not a registry entry
        self.assertIn("decision_id", decision)
        self.assertNotIn("status", decision)
        self.assertNotIn("activated_at", decision)
        self.assertEqual(decision["schema_version"], GOVERNANCE_DECISION_VERSION)


if __name__ == "__main__":
    unittest.main()
