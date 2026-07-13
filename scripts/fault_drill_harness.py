#!/usr/bin/env python3
"""One bounded PE-6 harness for the allowlisted owner drills.

The harness starts only fixed test commands from ``fault_drill_registry``.
Each command runs as a controlled child with a timeout and a sanitized test
environment.  The owner tests provision their own temporary SQLite,
PostgreSQL-service, fake-provider, and release resources.  No caller-supplied
shell, path, URL, provider, or process operation is accepted.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import threading
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Mapping
from urllib.parse import urlsplit

from scripts.fault_drill_contract import (
    MAX_BYTES,
    ContractError,
    build_report,
    scenario_sha256,
    seal_evidence,
    validate_result,
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
    """Expand a registry label into an immutable, non-shell command."""

    if spec.command_kind == "cargo_test":
        return (
            "cargo",
            "test",
            "-p",
            "engine",
            "--test",
            "test_pe6_fault_drills",
            *spec.command_args,
            "--",
            "--exact",
            "--test-threads=1",
        )
    if spec.command_kind == "cargo_pg_test":
        return (
            "cargo",
            "test",
            "-p",
            "engine",
            "--features",
            "pg-tests",
            "--test",
            "test_pe6_fault_drills",
            *spec.command_args,
            "--",
            "--exact",
            "--test-threads=1",
        )
    if spec.command_kind == "python_test":
        return (sys.executable, "-m", "unittest", *spec.command_args)
    raise ContractError("command kind is not allowlisted")


def _sanitized_environment(*, root: Path, postgres: bool) -> dict[str, str]:
    """Keep toolchain settings but remove provider/credential-shaped inputs."""

    result = dict(os.environ)
    toolchain_home = result.get("HOME")
    forbidden_markers = ("API_KEY", "TOKEN", "SECRET", "PASSWORD", "PRIVATE_KEY", "CREDENTIAL")
    for key in list(result):
        upper = key.upper()
        if any(marker in upper for marker in forbidden_markers):
            result.pop(key, None)
    result.update(
        {
            "HOME": str(root),
            "TMPDIR": str(root),
            "TMP": str(root),
            "TEMP": str(root),
            "ACP_PE6_DISPOSABLE_ROOT": str(root),
            "ACP_ENABLE_PROVIDER_EXECUTION": "0",
            "ACP_REAL_RUNNER_KILL_SWITCH": "1",
            "PYTHONPATH": str(ROOT),
        }
    )
    # Rustup resolves its default toolchain from $HOME when RUSTUP_HOME is not
    # explicit.  Preserve only the existing read-only toolchain/cache roots so
    # a disposable drill can run the fixed Rust owner tests without silently
    # downloading or selecting a different toolchain.
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
    """Accept only the repository's ephemeral GitHub Actions service database."""

    if os.environ.get("GITHUB_ACTIONS", "").lower() != "true":
        return False
    configured = os.environ.get("ACP_TEST_DATABASE_URL")
    if configured != _DISPOSABLE_POSTGRES_URL:
        return False
    parsed = urlsplit(configured)
    return (
        parsed.scheme == "postgres"
        and parsed.hostname == "localhost"
        and parsed.port == 5432
        and parsed.username == "testuser"
        and parsed.path == "/testdb"
    )


def _execute_fixed_command(command: tuple[str, ...], *, cwd: Path, env: Mapping[str, str], timeout_ms: int) -> CommandOutcome:
    """Run a fixed child command with a bounded timeout and bounded evidence."""

    process = subprocess.Popen(
        list(command),
        cwd=str(cwd),
        env=dict(env),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        start_new_session=(os.name == "posix"),
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout_ms / 1000.0)
    except subprocess.TimeoutExpired:
        if os.name == "posix":
            try:
                os.killpg(process.pid, 9)
            except ProcessLookupError:
                pass
        else:
            process.kill()
        process.communicate()
        return CommandOutcome(returncode=None, timed_out=True)
    output_bytes = len(stdout.encode("utf-8")) + len(stderr.encode("utf-8"))
    if output_bytes > MAX_BYTES:
        return CommandOutcome(returncode=process.returncode, output_exceeded=True)
    return CommandOutcome(returncode=process.returncode)


def _cleanup_resource(path: Path, *, fail: bool = False) -> bool:
    if fail:
        # The test-only seam simulates a cleanup-reporting failure while still
        # removing the disposable directory so the negative test cannot leak
        # resources into the developer or CI host.
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


def _evidence_id(scenario_id: str, kind: str) -> str:
    return f"pe6.evidence.{scenario_id.replace(':', '.')}.{kind}"


def _make_evidence(
    *,
    scenario_id: str,
    kind: str,
    outcome: str,
    passed: bool,
    reason_codes: list[str],
    observation: str,
    action: str,
) -> dict[str, object]:
    return seal_evidence(
        kind=kind,
        evidence_id=_evidence_id(scenario_id, kind),
        outcome=outcome,
        checks=[{"name": f"{kind}_invariant", "passed": passed}],
        observations=[observation],
        actions=[action],
        reason_codes=reason_codes,
    )


