from __future__ import annotations

import copy
import io
import json
import os
import subprocess
import sys
import unittest
from pathlib import Path
from unittest import mock


PACKAGE_ROOT = Path(__file__).resolve().parents[1]
SOURCE_ROOT = PACKAGE_ROOT / "src"
sys.path.insert(0, str(SOURCE_ROOT))

from acp_langgraph_adapter import adapter as MODULE  # noqa: E402


def benchmark() -> dict[str, object]:
    return {
        "definition_sha256": "1" * 64,
        "scenario_id": "canonical-memory-benchmark",
        "scenario_sha256": "2" * 64,
        "task_sha256": "3" * 64,
        "seed": 165,
        "quality_threshold": 0.9,
        "provider_id": "offline-fixture",
        "model_id": "deterministic-provider-v1",
        "tokenizer_id": "fixture-tokenizer-v1",
        "pricing_id": "fixture-pricing-v1",
        "required_reference_ids": ["ref-required"],
        "candidate_reference_ids": ["ref-required", "ref-unused"],
        "selected_reference_ids": ["ref-required"],
        "stale_reference_ids": [],
        "context_tokens": 700,
        "repeated_context_tokens": 100,
        "state_read_bytes": 128,
        "state_write_bytes": 64,
        "memory_maintenance_tokens": 20,
        "memory_maintenance_cost_usd": 0.00002,
        "tool_call_count": 2,
        "redundant_tool_call_count": 0,
    }


def provider_exchange(request: dict[str, object]) -> dict[str, object]:
    usage = {
        "input_tokens": 500,
        "output_tokens": 80,
        "cached_input_tokens": None,
        "cache_write_tokens": None,
        "reasoning_tokens": 15,
        "estimated_cost_usd": 0.001,
        "provider_reported_cost_usd": None,
        "latency_ms": 22,
        "retry_count": 0,
    }
    return {
        "exchange_id": "exchange-1",
        "invocation_id": request["invocation_id"],
        "scope_binding_sha256": request["scope_binding_sha256"],
        "provider_id": "provider-fixed",
        "model_id": "model-fixed",
        "response_sha256": "4" * 64,
        "typed_result": {
            "status": "pass",
            "decision_code": "quality-gate-pass",
            "selected_tool_ids": ["tool-required"],
            "quality_score": 0.95,
            "quality_method": "bounded-rule-v1",
        },
        "usage": usage,
        "metric_provenance": {
            field: (
                "unavailable"
                if value is None
                else "provider_reported"
                if field
                in {"input_tokens", "output_tokens", "reasoning_tokens", "latency_ms"}
                else "harness_derived"
            )
            for field, value in usage.items()
        },
    }


def request(
    strategy: str = "durable_state_bounded_recent",
    *,
    mode: str = "fixture",
    checkpoint: dict[str, object] | None = None,
) -> dict[str, object]:
    value: dict[str, object] = {
        "schema_version": MODULE.REQUEST_SCHEMA_VERSION,
        "invocation_id": "invocation-1",
        "tenant_id": "tenant-1",
        "workspace_id": "workspace-1",
        "run_id": "run-1",
        "workflow_id": "workflow-1",
        "node_id": "node-1",
        "thread_id": "thread-1",
        "attempt": 1,
        "mode": mode,
        "memory_strategy": strategy,
        "runtime": {
            "runtime_kind": "langgraph",
            "adapter_contract_version": MODULE.ADAPTER_CONTRACT_VERSION,
            "adapter_version": MODULE.ADAPTER_VERSION,
            "expected_langgraph_version": MODULE.LANGGRAPH_VERSION,
        },
        "scope_binding_sha256": "",
        "request_sha256": "",
        "checkpoint": checkpoint,
        "provider_exchange": None,
        "benchmark": benchmark(),
    }
    value["scope_binding_sha256"] = MODULE.scope_binding_sha256(value)
    if mode == "live":
        value["benchmark"]["provider_id"] = "provider-fixed"  # type: ignore[index]
        value["benchmark"]["model_id"] = "model-fixed"  # type: ignore[index]
        value["provider_exchange"] = provider_exchange(value)
    value["request_sha256"] = MODULE.request_sha256(value)
    return value


def rehash(value: dict[str, object]) -> None:
    value["scope_binding_sha256"] = MODULE.scope_binding_sha256(value)
    exchange = value.get("provider_exchange")
    if isinstance(exchange, dict):
        exchange["invocation_id"] = value["invocation_id"]
        exchange["scope_binding_sha256"] = value["scope_binding_sha256"]
    value["request_sha256"] = MODULE.request_sha256(value)


