"""Tests for Policy Candidate Lifecycle schema, validation, and helpers."""

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.policy_candidate import (
    APPROVAL_DECISIONS,
    APPROVAL_RECORD_VERSION,
    CANDIDATE_MANIFEST_VERSION,
    CANDIDATE_TYPES,
    EVIDENCE_PACK_VERSION,
    EVIDENCE_RECOMMENDATION,
    POLICY_REGISTRY_VERSION,
    REGISTRY_STATUSES,
    ROLLBACK_PLAN_VERSION,
    ROLLBACK_SCOPES,
    ROLLBACK_STATUSES,
    approval_allows_activation,
    can_activate_policy,
    candidate_has_required_evidence,
    create_policy_registry_entry,
    evidence_pack_is_adoptable,
    load_all_fixtures,
    load_fixture,
    rollback_plan_is_ready,
    should_reject_diagnostic_only_candidate,
    validate_approval_record,
    validate_candidate_evidence_pack,
    validate_policy_candidate_manifest,
    validate_policy_registry_entry,
    validate_rollback_plan,
)

FIXTURE_DIR = Path(__file__).resolve().parents[1] / "tests" / "fixtures" / "policy_candidate"


class SchemaVersionTests(unittest.TestCase):
    def test_manifest_version(self):
        self.assertEqual(CANDIDATE_MANIFEST_VERSION, "policy_candidate.v1")

    def test_evidence_pack_version(self):
        self.assertEqual(EVIDENCE_PACK_VERSION, "candidate_evidence.v1")

    def test_approval_version(self):
        self.assertEqual(APPROVAL_RECORD_VERSION, "approval_record.v1")

    def test_rollback_version(self):
        self.assertEqual(ROLLBACK_PLAN_VERSION, "rollback_plan.v1")

    def test_registry_version(self):
        self.assertEqual(POLICY_REGISTRY_VERSION, "policy_registry.v1")


class ValidManifestTests(unittest.TestCase):
    def test_valid_manifest_passes(self):
        data = load_fixture(FIXTURE_DIR / "manifest_valid.json")
        violations = validate_policy_candidate_manifest(data)
        self.assertEqual(violations, [])

    def test_valid_manifest_has_correct_schema(self):
        data = load_fixture(FIXTURE_DIR / "manifest_valid.json")
        self.assertEqual(data["schema_version"], "policy_candidate.v1")


class ManifestMissingFieldsTests(unittest.TestCase):
    def test_missing_fields_detected(self):
        data = load_fixture(FIXTURE_DIR / "manifest_missing_fields.json")
        violations = validate_policy_candidate_manifest(data)
        self.assertTrue(len(violations) > 0)
        self.assertTrue(any("missing required field" in v for v in violations))


class InvalidCandidateTypeTests(unittest.TestCase):
    def test_invalid_type_fails(self):
        data = load_fixture(FIXTURE_DIR / "manifest_invalid_type.json")
        violations = validate_policy_candidate_manifest(data)
        self.assertTrue(any("candidate_type" in v for v in violations))

    def test_valid_types(self):
        for ct in CANDIDATE_TYPES:
            data = load_fixture(FIXTURE_DIR / "manifest_valid.json")
            data["candidate_type"] = ct
            violations = validate_policy_candidate_manifest(data)
            type_violations = [v for v in violations if "candidate_type" in v]
            self.assertEqual(type_violations, [], f"candidate_type={ct} should be valid")


class ApprovalRequiredTests(unittest.TestCase):
    def test_approval_required_false_fails(self):
        data = load_fixture(FIXTURE_DIR / "manifest_no_approval.json")
        violations = validate_policy_candidate_manifest(data)
        self.assertTrue(any("approval_required" in v for v in violations))


class EvidencePackTests(unittest.TestCase):
    def test_valid_evidence_pack_passes(self):
        data = load_fixture(FIXTURE_DIR / "evidence_pack_valid.json")
        violations = validate_candidate_evidence_pack(data)
        self.assertEqual(violations, [])

    def test_diagnostic_only_accept_fails(self):
        data = load_fixture(FIXTURE_DIR / "evidence_diagnostic_only.json")
        violations = validate_candidate_evidence_pack(data)
        # Schema validation passes, but lifecycle check rejects
        ok, reason = evidence_pack_is_adoptable(data)
        self.assertFalse(ok)
        self.assertIn("diagnostic", reason)

    def test_admitted_accept_passes(self):
        data = load_fixture(FIXTURE_DIR / "evidence_admitted.json")
        violations = validate_candidate_evidence_pack(data)
        self.assertEqual(violations, [])
        ok, reason = evidence_pack_is_adoptable(data)
        self.assertTrue(ok)

    def test_diagnostic_only_reject_recommended(self):
        data = load_fixture(FIXTURE_DIR / "evidence_diagnostic_only.json")
        data["recommendation"] = "reject"
        ok, reason = evidence_pack_is_adoptable(data)
        self.assertFalse(ok)


