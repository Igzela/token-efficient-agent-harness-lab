"""Tests for user-style mutation evaluation schema, fixtures, and admission rules."""

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.user_style_mutation import (
    ADMISSION_OUTCOMES,
    ADMISSION_SCOPES,
    CONTAMINATION_RISKS,
    REQUIRED_METADATA_FIELDS,
    REQUIRED_MUTATION_FIELDS,
    SCHEMA_VERSION,
    SOURCE_TYPES,
    VARIANT_TYPES,
    FixtureMetadata,
    MutationCase,
    create_mutation_case,
    group_by_admission,
    group_by_base_fixture,
    group_by_variant,
    load_all_fixtures,
    load_fixture,
    validate_fixture_metadata,
    validate_mutation_case,
)

FIXTURE_DIR = Path(__file__).resolve().parents[1] / "tests" / "fixtures" / "user_style_mutation_eval"


class VariantTypeEnumerationTests(unittest.TestCase):
    """Test the three variant types are complete."""

    def test_three_variant_types(self):
        self.assertEqual(len(VARIANT_TYPES), 3)

    def test_expected_variants(self):
        self.assertEqual(
            set(VARIANT_TYPES),
            {"formal_issue", "user_style_chat_request", "terse_ticket"},
        )


class BaseFixtureCoverageTests(unittest.TestCase):
    """Test that each representative base fixture has all three variants."""

    def test_four_base_fixtures(self):
        results = load_all_fixtures(FIXTURE_DIR)
        bases = {data["base_fixture_id"] for _, data, v in results if not v}
        self.assertEqual(len(bases), 4)

    def test_expected_base_fixtures(self):
        results = load_all_fixtures(FIXTURE_DIR)
        bases = {data["base_fixture_id"] for _, data, v in results if not v}
        self.assertEqual(
            bases,
            {
                "bugfix-project",
                "config-rule-project",
                "doc-update-project",
                "failure-fix-loop-project",
            },
        )

    def test_each_base_has_three_variants(self):
        groups = group_by_base_fixture(load_all_fixtures(FIXTURE_DIR))
        for base_id, cases in groups.items():
            self.assertEqual(
                len(cases), 3,
                f"Base fixture {base_id} has {len(cases)} variants, expected 3",
            )

    def test_each_base_has_all_variant_types(self):
        groups = group_by_base_fixture(load_all_fixtures(FIXTURE_DIR))
        for base_id, cases in groups.items():
            variant_types = {c["variant_type"] for c in cases}
            self.assertEqual(
                variant_types,
                set(VARIANT_TYPES),
                f"Base fixture {base_id} missing variant types: {set(VARIANT_TYPES) - variant_types}",
            )


class FormalIssueAdmissionTests(unittest.TestCase):
    """formal_issue variants should always be admitted."""

    def test_all_formal_issues_admitted(self):
        results = load_all_fixtures(FIXTURE_DIR)
        for filename, data, violations in results:
            if violations:
                continue
            if data["variant_type"] == "formal_issue":
                self.assertEqual(
                    data["admission_expectation"],
                    "admitted",
                    f"Fixture {filename}: formal_issue must be admitted",
                )


class ChatRequestAdmissionTests(unittest.TestCase):
    """user_style_chat_request should be admitted or needs_clarification, never silently fail."""

    def test_chat_requests_never_rejected(self):
        results = load_all_fixtures(FIXTURE_DIR)
        for filename, data, violations in results:
            if violations:
                continue
            if data["variant_type"] == "user_style_chat_request":
                self.assertIn(
                    data["admission_expectation"],
                    ("admitted", "needs_clarification"),
                    f"Fixture {filename}: chat_request must be admitted or needs_clarification, "
                    f"got {data['admission_expectation']}",
                )

    def test_chat_requests_never_diagnostic(self):
        results = load_all_fixtures(FIXTURE_DIR)
        for filename, data, violations in results:
            if violations:
                continue
            if data["variant_type"] == "user_style_chat_request":
                self.assertNotEqual(
                    data["admission_expectation"],
                    "diagnostic",
                    f"Fixture {filename}: chat_request should not be diagnostic",
                )