class AdapterContractTests(unittest.TestCase):
    def test_actual_langgraph_supports_all_four_exact_memory_strategies(self) -> None:
        expected = {
            "full_history",
            "summary_memory",
            "retrieval_memory",
            "durable_state_bounded_recent",
        }
        self.assertEqual(MODULE.MEMORY_STRATEGIES, expected)

        with (
            mock.patch(
                "socket.create_connection",
                side_effect=AssertionError("fixture attempted network access"),
            ),
            mock.patch.dict(
                os.environ,
                {"LANGSMITH_TRACING": "true", "LANGCHAIN_TRACING_V2": "true"},
            ),
        ):
            results = {
                strategy: MODULE.execute_request(request(strategy))
                for strategy in sorted(expected)
            }
            self.assertEqual(os.environ["LANGSMITH_TRACING"], "true")
            self.assertEqual(os.environ["LANGCHAIN_TRACING_V2"], "true")

        self.assertEqual(set(results), expected)
        for strategy, result in results.items():
            self.assertEqual(result["schema_version"], MODULE.RESULT_SCHEMA_VERSION)
            self.assertEqual(result["memory_strategy"], strategy)
            self.assertEqual(result["invocation_count"], 1)
            self.assertEqual(result["runtime"]["runtime_kind"], "langgraph")
            self.assertEqual(
                result["runtime"]["runtime_version"], MODULE.LANGGRAPH_VERSION
            )
            self.assertEqual(result["trace_summary"]["graph_invoke_count"], 1)
            self.assertEqual(result["trace_summary"]["provider_exchanges_consumed"], 0)
            self.assertEqual(result["trace_summary"]["adapter_provider_calls"], 0)
            self.assertEqual(result["trace_summary"]["adapter_network_calls"], 0)
            self.assertEqual(
                result["scorecard_summary"]["measurement_confidence"], "high"
            )
            self.assertEqual(
                result["scope_binding_sha256"],
                request(strategy)["scope_binding_sha256"],
            )
            material = dict(result)
            digest = material.pop("result_sha256")
            self.assertEqual(digest, MODULE.canonical_sha256(material))

        self.assertIsNone(
            results["full_history"]["checkpoint_next"]["state_summary"][
                "summary_digest"
            ]
        )
        self.assertEqual(
            results["retrieval_memory"]["checkpoint_next"]["state_summary"][
                "selected_reference_ids"
            ],
            ["ref-required"],
        )

    def test_graph_node_runs_exactly_once(self) -> None:
        calls = 0
        original = MODULE._execute_node

        def counted(state: MODULE.GraphState) -> dict[str, object]:
            nonlocal calls
            calls += 1
            return original(state)

        with mock.patch.object(MODULE, "_execute_node", side_effect=counted):
            result = MODULE.execute_request(request())

        self.assertEqual(calls, 1)
        self.assertEqual(result["invocation_count"], 1)

    def test_checkpoint_resume_is_hash_and_scope_bound(self) -> None:
        first = MODULE.execute_request(request("durable_state_bounded_recent"))
        resumed_request = request(
            "durable_state_bounded_recent",
            checkpoint=first["checkpoint_next"],
        )
        resumed_request["invocation_id"] = "invocation-2"
        resumed_request["attempt"] = 2
        rehash(resumed_request)

        resumed = MODULE.execute_request(resumed_request)

        self.assertTrue(resumed["scorecard_summary"]["restart_resumed"])
        self.assertEqual(resumed["checkpoint_next"]["version"], 2)
        self.assertEqual(
            resumed["checkpoint_next"]["parent_checkpoint_id"],
            first["checkpoint_next"]["checkpoint_id"],
        )
        self.assertEqual(resumed["checkpoint_next"]["state_summary"]["turn_count"], 2)

        cross_workspace = copy.deepcopy(resumed_request)
        cross_workspace["workspace_id"] = "workspace-other"
        rehash(cross_workspace)
        with self.assertRaisesRegex(
            MODULE.AdapterError, "checkpoint identity"
        ) as error:
            MODULE.execute_request(cross_workspace)
        self.assertEqual(error.exception.code, "checkpoint_scope_mismatch")

    def test_live_exchange_is_typed_ephemeral_and_does_not_read_credentials(
        self,
    ) -> None:
        live_request = request("retrieval_memory", mode="live")
        credential_name = "OPENAI_" + "API_KEY"
        fake_credential = "s" + "k-" + "do-not-read-this-credential"
        with mock.patch.dict(os.environ, {credential_name: fake_credential}):
            result = MODULE.execute_request(live_request)

        self.assertEqual(result["trace_summary"]["provider_exchanges_consumed"], 1)
        self.assertEqual(result["trace_summary"]["adapter_provider_calls"], 0)
        self.assertEqual(result["trace_summary"]["adapter_network_calls"], 0)
        self.assertEqual(result["scorecard_summary"]["input_tokens"], 500)
        self.assertEqual(
            result["scorecard_summary"]["metric_provenance"]["input_tokens"],
            "provider_reported",
        )
        self.assertIsNone(result["scorecard_summary"]["cached_input_tokens"])
        self.assertEqual(
            result["scorecard_summary"]["metric_provenance"]["cached_input_tokens"],
            "unavailable",
        )
        serialized = json.dumps(result)
        self.assertNotIn(fake_credential, serialized)
        for forbidden in (
            "raw_prompt",
            "raw_output",
            "transcript",
            "credential",
            "repository_content",
        ):
            self.assertNotIn(forbidden, serialized.lower())

    def test_rejects_unknown_raw_sensitive_secret_and_oversized_values(self) -> None:
        unknown = request()
        unknown["unexpected"] = True
        rehash(unknown)
        with self.assertRaises(MODULE.AdapterError) as error:
            MODULE.execute_request(unknown)
        self.assertEqual(error.exception.code, "unknown_field")

        raw = request()
        raw["benchmark"]["raw_prompt"] = "ignored"  # type: ignore[index]
        rehash(raw)
        with self.assertRaises(MODULE.AdapterError) as error:
            MODULE.execute_request(raw)
        self.assertEqual(error.exception.code, "sensitive_field")

        secret = request()
        secret["benchmark"]["scenario_id"] = "api" + "_key=do-not-store"  # type: ignore[index]
        rehash(secret)
        with self.assertRaises(MODULE.AdapterError) as error:
            MODULE.execute_request(secret)
        self.assertEqual(error.exception.code, "secret_shaped_value")

        oversized = request()
        oversized["benchmark"]["scenario_id"] = "x" * (MODULE.MAX_STRING_BYTES + 1)  # type: ignore[index]
        rehash(oversized)
        with self.assertRaises(MODULE.AdapterError) as error:
            MODULE.execute_request(oversized)
        self.assertEqual(error.exception.code, "string_too_large")

    def test_rejects_malformed_hashes_cross_binding_and_unsupported_strategy(
        self,
    ) -> None:
        bad_hash = request()
        bad_hash["request_sha256"] = "0" * 64
        with self.assertRaises(MODULE.AdapterError) as error:
            MODULE.execute_request(bad_hash)
        self.assertEqual(error.exception.code, "request_hash_mismatch")

        cross_scope = request()
        cross_scope["workspace_id"] = "workspace-other"
        cross_scope["request_sha256"] = MODULE.request_sha256(cross_scope)
        with self.assertRaises(MODULE.AdapterError) as error:
            MODULE.execute_request(cross_scope)
        self.assertEqual(error.exception.code, "scope_binding_mismatch")

        unsupported = request()
        unsupported["memory_strategy"] = "hybrid"
        rehash(unsupported)
        with self.assertRaises(MODULE.AdapterError) as error:
            MODULE.execute_request(unsupported)
        self.assertEqual(error.exception.code, "invalid_memory_strategy")

    def test_hash_validation_does_not_rewrite_legal_integer_numbers(self) -> None:
        value = request()
        value["benchmark"]["quality_threshold"] = 1  # type: ignore[index]
        value["benchmark"]["memory_maintenance_cost_usd"] = 0  # type: ignore[index]
        value["request_sha256"] = MODULE.request_sha256(value)

        result = MODULE.execute_request(value)

        self.assertEqual(result["scorecard_summary"]["quality_threshold"], 1)
        self.assertEqual(result["scorecard_summary"]["memory_maintenance_cost_usd"], 0)

    def test_request_hash_excludes_claim_and_ephemeral_exchange_fields(self) -> None:
        original = request(mode="live")
        rebound = copy.deepcopy(original)
        rebound["invocation_id"] = "invocation-other"
        rebound["provider_exchange"]["invocation_id"] = "invocation-other"  # type: ignore[index]
        rebound["provider_exchange"]["response_sha256"] = "9" * 64  # type: ignore[index]

        self.assertEqual(
            MODULE.request_sha256(original), MODULE.request_sha256(rebound)
        )

        changed_benchmark = copy.deepcopy(original)
        changed_benchmark["benchmark"]["seed"] = 166  # type: ignore[index]
        self.assertNotEqual(
            MODULE.request_sha256(original), MODULE.request_sha256(changed_benchmark)
        )

    def test_live_exchange_rejects_cross_invocation_and_false_zero_provenance(
        self,
    ) -> None:
        cross_invocation = request(mode="live")
        cross_invocation["provider_exchange"]["invocation_id"] = "invocation-other"  # type: ignore[index]
        cross_invocation["request_sha256"] = MODULE.request_sha256(cross_invocation)
        with self.assertRaises(MODULE.AdapterError) as error:
            MODULE.execute_request(cross_invocation)
        self.assertEqual(error.exception.code, "exchange_invocation_mismatch")

        false_zero = request(mode="live")
        exchange = false_zero["provider_exchange"]
        exchange["usage"]["cached_input_tokens"] = None  # type: ignore[index]
        exchange["metric_provenance"]["cached_input_tokens"] = "provider_reported"  # type: ignore[index]
        false_zero["request_sha256"] = MODULE.request_sha256(false_zero)
        with self.assertRaises(MODULE.AdapterError) as error:
            MODULE.execute_request(false_zero)
        self.assertEqual(error.exception.code, "provenance_value_mismatch")

    def test_fixture_forbids_provider_exchange_and_live_requires_one(self) -> None:
        fixture = request()
        fixture["provider_exchange"] = provider_exchange(fixture)
        fixture["request_sha256"] = MODULE.request_sha256(fixture)
        with self.assertRaises(MODULE.AdapterError) as error:
            MODULE.execute_request(fixture)
        self.assertEqual(error.exception.code, "fixture_exchange_forbidden")

        live = request(mode="live")
        live["provider_exchange"] = None
        live["request_sha256"] = MODULE.request_sha256(live)
        with self.assertRaises(MODULE.AdapterError) as error:
            MODULE.execute_request(live)
        self.assertEqual(error.exception.code, "invalid_type")


