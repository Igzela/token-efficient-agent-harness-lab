from __future__ import annotations

import copy
import hashlib
import unittest

from scripts.fault_drill_contract import (
    ContractError,
    build_report_v2,
    build_result_v2,
    canonical_json_bytes,
    parse_json_bytes,
    scenario_sha256_v2,
    validate_owner_evidence_v2,
    validate_report_v2,
    validate_result_v2,
    validate_scenario_v2,
)
from scripts.fault_drill_registry import get_scenario, scenario_for


class FaultDrillContractV2Tests(unittest.TestCase):
    def setUp(self) -> None:
        self.scenario = scenario_for(
            get_scenario("pe6.storage.sqlite.atomicity.v2"),
            source_head="a" * 40,
            seed=9,
            worker_id=2,
        )

    def owner_evidence(self) -> dict[str, object]:
        return {
            "schema_version": "fault_owner_evidence.v2",
            "scenario_id": self.scenario["scenario_id"],
            "scenario_version": self.scenario["scenario_version"],
            "scenario_sha256": scenario_sha256_v2(self.scenario),
            "source_head": self.scenario["source_head"],
            "fault": {
                "fault_id": self.scenario["fault"]["fault_id"],
                "injection_point": self.scenario["fault"]["injection_point"],
            },
            "owner": {
                "identity": self.scenario["owner"],
                "resource_ids": [item["resource_id"] for item in self.scenario["resources"]],
            },
            "observed_state_before_fault": "one pending disposable SQLite run existed",
            "observed_fault": "a duplicate terminal replay was attempted",
            "observed_recovery_or_refusal": "the storage owner refused duplicate authority",
            "checks": [
                {"name": "pe6.sqlite.replay_refused", "category": "recovery", "outcome": "passed", "observation": "the replay returned an error"},
                {"name": "pe6.sqlite.rollback_unavailable", "category": "rollback", "outcome": "unsupported", "observation": "no release rollback was in scope"},
                {"name": "pe6.sqlite.integrity_ok", "category": "integrity", "outcome": "passed", "observation": "database integrity remained ok"},
                {"name": "pe6.sqlite.audit_present", "category": "audit", "outcome": "passed", "observation": "a bounded audit row was observed"},
                {"name": "pe6.sqlite.restart_safe", "category": "restart", "outcome": "passed", "observation": "reopen retained one terminal state"},
                {"name": "pe6.sqlite.cleanup_done", "category": "cleanup", "outcome": "passed", "observation": "the disposable directory was absent"},
            ],
            "cleanup": {"outcome": "passed", "observation": "the owner directory was removed"},
        }

    def passed_result(self) -> dict[str, object]:
        evidence = self.owner_evidence()
        return build_result_v2(
            scenario=self.scenario,
            configured_timeout_ms=self.scenario["timeout_ms"],
            observed_duration_ms=17,
            owner_exit_code=0,
            owner_evidence=evidence,
            owner_evidence_sha256=hashlib.sha256(canonical_json_bytes(evidence)).hexdigest(),
            status="passed",
            reason_codes=["DRILL_PASSED", "OWNER_EVIDENCE_VERIFIED"],
            harness_cleanup={"outcome": "passed", "observation": "harness directory was removed"},
        )

    def test_duplicate_keys_and_bounds_fail_closed(self) -> None:
        with self.assertRaises(ContractError):
            parse_json_bytes(b'{"a":1,"a":2}')
        with self.assertRaises(ContractError):
            parse_json_bytes(b'{"a":"' + b"x" * 2050 + b'"}')

    def test_scenario_v1_is_not_reinterpreted_as_v2(self) -> None:
        legacy = copy.deepcopy(self.scenario)
        legacy["schema_version"] = "fault_scenario.v1"
        legacy["scenario_version"] = "v1"
        with self.assertRaises(ContractError):
            validate_scenario_v2(legacy)

    def test_owner_bindings_and_generic_checks_fail_closed(self) -> None:
        evidence = self.owner_evidence()
        evidence["fault"]["fault_id"] = "pe6.fault.other.v2"
        with self.assertRaises(ContractError):
            validate_owner_evidence_v2(evidence, self.scenario)

        evidence = self.owner_evidence()
        evidence["checks"][0]["name"] = "recovery_invariant"
        with self.assertRaises(ContractError):
            validate_owner_evidence_v2(evidence, self.scenario)

    def test_unsupported_category_remains_explicit(self) -> None:
        result = self.passed_result()
        self.assertEqual(result["category_outcomes"]["rollback"], "unsupported")
        self.assertNotEqual(result["category_outcomes"]["rollback"], "passed")

    def test_zero_exit_or_canned_claim_without_valid_evidence_cannot_pass(self) -> None:
        with self.assertRaises(ContractError):
            build_result_v2(
                scenario=self.scenario,
                configured_timeout_ms=self.scenario["timeout_ms"],
                observed_duration_ms=0,
                owner_exit_code=0,
                owner_evidence=None,
                owner_evidence_sha256=None,
                status="passed",
                reason_codes=["DRILL_PASSED"],
                harness_cleanup={"outcome": "passed", "observation": "harness directory removed"},
            )
        canned = self.owner_evidence()
        canned["observed_recovery_or_refusal"] = "fixed owner test completed successfully"
        with self.assertRaises(ContractError):
            validate_owner_evidence_v2(canned, self.scenario)

        unsupported = self.owner_evidence()
        for check in unsupported["checks"]:
            if check["category"] == "recovery":
                check["outcome"] = "unsupported"
        with self.assertRaises(ContractError):
            build_result_v2(
                scenario=self.scenario,
                configured_timeout_ms=self.scenario["timeout_ms"],
                observed_duration_ms=17,
                owner_exit_code=0,
                owner_evidence=unsupported,
                owner_evidence_sha256=hashlib.sha256(
                    canonical_json_bytes(unsupported)
                ).hexdigest(),
                status="passed",
                reason_codes=["DRILL_PASSED"],
                harness_cleanup={
                    "outcome": "passed",
                    "observation": "harness directory removed",
                },
            )

    def test_result_and_report_tamper_fail_closed(self) -> None:
        result = self.passed_result()
        changed = copy.deepcopy(result)
        changed["category_outcomes"]["rollback"] = "passed"
        with self.assertRaises(ContractError):
            validate_result_v2(changed, self.scenario)
        changed = copy.deepcopy(result)
        changed["owner_evidence"]["observed_fault"] = "different emitted evidence"
        with self.assertRaises(ContractError):
            validate_result_v2(changed, self.scenario)
        report = build_report_v2(suite="storage", source_head="a" * 40, results=[result])
        self.assertEqual(validate_report_v2(report), report)
        report["summary"]["total"] = 9
        with self.assertRaises(ContractError):
            validate_report_v2(report)


if __name__ == "__main__":
    unittest.main()
