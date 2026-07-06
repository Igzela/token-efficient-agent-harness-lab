from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = ROOT / "scripts" / "token_efficiency_scorecard.py"
SPEC = importlib.util.spec_from_file_location("token_efficiency_scorecard", SCRIPT_PATH)
assert SPEC is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def valid_summary() -> dict[str, Any]:
    return {
        "adapter_run_id": "run-example",
        "runtime_kind": "langgraph",
        "runtime_version": "0.1.0",
        "scenario_id": "iterative_debug_basic",
        "mode": "stateful_store",
        "state_strategy": "durable_state",
        "status": "pass",
        "pass_fail_reason": "unit verification passed",
        "quality_score": 0.9,
        "quality_method": "test",
        "input_token_total": 12000,
        "output_token_total": 1800,
        "context_token_total": 9500,
        "repeated_context_token_total": 1100,
        "retrieved_ref_token_total": 2400,
        "tool_call_count": 18,
        "redundant_tool_call_count": 2,
        "retry_count": 1,
        "step_count": 2,
        "duration_ms": 42000,
        "estimated_cost_usd": 0.25,
        "raw_trace_artifact_id": "artifact-example",
        "redaction_status": "redacted",
        "steps": [
            {
                "adapter_step_id": "step-0",
                "adapter_run_id": "run-example",
                "step_index": 0,
                "node_name": "planner",
                "agent_role": "planner",
                "operation_kind": "model_call",
                "input_tokens": 5000,
                "output_tokens": 800,
                "context_tokens": 4000,
                "repeated_context_tokens": 400,
                "retrieved_refs_count": 2,
                "retrieved_ref_tokens": 1000,
                "tool_name": None,
                "tool_call_id": None,
                "status": "pass",
                "error_kind": "none",
                "state_read_bytes": 1024,
                "state_write_bytes": 512,
            },
            {
                "adapter_step_id": "step-1",
                "adapter_run_id": "run-example",
                "step_index": 1,
                "node_name": "verification",
                "agent_role": "executor",
                "operation_kind": "tool_call",
                "input_tokens": 7000,
                "output_tokens": 1000,
                "context_tokens": 5500,
                "repeated_context_tokens": 700,
                "retrieved_refs_count": 1,
                "retrieved_ref_tokens": 1400,
                "tool_name": "test_runner",
                "tool_call_id": "tool-1",
                "status": "pass",
                "error_kind": "none",
                "state_read_bytes": 2048,
                "state_write_bytes": 1024,
            },
        ],
    }


class TokenEfficiencyScorecardTests(unittest.TestCase):
    def test_valid_summary_imports_with_derived_metrics(self) -> None:
        scorecard = MODULE.import_scorecard(valid_summary())

        self.assertEqual(scorecard["schema_version"], MODULE.SCHEMA_VERSION)
        self.assertEqual(scorecard["derived_metrics"]["total_tokens"], 13800)
        self.assertEqual(scorecard["derived_metrics"]["tokens_per_passing_run"], 13800)
        self.assertEqual(len(scorecard["steps"]), 2)

    def test_rejects_redundant_tool_count_above_total(self) -> None:
        summary = valid_summary()
        summary["redundant_tool_call_count"] = 19

        with self.assertRaisesRegex(MODULE.ScorecardError, "redundant_tool_call_count"):
            MODULE.import_scorecard(summary)

    def test_rejects_raw_trace_fields(self) -> None:
        summary = valid_summary()
        summary["debug"] = {"raw_prompt": "full prompt should not be stored"}

        with self.assertRaisesRegex(MODULE.ScorecardError, "raw or sensitive trace"):
            MODULE.import_scorecard(summary)

    def test_passing_run_requires_quality_method(self) -> None:
        summary = valid_summary()
        summary["quality_method"] = "none"

        with self.assertRaisesRegex(MODULE.ScorecardError, "passing runs require"):
            MODULE.import_scorecard(summary)

    def test_step_count_must_match_supplied_steps(self) -> None:
        summary = valid_summary()
        summary["step_count"] = 3

        with self.assertRaisesRegex(MODULE.ScorecardError, "step_count"):
            MODULE.import_scorecard(summary)

    def test_failing_run_has_no_tokens_per_passing_run(self) -> None:
        summary = valid_summary()
        summary["status"] = "fail"
        summary["pass_fail_reason"] = "verification failed"

        scorecard = MODULE.import_scorecard(summary)

        self.assertIsNone(scorecard["derived_metrics"]["tokens_per_passing_run"])
        self.assertIsNone(scorecard["derived_metrics"]["cost_per_passing_run"])


if __name__ == "__main__":
    unittest.main()
