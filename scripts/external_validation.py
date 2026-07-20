#!/usr/bin/env python3
"""Clean-environment validation for strangers evaluating this repository.

Reuses the no-provider demo and local doctor owners. Produces a bounded
external_validation_report.v1 without provider calls, API keys, target-repo
writes, or persistent background processes.

This validates engineering usability only. It does not claim external adoption.
"""

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

REPORT_KIND = "external_validation_report.v1"
DEMO_SCRIPT = "scripts/demo_no_provider.py"
DOCTOR_SCRIPT = "scripts/acp_local_doctor.py"
EXACT_HEAD_LOCAL = "actions/exact-head-check/test_verify_local.sh"
EXACT_HEAD_VERIFY = "actions/exact-head-check/verify.sh"
ENGINE_CARGO = "engine/Cargo.toml"


def repo_root_from_script() -> Path:
    return Path(__file__).resolve().parents[1]


def _sanitize_text(text: str) -> str:
    """Drop absolute home/runner paths from bounded report/reason strings."""
    cleaned = text
    cleaned = re.sub(r"/home/[^/\s]+", "~", cleaned)
    cleaned = re.sub(r"/Users/[^/\s]+", "~", cleaned)
    cleaned = re.sub(r"/runner/work/[^\s:]+", "<runner>", cleaned)
    return cleaned


def _failure_core(text: str, limit: int = 280) -> str:
    """Prefer the last meaningful line so truncated reasons keep the root error."""
    lines = [ln.strip() for ln in _sanitize_text(text).splitlines() if ln.strip()]
    if not lines:
        return "unknown_failure"
    core = lines[-1]
    if len(core) > limit:
        core = core[-limit:]
    return core


def git_head_sha(repo_root: Path) -> str:
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repo_root,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise RuntimeError(f"git rev-parse HEAD failed: {result.stderr.strip()}")
    sha = result.stdout.strip()
    if len(sha) != 40 or any(ch not in "0123456789abcdef" for ch in sha):
        raise RuntimeError(f"unexpected HEAD sha: {sha!r}")
    return sha


