from __future__ import annotations

import copy
import concurrent.futures
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "efficiency_live_benchmark.py"
SPEC = importlib.util.spec_from_file_location("efficiency_live_benchmark", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = MODULE
SPEC.loader.exec_module(MODULE)


def measurement(
    value: int | float | bool | None,
    *,
    provenance: str = "harness_derived",
    reason: str | None = None,
) -> dict[str, Any]:
    unavailable = value is None
    return {
        "schema_version": MODULE.MEASUREMENT_SCHEMA_VERSION,
        "value": value,
        "provenance": "unavailable" if unavailable else provenance,
        "completeness": "unavailable" if unavailable else "complete",
        "confidence": "unavailable" if unavailable else "high",
        "unavailable_reason": reason if unavailable else None,
    }


def scorecard(
    runtime: str,
    runtime_version: str,
    run_id: str,
    scenario: str,
    state_strategy: str,
    contract: dict[str, Any],
    *,
    input_tokens: int,
    output_tokens: int = 10,
    context_tokens: int | None = None,
    repeated_tokens: int = 5,
    tool_calls: int = 2,
    redundant_tool_calls: int = 0,
    duration_ms: int = 25,
    quality: float = 1.0,
) -> dict[str, Any]:
    context = input_tokens if context_tokens is None else context_tokens
    pricing_input = contract["input_cost_per_1k_usd"]
    pricing_output = contract["output_cost_per_1k_usd"]
    cost = round(input_tokens * pricing_input / 1000 + output_tokens * pricing_output / 1000, 6)
    return {
        "schema_version": "token_efficiency_scorecard.v1",
        "adapter_run_id": run_id,
        "runtime_kind": runtime,
        "runtime_version": runtime_version,
        "scenario_id": scenario,
        "mode": "native_control_plane" if runtime == "native_harness" else "external_runtime",
        "state_strategy": state_strategy,
        "status": "pass",
        "pass_fail_reason": "bounded fixture passed",
        "quality_score": quality,
        "quality_method": "rule",
        "comparison_contract": {
            "scenario_digest": "1" * 64,
            "task_digest": "2" * 64,
            "runtime_kind": runtime,
            "runtime_version": runtime_version,
            "provider_id": contract["provider_id"],
            "model_id": contract["model_id"],
            "tokenizer_id": contract["tokenizer_id"],
            "pricing_id": contract["pricing_id"],
            "input_cost_per_1k_usd": pricing_input,
            "output_cost_per_1k_usd": pricing_output,
            "quality_method": "rule",
            "quality_threshold": contract["quality_threshold"],
            "evaluator_version": contract["evaluator_version"],
            "redaction_policy": "summary-only.v1",
            "retry_policy": contract["retry_policy"],
            "seed": contract["seed"],
        },
        "input_token_total": input_tokens,
        "output_token_total": output_tokens,
        "context_token_total": context,
        "repeated_context_token_total": repeated_tokens,
        "retrieved_ref_token_total": 0 if state_strategy == "full_history" else min(8, context),
        "tool_call_count": tool_calls,
        "redundant_tool_call_count": redundant_tool_calls,
        "retry_count": 0,
        "step_count": 0,
        "duration_ms": duration_ms,
        "estimated_cost_usd": cost,
        "raw_trace_artifact_id": f"bounded-{runtime}-{run_id}",
        "redaction_status": "not_needed",
    }


def strategy_metrics(card: dict[str, Any], mode: str) -> dict[str, Any]:
    provider_provenance = "provider_reported" if mode == "live" else "tokenizer_exact"
    values: dict[str, tuple[Any, str]] = {
        "input_tokens": (card["input_token_total"], provider_provenance),
        "output_tokens": (card["output_token_total"], provider_provenance),
        "cached_tokens": (0, provider_provenance),
        "cache_write_tokens": (0, provider_provenance),
        "reasoning_tokens": (0, provider_provenance),
        "context_tokens": (card["context_token_total"], "tokenizer_exact"),
        "repeated_context_tokens": (card["repeated_context_token_total"], "tokenizer_exact"),
        "retrieval_candidate_count": (3, "harness_derived"),
        "retrieval_selected_count": (2, "harness_derived"),
        "retrieval_precision": (1.0, "harness_derived"),
        "retrieval_recall": (1.0, "harness_derived"),
        "stale_memory_selection_rate": (0.0, "harness_derived"),
        "correction_conflict_rate": (0.0, "harness_derived"),
        "state_read_bytes": (128, "harness_derived"),
        "state_write_bytes": (64, "harness_derived"),
        "memory_maintenance_tokens": (4, "tokenizer_exact"),
        "memory_maintenance_cost_usd": (0.0, "estimated"),
        "tool_call_count": (card["tool_call_count"], "harness_derived"),
        "redundant_tool_calls": (card["redundant_tool_call_count"], "harness_derived"),
        "retries": (card["retry_count"], "harness_derived"),
        "latency_ms": (card["duration_ms"], "harness_derived"),
        "cost_usd": (card["estimated_cost_usd"], "estimated"),
        "quality": (card["quality_score"], "harness_derived"),
        "restart_persistence": (True, "harness_derived"),
    }
    assert set(values) == set(MODULE.MATERIAL_METRICS)
    return {name: measurement(value, provenance=provenance) for name, (value, provenance) in values.items()}


def tool_metrics(card: dict[str, Any], variant: str) -> dict[str, Any]:
    prompt_tokens = 100 if variant == "static_all" else 55
    reduction = 0.0 if variant == "static_all" else 0.45
    values = {
        "required_tool_recall": 1.0,
        "incorrect_tool_selection": 0,
        "prompt_tokens": prompt_tokens,
        "prompt_token_reduction": reduction,
        "quality": card["quality_score"],
        "latency_ms": card["duration_ms"],
        "cost_usd": card["estimated_cost_usd"],
    }
    return {name: measurement(value) for name, value in values.items()}


def runtime_result(
    runtime: str,
    request: dict[str, Any],
    *,
    mutate: Any = None,
) -> dict[str, Any]:
    runtime_version = f"{runtime}-fixture.v1"
    contract = request["comparison_contract"]
    mode = request["mode"]
    inputs = [100, 82, 68, 60]
    strategy_results = []
    for index, strategy in enumerate(MODULE.PRIMARY_STRATEGIES):
        card = scorecard(
            runtime,
            runtime_version,
            f"{runtime}-memory-{index}",
            "bounded-memory-efficiency",
            MODULE.SCORECARD_STATE_STRATEGIES[strategy],
            contract,
            input_tokens=inputs[index],
        )
        strategy_results.append(
            {
                "strategy_id": strategy,
                "scorecard": card,
                "metrics": strategy_metrics(card, mode),
                "evidence_references": [
                    {"source_id": f"fixture-source-{index}", "source_sha256": str(index + 3) * 64}
                ],
            }
        )

    descriptors = [
        {"tool_id": "read", "descriptor_sha256": "a" * 64},
        {"tool_id": "search", "descriptor_sha256": "b" * 64},
        {"tool_id": "summarize", "descriptor_sha256": "c" * 64},
        {"tool_id": "write", "descriptor_sha256": "d" * 64},
    ]
    tool_results = []
    for index, variant in enumerate(MODULE.TOOL_VARIANTS):
        card = scorecard(
            runtime,
            runtime_version,
            f"{runtime}-tools-{index}",
            "bounded-tool-discovery",
            "none",
            contract,
            input_tokens=100 if variant == "static_all" else 55,
        )
        selected = (
            [{"tool_id": item["tool_id"], "score": 1.0} for item in descriptors]
            if variant == "static_all"
            else [
                {"tool_id": "read", "score": 0.9},
                {"tool_id": "search", "score": 0.8},
                {"tool_id": "summarize", "score": 0.7},
            ]
        )
        tool_results.append(
            {
                "variant": variant,
                "scorecard": card,
                "metrics": tool_metrics(card, variant),
                "corpus_sha256": "7" * 64,
                "registry_sha256": "8" * 64,
                "retriever_version": MODULE.CANONICAL_DEFINITION["tool_discovery"]["retriever_version"],
                "descriptor_hashes": descriptors,
                "selected_tools": selected,
            }
        )

    result = {
        "schema_version": MODULE.RUNTIME_RESULT_SCHEMA_VERSION,
        "runtime_kind": runtime,
        "runtime_version": runtime_version,
        "adapter_version": f"{runtime}-adapter.v1",
        "benchmark_run_id": request["benchmark_run_id"],
        "definition_sha256": MODULE.DEFINITION_SHA256,
        "request_sha256": MODULE.sha256_value(request),
        "comparison_contract": copy.deepcopy(contract),
        "limits": copy.deepcopy(request["limits"]),
        "external_provider_calls": 0 if mode == "fixture" else 8,
        "strategy_results": strategy_results,
        "tool_discovery_results": tool_results,
        "audit_evidence": {
            "schema_version": MODULE.AUDIT_EVIDENCE_SCHEMA_VERSION,
            "event_count": 0 if mode == "fixture" else 8,
            "evidence_sha256": "9" * 64,
            "store_kind": "fixture-memory" if mode == "fixture" else "local-audit-file",
        },
    }
    if mutate is not None:
        mutate(result)
    return result


class FakeRuntimeRunner:
    def __init__(self, mutators: dict[str, Any] | None = None) -> None:
        self.mutators = mutators or {}
        self.calls: list[tuple[list[str], dict[str, str]]] = []

    def __call__(self, command: list[str], **kwargs: Any) -> subprocess.CompletedProcess[bytes]:
        request_path = Path(command[command.index("--benchmark-request") + 1])
        output_path = Path(command[command.index("--benchmark-output") + 1])
        request = json.loads(request_path.read_text(encoding="utf-8"))
        runtime = "langgraph" if "langgraph" in Path(command[0]).name else "native_harness"
        result = runtime_result(runtime, request, mutate=self.mutators.get(runtime))
        output_path.write_text(json.dumps(result), encoding="utf-8")
        self.calls.append((command, kwargs["env"]))
        return subprocess.CompletedProcess(command, 0, b"", b"")


class EfficiencyLiveBenchmarkTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.native = self.root / "native-runtime"
        self.langgraph = self.root / "langgraph-runtime"
        for executable in (self.native, self.langgraph):
            executable.write_text("fixture executable", encoding="utf-8")
            executable.chmod(0o700)
        self.catalog_patcher = mock.patch.object(
            MODULE,
            "_fetch_bounded_catalog_json",
            side_effect=self.fake_catalog_fetch,
        )
        self.catalog_patcher.start()

    def tearDown(self) -> None:
        self.catalog_patcher.stop()
        self.temp.cleanup()

    @staticmethod
    def fake_catalog_fetch(url: str, _timeout: float) -> dict[str, Any]:
        if url == MODULE.OPENROUTER_MODELS_URL:
            return {
                "data": [
                    {
                        "id": MODULE.OPENROUTER_HY3_MODEL_ID,
                        "canonical_slug": MODULE.OPENROUTER_HY3_CANONICAL_ID,
                        "context_length": 262_144,
                        "supported_parameters": sorted(MODULE.OPENROUTER_REQUIRED_PARAMETERS),
                        "pricing": {"prompt": "0", "completion": "0"},
                    }
                ]
            }
        if url == MODULE.OPENROUTER_HY3_ENDPOINTS_URL:
            return {
                "data": {
                    "id": MODULE.OPENROUTER_HY3_MODEL_ID,
                    "endpoints": [
                        {
                            "provider_name": MODULE.OPENROUTER_HY3_PROVIDER,
                            "status": 0,
                            "context_length": 262_144,
                            "supported_parameters": sorted(MODULE.OPENROUTER_REQUIRED_PARAMETERS),
                            "pricing": {"prompt": "0", "completion": "0", "discount": 0},
                        }
                    ],
                }
            }
        raise AssertionError(f"unexpected catalog URL: {url}")

    def args(self, *extra: str) -> Any:
        return MODULE.build_parser().parse_args(
            [
                "--native-cli",
                str(self.native),
                "--langgraph-adapter",
                str(self.langgraph),
                "--output-root",
                str(self.root / "reports"),
                "--benchmark-run-id",
                "fixture-run-001",
                *extra,
            ]
        )

    def live_args(self, *extra: str) -> Any:
        audit_store = self.root / "audit.jsonl"
        return self.args(
            "--mode",
            "live",
            "--live-confirmation",
            MODULE.LIVE_CONFIRMATION,
            "--provider",
            "openai_compatible",
            "--provider-base-url",
            MODULE.OPENROUTER_BASE_URL,
            "--model",
            MODULE.OPENROUTER_HY3_MODEL_ID,
            "--tokenizer",
            "fixed-tokenizer-v1",
            "--credential-env",
            "TEST_PROVIDER_KEY",
            "--kill-switch-env",
            "TEST_KILL_SWITCH",
            "--audit-store",
            str(audit_store),
            "--pricing-id",
            "operator-fixed-pricing-v1",
            "--pricing-effective-date",
            "2026-07-15T00:00:00Z",
            "--input-cost-per-1k-usd",
            "0",
            "--output-cost-per-1k-usd",
            "0",
            *extra,
        )

    def test_definition_binds_exact_primary_strategies_and_tool_variants(self) -> None:
        self.assertEqual(MODULE.CANONICAL_DEFINITION["primary_strategies"], list(MODULE.PRIMARY_STRATEGIES))
        self.assertEqual(
            MODULE.CANONICAL_DEFINITION["tool_discovery"]["variants"], list(MODULE.TOOL_VARIANTS)
        )
        self.assertEqual(MODULE.DEFINITION_SHA256, MODULE.sha256_value(MODULE.CANONICAL_DEFINITION))

    def test_fixture_execution_writes_hash_bound_comparable_report(self) -> None:
        runner = FakeRuntimeRunner()
        report, path = MODULE.execute(self.args(), env={"PATH": os.environ.get("PATH", "")}, runner=runner)

        self.assertEqual(len(runner.calls), 2)
        self.assertTrue(path.is_file())
        self.assertEqual(report["acceptance_status"], "PASS")
        unsigned = copy.deepcopy(report)
        report_hash = unsigned.pop("report_sha256")
        self.assertEqual(report_hash, MODULE.sha256_value(unsigned))
        self.assertTrue(all(row["comparable"] for row in report["cross_runtime_comparisons"]))
        for runtime in MODULE.RUNTIME_KINDS:
            evidence = report["runtime_evidence"][runtime]
            self.assertEqual(len(evidence["strategy_results"]), 4)
            self.assertEqual(len(evidence["tool_discovery_results"]), 2)
            self.assertTrue(evidence["quality_and_efficiency"][-1]["efficiency_advantage_reported"])
            self.assertTrue(evidence["tool_comparison"]["efficiency_advantage_reported"])
        serialized = path.read_text(encoding="utf-8")
        self.assertNotIn(str(self.root), serialized)
        self.assertNotIn("raw_prompt", serialized)

    def test_identical_concurrent_fixture_reports_publish_once_idempotently(self) -> None:
        def execute_once(_: int) -> tuple[dict[str, Any], Path]:
            return MODULE.execute(
                self.args(),
                env={"PATH": os.environ.get("PATH", "")},
                runner=FakeRuntimeRunner(),
            )

        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
            results = list(executor.map(execute_once, range(4)))

        hashes = {report["report_sha256"] for report, _ in results}
        paths = {path for _, path in results}
        self.assertEqual(len(hashes), 1)
        self.assertEqual(len(paths), 1)
        self.assertEqual(len(list((self.root / "reports").glob("*.report.json"))), 1)

    def test_cross_runtime_contract_mismatch_has_exact_reason(self) -> None:
        def mismatch(result: dict[str, Any]) -> None:
            result["comparison_contract"]["model_id"] = "different-model"
            for item in result["strategy_results"] + result["tool_discovery_results"]:
                item["scorecard"]["comparison_contract"]["model_id"] = "different-model"

        report, _ = MODULE.execute(
            self.args(),
            env={"PATH": os.environ.get("PATH", "")},
            runner=FakeRuntimeRunner({"langgraph": mismatch}),
        )
        for comparison in report["cross_runtime_comparisons"]:
            self.assertFalse(comparison["comparable"])
            self.assertEqual(comparison["reason_codes"], ["comparison_contract.model_id_mismatch"])
        self.assertEqual(report["acceptance_status"], "INCOMPARABLE")

    def test_unavailable_metric_is_labeled_and_makes_comparison_incomparable(self) -> None:
        def unavailable(result: dict[str, Any]) -> None:
            result["strategy_results"][2]["metrics"]["context_tokens"] = measurement(
                None, reason="provider_context_usage_unavailable"
            )

        report, _ = MODULE.execute(
            self.args(),
            env={"PATH": os.environ.get("PATH", "")},
            runner=FakeRuntimeRunner({"langgraph": unavailable}),
        )
        comparison = report["cross_runtime_comparisons"][2]
        self.assertFalse(comparison["comparable"])
        self.assertEqual(comparison["reason_codes"], ["langgraph.context_tokens_unavailable"])
        self.assertEqual(report["acceptance_status"], "INCOMPARABLE")

    def test_quality_regression_fails_acceptance_even_when_tokens_improve(self) -> None:
        def lower_quality(result: dict[str, Any]) -> None:
            strategy = result["strategy_results"][3]
            strategy["scorecard"]["quality_score"] = 0.8
            strategy["metrics"]["quality"] = measurement(0.8)

        report, _ = MODULE.execute(
            self.args(),
            env={"PATH": os.environ.get("PATH", "")},
            runner=FakeRuntimeRunner({"native_harness": lower_quality}),
        )
        final_strategy = report["runtime_evidence"]["native_harness"]["quality_and_efficiency"][3]
        self.assertTrue(final_strategy["quality_regression"])
        self.assertFalse(final_strategy["efficiency_advantage_reported"])
        self.assertEqual(report["acceptance_status"], "FAIL")
        self.assertIn(
            "native_harness.durable_state_bounded_recent.quality_regression",
            report["quality_failure_reasons"],
        )

    def test_runtime_rejects_missing_strategy_and_nondeterministic_top_k(self) -> None:
        def missing_strategy(result: dict[str, Any]) -> None:
            result["strategy_results"].pop()

        def wrong_order(result: dict[str, Any]) -> None:
            result["tool_discovery_results"][1]["selected_tools"].reverse()

        for mutator, error in (
            (missing_strategy, "exactly four strategy results"),
            (wrong_order, "ordering is nondeterministic"),
        ):
            with self.subTest(error=error):
                with self.assertRaisesRegex(MODULE.BenchmarkError, error):
                    MODULE.execute(
                        self.args(),
                        env={"PATH": os.environ.get("PATH", "")},
                        runner=FakeRuntimeRunner({"langgraph": mutator}),
                    )

    def test_runtime_result_must_bind_exact_request_limits_and_declared_fields(self) -> None:
        def wrong_hash(result: dict[str, Any]) -> None:
            result["request_sha256"] = "0" * 64

        def wrong_limits(result: dict[str, Any]) -> None:
            result["limits"]["max_calls"] -= 1

        def extra_field(result: dict[str, Any]) -> None:
            result["untrusted_note"] = "ignored evidence must not be accepted"

        for mutator, error in (
            (wrong_hash, "request hash does not match"),
            (wrong_limits, "limits do not match"),
            (extra_field, "invalid fields"),
        ):
            with self.subTest(error=error):
                with self.assertRaisesRegex(MODULE.BenchmarkError, error):
                    MODULE.execute(
                        self.args(),
                        env={"PATH": os.environ.get("PATH", "")},
                        runner=FakeRuntimeRunner({"native_harness": mutator}),
                    )

    def test_tool_prompt_reduction_is_recomputed_from_bound_token_counts(self) -> None:
        def fabricated_reduction(result: dict[str, Any]) -> None:
            metric = result["tool_discovery_results"][1]["metrics"]["prompt_token_reduction"]
            metric["value"] = 0.99

        with self.assertRaisesRegex(MODULE.BenchmarkError, "does not match prompt_tokens"):
            MODULE.execute(
                self.args(),
                env={"PATH": os.environ.get("PATH", "")},
                runner=FakeRuntimeRunner({"native_harness": fabricated_reduction}),
            )

    def test_scorecard_contract_cannot_diverge_from_runtime_contract(self) -> None:
        def stale_scorecard(result: dict[str, Any]) -> None:
            result["strategy_results"][0]["scorecard"]["comparison_contract"]["pricing_id"] = "stale"

        with self.assertRaisesRegex(MODULE.BenchmarkError, "does not match the runtime request"):
            MODULE.execute(
                self.args(),
                env={"PATH": os.environ.get("PATH", "")},
                runner=FakeRuntimeRunner({"native_harness": stale_scorecard}),
            )

    def test_live_mode_refuses_ci_before_starting_a_runtime(self) -> None:
        runner = FakeRuntimeRunner()
        with self.assertRaisesRegex(MODULE.BenchmarkError, "forbidden in CI"):
            MODULE.execute(
                self.live_args(),
                env={"CI": "true", "TEST_PROVIDER_KEY": "opaque", "TEST_KILL_SWITCH": "0"},
                runner=runner,
            )
        self.assertEqual(runner.calls, [])

    def test_live_mode_requires_symbolic_credential_and_available_kill_switch(self) -> None:
        args = self.live_args()
        cases = (
            ({"TEST_KILL_SWITCH": "0"}, "credential environment reference is not populated"),
            ({"TEST_PROVIDER_KEY": "opaque"}, "available kill switch"),
            ({"TEST_PROVIDER_KEY": "opaque", "TEST_KILL_SWITCH": "1"}, "kill switch is active"),
        )
        for env, error in cases:
            with self.subTest(error=error):
                with self.assertRaisesRegex(MODULE.BenchmarkError, error):
                    MODULE.execute(args, env=env, runner=FakeRuntimeRunner())

    def test_live_mode_rejects_query_bearing_base_url(self) -> None:
        args = self.live_args(
            "--provider-base-url",
            "https://provider.example/v1?unexpected=value",
        )
        with self.assertRaisesRegex(MODULE.BenchmarkError, "credential-free HTTPS"):
            MODULE.execute(
                args,
                env={"TEST_PROVIDER_KEY": "opaque", "TEST_KILL_SWITCH": "0"},
                runner=FakeRuntimeRunner(),
            )

    def test_live_mode_accepts_explicit_zero_token_prices(self) -> None:
        report, _ = MODULE.execute(
            self.live_args(
                "--input-cost-per-1k-usd",
                "0",
                "--output-cost-per-1k-usd",
                "0",
            ),
            env={"TEST_PROVIDER_KEY": "opaque", "TEST_KILL_SWITCH": "0"},
            runner=FakeRuntimeRunner(),
        )

        contract = report["runtime_evidence"]["native_harness"]["comparison_contract"]
        self.assertEqual(contract["input_cost_per_1k_usd"], 0.0)
        self.assertEqual(contract["output_cost_per_1k_usd"], 0.0)
        evidence = report["catalog_evidence"]
        self.assertEqual(evidence["requested_model_id"], MODULE.OPENROUTER_HY3_MODEL_ID)
        self.assertEqual(evidence["canonical_model_id"], MODULE.OPENROUTER_HY3_CANONICAL_ID)
        self.assertEqual(
            contract["pricing_id"],
            f"openrouter-catalog-sha256:{evidence['evidence_sha256']}",
        )
        self.assertEqual(evidence["request_routing"]["max_price"]["request"], 0)

    def test_openrouter_catalog_pricing_accepts_known_extra_zero_fields(self) -> None:
        def fetch(url: str, timeout: float) -> dict[str, Any]:
            document = copy.deepcopy(self.fake_catalog_fetch(url, timeout))
            if url == MODULE.OPENROUTER_MODELS_URL:
                document["data"][0]["pricing"].update({"request": "0", "image": 0})
            return document

        evidence = MODULE._openrouter_hy3_catalog_evidence(self.live_args(), fetch)
        self.assertEqual(evidence["model_pricing"]["request"], 0.0)
        self.assertEqual(evidence["model_pricing"]["image"], 0.0)

    def test_openrouter_catalog_pricing_fails_closed(self) -> None:
        cases = (
            ("unknown", {"surprise_charge": "0"}, "unknown charge dimensions"),
            ("nonzero", {"request": "0.000001"}, "not completely free"),
            ("malformed", {"request": "not-a-price"}, "malformed prices"),
        )
        for name, added, error in cases:
            def fetch(url: str, timeout: float, added: dict[str, Any] = added) -> dict[str, Any]:
                document = copy.deepcopy(self.fake_catalog_fetch(url, timeout))
                if url == MODULE.OPENROUTER_MODELS_URL:
                    document["data"][0]["pricing"].update(added)
                return document

            with self.subTest(name=name), self.assertRaisesRegex(MODULE.BenchmarkError, error):
                MODULE._openrouter_hy3_catalog_evidence(self.live_args(), fetch)

    def test_openrouter_catalog_rejects_model_rotation_and_incomplete_pricing(self) -> None:
        def rotated(url: str, timeout: float) -> dict[str, Any]:
            document = copy.deepcopy(self.fake_catalog_fetch(url, timeout))
            if url == MODULE.OPENROUTER_MODELS_URL:
                document["data"][0]["canonical_slug"] = "tencent/hy3-rotated"
            return document

        with self.assertRaisesRegex(MODULE.BenchmarkError, "canonical model identity changed"):
            MODULE._openrouter_hy3_catalog_evidence(self.live_args(), rotated)

        def incomplete(url: str, timeout: float) -> dict[str, Any]:
            document = copy.deepcopy(self.fake_catalog_fetch(url, timeout))
            if url == MODULE.OPENROUTER_MODELS_URL:
                document["data"][0]["pricing"] = {"prompt": "0"}
            return document

        with self.assertRaisesRegex(MODULE.BenchmarkError, "missing prompt or completion"):
            MODULE._openrouter_hy3_catalog_evidence(self.live_args(), incomplete)

    def test_live_mode_rejects_negative_token_prices(self) -> None:
        with self.assertRaisesRegex(MODULE.BenchmarkError, "input price"):
            MODULE.execute(
                self.live_args("--input-cost-per-1k-usd", "-0.001"),
                env={"TEST_PROVIDER_KEY": "opaque", "TEST_KILL_SWITCH": "0"},
                runner=FakeRuntimeRunner(),
            )

    def test_bounded_live_execution_forwards_secret_only_to_children_and_not_report(self) -> None:
        runner = FakeRuntimeRunner()
        opaque_value = "opaque-test-provider-value"
        report, path = MODULE.execute(
            self.live_args(),
            env={
                "PATH": os.environ.get("PATH", ""),
                "TEST_PROVIDER_KEY": opaque_value,
                "TEST_KILL_SWITCH": "0",
                "UNRELATED_SECRET": "must-not-cross-the-child-boundary",
            },
            runner=runner,
        )

        self.assertEqual(len(runner.calls), 2)
        self.assertTrue(all(env["TEST_PROVIDER_KEY"] == opaque_value for _, env in runner.calls))
        self.assertTrue(all("UNRELATED_SECRET" not in env for _, env in runner.calls))
        serialized = path.read_text(encoding="utf-8")
        self.assertNotIn(opaque_value, serialized)
        self.assertNotIn("TEST_PROVIDER_KEY", serialized)
        self.assertEqual(report["execution_mode"], "live")
        for runtime in MODULE.RUNTIME_KINDS:
            metrics = report["runtime_evidence"][runtime]["strategy_results"][0]["metrics"]
            self.assertEqual(metrics["input_tokens"]["provenance"], "provider_reported")
            self.assertEqual(metrics["context_tokens"]["provenance"], "tokenizer_exact")


if __name__ == "__main__":
    unittest.main()
