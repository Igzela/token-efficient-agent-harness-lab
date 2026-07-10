from __future__ import annotations

import argparse
import hashlib
import importlib.metadata
import json
import time
from datetime import UTC, datetime
from pathlib import Path
from typing import TypedDict

import tiktoken
from langgraph.graph import END, START, StateGraph
from langgraph.store.memory import InMemoryStore


SEED = 165
SCENARIO_ID = "langgraph_offline_state_retention_pilot_2026_07_10"
SCENARIO_SPEC = "offline-langgraph-state-retention-summary-v1"
TASK_SPEC = "deterministic-incremental-sha256-six-iterations-v1"
CAPTURE_ID = "langgraph-offline-pilot-20260710"


class PilotState(TypedDict):
    chunks: list[str]
    current_chunk: str
    digest: str


def token_count(encoding: tiktoken.Encoding, value: str) -> int:
    return len(encoding.encode(value))


def next_digest(previous: str, chunk: str) -> str:
    return hashlib.sha256(f"{previous}:{chunk}".encode("utf-8")).hexdigest()


def build_chunks() -> list[str]:
    chunks = []
    for iteration in range(6):
        atoms = [
            hashlib.sha256(f"pilot-{SEED}-{iteration}-{index}".encode("utf-8")).hexdigest()
            for index in range(48)
        ]
        chunks.append("|".join(atoms))
    return chunks


def expected_digest(chunks: list[str]) -> str:
    digest = ""
    for chunk in chunks:
        digest = next_digest(digest, chunk)
    return digest


def run_stateless(chunks: list[str], encoding: tiktoken.Encoding) -> dict[str, int | float | str]:
    def reread_node(state: PilotState) -> dict[str, str]:
        digest = ""
        for chunk in state["chunks"]:
            digest = next_digest(digest, chunk)
        return {"digest": digest}

    builder = StateGraph(PilotState)
    builder.add_node("reread", reread_node)
    builder.add_edge(START, "reread")
    builder.add_edge("reread", END)
    graph = builder.compile()

    input_total = 0
    output_total = 0
    context_total = 0
    repeated_total = 0
    digest = ""
    started = time.perf_counter()
    for index in range(len(chunks)):
        visible = chunks[: index + 1]
        input_tokens = sum(token_count(encoding, chunk) for chunk in visible)
        repeated_tokens = sum(token_count(encoding, chunk) for chunk in visible[:-1])
        result = graph.invoke({"chunks": visible, "current_chunk": "", "digest": ""})
        digest = result["digest"]
        input_total += input_tokens
        context_total += input_tokens
        repeated_total += repeated_tokens
        output_total += token_count(encoding, digest)
    duration_ms = max(1, round((time.perf_counter() - started) * 1000))
    return {
        "digest": digest,
        "input_token_total": input_total,
        "output_token_total": output_total,
        "context_token_total": context_total,
        "repeated_context_token_total": repeated_total,
        "retrieved_ref_token_total": 0,
        "duration_ms": duration_ms,
    }


def run_stateful(chunks: list[str], encoding: tiktoken.Encoding) -> dict[str, int | float | str]:
    store = InMemoryStore()
    namespace = ("pilot", CAPTURE_ID)

    def store_node(state: PilotState) -> dict[str, str]:
        item = store.get(namespace, "digest")
        previous = item.value["digest"] if item is not None else ""
        digest = next_digest(previous, state["current_chunk"])
        store.put(namespace, "digest", {"digest": digest})
        return {"digest": digest}

    builder = StateGraph(PilotState)
    builder.add_node("store", store_node)
    builder.add_edge(START, "store")
    builder.add_edge("store", END)
    graph = builder.compile(store=store)

    input_total = 0
    output_total = 0
    context_total = 0
    repeated_total = 0
    retrieved_total = 0
    digest = ""
    started = time.perf_counter()
    for chunk in chunks:
        item = store.get(namespace, "digest")
        previous = item.value["digest"] if item is not None else ""
        current_tokens = token_count(encoding, chunk)
        prior_digest_tokens = token_count(encoding, previous) if previous else 0
        result = graph.invoke({"chunks": [], "current_chunk": chunk, "digest": previous})
        digest = result["digest"]
        input_total += current_tokens + prior_digest_tokens
        context_total += current_tokens + prior_digest_tokens
        repeated_total += prior_digest_tokens
        retrieved_total += prior_digest_tokens
        output_total += token_count(encoding, digest)
    duration_ms = max(1, round((time.perf_counter() - started) * 1000))
    return {
        "digest": digest,
        "input_token_total": input_total,
        "output_token_total": output_total,
        "context_token_total": context_total,
        "repeated_context_token_total": repeated_total,
        "retrieved_ref_token_total": retrieved_total,
        "duration_ms": duration_ms,
    }


