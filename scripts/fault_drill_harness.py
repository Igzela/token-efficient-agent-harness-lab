#!/usr/bin/env python3
"""Bounded PE-6 v2 harness for owner-emitted fault evidence."""

from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Mapping

from scripts.fault_drill_contract import (
    MAX_BYTES,
    MAX_JSON_BYTES,
    ContractError,
    build_report_v2,
    build_result_v2,
    canonical_json_bytes,
    parse_json_bytes,
    validate_owner_evidence_v2,
    write_canonical_json,
)
from scripts.fault_drill_registry import ScenarioSpec, get_scenario, scenario_for, validate_registry


ROOT = Path(__file__).resolve().parents[1]
_ACTIVE_RESOURCE_IDS: set[str] = set()
_ACTIVE_LOCK = threading.Lock()
_DISPOSABLE_POSTGRES_URL = "postgres://testuser:testpass@localhost:5432/testdb"


@dataclass(frozen=True)
class CommandOutcome:
    returncode: int | None
    timed_out: bool = False
    output_exceeded: bool = False


def fixed_command(spec: ScenarioSpec) -> tuple[str, ...]:
    if spec.command_kind == "cargo_test":
        return (
            "cargo", "test", "-p", "engine", "--test", "test_pe6_fault_drills",
            *spec.command_args, "--", "--exact", "--test-threads=1",
        )
    if spec.command_kind == "cargo_pg_test":
        return (
            "cargo", "test", "-p", "engine", "--features", "pg-tests",
            "--test", "test_pe6_fault_drills", *spec.command_args,
            "--", "--exact", "--test-threads=1",
        )
    if spec.command_kind == "python_test":
        return (sys.executable, "-m", "unittest", *spec.command_args)
    raise ContractError("command kind is not allowlisted")


def _sanitized_environment(
    *, root: Path, postgres: bool, scenario_path: Path, evidence_path: Path
) -> dict[str, str]:
    result = dict(os.environ)
    toolchain_home = result.get("HOME")
    forbidden = ("API_KEY", "TOKEN", "SECRET", "PASSWORD", "PRIVATE_KEY", "CREDENTIAL")
    for key in list(result):
        if any(marker in key.upper() for marker in forbidden):
            result.pop(key, None)
    result.update(
        {
            "HOME": str(root),
            "TMPDIR": str(root),
            "TMP": str(root),
            "TEMP": str(root),
            "ACP_PE6_DISPOSABLE_ROOT": str(root),
            "ACP_PE6_SCENARIO_PATH": str(scenario_path),
            "ACP_PE6_EVIDENCE_PATH": str(evidence_path),
            "ACP_ENABLE_PROVIDER_EXECUTION": "0",
            "ACP_REAL_RUNNER_KILL_SWITCH": "1",
            "PYTHONPATH": str(ROOT),
        }
    )
    if toolchain_home:
        rustup_home = Path(toolchain_home) / ".rustup"
        cargo_home = Path(toolchain_home) / ".cargo"
        if rustup_home.is_dir():
            result["RUSTUP_HOME"] = str(rustup_home)
        if cargo_home.is_dir():
            result["CARGO_HOME"] = str(cargo_home)
    if not postgres:
        result.pop("ACP_TEST_DATABASE_URL", None)
    return result


def _disposable_postgres_service_available() -> bool:
    return (
        os.environ.get("GITHUB_ACTIONS", "").lower() == "true"
        and os.environ.get("ACP_TEST_DATABASE_URL") == _DISPOSABLE_POSTGRES_URL
    )


