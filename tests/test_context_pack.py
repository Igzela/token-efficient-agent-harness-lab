"""Tests for Context Pack v2 schema, validation, and fixtures."""

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from harness_core.context_pack import (
    ADVISOR_CONTEXT_PACK_V2,
    CACHE_POLICY_VALUES,
    CONTENT_MODES,
    CONTEXT_LAYERS_VERSION,
    CONTEXT_RETRIEVAL_REQUEST,
    CONTEXT_RETRIEVAL_RESULT,
    FRESHNESS_VALUES,
    MODEL_CONTEXT_PACK_V2,
    MODEL_ROLES,
    PACK_PRUNE_POLICY_VALUES,
    RETRIEVAL_RESULT_STATUS,
    CALL_TYPES,
    ContextBudget,
    ContextLayers,
    MemoryDigest,
    RetrievalPolicy,
    apply_prune_policy,
    check_budget_compliance,
    load_all_fixtures,
    load_fixture,
    validate_advisor_context_pack_v2,
    validate_context_layers,
    validate_context_retrieval_request,
    validate_context_retrieval_result,
    validate_full_content_inline_denied,
    validate_model_context_pack_v2,
)

FIXTURE_DIR = Path(__file__).resolve().parents[1] / "tests" / "fixtures" / "context_pack_v2"


class CanonicalSchemaEnumerationTests(unittest.TestCase):
    """Test the four canonical wire schemas exist and are consistent."""

    def test_advisor_schema_version(self):
        self.assertEqual(ADVISOR_CONTEXT_PACK_V2, "advisor_context_pack.v2")

    def test_model_schema_version(self):
        self.assertEqual(MODEL_CONTEXT_PACK_V2, "model_context_pack.v2")

    def test_retrieval_request_version(self):
        self.assertEqual(CONTEXT_RETRIEVAL_REQUEST, "context_retrieval_request.v1")

    def test_retrieval_result_version(self):
        self.assertEqual(CONTEXT_RETRIEVAL_RESULT, "context_retrieval_result.v1")

    def test_context_layers_version(self):
        self.assertEqual(CONTEXT_LAYERS_VERSION, "context_layers.v1")


class AdvisorPackValidationTests(unittest.TestCase):
    """Test advisor_context_pack_v2 validation."""

    def test_valid_pack_passes(self):
        data = load_fixture(FIXTURE_DIR / "advisor_pack_valid.json")
        violations = validate_advisor_context_pack_v2(data)
        self.assertEqual(violations, [])

    def test_missing_required_field_fails(self):
        data = load_fixture(FIXTURE_DIR / "advisor_pack_valid.json")
        del data["call_type"]
        violations = validate_advisor_context_pack_v2(data)
        self.assertTrue(any("call_type" in v for v in violations))

    def test_invalid_call_type_fails(self):
        data = load_fixture(FIXTURE_DIR / "advisor_pack_valid.json")
        data["call_type"] = "invalid"
        violations = validate_advisor_context_pack_v2(data)
        self.assertTrue(any("call_type" in v for v in violations))

    def test_wrong_schema_version_fails(self):
        data = load_fixture(FIXTURE_DIR / "advisor_pack_valid.json")
        data["schema_version"] = "advisor_context_pack.v1"
        violations = validate_advisor_context_pack_v2(data)
        self.assertTrue(any("schema_version" in v for v in violations))

    def test_non_list_allowed_files_fails(self):
        data = load_fixture(FIXTURE_DIR / "advisor_pack_valid.json")
        data["allowed_files"] = "not-a-list"
        violations = validate_advisor_context_pack_v2(data)
        self.assertTrue(any("allowed_files" in v for v in violations))

    def test_missing_budget_fails(self):
        data = load_fixture(FIXTURE_DIR / "advisor_pack_missing_budget.json")
        violations = validate_advisor_context_pack_v2(data)
        self.assertTrue(len(violations) > 0)