def summary(mode: str, measurements: dict[str, int | float | str], runtime_version: str, captured_at: str, capture_hash: str) -> dict[str, object]:
    quality_score = 1.0 if measurements["digest"] == EXPECTED_DIGEST else 0.0
    stateful = mode == "stateful_store"
    return {
        "schema_version": "token_efficiency_scorecard.v1",
        "adapter_run_id": f"{CAPTURE_ID}-{mode}",
        "runtime_kind": "langgraph",
        "runtime_version": runtime_version,
        "scenario_id": SCENARIO_ID,
        "mode": mode,
        "state_strategy": "durable_state" if stateful else "full_history",
        "status": "pass" if quality_score == 1.0 else "fail",
        "pass_fail_reason": "deterministic digest evaluator met shared threshold" if quality_score == 1.0 else "deterministic digest evaluator failed",
        "quality_score": quality_score,
        "quality_method": "rule",
        "comparison_contract": {
            "scenario_digest": hashlib.sha256(SCENARIO_SPEC.encode("utf-8")).hexdigest(),
            "task_digest": hashlib.sha256(TASK_SPEC.encode("utf-8")).hexdigest(),
            "runtime_kind": "langgraph",
            "runtime_version": runtime_version,
            "provider_id": "no-external-provider",
            "model_id": "deterministic-sha256-node.v1",
            "tokenizer_id": "tiktoken:cl100k_base",
            "pricing_id": "no-provider-zero-cost.v1",
            "input_cost_per_1k_usd": 0.0,
            "output_cost_per_1k_usd": 0.0,
            "quality_method": "rule",
            "quality_threshold": 1.0,
            "evaluator_version": "sha256-digest-rule.v1",
            "redaction_policy": "summary-only-no-payload.v1",
            "retry_policy": "no-retry.v1",
            "seed": SEED,
        },
        "input_token_total": measurements["input_token_total"],
        "output_token_total": measurements["output_token_total"],
        "context_token_total": measurements["context_token_total"],
        "repeated_context_token_total": measurements["repeated_context_token_total"],
        "retrieved_ref_token_total": measurements["retrieved_ref_token_total"],
        "tool_call_count": 0,
        "redundant_tool_call_count": 0,
        "retry_count": 0,
        "step_count": len(CHUNKS),
        "duration_ms": measurements["duration_ms"],
        "estimated_cost_usd": 0.0,
        "raw_trace_artifact_id": f"{CAPTURE_ID}-summary-only",
        "redaction_status": "redacted",
        "evidence_provenance": {
            "capture_id": CAPTURE_ID,
            "captured_at": captured_at,
            "source_kind": "external_runtime_offline",
            "external_model_calls": 0,
            "summary_level": True,
            "source_capture_sha256": capture_hash,
        },
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    runtime_version = importlib.metadata.version("langgraph")
    captured_at = datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")
    capture_hash = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
    stateless = summary("stateless_reread", run_stateless(CHUNKS, ENCODING), runtime_version, captured_at, capture_hash)
    stateful = summary("stateful_store", run_stateful(CHUNKS, ENCODING), runtime_version, captured_at, capture_hash)
    if stateless["status"] != "pass" or stateful["status"] != "pass":
        raise SystemExit("pilot quality verification failed")
    (args.output_dir / "stateless_reread.summary.json").write_text(json.dumps(stateless, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    (args.output_dir / "stateful_store.summary.json").write_text(json.dumps(stateful, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps({
        "runtime_version": runtime_version,
        "captured_at": captured_at,
        "capture_sha256": capture_hash,
        "external_model_calls": 0,
        "stateless_total_tokens": stateless["input_token_total"] + stateless["output_token_total"],
        "stateful_total_tokens": stateful["input_token_total"] + stateful["output_token_total"],
    }, sort_keys=True))


ENCODING = tiktoken.get_encoding("cl100k_base")
CHUNKS = build_chunks()
EXPECTED_DIGEST = expected_digest(CHUNKS)


if __name__ == "__main__":
    main()