def _execute_fixed_command(
    command: tuple[str, ...], *, cwd: Path, env: Mapping[str, str], timeout_ms: int
) -> CommandOutcome:
    process = subprocess.Popen(
        list(command), cwd=str(cwd), env=dict(env), stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, start_new_session=(os.name == "posix"),
    )
    output_exceeded = threading.Event()

    def terminate() -> None:
        if process.poll() is not None:
            return
        if os.name == "posix":
            try:
                os.killpg(process.pid, 9)
            except ProcessLookupError:
                pass
        else:
            process.kill()

    def drain_output() -> None:
        assert process.stdout is not None
        observed = 0
        for chunk in iter(lambda: process.stdout.read(64 * 1024), b""):
            observed += len(chunk)
            if observed > MAX_BYTES:
                output_exceeded.set()
                terminate()

    reader = threading.Thread(target=drain_output, name="pe6-owner-output", daemon=True)
    reader.start()
    try:
        process.wait(timeout=timeout_ms / 1000.0)
    except subprocess.TimeoutExpired:
        terminate()
        process.wait(timeout=5)
        reader.join(timeout=5)
        if process.stdout is not None:
            process.stdout.close()
        return CommandOutcome(returncode=None, timed_out=True)
    reader.join(timeout=5)
    if reader.is_alive():
        terminate()
        process.wait(timeout=5)
        reader.join(timeout=5)
        if process.stdout is not None:
            process.stdout.close()
        return CommandOutcome(returncode=None, output_exceeded=True)
    if process.stdout is not None:
        process.stdout.close()
    if output_exceeded.is_set():
        return CommandOutcome(returncode=process.returncode, output_exceeded=True)
    return CommandOutcome(returncode=process.returncode)


def _cleanup_resource(path: Path, *, fail: bool = False) -> bool:
    if fail:
        try:
            shutil.rmtree(path)
        except OSError:
            pass
        return False
    try:
        shutil.rmtree(path)
    except OSError:
        return False
    return not path.exists()


def _read_owner_evidence(path: Path, scenario: Mapping[str, object]) -> tuple[dict[str, object], str]:
    if not path.is_file() or path.stat().st_size > MAX_JSON_BYTES:
        raise ContractError("owner command did not emit bounded evidence")
    raw = path.read_bytes()
    value = parse_json_bytes(raw)
    if not isinstance(value, dict):
        raise ContractError("owner evidence is not an object")
    validated = validate_owner_evidence_v2(value, scenario)
    if raw != canonical_json_bytes(validated):
        raise ContractError("owner evidence must use the canonical encoding")
    return validated, hashlib.sha256(raw).hexdigest()


def _result_without_owner(
    scenario: Mapping[str, object], *, status: str, reasons: list[str],
    duration_ms: int, exit_code: int | None, cleanup_ok: bool,
) -> dict[str, object]:
    if not cleanup_ok:
        status = "cleanup_failed"
        reasons = [*reasons, "HARNESS_CLEANUP_FAILED"]
    return build_result_v2(
        scenario=scenario,
        configured_timeout_ms=int(scenario["timeout_ms"]),
        observed_duration_ms=duration_ms,
        owner_exit_code=exit_code,
        owner_evidence=None,
        owner_evidence_sha256=None,
        status=status,
        reason_codes=reasons,
        harness_cleanup={
            "outcome": "passed" if cleanup_ok else "failed",
            "observation": (
                "harness disposable directory removal was verified"
                if cleanup_ok else "harness disposable directory removal was not verified"
            ),
        },
    )


