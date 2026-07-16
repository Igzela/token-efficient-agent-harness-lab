#!/usr/bin/env python3
"""Run the canonical native/LangGraph efficiency benchmark.

This module is an operator orchestrator, not another scheduler or evidence
store.  It passes one hash-bound definition to two explicitly selected runtime
commands, validates their bounded scorecards and summary evidence, and writes a
single hash-bound comparison report.  Fixture mode is deterministic and live
mode is deliberately difficult to enable accidentally.
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import math
import os
import re
import stat
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
import uuid
from datetime import datetime, timezone
from decimal import Decimal, InvalidOperation
from pathlib import Path
from typing import Any, Callable, Mapping


ROOT = Path(__file__).resolve().parents[1]
VALIDATOR_PATH = ROOT / "scripts" / "token_efficiency_scorecard.py"
VALIDATOR_SPEC = importlib.util.spec_from_file_location(
    "efficiency_benchmark_scorecard_validator", VALIDATOR_PATH
)
assert VALIDATOR_SPEC is not None and VALIDATOR_SPEC.loader is not None
VALIDATOR = importlib.util.module_from_spec(VALIDATOR_SPEC)
sys.modules[VALIDATOR_SPEC.name] = VALIDATOR
VALIDATOR_SPEC.loader.exec_module(VALIDATOR)


DEFINITION_SCHEMA_VERSION = "efficiency_benchmark_definition.v1"
RUNTIME_RESULT_SCHEMA_VERSION = "efficiency_benchmark_runtime_result.v1"
REPORT_SCHEMA_VERSION = "efficiency_benchmark_report.v1"
MEASUREMENT_SCHEMA_VERSION = "efficiency_measurement.v1"
AUDIT_EVIDENCE_SCHEMA_VERSION = "efficiency_benchmark_audit_evidence.v1"
LIVE_CONFIRMATION = "I_CONFIRM_BOUNDED_LIVE_PROVIDER_COSTS"
FIXTURE_PRICING_EFFECTIVE_DATE = "2026-01-01T00:00:00Z"
MAX_JSON_BYTES = 1_048_576
MAX_STRING_BYTES = 2_048
MAX_ARRAY_ITEMS = 256
MAX_OBJECT_FIELDS = 128
MAX_DEPTH = 16
MAX_PER_CALL_COST_USD = 0.05
MAX_RUN_COST_USD = 0.50
MAX_DAILY_COST_USD = 2.00
MAX_TIMEOUT_SECONDS = 60.0
MAX_CALLS = 64
MAX_TOKENS = 200_000
MAX_OUTPUT_TOKENS = 1_024
OPENROUTER_BASE_URL = "https://openrouter.ai/api/v1"
OPENROUTER_MODELS_URL = f"{OPENROUTER_BASE_URL}/models"
OPENROUTER_HY3_ENDPOINTS_URL = (
    f"{OPENROUTER_BASE_URL}/models/tencent/hy3:free/endpoints"
)
OPENROUTER_HY3_MODEL_ID = "tencent/hy3:free"
OPENROUTER_HY3_CANONICAL_ID = "tencent/hy3-20260706"
OPENROUTER_HY3_PROVIDER = "Novita"
OPENROUTER_REQUIRED_PARAMETERS = {
    "max_tokens",
    "structured_outputs",
    "tool_choice",
    "tools",
}
OPENROUTER_KNOWN_PRICE_FIELDS = {
    "prompt",
    "completion",
    "request",
    "image",
    "web_search",
    "internal_reasoning",
    "input_cache_read",
    "input_cache_write",
    "discount",
}
OPENROUTER_REQUIRED_PRICE_FIELDS = OPENROUTER_KNOWN_PRICE_FIELDS - {"discount"}

PRIMARY_STRATEGIES = (
    "full_history",
    "summary_memory",
    "retrieval_memory",
    "durable_state_bounded_recent",
)
SCORECARD_STATE_STRATEGIES = {
    "full_history": "full_history",
    "summary_memory": "memory_digest",
    "retrieval_memory": "retrieval_refs",
    "durable_state_bounded_recent": "mixed",
}
TOOL_VARIANTS = ("static_all", "deterministic_top_k")
RUNTIME_KINDS = ("native_harness", "langgraph")
PROVENANCE_VALUES = {
    "provider_reported",
    "tokenizer_exact",
    "harness_derived",
    "estimated",
    "unavailable",
}
COMPLETENESS_VALUES = {"complete", "partial", "unavailable"}
CONFIDENCE_VALUES = {"high", "medium", "low", "unavailable"}

MATERIAL_METRICS = (
    "input_tokens",
    "output_tokens",
    "cached_tokens",
    "cache_write_tokens",
    "reasoning_tokens",
    "context_tokens",
    "repeated_context_tokens",
    "retrieval_candidate_count",
    "retrieval_selected_count",
    "retrieval_precision",
    "retrieval_recall",
    "stale_memory_selection_rate",
    "correction_conflict_rate",
    "state_read_bytes",
    "state_write_bytes",
    "memory_maintenance_tokens",
    "memory_maintenance_cost_usd",
    "tool_call_count",
    "redundant_tool_calls",
    "retries",
    "latency_ms",
    "cost_usd",
    "quality",
    "restart_persistence",
)
TOOL_METRICS = (
    "required_tool_recall",
    "incorrect_tool_selection",
    "prompt_tokens",
    "prompt_token_reduction",
    "quality",
    "latency_ms",
    "cost_usd",
)
RATIO_METRICS = {
    "retrieval_precision",
    "retrieval_recall",
    "stale_memory_selection_rate",
    "correction_conflict_rate",
    "quality",
    "required_tool_recall",
    "prompt_token_reduction",
}
SHARED_COMPARISON_FIELDS = (
    "definition_sha256",
    "provider_id",
    "model_id",
    "tokenizer_id",
    "pricing_id",
    "pricing_effective_date",
    "currency",
    "input_cost_per_1k_usd",
    "output_cost_per_1k_usd",
    "seed",
    "retry_policy",
    "evaluator_version",
    "output_limit_tokens",
    "quality_threshold",
)
REQUIRED_CROSS_RUNTIME_METRICS = (
    "input_tokens",
    "output_tokens",
    "context_tokens",
    "latency_ms",
    "cost_usd",
    "quality",
)

_ID = re.compile(r"^[a-zA-Z0-9][a-zA-Z0-9._:-]{0,127}$")
_ENV_NAME = re.compile(r"^[A-Z][A-Z0-9_]{1,127}$")
_SHA256 = re.compile(r"^[0-9a-f]{64}$")
_RFC3339 = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$"
)
_HTTPS_BASE_URL = re.compile(
    r"https://"
    r"(?:[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)"
    r"(?:\.(?:[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?))*"
    r"(?::([1-9][0-9]{0,4}))?"
    r"(?:/[A-Za-z0-9._~!$&'()*+,;=:@%/-]*)?"
)
_SECRET_PATTERNS = (
    re.compile(r"\bsk-[A-Za-z0-9_-]{12,}\b"),
    re.compile(r"\bgh[pousr]_[A-Za-z0-9_]{16,}\b"),
    re.compile(r"(?:api[_-]?key|auth[_-]?token|password)\s*[:=]\s*\S+", re.I),
)
_PRIVATE_PATH_PATTERNS = (
    re.compile(r"(^|\s)/(?:home|Users)/[^\s]+"),
    re.compile(r"\b[A-Za-z]:\\Users\\[^\s]+"),
)
_FORBIDDEN_KEYS = {
    "raw_prompt",
    "raw_output",
    "transcript",
    "checkpoint",
    "checkpoint_content",
    "credential",
    "credential_value",
    "api_key",
    "password",
    "secret",
    "repository_content",
    "repo_content",
    "private_path",
}


CANONICAL_DEFINITION: dict[str, Any] = {
    "schema_version": DEFINITION_SCHEMA_VERSION,
    "benchmark_id": "native_langgraph_memory_tool_efficiency_v1",
    "primary_strategies": list(PRIMARY_STRATEGIES),
    "memory_scenario": {
        "scenario_id": "bounded_cross_run_reference_resolution_v1",
        "required_reference_ids": ["benchmark-ref-current", "benchmark-ref-correction"],
        "stale_reference_ids": ["benchmark-ref-stale"],
        "conflicting_reference_ids": ["benchmark-ref-conflict"],
        "quality_method": "deterministic_reference_rule.v1",
        "quality_threshold": 0.90,
        "allowed_quality_regression": 0.0,
        "seed": 165,
        "output_limit_tokens": 256,
    },
    "tool_discovery": {
        "scenario_registry_version": "tool_discovery_scenarios.v1",
        "variants": list(TOOL_VARIANTS),
        "retriever_version": "deterministic_descriptor_overlap.v1",
        "top_k": 3,
        "tie_break": "score_desc_then_tool_id_asc",
        "tasks": [
            {
                "task_id": "inspect_bounded_evidence",
                "required_tool_ids": ["read"],
                "acceptable_tool_ids": ["read", "search"],
                "forbidden_tool_ids": ["write", "shell"],
            },
            {
                "task_id": "locate_relevant_reference",
                "required_tool_ids": ["search"],
                "acceptable_tool_ids": ["read", "search"],
                "forbidden_tool_ids": ["write", "shell"],
            },
        ],
    },
}


class BenchmarkError(ValueError):
    """Raised when execution or evidence fails the bounded contract."""


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req: Any, fp: Any, code: int, msg: str, headers: Any, newurl: str) -> None:
        return None


def _fetch_bounded_catalog_json(url: str, timeout_seconds: float) -> dict[str, Any]:
    if url not in {OPENROUTER_MODELS_URL, OPENROUTER_HY3_ENDPOINTS_URL}:
        raise BenchmarkError("catalog URL is outside the fixed OpenRouter allowlist")
    request = urllib.request.Request(
        url,
        headers={"Accept": "application/json", "Accept-Encoding": "identity"},
        method="GET",
    )
    opener = urllib.request.build_opener(_NoRedirect())
    try:
        with opener.open(request, timeout=min(timeout_seconds, 15.0)) as response:
            if response.status != 200:
                raise BenchmarkError(f"OpenRouter catalog returned HTTP {response.status}")
            declared = response.headers.get("Content-Length")
            if declared is not None and int(declared) > MAX_JSON_BYTES:
                raise BenchmarkError("OpenRouter catalog exceeds the response-size ceiling")
            content_encoding = response.headers.get("Content-Encoding")
            if content_encoding not in (None, "", "identity"):
                raise BenchmarkError("OpenRouter catalog returned an unexpected content encoding")
            encoded = response.read(MAX_JSON_BYTES + 1)
    except urllib.error.HTTPError as exc:
        if 300 <= exc.code <= 399:
            raise BenchmarkError("OpenRouter catalog redirect refused") from exc
        raise BenchmarkError(f"OpenRouter catalog returned HTTP {exc.code}") from exc
    except (urllib.error.URLError, OSError, ValueError) as exc:
        raise BenchmarkError("OpenRouter catalog request failed") from exc
    if not encoded or len(encoded) > MAX_JSON_BYTES:
        raise BenchmarkError("OpenRouter catalog response is empty or oversized")
    try:
        value = json.loads(encoded.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise BenchmarkError("OpenRouter catalog response is malformed") from exc
    if not isinstance(value, dict):
        raise BenchmarkError("OpenRouter catalog response must be an object")
    return value


CatalogFetcher = Callable[[str, float], dict[str, Any]]


def _zero_pricing(value: Any, field: str) -> dict[str, float]:
    if not isinstance(value, dict) or not value:
        raise BenchmarkError(f"{field} pricing is missing")
    unknown = set(value) - OPENROUTER_KNOWN_PRICE_FIELDS
    if unknown:
        raise BenchmarkError(f"{field} pricing contains unknown charge dimensions")
    missing = OPENROUTER_REQUIRED_PRICE_FIELDS - set(value)
    if missing:
        raise BenchmarkError(f"{field} pricing is missing modeled charge dimensions")
    normalized: dict[str, float] = {}
    for name, raw in value.items():
        if isinstance(raw, bool):
            raise BenchmarkError(f"{field} pricing contains malformed prices")
        try:
            parsed = Decimal(str(raw))
        except (InvalidOperation, TypeError, ValueError) as exc:
            raise BenchmarkError(f"{field} pricing contains malformed prices") from exc
        if not parsed.is_finite() or parsed != Decimal(0):
            raise BenchmarkError(f"{field} pricing is not completely free")
        normalized[name] = 0.0
    return {name: normalized[name] for name in sorted(normalized)}


def _openrouter_hy3_catalog_evidence(
    args: argparse.Namespace,
    fetcher: CatalogFetcher,
) -> dict[str, Any]:
    if args.provider_base_url.rstrip("/") != OPENROUTER_BASE_URL:
        raise BenchmarkError("free Hy3 live execution requires the official OpenRouter base URL")
    if args.model != OPENROUTER_HY3_MODEL_ID:
        raise BenchmarkError(f"free Hy3 live execution requires {OPENROUTER_HY3_MODEL_ID}")
    if args.input_cost_per_1k_usd != 0.0 or args.output_cost_per_1k_usd != 0.0:
        raise BenchmarkError("free Hy3 live execution requires explicit zero token prices")
    models = fetcher(OPENROUTER_MODELS_URL, float(args.timeout_seconds))
    rows = models.get("data")
    if not isinstance(rows, list):
        raise BenchmarkError("OpenRouter model catalog is missing data")
    matches = [row for row in rows if isinstance(row, dict) and row.get("id") == args.model]
    if len(matches) != 1:
        raise BenchmarkError("OpenRouter catalog does not contain exactly one requested Hy3 model")
    model = matches[0]
    if model.get("canonical_slug") != OPENROUTER_HY3_CANONICAL_ID:
        raise BenchmarkError("OpenRouter Hy3 canonical model identity changed")
    context_length = model.get("context_length")
    if not isinstance(context_length, int) or isinstance(context_length, bool):
        raise BenchmarkError("OpenRouter Hy3 context length is malformed")
    if context_length < args.max_tokens + args.output_limit_tokens:
        raise BenchmarkError("OpenRouter Hy3 context is smaller than the benchmark contract")
    parameters = model.get("supported_parameters")
    if not isinstance(parameters, list) or not all(isinstance(item, str) for item in parameters):
        raise BenchmarkError("OpenRouter Hy3 capabilities are malformed")
    if not OPENROUTER_REQUIRED_PARAMETERS.issubset(parameters):
        raise BenchmarkError("OpenRouter Hy3 lacks required benchmark capabilities")
    model_pricing = _zero_pricing(model.get("pricing"), "OpenRouter model")

    endpoint_document = fetcher(OPENROUTER_HY3_ENDPOINTS_URL, float(args.timeout_seconds))
    endpoint_data = endpoint_document.get("data")
    if not isinstance(endpoint_data, dict) or endpoint_data.get("id") != args.model:
        raise BenchmarkError("OpenRouter endpoint catalog model identity changed")
    endpoints = endpoint_data.get("endpoints")
    if not isinstance(endpoints, list):
        raise BenchmarkError("OpenRouter Hy3 endpoints are missing")
    matching_endpoints = [
        endpoint
        for endpoint in endpoints
        if isinstance(endpoint, dict)
        and endpoint.get("provider_name") == OPENROUTER_HY3_PROVIDER
        and endpoint.get("status") == 0
    ]
    if len(matching_endpoints) != 1:
        raise BenchmarkError("OpenRouter Hy3 does not have exactly one healthy Novita endpoint")
    endpoint = matching_endpoints[0]
    endpoint_context = endpoint.get("context_length")
    if not isinstance(endpoint_context, int) or endpoint_context < args.max_tokens + args.output_limit_tokens:
        raise BenchmarkError("OpenRouter Hy3 endpoint context is insufficient")
    endpoint_parameters = endpoint.get("supported_parameters")
    if not isinstance(endpoint_parameters, list) or not OPENROUTER_REQUIRED_PARAMETERS.issubset(
        endpoint_parameters
    ):
        raise BenchmarkError("OpenRouter Hy3 endpoint lacks required benchmark capabilities")
    endpoint_pricing = _zero_pricing(endpoint.get("pricing"), "OpenRouter endpoint")
    observed_at = datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    evidence = {
        "schema_version": "openrouter_live_catalog_evidence.v1",
        "observed_at": observed_at,
        "requested_model_id": args.model,
        "canonical_model_id": OPENROUTER_HY3_CANONICAL_ID,
        "context_length": context_length,
        "required_capabilities": sorted(OPENROUTER_REQUIRED_PARAMETERS),
        "model_pricing": model_pricing,
        "endpoint_provider": OPENROUTER_HY3_PROVIDER,
        "endpoint_context_length": endpoint_context,
        "endpoint_pricing": endpoint_pricing,
        "request_routing": {
            "only": [OPENROUTER_HY3_PROVIDER],
            "allow_fallbacks": False,
            "require_parameters": True,
            "max_price": {"completion": 0, "image": 0, "prompt": 0, "request": 0},
        },
        "source_urls": [OPENROUTER_MODELS_URL, OPENROUTER_HY3_ENDPOINTS_URL],
    }
    evidence["evidence_sha256"] = sha256_value(evidence)
    return evidence


def canonical_json_bytes(value: Any) -> bytes:
    return (
        json.dumps(
            value,
            ensure_ascii=True,
            sort_keys=True,
            separators=(",", ":"),
            allow_nan=False,
        )
        + "\n"
    ).encode("utf-8")


def sha256_value(value: Any) -> str:
    return hashlib.sha256(canonical_json_bytes(value)).hexdigest()


DEFINITION_SHA256 = sha256_value(CANONICAL_DEFINITION)


def _bounded_string(value: Any, field: str, *, maximum: int = MAX_STRING_BYTES) -> str:
    if not isinstance(value, str) or not value or len(value.encode("utf-8")) > maximum:
        raise BenchmarkError(f"{field} must be a bounded non-empty string")
    return value


def _identifier(value: Any, field: str) -> str:
    text = _bounded_string(value, field, maximum=128)
    if not _ID.fullmatch(text):
        raise BenchmarkError(f"{field} must be a bounded identifier")
    return text


def _sha(value: Any, field: str) -> str:
    text = _bounded_string(value, field, maximum=64)
    if not _SHA256.fullmatch(text):
        raise BenchmarkError(f"{field} must be a lowercase SHA-256")
    return text


def _finite_number(value: Any, field: str, *, minimum: float = 0.0) -> float | int:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise BenchmarkError(f"{field} must be numeric")
    if not math.isfinite(float(value)) or float(value) < minimum:
        raise BenchmarkError(f"{field} must be finite and at least {minimum}")
    return value


def _check_json_bounds(value: Any, field: str = "$", depth: int = 0) -> None:
    if depth > MAX_DEPTH:
        raise BenchmarkError(f"{field} exceeds the JSON depth bound")
    if isinstance(value, str):
        if len(value.encode("utf-8")) > MAX_STRING_BYTES:
            raise BenchmarkError(f"{field} exceeds the string bound")
    elif value is None or isinstance(value, bool):
        return
    elif isinstance(value, (int, float)):
        if isinstance(value, float) and not math.isfinite(value):
            raise BenchmarkError(f"{field} contains a non-finite number")
    elif isinstance(value, list):
        if len(value) > MAX_ARRAY_ITEMS:
            raise BenchmarkError(f"{field} exceeds the array bound")
        for index, item in enumerate(value):
            _check_json_bounds(item, f"{field}[{index}]", depth + 1)
    elif isinstance(value, dict):
        if len(value) > MAX_OBJECT_FIELDS:
            raise BenchmarkError(f"{field} exceeds the object-field bound")
        for key, item in value.items():
            if not isinstance(key, str) or len(key.encode("utf-8")) > 128:
                raise BenchmarkError(f"{field} contains an invalid key")
            _check_json_bounds(item, f"{field}.{key}", depth + 1)
    else:
        raise BenchmarkError(f"{field} contains an unsupported JSON value")


def _reject_sensitive(value: Any, field: str = "$", *, allow_source_ref: bool = False) -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            normalized = key.lower()
            if normalized in _FORBIDDEN_KEYS and not (
                allow_source_ref and normalized == "raw_trace_artifact_id"
            ):
                raise BenchmarkError(f"{field} contains forbidden sensitive field {key}")
            _reject_sensitive(item, f"{field}.{key}", allow_source_ref=allow_source_ref)
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _reject_sensitive(item, f"{field}[{index}]", allow_source_ref=allow_source_ref)
    elif isinstance(value, str):
        if any(pattern.search(value) for pattern in _SECRET_PATTERNS):
            raise BenchmarkError(f"{field} contains a secret-shaped value")
        if any(pattern.search(value) for pattern in _PRIVATE_PATH_PATTERNS):
            raise BenchmarkError(f"{field} contains a private path")


def _load_bounded_json(path: Path) -> dict[str, Any]:
    try:
        flags = os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0)
        descriptor = os.open(path, flags)
    except OSError as exc:
        raise BenchmarkError("runtime did not produce its bounded result") from exc
    try:
        with os.fdopen(descriptor, "rb") as handle:
            if not stat.S_ISREG(os.fstat(handle.fileno()).st_mode):
                raise BenchmarkError("runtime result must be a regular file")
            encoded = handle.read(MAX_JSON_BYTES + 1)
    except (OSError, BenchmarkError):
        raise
    if not encoded or len(encoded) > MAX_JSON_BYTES:
        raise BenchmarkError("runtime result exceeds the bounded size")
    try:
        value = json.loads(encoded.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise BenchmarkError("runtime result is not valid bounded JSON") from exc
    if not isinstance(value, dict):
        raise BenchmarkError("runtime result must be an object")
    _check_json_bounds(value)
    _reject_sensitive(value, allow_source_ref=True)
    return value


def _normalize_measurement(value: Any, metric: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise BenchmarkError(f"metric {metric} must be an evidence object")
    expected = {"schema_version", "value", "provenance", "completeness", "confidence", "unavailable_reason"}
    if set(value) != expected:
        raise BenchmarkError(f"metric {metric} has invalid evidence fields")
    if value.get("schema_version") != MEASUREMENT_SCHEMA_VERSION:
        raise BenchmarkError(f"metric {metric} has an unsupported schema")
    provenance = value.get("provenance")
    completeness = value.get("completeness")
    confidence = value.get("confidence")
    if provenance not in PROVENANCE_VALUES:
        raise BenchmarkError(f"metric {metric} has invalid provenance")
    if completeness not in COMPLETENESS_VALUES:
        raise BenchmarkError(f"metric {metric} has invalid completeness")
    if confidence not in CONFIDENCE_VALUES:
        raise BenchmarkError(f"metric {metric} has invalid confidence")
    measured = value.get("value")
    reason = value.get("unavailable_reason")
    unavailable = provenance == "unavailable" or completeness == "unavailable"
    if measured is None:
        if not unavailable or confidence != "unavailable":
            raise BenchmarkError(f"metric {metric} missing value must be explicitly unavailable")
        _bounded_string(reason, f"metric {metric} unavailable_reason", maximum=256)
    else:
        if unavailable or confidence == "unavailable" or reason is not None:
            raise BenchmarkError(f"metric {metric} available value has unavailable metadata")
        if metric == "restart_persistence":
            if not isinstance(measured, bool):
                raise BenchmarkError("restart_persistence must be boolean when available")
        else:
            _finite_number(measured, f"metric {metric}")
            if metric in RATIO_METRICS and float(measured) > 1.0:
                raise BenchmarkError(f"metric {metric} must be between 0 and 1")
    return {key: value[key] for key in sorted(expected)}


def _normalize_metrics(value: Any, expected_names: tuple[str, ...], field: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != set(expected_names):
        raise BenchmarkError(f"{field} must contain every required metric exactly once")
    return {name: _normalize_measurement(value[name], name) for name in expected_names}


def _validate_comparison_contract(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != set(SHARED_COMPARISON_FIELDS):
        raise BenchmarkError("comparison_contract has invalid fields")
    for field in (
        "provider_id",
        "model_id",
        "tokenizer_id",
        "pricing_id",
        "pricing_effective_date",
        "currency",
        "retry_policy",
        "evaluator_version",
    ):
        _bounded_string(value.get(field), f"comparison_contract.{field}", maximum=256)
    _sha(value.get("definition_sha256"), "comparison_contract.definition_sha256")
    if value["definition_sha256"] != DEFINITION_SHA256:
        raise BenchmarkError("runtime used a different benchmark definition")
    for field in (
        "input_cost_per_1k_usd",
        "output_cost_per_1k_usd",
        "seed",
        "output_limit_tokens",
        "quality_threshold",
    ):
        _finite_number(value.get(field), f"comparison_contract.{field}")
    if float(value["quality_threshold"]) > 1.0:
        raise BenchmarkError("comparison_contract.quality_threshold must be at most 1")
    if not isinstance(value["seed"], int) or isinstance(value["seed"], bool):
        raise BenchmarkError("comparison_contract.seed must be an integer")
    if not isinstance(value["output_limit_tokens"], int) or isinstance(value["output_limit_tokens"], bool):
        raise BenchmarkError("comparison_contract.output_limit_tokens must be an integer")
    return {field: value[field] for field in SHARED_COMPARISON_FIELDS}


def _normalize_limits(value: Any) -> dict[str, Any]:
    expected = {
        "per_call_cost_cap_usd",
        "run_cost_cap_usd",
        "daily_cost_cap_usd",
        "max_calls",
        "max_tokens",
        "timeout_seconds",
        "output_limit_tokens",
    }
    if not isinstance(value, dict) or set(value) != expected:
        raise BenchmarkError("runtime result limits have invalid fields")
    limits = {
        "per_call_cost_cap_usd": _positive_float(
            value["per_call_cost_cap_usd"], "per-call cost cap", MAX_PER_CALL_COST_USD
        ),
        "run_cost_cap_usd": _positive_float(value["run_cost_cap_usd"], "run cost cap", MAX_RUN_COST_USD),
        "daily_cost_cap_usd": _positive_float(
            value["daily_cost_cap_usd"], "daily cost cap", MAX_DAILY_COST_USD
        ),
        "max_calls": _positive_int(value["max_calls"], "max calls", MAX_CALLS),
        "max_tokens": _positive_int(value["max_tokens"], "max tokens", MAX_TOKENS),
        "timeout_seconds": _positive_float(value["timeout_seconds"], "timeout", MAX_TIMEOUT_SECONDS),
        "output_limit_tokens": _positive_int(
            value["output_limit_tokens"], "output token limit", MAX_OUTPUT_TOKENS
        ),
    }
    if not limits["per_call_cost_cap_usd"] <= limits["run_cost_cap_usd"] <= limits["daily_cost_cap_usd"]:
        raise BenchmarkError("runtime result cost caps must satisfy per-call <= run <= daily")
    return limits


def _canonical_scorecard(value: Any, expected_runtime: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise BenchmarkError("strategy scorecard must be an object")
    try:
        scorecard = VALIDATOR.import_scorecard(value)
    except VALIDATOR.ScorecardError as exc:
        raise BenchmarkError(f"runtime returned an invalid scorecard: {exc}") from exc
    if scorecard.get("runtime_kind") != expected_runtime:
        raise BenchmarkError("scorecard runtime_kind does not match its adapter")
    return scorecard


def _assert_scorecard_contract(
    scorecard: Mapping[str, Any],
    shared_contract: Mapping[str, Any],
    runtime_version: str,
) -> None:
    contract = scorecard.get("comparison_contract")
    if not isinstance(contract, dict):
        raise BenchmarkError("scorecard must contain a comparison_contract")
    if scorecard.get("runtime_version") != runtime_version:
        raise BenchmarkError("scorecard runtime_version does not match the runtime result")
    bindings = {
        "provider_id": "provider_id",
        "model_id": "model_id",
        "tokenizer_id": "tokenizer_id",
        "pricing_id": "pricing_id",
        "input_cost_per_1k_usd": "input_cost_per_1k_usd",
        "output_cost_per_1k_usd": "output_cost_per_1k_usd",
        "quality_threshold": "quality_threshold",
        "seed": "seed",
        "retry_policy": "retry_policy",
        "evaluator_version": "evaluator_version",
    }
    for scorecard_field, shared_field in bindings.items():
        if contract.get(scorecard_field) != shared_contract.get(shared_field):
            raise BenchmarkError(
                f"scorecard comparison_contract.{scorecard_field} does not match the runtime request"
            )


def _measurement_value(metrics: Mapping[str, Any], name: str) -> Any:
    return metrics[name]["value"]


def _assert_scorecard_metric_consistency(scorecard: Mapping[str, Any], metrics: Mapping[str, Any]) -> None:
    expected = {
        "input_tokens": scorecard.get("input_token_total"),
        "output_tokens": scorecard.get("output_token_total"),
        "context_tokens": scorecard.get("context_token_total"),
        "repeated_context_tokens": scorecard.get("repeated_context_token_total"),
        "tool_call_count": scorecard.get("tool_call_count"),
        "redundant_tool_calls": scorecard.get("redundant_tool_call_count"),
        "retries": scorecard.get("retry_count"),
        "latency_ms": scorecard.get("duration_ms"),
        "cost_usd": scorecard.get("estimated_cost_usd"),
        "quality": scorecard.get("quality_score"),
    }
    for metric, scorecard_value in expected.items():
        measured = _measurement_value(metrics, metric)
        if measured is not None and scorecard_value is not None:
            if isinstance(measured, (int, float)) and isinstance(scorecard_value, (int, float)):
                if abs(float(measured) - float(scorecard_value)) > 0.0000005:
                    raise BenchmarkError(f"metric {metric} does not match the scorecard")


def _assert_tool_scorecard_metric_consistency(
    scorecard: Mapping[str, Any], metrics: Mapping[str, Any]
) -> None:
    expected = {
        "prompt_tokens": scorecard.get("input_token_total"),
        "latency_ms": scorecard.get("duration_ms"),
        "cost_usd": scorecard.get("estimated_cost_usd"),
        "quality": scorecard.get("quality_score"),
    }
    for metric, scorecard_value in expected.items():
        measured = _measurement_value(metrics, metric)
        if measured is not None and scorecard_value is not None:
            if abs(float(measured) - float(scorecard_value)) > 0.0000005:
                raise BenchmarkError(f"tool metric {metric} does not match the scorecard")


def _normalize_strategy_result(
    value: Any,
    expected_runtime: str,
    shared_contract: Mapping[str, Any],
    runtime_version: str,
) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise BenchmarkError("strategy result must be an object")
    if set(value) != {"strategy_id", "scorecard", "metrics", "evidence_references"}:
        raise BenchmarkError("strategy result has invalid fields")
    strategy_id = value.get("strategy_id")
    if strategy_id not in PRIMARY_STRATEGIES:
        raise BenchmarkError("strategy result has an unsupported strategy_id")
    scorecard = _canonical_scorecard(value.get("scorecard"), expected_runtime)
    _assert_scorecard_contract(scorecard, shared_contract, runtime_version)
    if scorecard.get("state_strategy") != SCORECARD_STATE_STRATEGIES[strategy_id]:
        raise BenchmarkError("strategy_id does not match the scorecard state_strategy")
    metrics = _normalize_metrics(value.get("metrics"), MATERIAL_METRICS, "strategy metrics")
    _assert_scorecard_metric_consistency(scorecard, metrics)
    evidence_refs = value.get("evidence_references")
    if not isinstance(evidence_refs, list) or len(evidence_refs) > 64:
        raise BenchmarkError("strategy evidence_references must be a bounded list")
    normalized_refs = []
    for index, reference in enumerate(evidence_refs):
        if not isinstance(reference, dict) or set(reference) != {"source_id", "source_sha256"}:
            raise BenchmarkError("strategy evidence reference has invalid fields")
        normalized_refs.append(
            {
                "source_id": _identifier(reference.get("source_id"), f"evidence_references[{index}].source_id"),
                "source_sha256": _sha(
                    reference.get("source_sha256"), f"evidence_references[{index}].source_sha256"
                ),
            }
        )
    source_ref_ids = [item["source_id"] for item in normalized_refs]
    if len(source_ref_ids) != len(set(source_ref_ids)):
        raise BenchmarkError("strategy evidence references contain duplicate source IDs")
    scorecard_sha256 = hashlib.sha256(VALIDATOR.canonical_scorecard_json(scorecard).encode("utf-8")).hexdigest()
    return {
        "strategy_id": strategy_id,
        "adapter_run_id": scorecard["adapter_run_id"],
        "scorecard_sha256": scorecard_sha256,
        "scorecard_status": scorecard["status"],
        "metrics": metrics,
        "evidence_references": normalized_refs,
    }


def _normalize_tool_selection(value: Any, variant: str) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    descriptor_hashes = value.get("descriptor_hashes") if isinstance(value, dict) else None
    selected = value.get("selected_tools") if isinstance(value, dict) else None
    if not isinstance(descriptor_hashes, list) or not descriptor_hashes or len(descriptor_hashes) > 128:
        raise BenchmarkError("tool descriptor_hashes must be a bounded non-empty list")
    if not isinstance(selected, list) or len(selected) > 128:
        raise BenchmarkError("selected_tools must be a bounded list")
    descriptors = []
    for item in descriptor_hashes:
        if not isinstance(item, dict) or set(item) != {"tool_id", "descriptor_sha256"}:
            raise BenchmarkError("tool descriptor hash has invalid fields")
        descriptors.append(
            {
                "tool_id": _identifier(item.get("tool_id"), "descriptor tool_id"),
                "descriptor_sha256": _sha(item.get("descriptor_sha256"), "descriptor_sha256"),
            }
        )
    descriptor_ids = [item["tool_id"] for item in descriptors]
    if descriptor_ids != sorted(descriptor_ids) or len(descriptor_ids) != len(set(descriptor_ids)):
        raise BenchmarkError("tool descriptors must have unique tool IDs in ascending order")
    selections = []
    for item in selected:
        if not isinstance(item, dict) or set(item) != {"tool_id", "score"}:
            raise BenchmarkError("selected tool has invalid fields")
        selections.append(
            {
                "tool_id": _identifier(item.get("tool_id"), "selected tool_id"),
                "score": _finite_number(item.get("score"), "selected tool score"),
            }
        )
    selected_ids = [item["tool_id"] for item in selections]
    if len(selected_ids) != len(set(selected_ids)) or any(item not in descriptor_ids for item in selected_ids):
        raise BenchmarkError("selected tools must be unique members of the descriptor corpus")
    if variant == "static_all":
        if selected_ids != descriptor_ids:
            raise BenchmarkError("static_all must select the complete sorted descriptor corpus")
    else:
        expected_count = min(
            int(CANONICAL_DEFINITION["tool_discovery"]["top_k"]),
            len(descriptors),
        )
        if len(selections) != expected_count:
            raise BenchmarkError("deterministic_top_k must select exactly the canonical K")
        expected_order = sorted(selections, key=lambda item: (-float(item["score"]), item["tool_id"]))
        if selections != expected_order:
            raise BenchmarkError("deterministic_top_k ordering is nondeterministic")
    return descriptors, selections


def _normalize_tool_result(
    value: Any,
    expected_runtime: str,
    shared_contract: Mapping[str, Any],
    runtime_version: str,
) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise BenchmarkError("tool discovery result must be an object")
    if set(value) != {
        "variant",
        "scorecard",
        "metrics",
        "corpus_sha256",
        "registry_sha256",
        "retriever_version",
        "descriptor_hashes",
        "selected_tools",
        "provider_selected_tool_id",
        "adapter_result_sha256",
    }:
        raise BenchmarkError("tool discovery result has invalid fields")
    variant = value.get("variant")
    if variant not in TOOL_VARIANTS:
        raise BenchmarkError("tool discovery result has an unsupported variant")
    scorecard = _canonical_scorecard(value.get("scorecard"), expected_runtime)
    _assert_scorecard_contract(scorecard, shared_contract, runtime_version)
    metrics = _normalize_metrics(value.get("metrics"), TOOL_METRICS, "tool discovery metrics")
    _assert_tool_scorecard_metric_consistency(scorecard, metrics)
    descriptors, selected = _normalize_tool_selection(value, variant)
    provider_selected_tool_id = _identifier(
        value.get("provider_selected_tool_id"), "provider selected tool id"
    )
    if provider_selected_tool_id not in {item["tool_id"] for item in selected}:
        raise BenchmarkError("provider selected a tool outside the exposed descriptor set")
    adapter_result_sha256 = value.get("adapter_result_sha256")
    if adapter_result_sha256 is not None:
        adapter_result_sha256 = _sha(adapter_result_sha256, "tool adapter_result_sha256")
    if expected_runtime == "langgraph" and shared_contract["provider_id"] != "fixture":
        if adapter_result_sha256 is None:
            raise BenchmarkError("live LangGraph tool result is missing adapter evidence")
    corpus_sha256 = _sha(value.get("corpus_sha256"), "tool corpus_sha256")
    registry_sha256 = _sha(value.get("registry_sha256"), "tool registry_sha256")
    retriever_version = _bounded_string(value.get("retriever_version"), "retriever_version", maximum=128)
    if retriever_version != CANONICAL_DEFINITION["tool_discovery"]["retriever_version"]:
        raise BenchmarkError("tool result used a different retriever version")
    scorecard_sha256 = hashlib.sha256(VALIDATOR.canonical_scorecard_json(scorecard).encode("utf-8")).hexdigest()
    return {
        "variant": variant,
        "adapter_run_id": scorecard["adapter_run_id"],
        "scorecard_sha256": scorecard_sha256,
        "scorecard_status": scorecard["status"],
        "metrics": metrics,
        "corpus_sha256": corpus_sha256,
        "registry_sha256": registry_sha256,
        "retriever_version": retriever_version,
        "descriptor_hashes": descriptors,
        "selected_tools": selected,
        "provider_selected_tool_id": provider_selected_tool_id,
        "adapter_result_sha256": adapter_result_sha256,
    }


def _normalize_audit_evidence(value: Any, mode: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != {
        "schema_version",
        "event_count",
        "evidence_sha256",
        "store_kind",
    }:
        raise BenchmarkError("audit_evidence has invalid fields")
    if value.get("schema_version") != AUDIT_EVIDENCE_SCHEMA_VERSION:
        raise BenchmarkError("audit_evidence has an unsupported schema")
    count = value.get("event_count")
    if not isinstance(count, int) or isinstance(count, bool) or count < 0:
        raise BenchmarkError("audit_evidence.event_count must be non-negative")
    if mode == "live" and count == 0:
        raise BenchmarkError("live runtime result requires persisted audit evidence")
    return {
        "schema_version": AUDIT_EVIDENCE_SCHEMA_VERSION,
        "event_count": count,
        "evidence_sha256": _sha(value.get("evidence_sha256"), "audit evidence_sha256"),
        "store_kind": _bounded_string(value.get("store_kind"), "audit store_kind", maximum=128),
    }


def validate_runtime_result(
    value: dict[str, Any],
    expected_runtime: str,
    mode: str,
    benchmark_run_id: str,
    expected_request_sha256: str,
    expected_limits: Mapping[str, Any],
) -> dict[str, Any]:
    expected_fields = {
        "schema_version",
        "runtime_kind",
        "runtime_version",
        "adapter_version",
        "benchmark_run_id",
        "definition_sha256",
        "request_sha256",
        "comparison_contract",
        "limits",
        "external_provider_calls",
        "strategy_results",
        "tool_discovery_results",
        "audit_evidence",
    }
    if set(value) != expected_fields:
        raise BenchmarkError("runtime result has invalid fields")
    if value.get("schema_version") != RUNTIME_RESULT_SCHEMA_VERSION:
        raise BenchmarkError("runtime result has an unsupported schema")
    if value.get("runtime_kind") != expected_runtime:
        raise BenchmarkError("runtime result identity does not match the selected adapter")
    runtime_version = _bounded_string(value.get("runtime_version"), "runtime_version", maximum=256)
    _bounded_string(value.get("adapter_version"), "adapter_version", maximum=256)
    if value.get("benchmark_run_id") != benchmark_run_id:
        raise BenchmarkError("runtime result benchmark_run_id does not match the request")
    if value.get("definition_sha256") != DEFINITION_SHA256:
        raise BenchmarkError("runtime result definition hash does not match")
    request_sha256 = _sha(value.get("request_sha256"), "runtime result request_sha256")
    if request_sha256 != expected_request_sha256:
        raise BenchmarkError("runtime result request hash does not match the invocation")
    contract = _validate_comparison_contract(value.get("comparison_contract"))
    limits = _normalize_limits(value.get("limits"))
    if limits != expected_limits:
        raise BenchmarkError("runtime result limits do not match the invocation")
    external_calls = value.get("external_provider_calls")
    if not isinstance(external_calls, int) or isinstance(external_calls, bool) or external_calls < 0:
        raise BenchmarkError("external_provider_calls must be non-negative")
    if mode == "fixture" and external_calls != 0:
        raise BenchmarkError("fixture runtime result must not contain external provider calls")
    if mode == "live" and external_calls == 0:
        raise BenchmarkError("live runtime result did not record an external provider call")

    strategy_values = value.get("strategy_results")
    if not isinstance(strategy_values, list) or len(strategy_values) != len(PRIMARY_STRATEGIES):
        raise BenchmarkError("runtime result must contain exactly four strategy results")
    strategies = [
        _normalize_strategy_result(item, expected_runtime, contract, runtime_version)
        for item in strategy_values
    ]
    strategy_ids = [item["strategy_id"] for item in strategies]
    if strategy_ids != list(PRIMARY_STRATEGIES):
        raise BenchmarkError("runtime strategy results must use canonical order and identities")

    tool_values = value.get("tool_discovery_results")
    if not isinstance(tool_values, list) or len(tool_values) != len(TOOL_VARIANTS):
        raise BenchmarkError("runtime result must contain both tool-discovery variants")
    tools = [
        _normalize_tool_result(item, expected_runtime, contract, runtime_version)
        for item in tool_values
    ]
    if [item["variant"] for item in tools] != list(TOOL_VARIANTS):
        raise BenchmarkError("tool-discovery variants must use canonical order and identities")
    if tools[0]["corpus_sha256"] != tools[1]["corpus_sha256"]:
        raise BenchmarkError("tool-discovery variants used different descriptor corpora")
    if tools[0]["registry_sha256"] != tools[1]["registry_sha256"]:
        raise BenchmarkError("tool-discovery variants used different scenario registries")
    if tools[0]["descriptor_hashes"] != tools[1]["descriptor_hashes"]:
        raise BenchmarkError("tool-discovery descriptor bindings changed between variants")

    audit = _normalize_audit_evidence(value.get("audit_evidence"), mode)
    normalized = {
        "schema_version": RUNTIME_RESULT_SCHEMA_VERSION,
        "runtime_kind": expected_runtime,
        "runtime_version": value["runtime_version"],
        "adapter_version": value["adapter_version"],
        "benchmark_run_id": benchmark_run_id,
        "definition_sha256": DEFINITION_SHA256,
        "request_sha256": request_sha256,
        "comparison_contract": contract,
        "limits": limits,
        "external_provider_calls": external_calls,
        "strategy_results": strategies,
        "tool_discovery_results": tools,
        "audit_evidence": audit,
    }
    _check_json_bounds(normalized)
    _reject_sensitive(normalized)
    normalized["runtime_result_sha256"] = sha256_value(normalized)
    return normalized


def _unavailable_reason(runtime: str, result: Mapping[str, Any], metric: str) -> str | None:
    measurement = result["metrics"][metric]
    if measurement["value"] is None or measurement["completeness"] != "complete":
        return f"{runtime}.{metric}_unavailable"
    return None


def _cross_runtime_comparisons(results: Mapping[str, dict[str, Any]]) -> list[dict[str, Any]]:
    native = results["native_harness"]
    langgraph = results["langgraph"]
    contract_reasons = [
        f"comparison_contract.{field}_mismatch"
        for field in SHARED_COMPARISON_FIELDS
        if native["comparison_contract"].get(field) != langgraph["comparison_contract"].get(field)
    ]
    comparisons = []
    for index, strategy_id in enumerate(PRIMARY_STRATEGIES):
        native_strategy = native["strategy_results"][index]
        langgraph_strategy = langgraph["strategy_results"][index]
        reasons = list(contract_reasons)
        for runtime, strategy in (
            ("native_harness", native_strategy),
            ("langgraph", langgraph_strategy),
        ):
            for metric in REQUIRED_CROSS_RUNTIME_METRICS:
                reason = _unavailable_reason(runtime, strategy, metric)
                if reason:
                    reasons.append(reason)
        reasons = sorted(set(reasons))
        comparisons.append(
            {
                "strategy_id": strategy_id,
                "comparable": not reasons,
                "reason_codes": reasons,
                "native_scorecard_sha256": native_strategy["scorecard_sha256"],
                "langgraph_scorecard_sha256": langgraph_strategy["scorecard_sha256"],
            }
        )
    return comparisons


def _quality_and_efficiency(runtime_result: Mapping[str, Any]) -> list[dict[str, Any]]:
    baseline = runtime_result["strategy_results"][0]
    baseline_tokens = _measurement_value(baseline["metrics"], "input_tokens")
    baseline_quality = _measurement_value(baseline["metrics"], "quality")
    threshold = float(runtime_result["comparison_contract"]["quality_threshold"])
    rows = []
    for strategy in runtime_result["strategy_results"]:
        tokens = _measurement_value(strategy["metrics"], "input_tokens")
        quality = _measurement_value(strategy["metrics"], "quality")
        quality_regression = (
            quality is None
            or baseline_quality is None
            or float(quality) < threshold
            or float(quality) < float(baseline_quality)
        )
        reduction = None
        if (
            not quality_regression
            and isinstance(tokens, (int, float))
            and isinstance(baseline_tokens, (int, float))
            and baseline_tokens > 0
        ):
            reduction = round((float(baseline_tokens) - float(tokens)) / float(baseline_tokens), 6)
        rows.append(
            {
                "strategy_id": strategy["strategy_id"],
                "quality_regression": quality_regression,
                "input_token_reduction": reduction,
                "efficiency_advantage_reported": reduction is not None and reduction > 0,
            }
        )
    return rows


def _tool_comparison(runtime_result: Mapping[str, Any]) -> dict[str, Any]:
    baseline, candidate = runtime_result["tool_discovery_results"]
    baseline_quality = _measurement_value(baseline["metrics"], "quality")
    candidate_quality = _measurement_value(candidate["metrics"], "quality")
    recall = _measurement_value(candidate["metrics"], "required_tool_recall")
    incorrect = _measurement_value(candidate["metrics"], "incorrect_tool_selection")
    baseline_prompt_tokens = _measurement_value(baseline["metrics"], "prompt_tokens")
    candidate_prompt_tokens = _measurement_value(candidate["metrics"], "prompt_tokens")
    baseline_reduction = _measurement_value(baseline["metrics"], "prompt_token_reduction")
    reduction = _measurement_value(candidate["metrics"], "prompt_token_reduction")
    if baseline_reduction is not None and float(baseline_reduction) != 0.0:
        raise BenchmarkError("static_all prompt_token_reduction must be zero")
    if (
        isinstance(baseline_prompt_tokens, (int, float))
        and isinstance(candidate_prompt_tokens, (int, float))
        and baseline_prompt_tokens > 0
        and isinstance(reduction, (int, float))
    ):
        expected_reduction = round(
            (float(baseline_prompt_tokens) - float(candidate_prompt_tokens))
            / float(baseline_prompt_tokens),
            6,
        )
        if abs(float(reduction) - expected_reduction) > 0.0000005:
            raise BenchmarkError("tool prompt_token_reduction does not match prompt_tokens")
    quality_threshold = float(runtime_result["comparison_contract"]["quality_threshold"])
    quality_regression = (
        baseline_quality is None
        or candidate_quality is None
        or float(candidate_quality) < quality_threshold
        or float(candidate_quality) < float(baseline_quality)
        or recall is None
        or float(recall) < 1.0
        or incorrect is None
        or float(incorrect) > 0
    )
    return {
        "corpus_sha256": baseline["corpus_sha256"],
        "registry_sha256": baseline["registry_sha256"],
        "quality_regression": quality_regression,
        "prompt_token_reduction": None if quality_regression else reduction,
        "efficiency_advantage_reported": (
            not quality_regression and isinstance(reduction, (int, float)) and reduction > 0
        ),
    }


def build_report(
    benchmark_run_id: str,
    mode: str,
    results: Mapping[str, dict[str, Any]],
    catalog_evidence: dict[str, Any] | None = None,
) -> dict[str, Any]:
    if set(results) != set(RUNTIME_KINDS):
        raise BenchmarkError("both native_harness and langgraph results are required")
    quality_results = {
        runtime: _quality_and_efficiency(results[runtime]) for runtime in RUNTIME_KINDS
    }
    tool_results = {runtime: _tool_comparison(results[runtime]) for runtime in RUNTIME_KINDS}
    cross_runtime = _cross_runtime_comparisons(results)
    quality_failure_reasons = sorted(
        [
            f"{runtime}.{row['strategy_id']}.quality_regression"
            for runtime in RUNTIME_KINDS
            for row in quality_results[runtime]
            if row["quality_regression"]
        ]
        + [
            f"{runtime}.deterministic_top_k.quality_regression"
            for runtime in RUNTIME_KINDS
            if tool_results[runtime]["quality_regression"]
        ]
    )
    incomparable_reasons = sorted(
        {
            f"{row['strategy_id']}.{reason}"
            for row in cross_runtime
            for reason in row["reason_codes"]
        }
    )
    acceptance_status = (
        "FAIL"
        if quality_failure_reasons
        else "INCOMPARABLE"
        if incomparable_reasons
        else "PASS"
    )
    report = {
        "schema_version": REPORT_SCHEMA_VERSION,
        "benchmark_run_id": benchmark_run_id,
        "execution_mode": mode,
        "definition_sha256": DEFINITION_SHA256,
        "read_only": True,
        "report_only": True,
        "target_repository_writes": "disabled",
        "catalog_evidence": catalog_evidence,
        "acceptance_status": acceptance_status,
        "quality_failure_reasons": quality_failure_reasons,
        "incomparable_reasons": incomparable_reasons,
        "runtime_evidence": {
            runtime: {
                "runtime_version": results[runtime]["runtime_version"],
                "adapter_version": results[runtime]["adapter_version"],
                "runtime_result_sha256": results[runtime]["runtime_result_sha256"],
                "request_sha256": results[runtime]["request_sha256"],
                "comparison_contract": results[runtime]["comparison_contract"],
                "limits": results[runtime]["limits"],
                "external_provider_calls": results[runtime]["external_provider_calls"],
                "audit_evidence": results[runtime]["audit_evidence"],
                "strategy_results": results[runtime]["strategy_results"],
                "tool_discovery_results": results[runtime]["tool_discovery_results"],
                "quality_and_efficiency": quality_results[runtime],
                "tool_comparison": tool_results[runtime],
            }
            for runtime in RUNTIME_KINDS
        },
        "cross_runtime_comparisons": cross_runtime,
    }
    _check_json_bounds(report)
    _reject_sensitive(report)
    report["report_sha256"] = sha256_value(report)
    return report


def _positive_float(value: Any, name: str, maximum: float) -> float:
    parsed = float(_finite_number(value, name, minimum=0.0))
    if parsed <= 0 or parsed > maximum:
        raise BenchmarkError(f"{name} must be positive and at most {maximum}")
    return parsed


def _nonnegative_float(value: Any, name: str, maximum: float) -> float:
    parsed = float(_finite_number(value, name, minimum=0.0))
    if parsed > maximum:
        raise BenchmarkError(f"{name} must be non-negative and at most {maximum}")
    return parsed


def _positive_int(value: Any, name: str, maximum: int) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or not 1 <= value <= maximum:
        raise BenchmarkError(f"{name} must be between 1 and {maximum}")
    return value


def _validate_pricing_effective_date(value: Any) -> None:
    try:
        if not isinstance(value, str) or not _RFC3339.fullmatch(value):
            raise ValueError
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
        if parsed.tzinfo is None:
            raise ValueError
    except ValueError as exc:
        raise BenchmarkError("pricing effective date must be RFC3339") from exc


def _validate_live_args(args: argparse.Namespace, env: Mapping[str, str]) -> None:
    if args.mode != "live":
        return
    if "CI" in env or env.get("GITHUB_ACTIONS") == "true":
        raise BenchmarkError("live provider execution is forbidden in CI")
    if args.live_confirmation != LIVE_CONFIRMATION:
        raise BenchmarkError(f"live execution requires --live-confirmation {LIVE_CONFIRMATION}")
    if args.provider != "openai_compatible":
        raise BenchmarkError("live execution currently requires the fixed openai_compatible provider")
    if not args.model or args.model.startswith("fixture"):
        raise BenchmarkError("live execution requires one explicit fixed model")
    if not args.tokenizer or args.tokenizer.startswith("fixture"):
        raise BenchmarkError("live execution requires one explicit fixed tokenizer")
    base_url_match = _HTTPS_BASE_URL.fullmatch(args.provider_base_url or "")
    if base_url_match is None or (
        base_url_match.group(1) is not None and int(base_url_match.group(1)) > 65535
    ):
        raise BenchmarkError("live execution requires a credential-free HTTPS provider base URL")
    if not args.credential_env or not _ENV_NAME.fullmatch(args.credential_env):
        raise BenchmarkError("live execution requires a symbolic credential environment reference")
    if not env.get(args.credential_env, "").strip():
        raise BenchmarkError("the symbolic credential environment reference is not populated")
    if not args.kill_switch_env or not _ENV_NAME.fullmatch(args.kill_switch_env):
        raise BenchmarkError("live execution requires a symbolic kill-switch environment name")
    if args.kill_switch_env not in env:
        raise BenchmarkError("live execution requires an available kill switch")
    if env.get(args.kill_switch_env) == "1":
        raise BenchmarkError("the live benchmark kill switch is active")
    if args.audit_store is None:
        raise BenchmarkError("live execution requires an explicit audit store")
    audit_parent = args.audit_store.expanduser().resolve().parent
    if not audit_parent.is_dir():
        raise BenchmarkError("the explicit audit store parent does not exist")
    if args.audit_store.exists() and not args.audit_store.is_file():
        raise BenchmarkError("the explicit audit store must be a file path")
    if args.pricing_id.startswith("fixture"):
        raise BenchmarkError("live execution requires an explicit non-fixture pricing identity")
    if args.pricing_effective_date == FIXTURE_PRICING_EFFECTIVE_DATE:
        raise BenchmarkError("live execution requires an explicit pricing effective date")
    _nonnegative_float(args.input_cost_per_1k_usd, "input price", 100.0)
    _nonnegative_float(args.output_cost_per_1k_usd, "output price", 100.0)


def _validate_common_args(args: argparse.Namespace) -> None:
    for field in ("provider", "model", "tokenizer", "pricing_id", "pricing_effective_date", "retry_policy"):
        _bounded_string(getattr(args, field), field, maximum=256)
    _validate_pricing_effective_date(args.pricing_effective_date)
    _reject_sensitive(
        {
            "provider_id": args.provider,
            "model_id": args.model,
            "tokenizer_id": args.tokenizer,
            "pricing_id": args.pricing_id,
            "retry_policy": args.retry_policy,
        }
    )
    _finite_number(args.input_cost_per_1k_usd, "input price")
    _finite_number(args.output_cost_per_1k_usd, "output price")
    per_call = _positive_float(
        args.per_call_cost_cap_usd, "per-call cost cap", MAX_PER_CALL_COST_USD
    )
    run_cap = _positive_float(args.run_cost_cap_usd, "run cost cap", MAX_RUN_COST_USD)
    daily_cap = _positive_float(args.daily_cost_cap_usd, "daily cost cap", MAX_DAILY_COST_USD)
    if not per_call <= run_cap <= daily_cap:
        raise BenchmarkError("cost caps must satisfy per-call <= run <= daily")
    _positive_float(args.timeout_seconds, "timeout", MAX_TIMEOUT_SECONDS)
    _positive_int(args.max_calls, "max calls", MAX_CALLS)
    _positive_int(args.max_tokens, "max tokens", MAX_TOKENS)
    _positive_int(args.output_limit_tokens, "output token limit", MAX_OUTPUT_TOKENS)
    if args.output_limit_tokens != CANONICAL_DEFINITION["memory_scenario"]["output_limit_tokens"]:
        raise BenchmarkError("output token limit must match the canonical benchmark definition")


def _comparison_contract_from_args(args: argparse.Namespace) -> dict[str, Any]:
    return {
        "definition_sha256": DEFINITION_SHA256,
        "provider_id": args.provider,
        "model_id": args.model,
        "tokenizer_id": args.tokenizer,
        "pricing_id": args.pricing_id,
        "pricing_effective_date": args.pricing_effective_date,
        "currency": "USD",
        "input_cost_per_1k_usd": args.input_cost_per_1k_usd,
        "output_cost_per_1k_usd": args.output_cost_per_1k_usd,
        "seed": CANONICAL_DEFINITION["memory_scenario"]["seed"],
        "retry_policy": args.retry_policy,
        "evaluator_version": CANONICAL_DEFINITION["memory_scenario"]["quality_method"],
        "output_limit_tokens": args.output_limit_tokens,
        "quality_threshold": CANONICAL_DEFINITION["memory_scenario"]["quality_threshold"],
    }


def _runtime_request(args: argparse.Namespace, benchmark_run_id: str) -> dict[str, Any]:
    request = {
        "schema_version": "efficiency_benchmark_runtime_request.v1",
        "benchmark_run_id": benchmark_run_id,
        "mode": args.mode,
        "definition": CANONICAL_DEFINITION,
        "definition_sha256": DEFINITION_SHA256,
        "comparison_contract": _comparison_contract_from_args(args),
        "limits": {
            "per_call_cost_cap_usd": args.per_call_cost_cap_usd,
            "run_cost_cap_usd": args.run_cost_cap_usd,
            "daily_cost_cap_usd": args.daily_cost_cap_usd,
            "max_calls": args.max_calls,
            "max_tokens": args.max_tokens,
            "timeout_seconds": args.timeout_seconds,
            "output_limit_tokens": args.output_limit_tokens,
        },
        "operator_inputs": {
            "provider_base_url": args.provider_base_url,
            "credential_env": args.credential_env,
            "kill_switch_env": args.kill_switch_env,
            "audit_store": str(args.audit_store.expanduser().resolve()) if args.audit_store else None,
        },
    }
    return request


def _child_environment(args: argparse.Namespace, env: Mapping[str, str]) -> dict[str, str]:
    allowed = {
        key: env[key]
        for key in (
            "PATH",
            "LD_LIBRARY_PATH",
            "SSL_CERT_FILE",
            "SSL_CERT_DIR",
            "REQUESTS_CA_BUNDLE",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "NO_PROXY",
        )
        if key in env
    }
    allowed["ACP_EFFICIENCY_BENCHMARK_MODE"] = args.mode
    allowed["ACP_EFFICIENCY_BENCHMARK_DEFINITION_SHA256"] = DEFINITION_SHA256
    if args.mode == "live":
        assert args.credential_env and args.kill_switch_env
        allowed[args.credential_env] = env[args.credential_env]
        allowed[args.kill_switch_env] = env[args.kill_switch_env]
        # The operator may choose a symbolic name, while the existing Rust
        # provider owner observes one canonical kill-switch variable.
        allowed["ACP_LOCAL_RUNNER_KILL_SWITCH"] = env[args.kill_switch_env]
        allowed["ACP_LOCAL_RUNNER_PROVIDER_TYPE"] = args.provider
        allowed["ACP_LOCAL_RUNNER_BASE_URL"] = args.provider_base_url
        allowed["ACP_LOCAL_RUNNER_MODEL"] = args.model
        allowed["ACP_LOCAL_RUNNER_API_KEY_ENV"] = args.credential_env
        allowed["ACP_PROVIDER_INPUT_COST_PER_1K_USD"] = str(args.input_cost_per_1k_usd)
        allowed["ACP_PROVIDER_OUTPUT_COST_PER_1K_USD"] = str(args.output_cost_per_1k_usd)
        allowed["ACP_ENABLE_PROVIDER_EXECUTION"] = "1"
    return allowed


RunCallable = Callable[..., subprocess.CompletedProcess[Any]]


def _invoke_runtime(
    executable: Path,
    expected_runtime: str,
    args: argparse.Namespace,
    benchmark_run_id: str,
    env: Mapping[str, str],
    runner: RunCallable,
) -> dict[str, Any]:
    if not executable.is_file() or not os.access(executable, os.X_OK):
        raise BenchmarkError(f"{expected_runtime} executable path is not an executable file")
    request = _runtime_request(args, benchmark_run_id)
    request_sha256 = sha256_value(request)
    expected_limits = _normalize_limits(request["limits"])
    with tempfile.TemporaryDirectory(prefix="acp-efficiency-benchmark-") as directory:
        temp = Path(directory)
        request_path = temp / "request.json"
        output_path = temp / "result.json"
        request_path.write_bytes(canonical_json_bytes(request))
        command = [
            str(executable),
            "--benchmark-request",
            str(request_path),
            "--benchmark-output",
            str(output_path),
        ]
        try:
            completed = runner(
                command,
                check=False,
                timeout=float(args.timeout_seconds) + 5.0,
                env=_child_environment(args, env),
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        except subprocess.TimeoutExpired as exc:
            raise BenchmarkError(f"{expected_runtime} invocation exceeded its bounded timeout") from exc
        except OSError as exc:
            raise BenchmarkError(f"{expected_runtime} invocation could not start") from exc
        if completed.returncode != 0:
            raise BenchmarkError(f"{expected_runtime} invocation failed with status {completed.returncode}")
        result = _load_bounded_json(output_path)
    return validate_runtime_result(
        result,
        expected_runtime,
        args.mode,
        benchmark_run_id,
        request_sha256,
        expected_limits,
    )


def _write_report(output_root: Path, benchmark_run_id: str, report: dict[str, Any]) -> Path:
    output_root = output_root.expanduser().resolve()
    output_root.mkdir(parents=True, exist_ok=True)
    if not output_root.is_dir():
        raise BenchmarkError("output root must be a directory")
    target = output_root / f"efficiency-benchmark-{benchmark_run_id}.report.json"
    encoded = canonical_json_bytes(report)
    if len(encoded) > MAX_JSON_BYTES:
        raise BenchmarkError("benchmark report exceeds the bounded size")
    if target.exists():
        if target.is_symlink() or not target.is_file():
            raise BenchmarkError("benchmark report identity is not a regular file")
        if target.read_bytes() == encoded:
            return target
        raise BenchmarkError("benchmark report identity already exists with different content")
    temporary = output_root / f".{target.name}.{uuid.uuid4().hex}.tmp"
    try:
        with temporary.open("xb") as handle:
            handle.write(encoded)
            handle.flush()
            os.fsync(handle.fileno())
        try:
            os.link(temporary, target)
        except FileExistsError:
            if target.is_symlink() or not target.is_file() or target.read_bytes() != encoded:
                raise BenchmarkError(
                    "benchmark report identity was concurrently created with different content"
                )
        directory_descriptor = os.open(output_root, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        try:
            os.fsync(directory_descriptor)
        finally:
            os.close(directory_descriptor)
    finally:
        if temporary.exists():
            temporary.unlink()
    return target


def execute(
    args: argparse.Namespace,
    *,
    env: Mapping[str, str] | None = None,
    runner: RunCallable = subprocess.run,
    catalog_fetcher: CatalogFetcher | None = None,
) -> tuple[dict[str, Any], Path]:
    environment = dict(os.environ if env is None else env)
    benchmark_run_id = _identifier(args.benchmark_run_id, "benchmark_run_id")
    _validate_common_args(args)
    _validate_live_args(args, environment)
    catalog_evidence = None
    effective_args = argparse.Namespace(**vars(args))
    if args.mode == "live":
        catalog_evidence = _openrouter_hy3_catalog_evidence(
            args, catalog_fetcher or _fetch_bounded_catalog_json
        )
        effective_args.pricing_id = (
            f"openrouter-catalog-sha256:{catalog_evidence['evidence_sha256']}"
        )
        effective_args.pricing_effective_date = catalog_evidence["observed_at"]
    results = {
        "native_harness": _invoke_runtime(
            effective_args.native_cli,
            "native_harness",
            effective_args,
            benchmark_run_id,
            environment,
            runner,
        ),
        "langgraph": _invoke_runtime(
            effective_args.langgraph_adapter,
            "langgraph",
            effective_args,
            benchmark_run_id,
            environment,
            runner,
        ),
    }
    report = build_report(benchmark_run_id, args.mode, results, catalog_evidence)
    path = _write_report(effective_args.output_root, benchmark_run_id, report)
    return report, path


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("fixture", "live"), default="fixture")
    parser.add_argument("--native-cli", type=Path, required=True)
    parser.add_argument("--langgraph-adapter", type=Path, required=True)
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--audit-store", type=Path)
    parser.add_argument("--benchmark-run-id", default=f"benchmark-{uuid.uuid4().hex[:16]}")
    parser.add_argument("--live-confirmation")
    parser.add_argument("--provider", default="fixture")
    parser.add_argument("--provider-base-url")
    parser.add_argument("--model", default="fixture-deterministic")
    parser.add_argument("--tokenizer", default="fixture-exact.v1")
    parser.add_argument("--credential-env")
    parser.add_argument("--kill-switch-env", default="ACP_EFFICIENCY_BENCHMARK_KILL_SWITCH")
    parser.add_argument("--pricing-id", default="fixture-zero-cost.v1")
    parser.add_argument("--pricing-effective-date", default=FIXTURE_PRICING_EFFECTIVE_DATE)
    parser.add_argument("--input-cost-per-1k-usd", type=float, default=0.0)
    parser.add_argument("--output-cost-per-1k-usd", type=float, default=0.0)
    parser.add_argument("--per-call-cost-cap-usd", type=float, default=0.01)
    parser.add_argument("--run-cost-cap-usd", type=float, default=0.10)
    parser.add_argument("--daily-cost-cap-usd", type=float, default=0.50)
    parser.add_argument("--timeout-seconds", type=float, default=30.0)
    parser.add_argument("--max-calls", type=int, default=24)
    parser.add_argument("--max-tokens", type=int, default=120_000)
    parser.add_argument("--output-limit-tokens", type=int, default=256)
    parser.add_argument("--retry-policy", default="no-retry.v1")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        report, _ = execute(args)
    except (BenchmarkError, OSError, ValueError) as exc:
        print(f"efficiency benchmark refused: {exc}", file=sys.stderr)
        return 1
    print(
        json.dumps(
            {
                "benchmark_run_id": report["benchmark_run_id"],
                "report_sha256": report["report_sha256"],
                "acceptance_status": report["acceptance_status"],
            },
            sort_keys=True,
            separators=(",", ":"),
        )
    )
    if report["acceptance_status"] == "FAIL":
        return 2
    if report["acceptance_status"] == "INCOMPARABLE":
        return 3
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
