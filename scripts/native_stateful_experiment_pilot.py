#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
SOURCE_REF_KEY = "raw" + "_trace_artifact_id"


def _load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


VALIDATOR = _load("token_efficiency_scorecard", ROOT / "scripts" / "token_efficiency_scorecard.py")
COMPARISON = _load("scorecard_comparison", ROOT / "scripts" / "scorecard_comparison.py")
SCENARIO_ID = "native_remember_dont_reread_pilot"
RUNTIME_VERSION = "native-stateful-pilot.v1"
MODES = {"stateless_reread", "stateful_store"}


class NativeStatefulPilotError(ValueError):
    pass


def _score(iteration: int) -> float:
    return round(0.62 + min(iteration, 10) * 0.031, 3)


def _tokens(mode: str, iteration: int) -> tuple[int, int, int, int, int]:
    if mode == "stateless_reread":
        context = 420 + iteration * 210
        repeated = iteration * 170
        refs = 0
    elif mode == "stateful_store":
        context = 390 + min(iteration, 2) * 45
        repeated = min(iteration, 2) * 18
        refs = 120
    else:
        raise NativeStatefulPilotError(f"unsupported mode: {mode}")
    return context + 140, 72, context, repeated, refs


def _step(mode: str, run_id: str, iteration: int, status: str) -> dict[str, Any]:
    input_tokens, output_tokens, context, repeated, refs = _tokens(mode, iteration)
    return {
        "adapter_step_id": f"{run_id}-iter-{iteration:02d}",
        "adapter_run_id": run_id,
        "step_index": iteration,
        "node_name": f"experiment_iteration_{iteration:02d}",
        "agent_role": "executor",
        "operation_kind": "model_call",
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "context_tokens": context,
        "repeated_context_tokens": repeated,
        "retrieved_refs_count": 1 if refs else 0,
        "retrieved_ref_tokens": refs,
        "tool_name": None,
        "tool_call_id": None,
        "status": status,
        "error_kind": "none" if status == "pass" else "pilot_failure",
        "state_read_bytes": 2048 if mode == "stateful_store" else 0,
        "state_write_bytes": 1024 if mode == "stateful_store" else 0,
    }


def build_bounded_summary(mode: str, iterations: int = 10, status: str = "pass") -> dict[str, Any]:
    if mode not in MODES:
        raise NativeStatefulPilotError("mode must be stateless_reread or stateful_store")
    if not 2 <= iterations <= 50:
        raise NativeStatefulPilotError("iterations must be between 2 and 50")
    if status not in {"pass", "fail"}:
        raise NativeStatefulPilotError("status must be pass or fail")
    run_id = f"native-pilot-{mode}"
    steps = [_step(mode, run_id, i, status) for i in range(iterations)]
    input_total = sum(s["input_tokens"] for s in steps)
    output_total = sum(s["output_tokens"] for s in steps)
    context_total = sum(s["context_tokens"] for s in steps)
    repeated_total = sum(s["repeated_context_tokens"] for s in steps)
    refs_total = sum(s["retrieved_ref_tokens"] for s in steps)
    quality_method = "test" if status == "pass" else "none"
    final_score = max(_score(i) for i in range(iterations))
    summary: dict[str, Any] = {
        "adapter_run_id": run_id,
        "runtime_kind": "native_harness",
        "runtime_version": RUNTIME_VERSION,
        "scenario_id": SCENARIO_ID,
        "mode": mode,
        "state_strategy": "durable_state" if mode == "stateful_store" else "full_history",
        "status": status,
        "pass_fail_reason": "same deterministic success criterion met" if status == "pass" else "pilot forced non-passing run",
        "quality_method": quality_method,
        "comparison_contract": {
            "scenario_digest": hashlib.sha256(SCENARIO_ID.encode("utf-8")).hexdigest(),
            "task_digest": hashlib.sha256(
                f"{SCENARIO_ID}:iterations={iterations}:status={status}".encode("utf-8")
            ).hexdigest(),
            "runtime_kind": "native_harness",
            "runtime_version": RUNTIME_VERSION,
            "provider_id": "native-deterministic-pilot",
            "model_id": "deterministic-score-function.v1",
            "tokenizer_id": "deterministic-counter.v1",
            "pricing_id": "native-pilot-fixed-pricing.v1",
            "input_cost_per_1k_usd": 0.0015,
            "output_cost_per_1k_usd": 0.006,
            "quality_method": quality_method,
            "quality_threshold": final_score if status == "pass" else 1.0,
            "evaluator_version": "native-hidden-score.v1",
            "redaction_policy": "not-needed-generated-summary.v1",
            "retry_policy": "no-retry.v1",
            "seed": 0,
        },
        "input_token_total": input_total,
        "output_token_total": output_total,
        "context_token_total": context_total,
        "repeated_context_token_total": repeated_total,
        "retrieved_ref_token_total": refs_total,
        "tool_call_count": iterations,
        "redundant_tool_call_count": 0,
        "retry_count": 0,
        "step_count": iterations,
        "duration_ms": 750 * iterations if mode == "stateful_store" else 900 * iterations,
        "estimated_cost_usd": round((input_total * 0.0000015) + (output_total * 0.000006), 6),
        SOURCE_REF_KEY: f"bounded-native-pilot-source-{mode}",
        "redaction_status": "not_needed",
        "pilot_metadata": {
            "deterministic": True,
            "external_calls": 0,
            "target_reads": 0,
            "final_score": final_score,
            "context_protocol": "full_history_reread" if mode == "stateless_reread" else "compact_summary_plus_recent_window",
        },
        "steps": steps,
    }
    if status == "pass":
        summary["quality_score"] = summary["pilot_metadata"]["final_score"]
    return summary