class ApprovalRecordTests(unittest.TestCase):
    def test_valid_approval_passes(self):
        data = load_fixture(FIXTURE_DIR / "approval_valid.json")
        violations = validate_approval_record(data)
        self.assertEqual(violations, [])

    def test_rejected_approval(self):
        data = load_fixture(FIXTURE_DIR / "approval_rejected.json")
        violations = validate_approval_record(data)
        self.assertEqual(violations, [])
        ok, reason = approval_allows_activation(data)
        self.assertFalse(ok)
        self.assertIn("rejected", reason)

    def test_deferred_approval(self):
        data = load_fixture(FIXTURE_DIR / "approval_deferred.json")
        violations = validate_approval_record(data)
        self.assertEqual(violations, [])
        ok, reason = approval_allows_activation(data)
        self.assertFalse(ok)
        self.assertIn("deferred", reason)

    def test_approved_approval(self):
        data = load_fixture(FIXTURE_DIR / "approval_valid.json")
        ok, reason = approval_allows_activation(data)
        self.assertTrue(ok)


class RollbackPlanTests(unittest.TestCase):
    def test_valid_rollback_passes(self):
        data = load_fixture(FIXTURE_DIR / "rollback_plan_valid.json")
        violations = validate_rollback_plan(data)
        self.assertEqual(violations, [])

    def test_no_steps_fails(self):
        data = load_fixture(FIXTURE_DIR / "rollback_no_steps.json")
        violations = validate_rollback_plan(data)
        self.assertTrue(any("rollback_steps" in v for v in violations))

    def test_user_project_path_fails(self):
        data = load_fixture(FIXTURE_DIR / "rollback_user_project.json")
        violations = validate_rollback_plan(data)
        self.assertTrue(any("user project" in v for v in violations))

    def test_rollback_plan_ready(self):
        data = load_fixture(FIXTURE_DIR / "rollback_plan_valid.json")
        ok, reason = rollback_plan_is_ready(data)
        self.assertTrue(ok)

    def test_rollback_plan_not_ready_no_steps(self):
        data = load_fixture(FIXTURE_DIR / "rollback_no_steps.json")
        ok, reason = rollback_plan_is_ready(data)
        self.assertFalse(ok)

    def test_rollback_plan_failed_status(self):
        data = load_fixture(FIXTURE_DIR / "rollback_plan_valid.json")
        data["status"] = "failed"
        ok, reason = rollback_plan_is_ready(data)
        self.assertFalse(ok)


class RegistryEntryTests(unittest.TestCase):
    def test_valid_registry_passes(self):
        data = load_fixture(FIXTURE_DIR / "registry_valid.json")
        violations = validate_policy_registry_entry(data)
        self.assertEqual(violations, [])

    def test_active_missing_approval_fails(self):
        data = load_fixture(FIXTURE_DIR / "registry_active_no_approval.json")
        violations = validate_policy_registry_entry(data)
        self.assertTrue(any("approval_ref" in v for v in violations))

    def test_active_missing_rollback_fails(self):
        data = load_fixture(FIXTURE_DIR / "registry_active_no_rollback.json")
        violations = validate_policy_registry_entry(data)
        self.assertTrue(any("rollback_plan_ref" in v for v in violations))

    def test_proposed_can_lack_refs(self):
        data = load_fixture(FIXTURE_DIR / "registry_proposed.json")
        violations = validate_policy_registry_entry(data)
        self.assertEqual(violations, [])


