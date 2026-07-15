"""One-shot, content-free LangGraph external-runtime adapter.

Rust owns authorization, provider execution, leases, persistence, and budgets.
This process is only an Adapter: validate a bounded request, invoke one graph
once, and return a bounded summary that can be committed by the Rust owner.
"""

from __future__ import annotations

import hashlib
import importlib.metadata
import json
import math
import os
import re
import sys
import time
from collections.abc import Mapping
from contextlib import contextmanager
from typing import Any, TypedDict

from langgraph.graph import END, START, StateGraph


REQUEST_SCHEMA_VERSION = "external_runtime_request.v1"
RESULT_SCHEMA_VERSION = "external_runtime_result.v1"
ERROR_SCHEMA_VERSION = "external_runtime_error.v1"
ADAPTER_CONTRACT_VERSION = "external_runtime_adapter.v1"
ADAPTER_VERSION = "0.1.0"
LANGGRAPH_VERSION = "1.2.9"
RUNTIME_KIND = "langgraph"

MAX_INPUT_BYTES = 256 * 1024
MAX_OUTPUT_BYTES = 128 * 1024
MAX_STRING_BYTES = 512
MAX_STATE_BYTES = 16 * 1024
MAX_COLLECTION_ITEMS = 64
MAX_DEPTH = 8
MAX_ATTEMPT = 1_000_000
MAX_COUNTER = 1_000_000_000

MEMORY_STRATEGIES = frozenset(
    {
        "full_history",
        "summary_memory",
        "retrieval_memory",
        "durable_state_bounded_recent",
    }
)
MODES = frozenset({"fixture", "live"})
METRIC_PROVENANCE = frozenset(
    {
        "provider_reported",
        "tokenizer_exact",
        "harness_derived",
        "estimated",
        "unavailable",
    }
)

_ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:@-]{0,127}$")
_SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
_SECRET_VALUE_PATTERNS = (
    re.compile(r"\b(?:sk|rk|pk)-[A-Za-z0-9_-]{16,}\b", re.IGNORECASE),
    re.compile(r"\bapi[_-]?key\s*[:=]", re.IGNORECASE),
    re.compile(
        r"\b(?:authorization|bearer|password|secret|token)\s*[:=]", re.IGNORECASE
    ),
    re.compile(r"-----BEGIN [A-Z ]+PRIVATE KEY-----"),
    re.compile(r"(?:^|\s)/(?:home|Users|private|var|etc)/"),
    re.compile(r"[A-Za-z]:\\(?:Users|Windows|private)\\", re.IGNORECASE),
)
_FORBIDDEN_KEY_PARTS = (
    "raw_prompt",
    "system_prompt",
    "raw_output",
    "transcript",
    "message",
    "repository_content",
    "repo_content",
    "private_path",
    "credential",
    "authorization",
    "api_key",
    "password",
    "secret",
    "tool_payload",
)

_TOP_LEVEL_FIELDS = frozenset(
    {
        "schema_version",
        "invocation_id",
        "tenant_id",
        "workspace_id",
        "run_id",
        "workflow_id",
        "node_id",
        "thread_id",
        "attempt",
        "mode",
        "memory_strategy",
        "runtime",
        "scope_binding_sha256",
        "request_sha256",
        "checkpoint",
        "provider_exchange",
        "benchmark",
    }
)
_RUNTIME_FIELDS = frozenset(
    {
        "runtime_kind",
        "adapter_contract_version",
        "adapter_version",
        "expected_langgraph_version",
    }
)
_CHECKPOINT_FIELDS = frozenset(
    {
        "checkpoint_id",
        "version",
        "parent_checkpoint_id",
        "state_summary",
        "state_sha256",
    }
)
_STATE_FIELDS = frozenset(
    {
        "memory_digest",
        "summary_digest",
        "fact_ids",
        "selected_reference_ids",
        "recent_event_hashes",
        "turn_count",
        "conflict_count",
        "correction_count",
    }
)
_BENCHMARK_FIELDS = frozenset(
    {
        "definition_sha256",
        "scenario_id",
        "scenario_sha256",
        "task_sha256",
        "seed",
        "quality_threshold",
        "provider_id",
        "model_id",
        "tokenizer_id",
        "pricing_id",
        "required_reference_ids",
        "candidate_reference_ids",
        "selected_reference_ids",
        "stale_reference_ids",
        "context_tokens",
        "repeated_context_tokens",
        "state_read_bytes",
        "state_write_bytes",
        "memory_maintenance_tokens",
        "memory_maintenance_cost_usd",
        "tool_call_count",
        "redundant_tool_call_count",
    }
)
_EXCHANGE_FIELDS = frozenset(
    {
        "exchange_id",
        "invocation_id",
        "scope_binding_sha256",
        "provider_id",
        "model_id",
        "response_sha256",
        "typed_result",
        "usage",
        "metric_provenance",
    }
)
_TYPED_RESULT_FIELDS = frozenset(
    {"status", "decision_code", "selected_tool_ids", "quality_score", "quality_method"}
)
_USAGE_FIELDS = frozenset(
    {
        "input_tokens",
        "output_tokens",
        "cached_input_tokens",
        "cache_write_tokens",
        "reasoning_tokens",
        "estimated_cost_usd",
        "provider_reported_cost_usd",
        "latency_ms",
        "retry_count",
    }
)