def run_capture(
    cmd: list[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
    timeout: float | None = None,
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=cwd,
        env=env,
        capture_output=True,
        text=True,
        check=False,
        timeout=timeout,
    )


def stage(
    stages: list[dict[str, Any]],
    name: str,
    status: str,
    reason: str,
    *,
    detail: dict[str, Any] | None = None,
) -> None:
    entry: dict[str, Any] = {
        "name": name,
        "status": status,
        "reason": reason,
    }
    if detail:
        # Keep only bounded non-sensitive keys.
        safe = {
            k: v
            for k, v in detail.items()
            if k
            in {
                "tool",
                "version",
                "package_name",
                "binary_name",
                "path_kind",
                "proof_kind",
                "source_revision_prefix",
                "dispatch_id_present",
                "executor_type",
                "stale_head_rejected",
                "exit_code",
                "mock_case",
            }
        }
        if safe:
            entry["detail"] = safe
    stages.append(entry)
    mark = "PASS" if status == "pass" else ("SKIP" if status == "skip" else "FAIL")
    print(f"{mark}  {name}: {reason}")


def detect_tools(
    repo_root: Path,
    stages: list[dict[str, Any]],
    *,
    skip_demo: bool,
) -> dict[str, str]:
    """Detect required tools; reuse doctor JSON when available.

    Full clean-env needs cargo/rustc/bun/uv. --skip-demo only needs git and jq
    (install-contract parse + exact-head offline self-validation).
    """
    versions: dict[str, str] = {
        "python": sys.version.split()[0],
        "os": platform.system(),
        "arch": platform.machine(),
        "platform": platform.platform(),
    }
    if skip_demo:
        required = {
            "git": ["--version"],
            "jq": ["--version"],
        }
    else:
        required = {
            "git": ["--version"],
            "cargo": ["--version"],
            "rustc": ["--version"],
            "bun": ["--version"],
            "uv": ["--version"],
            "jq": ["--version"],
        }
    missing: list[str] = []
    for name, args in required.items():
        path = shutil.which(name)
        if not path:
            missing.append(name)
            versions[name] = "missing"
            continue
        result = run_capture([path, *args], cwd=repo_root, timeout=10)
        line = (result.stdout or result.stderr or "").strip().splitlines()
        versions[name] = line[0] if line else "version unknown"

    # Doctor is advisory for public demo (auth/port warnings are expected).
    # Skip when not doing a live demo to keep unit CI free of bun/cargo.
    doctor = repo_root / DOCTOR_SCRIPT
    if doctor.exists() and not skip_demo:
        result = run_capture(
            [sys.executable, str(doctor), "--json"],
            cwd=repo_root,
            timeout=30,
        )
        if result.returncode not in (0, 1):
            stage(
                stages,
                "detect_tools",
                "fail",
                f"doctor exited {result.returncode}",
            )
            raise RuntimeError("doctor failed unexpectedly")
        try:
            payload = json.loads(result.stdout)
            doctor_statuses = {
                c.get("name"): c.get("status") for c in payload.get("checks", [])
            }
            versions["doctor_checks"] = ",".join(
                f"{k}={v}" for k, v in sorted(doctor_statuses.items()) if k
            )
        except json.JSONDecodeError:
            versions["doctor_checks"] = "unparsed"

    if missing:
        stage(
            stages,
            "detect_tools",
            "fail",
            f"missing required tools: {', '.join(missing)}",
            detail={"tool": missing[0]},
        )
        raise RuntimeError(f"missing tools: {missing}")

    tool_list = ", ".join(sorted(required.keys()))
    stage(
        stages,
        "detect_tools",
        "pass",
        f"{tool_list} present",
        detail={
            "tool": "git" if skip_demo else "cargo",
            "version": versions.get("git" if skip_demo else "cargo", "")[:80],
        },
    )
    return versions


def verify_install_contract(repo_root: Path, stages: list[dict[str, Any]]) -> None:
    """Verify documented Cargo package/binary relationship at this revision."""
    cargo_toml = (repo_root / ENGINE_CARGO).read_text(encoding="utf-8")
    package_match = re.search(r'(?m)^name\s*=\s*"([^"]+)"', cargo_toml)
    bin_match = re.search(
        r'(?ms)^\[\[bin\]\]\s*\nname\s*=\s*"([^"]+)"',
        cargo_toml,
    )
    if not package_match or package_match.group(1) != "engine":
        stage(stages, "install_contract", "fail", "engine package name mismatch")
        raise RuntimeError("Cargo package name must be engine")
    if not bin_match or bin_match.group(1) != "agent-control-plane":
        stage(stages, "install_contract", "fail", "primary binary name mismatch")
        raise RuntimeError("primary binary must be agent-control-plane")

    readme = (repo_root / "README.md").read_text(encoding="utf-8")
    if "engine --bin agent-control-plane" not in readme:
        stage(stages, "install_contract", "fail", "README missing cargo install form")
        raise RuntimeError("README must document engine --bin agent-control-plane")
    if re.search(r"igzela/agent-control-plane:latest", readme, re.I):
        stage(stages, "install_contract", "fail", "README advertises docker :latest")
        raise RuntimeError("forbidden docker :latest advertisement")

    # Manifest + public docs only; the disposable cargo build is a separate stage.
    stage(
        stages,
        "install_contract",
        "pass",
        "Cargo package engine / binary agent-control-plane matches README",
        detail={"package_name": "engine", "binary_name": "agent-control-plane"},
    )


def build_engine(
    repo_root: Path,
    target_dir: Path,
    stages: list[dict[str, Any]],
    *,
    timeout: float,
) -> Path:
    env = {
        **os.environ,
        "CARGO_TARGET_DIR": str(target_dir),
        "CARGO_TERM_COLOR": "never",
    }
    # Avoid reusing a possibly stale workspace target/ by forcing disposable dir.
    result = run_capture(
        [
            "cargo",
            "build",
            "--locked",
            "-p",
            "engine",
            "--bin",
            "agent-control-plane",
        ],
        cwd=repo_root,
        env=env,
        timeout=timeout,
    )
    if result.returncode != 0:
        stage(
            stages,
            "build_engine",
            "fail",
            "cargo build -p engine --bin agent-control-plane failed",
            detail={"exit_code": result.returncode},
        )
        raise RuntimeError(
            "cargo build failed:\n"
            + (result.stderr or result.stdout or "")[-4000:]
        )
    suffix = ".exe" if sys.platform == "win32" else ""
    binary = target_dir / "debug" / f"agent-control-plane{suffix}"
    if not binary.exists():
        stage(stages, "build_engine", "fail", f"binary missing at {binary.name}")
        raise RuntimeError(f"built binary missing: {binary}")
    stage(
        stages,
        "build_engine",
        "pass",
        "engine built in disposable CARGO_TARGET_DIR",
        detail={"path_kind": "disposable_target_dir"},
    )
    return binary


def build_dashboard(
    repo_root: Path,
    stages: list[dict[str, Any]],
    *,
    timeout: float,
) -> Path:
    dashboard = repo_root / "dashboard"
    install = run_capture(
        ["bun", "install", "--frozen-lockfile"],
        cwd=dashboard,
        timeout=timeout,
    )
    if install.returncode != 0:
        stage(stages, "build_dashboard", "fail", "bun install failed")
        raise RuntimeError(
            "bun install failed:\n" + (install.stderr or install.stdout or "")[-2000:]
        )
    build = run_capture(
        ["bun", "run", "build:static"],
        cwd=dashboard,
        timeout=timeout,
    )
    if build.returncode != 0:
        stage(stages, "build_dashboard", "fail", "bun run build:static failed")
        raise RuntimeError(
            "dashboard build failed:\n" + (build.stderr or build.stdout or "")[-2000:]
        )
    index = dashboard / "out" / "index.html"
    if not index.exists():
        stage(stages, "build_dashboard", "fail", "dashboard out/index.html missing")
        raise RuntimeError("dashboard static export missing index.html")
    stage(stages, "build_dashboard", "pass", "static dashboard built")
    return dashboard / "out"


def run_no_provider_demo(
    repo_root: Path,
    engine_bin: Path,
    dashboard_dir: Path,
    stages: list[dict[str, Any]],
    *,
    timeout: float,
) -> dict[str, Any]:
    result = run_capture(
        [
            sys.executable,
            str(repo_root / DEMO_SCRIPT),
            "--repo-root",
            str(repo_root),
            "--engine-bin",
            str(engine_bin),
            "--dashboard-dir",
            str(dashboard_dir),
            "--timeout",
            str(timeout),
        ],
        cwd=repo_root,
        timeout=timeout + 90,
    )
    output = (result.stdout or "") + (result.stderr or "")
    if result.returncode != 0:
        # Prefer the final meaningful line for logs/report (traceback heads truncate poorly).
        tail = _sanitize_text(output[-2500:] if output else "no demo output")
        print(tail, file=sys.stderr, flush=True)
        stage(
            stages,
            "no_provider_demo",
            "fail",
            _failure_core(f"demo exit {result.returncode}: {tail}"),
            detail={"exit_code": result.returncode},
        )
        raise RuntimeError(_failure_core(f"demo exit {result.returncode}: {tail}"))

    required_marks = (
        "PASS  local runtime healthy",
        "PASS  fixture dispatch recorded",
        "PASS  evidence bound",
        "PASS  stale-head scenario rejected",
        "CLEANUP",
    )
    for mark in required_marks:
        if mark not in output:
            stage(
                stages,
                "no_provider_demo",
                "fail",
                f"demo output missing {mark!r}",
            )
            raise RuntimeError(f"demo missing mark: {mark}")

    # Extract proof path if printed.
    proof_refs: dict[str, Any] = {
        "demo_marks_present": True,
        "stale_head_rejected": True,
        "provider_calls": False,
        "target_repo_writes": False,
    }
    for line in output.splitlines():
        if line.startswith("PROOF "):
            proof_path = line[len("PROOF ") :].strip()
            # Temp path may be gone after cleanup; do not require file retention.
            proof_refs["proof_path_emitted"] = True
            proof_refs["proof_path_kind"] = "temporary"
            del proof_path  # avoid retaining full home-like paths in outer scope
            break

    stage(
        stages,
        "no_provider_demo",
        "pass",
        "fixture dispatch + exact-revision bind + stale-head reject + cleanup",
        detail={
            "proof_kind": "acp-no-provider-demo-proof.v1",
            "stale_head_rejected": True,
            "executor_type": "noop",
        },
    )
    stage(
        stages,
        "exact_revision_binding",
        "pass",
        "demo proof bound to source revision",
        detail={"source_revision_prefix": git_head_sha(repo_root)[:12]},
    )
    stage(
        stages,
        "stale_head_rejection",
        "pass",
        "stale-head scenario rejected by demo owner",
        detail={"stale_head_rejected": True},
    )
    return proof_refs


def run_exact_head_action_self_validation(
    repo_root: Path,
    stages: list[dict[str, Any]],
) -> dict[str, Any]:
    """Self-validate actions/exact-head-check offline (not external adoption)."""
    local_test = repo_root / EXACT_HEAD_LOCAL
    result = run_capture(["bash", str(local_test)], cwd=repo_root, timeout=30)
    if result.returncode != 0:
        stage(
            stages,
            "exact_head_action_local",
            "fail",
            "offline local validation failed",
            detail={"exit_code": result.returncode},
        )
        raise RuntimeError(
            "exact-head local tests failed:\n"
            + (result.stderr or result.stdout or "")[-2000:]
        )
    stage(
        stages,
        "exact_head_action_local",
        "pass",
        "malformed identity fails closed (self-validation)",
        detail={"mock_case": "malformed_inputs"},
    )

    # Mock gh for match / head_moved / fork-deny without network.
    verify = repo_root / EXACT_HEAD_VERIFY
    mock_cases: list[tuple[str, str, int]] = []
    with tempfile.TemporaryDirectory(prefix="acp-exact-head-mock-") as tmp:
        tmp_path = Path(tmp)
        mock_gh = tmp_path / "gh"
        proof_match = tmp_path / "proof-match.json"
        proof_moved = tmp_path / "proof-moved.json"
        proof_fork = tmp_path / "proof-fork.json"

        mock_gh.write_text(
            """#!/usr/bin/env bash
set -euo pipefail
# args: api repos/.../pulls/N --jq '...'
case "${ACP_MOCK_CASE:-}" in
  match)
    cat <<'JSON'
{"number":1,"state":"open","head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","head_ref":"f","head_repo":"o/r","base_repo":"o/r","base_ref":"main","html_url":"https://example.invalid/1"}
JSON
    ;;
  moved)
    cat <<'JSON'
{"number":1,"state":"open","head_sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","head_ref":"f","head_repo":"o/r","base_repo":"o/r","base_ref":"main","html_url":"https://example.invalid/1"}
JSON
    ;;
  fork)
    cat <<'JSON'
{"number":1,"state":"open","head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","head_ref":"f","head_repo":"fork/r","base_repo":"o/r","base_ref":"main","html_url":"https://example.invalid/1"}
JSON
    ;;
  *)
    echo "unknown mock" >&2
    exit 2
    ;;
esac
""",
            encoding="utf-8",
        )
        mock_gh.chmod(0o755)
        mock_jq = shutil.which("jq")
        if not mock_jq:
            stage(
                stages,
                "exact_head_action_mock",
                "fail",
                "jq required for exact-head mock cases",
            )
            raise RuntimeError("jq not found")

        env_base = {
            **os.environ,
            "PATH": f"{tmp_path}:{os.environ.get('PATH', '')}",
            "INPUT_PULL_REQUEST": "1",
            "INPUT_EXPECTED_HEAD": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "INPUT_REPOSITORY": "o/r",
            "INPUT_ALLOW_FORK_HEAD": "false",
            "GITHUB_REPOSITORY": "o/r",
            "GITHUB_WORKFLOW": "external-validation-self",
            "GITHUB_RUN_ID": "0",
            "GITHUB_RUN_ATTEMPT": "1",
            "GITHUB_EVENT_NAME": "self_validation",
            "GITHUB_SHA": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        }

        # Match should pass.
        env = {**env_base, "ACP_MOCK_CASE": "match", "INPUT_PROOF_PATH": str(proof_match)}
        # verify.sh calls `gh api` then pipes to jq -r; our mock returns full JSON once.
        # But verify.sh does: raw="$(gh api ... --jq '{...}')" so gh must apply jq OR return
        # already-filtered JSON. Our mock ignores --jq and returns the object — then the
        # outer script assigns raw to that object. Then it runs echo raw | jq -r fields.
        # That works if raw is the JSON object.
        r = run_capture(["bash", str(verify)], cwd=repo_root, env=env, timeout=15)
        if r.returncode != 0:
            stage(
                stages,
                "exact_head_action_match",
                "fail",
                "expected match to pass",
                detail={"exit_code": r.returncode},
            )
            raise RuntimeError(f"match mock failed:\n{r.stderr}\n{r.stdout}")
        proof = json.loads(proof_match.read_text(encoding="utf-8"))
        if proof.get("status") != "pass" or proof.get("kind") != "exact-head-check-proof.v1":
            stage(stages, "exact_head_action_match", "fail", "proof shape mismatch")
            raise RuntimeError(f"unexpected match proof: {proof}")
        mock_cases.append(("match", "pass", 0))
        stage(
            stages,
            "exact_head_action_match",
            "pass",
            "expected exact head succeeds (mocked self-validation)",
            detail={"mock_case": "match", "proof_kind": "exact-head-check-proof.v1"},
        )

        # Moved head must fail closed.
        env = {**env_base, "ACP_MOCK_CASE": "moved", "INPUT_PROOF_PATH": str(proof_moved)}
        r = run_capture(["bash", str(verify)], cwd=repo_root, env=env, timeout=15)
        if r.returncode == 0:
            stage(stages, "exact_head_action_stale", "fail", "moved head unexpectedly passed")
            raise RuntimeError("moved head mock should fail")
        proof = json.loads(proof_moved.read_text(encoding="utf-8"))
        if proof.get("status") != "fail" or proof.get("reason") != "head_moved":
            stage(stages, "exact_head_action_stale", "fail", "expected head_moved reason")
            raise RuntimeError(f"unexpected moved proof: {proof}")
        mock_cases.append(("moved", "fail", r.returncode))
        stage(
            stages,
            "exact_head_action_stale",
            "pass",
            "moved/stale head fails closed (mocked self-validation)",
            detail={"mock_case": "moved", "stale_head_rejected": True},
        )

        # Fork without allow-fork-head must fail closed.
        env = {**env_base, "ACP_MOCK_CASE": "fork", "INPUT_PROOF_PATH": str(proof_fork)}
        r = run_capture(["bash", str(verify)], cwd=repo_root, env=env, timeout=15)
        if r.returncode == 0:
            stage(stages, "exact_head_action_fork", "fail", "fork head unexpectedly passed")
            raise RuntimeError("fork mock should fail")
        mock_cases.append(("fork", "fail", r.returncode))
        stage(
            stages,
            "exact_head_action_fork",
            "pass",
            "fork head denied by default policy (mocked self-validation)",
            detail={"mock_case": "fork"},
        )

    return {
        "label": "self-validation",
        "not_external_adoption": True,
        "cases": [c[0] for c in mock_cases],
        "job_summary_supported": True,
        "json_proof_kind": "exact-head-check-proof.v1",
    }


def verify_cleanup(
    repo_root: Path,
    target_dir: Path,
    stages: list[dict[str, Any]],
) -> None:
    demo_state = repo_root / ".acp-demo-state"
    if demo_state.exists():
        stage(
            stages,
            "cleanup",
            "fail",
            ".acp-demo-state still present after validation",
        )
        raise RuntimeError("demo left persistent state")
    # Disposable target dir is removed by caller; assert no engine listening is hard.
    # Ensure we do not leave the demo keep-state behind.
    stage(
        stages,
        "cleanup",
        "pass",
        "no .acp-demo-state; disposable dirs removed by runner",
        detail={"path_kind": "disposable"},
    )
    del target_dir  # used only for intent clarity


def build_report(
    *,
    source_revision: str,
    versions: dict[str, str],
    stages: list[dict[str, Any]],
    status: str,
    reason: str,
    elapsed_ms: int,
    demo_proof: dict[str, Any] | None,
    exact_head: dict[str, Any] | None,
) -> dict[str, Any]:
    return {
        "kind": REPORT_KIND,
        "source_revision": source_revision,
        "os": versions.get("os", platform.system()),
        "arch": versions.get("arch", platform.machine()),
        "platform": versions.get("platform", platform.platform())[:120],
        "tool_versions": {
            k: v
            for k, v in versions.items()
            if k
            in {
                "python",
                "git",
                "cargo",
                "rustc",
                "bun",
                "uv",
                "doctor_checks",
            }
        },
        "stages": stages,
        "status": status,
        "reason": reason,
        "elapsed_ms": elapsed_ms,
        "demo_proof": demo_proof or {},
        "exact_head_action": exact_head or {},
        "cleanup": next(
            (s for s in stages if s.get("name") == "cleanup"),
            {"name": "cleanup", "status": "unknown", "reason": "not_run"},
        ),
        "provider_calls": False,
        "target_repo_writes": False,
        "external_adoption_claimed": False,
        "notes": (
            "Self-hosted clean-environment engineering validation. "
            "Not evidence of external users, dependents, or Marketplace adoption."
        ),
    }


def run_self_test() -> int:
    """Pure contract checks without building the engine."""
    report = build_report(
        source_revision="a" * 40,
        versions={"os": "Linux", "arch": "x86_64", "python": "3.11.0", "cargo": "cargo 1"},
        stages=[
            {"name": "detect_tools", "status": "pass", "reason": "ok"},
            {"name": "cleanup", "status": "pass", "reason": "ok"},
        ],
        status="pass",
        reason="self_test",
        elapsed_ms=1,
        demo_proof={"provider_calls": False},
        exact_head={"label": "self-validation", "not_external_adoption": True},
    )
    if report["kind"] != REPORT_KIND:
        raise SystemExit("kind mismatch")
    if report["provider_calls"] is not False:
        raise SystemExit("provider_calls must be false")
    if report["external_adoption_claimed"] is not False:
        raise SystemExit("must not claim external adoption")
    # Ensure no home path keys.
    blob = json.dumps(report)
    if "/home/" in blob or "Users/" in blob:
        raise SystemExit("report must not embed home paths")
    print("self-test ok")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Clean-environment validation: tools, disposable build, no-provider demo, "
            "exact-head action self-validation, machine-readable report."
        )
    )
    parser.add_argument("--repo-root", type=Path, default=repo_root_from_script())
    parser.add_argument(
        "--report",
        type=Path,
        help="Write external_validation_report.v1 JSON to this path",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Also print the report JSON to stdout at the end",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="Pure report-contract checks without building or starting the engine",
    )
    parser.add_argument(
        "--skip-demo",
        action="store_true",
        help="Skip engine build and live demo (install contract + exact-head only)",
    )
    parser.add_argument("--build-timeout", type=float, default=900.0)
    parser.add_argument(
        "--demo-timeout",
        type=float,
        default=120.0,
        help="Seconds to wait for engine health during the no-provider demo",
    )
    args = parser.parse_args()
    repo_root = args.repo_root.resolve()

    if args.self_test:
        return run_self_test()

    # Line-buffer human progress so CI logs are ordered if stdout is not a TTY.
    try:
        sys.stdout.reconfigure(line_buffering=True)  # type: ignore[attr-defined]
        sys.stderr.reconfigure(line_buffering=True)  # type: ignore[attr-defined]
    except Exception:  # noqa: BLE001 - best-effort on older interpreters
        pass

    started = time.monotonic()
    stages: list[dict[str, Any]] = []
    versions: dict[str, str] = {}
    demo_proof: dict[str, Any] | None = None
    exact_head: dict[str, Any] | None = None
    source_revision = ""
    status = "fail"
    reason = "incomplete"
    target_tmp: tempfile.TemporaryDirectory[str] | None = None

    try:
        source_revision = git_head_sha(repo_root)
        print(f"INFO  source_revision={source_revision}", flush=True)
        print(
            "INFO  clean-environment validation (no provider, no API key, no target write)",
            flush=True,
        )
        print(
            "INFO  self-validation only - not external adoption evidence",
            flush=True,
        )

        versions = detect_tools(repo_root, stages, skip_demo=args.skip_demo)
        verify_install_contract(repo_root, stages)

        if not args.skip_demo:
            # Prefer runner-provided temp when present (GitHub Actions RUNNER_TEMP).
            tmp_parent = os.environ.get("RUNNER_TEMP") or None
            if tmp_parent and not Path(tmp_parent).is_dir():
                tmp_parent = None
            target_tmp = tempfile.TemporaryDirectory(
                prefix="acp-extval-target-",
                dir=tmp_parent,
            )
            target_dir = Path(target_tmp.name) / "target"
            target_dir.mkdir(parents=True, exist_ok=True)
            engine_bin = build_engine(
                repo_root,
                target_dir,
                stages,
                timeout=args.build_timeout,
            )
            # Confirm the disposable binary is executable before the demo owns it.
            if not os.access(engine_bin, os.X_OK):
                stage(
                    stages,
                    "build_engine",
                    "fail",
                    "engine binary is not executable",
                )
                raise RuntimeError(f"engine binary not executable: {engine_bin.name}")
            dashboard_dir = build_dashboard(
                repo_root,
                stages,
                timeout=args.build_timeout,
            )
            demo_proof = run_no_provider_demo(
                repo_root,
                engine_bin,
                dashboard_dir,
                stages,
                timeout=args.demo_timeout,
            )
        else:
            stage(stages, "build_engine", "skip", "skip-demo")
            stage(stages, "build_dashboard", "skip", "skip-demo")
            stage(stages, "no_provider_demo", "skip", "skip-demo")
            stage(stages, "exact_revision_binding", "skip", "skip-demo")
            stage(stages, "stale_head_rejection", "skip", "skip-demo")

        exact_head = run_exact_head_action_self_validation(repo_root, stages)

        if target_tmp is not None:
            target_tmp.cleanup()
            target_tmp = None
        verify_cleanup(repo_root, Path("/tmp"), stages)

        status = "pass"
        reason = "all_stages_passed"
        print("DONE  external validation passed (engineering usability only)")
        return 0
    except Exception as exc:  # noqa: BLE001 - top-level validation boundary
        reason = _failure_core(f"{type(exc).__name__}: {exc}")
        status = "fail"
        print(f"FAIL  {reason}", file=sys.stderr, flush=True)
        # Record a terminal stage if the last one was not already fail.
        if not stages or stages[-1].get("status") != "fail":
            stage(stages, "terminal", "fail", reason[:200])
        return 1
    finally:
        if target_tmp is not None:
            target_tmp.cleanup()
        elapsed_ms = int((time.monotonic() - started) * 1000)
        report = build_report(
            source_revision=source_revision or ("0" * 40),
            versions=versions or {"os": platform.system(), "arch": platform.machine()},
            stages=stages,
            status=status,
            reason=reason,
            elapsed_ms=elapsed_ms,
            demo_proof=demo_proof,
            exact_head=exact_head,
        )
        if args.report:
            args.report.parent.mkdir(parents=True, exist_ok=True)
            args.report.write_text(
                json.dumps(report, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            print(f"REPORT  {args.report}")
        if args.json:
            print(json.dumps(report, indent=2, sort_keys=True))


if __name__ == "__main__":
    raise SystemExit(main())
