"""Tests for Model Harness Profile and Shadow Routing schema, validation."""

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.model_profiles import (
    CACHE_STRATEGY,
    ENFORCEMENT_SCOPES,
    FALLBACK_POLICY,
    JSON_TOLERANCE,
    MODEL_PROFILE_SCHEMA_VERSION,
    PARALLEL_TOOL_PREFERENCE,
    REASONING_EFFORT,
    RECOMMENDATION_VALUES,
    RISK_LEVELS,
    SHADOW_ROUTING_SCHEMA_VERSION,
    TIERS,
    TOOL_STRICTNESS,
    CostMetadata,
    ForbiddenPreviousTool,
    ModelHarnessProfile,
    ShadowRoutingRecommendation,
    can_compare_with_usage_ledger,
    is_shadow_only,
    load_all_fixtures,
    load_fixture,
    load_and_validate_profile,
    load_and_validate_shadow,
    validate_model_harness_profile,
    validate_shadow_routing_recommendation,
)

FIXTURE_DIR = Path(__file__).resolve().parents[1] / "tests" / "fixtures" / "model_profiles"


class SchemaVersionTests(unittest.TestCase):
    def test_profile_schema_version(self):
        self.assertEqual(MODEL_PROFILE_SCHEMA_VERSION, "model_harness_profile.v1")

    def test_shadow_schema_version(self):
        self.assertEqual(SHADOW_ROUTING_SCHEMA_VERSION, "shadow_routing_recommendation.v1")


class ValidProfileTests(unittest.TestCase):
    def test_valid_profile_passes(self):
        data = load_fixture(FIXTURE_DIR / "valid_profile.json")
        violations = validate_model_harness_profile(data)
        self.assertEqual(violations, [])

    def test_valid_profile_has_correct_schema(self):
        data = load_fixture(FIXTURE_DIR / "valid_profile.json")
        self.assertEqual(data["schema_version"], "model_harness_profile.v1")


class MissingFieldsTests(unittest.TestCase):
    def test_missing_fields_detected(self):
        data = load_fixture(FIXTURE_DIR / "missing_fields.json")
        violations = validate_model_harness_profile(data)
        self.assertTrue(len(violations) > 0)
        self.assertTrue(any("missing required field" in v for v in violations))


class InvalidEnumTests(unittest.TestCase):
    def test_invalid_tier_fails(self):
        data = load_fixture(FIXTURE_DIR / "invalid_tier.json")
        violations = validate_model_harness_profile(data)
        self.assertTrue(any("tier" in v for v in violations))

    def test_valid_tiers(self):
        for tier in TIERS:
            data = load_fixture(FIXTURE_DIR / "valid_profile.json")
            data["tier"] = tier
            violations = validate_model_harness_profile(data)
            tier_violations = [v for v in violations if "tier" in v]
            self.assertEqual(tier_violations, [], f"tier={tier} should be valid")


class ContextWindowTests(unittest.TestCase):
    def test_zero_context_window_fails(self):
        data = load_fixture(FIXTURE_DIR / "zero_context_window.json")
        violations = validate_model_harness_profile(data)
        self.assertTrue(any("context_window" in v for v in violations))

    def test_negative_context_window_fails(self):
        data = load_fixture(FIXTURE_DIR / "valid_profile.json")
        data["context_window"] = -100
        violations = validate_model_harness_profile(data)
        self.assertTrue(any("context_window" in v for v in violations))

    def test_positive_context_window_passes(self):
        data = load_fixture(FIXTURE_DIR / "valid_profile.json")
        self.assertGreater(data["context_window"], 0)


class CostMetadataTests(unittest.TestCase):
    def test_negative_cost_fails(self):
        data = load_fixture(FIXTURE_DIR / "negative_cost.json")
        violations = validate_model_harness_profile(data)
        self.assertTrue(any("cost_metadata" in v and "non-negative" in v for v in violations))

    def test_zero_cost_passes(self):
        data = load_fixture(FIXTURE_DIR / "valid_profile.json")
        data["cost_metadata"]["input_cost_per_1k"] = 0.0
        data["cost_metadata"]["output_cost_per_1k"] = 0.0
        violations = validate_model_harness_profile(data)
        cost_violations = [v for v in violations if "cost_metadata" in v]
        self.assertEqual(cost_violations, [])


class ToolConflictTests(unittest.TestCase):
    def test_allowed_forbidden_conflict_fails(self):
        data = load_fixture(FIXTURE_DIR / "tool_conflict.json")
        violations = validate_model_harness_profile(data)
        self.assertTrue(any("conflict" in v for v in violations))


