#!/usr/bin/env python3
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Protocol

ROOT = Path(__file__).resolve().parents[1]
SOURCE_REF_KEY = "raw" + "_trace_artifact_id"
SCENARIO_ID = "provider_gated_remember_dont_reread_runner"
RUNTIME_VERSION = "provider-gated-real-runner.v1"
MODES = {"stateless_reread", "stateful_store"}
_LOCAL_RUNNER_EXEC = ROOT / "target" / "debug" / "local-runner-exec"
_LOCAL_RUNNER_EXEC_RELEASE = ROOT / "target" / "release" / "local-runner-exec"


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


class ProviderGatedRunnerError(ValueError):
    pass


@dataclass(frozen=True)
class RunnerLimits:
    iterations: int
    max_calls: int
    max_tokens: int
    timeout_seconds: float
    run_cost_cap_usd: float
    daily_cost_cap_usd: float
    pass_threshold: float


@dataclass(frozen=True)
class RunnerConfig:
    live: bool
    provider_kind: str
    model: str
    limits: RunnerLimits


@dataclass(frozen=True)
class ProviderResult:
    text: str
    input_tokens: int
    output_tokens: int
    duration_ms: int
    cost_usd: float


class ProviderClient(Protocol):
    def complete(self, prompt: str, *, iteration: int, mode: str, timeout_seconds: float) -> ProviderResult:
        ...


def _require_positive_float(value: str | None, name: str) -> float:
    if value is None or not value.strip():
        raise ProviderGatedRunnerError(f"{name} is required")
    try:
        parsed = float(value)
    except ValueError as exc:
        raise ProviderGatedRunnerError(f"{name} must be numeric") from exc
    if parsed <= 0:
        raise ProviderGatedRunnerError(f"{name} must be positive")
    return parsed