class AdapterError(ValueError):
    """A stable, safe adapter failure that never includes input values."""

    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code
        self.message = message


class GraphState(TypedDict):
    request: dict[str, Any]
    result: dict[str, Any] | None


def _canonical_json(value: Any) -> bytes:
    try:
        return json.dumps(
            value,
            sort_keys=True,
            separators=(",", ":"),
            ensure_ascii=True,
            allow_nan=False,
        ).encode("utf-8")
    except (TypeError, ValueError) as exc:
        raise AdapterError(
            "invalid_json_value", "request contains a non-canonical JSON value"
        ) from exc


def canonical_sha256(value: Any) -> str:
    return hashlib.sha256(_canonical_json(value)).hexdigest()


def scope_binding_sha256(request: Mapping[str, Any]) -> str:
    return canonical_sha256(
        {
            "tenant_id": request.get("tenant_id"),
            "workspace_id": request.get("workspace_id"),
            "run_id": request.get("run_id"),
            "workflow_id": request.get("workflow_id"),
            "node_id": request.get("node_id"),
            "thread_id": request.get("thread_id"),
        }
    )


def request_sha256(request: Mapping[str, Any]) -> str:
    material = dict(request)
    material.pop("request_sha256", None)
    material.pop("invocation_id", None)
    material.pop("provider_exchange", None)
    return canonical_sha256(material)


def _checkpoint_id(request: Mapping[str, Any], version: int, state_sha256: str) -> str:
    digest = canonical_sha256(
        {
            "scope_binding_sha256": request["scope_binding_sha256"],
            "node_id": request["node_id"],
            "thread_id": request["thread_id"],
            "version": version,
            "state_sha256": state_sha256,
        }
    )
    return f"ckpt-{digest}"