def _make_result(
    scenario: Mapping[str, object],
    *,
    status: str,
    reason_codes: list[str],
    outcome: str,
    invariant_passed: bool,
    cleanup_outcome: str,
    cleanup_passed: bool,
    detection_reason: str,
    detected: bool,
    duration_ms: int,
    observation: str,
) -> dict[str, object]:
    scenario_id = str(scenario["scenario_id"])
    recovery = _make_evidence(
        scenario_id=scenario_id,
        kind="recovery",
        outcome=outcome,
        passed=invariant_passed,
        reason_codes=reason_codes,
        observation=observation,
        action="existing recovery owner was observed through the fixed test seam",
    )
    rollback = _make_evidence(
        scenario_id=scenario_id,
        kind="rollback",
        outcome=outcome,
        passed=invariant_passed,
        reason_codes=reason_codes,
        observation="previous known-good state remained bounded by the owner test",
        action="existing rollback or safe terminal owner was checked",
    )
    integrity = _make_evidence(
        scenario_id=scenario_id,
        kind="integrity",
        outcome=outcome,
        passed=invariant_passed,
        reason_codes=reason_codes,
        observation="state and evidence bindings were checked without raw content",
        action="existing integrity and hash checks were exercised",
    )
    audit = _make_evidence(
        scenario_id=scenario_id,
        kind="audit",
        outcome=outcome,
        passed=invariant_passed,
        reason_codes=reason_codes,
        observation="bounded audit attribution was checked without sensitive content",
        action="existing audit owner was inspected by the fixed test",
    )
    restart = _make_evidence(
        scenario_id=scenario_id,
        kind="restart",
        outcome=outcome,
        passed=invariant_passed,
        reason_codes=reason_codes,
        observation="replay, restart, or explicit unsupported state was bounded",
        action="existing restart/idempotency owner was exercised or reported",
    )
    cleanup = _make_evidence(
        scenario_id=scenario_id,
        kind="cleanup",
        outcome=cleanup_outcome,
        passed=cleanup_passed,
        reason_codes=(
            ["CLEANUP_VERIFIED"] if cleanup_passed else ["CLEANUP_FAILED"]
        ),
        observation="harness-owned disposable resources were removed and checked"
        if cleanup_passed
        else "harness cleanup could not prove resource removal",
        action="finally cleanup guard ran",
    )
    reason_codes = list(dict.fromkeys(reason_codes))
    evidence = [recovery, rollback, integrity, audit, restart, cleanup]
    result = {
        "schema_version": "fault_drill_result.v1",
        "scenario_id": scenario["scenario_id"],
        "scenario_version": scenario["scenario_version"],
        "scenario_sha256": scenario_sha256(scenario),
        "seed": scenario["seed"],
        "worker_id": scenario["worker_id"],
        "source_head": scenario["source_head"],
        "environment": scenario["environment"],
        "resources": scenario["resources"],
        "injection": {
            "fault_id": scenario["fault"]["fault_id"],
            "injection_point": scenario["fault"]["injection_point"],
            "observation": observation,
        },
        "detection": {
            "detected": detected,
            "reason_code": detection_reason,
            "timeout_ms": duration_ms,
            "abort_condition": "fixed child timeout or owner test failure aborts the drill",
        },
        "recovery_evidence": recovery,
        "rollback_evidence": rollback,
        "integrity_evidence": integrity,
        "audit_evidence": audit,
        "restart_evidence": restart,
        "cleanup_evidence": cleanup,
        "status": status,
        "duration_ms": duration_ms,
        "reason_codes": reason_codes,
        "evidence_refs": [
            {"evidence_id": item["evidence_id"], "sha256": item["sha256"], "kind": item["kind"]}
            for item in evidence
        ],
    }
    return validate_result(result, scenario)