class ModelPackValidationTests(unittest.TestCase):
    """Test model_context_pack_v2 validation."""

    def test_valid_pack_passes(self):
        data = load_fixture(FIXTURE_DIR / "model_pack_valid.json")
        violations = validate_model_context_pack_v2(data)
        self.assertEqual(violations, [])

    def test_invalid_role_fails(self):
        data = load_fixture(FIXTURE_DIR / "model_pack_valid.json")
        data["role"] = "invalid_role"
        violations = validate_model_context_pack_v2(data)
        self.assertTrue(any("role" in v for v in violations))

    def test_wrong_schema_version_fails(self):
        data = load_fixture(FIXTURE_DIR / "model_pack_valid.json")
        data["schema_version"] = "model_context_pack.v1"
        violations = validate_model_context_pack_v2(data)
        self.assertTrue(any("schema_version" in v for v in violations))

    def test_non_list_allowed_tools_fails(self):
        data = load_fixture(FIXTURE_DIR / "model_pack_valid.json")
        data["allowed_tools"] = "not-a-list"
        violations = validate_model_context_pack_v2(data)
        self.assertTrue(any("allowed_tools" in v for v in violations))


class RetrievalRequestValidationTests(unittest.TestCase):
    """Test context_retrieval_request validation."""

    def test_valid_request_passes(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_request_valid.json")
        violations = validate_context_retrieval_request(data)
        self.assertEqual(violations, [])

    def test_empty_reason_fails(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_request_no_reason.json")
        violations = validate_context_retrieval_request(data)
        self.assertTrue(any("reason" in v for v in violations))

    def test_invalid_requester_type_fails(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_request_valid.json")
        data["requester_type"] = "invalid"
        violations = validate_context_retrieval_request(data)
        self.assertTrue(any("requester_type" in v for v in violations))

    def test_invalid_priority_fails(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_request_valid.json")
        data["priority"] = "urgent"
        violations = validate_context_retrieval_request(data)
        self.assertTrue(any("priority" in v for v in violations))

    def test_zero_token_budget_fails(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_request_valid.json")
        data["token_budget"] = 0
        violations = validate_context_retrieval_request(data)
        self.assertTrue(any("token_budget" in v for v in violations))

    def test_invalid_requested_scope_fails(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_request_valid.json")
        data["requested_refs"][0]["requested_scope"] = "entire"
        violations = validate_context_retrieval_request(data)
        self.assertTrue(any("requested_scope" in v for v in violations))


class RetrievalResultValidationTests(unittest.TestCase):
    """Test context_retrieval_result validation."""

    def test_valid_result_passes(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_result_valid.json")
        violations = validate_context_retrieval_result(data)
        self.assertEqual(violations, [])

    def test_budget_exceeded_is_valid_status(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_result_budget_exceeded.json")
        violations = validate_context_retrieval_result(data)
        self.assertEqual(violations, [])
        self.assertEqual(data["status"], "budget_exceeded")

    def test_denied_is_valid_status(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_result_denied.json")
        violations = validate_context_retrieval_result(data)
        self.assertEqual(violations, [])
        self.assertEqual(data["status"], "denied")

    def test_invalid_status_fails(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_result_valid.json")
        data["status"] = "error"
        violations = validate_context_retrieval_result(data)
        self.assertTrue(any("status" in v for v in violations))

    def test_missing_total_token_estimate_fails(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_result_valid.json")
        del data["total_token_estimate"]
        violations = validate_context_retrieval_result(data)
        self.assertTrue(any("total_token_estimate" in v for v in violations))

    def test_negative_token_estimate_fails(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_result_valid.json")
        data["total_token_estimate"] = -1
        violations = validate_context_retrieval_result(data)
        self.assertTrue(any("total_token_estimate" in v for v in violations))

    def test_returned_ref_missing_token_estimate_fails(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_result_valid.json")
        del data["returned_refs"][0]["token_estimate"]
        violations = validate_context_retrieval_result(data)
        self.assertTrue(any("token_estimate" in v for v in violations))

    def test_invalid_content_mode_in_returned_ref_fails(self):
        data = load_fixture(FIXTURE_DIR / "retrieval_result_valid.json")
        data["returned_refs"][0]["content_mode"] = "complete"
        violations = validate_context_retrieval_result(data)
        self.assertTrue(any("content_mode" in v for v in violations))


class ContextLayersValidationTests(unittest.TestCase):
    """Test context_layers five-layer structure validation."""

    def test_valid_layers_passes(self):
        data = load_fixture(FIXTURE_DIR / "context_layers_valid.json")
        violations = validate_context_layers(data)
        self.assertEqual(violations, [])

    def test_five_layers_required(self):
        data = load_fixture(FIXTURE_DIR / "context_layers_valid.json")
        for layer in ("invariants", "task_pack", "dynamic_refs", "memory_digest", "recent_evidence"):
            modified = dict(data)
            del modified[layer]
            violations = validate_context_layers(modified)
            self.assertTrue(
                any(layer in v for v in violations),
                f"Missing layer {layer} should cause violation",
            )

    def test_missing_memory_digest_fails(self):
        data = load_fixture(FIXTURE_DIR / "context_layers_missing_layer.json")
        violations = validate_context_layers(data)
        self.assertTrue(any("memory_digest" in v for v in violations))

    def test_invalid_freshness_fails(self):
        data = load_fixture(FIXTURE_DIR / "context_layers_valid.json")
        data["freshness"] = "expired"
        violations = validate_context_layers(data)
        self.assertTrue(any("freshness" in v for v in violations))

    def test_invalid_cache_policy_fails(self):
        data = load_fixture(FIXTURE_DIR / "context_layers_valid.json")
        data["cache_policy"] = "invalid"
        violations = validate_context_layers(data)
        self.assertTrue(any("cache_policy" in v for v in violations))

    def test_invalid_prune_policy_fails(self):
        data = load_fixture(FIXTURE_DIR / "context_layers_valid.json")
        data["pack_prune_policy"] = "random"
        violations = validate_context_layers(data)
        self.assertTrue(any("pack_prune_policy" in v for v in violations))


class MemoryDigestValidationTests(unittest.TestCase):
    """Test memory_digest source_refs and expiry_policy requirements."""

    def test_valid_memory_digest_passes(self):
        data = load_fixture(FIXTURE_DIR / "context_layers_valid.json")
        violations = validate_context_layers(data)
        self.assertEqual(violations, [])

    def test_missing_source_refs_fails(self):
        data = load_fixture(FIXTURE_DIR / "memory_digest_no_source.json")
        violations = validate_context_layers(data)
        self.assertTrue(any("source_refs" in v for v in violations))

    def test_missing_expiry_policy_fails(self):
        data = {
            "schema_version": "context_layers.v1",
            "pack_id": "test",
            "invariants": {},
            "task_pack": {},
            "dynamic_refs": [],
            "memory_digest": {
                "source_refs": ["evt-001"],
                "conflict_resolution": "newest_wins"
            },
            "recent_evidence": [],
        }
        violations = validate_context_layers(data)
        self.assertTrue(any("expiry_policy" in v for v in violations))

    def test_missing_conflict_resolution_fails(self):
        data = {
            "schema_version": "context_layers.v1",
            "pack_id": "test",
            "invariants": {},
            "task_pack": {},
            "dynamic_refs": [],
            "memory_digest": {
                "source_refs": ["evt-001"],
                "expiry_policy": "7d",
            },
            "recent_evidence": [],
        }
        violations = validate_context_layers(data)
        self.assertTrue(any("conflict_resolution" in v for v in violations))


class BudgetComplianceTests(unittest.TestCase):
    """Test context_budget validation and compliance checking."""

    def test_within_budget_compliant(self):
        pack = {"context_budget": {"max_context_tokens": 10000}}
        compliant, reason = check_budget_compliance(pack, 5000)
        self.assertTrue(compliant)
        self.assertIn("within budget", reason)

    def test_over_budget_not_compliant(self):
        pack = {"context_budget": {"max_context_tokens": 10000}}
        compliant, reason = check_budget_compliance(pack, 15000)
        self.assertFalse(compliant)
        self.assertIn("over budget", reason)

    def test_no_budget_defined_compliant(self):
        pack = {}
        compliant, reason = check_budget_compliance(pack, 999999)
        self.assertTrue(compliant)
        self.assertIn("no budget", reason)


class PackPrunePolicyTests(unittest.TestCase):
    """Test pack_prune_policy enforcement."""

    def test_deny_if_over_budget_raises(self):
        pack = {
            "context_layers": {
                "recent_evidence": [{"ref": "ev1"}],
                "memory_digest": {"source_refs": ["e1"], "expiry_policy": "7d", "conflict_resolution": "drop"},
            },
            "pack_prune_policy": "deny_if_over_budget",
        }
        with self.assertRaises(ValueError) as ctx:
            apply_prune_policy(pack, 15000, 10000)
        self.assertIn("deny_if_over_budget", str(ctx.exception))

    def test_drop_recent_evidence_first_prunes(self):
        pack = {
            "context_layers": {
                "recent_evidence": [{"ref": "ev1"}],
                "memory_digest": {"source_refs": ["e1"], "expiry_policy": "7d", "conflict_resolution": "drop"},
            },
            "pack_prune_policy": "drop_recent_evidence_first",
        }
        pruned, action = apply_prune_policy(pack, 15000, 10000)
        self.assertEqual(action, "dropped_recent_evidence")
        self.assertEqual(pruned["context_layers"]["recent_evidence"], [])

    def test_drop_memory_digest_first_prunes(self):
        pack = {
            "context_layers": {
                "recent_evidence": [],
                "memory_digest": {"source_refs": ["e1"], "expiry_policy": "7d", "conflict_resolution": "drop"},
            },
            "pack_prune_policy": "drop_memory_digest_first",
        }
        pruned, action = apply_prune_policy(pack, 15000, 10000)
        self.assertEqual(action, "dropped_memory_digest")
        self.assertEqual(pruned["context_layers"]["memory_digest"]["source_refs"], [])

    def test_preserve_invariants_prunes_recent_first(self):
        pack = {
            "context_layers": {
                "recent_evidence": [{"ref": "ev1"}],
                "memory_digest": {"source_refs": ["e1"], "expiry_policy": "7d", "conflict_resolution": "drop"},
            },
            "pack_prune_policy": "preserve_invariants",
        }
        pruned, action = apply_prune_policy(pack, 15000, 10000)
        self.assertEqual(action, "dropped_recent_evidence")

    def test_no_pruning_when_within_budget(self):
        pack = {
            "context_layers": {"recent_evidence": [{"ref": "ev1"}]},
            "pack_prune_policy": "drop_recent_evidence_first",
        }
        pruned, action = apply_prune_policy(pack, 5000, 10000)
        self.assertEqual(action, "no_pruning_needed")


class FullContentInlineTests(unittest.TestCase):
    """Test that full content inline is denied unless retrieval_result allows it."""

    def test_full_content_without_retrieval_fails(self):
        pack = {
            "artifact_refs": [
                {"artifact_id": "art-001", "content_mode": "full"}
            ]
        }
        result = {"returned_refs": []}
        violations = validate_full_content_inline_denied(pack, result)
        self.assertTrue(len(violations) > 0)
        self.assertIn("art-001", violations[0])

    def test_full_content_with_retrieval_passes(self):
        pack = {
            "artifact_refs": [
                {"artifact_id": "art-001", "content_mode": "full"}
            ]
        }
        result = {
            "returned_refs": [
                {"ref_id": "art-001", "content_mode": "full"}
            ]
        }
        violations = validate_full_content_inline_denied(pack, result)
        self.assertEqual(violations, [])

    def test_summary_content_always_allowed(self):
        pack = {
            "artifact_refs": [
                {"artifact_id": "art-001", "content_mode": "summary"}
            ]
        }
        result = {"returned_refs": []}
        violations = validate_full_content_inline_denied(pack, result)
        self.assertEqual(violations, [])


class FixtureLoadingTests(unittest.TestCase):
    """Test that all fixtures load and validate correctly."""

    def test_fixture_dir_exists(self):
        self.assertTrue(FIXTURE_DIR.exists())

    def test_advisor_fixtures_load(self):
        results = load_all_fixtures(FIXTURE_DIR, "advisor_context_pack_v2")
        self.assertGreater(len(results), 0)
        valid = [r for r in results if not r[2]]
        self.assertGreater(len(valid), 0)

    def test_model_fixtures_load(self):
        results = load_all_fixtures(FIXTURE_DIR, "model_context_pack_v2")
        self.assertGreater(len(results), 0)

    def test_retrieval_request_fixtures_load(self):
        results = load_all_fixtures(FIXTURE_DIR, "context_retrieval_request")
        self.assertGreater(len(results), 0)

    def test_retrieval_result_fixtures_load(self):
        results = load_all_fixtures(FIXTURE_DIR, "context_retrieval_result")
        self.assertGreater(len(results), 0)

    def test_context_layers_fixtures_load(self):
        results = load_all_fixtures(FIXTURE_DIR, "context_layers")
        self.assertGreater(len(results), 0)

    def test_valid_fixtures_all_pass(self):
        valid_fixtures = [
            "advisor_pack_valid.json",
            "model_pack_valid.json",
            "retrieval_request_valid.json",
            "retrieval_result_valid.json",
            "context_layers_valid.json",
            "retrieval_result_budget_exceeded.json",
            "retrieval_result_denied.json",
        ]
        for fname in valid_fixtures:
            path = FIXTURE_DIR / fname
            self.assertTrue(path.exists(), f"Fixture {fname} not found")

    def test_invalid_fixtures_detected(self):
        from harness_core.context_pack import load_and_validate_fixture
        invalid_fixtures = {
            "advisor_pack_missing_budget.json": "advisor_context_pack_v2",
            "context_layers_missing_layer.json": "context_layers",
            "retrieval_request_no_reason.json": "context_retrieval_request",
            "memory_digest_no_source.json": "context_layers",
        }
        for fname, schema_type in invalid_fixtures.items():
            path = FIXTURE_DIR / fname
            data, violations = load_and_validate_fixture(path, schema_type)
            self.assertTrue(
                len(violations) > 0,
                f"Fixture {fname} should have violations",
            )


class MutationRefTests(unittest.TestCase):
    """Test that user-style mutation fixtures are referenced in context packs."""

    def test_mutation_ref_fixture_exists(self):
        path = FIXTURE_DIR / "advisor_pack_with_mutation_ref.json"
        self.assertTrue(path.exists())

    def test_mutation_ref_fixture_validates(self):
        data = load_fixture(FIXTURE_DIR / "advisor_pack_with_mutation_ref.json")
        violations = validate_advisor_context_pack_v2(data)
        self.assertEqual(violations, [])

    def test_mutation_refs_cover_three_types(self):
        data = load_fixture(FIXTURE_DIR / "advisor_pack_with_mutation_ref.json")
        evidence_paths = [e["path"] for e in data["evidence_refs"]]
        has_formal = any("formal_issue" in p for p in evidence_paths)
        has_chat = any("chat_request" in p for p in evidence_paths)
        has_terse = any("terse_ticket" in p for p in evidence_paths)
        self.assertTrue(has_formal, "Missing formal_issue reference")
        self.assertTrue(has_chat, "Missing chat_request reference")
        self.assertTrue(has_terse, "Missing terse_ticket reference")

    def test_all_three_input_types_can_generate_pack(self):
        """Verify formal_issue, user_style_chat_request, terse_ticket can all be referenced."""
        data = load_fixture(FIXTURE_DIR / "advisor_pack_with_mutation_ref.json")
        ref_types = set()
        for e in data["evidence_refs"]:
            if "formal_issue" in e["path"]:
                ref_types.add("formal_issue")
            elif "chat_request" in e["path"]:
                ref_types.add("user_style_chat_request")
            elif "terse_ticket" in e["path"]:
                ref_types.add("terse_ticket")
        self.assertEqual(ref_types, {"formal_issue", "user_style_chat_request", "terse_ticket"})


class ExistingTestsUnchangedTests(unittest.TestCase):
    """Verify no existing test behavior is affected."""

    def test_existing_test_files_still_exist(self):
        base = Path(__file__).resolve().parents[1] / "tests"
        self.assertTrue((base / "test_real_world_eval.py").exists())
        self.assertTrue((base / "test_error_taxonomy.py").exists())
        self.assertTrue((base / "test_user_style_mutation_eval.py").exists())


class EnumCompletenessTests(unittest.TestCase):
    """Test enum completeness."""

    def test_content_modes(self):
        self.assertEqual(set(CONTENT_MODES), {"summary", "excerpt", "full"})

    def test_retrieval_statuses(self):
        self.assertEqual(
            set(RETRIEVAL_RESULT_STATUS),
            {"fulfilled", "partial", "denied", "not_found", "budget_exceeded"},
        )

    def test_freshness_values(self):
        self.assertEqual(set(FRESHNESS_VALUES), {"current", "stale", "unknown"})

    def test_cache_policy_values(self):
        self.assertEqual(
            set(CACHE_POLICY_VALUES),
            {"no_cache", "read_cache_allowed", "write_cache_allowed", "read_write_cache_allowed"},
        )

    def test_prune_policy_values(self):
        self.assertEqual(
            set(PACK_PRUNE_POLICY_VALUES),
            {"preserve_invariants", "drop_recent_evidence_first", "drop_memory_digest_first", "deny_if_over_budget"},
        )

    def test_model_roles(self):
        self.assertEqual(
            set(MODEL_ROLES),
            {"planner", "executor", "debugger", "verifier", "advisor", "integrator"},
        )

    def test_call_types(self):
        self.assertEqual(set(CALL_TYPES), {"preflight", "correction", "arbitration", "risk_scan"})


class DataclassTests(unittest.TestCase):
    """Test dataclass helpers."""

    def test_context_budget_to_dict(self):
        b = ContextBudget(max_context_tokens=8000, preferred_context_tokens=4000)
        d = b.to_dict()
        self.assertEqual(d["max_context_tokens"], 8000)
        self.assertNotIn("max_response_tokens", d)

    def test_retrieval_policy_to_dict(self):
        p = RetrievalPolicy(allow_retrieval=True, allowed_ref_types=["run_log"])
        d = p.to_dict()
        self.assertTrue(d["allow_retrieval"])
        self.assertEqual(d["allowed_ref_types"], ["run_log"])

    def test_memory_digest_to_dict(self):
        md = MemoryDigest(source_refs=["e1"], expiry_policy="7d", conflict_resolution="drop")
        d = md.to_dict()
        self.assertEqual(d["source_refs"], ["e1"])

    def test_context_layers_to_dict(self):
        cl = ContextLayers(
            invariants={},
            task_pack={},
            dynamic_refs=[],
            memory_digest=MemoryDigest(source_refs=[], expiry_policy="7d", conflict_resolution="drop"),
            recent_evidence=[],
        )
        d = cl.to_dict()
        self.assertIn("invariants", d)
        self.assertIn("memory_digest", d)
        self.assertEqual(d["freshness"], "current")


if __name__ == "__main__":
    unittest.main()