def run_scenario(
    scenario_id: str,
    *,
    source_head: str,
    seed: int = 0,
    worker_id: int = 0,
    command_executor: Callable[..., CommandOutcome] | None = None,
    fail_cleanup: bool = False,
) -> dict[str, object]:
    validate_registry()
    spec = get_scenario(scenario_id)
    scenario = scenario_for(spec, source_head=source_head, seed=seed, worker_id=worker_id)
    resource_ids = tuple(resource["resource_id"] for resource in scenario["resources"])
    with _ACTIVE_LOCK:
        if any(resource_id in _ACTIVE_RESOURCE_IDS for resource_id in resource_ids):
            return _result_without_owner(
                scenario, status="invalid_scenario", reasons=["RESOURCE_ID_CONFLICT"],
                duration_ms=0, exit_code=None, cleanup_ok=True,
            )
        _ACTIVE_RESOURCE_IDS.update(resource_ids)

    disposable_root = Path(tempfile.mkdtemp(prefix="pe6-drill-"))
    scenario_path = disposable_root / "scenario.v2.json"
    evidence_path = disposable_root / "owner-evidence.v2.json"
    write_canonical_json(scenario_path, scenario)
    owner_evidence: dict[str, object] | None = None
    owner_hash: str | None = None
    outcome = CommandOutcome(returncode=None)
    observed_duration_ms = 0
    status = "failed_recovery"
    reasons = ["OWNER_EVIDENCE_INVALID"]
    postgres = spec.environment == "postgres-service"
    unsupported = postgres and not _disposable_postgres_service_available()
    try:
        if unsupported:
            status = "unsupported"
            reasons = ["UNSUPPORTED_ENVIRONMENT"]
        else:
            executor = command_executor or _execute_fixed_command
            started = time.monotonic_ns()
            outcome = executor(
                fixed_command(spec), cwd=ROOT,
                env=_sanitized_environment(
                    root=disposable_root, postgres=postgres,
                    scenario_path=scenario_path, evidence_path=evidence_path,
                ),
                timeout_ms=spec.timeout_ms,
            )
            observed_duration_ms = (time.monotonic_ns() - started) // 1_000_000
            if outcome.timed_out:
                status, reasons = "aborted", ["DRILL_TIMEOUT"]
            elif outcome.output_exceeded:
                status, reasons = "aborted", ["OWNER_OUTPUT_BOUNDS_EXCEEDED"]
            elif outcome.returncode != 0:
                status, reasons = "failed_recovery", ["OWNER_TEST_FAILED"]
            else:
                try:
                    owner_evidence, owner_hash = _read_owner_evidence(evidence_path, scenario)
                    categories = {
                        check["category"]: check["outcome"]
                        for check in owner_evidence["checks"]
                        if check["outcome"] == "failed"
                    }
                    if owner_evidence["cleanup"]["outcome"] != "passed":
                        status, reasons = "cleanup_failed", ["OWNER_CLEANUP_FAILED"]
                    elif categories:
                        status = "failed_rollback" if "rollback" in categories else "failed_recovery"
                        reasons = ["OWNER_CHECK_FAILED"]
                    else:
                        status, reasons = "passed", ["DRILL_PASSED", "OWNER_EVIDENCE_VERIFIED"]
                except ContractError:
                    status, reasons = "failed_recovery", ["OWNER_EVIDENCE_INVALID"]
    except Exception:
        status, reasons = "aborted", ["DRILL_ABORTED"]
    finally:
        cleanup_ok = _cleanup_resource(disposable_root, fail=fail_cleanup)
        with _ACTIVE_LOCK:
            for resource_id in resource_ids:
                _ACTIVE_RESOURCE_IDS.discard(resource_id)

    if not cleanup_ok:
        status = "cleanup_failed"
        reasons = [*reasons, "HARNESS_CLEANUP_FAILED"]
    return build_result_v2(
        scenario=scenario,
        configured_timeout_ms=spec.timeout_ms,
        observed_duration_ms=observed_duration_ms,
        owner_exit_code=outcome.returncode,
        owner_evidence=owner_evidence,
        owner_evidence_sha256=owner_hash,
        status=status,
        reason_codes=reasons,
        harness_cleanup={
            "outcome": "passed" if cleanup_ok else "failed",
            "observation": (
                "harness disposable directory removal was verified"
                if cleanup_ok else "harness disposable directory removal was not verified"
            ),
        },
    )


def run_selected(
    scenario_ids: tuple[str, ...], *, source_head: str, seed: int, worker_id: int
) -> list[dict[str, object]]:
    return [
        run_scenario(scenario_id, source_head=source_head, seed=seed, worker_id=worker_id)
        for scenario_id in scenario_ids
    ]


def report_for(
    *, suite: str, source_head: str, seed: int, worker_id: int,
    results: list[Mapping[str, object]],
) -> dict[str, object]:
    del seed, worker_id
    return build_report_v2(suite=suite, source_head=source_head, results=list(results))