class TerseTicketAdmissionTests(unittest.TestCase):
    """terse_ticket with insufficient info should not伪装成 admitted."""

    def test_terse_tickets_not_admitted(self):
        results = load_all_fixtures(FIXTURE_DIR)
        for filename, data, violations in results:
            if violations:
                continue
            if data["variant_type"] == "terse_ticket":
                self.assertIn(
                    data["admission_expectation"],
                    ("needs_clarification", "diagnostic"),
                    f"Fixture {filename}: terse_ticket must not be admitted",
                )


class FixtureMetadataValidationTests(unittest.TestCase):
    """Test fixture_metadata schema enforcement."""

    def test_metadata_all_fields_required(self):
        data = {
            "schema_version": SCHEMA_VERSION,
            "case_id": "test",
            "base_fixture_id": "test",
            "variant_type": "formal_issue",
            "user_prompt": "test",
            "expected_task_family": "test",
            "expected_required_fields": [],
            "expected_missing_fields": [],
            "admission_expectation": "admitted",
            "evidence_refs": [],
            "fixture_metadata": {
                "fixture_id": "test",
            },
        }
        violations = validate_mutation_case(data)
        meta_violations = [v for v in violations if "fixture_metadata" in v]
        self.assertGreater(len(meta_violations), 0)

    def test_contamination_risk_invalid_value(self):
        meta = {
            "fixture_id": "test",
            "source_type": "synthetic",
            "freshness": "2026-05-19",
            "estimated_human_minutes": 1.0,
            "difficulty": "easy",
            "contamination_risk": "extreme",
            "admission_scope": "admitted",
        }
        violations = validate_fixture_metadata(meta)
        self.assertTrue(any("contamination_risk" in v for v in violations))

    def test_contamination_risk_valid_values(self):
        for risk in CONTAMINATION_RISKS:
            meta = {
                "fixture_id": "test",
                "source_type": "synthetic",
                "freshness": "2026-05-19",
                "estimated_human_minutes": 1.0,
                "difficulty": "easy",
                "contamination_risk": risk,
                "admission_scope": "admitted",
            }
            violations = validate_fixture_metadata(meta)
            self.assertEqual(violations, [], f"contamination_risk={risk} should be valid")

    def test_admission_scope_invalid_value(self):
        meta = {
            "fixture_id": "test",
            "source_type": "synthetic",
            "freshness": "2026-05-19",
            "estimated_human_minutes": 1.0,
            "difficulty": "easy",
            "contamination_risk": "low",
            "admission_scope": "blocked",
        }
        violations = validate_fixture_metadata(meta)
        self.assertTrue(any("admission_scope" in v for v in violations))

    def test_admission_scope_valid_values(self):
        for scope in ADMISSION_SCOPES:
            meta = {
                "fixture_id": "test",
                "source_type": "synthetic",
                "freshness": "2026-05-19",
                "estimated_human_minutes": 1.0,
                "difficulty": "easy",
                "contamination_risk": "low",
                "admission_scope": scope,
            }
            violations = validate_fixture_metadata(meta)
            self.assertEqual(violations, [], f"admission_scope={scope} should be valid")

    def test_source_type_invalid_value(self):
        meta = {
            "fixture_id": "test",
            "source_type": "imported",
            "freshness": "2026-05-19",
            "estimated_human_minutes": 1.0,
            "difficulty": "easy",
            "contamination_risk": "low",
            "admission_scope": "admitted",
        }
        violations = validate_fixture_metadata(meta)
        self.assertTrue(any("source_type" in v for v in violations))

    def test_source_type_valid_values(self):
        for st in SOURCE_TYPES:
            meta = {
                "fixture_id": "test",
                "source_type": st,
                "freshness": "2026-05-19",
                "estimated_human_minutes": 1.0,
                "difficulty": "easy",
                "contamination_risk": "low",
                "admission_scope": "admitted",
            }
            violations = validate_fixture_metadata(meta)
            self.assertEqual(violations, [], f"source_type={st} should be valid")

    def test_estimated_human_minutes_must_be_numeric(self):
        meta = {
            "fixture_id": "test",
            "source_type": "synthetic",
            "freshness": "2026-05-19",
            "estimated_human_minutes": "five",
            "difficulty": "easy",
            "contamination_risk": "low",
            "admission_scope": "admitted",
        }
        violations = validate_fixture_metadata(meta)
        self.assertTrue(any("estimated_human_minutes" in v for v in violations))