class LifecycleTests(unittest.TestCase):
    def test_candidate_has_required_evidence_with_admitted(self):
        manifest = {"candidate_id": "c1"}
        evidence = {"admitted_evidence_refs": ["ref1"]}
        ok, reason = candidate_has_required_evidence(manifest, evidence)
        self.assertTrue(ok)

    def test_candidate_no_admitted_evidence(self):
        manifest = {"candidate_id": "c1"}
        evidence = {"admitted_evidence_refs": []}
        ok, reason = candidate_has_required_evidence(manifest, evidence)
        self.assertFalse(ok)
        self.assertIn("admitted", reason)

    def test_can_activate_with_all_refs(self):
        registry = load_fixture(FIXTURE_DIR / "registry_valid.json")
        ok, reason = can_activate_policy(registry)
        self.assertTrue(ok)

    def test_can_activate_proposed_without_refs(self):
        registry = load_fixture(FIXTURE_DIR / "registry_proposed.json")
        ok, reason = can_activate_policy(registry)
        self.assertTrue(ok)

    def test_rejected_approval_blocks_activation(self):
        registry = {"status": "proposed", "approval_ref": "", "rollback_plan_ref": ""}
        approval = load_fixture(FIXTURE_DIR / "approval_rejected.json")
        ok, reason = can_activate_policy(registry, approval=approval)
        self.assertFalse(ok)

    def test_deferred_approval_blocks_activation(self):
        registry = {"status": "proposed", "approval_ref": "", "rollback_plan_ref": ""}
        approval = load_fixture(FIXTURE_DIR / "approval_deferred.json")
        ok, reason = can_activate_policy(registry, approval=approval)
        self.assertFalse(ok)

    def test_approved_without_rollback_blocks_activation(self):
        registry = {"status": "proposed", "approval_ref": "", "rollback_plan_ref": ""}
        approval = load_fixture(FIXTURE_DIR / "approval_valid.json")
        rollback = load_fixture(FIXTURE_DIR / "rollback_no_steps.json")
        ok, reason = can_activate_policy(registry, approval=approval, rollback=rollback)
        self.assertFalse(ok)
        self.assertIn("rollback", reason.lower())

    def test_diagnostic_only_rejected(self):
        evidence = load_fixture(FIXTURE_DIR / "evidence_diagnostic_only.json")
        ok, reason = should_reject_diagnostic_only_candidate(evidence)
        self.assertTrue(ok)
        self.assertIn("diagnostic", reason)

    def test_admitted_evidence_not_rejected(self):
        evidence = load_fixture(FIXTURE_DIR / "evidence_admitted.json")
        ok, reason = should_reject_diagnostic_only_candidate(evidence)
        self.assertFalse(ok)


class HappyPathLifecycleTests(unittest.TestCase):
    def test_full_lifecycle(self):
        lifecycle = load_fixture(FIXTURE_DIR / "lifecycle_happy_path.json")

        # 1. Manifest validates
        manifest_v = validate_policy_candidate_manifest(lifecycle["manifest"])
        self.assertEqual(manifest_v, [])

        # 2. Evidence pack validates and is adoptable
        evidence_v = validate_candidate_evidence_pack(lifecycle["evidence_pack"])
        self.assertEqual(evidence_v, [])
        ok, _ = evidence_pack_is_adoptable(lifecycle["evidence_pack"])
        self.assertTrue(ok)

        # 3. Approval validates and allows activation
        approval_v = validate_approval_record(lifecycle["approval"])
        self.assertEqual(approval_v, [])
        ok, _ = approval_allows_activation(lifecycle["approval"])
        self.assertTrue(ok)

        # 4. Rollback plan validates and is ready
        rollback_v = validate_rollback_plan(lifecycle["rollback_plan"])
        self.assertEqual(rollback_v, [])
        ok, _ = rollback_plan_is_ready(lifecycle["rollback_plan"])
        self.assertTrue(ok)

        # 5. Registry validates
        registry_v = validate_policy_registry_entry(lifecycle["registry"])
        self.assertEqual(registry_v, [])


class ShadowRoutingEvidenceTests(unittest.TestCase):
    def test_shadow_routing_only_diagnostic(self):
        data = load_fixture(FIXTURE_DIR / "evidence_shadow_routing.json")
        violations = validate_candidate_evidence_pack(data)
        self.assertEqual(violations, [])
        # Shadow routing evidence should be diagnostic only
        self.assertEqual(len(data["admitted_evidence_refs"]), 0)
        self.assertGreater(len(data["diagnostic_evidence_refs"]), 0)


class UnknownErrorEvidenceTests(unittest.TestCase):
    def test_unknown_error_forces_human_review(self):
        data = load_fixture(FIXTURE_DIR / "evidence_unknown_error.json")
        violations = validate_candidate_evidence_pack(data)
        self.assertEqual(violations, [])
        # Unknown error evidence should require human review
        self.assertGreater(len(data["human_review_refs"]), 0)
        ok, reason = should_reject_diagnostic_only_candidate(data)
        self.assertTrue(ok)


class CrossReferenceTests(unittest.TestCase):
    def test_manifest_references_usage_ledger(self):
        data = load_fixture(FIXTURE_DIR / "manifest_with_ledger_ref.json")
        violations = validate_policy_candidate_manifest(data)
        self.assertEqual(violations, [])
        self.assertTrue(any("usage_ledger" in r for r in data["source_refs"]))

    def test_manifest_references_shadow_routing(self):
        data = load_fixture(FIXTURE_DIR / "manifest_with_shadow_ref.json")
        violations = validate_policy_candidate_manifest(data)
        self.assertEqual(violations, [])
        self.assertTrue(any("shadow_routing" in r for r in data["source_refs"]))

    def test_manifest_references_context_pack(self):
        data = load_fixture(FIXTURE_DIR / "manifest_with_cp_ref.json")
        violations = validate_policy_candidate_manifest(data)
        self.assertEqual(violations, [])
        self.assertTrue(any("context_pack" in r for r in data["source_refs"]))