def build_scorecard(mode: str, iterations: int = 10, status: str = "pass") -> dict[str, Any]:
    try:
        return VALIDATOR.import_scorecard(build_bounded_summary(mode, iterations, status))
    except VALIDATOR.ScorecardError as exc:
        raise NativeStatefulPilotError(str(exc)) from exc


def build_pair(iterations: int = 10, status: str = "pass") -> tuple[dict[str, Any], dict[str, Any]]:
    return build_scorecard("stateless_reread", iterations, status), build_scorecard("stateful_store", iterations, status)


def build_comparison(iterations: int = 10, status: str = "pass") -> dict[str, Any]:
    stateless, stateful = build_pair(iterations, status)
    result = COMPARISON.compare_scorecards([stateless, stateful])
    result["pilot_kind"] = "native_stateful_vs_stateless_deterministic"
    result["claim_scope"] = "token-shape pilot only; not a full autonomous experimentation runner"
    return result


def _render(value: Any, compact: bool) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":") if compact else None, indent=None if compact else 2)


def write_output_dir(output_dir: Path, iterations: int, status: str, compact: bool) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    stateless, stateful = build_pair(iterations, status)
    comparison = build_comparison(iterations, status)
    (output_dir / "stateless_reread.scorecard.json").write_text(_render(stateless, compact) + "\n", encoding="utf-8")
    (output_dir / "stateful_store.scorecard.json").write_text(_render(stateful, compact) + "\n", encoding="utf-8")
    (output_dir / "comparison.json").write_text(_render(comparison, compact) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Run deterministic native stateful-vs-stateless pilot.")
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--compare", action="store_true")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--iterations", type=int, default=10)
    parser.add_argument("--status", choices=["pass", "fail"], default="pass")
    parser.add_argument("--compact", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.compare:
            rendered = _render(build_comparison(args.iterations, args.status), args.compact)
            if args.output:
                args.output.write_text(rendered + "\n", encoding="utf-8")
            else:
                print(rendered)
            return 0
        if args.output_dir is None:
            raise NativeStatefulPilotError("--output-dir is required unless --compare is used")
        write_output_dir(args.output_dir, args.iterations, args.status, args.compact)
        return 0
    except (NativeStatefulPilotError, COMPARISON.ScorecardComparisonError) as exc:
        print(f"native stateful pilot failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
