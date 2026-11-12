#!/usr/bin/env python3
"""Operator self-test matrix for the usable local harness.

Provider-free by default. Live model performance mode is opt-in and fail-closed:
it requires an explicit confirmation string, a symbolic credential env that is
already set, and the existing efficiency live gates. This script orchestrates
existing owners; it is not a second runtime, scheduler, store, or evaluator.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

LIVE_CONFIRMATION = "I_CONFIRM_BOUNDED_LIVE_PROVIDER_COSTS"
ROOT = Path(__file__).resolve().parents[1]


class SelfTestError(RuntimeError):
    pass


def _run(
    cmd: list[str],
    *,
    cwd: Path | None = None,
    env: dict[str, str] | None = None,
    timeout: int = 900,
) -> subprocess.CompletedProcess[str]:
    print(f"[self-test] $ {' '.join(cmd)}", flush=True)
    try:
        return subprocess.run(
            cmd,
            cwd=str(cwd or ROOT),
            env=env,
            text=True,
            capture_output=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as exc:
        raise SelfTestError(f"timed out after {timeout}s: {' '.join(cmd)}") from exc


def _require_ok(proc: subprocess.CompletedProcess[str], label: str) -> None:
    if proc.returncode == 0:
        if proc.stdout.strip():
            print(proc.stdout.rstrip(), flush=True)
        return
    detail = (proc.stderr or proc.stdout or "").strip()
    raise SelfTestError(f"{label} failed (exit {proc.returncode})\n{detail}")


def ensure_binaries() -> dict[str, Path]:
    engine = ROOT / "target" / "debug" / "agent-control-plane"
    native = ROOT / "target" / "debug" / "efficiency_native_runtime"
    langgraph = ROOT / "target" / "debug" / "efficiency_langgraph_runtime"
    missing = [p.name for p in (engine, native, langgraph) if not p.is_file()]
    if missing:
        print("[self-test] building engine binaries…", flush=True)
        _require_ok(
            _run(["cargo", "build", "-p", "engine"], timeout=1800),
            "cargo build -p engine",
        )
    for path in (engine, native, langgraph):
        if not path.is_file():
            raise SelfTestError(f"missing required binary: {path}")
    return {"engine": engine, "native": native, "langgraph": langgraph}


def step_pure_unit() -> None:
    for script in (
        ["uv", "run", "--no-project", "python", "scripts/start_operator_desktop.py", "--self-test"],
        ["uv", "run", "--no-project", "python", "scripts/start_local_work.py", "--self-test"],
        ["uv", "run", "--no-project", "python", "scripts/demo_no_provider.py", "--self-test"],
    ):
        _require_ok(_run(script, timeout=120), " ".join(script))


def step_doctor() -> None:
    _require_ok(
        _run(["uv", "run", "--no-project", "python", "scripts/acp_local_doctor.py"], timeout=180),
        "acp_local_doctor",
    )


def step_local_work() -> None:
    # Full product golden path: start engine, apply one local-folder change, cleanup.
    _require_ok(
        _run(["./scripts/start_local_work.sh"], timeout=900),
        "start_local_work",
    )


def step_efficiency_fixture(bins: dict[str, Path], output_root: Path) -> Path:
    output_root.mkdir(parents=True, exist_ok=True)
    run_id = f"self-test-fixture-{int(time.time())}"
    proc = _run(
        [
            "uv",
            "run",
            "--no-project",
            "python",
            "scripts/efficiency_live_benchmark.py",
            "--mode",
            "fixture",
            "--native-cli",
            str(bins["native"]),
            "--langgraph-adapter",
            str(bins["langgraph"]),
            "--output-root",
            str(output_root),
            "--benchmark-run-id",
            run_id,
        ],
        timeout=900,
    )
    _require_ok(proc, "efficiency fixture benchmark")
    report = _find_benchmark_report(output_root)
    data = json.loads(report.read_text(encoding="utf-8"))
    status = data.get("acceptance_status")
    if status == "FAIL":
        raise SelfTestError(f"fixture benchmark FAIL: {report}")
    if status == "INCOMPARABLE":
        raise SelfTestError(f"fixture benchmark INCOMPARABLE: {report}")
    if status != "PASS":
        raise SelfTestError(f"fixture benchmark unexpected status={status!r}: {report}")
    print(f"[self-test] fixture report: {report} status={status!r}", flush=True)
    return report


def _find_benchmark_report(output_root: Path) -> Path:
    candidates = sorted(output_root.glob("efficiency-benchmark-*.report.json"))
    if not candidates:
        candidates = sorted(output_root.rglob("efficiency-benchmark-*.report.json"))
    if not candidates:
        raise SelfTestError(f"benchmark wrote no report under {output_root}")
    return candidates[-1]


def step_efficiency_live(bins: dict[str, Path], output_root: Path) -> Path:
    if os.environ.get("CI", "").strip() in {"1", "true", "TRUE", "yes"}:
        raise SelfTestError("live self-test refuses CI")

    confirm = os.environ.get("ACP_SELF_TEST_LIVE_CONFIRM", "").strip()
    if confirm != LIVE_CONFIRMATION:
        raise SelfTestError(
            "live mode requires ACP_SELF_TEST_LIVE_CONFIRM="
            f"{LIVE_CONFIRMATION}"
        )

    credential_env = os.environ.get("ACP_SELF_TEST_CREDENTIAL_ENV", "OPENAI_API_KEY").strip()
    if not credential_env or not os.environ.get(credential_env):
        raise SelfTestError(
            f"live mode requires credential env {credential_env!r} to be set "
            "(symbolic name only; value never logged)"
        )

    base_url = os.environ.get("ACP_PROVIDER_BASE_URL", "https://api.openai.com/v1").strip()
    model = os.environ.get("ACP_SELF_TEST_MODEL", "gpt-4.1-mini").strip()
    tokenizer = os.environ.get("ACP_SELF_TEST_TOKENIZER", "cl100k_base").strip()
    kill_switch_env = "ACP_EFFICIENCY_KILL_SWITCH"
    os.environ.setdefault(kill_switch_env, "0")

    pricing_id = os.environ.get("ACP_SELF_TEST_PRICING_ID", "openai-gpt-4.1-mini-self-test").strip()
    pricing_date = os.environ.get("ACP_SELF_TEST_PRICING_DATE", "2026-01-01").strip()
    input_cost = os.environ.get("ACP_SELF_TEST_INPUT_COST_PER_1K_USD", "0.0004").strip()
    output_cost = os.environ.get("ACP_SELF_TEST_OUTPUT_COST_PER_1K_USD", "0.0016").strip()
    per_call = os.environ.get("ACP_SELF_TEST_PER_CALL_COST_CAP_USD", "0.05").strip()
    run_cap = os.environ.get("ACP_SELF_TEST_RUN_COST_CAP_USD", "0.50").strip()
    daily_cap = os.environ.get("ACP_SELF_TEST_DAILY_COST_CAP_USD", "1.00").strip()

    audit_parent = output_root / "audit-store"
    audit_parent.mkdir(parents=True, exist_ok=True)
    run_id = f"self-test-live-{int(time.time())}"

    env = os.environ.copy()
    env["ACP_ENABLE_PROVIDER_EXECUTION"] = "1"

    proc = _run(
        [
            "uv",
            "run",
            "--no-project",
            "python",
            "scripts/efficiency_live_benchmark.py",
            "--mode",
            "live",
            "--live-confirmation",
            LIVE_CONFIRMATION,
            "--native-cli",
            str(bins["native"]),
            "--langgraph-adapter",
            str(bins["langgraph"]),
            "--output-root",
            str(output_root),
            "--audit-store",
            str(audit_parent),
            "--benchmark-run-id",
            run_id,
            "--provider",
            "openai_compatible",
            "--provider-base-url",
            base_url,
            "--model",
            model,
            "--tokenizer",
            tokenizer,
            "--credential-env",
            credential_env,
            "--kill-switch-env",
            kill_switch_env,
            "--pricing-id",
            pricing_id,
            "--pricing-effective-date",
            pricing_date,
            "--input-cost-per-1k-usd",
            input_cost,
            "--output-cost-per-1k-usd",
            output_cost,
            "--per-call-cost-cap-usd",
            per_call,
            "--run-cost-cap-usd",
            run_cap,
            "--daily-cost-cap-usd",
            daily_cap,
            "--timeout-seconds",
            os.environ.get("ACP_SELF_TEST_TIMEOUT_SECONDS", "120"),
            "--max-calls",
            os.environ.get("ACP_SELF_TEST_MAX_CALLS", "8"),
        ],
        env=env,
        timeout=1800,
    )
    _require_ok(proc, "efficiency live benchmark")
    report = _find_benchmark_report(output_root)
    data = json.loads(report.read_text(encoding="utf-8"))
    status = data.get("acceptance_status")
    if status == "FAIL":
        raise SelfTestError(f"live benchmark FAIL: {report}")
    if status == "INCOMPARABLE":
        raise SelfTestError(f"live benchmark INCOMPARABLE: {report}")
    if status != "PASS":
        raise SelfTestError(f"live benchmark unexpected status={status!r}: {report}")
    print(f"[self-test] live report: {report} status={status!r}", flush=True)
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mode",
        choices=("fixture", "live", "auto"),
        default="auto",
        help="fixture=provider-free; live=requires gates; auto=live only when gates present",
    )
    parser.add_argument(
        "--skip-work",
        action="store_true",
        help="skip the local-folder product golden path (faster smoke)",
    )
    parser.add_argument(
        "--output-root",
        default="",
        help="directory for benchmark artifacts (default: temp under /tmp)",
    )
    args = parser.parse_args()

    output_root = Path(args.output_root) if args.output_root else Path(
        tempfile.mkdtemp(prefix="acp-harness-self-test-", dir="/tmp")
    )
    print(f"[self-test] output_root={output_root}", flush=True)

    started = time.time()
    results: dict[str, str] = {}

    try:
        bins = ensure_binaries()
        step_pure_unit()
        results["pure_unit"] = "PASS"
        step_doctor()
        results["doctor"] = "PASS"
        if not args.skip_work:
            step_local_work()
            results["local_work"] = "PASS"
        else:
            results["local_work"] = "SKIP"

        fixture_report = step_efficiency_fixture(bins, output_root / "fixture")
        results["efficiency_fixture"] = f"PASS:{fixture_report}"

        want_live = args.mode == "live"
        if args.mode == "auto":
            want_live = (
                os.environ.get("ACP_SELF_TEST_LIVE_CONFIRM", "").strip() == LIVE_CONFIRMATION
                and bool(os.environ.get(os.environ.get("ACP_SELF_TEST_CREDENTIAL_ENV", "OPENAI_API_KEY"), ""))
            )

        if want_live:
            live_report = step_efficiency_live(bins, output_root / "live")
            results["efficiency_live"] = f"PASS:{live_report}"
        else:
            results["efficiency_live"] = "SKIP:live_gates_absent"
            if args.mode == "live":
                raise SelfTestError("live mode requested but gates incomplete")

        elapsed = time.time() - started
        summary = {
            "schema_version": "harness_self_test_result.v1",
            "mode": args.mode,
            "elapsed_seconds": round(elapsed, 3),
            "results": results,
            "output_root": str(output_root),
            "acceptance_status": "PASS",
        }
        summary_path = output_root / "self_test_summary.json"
        summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        print(json.dumps(summary, indent=2, sort_keys=True), flush=True)
        print(f"[self-test] PASS in {elapsed:.1f}s → {summary_path}", flush=True)
        return 0
    except SelfTestError as exc:
        elapsed = time.time() - started
        summary = {
            "schema_version": "harness_self_test_result.v1",
            "mode": args.mode,
            "elapsed_seconds": round(elapsed, 3),
            "results": results,
            "output_root": str(output_root),
            "acceptance_status": "FAIL",
            "error": str(exc),
        }
        try:
            (output_root / "self_test_summary.json").write_text(
                json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
            )
        except OSError:
            pass
        print(str(exc), file=sys.stderr)
        print(f"[self-test] FAIL after {elapsed:.1f}s", file=sys.stderr)
        return 1
    finally:
        # Best-effort cleanup of leftover local sessions.
        for cleanup in (
            ["uv", "run", "--no-project", "python", "scripts/start_operator_desktop.py", "--cleanup"],
            ["uv", "run", "--no-project", "python", "scripts/start_local_work.py", "--cleanup"],
            ["uv", "run", "--no-project", "python", "scripts/demo_no_provider.py", "--cleanup"],
        ):
            subprocess.run(cleanup, cwd=str(ROOT), capture_output=True, text=True, check=False)


if __name__ == "__main__":
    sys.exit(main())