def _require_object(value: Any, path: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise AdapterError("invalid_type", f"{path} must be an object")
    return value


def _require_exact_fields(
    value: Mapping[str, Any], allowed: frozenset[str], path: str
) -> None:
    unknown = sorted(set(value) - allowed)
    missing = sorted(allowed - set(value))
    if unknown:
        raise AdapterError("unknown_field", f"{path} contains unknown fields")
    if missing:
        raise AdapterError("missing_field", f"{path} is missing required fields")


def _require_id(value: Any, path: str) -> str:
    if not isinstance(value, str) or not _ID_RE.fullmatch(value):
        raise AdapterError("invalid_identifier", f"{path} must be a bounded identifier")
    return value


def _require_sha256(value: Any, path: str) -> str:
    if not isinstance(value, str) or not _SHA256_RE.fullmatch(value):
        raise AdapterError(
            "invalid_sha256", f"{path} must be a lowercase SHA-256 digest"
        )
    return value


def _require_int(value: Any, path: str, *, maximum: int = MAX_COUNTER) -> int:
    if (
        isinstance(value, bool)
        or not isinstance(value, int)
        or not 0 <= value <= maximum
    ):
        raise AdapterError(
            "invalid_integer", f"{path} must be a bounded non-negative integer"
        )
    return value


def _require_optional_int(value: Any, path: str) -> int | None:
    return None if value is None else _require_int(value, path)


def _require_number(value: Any, path: str, *, maximum: float = 1_000_000.0) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise AdapterError("invalid_number", f"{path} must be a bounded number")
    result = float(value)
    if not math.isfinite(result) or not 0.0 <= result <= maximum:
        raise AdapterError("invalid_number", f"{path} must be a bounded number")
    return result


def _require_optional_number(value: Any, path: str) -> float | None:
    return None if value is None else _require_number(value, path)


def _require_ids(value: Any, path: str) -> list[str]:
    if not isinstance(value, list) or len(value) > MAX_COLLECTION_ITEMS:
        raise AdapterError(
            "invalid_collection", f"{path} must be a bounded identifier list"
        )
    result = [_require_id(item, f"{path}[]") for item in value]
    if len(result) != len(set(result)):
        raise AdapterError(
            "duplicate_identifier", f"{path} must not contain duplicates"
        )
    return result


def _reject_sensitive(value: Any, path: str = "request", depth: int = 0) -> None:
    if depth > MAX_DEPTH:
        raise AdapterError(
            "max_depth_exceeded", "request nesting exceeds the adapter limit"
        )
    if isinstance(value, dict):
        if len(value) > MAX_COLLECTION_ITEMS:
            raise AdapterError(
                "collection_too_large", f"{path} contains too many fields"
            )
        for key, child in value.items():
            if not isinstance(key, str):
                raise AdapterError("invalid_key", f"{path} contains a non-string key")
            if len(key.encode("utf-8")) > 64:
                raise AdapterError(
                    "field_name_too_large", f"{path} contains an oversized field name"
                )
            normalized = key.lower().replace("-", "_")
            if any(part in normalized for part in _FORBIDDEN_KEY_PARTS):
                raise AdapterError(
                    "sensitive_field", f"{path} contains a raw or sensitive field"
                )
            _reject_sensitive(child, f"{path}.{key}", depth + 1)
        return
    if isinstance(value, list):
        if len(value) > MAX_COLLECTION_ITEMS:
            raise AdapterError(
                "collection_too_large", f"{path} contains too many items"
            )
        for child in value:
            _reject_sensitive(child, f"{path}[]", depth + 1)
        return
    if isinstance(value, str):
        if len(value.encode("utf-8")) > MAX_STRING_BYTES:
            raise AdapterError(
                "string_too_large", f"{path} contains an oversized string"
            )
        if any(pattern.search(value) for pattern in _SECRET_VALUE_PATTERNS):
            raise AdapterError(
                "secret_shaped_value", f"{path} contains a secret-shaped value"
            )
        return
    if value is not None and not isinstance(value, (bool, int, float)):
        raise AdapterError(
            "invalid_json_value", f"{path} contains an unsupported value"
        )


def _validate_runtime(value: Any) -> dict[str, Any]:
    runtime = _require_object(value, "runtime")
    _require_exact_fields(runtime, _RUNTIME_FIELDS, "runtime")
    expected = {
        "runtime_kind": RUNTIME_KIND,
        "adapter_contract_version": ADAPTER_CONTRACT_VERSION,
        "adapter_version": ADAPTER_VERSION,
        "expected_langgraph_version": LANGGRAPH_VERSION,
    }
    if runtime != expected:
        raise AdapterError(
            "runtime_identity_mismatch", "runtime identity does not match this adapter"
        )
    try:
        installed = importlib.metadata.version("langgraph")
    except importlib.metadata.PackageNotFoundError as exc:
        raise AdapterError(
            "runtime_unavailable", "the pinned LangGraph runtime is unavailable"
        ) from exc
    if installed != LANGGRAPH_VERSION:
        raise AdapterError(
            "runtime_version_mismatch",
            "the installed LangGraph version is not authorized",
        )
    return runtime


def _validate_state_summary(value: Any) -> dict[str, Any]:
    state = _require_object(value, "checkpoint.state_summary")
    _require_exact_fields(state, _STATE_FIELDS, "checkpoint.state_summary")
    _require_sha256(state["memory_digest"], "checkpoint.state_summary.memory_digest")
    if state["summary_digest"] is not None:
        _require_sha256(
            state["summary_digest"], "checkpoint.state_summary.summary_digest"
        )
    _require_ids(state["fact_ids"], "checkpoint.state_summary.fact_ids")
    _require_ids(
        state["selected_reference_ids"],
        "checkpoint.state_summary.selected_reference_ids",
    )
    hashes = state["recent_event_hashes"]
    if not isinstance(hashes, list) or len(hashes) > 8:
        raise AdapterError(
            "invalid_collection",
            "checkpoint.state_summary.recent_event_hashes must be bounded",
        )
    for item in hashes:
        _require_sha256(item, "checkpoint.state_summary.recent_event_hashes[]")
    _require_int(
        state["turn_count"],
        "checkpoint.state_summary.turn_count",
        maximum=MAX_COUNTER - 1,
    )
    for field in ("conflict_count", "correction_count"):
        _require_int(state[field], f"checkpoint.state_summary.{field}")
    if len(_canonical_json(state)) > MAX_STATE_BYTES:
        raise AdapterError(
            "checkpoint_too_large", "checkpoint state summary exceeds the adapter limit"
        )
    return state


def _validate_checkpoint(
    value: Any, request: Mapping[str, Any]
) -> dict[str, Any] | None:
    if value is None:
        return None
    checkpoint = _require_object(value, "checkpoint")
    _require_exact_fields(checkpoint, _CHECKPOINT_FIELDS, "checkpoint")
    _require_id(checkpoint["checkpoint_id"], "checkpoint.checkpoint_id")
    version = _require_int(
        checkpoint["version"], "checkpoint.version", maximum=MAX_COUNTER - 1
    )
    if version == 0:
        raise AdapterError(
            "invalid_checkpoint_version", "checkpoint.version must be positive"
        )
    if checkpoint["parent_checkpoint_id"] is not None:
        _require_id(
            checkpoint["parent_checkpoint_id"], "checkpoint.parent_checkpoint_id"
        )
    state = _validate_state_summary(checkpoint["state_summary"])
    state_hash = _require_sha256(checkpoint["state_sha256"], "checkpoint.state_sha256")
    if state_hash != canonical_sha256(state):
        raise AdapterError(
            "checkpoint_hash_mismatch",
            "checkpoint state hash does not match its summary",
        )
    if checkpoint["checkpoint_id"] != _checkpoint_id(request, version, state_hash):
        raise AdapterError(
            "checkpoint_scope_mismatch",
            "checkpoint identity is not bound to this scope",
        )
    return checkpoint


def _validate_benchmark(value: Any) -> dict[str, Any]:
    benchmark = _require_object(value, "benchmark")
    _require_exact_fields(benchmark, _BENCHMARK_FIELDS, "benchmark")
    for field in ("definition_sha256", "scenario_sha256", "task_sha256"):
        _require_sha256(benchmark[field], f"benchmark.{field}")
    for field in (
        "scenario_id",
        "provider_id",
        "model_id",
        "tokenizer_id",
        "pricing_id",
    ):
        _require_id(benchmark[field], f"benchmark.{field}")
    _require_int(benchmark["seed"], "benchmark.seed")
    _require_number(
        benchmark["quality_threshold"], "benchmark.quality_threshold", maximum=1.0
    )
    list_fields = (
        "required_reference_ids",
        "candidate_reference_ids",
        "selected_reference_ids",
        "stale_reference_ids",
    )
    for field in list_fields:
        _require_ids(benchmark[field], f"benchmark.{field}")
    candidates = set(benchmark["candidate_reference_ids"])
    selected = set(benchmark["selected_reference_ids"])
    stale = set(benchmark["stale_reference_ids"])
    required = set(benchmark["required_reference_ids"])
    if (
        not selected <= candidates
        or not stale <= selected
        or not required <= candidates
    ):
        raise AdapterError(
            "reference_binding_mismatch",
            "benchmark references are not consistently bound",
        )
    for field in (
        "context_tokens",
        "repeated_context_tokens",
        "state_read_bytes",
        "state_write_bytes",
        "memory_maintenance_tokens",
        "tool_call_count",
        "redundant_tool_call_count",
    ):
        _require_int(benchmark[field], f"benchmark.{field}")
    _require_optional_number(
        benchmark["memory_maintenance_cost_usd"],
        "benchmark.memory_maintenance_cost_usd",
    )
    if benchmark["repeated_context_tokens"] > benchmark["context_tokens"]:
        raise AdapterError(
            "invalid_metric_relation", "repeated context cannot exceed total context"
        )
    if benchmark["redundant_tool_call_count"] > benchmark["tool_call_count"]:
        raise AdapterError(
            "invalid_metric_relation",
            "redundant tool calls cannot exceed total tool calls",
        )
    return benchmark


def _validate_provider_exchange(
    value: Any, request: Mapping[str, Any]
) -> dict[str, Any] | None:
    if request["mode"] == "fixture":
        if value is not None:
            raise AdapterError(
                "fixture_exchange_forbidden",
                "fixture mode must not include a provider exchange",
            )
        return None
    exchange = _require_object(value, "provider_exchange")
    _require_exact_fields(exchange, _EXCHANGE_FIELDS, "provider_exchange")
    for field in ("exchange_id", "invocation_id", "provider_id", "model_id"):
        _require_id(exchange[field], f"provider_exchange.{field}")
    _require_sha256(
        exchange["scope_binding_sha256"], "provider_exchange.scope_binding_sha256"
    )
    _require_sha256(exchange["response_sha256"], "provider_exchange.response_sha256")
    if exchange["invocation_id"] != request["invocation_id"]:
        raise AdapterError(
            "exchange_invocation_mismatch",
            "provider exchange is bound to a different invocation",
        )
    if exchange["scope_binding_sha256"] != request["scope_binding_sha256"]:
        raise AdapterError(
            "exchange_scope_mismatch", "provider exchange is bound to a different scope"
        )
    benchmark = request["benchmark"]
    if (
        exchange["provider_id"] != benchmark["provider_id"]
        or exchange["model_id"] != benchmark["model_id"]
    ):
        raise AdapterError(
            "exchange_provider_mismatch",
            "provider exchange identity does not match the benchmark",
        )

    typed = _require_object(exchange["typed_result"], "provider_exchange.typed_result")
    _require_exact_fields(typed, _TYPED_RESULT_FIELDS, "provider_exchange.typed_result")
    if typed["status"] not in {"pass", "fail"}:
        raise AdapterError(
            "invalid_status", "provider_exchange.typed_result.status is invalid"
        )
    _require_id(typed["decision_code"], "provider_exchange.typed_result.decision_code")
    _require_ids(
        typed["selected_tool_ids"], "provider_exchange.typed_result.selected_tool_ids"
    )
    if typed["quality_score"] is not None:
        _require_number(
            typed["quality_score"],
            "provider_exchange.typed_result.quality_score",
            maximum=1.0,
        )
    _require_id(
        typed["quality_method"], "provider_exchange.typed_result.quality_method"
    )

    usage = _require_object(exchange["usage"], "provider_exchange.usage")
    _require_exact_fields(usage, _USAGE_FIELDS, "provider_exchange.usage")
    for field in (
        "input_tokens",
        "output_tokens",
        "cached_input_tokens",
        "cache_write_tokens",
        "reasoning_tokens",
        "latency_ms",
        "retry_count",
    ):
        _require_optional_int(usage[field], f"provider_exchange.usage.{field}")
    for field in ("estimated_cost_usd", "provider_reported_cost_usd"):
        _require_optional_number(usage[field], f"provider_exchange.usage.{field}")

    provenance = _require_object(
        exchange["metric_provenance"], "provider_exchange.metric_provenance"
    )
    _require_exact_fields(
        provenance, _USAGE_FIELDS, "provider_exchange.metric_provenance"
    )
    for field in _USAGE_FIELDS:
        if provenance[field] not in METRIC_PROVENANCE:
            raise AdapterError(
                "invalid_provenance",
                "provider exchange contains unsupported metric provenance",
            )
        if usage[field] is None and provenance[field] != "unavailable":
            raise AdapterError(
                "provenance_value_mismatch",
                "unavailable provider metrics must be labeled unavailable",
            )
        if usage[field] is not None and provenance[field] == "unavailable":
            raise AdapterError(
                "provenance_value_mismatch",
                "available provider metrics cannot be labeled unavailable",
            )
    return exchange


def validate_request(value: Any) -> dict[str, Any]:
    request = _require_object(value, "request")
    _reject_sensitive(request)
    _require_exact_fields(request, _TOP_LEVEL_FIELDS, "request")
    if request["schema_version"] != REQUEST_SCHEMA_VERSION:
        raise AdapterError(
            "schema_version_mismatch", "request schema version is unsupported"
        )
    for field in (
        "invocation_id",
        "tenant_id",
        "workspace_id",
        "run_id",
        "workflow_id",
        "node_id",
        "thread_id",
    ):
        _require_id(request[field], field)
    attempt = _require_int(request["attempt"], "attempt", maximum=MAX_ATTEMPT)
    if attempt == 0:
        raise AdapterError("invalid_attempt", "attempt must be positive")
    if request["mode"] not in MODES:
        raise AdapterError("invalid_mode", "mode must be fixture or live")
    if request["memory_strategy"] not in MEMORY_STRATEGIES:
        raise AdapterError("invalid_memory_strategy", "memory strategy is unsupported")
    _validate_runtime(request["runtime"])
    expected_scope = scope_binding_sha256(request)
    if (
        _require_sha256(request["scope_binding_sha256"], "scope_binding_sha256")
        != expected_scope
    ):
        raise AdapterError(
            "scope_binding_mismatch",
            "request scope binding does not match its identities",
        )
    _validate_benchmark(request["benchmark"])
    _validate_checkpoint(request["checkpoint"], request)
    _validate_provider_exchange(request["provider_exchange"], request)
    expected_request = request_sha256(request)
    if _require_sha256(request["request_sha256"], "request_sha256") != expected_request:
        raise AdapterError(
            "request_hash_mismatch", "request hash does not match the canonical request"
        )
    return request


def _fixture_usage(strategy: str) -> tuple[dict[str, Any], dict[str, str]]:
    input_tokens = {
        "full_history": 1200,
        "summary_memory": 720,
        "retrieval_memory": 560,
        "durable_state_bounded_recent": 640,
    }[strategy]
    usage: dict[str, Any] = {
        "input_tokens": input_tokens,
        "output_tokens": 80,
        "cached_input_tokens": None,
        "cache_write_tokens": None,
        "reasoning_tokens": None,
        "estimated_cost_usd": round((input_tokens + 80) * 0.000002, 9),
        "provider_reported_cost_usd": None,
        "latency_ms": 1,
        "retry_count": 0,
    }
    provenance = {
        field: ("unavailable" if value is None else "harness_derived")
        for field, value in usage.items()
    }
    return usage, provenance


def _ratio(numerator: int, denominator: int) -> float | None:
    return None if denominator == 0 else round(numerator / denominator, 6)


def _next_state(request: Mapping[str, Any], event_hash: str) -> dict[str, Any]:
    checkpoint = request["checkpoint"]
    previous = checkpoint["state_summary"] if checkpoint else None
    previous_memory = previous["memory_digest"] if previous else "0" * 64
    strategy = request["memory_strategy"]
    benchmark = request["benchmark"]
    turn_count = (previous["turn_count"] if previous else 0) + 1
    memory_digest = canonical_sha256(
        {
            "previous_memory_digest": previous_memory,
            "event_hash": event_hash,
            "definition_sha256": benchmark["definition_sha256"],
            "strategy": strategy,
            "turn_count": turn_count,
        }
    )
    summary_digest = (
        canonical_sha256(
            {
                "memory_digest": memory_digest,
                "strategy": strategy,
                "selected_reference_ids": benchmark["selected_reference_ids"],
            }
        )
        if strategy != "full_history"
        else None
    )
    previous_events = list(previous["recent_event_hashes"]) if previous else []
    recent_limit = 1 if strategy == "full_history" else 4
    recent_events = (previous_events + [event_hash])[-recent_limit:]
    return {
        "memory_digest": memory_digest,
        "summary_digest": summary_digest,
        "fact_ids": list(benchmark["required_reference_ids"]),
        "selected_reference_ids": (
            list(benchmark["selected_reference_ids"])
            if strategy == "retrieval_memory"
            else []
        ),
        "recent_event_hashes": recent_events,
        "turn_count": turn_count,
        "conflict_count": previous["conflict_count"] if previous else 0,
        "correction_count": previous["correction_count"] if previous else 0,
    }


def _execute_node(state: GraphState) -> dict[str, Any]:
    request = state["request"]
    benchmark = request["benchmark"]
    exchange = request["provider_exchange"]
    event_hash = (
        exchange["response_sha256"]
        if exchange is not None
        else canonical_sha256(
            {
                "fixture": True,
                "invocation_id": request["invocation_id"],
                "definition_sha256": benchmark["definition_sha256"],
                "attempt": request["attempt"],
            }
        )
    )
    next_state = _next_state(request, event_hash)
    next_state_hash = canonical_sha256(next_state)
    previous_checkpoint = request["checkpoint"]
    next_version = (previous_checkpoint["version"] if previous_checkpoint else 0) + 1
    checkpoint_next = {
        "checkpoint_id": _checkpoint_id(request, next_version, next_state_hash),
        "version": next_version,
        "parent_checkpoint_id": previous_checkpoint["checkpoint_id"]
        if previous_checkpoint
        else None,
        "state_summary": next_state,
        "state_sha256": next_state_hash,
    }

    if exchange is None:
        usage, provenance = _fixture_usage(request["memory_strategy"])
        status = "pass"
        quality_score: float | None = 1.0
        quality_method = "deterministic_fixture.v1"
        selected_tool_ids: list[str] = []
    else:
        usage = dict(exchange["usage"])
        provenance = dict(exchange["metric_provenance"])
        typed = exchange["typed_result"]
        status = typed["status"]
        quality_score = typed["quality_score"]
        quality_method = typed["quality_method"]
        selected_tool_ids = list(typed["selected_tool_ids"])

    required = set(benchmark["required_reference_ids"])
    selected = set(benchmark["selected_reference_ids"])
    stale = set(benchmark["stale_reference_ids"])
    precision = _ratio(len(required & selected), len(selected))
    recall = _ratio(len(required & selected), len(required))
    stale_rate = _ratio(len(stale), len(selected))
    provider_fields = (
        "input_tokens",
        "output_tokens",
        "estimated_cost_usd",
        "latency_ms",
    )
    available = sum(usage[field] is not None for field in provider_fields)
    completeness = round(available / len(provider_fields), 6)
    confidence = (
        "high" if completeness == 1.0 else "medium" if completeness >= 0.5 else "low"
    )

    scorecard = {
        "schema_version": "external_runtime_scorecard_summary.v1",
        "runtime_kind": RUNTIME_KIND,
        "runtime_version": LANGGRAPH_VERSION,
        "adapter_version": ADAPTER_VERSION,
        "definition_sha256": benchmark["definition_sha256"],
        "scenario_id": benchmark["scenario_id"],
        "scenario_sha256": benchmark["scenario_sha256"],
        "task_sha256": benchmark["task_sha256"],
        "memory_strategy": request["memory_strategy"],
        "mode": request["mode"],
        "provider_id": benchmark["provider_id"],
        "model_id": benchmark["model_id"],
        "tokenizer_id": benchmark["tokenizer_id"],
        "pricing_id": benchmark["pricing_id"],
        "seed": benchmark["seed"],
        "status": status,
        "quality_score": quality_score,
        "quality_method": quality_method,
        "quality_threshold": benchmark["quality_threshold"],
        "input_tokens": usage["input_tokens"],
        "output_tokens": usage["output_tokens"],
        "cached_input_tokens": usage["cached_input_tokens"],
        "cache_write_tokens": usage["cache_write_tokens"],
        "reasoning_tokens": usage["reasoning_tokens"],
        "estimated_cost_usd": usage["estimated_cost_usd"],
        "provider_reported_cost_usd": usage["provider_reported_cost_usd"],
        "context_tokens": benchmark["context_tokens"],
        "repeated_context_tokens": benchmark["repeated_context_tokens"],
        "retrieval_candidate_count": len(benchmark["candidate_reference_ids"]),
        "retrieval_selected_count": len(benchmark["selected_reference_ids"]),
        "retrieval_precision": precision,
        "retrieval_recall": recall,
        "stale_memory_selection_rate": stale_rate,
        "correction_count": next_state["correction_count"],
        "conflict_count": next_state["conflict_count"],
        "state_read_bytes": benchmark["state_read_bytes"],
        "state_write_bytes": benchmark["state_write_bytes"],
        "memory_maintenance_tokens": benchmark["memory_maintenance_tokens"],
        "memory_maintenance_cost_usd": benchmark["memory_maintenance_cost_usd"],
        "tool_call_count": benchmark["tool_call_count"],
        "redundant_tool_call_count": benchmark["redundant_tool_call_count"],
        "selected_tool_count": len(selected_tool_ids),
        "retry_count": usage["retry_count"],
        "latency_ms": usage["latency_ms"],
        "restart_resumed": previous_checkpoint is not None,
        "metric_provenance": {
            **provenance,
            "context_tokens": "harness_derived",
            "repeated_context_tokens": "harness_derived",
            "retrieval_precision": "harness_derived",
            "retrieval_recall": "harness_derived",
            "stale_memory_selection_rate": "harness_derived",
            "state_read_bytes": "harness_derived",
            "state_write_bytes": "harness_derived",
            "memory_maintenance_tokens": "harness_derived",
            "memory_maintenance_cost_usd": (
                "unavailable"
                if benchmark["memory_maintenance_cost_usd"] is None
                else "harness_derived"
            ),
        },
        "measurement_completeness": completeness,
        "measurement_confidence": confidence,
    }
    trace = {
        "schema_version": "external_runtime_trace_summary.v1",
        "invocation_id": request["invocation_id"],
        "scope_binding_sha256": request["scope_binding_sha256"],
        "request_sha256": request["request_sha256"],
        "runtime_kind": RUNTIME_KIND,
        "runtime_version": LANGGRAPH_VERSION,
        "adapter_version": ADAPTER_VERSION,
        "node_count": 1,
        "graph_invoke_count": 1,
        "status": status,
        "checkpoint_before_sha256": previous_checkpoint["state_sha256"]
        if previous_checkpoint
        else None,
        "checkpoint_after_sha256": next_state_hash,
        "provider_response_sha256": exchange["response_sha256"] if exchange else None,
        "provider_exchanges_consumed": 0 if exchange is None else 1,
        "adapter_provider_calls": 0,
        "adapter_network_calls": 0,
        "summary_level": True,
    }
    return {
        "result": {
            "checkpoint_next": checkpoint_next,
            "scorecard_summary": scorecard,
            "trace_summary": trace,
        }
    }


@contextmanager
def _tracing_disabled() -> Any:
    """Prevent environment-enabled LangChain/LangSmith telemetry for this invocation."""

    keys = ("LANGSMITH_TRACING", "LANGCHAIN_TRACING_V2")
    previous = {key: os.environ.get(key) for key in keys}
    try:
        for key in keys:
            os.environ[key] = "false"
        yield
    finally:
        for key, value in previous.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value


def _invoke_graph_once(request: dict[str, Any]) -> dict[str, Any]:
    builder = StateGraph(GraphState)
    builder.add_node("bounded_external_runtime_step", _execute_node)
    builder.add_edge(START, "bounded_external_runtime_step")
    builder.add_edge("bounded_external_runtime_step", END)
    graph = builder.compile()
    with _tracing_disabled():
        invoked = graph.invoke({"request": request, "result": None})
    result = invoked.get("result")
    if not isinstance(result, dict):
        raise AdapterError(
            "invalid_graph_result", "LangGraph returned an invalid bounded result"
        )
    return result


def execute_request(value: Any) -> dict[str, Any]:
    request = validate_request(value)
    started = time.perf_counter()
    graph_result = _invoke_graph_once(request)
    adapter_duration_ms = max(0, round((time.perf_counter() - started) * 1000))
    result: dict[str, Any] = {
        "schema_version": RESULT_SCHEMA_VERSION,
        "invocation_id": request["invocation_id"],
        "tenant_id": request["tenant_id"],
        "workspace_id": request["workspace_id"],
        "run_id": request["run_id"],
        "workflow_id": request["workflow_id"],
        "node_id": request["node_id"],
        "thread_id": request["thread_id"],
        "attempt": request["attempt"],
        "mode": request["mode"],
        "memory_strategy": request["memory_strategy"],
        "runtime": {
            "runtime_kind": RUNTIME_KIND,
            "runtime_version": LANGGRAPH_VERSION,
            "adapter_contract_version": ADAPTER_CONTRACT_VERSION,
            "adapter_version": ADAPTER_VERSION,
        },
        "scope_binding_sha256": request["scope_binding_sha256"],
        "request_sha256": request["request_sha256"],
        "invocation_count": 1,
        "adapter_duration_ms": adapter_duration_ms,
        "checkpoint_next": graph_result["checkpoint_next"],
        "scorecard_summary": graph_result["scorecard_summary"],
        "trace_summary": graph_result["trace_summary"],
        "result_sha256": "",
    }
    material = dict(result)
    material.pop("result_sha256")
    result["result_sha256"] = canonical_sha256(material)
    _reject_sensitive(result, "result")
    rendered = _canonical_json(result)
    if len(rendered) > MAX_OUTPUT_BYTES:
        raise AdapterError(
            "result_too_large", "bounded adapter result exceeds the output limit"
        )
    return result


def _parse_input() -> dict[str, Any]:
    payload = sys.stdin.buffer.read(MAX_INPUT_BYTES + 1)
    if len(payload) > MAX_INPUT_BYTES:
        raise AdapterError(
            "request_too_large", "request exceeds the adapter input limit"
        )
    if not payload:
        raise AdapterError("empty_request", "request must contain one JSON object")
    try:
        value = json.loads(
            payload,
            parse_constant=lambda _value: (_ for _ in ()).throw(
                ValueError("non-finite number")
            ),
        )
    except (UnicodeDecodeError, json.JSONDecodeError, ValueError) as exc:
        raise AdapterError(
            "invalid_json", "request must contain one valid JSON object"
        ) from exc
    return _require_object(value, "request")


def _safe_error(error: AdapterError) -> str:
    return json.dumps(
        {
            "schema_version": ERROR_SCHEMA_VERSION,
            "code": error.code,
            "message": error.message,
        },
        sort_keys=True,
        separators=(",", ":"),
    )


def main() -> int:
    try:
        result = execute_request(_parse_input())
    except AdapterError as exc:
        print(_safe_error(exc), file=sys.stderr)
        return 2
    except Exception:
        print(
            _safe_error(
                AdapterError(
                    "internal_error", "adapter failed without producing a result"
                )
            ),
            file=sys.stderr,
        )
        return 3
    sys.stdout.buffer.write(_canonical_json(result) + b"\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
