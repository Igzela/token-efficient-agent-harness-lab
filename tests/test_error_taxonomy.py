"""Tests for Tool/Error Taxonomy schema, validation, and fixtures."""

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.error_taxonomy import (
    CANONICAL_DOMAINS,
    MANDATORY_TRIAGE_DOMAINS,
    NON_ADOPTABLE_DOMAINS,
    NON_RETRYABLE_DOMAINS,
    REQUIRED_FIELDS,
    SCHEMA_VERSION,
    ErrorDomain,
    ErrorRecord,
    create_error_record,
    is_adoptable,
    is_retryable,
    load_all_fixtures,
    load_fixture,
    requires_triage,
    validate_error_record,
)

FIXTURE_DIR = Path(__file__).resolve().parents[1] / "tests" / "fixtures" / "tool_error_cases"


class CanonicalDomainEnumerationTests(unittest.TestCase):
    """Test the canonical domain set is complete and consistent."""

    def test_ten_domains_defined(self):
        self.assertEqual(len(CANONICAL_DOMAINS), 10)

    def test_expected_domains_present(self):
        expected = {
            "tool_contract_error",
            "environment_error",
            "context_error",
            "model_judgment_error",
            "evaluation_error",
            "harness_bug",
            "user_abort",
            "provider_error",
            "timeout",
            "unknown_error",
        }
        self.assertEqual(set(CANONICAL_DOMAINS), expected)

    def test_enum_values_match_list(self):
        self.assertEqual(CANONICAL_DOMAINS, [d.value for d in ErrorDomain])

    def test_unknown_error_is_last_canonical(self):
        self.assertEqual(CANONICAL_DOMAINS[-1], "unknown_error")