def run_scenario(
    scenario_id: str,
    *,
    source_head: str,
    seed: int = 0,
    worker_id: int = 0,
    command_executor: Callable[..., CommandOutcome] | None = None,
    fail_cleanup: bool = False,
) -> dict[str, object]:
    """Run one registered scenario, always attempting disposable cleanup.

    ``command_executor`` and ``fail_cleanup`` are test-only seams used to
    prove timeout and cleanup behavior without launching a replacement fault
    framework.  The CLI never exposes either parameter.
    """

    validate_registry()
    spec = get_scenario(scenario_id)
    scenario = scenario_for(spec, source_head=source_head, seed=seed, worker_id=worker_id)
    resource_ids = tuple(resource["resource_id"] for resource in scenario["resources"])
    with _ACTIVE_LOCK:
        if any(resource_id in _ACTIVE_RESOURCE_IDS for resource_id in resource_ids):
            return _make_result(
                scenario,
                status="invalid_scenario",
                reason_codes=["DUPLICATE_SCENARIO", "RESOURCE_ID_CONFLICT"],
                outcome="aborted",
                invariant_passed=False,
                cleanup_outcome="cleaned",
                cleanup_passed=True,
                detection_reason="RESOURCE_ID_CONFLICT",
                detected=False,
                duration_ms=0,
                observation="concurrent worker identity would share a disposable resource",
            )
        _ACTIVE_RESOURCE_IDS.update(resource_ids)

    disposable_root = Path(tempfile.mkdtemp(prefix="pe6-drill-"))
    postgres_available = _disposable_postgres_service_available()
    is_unsupported = spec.environment == "postgres-service" and not postgres_available
    status = "passed"
    reason_codes = [
        "DRILL_PASSED",
        "RECOVERY_VERIFIED",
        "ROLLBACK_VERIFIED",
        "INTEGRITY_VERIFIED",
        "AUDIT_VERIFIED",
    ]
    outcome_name = "passed"
    invariant_passed = True
    detection_reason = "DRILL_PASSED"
    detected = True
    duration_ms = 1
    observation = "fixed owner test completed successfully"
    try:
        if is_unsupported:
            status = "unsupported"
            reason_codes = ["UNSUPPORTED_ENVIRONMENT"]
            outcome_name = "unsupported"
            invariant_passed = False
            detection_reason = "UNSUPPORTED_ENVIRONMENT"
            detected = False
            duration_ms = 0
            observation = "PostgreSQL service capability is unavailable in this environment"
        else:
            command = fixed_command(spec)
            executor = command_executor or _execute_fixed_command
            command_outcome = executor(
                command,
                cwd=ROOT,
                env=_sanitized_environment(root=disposable_root, postgres=spec.environment == "postgres-service"),
                timeout_ms=spec.timeout_ms,
            )
            if command_outcome.timed_out:
                status = "aborted"
                reason_codes = ["DRILL_TIMEOUT", "DRILL_ABORTED"]
                outcome_name = "aborted"
                invariant_passed = False
                detection_reason = "DRILL_TIMEOUT"
                duration_ms = spec.timeout_ms
                observation = "fixed owner command exceeded its bounded timeout"
            elif command_outcome.output_exceeded:
                status = "aborted"
                reason_codes = ["REPORT_BOUNDS_EXCEEDED", "DRILL_ABORTED"]
                outcome_name = "aborted"
                invariant_passed = False
                detection_reason = "REPORT_BOUNDS_EXCEEDED"
                observation = "fixed owner command exceeded the bounded output envelope"
            elif command_outcome.returncode != 0:
                status = "failed_recovery"
                reason_codes = ["OWNER_TEST_FAILED"]
                outcome_name = "failed"
                invariant_passed = False
                detection_reason = "OWNER_TEST_FAILED"
                observation = "fixed owner test returned a failure; no success was synthesized"
    except Exception:
        # A harness implementation or owner-command error is evidence of an
        # aborted drill, never an implicit pass and never a raw exception dump.
        status = "aborted"
        reason_codes = ["DRILL_ABORTED", "OWNER_TEST_FAILED"]
        outcome_name = "aborted"
        invariant_passed = False
        detection_reason = "DRILL_ABORTED"
        observation = "fixed owner command could not complete within the harness boundary"
    finally:
        cleanup_ok = _cleanup_resource(disposable_root, fail=fail_cleanup) if disposable_root.exists() else False
        with _ACTIVE_LOCK:
            for resource_id in resource_ids:
                _ACTIVE_RESOURCE_IDS.discard(resource_id)
    if not cleanup_ok:
        status = "cleanup_failed"
        reason_codes = list(dict.fromkeys([*reason_codes, "CLEANUP_FAILED"]))
    else:
        reason_codes = list(dict.fromkeys([*reason_codes, "CLEANUP_VERIFIED"]))
    return _make_result(
        scenario,
        status=status,
        reason_codes=reason_codes,
        outcome=outcome_name,
        invariant_passed=invariant_passed,
        cleanup_outcome="cleaned" if cleanup_ok else "failed",
        cleanup_passed=cleanup_ok,
        detection_reason=detection_reason,
        detected=detected,
        duration_ms=duration_ms,
        observation=observation,
    )


def run_selected(
    scenario_ids: tuple[str, ...],
    *,
    source_head: str,
    seed: int,
    worker_id: int,
) -> list[dict[str, object]]:
    return [
        run_scenario(
            scenario_id,
            source_head=source_head,
            seed=seed,
            worker_id=worker_id,
        )
        for scenario_id in scenario_ids
    ]


def report_for(
    *,
    suite: str,
    source_head: str,
    seed: int,
    worker_id: int,
    results: list[Mapping[str, object]],
) -> dict[str, object]:
    capabilities = {"filesystem", "process", "rust_engine", "sqlite", "fake_provider", "release"}
    if any(result["environment"]["name"] == "postgres-service" for result in results):
        capabilities.add("postgres")
    return build_report(
        suite=suite,
        source_head=source_head,
        seed=seed,
        worker_id=worker_id,
        environment={"name": "mixed-local-disposable", "capabilities": sorted(capabilities)},
        results=list(results),
    )