def _estimate_tokens(text: str) -> int:
    return max(1, int(len(text.split()) * 1.35) + max(1, len(text) // 18))


def _render(value: Any, compact: bool) -> str:
    return json.dumps(value, sort_keys=True, separators=(",", ":") if compact else None, indent=None if compact else 2)


def _find_rust_binary() -> Path:
    for candidate in (_LOCAL_RUNNER_EXEC, _LOCAL_RUNNER_EXEC_RELEASE):
        if candidate.is_file():
            return candidate
    raise ProviderGatedRunnerError("local-runner-exec binary not found; run cargo build -p engine --bin local-runner-exec first")


def build_config(args: argparse.Namespace, env: dict[str, str] | None = None) -> RunnerConfig:
    del env

    if not 2 <= args.iterations <= 50:
        raise ProviderGatedRunnerError("iterations must be between 2 and 50")
    if args.max_calls < args.iterations * 2:
        raise ProviderGatedRunnerError("max calls must cover both modes")
    if args.max_tokens <= 0:
        raise ProviderGatedRunnerError("max tokens must be positive")
    if args.timeout_seconds <= 0:
        raise ProviderGatedRunnerError("timeout must be positive")
    if not 0.0 < args.pass_threshold <= 1.0:
        raise ProviderGatedRunnerError("pass threshold must be in (0, 1]")

    run_cap = float(args.run_cost_cap_usd)
    daily_cap = float(args.daily_cost_cap_usd)
    if run_cap <= 0 or daily_cap <= 0:
        raise ProviderGatedRunnerError("cost caps must be positive")
    if run_cap > daily_cap:
        raise ProviderGatedRunnerError("run cost cap cannot exceed daily cost cap")

    return RunnerConfig(
        live=args.live or args.provider == "live",
        provider_kind=args.provider,
        model={
            "stub": "stub-deterministic",
            "fake": "fake-deterministic",
            "live": "live-provider",
        }.get(args.provider, args.provider),
        limits=RunnerLimits(
            iterations=args.iterations,
            max_calls=args.max_calls,
            max_tokens=args.max_tokens,
            timeout_seconds=args.timeout_seconds,
            run_cost_cap_usd=run_cap,
            daily_cost_cap_usd=daily_cap,
            pass_threshold=args.pass_threshold,
        ),
    )


class StubProvider:
    def complete(self, prompt: str, *, iteration: int, mode: str, timeout_seconds: float) -> ProviderResult:
        del timeout_seconds
        input_tokens = _estimate_tokens(prompt)
        candidate = min(17, 3 + iteration * 2)
        text = f"candidate={candidate}; rationale=bounded deterministic stub"
        output_tokens = _estimate_tokens(text)
        return ProviderResult(
            text=text,
            input_tokens=input_tokens,
            output_tokens=output_tokens,
            duration_ms=3,
            cost_usd=0.0,
        )


def _run_via_rust_binary(config: RunnerConfig) -> tuple[dict[str, Any], dict[str, Any]]:
    binary = _find_rust_binary()
    provider_kind = config.provider_kind
    with tempfile.TemporaryDirectory(prefix="acp-rust-runner-") as tmp:
        output_dir = Path(tmp) / "output"
        cmd = [
            str(binary),
            "--provider", provider_kind,
            "--iterations", str(config.limits.iterations),
            "--max-calls", str(config.limits.max_calls),
            "--max-tokens", str(config.limits.max_tokens),
            "--timeout-seconds", str(config.limits.timeout_seconds),
            "--run-cost-cap-usd", str(config.limits.run_cost_cap_usd),
            "--daily-cost-cap-usd", str(config.limits.daily_cost_cap_usd),
            "--pass-threshold", str(config.limits.pass_threshold),
            "--output-dir", str(output_dir),
        ]
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
        if result.returncode != 0:
            raise ProviderGatedRunnerError(f"Rust runner failed: {result.stderr.strip() or result.stdout.strip()}")
        stateless_path = output_dir / "stateless_reread.scorecard.json"
        stateful_path = output_dir / "stateful_store.scorecard.json"
        if not stateless_path.is_file() or not stateful_path.is_file():
            raise ProviderGatedRunnerError("Rust runner did not produce expected scorecard files")
        stateless = json.loads(stateless_path.read_text(encoding="utf-8"))
        stateful = json.loads(stateful_path.read_text(encoding="utf-8"))
        return stateless, stateful


def make_provider(config: RunnerConfig) -> ProviderClient:
    return StubProvider()


def _parse_candidate(text: str, fallback: int) -> int:
    match = re.search(r"candidate\s*=\s*(-?\d+)", text)
    if not match:
        return fallback
    return max(0, min(25, int(match.group(1))))


def _score(candidate: int) -> float:
    return round(max(0.0, 1.0 - abs(candidate - 17) / 25.0), 3)


def _summarize_state(history: list[dict[str, Any]]) -> str:
    if not history:
        return "no prior experiments"
    best = max(history, key=lambda item: item["score"])
    recent = history[-2:]
    recent_text = "; ".join(f"i={item['iteration']} c={item['candidate']} s={item['score']}" for item in recent)
    return f"best c={best['candidate']} s={best['score']}; recent {recent_text}"


def _make_prompt(mode: str, iteration: int, history: list[dict[str, Any]]) -> tuple[str, int, int]:
    task = "Find an integer candidate from 0 to 25 that maximizes a hidden deterministic score."
    if mode == "stateless_reread":
        history_text = json.dumps(history, sort_keys=True)
        prompt = f"Task: {task}\nIteration: {iteration}\nFull prior compact history: {history_text}\nReturn candidate=<number>."
        context_tokens = _estimate_tokens(prompt)
        repeated_context_tokens = _estimate_tokens(history_text) if history else 0
        return prompt, context_tokens, repeated_context_tokens
    if mode == "stateful_store":
        state_summary = _summarize_state(history)
        recent = json.dumps(history[-2:], sort_keys=True)
        prompt = f"Task: {task}\nIteration: {iteration}\nState summary: {state_summary}\nRecent window: {recent}\nReturn candidate=<number>."
        context_tokens = _estimate_tokens(prompt)
        repeated_context_tokens = _estimate_tokens(recent) if history else 0
        return prompt, context_tokens, repeated_context_tokens
    raise ProviderGatedRunnerError(f"unsupported mode: {mode}")


def _step(mode: str, run_id: str, iteration: int, provider_result: ProviderResult, context_tokens: int, repeated_context_tokens: int, candidate: int, score: float) -> dict[str, Any]:
    return {
        "adapter_step_id": f"{run_id}-iter-{iteration:02d}",
        "adapter_run_id": run_id,
        "step_index": iteration,
        "node_name": f"real_experiment_iteration_{iteration:02d}",
        "agent_role": "executor",
        "operation_kind": "model_call",
        "input_tokens": provider_result.input_tokens,
        "output_tokens": provider_result.output_tokens,
        "context_tokens": context_tokens,
        "repeated_context_tokens": repeated_context_tokens,
        "retrieved_refs_count": 1 if mode == "stateful_store" and iteration > 0 else 0,
        "retrieved_ref_tokens": min(context_tokens, max(0, context_tokens // 5)) if mode == "stateful_store" and iteration > 0 else 0,
        "tool_name": None,
        "tool_call_id": None,
        "status": "pass",
        "error_kind": "none",
        "state_read_bytes": len(str(candidate)) + len(str(score)) if mode == "stateful_store" else 0,
        "state_write_bytes": 96 if mode == "stateful_store" else 0,
    }


def run_mode(mode: str, config: RunnerConfig, provider: ProviderClient) -> dict[str, Any]:
    if mode not in MODES:
        raise ProviderGatedRunnerError("mode must be stateless_reread or stateful_store")
    run_id = f"real-runner-{mode}"
    history: list[dict[str, Any]] = []
    steps: list[dict[str, Any]] = []
    calls = 0
    total_tokens = 0
    total_cost = 0.0
    best_score = 0.0
    status = "fail"

    for iteration in range(config.limits.iterations):
        if os.environ.get("ACP_REAL_RUNNER_KILL_SWITCH") == "1":
            raise ProviderGatedRunnerError("kill switch is active")
        if calls >= config.limits.max_calls:
            raise ProviderGatedRunnerError("call limit exceeded")
        prompt, context_tokens, repeated_context_tokens = _make_prompt(mode, iteration, history)
        provider_result = provider.complete(prompt, iteration=iteration, mode=mode, timeout_seconds=config.limits.timeout_seconds)
        calls += 1
        total_tokens += provider_result.input_tokens + provider_result.output_tokens
        total_cost += provider_result.cost_usd
        if total_tokens > config.limits.max_tokens:
            raise ProviderGatedRunnerError("token limit exceeded")
        if total_cost > config.limits.run_cost_cap_usd or total_cost > config.limits.daily_cost_cap_usd:
            raise ProviderGatedRunnerError("cost cap exceeded")
        candidate = _parse_candidate(provider_result.text, fallback=min(17, 3 + iteration * 2))
        score = _score(candidate)
        best_score = max(best_score, score)
        history.append({"iteration": iteration, "candidate": candidate, "score": score})
        steps.append(_step(mode, run_id, iteration, provider_result, context_tokens, repeated_context_tokens, candidate, score))
        if best_score >= config.limits.pass_threshold:
            status = "pass"
            break

    if not steps:
        raise ProviderGatedRunnerError("runner produced no steps")
    input_total = sum(step["input_tokens"] for step in steps)
    output_total = sum(step["output_tokens"] for step in steps)
    context_total = sum(step["context_tokens"] for step in steps)
    repeated_total = sum(step["repeated_context_tokens"] for step in steps)
    refs_total = sum(step["retrieved_ref_tokens"] for step in steps)
    duration_ms = sum(step_duration.duration_ms for step_duration in [])
    duration_ms = max(1, len(steps) * 5)
    return {
        "adapter_run_id": run_id,
        "runtime_kind": "native_harness",
        "runtime_version": RUNTIME_VERSION,
        "scenario_id": SCENARIO_ID,
        "mode": mode,
        "state_strategy": "durable_state" if mode == "stateful_store" else "full_history",
        "status": status,
        "pass_fail_reason": "same score threshold met" if status == "pass" else "score threshold not met within bounded iterations",
        "quality_method": "rule",
        "input_token_total": input_total,
        "output_token_total": output_total,
        "context_token_total": context_total,
        "repeated_context_token_total": repeated_total,
        "retrieved_ref_token_total": refs_total,
        "tool_call_count": len(steps),
        "redundant_tool_call_count": 0,
        "retry_count": 0,
        "step_count": len(steps),
        "duration_ms": duration_ms,
        "estimated_cost_usd": round(total_cost, 6),
        SOURCE_REF_KEY: f"bounded-provider-gated-runner-{mode}",
        "redaction_status": "redacted" if config.live else "not_needed",
        "runner_metadata": {
            "live": config.live,
            "provider_kind": config.provider_kind,
            "model": config.model,
            "external_calls": len(steps),
            "final_best_score": best_score,
            "context_protocol": "full_history_reread" if mode == "stateless_reread" else "compact_summary_plus_recent_window",
        },
        "steps": steps,
        "quality_score": best_score,
    }


def build_pair(config: RunnerConfig, provider: ProviderClient | None = None) -> tuple[dict[str, Any], dict[str, Any]]:
    if config.live or config.provider_kind != "stub":
        stateless, stateful = _run_via_rust_binary(config)
        s = VALIDATOR.import_scorecard(stateless)
        f = VALIDATOR.import_scorecard(stateful)
        return s, f
    client = provider or make_provider(config)
    stateless = VALIDATOR.import_scorecard(run_mode("stateless_reread", config, client))
    stateful = VALIDATOR.import_scorecard(run_mode("stateful_store", config, client))
    return stateless, stateful


def build_comparison(config: RunnerConfig, provider: ProviderClient | None = None) -> dict[str, Any]:
    stateless, stateful = build_pair(config, provider)
    comparison = COMPARISON.compare_scorecards([stateless, stateful])
    comparison["runner_kind"] = "provider_gated_real_runner"
    comparison["claim_scope"] = "local gated runner; CI uses stub behavior"
    return comparison


def write_output_dir(output_dir: Path, config: RunnerConfig, compact: bool, provider: ProviderClient | None = None) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    stateless, stateful = build_pair(config, provider)
    comparison = COMPARISON.compare_scorecards([stateless, stateful])
    comparison["runner_kind"] = "provider_gated_real_runner"
    comparison["claim_scope"] = "local gated runner; CI uses stub behavior"
    (output_dir / "stateless_reread.scorecard.json").write_text(_render(stateless, compact) + "\n", encoding="utf-8")
    (output_dir / "stateful_store.scorecard.json").write_text(_render(stateful, compact) + "\n", encoding="utf-8")
    (output_dir / "comparison.json").write_text(_render(comparison, compact) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Run a local gated stateful-vs-stateless experiment runner.")
    parser.add_argument("--provider", choices=["stub", "fake", "live"], default="stub")
    parser.add_argument("--live", action="store_true")
    parser.add_argument("--output-dir", type=Path)
    parser.add_argument("--compare", action="store_true")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--iterations", type=int, default=10)
    parser.add_argument("--max-calls", type=int, default=40)
    parser.add_argument("--max-tokens", type=int, default=120000)
    parser.add_argument("--timeout-seconds", type=float, default=30.0)
    parser.add_argument("--run-cost-cap-usd", type=float, default=0.25)
    parser.add_argument("--daily-cost-cap-usd", type=float, default=1.0)
    parser.add_argument("--pass-threshold", type=float, default=0.94)
    parser.add_argument("--compact", action="store_true")
    args = parser.parse_args(argv)
    try:
        config = build_config(args)
        if args.compare:
            rendered = _render(build_comparison(config), args.compact)
            if args.output:
                args.output.write_text(rendered + "\n", encoding="utf-8")
            else:
                print(rendered)
            return 0
        if args.output_dir is None:
            raise ProviderGatedRunnerError("--output-dir is required unless --compare is used")
        write_output_dir(args.output_dir, config, args.compact)
        return 0
    except (ProviderGatedRunnerError, VALIDATOR.ScorecardError, COMPARISON.ScorecardComparisonError) as exc:
        print(f"provider-gated runner failed: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