class ForbiddenPreviousToolsTests(unittest.TestCase):
    def test_missing_reason_fails(self):
        data = load_fixture(FIXTURE_DIR / "forbidden_no_reason.json")
        violations = validate_model_harness_profile(data)
        self.assertTrue(any("reason" in v for v in violations))

    def test_valid_forbidden_tool_passes(self):
        data = load_fixture(FIXTURE_DIR / "valid_profile.json")
        violations = validate_model_harness_profile(data)
        fpt_violations = [v for v in violations if "forbidden_previous_tools" in v]
        self.assertEqual(fpt_violations, [])


class CredentialDetectionTests(unittest.TestCase):
    def test_api_key_detected(self):
        data = load_fixture(FIXTURE_DIR / "with_credentials.json")
        violations = validate_model_harness_profile(data)
        self.assertTrue(any("credential" in v.lower() or "api_key" in v for v in violations))

    def test_no_credentials_in_valid_profile(self):
        data = load_fixture(FIXTURE_DIR / "valid_profile.json")
        violations = validate_model_harness_profile(data)
        cred_violations = [v for v in violations if "credential" in v.lower()]
        self.assertEqual(cred_violations, [])


class ShadowRoutingValidationTests(unittest.TestCase):
    def test_valid_shadow_passes(self):
        data = load_fixture(FIXTURE_DIR / "shadow_valid.json")
        violations = validate_shadow_routing_recommendation(data)
        self.assertEqual(violations, [])

    def test_wrong_admission_scope_fails(self):
        data = load_fixture(FIXTURE_DIR / "shadow_wrong_admission.json")
        violations = validate_shadow_routing_recommendation(data)
        self.assertTrue(any("admission_scope" in v for v in violations))

    def test_active_routing_allowed_true_fails(self):
        data = load_fixture(FIXTURE_DIR / "shadow_active_routing.json")
        violations = validate_shadow_routing_recommendation(data)
        self.assertTrue(any("active_routing_allowed" in v for v in violations))

    def test_shadow_is_diagnostic_only(self):
        data = load_fixture(FIXTURE_DIR / "shadow_valid.json")
        self.assertTrue(is_shadow_only(data))

    def test_shadow_not_diagnostic(self):
        data = load_fixture(FIXTURE_DIR / "shadow_active_routing.json")
        self.assertFalse(is_shadow_only(data))


class ShadowWithLedgerRefTests(unittest.TestCase):
    def test_shadow_with_ledger_ref_validates(self):
        data = load_fixture(FIXTURE_DIR / "shadow_with_ledger_ref.json")
        violations = validate_shadow_routing_recommendation(data)
        self.assertEqual(violations, [])
        self.assertTrue(any("usage_ledger" in r for r in data["evidence_refs"]))


class ShadowWithCpRefTests(unittest.TestCase):
    def test_shadow_with_cp_ref_validates(self):
        data = load_fixture(FIXTURE_DIR / "shadow_with_cp_ref.json")
        violations = validate_shadow_routing_recommendation(data)
        self.assertEqual(violations, [])
        self.assertTrue(any("context_pack" in r for r in data["evidence_refs"]))


class CompareWithUsageLedgerTests(unittest.TestCase):
    def test_matching_group_passes(self):
        rec = load_fixture(FIXTURE_DIR / "shadow_valid.json")
        ok, reason = can_compare_with_usage_ledger(rec, "real_world_eval/bugfix/formal_issue/passes_final_gate")
        self.assertTrue(ok)

    def test_non_matching_group_fails(self):
        rec = load_fixture(FIXTURE_DIR / "shadow_valid.json")
        ok, reason = can_compare_with_usage_ledger(rec, "other_suite/docs/update/some_criterion")
        self.assertFalse(ok)


class EnumCompletenessTests(unittest.TestCase):
    def test_tiers(self):
        self.assertEqual(set(TIERS), {"cheap_executor", "balanced_worker", "strong_planner", "verifier", "advisor"})

    def test_tool_strictness(self):
        self.assertEqual(set(TOOL_STRICTNESS), {"strict", "tolerant", "unsupported"})

    def test_json_tolerance(self):
        self.assertEqual(set(JSON_TOLERANCE), {"strict_json", "tolerant_json", "text_only"})

    def test_reasoning_effort(self):
        self.assertEqual(set(REASONING_EFFORT), {"low", "medium", "high"})

    def test_parallel_tool_preference(self):
        self.assertEqual(set(PARALLEL_TOOL_PREFERENCE), {"none", "allowed", "preferred", "forbidden"})

    def test_cache_strategy(self):
        self.assertEqual(set(CACHE_STRATEGY), {"no_cache", "read_cache", "write_cache", "read_write_cache"})

    def test_fallback_policy(self):
        self.assertEqual(set(FALLBACK_POLICY), {"no_fallback", "same_tier_only", "lower_cost_allowed", "higher_quality_allowed", "human_required"})

    def test_enforcement_scopes(self):
        self.assertEqual(set(ENFORCEMENT_SCOPES), {"prompt_assembly", "gateway_validation", "context_broker", "all"})

    def test_recommendation_values(self):
        self.assertEqual(set(RECOMMENDATION_VALUES), {"keep_baseline", "try_candidate", "reject_candidate", "needs_more_evidence"})

    def test_risk_levels(self):
        self.assertEqual(set(RISK_LEVELS), {"low", "medium", "high", "critical"})


