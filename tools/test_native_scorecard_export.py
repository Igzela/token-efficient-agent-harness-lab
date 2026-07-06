from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = ROOT / "scripts" / "native_scorecard_export.py"
SPEC = importlib.util.spec_from_file_location("native_scorecard_export", SCRIPT_PATH)
assert SPEC is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def native_summary(status: str = "completed") -> dict[str, Any]:
    return {
        "runtime_version": "native-test-1",
        "scenario_id": "native_debug",
        "workflow_run": {
            "run_id": "run-native-1",
            "status": status,
            "duration_ms": 2500,
            "retry_count": 1,
        },
        "dispatch": {
            "dispatch_id": "dispatch-1",
            "scenario_id": "native_debug",
        },
        "evidence": {
            "artifact_id": "artifact-bounded-summary",
            "pass_fail_reason": "unit verification passed",
            "quality_method": "test",
            "quality_score": 0.8,
            "redaction_status": "redacted",
        },
        "metrics": {
            "tool_call_count": 4,
            "redundant_tool_call_count": 1,
            "repeated_context_token_total": 15,
            "retrieved_ref_token_total": 20,
            "estimated_cost_usd": 0.05,
        },
        "steps": [
            {
                "node_id": "plan",
                "agent_role": "planner",
                "operation_kind": "model_call",
                "status": "completed",
                "input_tokens": 100,
                "output_tokens": 20,
                "context_tokens": 80,
                "repeated_context_tokens": 10,
                "retrieved_refs_count": 1,
                "retrieved_ref_tokens": 15,
                "state_read_bytes": 128,
                "state_write_bytes": 64,
            },
            {
                "node_id": "verify",
                "agent_role": "executor",
                "tool_name": "unit_test",
                "tool_call_id": "tool-1",
                "status": "completed",
                "input_tokens": 50,
                "output_tokens": 10,
                "context_tokens": 40,
                "repeated_context_tokens": 5,
                "retrieved_refs_count": 1,
                "retrieved_ref_tokens": 5,
                "state_read_bytes": 256,
                "state_write_bytes": 128,
            },
        ],
    }


class NativeScorecardExportTests(unittest.TestCase):
    def test_exports_pass_run_as_valid_scorecard_artifact(self) -> None:
        scorecard = MODULE.VALIDATOR.import_scorecard(
            MODULE.native_summary_to_trace_summary(native_summary())
        )
        artifact = MODULE.build_artifact(scorecard)

        self.assertEqual(scorecard["schema_version"], "token_efficiency_scorecard.v1")
        self.assertEqual(scorecard["runtime_kind"], "native_harness")
        self.assertEqual(scorecard["derived_metrics"]["total_tokens"], 180)
        self.assertEqual(scorecard["derived_metrics"]["tokens_per_passing_run"], 180)
        self.assertEqual(artifact["artifact_kind"], "token_efficiency_scorecard")
        self.assertTrue(artifact["read_only"])
        self.assertEqual(artifact["scorecard"], scorecard)

    def test_failed_run_has_no_tokens_per_passing_run(self) -> None:
        summary = native_summary(status="failed")
        summary["evidence"]["pass_fail_reason"] = "unit verification failed"
        summary["evidence"]["quality_method"] = "none"

        scorecard = MODULE.VALIDATOR.import_scorecard(
            MODULE.native_summary_to_trace_summary(summary)
        )

        self.assertEqual(scorecard["status"], "fail")
        self.assertIsNone(scorecard["derived_metrics"]["tokens_per_passing_run"])

    def test_rejects_missing_native_run_identifier(self) -> None:
        summary = native_summary()
        del summary["workflow_run"]["run_id"]
        del summary["dispatch"]["dispatch_id"]

        with self.assertRaisesRegex(MODULE.NativeScorecardExportError, "missing required string"):
            MODULE.native_summary_to_trace_summary(summary)

    def test_rejects_raw_trace_fields_before_projection(self) -> None:
        summary = native_summary()
        summary["raw_output"] = "full model output must not be accepted"

        with self.assertRaisesRegex(MODULE.VALIDATOR.ScorecardError, "raw or sensitive trace"):
            MODULE.native_summary_to_trace_summary(summary)

    def test_rejects_redundant_tool_count_above_total(self) -> None:
        summary = native_summary()
        summary["metrics"]["redundant_tool_call_count"] = 5

        with self.assertRaisesRegex(MODULE.VALIDATOR.ScorecardError, "redundant_tool_call_count"):
            MODULE.VALIDATOR.import_scorecard(MODULE.native_summary_to_trace_summary(summary))

    def test_cli_scorecard_output_can_be_revalidated(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            input_path = Path(tmp) / "native.json"
            output_path = Path(tmp) / "scorecard.json"
            input_path.write_text(json.dumps(native_summary()), encoding="utf-8")

            code = MODULE.main([str(input_path), "--scorecard-only", "--output", str(output_path)])

            self.assertEqual(code, 0)
            scorecard = json.loads(output_path.read_text(encoding="utf-8"))
            self.assertEqual(MODULE.VALIDATOR.import_scorecard(scorecard), scorecard)


if __name__ == "__main__":
    unittest.main()
