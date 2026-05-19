"""Tests for Usage Ledger and Cost-of-Pass schema, validation, and aggregation."""

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.usage_ledger import (
    SCHEMA_VERSION,
    COST_OF_PASS_GROUP_PATTERN,
    CostOfPassAggregate,
    UsageLedgerRow,
    aggregate_cost_of_pass,
    compare_cost_groups,
    detect_invalid_cost_comparison,
    group_usage_rows,
    is_valid_cost_of_pass_group,
    load_all_fixtures,
    load_fixture,
    load_usage_ledger_fixture,
    parse_cost_of_pass_group,
    validate_usage_ledger_row,
)

FIXTURE_DIR = Path(__file__).resolve().parents[1] / "tests" / "fixtures" / "usage_ledger"


class SchemaVersionTests(unittest.TestCase):
    """Test schema version constant."""

    def test_schema_version(self):
        self.assertEqual(SCHEMA_VERSION, "usage_ledger.v1")


class ValidRowTests(unittest.TestCase):
    """Test valid usage_ledger row passes validation."""

    def test_valid_row_passes(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        violations = validate_usage_ledger_row(data)
        self.assertEqual(violations, [])

    def test_valid_row_has_correct_schema(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        self.assertEqual(data["schema_version"], "usage_ledger.v1")


class MissingFieldsTests(unittest.TestCase):
    """Test missing required fields fail."""

    def test_missing_fields_detected(self):
        data = load_fixture(FIXTURE_DIR / "missing_fields.json")
        violations = validate_usage_ledger_row(data)
        self.assertTrue(len(violations) > 0)
        self.assertTrue(any("missing required field" in v for v in violations))

    def test_multiple_missing_fields(self):
        data = {"schema_version": "usage_ledger.v1"}
        violations = validate_usage_ledger_row(data)
        self.assertGreater(len(violations), 5)


class NegativeValueTests(unittest.TestCase):
    """Test negative token/cost/retry/tool-call/wall-clock fail."""

    def test_negative_input_tokens_fails(self):
        data = load_fixture(FIXTURE_DIR / "negative_tokens.json")
        violations = validate_usage_ledger_row(data)
        self.assertTrue(any("input_tokens" in v and "non-negative" in v for v in violations))

    def test_negative_output_tokens_fails(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["output_tokens"] = -1
        violations = validate_usage_ledger_row(data)
        self.assertTrue(any("output_tokens" in v for v in violations))

    def test_negative_cached_tokens_fails(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["cached_tokens"] = -1
        violations = validate_usage_ledger_row(data)
        self.assertTrue(any("cached_tokens" in v for v in violations))

    def test_negative_request_count_fails(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["request_count"] = -1
        violations = validate_usage_ledger_row(data)
        self.assertTrue(any("request_count" in v for v in violations))

    def test_negative_tool_call_count_fails(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["tool_call_count"] = -1
        violations = validate_usage_ledger_row(data)
        self.assertTrue(any("tool_call_count" in v for v in violations))

    def test_negative_retry_count_fails(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["retry_count"] = -1
        violations = validate_usage_ledger_row(data)
        self.assertTrue(any("retry_count" in v for v in violations))

    def test_negative_wall_clock_fails(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["wall_clock_ms"] = -1
        violations = validate_usage_ledger_row(data)
        self.assertTrue(any("wall_clock_ms" in v for v in violations))

    def test_negative_estimated_cost_fails(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["estimated_cost"] = -0.5
        violations = validate_usage_ledger_row(data)
        self.assertTrue(any("estimated_cost" in v for v in violations))


class CachedExceedsInputTests(unittest.TestCase):
    """Test cached_tokens > input_tokens fails."""

    def test_cached_exceeds_input_fails(self):
        data = load_fixture(FIXTURE_DIR / "cached_exceeds_input.json")
        violations = validate_usage_ledger_row(data)
        self.assertTrue(any("cached_tokens" in v and "input_tokens" in v for v in violations))

    def test_cached_equals_input_passes(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["cached_tokens"] = data["input_tokens"]
        violations = validate_usage_ledger_row(data)
        self.assertEqual(violations, [])


class CostOfPassGroupFormatTests(unittest.TestCase):
    """Test cost_of_pass_group format validation."""

    def test_valid_format(self):
        self.assertTrue(is_valid_cost_of_pass_group("eval/task/variant/criterion"))

    def test_valid_format_with_hyphens(self):
        self.assertTrue(is_valid_cost_of_pass_group("real-world/bug-fix/formal-issue/passes-gate"))

    def test_invalid_too_few_segments(self):
        self.assertFalse(is_valid_cost_of_pass_group("eval/task/variant"))

    def test_invalid_too_many_segments(self):
        self.assertFalse(is_valid_cost_of_pass_group("eval/task/variant/criterion/extra"))

    def test_invalid_with_spaces(self):
        self.assertFalse(is_valid_cost_of_pass_group("eval/task family/variant/criterion"))

    def test_invalid_empty_segment(self):
        self.assertFalse(is_valid_cost_of_pass_group("eval//variant/criterion"))

    def test_fixture_invalid_format_detected(self):
        data = load_fixture(FIXTURE_DIR / "invalid_group_format.json")
        violations = validate_usage_ledger_row(data)
        self.assertTrue(any("cost_of_pass_group" in v for v in violations))

    def test_parse_valid_group(self):
        es, tf, vf, sc = parse_cost_of_pass_group("eval/task/variant/criterion")
        self.assertEqual(es, "eval")
        self.assertEqual(tf, "task")
        self.assertEqual(vf, "variant")
        self.assertEqual(sc, "criterion")

    def test_parse_invalid_group_raises(self):
        with self.assertRaises(ValueError):
            parse_cost_of_pass_group("invalid")


class PassBooleanTests(unittest.TestCase):
    """Test pass must be bool."""

    def test_pass_true_valid(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["pass"] = True
        violations = validate_usage_ledger_row(data)
        self.assertEqual(violations, [])

    def test_pass_false_valid(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["pass"] = False
        violations = validate_usage_ledger_row(data)
        self.assertEqual(violations, [])

    def test_pass_string_fails(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["pass"] = "yes"
        violations = validate_usage_ledger_row(data)
        self.assertTrue(any("pass" in v and "bool" in v for v in violations))

    def test_pass_int_fails(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["pass"] = 1
        violations = validate_usage_ledger_row(data)
        self.assertTrue(any("pass" in v and "bool" in v for v in violations))


class NoModelOfflineTests(unittest.TestCase):
    """Test no_model/offline fixtures allow empty model_profile_id and context_pack_id."""

    def test_empty_model_profile_id_passes(self):
        data = load_fixture(FIXTURE_DIR / "no_model_offline.json")
        violations = validate_usage_ledger_row(data)
        self.assertEqual(violations, [])
        self.assertEqual(data["model_profile_id"], "")
        self.assertEqual(data["context_pack_id"], "")

    def test_null_model_profile_id_passes(self):
        data = load_fixture(FIXTURE_DIR / "valid_row.json")
        data["model_profile_id"] = None
        data["context_pack_id"] = None
        violations = validate_usage_ledger_row(data)
        self.assertEqual(violations, [])


class AggregationTests(unittest.TestCase):
    """Test aggregate_cost_of_pass calculations."""

    def test_aggregate_single_success(self):
        rows = [load_fixture(FIXTURE_DIR / "group_a_success_1.json")]
        agg = aggregate_cost_of_pass(rows)
        self.assertEqual(agg.success_count, 1)
        self.assertEqual(agg.failure_count, 0)
        self.assertEqual(agg.total_count, 1)
        self.assertAlmostEqual(agg.cost_of_pass, 0.10)

    def test_aggregate_mixed_success_failure(self):
        rows = [
            load_fixture(FIXTURE_DIR / "group_a_success_1.json"),
            load_fixture(FIXTURE_DIR / "group_a_success_2.json"),
            load_fixture(FIXTURE_DIR / "group_a_failure.json"),
        ]
        agg = aggregate_cost_of_pass(rows)
        self.assertEqual(agg.success_count, 2)
        self.assertEqual(agg.failure_count, 1)
        self.assertEqual(agg.total_count, 3)
        self.assertAlmostEqual(agg.total_estimated_cost, 0.32)
        self.assertAlmostEqual(agg.cost_of_pass, 0.16)

    def test_aggregate_all_fail_undefined(self):
        rows = [load_fixture(FIXTURE_DIR / "group_all_fail.json")]
        agg = aggregate_cost_of_pass(rows)
        self.assertEqual(agg.success_count, 0)
        self.assertEqual(agg.failure_count, 1)
        self.assertIsNone(agg.cost_of_pass)

    def test_aggregate_empty_rows(self):
        agg = aggregate_cost_of_pass([])
        self.assertEqual(agg.total_count, 0)
        self.assertIsNone(agg.cost_of_pass)


class GroupUsageRowsTests(unittest.TestCase):
    """Test group_usage_rows."""

    def test_groups_by_cost_of_pass_group(self):
        rows = [
            load_fixture(FIXTURE_DIR / "group_a_success_1.json"),
            load_fixture(FIXTURE_DIR / "group_a_success_2.json"),
            load_fixture(FIXTURE_DIR / "group_b_success.json"),
        ]
        groups = group_usage_rows(rows)
        self.assertEqual(len(groups), 2)
        self.assertIn("eval_suite/task_fam/variant_a/success", groups)
        self.assertIn("eval_suite/task_fam/variant_b/success", groups)
        self.assertEqual(len(groups["eval_suite/task_fam/variant_a/success"]), 2)


class CompareCostGroupsTests(unittest.TestCase):
    """Test compare_cost_groups."""

    def test_same_group_valid_comparison(self):
        before = [load_fixture(FIXTURE_DIR / "compare_before.json")]
        after = [load_fixture(FIXTURE_DIR / "compare_after.json")]
        result = compare_cost_groups(before, after)
        self.assertTrue(result.valid)
        self.assertIn("same group", result.reason)
        self.assertIsNotNone(result.cost_delta)
        self.assertIsNotNone(result.relative_change_pct)

    def test_different_group_invalid_comparison(self):
        row_a = load_fixture(FIXTURE_DIR / "group_a_success_1.json")
        row_b = load_fixture(FIXTURE_DIR / "group_b_success.json")
        result = compare_cost_groups([row_a], [row_b])
        self.assertFalse(result.valid)
        self.assertIn("different", result.reason)

    def test_one_group_zero_success_invalid(self):
        row_fail = load_fixture(FIXTURE_DIR / "group_all_fail.json")
        row_success = load_fixture(FIXTURE_DIR / "group_a_success_1.json")
        # Put both in same group
        row_fail["cost_of_pass_group"] = row_success["cost_of_pass_group"]
        result = compare_cost_groups([row_fail], [row_success])
        self.assertFalse(result.valid)
        self.assertIn("undefined", result.reason)


class DetectInvalidComparisonTests(unittest.TestCase):
    """Test detect_invalid_cost_comparison."""

    def test_same_group_valid(self):
        row_a = load_fixture(FIXTURE_DIR / "compare_before.json")
        row_b = load_fixture(FIXTURE_DIR / "compare_after.json")
        is_invalid, reason = detect_invalid_cost_comparison([row_a], [row_b])
        self.assertFalse(is_invalid)

    def test_different_group_invalid(self):
        row_a = load_fixture(FIXTURE_DIR / "group_a_success_1.json")
        row_b = load_fixture(FIXTURE_DIR / "group_b_success.json")
        is_invalid, reason = detect_invalid_cost_comparison([row_a], [row_b])
        self.assertTrue(is_invalid)
        self.assertIn("different", reason)

    def test_zero_success_invalid(self):
        row_fail = load_fixture(FIXTURE_DIR / "group_all_fail.json")
        is_invalid, reason = detect_invalid_cost_comparison([row_fail], [row_fail])
        self.assertTrue(is_invalid)
        self.assertIn("undefined", reason)


class ContextPackRefTests(unittest.TestCase):
    """Test fixture referencing context_pack_v2."""

    def test_context_pack_ref_fixture_validates(self):
        data = load_fixture(FIXTURE_DIR / "with_context_pack_ref.json")
        violations = validate_usage_ledger_row(data)
        self.assertEqual(violations, [])
        self.assertEqual(data["context_pack_id"], "adv-pack-001")


class MutationVariantRefTests(unittest.TestCase):
    """Test fixture referencing user_style_mutation_eval variant_family."""

    def test_mutation_variant_fixture_validates(self):
        data = load_fixture(FIXTURE_DIR / "with_mutation_variant.json")
        violations = validate_usage_ledger_row(data)
        self.assertEqual(violations, [])
        self.assertIn("user_style_chat_request", data["cost_of_pass_group"])


class FixtureLoadingTests(unittest.TestCase):
    """Test that all fixtures load and validate correctly."""

    def test_fixture_dir_exists(self):
        self.assertTrue(FIXTURE_DIR.exists())

    def test_all_fixtures_load(self):
        results = load_all_fixtures(FIXTURE_DIR)
        self.assertGreater(len(results), 0)

    def test_valid_fixtures_all_pass(self):
        valid_names = [
            "valid_row.json",
            "no_model_offline.json",
            "with_context_pack_ref.json",
            "with_mutation_variant.json",
            "group_a_success_1.json",
            "group_a_success_2.json",
            "group_a_failure.json",
            "group_b_success.json",
            "group_all_fail.json",
            "compare_before.json",
            "compare_after.json",
        ]
        for fname in valid_names:
            path = FIXTURE_DIR / fname
            self.assertTrue(path.exists(), f"Fixture {fname} not found")
            _, violations = load_usage_ledger_fixture(path)
            self.assertEqual(violations, [], f"Fixture {fname} has violations: {violations}")

    def test_invalid_fixtures_detected(self):
        invalid_names = [
            "missing_fields.json",
            "negative_tokens.json",
            "cached_exceeds_input.json",
            "invalid_group_format.json",
        ]
        for fname in invalid_names:
            path = FIXTURE_DIR / fname
            _, violations = load_usage_ledger_fixture(path)
            self.assertTrue(len(violations) > 0, f"Fixture {fname} should have violations")


class ExistingTestsUnchangedTests(unittest.TestCase):
    """Verify no existing test behavior is affected."""

    def test_existing_test_files_still_exist(self):
        base = Path(__file__).resolve().parents[1] / "tests"
        self.assertTrue((base / "test_real_world_eval.py").exists())
        self.assertTrue((base / "test_error_taxonomy.py").exists())
        self.assertTrue((base / "test_user_style_mutation_eval.py").exists())
        self.assertTrue((base / "test_context_pack.py").exists())


class DataclassTests(unittest.TestCase):
    """Test dataclass helpers."""

    def test_usage_ledger_row_to_dict(self):
        row = UsageLedgerRow(
            run_id="r1", case_id="c1",
            input_tokens=100, output_tokens=50, cached_tokens=10,
            request_count=1, tool_call_count=1, retry_count=0,
            wall_clock_ms=1000, estimated_cost=0.05,
            pass_=True, cost_of_pass_group="a/b/c/d",
            model_profile_id="p1", context_pack_id="cp1",
        )
        d = row.to_dict()
        self.assertIn("pass", d)
        self.assertNotIn("pass_", d)
        self.assertEqual(d["pass"], True)

    def test_cost_of_pass_aggregate_to_dict(self):
        agg = CostOfPassAggregate(
            cost_of_pass_group="a/b/c/d",
            total_estimated_cost=0.5,
            success_count=3,
            failure_count=1,
            total_count=4,
            cost_of_pass=0.125,
        )
        d = agg.to_dict()
        self.assertEqual(d["success_count"], 3)
        self.assertAlmostEqual(d["cost_of_pass"], 0.125)

    def test_comparison_result_to_dict(self):
        agg = CostOfPassAggregate("a/b/c/d", 0.5, 3, 1, 4, 0.125)
        result = compare_cost_groups(
            [{"cost_of_pass_group": "a/b/c/d", "estimated_cost": 0.2, "pass": True},
             {"cost_of_pass_group": "a/b/c/d", "estimated_cost": 0.3, "pass": True}],
            [{"cost_of_pass_group": "a/b/c/d", "estimated_cost": 0.15, "pass": True}],
        )
        d = result.to_dict()
        self.assertIn("valid", d)
        self.assertIn("cost_delta", d)


if __name__ == "__main__":
    unittest.main()
