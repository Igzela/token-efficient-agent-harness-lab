from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SCRIPT_PATH = ROOT / "scripts" / "langgraph_trace_import.py"
SPEC = importlib.util.spec_from_file_location("langgraph_trace_import", SCRIPT_PATH)
assert SPEC is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def langgraph_summary(mode: str = "stateful_store", status: str = "pass") -> dict[str, Any]:
    quality_method = "test" if status == "pass" else "none"
    return {
        "schema_version": "token_efficiency_scorecard.v1",
        "adapter_run_id": f"lg-{mode}",
        "runtime_kind": "langgraph",
        "runtime_version": "0.2.x-bounded-summary",
        "scenario_id": "state_retention_baseline",
        "mode": mode,
        "state_strategy": "durable_state" if mode == "stateful_store" else "full_history",
        "status": status,
        "pass_fail_reason": "unit checks passed" if status == "pass" else "unit checks failed",
        "quality_score": 0.95 if status == "pass" else None,
        "quality_method": quality_method,
        "input_token_total": 7_200 if mode == "stateful_store" else 12_000,
        "output_token_total": 900,
        "context_token_total": 5_000 if mode == "stateful_store" else 10_000,
        "repeated_context_token_total": 600 if mode == "stateful_store" else 3_900,
        "retrieved_ref_token_total": 800 if mode == "stateful_store" else 0,
        "tool_call_count": 4,
        "redundant_tool_call_count": 0,
        "retry_count": 1,
        "step_count": 2,
        "duration_ms": 18_000 if mode == "stateful_store" else 22_000,
        "estimated_cost_usd": 0.12 if mode == "stateful_store" else 0.20,
        "raw_trace_artifact_id": f"bounded-langgraph-source-{mode}",
        "redaction_status": "redacted",
        "steps": [
            {
                "adapter_step_id": f"{mode}-planner",
                "adapter_run_id": f"lg-{mode}",
                "step_index": 0,
                "node_name": "planner",
                "agent_role": "planner",
                "operation_kind": "model_call",
                "input_tokens": 4_000 if mode == "stateful_store" else 7_000,
                "output_tokens": 500,
                "context_tokens": 3_000 if mode == "stateful_store" else 6_000,
                "repeated_context_tokens": 300 if mode == "stateful_store" else 2_200,
                "retrieved_refs_count": 1 if mode == "stateful_store" else 0,
                "retrieved_ref_tokens": 500 if mode == "stateful_store" else 0,
                "tool_name": None,
                "tool_call_id": None,
                "status": status,
                "error_kind": "none" if status == "pass" else "test_failure",
                "state_read_bytes": 1024 if mode == "stateful_store" else 0,
                "state_write_bytes": 512 if mode == "stateful_store" else 0,
            },
            {
                "adapter_step_id": f"{mode}-verify",
                "adapter_run_id": f"lg-{mode}",
                "step_index": 1,
                "node_name": "verify",
                "agent_role": "executor",
                "operation_kind": "tool_call",
                "input_tokens": 3_200 if mode == "stateful_store" else 5_000,
                "output_tokens": 400,
                "context_tokens": 2_000 if mode == "stateful_store" else 4_000,
                "repeated_context_tokens": 300 if mode == "stateful_store" else 1_700,
                "retrieved_refs_count": 1 if mode == "stateful_store" else 0,
                "retrieved_ref_tokens": 300 if mode == "stateful_store" else 0,
                "tool_name": "pytest",
                "tool_call_id": f"{mode}-tool-1",
                "status": status,
                "error_kind": "none" if status == "pass" else "test_failure",
                "state_read_bytes": 2048 if mode == "stateful_store" else 0,
                "state_write_bytes": 128 if mode == "stateful_store" else 0,
            },
        ],
    }