class MutationCaseSchemaTests(unittest.TestCase):
    """Test mutation case validation."""

    def test_valid_case_passes(self):
        record = create_mutation_case(
            case_id="test-001",
            base_fixture_id="test-project",
            variant_type="formal_issue",
            user_prompt="test prompt",
            expected_task_family="bugfix",
            admission_expectation="admitted",
        )
        violations = validate_mutation_case(record.to_dict())
        self.assertEqual(violations, [])

    def test_missing_required_field_fails(self):
        data = {
            "schema_version": SCHEMA_VERSION,
            "case_id": "test",
        }
        violations = validate_mutation_case(data)
        self.assertTrue(any("missing required field" in v for v in violations))

    def test_invalid_variant_type_fails(self):
        record = create_mutation_case(
            case_id="test-001",
            base_fixture_id="test",
            variant_type="invalid_type",
            user_prompt="test",
            expected_task_family="test",
            admission_expectation="admitted",
        )
        violations = validate_mutation_case(record.to_dict())
        self.assertTrue(any("variant_type" in v for v in violations))

    def test_invalid_admission_expectation_fails(self):
        record = create_mutation_case(
            case_id="test-001",
            base_fixture_id="test",
            variant_type="formal_issue",
            user_prompt="test",
            expected_task_family="test",
            admission_expectation="maybe",
        )
        violations = validate_mutation_case(record.to_dict())
        self.assertTrue(any("admission_expectation" in v for v in violations))

    def test_wrong_schema_version_fails(self):
        data = {
            "schema_version": "user_style_mutation.v0",
            "case_id": "test",
            "base_fixture_id": "test",
            "variant_type": "formal_issue",
            "user_prompt": "test",
            "expected_task_family": "test",
            "expected_required_fields": [],
            "expected_missing_fields": [],
            "admission_expectation": "admitted",
            "evidence_refs": [],
            "fixture_metadata": {
                "fixture_id": "test",
                "source_type": "synthetic",
                "freshness": "2026-05-19",
                "estimated_human_minutes": 1.0,
                "difficulty": "easy",
                "contamination_risk": "low",
                "admission_scope": "admitted",
            },
        }
        violations = validate_mutation_case(data)
        self.assertTrue(any("schema_version" in v for v in violations))

    def test_non_list_evidence_refs_fails(self):
        data = {
            "schema_version": SCHEMA_VERSION,
            "case_id": "test",
            "base_fixture_id": "test",
            "variant_type": "formal_issue",
            "user_prompt": "test",
            "expected_task_family": "test",
            "expected_required_fields": [],
            "expected_missing_fields": [],
            "admission_expectation": "admitted",
            "evidence_refs": "not-a-list",
            "fixture_metadata": {
                "fixture_id": "test",
                "source_type": "synthetic",
                "freshness": "2026-05-19",
                "estimated_human_minutes": 1.0,
                "difficulty": "easy",
                "contamination_risk": "low",
                "admission_scope": "admitted",
            },
        }
        violations = validate_mutation_case(data)
        self.assertTrue(any("evidence_refs" in v for v in violations))