class ErrorRecordSchemaTests(unittest.TestCase):
    """Test error_record schema validation."""

    def test_valid_record_passes(self):
        record = create_error_record(
            error_domain="tool_contract_error",
            error_class="SchemaViolationError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        violations = validate_error_record(record.to_dict())
        self.assertEqual(violations, [])

    def test_missing_required_field_fails(self):
        record = create_error_record(
            error_domain="tool_contract_error",
            error_class="SchemaViolationError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        d = record.to_dict()
        del d["error_id"]
        violations = validate_error_record(d)
        self.assertTrue(any("error_id" in v for v in violations))

    def test_missing_schema_version_fails(self):
        record = create_error_record(
            error_domain="tool_contract_error",
            error_class="SchemaViolationError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        d = record.to_dict()
        del d["schema_version"]
        violations = validate_error_record(d)
        self.assertTrue(any("schema_version" in v for v in violations))

    def test_wrong_schema_version_fails(self):
        record = create_error_record(
            error_domain="tool_contract_error",
            error_class="SchemaViolationError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        d = record.to_dict()
        d["schema_version"] = "error_record.v0"
        violations = validate_error_record(d)
        self.assertTrue(any("schema_version" in v for v in violations))

    def test_invalid_domain_fails(self):
        record = create_error_record(
            error_domain="tool_contract_error",
            error_class="SchemaViolationError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        d = record.to_dict()
        d["error_domain"] = "bogus_domain"
        violations = validate_error_record(d)
        self.assertTrue(any("error_domain" in v for v in violations))

    def test_non_bool_retryable_fails(self):
        record = create_error_record(
            error_domain="tool_contract_error",
            error_class="SchemaViolationError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        d = record.to_dict()
        d["retryable"] = "yes"
        violations = validate_error_record(d)
        self.assertTrue(any("retryable" in v for v in violations))

    def test_non_bool_counts_against_model_fails(self):
        record = create_error_record(
            error_domain="tool_contract_error",
            error_class="SchemaViolationError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        d = record.to_dict()
        d["counts_against_model"] = 1
        violations = validate_error_record(d)
        self.assertTrue(any("counts_against_model" in v for v in violations))

    def test_non_list_evidence_refs_fails(self):
        record = create_error_record(
            error_domain="tool_contract_error",
            error_class="SchemaViolationError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        d = record.to_dict()
        d["evidence_refs"] = "not-a-list"
        violations = validate_error_record(d)
        self.assertTrue(any("evidence_refs" in v for v in violations))

    def test_empty_error_id_fails(self):
        record = create_error_record(
            error_domain="tool_contract_error",
            error_class="SchemaViolationError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        d = record.to_dict()
        d["error_id"] = ""
        violations = validate_error_record(d)
        self.assertTrue(any("error_id" in v for v in violations))


class UnknownErrorFailHardTests(unittest.TestCase):
    """Test unknown_error domain constraints: fail-hard, mandatory triage, non-adoptable."""

    def test_unknown_error_must_be_non_retryable(self):
        record = create_error_record(
            error_domain="unknown_error",
            error_class="UnrecognisedError",
            retryable=False,
            counts_against_model=False,
            requires_human_triage=True,
        )
        violations = validate_error_record(record.to_dict())
        self.assertEqual(violations, [])

    def test_unknown_error_retryable_true_fails(self):
        record = create_error_record(
            error_domain="unknown_error",
            error_class="UnrecognisedError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=True,
        )
        violations = validate_error_record(record.to_dict())
        self.assertTrue(any("retryable=false" in v for v in violations))

    def test_unknown_error_requires_human_triage(self):
        record = create_error_record(
            error_domain="unknown_error",
            error_class="UnrecognisedError",
            retryable=False,
            counts_against_model=False,
            requires_human_triage=False,
        )
        violations = validate_error_record(record.to_dict())
        self.assertTrue(any("requires_human_triage=true" in v for v in violations))

    def test_unknown_error_not_adoptable(self):
        self.assertFalse(is_adoptable("unknown_error"))
        self.assertIn("unknown_error", NON_ADOPTABLE_DOMAINS)

    def test_unknown_error_non_retryable(self):
        self.assertFalse(is_retryable("unknown_error"))
        self.assertIn("unknown_error", NON_RETRYABLE_DOMAINS)

    def test_unknown_error_mandatory_triage(self):
        self.assertTrue(requires_triage("unknown_error"))
        self.assertIn("unknown_error", MANDATORY_TRIAGE_DOMAINS)


class RetryableAndCountsAgainstModelCombinationTests(unittest.TestCase):
    """Test that retryable and counts_against_model combinations are valid."""

    def test_user_abort_not_retryable(self):
        violations = validate_error_record({
            "schema_version": SCHEMA_VERSION,
            "error_id": "test",
            "error_domain": "user_abort",
            "error_class": "UserCancelledError",
            "retryable": True,
            "counts_against_model": False,
            "requires_human_triage": False,
            "tool_name": "",
            "model_profile_id": "",
            "context_pack_id": "",
            "event_id": "",
            "evidence_refs": [],
            "created_at": "2026-05-19T00:00:00Z",
        })
        self.assertTrue(any("user_abort" in v and "retryable" in v for v in violations))

    def test_harness_bug_not_retryable(self):
        violations = validate_error_record({
            "schema_version": SCHEMA_VERSION,
            "error_id": "test",
            "error_domain": "harness_bug",
            "error_class": "InternalAssertionError",
            "retryable": True,
            "counts_against_model": False,
            "requires_human_triage": True,
            "tool_name": "",
            "model_profile_id": "",
            "context_pack_id": "",
            "event_id": "",
            "evidence_refs": [],
            "created_at": "2026-05-19T00:00:00Z",
        })
        self.assertTrue(any("harness_bug" in v and "retryable" in v for v in violations))

    def test_harness_bug_requires_triage(self):
        violations = validate_error_record({
            "schema_version": SCHEMA_VERSION,
            "error_id": "test",
            "error_domain": "harness_bug",
            "error_class": "InternalAssertionError",
            "retryable": False,
            "counts_against_model": False,
            "requires_human_triage": False,
            "tool_name": "",
            "model_profile_id": "",
            "context_pack_id": "",
            "event_id": "",
            "evidence_refs": [],
            "created_at": "2026-05-19T00:00:00Z",
        })
        self.assertTrue(any("harness_bug" in v and "triage" in v for v in violations))

    def test_provider_error_retryable(self):
        violations = validate_error_record({
            "schema_version": SCHEMA_VERSION,
            "error_id": "test",
            "error_domain": "provider_error",
            "error_class": "RateLimitError",
            "retryable": False,
            "counts_against_model": False,
            "requires_human_triage": False,
            "tool_name": "",
            "model_profile_id": "",
            "context_pack_id": "",
            "event_id": "",
            "evidence_refs": [],
            "created_at": "2026-05-19T00:00:00Z",
        })
        self.assertTrue(any("provider_error" in v and "retryable" in v for v in violations))

    def test_timeout_retryable(self):
        violations = validate_error_record({
            "schema_version": SCHEMA_VERSION,
            "error_id": "test",
            "error_domain": "timeout",
            "error_class": "ExecutionTimeoutError",
            "retryable": False,
            "counts_against_model": False,
            "requires_human_triage": False,
            "tool_name": "",
            "model_profile_id": "",
            "context_pack_id": "",
            "event_id": "",
            "evidence_refs": [],
            "created_at": "2026-05-19T00:00:00Z",
        })
        self.assertTrue(any("timeout" in v and "retryable" in v for v in violations))

    def test_model_judgment_error_counts_against_model(self):
        record = create_error_record(
            error_domain="model_judgment_error",
            error_class="WrongToolSelectedError",
            retryable=True,
            counts_against_model=True,
            requires_human_triage=False,
        )
        violations = validate_error_record(record.to_dict())
        self.assertEqual(violations, [])

    def test_tool_contract_error_default_no_count(self):
        record = create_error_record(
            error_domain="tool_contract_error",
            error_class="SchemaViolationError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        violations = validate_error_record(record.to_dict())
        self.assertEqual(violations, [])

    def test_environment_error_no_count(self):
        record = create_error_record(
            error_domain="environment_error",
            error_class="FileNotFoundError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        violations = validate_error_record(record.to_dict())
        self.assertEqual(violations, [])

    def test_provider_error_and_timeout_distinguishable(self):
        self.assertNotEqual("provider_error", "timeout")
        self.assertIn("provider_error", CANONICAL_DOMAINS)
        self.assertIn("timeout", CANONICAL_DOMAINS)


class UserAbortNotSystemFailureTests(unittest.TestCase):
    """Verify user_abort is treated as graceful, not a system failure."""

    def test_user_abort_valid_when_non_retryable(self):
        record = create_error_record(
            error_domain="user_abort",
            error_class="UserCancelledError",
            retryable=False,
            counts_against_model=False,
            requires_human_triage=False,
        )
        violations = validate_error_record(record.to_dict())
        self.assertEqual(violations, [])

    def test_user_abort_not_in_mandatory_triage(self):
        self.assertNotIn("user_abort", MANDATORY_TRIAGE_DOMAINS)

    def test_user_abort_not_adoptable_is_not_relevant(self):
        # user_abort is adoptable by the rule (not in NON_ADOPTABLE_DOMAINS)
        # but it carries no model signal
        self.assertTrue(is_adoptable("user_abort"))


class FixtureLoadingTests(unittest.TestCase):
    """Test that all fixtures load and validate correctly."""

    def test_fixture_dir_exists(self):
        self.assertTrue(FIXTURE_DIR.exists())
        self.assertTrue(FIXTURE_DIR.is_dir())

    def test_all_fixtures_load(self):
        results = load_all_fixtures(FIXTURE_DIR)
        self.assertGreater(len(results), 0)
        for filename, data, violations in results:
            self.assertEqual(
                violations, [], f"Fixture {filename} has violations: {violations}"
            )

    def test_one_fixture_per_domain(self):
        results = load_all_fixtures(FIXTURE_DIR)
        domains = {data["error_domain"] for _, data, _ in results}
        self.assertEqual(domains, set(CANONICAL_DOMAINS))

    def test_each_fixture_has_correct_schema_version(self):
        results = load_all_fixtures(FIXTURE_DIR)
        for filename, data, _ in results:
            self.assertEqual(
                data["schema_version"],
                SCHEMA_VERSION,
                f"Fixture {filename} has wrong schema_version",
            )

    def test_each_fixture_has_unique_error_id(self):
        results = load_all_fixtures(FIXTURE_DIR)
        ids = [data["error_id"] for _, data, _ in results]
        self.assertEqual(len(ids), len(set(ids)), "Duplicate error_id in fixtures")

    def test_fixture_unknown_error_enforces_fail_hard(self):
        data = load_fixture(FIXTURE_DIR / "unknown_error.json")
        self.assertFalse(data["retryable"])
        self.assertTrue(data["requires_human_triage"])

    def test_fixture_user_abort_not_retryable(self):
        data = load_fixture(FIXTURE_DIR / "user_abort.json")
        self.assertFalse(data["retryable"])
        self.assertFalse(data["counts_against_model"])

    def test_fixture_model_judgment_counts(self):
        data = load_fixture(FIXTURE_DIR / "model_judgment_error.json")
        self.assertTrue(data["counts_against_model"])

    def test_fixture_provider_error_retryable(self):
        data = load_fixture(FIXTURE_DIR / "provider_error.json")
        self.assertTrue(data["retryable"])

    def test_fixture_timeout_retryable(self):
        data = load_fixture(FIXTURE_DIR / "timeout.json")
        self.assertTrue(data["retryable"])

    def test_fixture_timeout_not_provider_error(self):
        timeout = load_fixture(FIXTURE_DIR / "timeout.json")
        provider = load_fixture(FIXTURE_DIR / "provider_error.json")
        self.assertNotEqual(timeout["error_domain"], provider["error_domain"])
        self.assertNotEqual(timeout["error_class"], provider["error_class"])

    def test_fixture_harness_bug_non_retryable_triage(self):
        data = load_fixture(FIXTURE_DIR / "harness_bug.json")
        self.assertFalse(data["retryable"])
        self.assertTrue(data["requires_human_triage"])


class ErrorRecordDataclassTests(unittest.TestCase):
    """Test ErrorRecord dataclass and factory."""

    def test_to_dict_roundtrip(self):
        record = create_error_record(
            error_domain="environment_error",
            error_class="FileNotFoundError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        d = record.to_dict()
        self.assertIsInstance(d, dict)
        self.assertEqual(d["error_domain"], "environment_error")

    def test_to_json_produces_valid_json(self):
        record = create_error_record(
            error_domain="timeout",
            error_class="ExecutionTimeoutError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        raw = record.to_json(indent=2)
        parsed = json.loads(raw)
        self.assertEqual(parsed["error_domain"], "timeout")

    def test_factory_defaults(self):
        record = create_error_record(
            error_domain="context_error",
            error_class="MissingContextPackError",
            retryable=True,
            counts_against_model=False,
            requires_human_triage=False,
        )
        self.assertEqual(record.schema_version, SCHEMA_VERSION)
        self.assertTrue(len(record.error_id) > 0)
        self.assertTrue(len(record.created_at) > 0)
        self.assertEqual(record.evidence_refs, [])


class HelperFunctionTests(unittest.TestCase):
    """Test is_adoptable, is_retryable, requires_triage helpers."""

    def test_is_adoptable_for_normal_domains(self):
        self.assertTrue(is_adoptable("tool_contract_error"))
        self.assertTrue(is_adoptable("model_judgment_error"))
        self.assertTrue(is_adoptable("provider_error"))

    def test_is_not_adoptable_for_unknown(self):
        self.assertFalse(is_adoptable("unknown_error"))

    def test_is_retryable_for_retryable_domains(self):
        self.assertTrue(is_retryable("tool_contract_error"))
        self.assertTrue(is_retryable("environment_error"))
        self.assertTrue(is_retryable("provider_error"))
        self.assertTrue(is_retryable("timeout"))

    def test_is_not_retryable_for_non_retryable_domains(self):
        self.assertFalse(is_retryable("user_abort"))
        self.assertFalse(is_retryable("harness_bug"))
        self.assertFalse(is_retryable("unknown_error"))

    def test_requires_triage_for_mandatory_domains(self):
        self.assertTrue(requires_triage("unknown_error"))
        self.assertTrue(requires_triage("harness_bug"))

    def test_does_not_require_triage_for_other_domains(self):
        self.assertFalse(requires_triage("tool_contract_error"))
        self.assertFalse(requires_triage("user_abort"))
        self.assertFalse(requires_triage("provider_error"))


if __name__ == "__main__":
    unittest.main()
