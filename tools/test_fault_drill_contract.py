from __future__ import annotations

import copy
import unittest

from scripts.fault_drill_contract import (
    ContractError,
    build_report,
    parse_json_bytes,
    validate_result,
    validate_scenario,
    validate_report,
)
from scripts.fault_drill_harness import _make_result
from scripts.fault_drill_registry import get_scenario, scenario_for


class FaultDrillContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.scenario = scenario_for(
            get_scenario("pe6.storage.sqlite.atomicity.v1"),
            source_head="a" * 40,
            seed=9,
            worker_id=2,
        )

    def _passed_result(self) -> dict[str, object]:
        return _make_result(
            self.scenario,
            status="passed",
            reason_codes=[
                "DRILL_PASSED",
                "RECOVERY_VERIFIED",
                "ROLLBACK_VERIFIED",
                "INTEGRITY_VERIFIED",
                "AUDIT_VERIFIED",
                "CLEANUP_VERIFIED",
            ],
            outcome="passed",
            invariant_passed=True,
            cleanup_outcome="cleaned",
            cleanup_passed=True,
            detection_reason="DRILL_PASSED",
            detected=True,
            duration_ms=1,
            observation="fixed owner test completed successfully",
        )

    def test_canonical_duplicate_keys_and_bounds_fail_closed(self) -> None:
        with self.assertRaises(ContractError):
            parse_json_bytes(b'{"a":1,"a":2}')
        with self.assertRaises(ContractError):
            parse_json_bytes(b'{"a":"' + (b"x" * 2050) + b'"}')

    def test_scenario_rejects_unknown_fault_and_non_disposable_resource(self) -> None:
        unknown = copy.deepcopy(self.scenario)
        unknown["fault"]["injection_point"] = "arbitrary_process_kill"
        with self.assertRaises(ContractError):
            validate_scenario(unknown)

        non_disposable = copy.deepcopy(self.scenario)
        non_disposable["resources"][0]["disposable"] = False
        with self.assertRaises(ContractError):
            validate_scenario(non_disposable)

    def test_result_evidence_tamper_and_cleanup_omission_fail_closed(self) -> None:
        result = self._passed_result()
        tampered = copy.deepcopy(result)
        tampered["recovery_evidence"]["observations"][0] = "changed after sealing"
        with self.assertRaises(ContractError):
            validate_result(tampered, self.scenario)

        omitted_cleanup = copy.deepcopy(result)
        omitted_cleanup["cleanup_evidence"]["outcome"] = "failed"
        with self.assertRaises(ContractError):
            validate_result(omitted_cleanup, self.scenario)

    def test_result_resources_and_evidence_references_remain_bound(self) -> None:
        result = self._passed_result()
        result["environment"] = {"name": "linux-sqlite", "capabilities": ["filesystem"]}
        with self.assertRaises(ContractError):
            validate_result(result, self.scenario)

        result = self._passed_result()
        result["evidence_refs"][0]["kind"] = "audit"
        with self.assertRaises(ContractError):
            validate_result(result, self.scenario)

    def test_unsupported_is_explicit_and_never_passes(self) -> None:
        unsupported = _make_result(
            self.scenario,
            status="unsupported",
            reason_codes=["UNSUPPORTED_ENVIRONMENT"],
            outcome="unsupported",
            invariant_passed=False,
            cleanup_outcome="cleaned",
            cleanup_passed=True,
            detection_reason="UNSUPPORTED_ENVIRONMENT",
            detected=False,
            duration_ms=0,
            observation="capability is unavailable",
        )
        self.assertEqual(validate_result(unsupported, self.scenario)["status"], "unsupported")
        self.assertNotEqual(unsupported["status"], "passed")

    def test_deterministic_result_and_report_hash(self) -> None:
        first = self._passed_result()
        second = self._passed_result()
        self.assertEqual(first, second)
        report_a = build_report(
            suite="storage",
            source_head="a" * 40,
            seed=9,
            worker_id=2,
            environment={"name": "linux-sqlite", "capabilities": ["filesystem", "sqlite", "rust_engine"]},
            results=[first],
        )
        report_b = build_report(
            suite="storage",
            source_head="a" * 40,
            seed=9,
            worker_id=2,
            environment={"name": "linux-sqlite", "capabilities": ["filesystem", "sqlite", "rust_engine"]},
            results=[second],
        )
        self.assertEqual(report_a, report_b)
        self.assertEqual(validate_report(report_a), report_a)


if __name__ == "__main__":
    unittest.main()