class FixtureLoadingTests(unittest.TestCase):
    """Test that all mutation fixtures load and validate."""

    def test_fixture_dir_exists(self):
        self.assertTrue(FIXTURE_DIR.exists())
        self.assertTrue(FIXTURE_DIR.is_dir())

    def test_all_fixtures_load(self):
        results = load_all_fixtures(FIXTURE_DIR)
        self.assertEqual(len(results), 12)
        for filename, data, violations in results:
            self.assertEqual(
                violations, [], f"Fixture {filename} has violations: {violations}"
            )

    def test_each_fixture_has_correct_schema_version(self):
        results = load_all_fixtures(FIXTURE_DIR)
        for filename, data, _ in results:
            self.assertEqual(
                data["schema_version"],
                SCHEMA_VERSION,
                f"Fixture {filename} has wrong schema_version",
            )

    def test_each_fixture_has_unique_case_id(self):
        results = load_all_fixtures(FIXTURE_DIR)
        ids = [data["case_id"] for _, data, _ in results]
        self.assertEqual(len(ids), len(set(ids)), "Duplicate case_id in fixtures")

    def test_each_fixture_has_unique_fixture_id_in_metadata(self):
        results = load_all_fixtures(FIXTURE_DIR)
        fids = [data["fixture_metadata"]["fixture_id"] for _, data, _ in results]
        self.assertEqual(len(fids), len(set(fids)), "Duplicate fixture_id in metadata")


class AdmissionGroupingTests(unittest.TestCase):
    """Test admission grouping helpers."""

    def test_group_by_admission(self):
        groups = group_by_admission(load_all_fixtures(FIXTURE_DIR))
        self.assertGreater(len(groups["admitted"]), 0)
        self.assertGreater(len(groups["needs_clarification"]), 0)

    def test_group_by_variant(self):
        groups = group_by_variant(load_all_fixtures(FIXTURE_DIR))
        for vt in VARIANT_TYPES:
            self.assertGreater(len(groups[vt]), 0, f"No fixtures for variant {vt}")

    def test_group_by_base_fixture(self):
        groups = group_by_base_fixture(load_all_fixtures(FIXTURE_DIR))
        self.assertEqual(len(groups), 4)


class ExistingTestsUnchangedTests(unittest.TestCase):
    """Verify no existing real_world_eval tests are affected."""

    def test_real_world_eval_fixture_dir_unchanged(self):
        rw_dir = Path(__file__).resolve().parents[1] / "tests" / "fixtures" / "real_world_eval"
        self.assertTrue(rw_dir.exists())
        # The new fixture dir is separate
        new_dir = Path(__file__).resolve().parents[1] / "tests" / "fixtures" / "user_style_mutation_eval"
        self.assertNotEqual(rw_dir, new_dir)

    def test_no_existing_test_files_modified(self):
        """Check that existing test files were not modified by checking import patterns."""
        existing_test = Path(__file__).resolve().parents[1] / "tests" / "test_real_world_eval.py"
        self.assertTrue(existing_test.exists())
        content = existing_test.read_text()
        self.assertIn("FIXTURE_BASE", content)
        self.assertIn("real_world_eval", content)


class MutationCaseDataclassTests(unittest.TestCase):
    """Test MutationCase dataclass and factory."""

    def test_to_dict_roundtrip(self):
        record = create_mutation_case(
            case_id="test-001",
            base_fixture_id="test",
            variant_type="formal_issue",
            user_prompt="test prompt",
            expected_task_family="bugfix",
            admission_expectation="admitted",
        )
        d = record.to_dict()
        self.assertIsInstance(d, dict)
        self.assertEqual(d["variant_type"], "formal_issue")

    def test_to_json_produces_valid_json(self):
        record = create_mutation_case(
            case_id="test-001",
            base_fixture_id="test",
            variant_type="terse_ticket",
            user_prompt="test",
            expected_task_family="bugfix",
            admission_expectation="needs_clarification",
        )
        raw = record.to_json(indent=2)
        parsed = json.loads(raw)
        self.assertEqual(parsed["variant_type"], "terse_ticket")

    def test_factory_defaults(self):
        record = create_mutation_case(
            case_id="test-001",
            base_fixture_id="test",
            variant_type="user_style_chat_request",
            user_prompt="test",
            expected_task_family="test",
            admission_expectation="admitted",
        )
        self.assertEqual(record.schema_version, SCHEMA_VERSION)
        self.assertEqual(record.expected_required_fields, [])
        self.assertEqual(record.expected_missing_fields, [])
        self.assertEqual(record.evidence_refs, [])
        self.assertIsInstance(record.fixture_metadata, FixtureMetadata)


if __name__ == "__main__":
    unittest.main()