class FixtureLoadingTests(unittest.TestCase):
    def test_fixture_dir_exists(self):
        self.assertTrue(FIXTURE_DIR.exists())

    def test_all_fixtures_load(self):
        results = load_all_fixtures(FIXTURE_DIR)
        self.assertGreater(len(results), 0)

    def test_valid_fixtures_all_pass(self):
        valid_profiles = [
            "valid_profile.json",
        ]
        for fname in valid_profiles:
            _, violations = load_and_validate_profile(FIXTURE_DIR / fname)
            self.assertEqual(violations, [], f"Fixture {fname} has violations: {violations}")

    def test_valid_shadow_fixtures_pass(self):
        valid_shadows = [
            "shadow_valid.json",
            "shadow_with_ledger_ref.json",
            "shadow_with_cp_ref.json",
        ]
        for fname in valid_shadows:
            _, violations = load_and_validate_shadow(FIXTURE_DIR / fname)
            self.assertEqual(violations, [], f"Fixture {fname} has violations: {violations}")

    def test_invalid_fixtures_detected(self):
        invalid_profiles = [
            "missing_fields.json",
            "invalid_tier.json",
            "zero_context_window.json",
            "negative_cost.json",
            "tool_conflict.json",
            "forbidden_no_reason.json",
            "with_credentials.json",
        ]
        for fname in invalid_profiles:
            _, violations = load_and_validate_profile(FIXTURE_DIR / fname)
            self.assertTrue(len(violations) > 0, f"Fixture {fname} should have violations")

    def test_invalid_shadow_fixtures_detected(self):
        invalid_shadows = [
            "shadow_wrong_admission.json",
            "shadow_active_routing.json",
        ]
        for fname in invalid_shadows:
            _, violations = load_and_validate_shadow(FIXTURE_DIR / fname)
            self.assertTrue(len(violations) > 0, f"Fixture {fname} should have violations")


class ExistingTestsUnchangedTests(unittest.TestCase):
    def test_existing_test_files_still_exist(self):
        base = Path(__file__).resolve().parents[1] / "tests"
        self.assertTrue((base / "test_real_world_eval.py").exists())
        self.assertTrue((base / "test_error_taxonomy.py").exists())
        self.assertTrue((base / "test_user_style_mutation_eval.py").exists())
        self.assertTrue((base / "test_context_pack.py").exists())
        self.assertTrue((base / "test_usage_ledger.py").exists())


class DataclassTests(unittest.TestCase):
    def test_cost_metadata_to_dict(self):
        cm = CostMetadata(input_cost_per_1k=0.015, output_cost_per_1k=0.075)
        d = cm.to_dict()
        self.assertEqual(d["input_cost_per_1k"], 0.015)
        self.assertNotIn("cache_read_cost_per_1k", d)

    def test_forbidden_previous_tool_to_dict(self):
        fpt = ForbiddenPreviousTool(tool_id="web_search", reason="not available")
        d = fpt.to_dict()
        self.assertEqual(d["tool_id"], "web_search")
        self.assertEqual(d["reason"], "not available")

    def test_model_harness_profile_to_dict(self):
        profile = ModelHarnessProfile(
            profile_id="p1", provider="anthropic", model_id="m1",
            tier="strong_planner", tool_strictness="strict",
            json_tolerance="strict_json", reasoning_effort="high",
            output_format_expectation="json", parallel_tool_preference="allowed",
            escaping_quirks="none", cache_strategy="no_cache",
            fallback_policy="no_fallback", context_window=100000,
            cost_metadata=CostMetadata(), allowed_tools=[],
            forbidden_previous_tools=[],
        )
        d = profile.to_dict()
        self.assertEqual(d["profile_id"], "p1")
        self.assertIsInstance(d["cost_metadata"], dict)

    def test_shadow_recommendation_to_dict(self):
        rec = ShadowRoutingRecommendation(
            recommendation_id="r1", task_family="bugfix",
            variant_family="formal", success_criterion="pass",
            candidate_profile_id="p1", baseline_profile_id="p2",
            rationale="test", evidence_refs=[], expected_quality_delta=0.1,
            expected_cost_delta=0.02, risk_level="low",
            recommendation="try_candidate",
        )
        d = rec.to_dict()
        self.assertEqual(d["recommendation_id"], "r1")
        self.assertFalse(d["active_routing_allowed"])
        self.assertEqual(d["admission_scope"], "diagnostic")


if __name__ == "__main__":
    unittest.main()