class EnumCompletenessTests(unittest.TestCase):
    def test_candidate_types(self):
        self.assertEqual(
            set(CANDIDATE_TYPES),
            {"context_pack", "tool_contract", "routing_rule", "skill_package",
             "eval_gate", "error_taxonomy", "model_profile"},
        )

    def test_evidence_recommendation(self):
        self.assertEqual(
            set(EVIDENCE_RECOMMENDATION),
            {"accept", "reject", "revise", "needs_more_evidence"},
        )

    def test_approval_decisions(self):
        self.assertEqual(set(APPROVAL_DECISIONS), {"approved", "rejected", "deferred"})

    def test_rollback_scopes(self):
        self.assertEqual(
            set(ROLLBACK_SCOPES),
            {"docs_only", "config", "schema", "profile", "skill", "eval_gate", "runtime_guard"},
        )

    def test_registry_statuses(self):
        self.assertEqual(
            set(REGISTRY_STATUSES),
            {"proposed", "approved", "active", "rolled_back", "retired"},
        )

    def test_rollback_statuses(self):
        self.assertEqual(
            set(ROLLBACK_STATUSES),
            {"proposed", "approved", "executed", "failed", "obsolete"},
        )


class FixtureLoadingTests(unittest.TestCase):
    def test_fixture_dir_exists(self):
        self.assertTrue(FIXTURE_DIR.exists())

    def test_all_fixtures_load(self):
        results = load_all_fixtures(FIXTURE_DIR)
        self.assertGreater(len(results), 0)

    def test_valid_fixtures_all_pass(self):
        valid_names = [
            "manifest_valid.json",
            "evidence_pack_valid.json",
            "approval_valid.json",
            "rollback_plan_valid.json",
            "registry_valid.json",
            "evidence_admitted.json",
            "registry_proposed.json",
        ]
        for fname in valid_names:
            _, violations = _load_and_validate(FIXTURE_DIR / fname)
            self.assertEqual(violations, [], f"Fixture {fname} has violations: {violations}")

    def test_invalid_fixtures_detected(self):
        invalid_names = [
            "manifest_missing_fields.json",
            "manifest_invalid_type.json",
            "manifest_no_approval.json",
            "rollback_no_steps.json",
            "rollback_user_project.json",
            "registry_active_no_approval.json",
            "registry_active_no_rollback.json",
        ]
        for fname in invalid_names:
            _, violations = _load_and_validate(FIXTURE_DIR / fname)
            self.assertTrue(len(violations) > 0, f"Fixture {fname} should have violations")


class ExistingTestsUnchangedTests(unittest.TestCase):
    def test_existing_test_files_still_exist(self):
        base = Path(__file__).resolve().parents[1] / "tests"
        self.assertTrue((base / "test_real_world_eval.py").exists())
        self.assertTrue((base / "test_error_taxonomy.py").exists())
        self.assertTrue((base / "test_user_style_mutation_eval.py").exists())
        self.assertTrue((base / "test_context_pack.py").exists())
        self.assertTrue((base / "test_usage_ledger.py").exists())
        self.assertTrue((base / "test_model_profiles.py").exists())


class CreateRegistryEntryTests(unittest.TestCase):
    def test_create_entry(self):
        entry = create_policy_registry_entry(
            policy_id="pol-new-001",
            candidate_id="cand-001",
            policy_type="context_pack",
        )
        self.assertEqual(entry["status"], "proposed")
        self.assertEqual(entry["schema_version"], POLICY_REGISTRY_VERSION)


def _load_and_validate(path: Path):
    data = load_fixture(path)
    sv = data.get("schema_version", "")
    if sv == CANDIDATE_MANIFEST_VERSION:
        return data, validate_policy_candidate_manifest(data)
    elif sv == EVIDENCE_PACK_VERSION:
        return data, validate_candidate_evidence_pack(data)
    elif sv == APPROVAL_RECORD_VERSION:
        return data, validate_approval_record(data)
    elif sv == ROLLBACK_PLAN_VERSION:
        return data, validate_rollback_plan(data)
    elif sv == POLICY_REGISTRY_VERSION:
        return data, validate_policy_registry_entry(data)
    return data, [f"unknown schema: {sv}"]


if __name__ == "__main__":
    unittest.main()