class LangGraphTraceImportTests(unittest.TestCase):
    def test_imports_legal_langgraph_stateful_summary(self) -> None:
        scorecard = MODULE.import_langgraph_scorecard(langgraph_summary("stateful_store"))

        self.assertEqual(scorecard["schema_version"], "token_efficiency_scorecard.v1")
        self.assertEqual(scorecard["runtime_kind"], "langgraph")
        self.assertEqual(scorecard["mode"], "stateful_store")
        self.assertEqual(scorecard["derived_metrics"]["total_tokens"], 8100)
        self.assertEqual(scorecard["derived_metrics"]["tokens_per_passing_run"], 8100)

    def test_imports_legal_langgraph_stateless_summary(self) -> None:
        scorecard = MODULE.import_langgraph_scorecard(langgraph_summary("stateless_reread"))

        self.assertEqual(scorecard["mode"], "stateless_reread")
        self.assertEqual(scorecard["state_strategy"], "full_history")
        self.assertEqual(scorecard["derived_metrics"]["total_tokens"], 12900)
        self.assertGreater(scorecard["derived_metrics"]["repeated_context_ratio"], 0)

    def test_rejects_raw_trace_and_sensitive_values(self) -> None:
        summary = langgraph_summary()
        summary["raw_trace"] = {"transcript": "do not store"}
        with self.assertRaisesRegex(MODULE.LangGraphTraceImportError, "raw or sensitive"):
            MODULE.import_langgraph_scorecard(summary)

        secret_summary = langgraph_summary()
        secret_summary["notes"] = "token=sk-abcdefghijklmnopqrstuvwxyz"
        with self.assertRaisesRegex(MODULE.LangGraphTraceImportError, "secret-shaped"):
            MODULE.import_langgraph_scorecard(secret_summary)

    def test_validates_schema_runtime_and_mode(self) -> None:
        bad_schema = langgraph_summary()
        bad_schema["schema_version"] = "other.v1"
        with self.assertRaisesRegex(MODULE.LangGraphTraceImportError, "schema_version"):
            MODULE.import_langgraph_scorecard(bad_schema)

        bad_runtime = langgraph_summary()
        bad_runtime["runtime_kind"] = "crewai"
        with self.assertRaisesRegex(MODULE.LangGraphTraceImportError, "runtime_kind"):
            MODULE.import_langgraph_scorecard(bad_runtime)

        bad_mode = langgraph_summary()
        bad_mode["mode"] = "native_control_plane"
        with self.assertRaisesRegex(MODULE.LangGraphTraceImportError, "mode"):
            MODULE.import_langgraph_scorecard(bad_mode)

    def test_compares_same_scenario_stateful_and_stateless(self) -> None:
        stateful = MODULE.import_langgraph_scorecard(langgraph_summary("stateful_store"))
        stateless = MODULE.import_langgraph_scorecard(langgraph_summary("stateless_reread"))

        comparison = MODULE.compare_scorecards([stateless, stateful])

        self.assertTrue(comparison["read_only"])
        self.assertEqual(comparison["scenario_id"], "state_retention_baseline")
        self.assertEqual(comparison["fields"], MODULE.COMPARISON_FIELDS)
        self.assertEqual(comparison["rows"][0]["mode"], "stateless_reread")
        self.assertEqual(comparison["rows"][0]["total_tokens"], 12900)
        self.assertEqual(comparison["rows"][1]["mode"], "stateful_store")
        self.assertEqual(comparison["rows"][1]["total_tokens"], 8100)
        for field in [
            "context_share",
            "repeated_context_ratio",
            "tool_call_count",
            "retry_count",
            "duration_ms",
            "status",
            "quality_method",
            "tokens_per_passing_run",
            "cost_per_passing_run",
        ]:
            self.assertIn(field, comparison["rows"][0])

    def test_failed_langgraph_run_has_null_passing_metrics(self) -> None:
        scorecard = MODULE.import_langgraph_scorecard(langgraph_summary("stateful_store", status="fail"))

        self.assertEqual(scorecard["status"], "fail")
        self.assertIsNone(scorecard["derived_metrics"]["tokens_per_passing_run"])
        self.assertIsNone(scorecard["derived_metrics"]["cost_per_passing_run"])

    def test_cli_import_and_compare_outputs_are_revalidatable(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            stateful_path = Path(tmp) / "stateful.json"
            stateless_path = Path(tmp) / "stateless.json"
            scorecard_path = Path(tmp) / "scorecard.json"
            comparison_path = Path(tmp) / "comparison.json"
            stateful_path.write_text(json.dumps(langgraph_summary("stateful_store")), encoding="utf-8")
            stateless_path.write_text(json.dumps(langgraph_summary("stateless_reread")), encoding="utf-8")

            self.assertEqual(MODULE.main([str(stateful_path), "--output", str(scorecard_path)]), 0)
            scorecard = json.loads(scorecard_path.read_text(encoding="utf-8"))
            self.assertEqual(scorecard, MODULE.import_langgraph_scorecard(scorecard))

            self.assertEqual(
                MODULE.main([
                    str(stateless_path),
                    str(stateful_path),
                    "--compare",
                    "--output",
                    str(comparison_path),
                ]),
                0,
            )
            comparison = json.loads(comparison_path.read_text(encoding="utf-8"))
            self.assertEqual(len(comparison["rows"]), 2)


if __name__ == "__main__":
    unittest.main()