class AdapterProcessTests(unittest.TestCase):
    def test_stable_file_entrypoint_runs_one_fixture_invocation(self) -> None:
        runner = PACKAGE_ROOT / "runner.py"
        process = subprocess.run(
            [sys.executable, str(runner)],
            input=json.dumps(request()).encode(),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env={"PYTHONIOENCODING": "utf-8", "PYTHONDONTWRITEBYTECODE": "1"},
            check=False,
            timeout=10,
        )
        self.assertEqual(process.returncode, 0, process.stderr.decode())
        result = json.loads(process.stdout)
        self.assertEqual(result["trace_summary"]["graph_invoke_count"], 1)

    def test_cli_emits_exactly_one_result_line(self) -> None:
        process = subprocess.run(
            [sys.executable, "-m", "acp_langgraph_adapter"],
            input=json.dumps(request()).encode(),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env={**os.environ, "PYTHONPATH": str(SOURCE_ROOT)},
            check=False,
        )
        self.assertEqual(process.returncode, 0, process.stderr.decode())
        self.assertEqual(process.stderr, b"")
        self.assertEqual(len(process.stdout.splitlines()), 1)
        result = json.loads(process.stdout)
        self.assertEqual(result["schema_version"], MODULE.RESULT_SCHEMA_VERSION)

    def test_cli_failure_has_no_partial_stdout_or_input_echo(self) -> None:
        fake_credential = "s" + "k-" + "must-not-be-echoed-123456789"
        process = subprocess.run(
            [sys.executable, "-m", "acp_langgraph_adapter"],
            input=json.dumps({"raw_prompt": fake_credential}).encode(),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env={**os.environ, "PYTHONPATH": str(SOURCE_ROOT)},
            check=False,
        )
        self.assertEqual(process.returncode, 2)
        self.assertEqual(process.stdout, b"")
        error = json.loads(process.stderr)
        self.assertEqual(error["schema_version"], MODULE.ERROR_SCHEMA_VERSION)
        self.assertNotIn(fake_credential, process.stderr.decode())

    def test_parse_input_rejects_non_finite_numbers_and_byte_overflow(self) -> None:
        original_stdin = sys.stdin
        try:
            fake = io.TextIOWrapper(io.BytesIO(b'{"value":NaN}'), encoding="utf-8")
            sys.stdin = fake
            with self.assertRaises(MODULE.AdapterError) as error:
                MODULE._parse_input()
            self.assertEqual(error.exception.code, "invalid_json")
        finally:
            sys.stdin = original_stdin

        original_stdin = sys.stdin
        try:
            fake = io.TextIOWrapper(
                io.BytesIO(b"x" * (MODULE.MAX_INPUT_BYTES + 1)), encoding="utf-8"
            )
            sys.stdin = fake
            with self.assertRaises(MODULE.AdapterError) as error:
                MODULE._parse_input()
            self.assertEqual(error.exception.code, "request_too_large")
        finally:
            sys.stdin = original_stdin


if __name__ == "__main__":
    unittest.main()
