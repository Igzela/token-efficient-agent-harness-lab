#!/usr/bin/env python3
"""The PE-6 allowlisted scenario registry.

Registry entries are data, not a second scheduler.  Each command is a fixed
test owner invocation; callers may select only these IDs or named suites.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Mapping

from scripts.fault_drill_contract import (
    CAPABILITIES,
    SCENARIO_SCHEMA_VERSION_V2,
    ContractError,
    validate_scenario_v2,
)


@dataclass(frozen=True)
class ScenarioSpec:
    scenario_id: str
    subsystem: str
    owner: str
    fault_id: str
    injection_point: str
    mode: str
    required_capabilities: tuple[str, ...]
    environment: str
    resource_kinds: tuple[tuple[str, str], ...]
    suites: tuple[str, ...]
    command_kind: str
    command_args: tuple[str, ...]
    timeout_ms: int


REGISTRY: tuple[ScenarioSpec, ...] = (
    ScenarioSpec(
        "pe6.harness.timeout_cleanup.v2",
        "harness",
        "scripts/fault_drill_harness.py",
        "pe6.fault.harness.timeout_cleanup.v2",
        "executor_timeout",
        "timeout",
        ("filesystem", "process"),
        "linux-sqlite",
        (("temp_dir", "filesystem"), ("controlled_process", "process")),
        ("core",),
        "python_test",
        ("tools.test_pe6_harness_drill.HarnessOwnerDrillTests.test_owner_timeout_and_cleanup_evidence",),
        30_000,
    ),
    ScenarioSpec(
        "pe6.storage.sqlite.atomicity.v2",
        "storage",
        "engine::storage::local_product_store::workflow_runs",
        "pe6.fault.storage.sqlite.duplicate_replay.v2",
        "duplicate_write",
        "duplicate",
        ("filesystem", "sqlite", "rust_engine"),
        "linux-sqlite",
        (("temp_dir", "filesystem"), ("sqlite_db", "sqlite")),
        ("core", "storage"),
        # Reuse the pg-tests feature graph so CI's post-suite harness re-run does not
        # rebuild a second engine fingerprint under a sanitized HOME/TMPDIR sandbox.
        "cargo_pg_test",
        ("pe6_sqlite_atomicity_and_integrity",),
        90_000,
    ),
    ScenarioSpec(
        "pe6.storage.sqlite.backup_restore.v2",
        "storage",
        "engine::storage::backup_manager",
        "pe6.fault.storage.sqlite.backup_restore.v2",
        "integrity_tamper",
        "tamper",
        ("filesystem", "sqlite", "rust_engine"),
        "linux-sqlite",
        (("temp_dir", "filesystem"), ("sqlite_db", "sqlite")),
        ("storage",),
        "cargo_pg_test",
        ("pe6_sqlite_backup_restore_and_cleanup",),
        90_000,
    ),
    ScenarioSpec(
        "pe6.storage.postgres.atomicity.v2",
        "storage",
        "engine::storage::local_product_store::pg_backend",
        "pe6.fault.storage.postgres.atomicity.v2",
        "during_transaction",
        "interrupt",
        ("filesystem", "postgres", "rust_engine"),
        "postgres-service",
        (("temp_dir", "filesystem"), ("postgres_container", "postgres")),
        ("storage",),
        "cargo_pg_test",
        ("pe6_postgres_atomicity_when_service_is_available",),
        120_000,
    ),
    ScenarioSpec(
        "pe6.workflow.recovery.v2",
        "workflow",
        "engine::storage::local_product_store::workflow_runs + engine::scheduler",
        "pe6.fault.workflow.recovery.v2",
        "concurrent_conflict",
        "race",
        ("filesystem", "sqlite", "rust_engine", "process"),
        "linux-sqlite",
        (("temp_dir", "filesystem"), ("sqlite_db", "sqlite"), ("controlled_process", "process")),
        ("core", "workflow"),
        "cargo_test",
        ("pe6_workflow_timeout_retry_concurrency_and_restart",),
        90_000,
    ),
    ScenarioSpec(
        "pe6.provider.safety.v2",
        "provider",
        "engine::provider::executor + engine::provider::audit + engine::provider::cost_gate",
        "pe6.fault.provider.timeout.v2",
        "provider_timeout",
        "timeout",
        ("fake_provider", "rust_engine", "process"),
        "linux-sqlite",
        (("fake_provider", "fake_provider"), ("controlled_process", "process")),
        ("core", "provider"),
        "cargo_test",
        ("pe6_provider_timeout_retry_budget_audit_and_redaction",),
        90_000,
    ),
    ScenarioSpec(
        "pe6.release.provenance_rollback.v2",
        "release",
        "scripts/release_provenance.py + scripts/install-from-release.sh + scripts/upgrade.sh",
        "pe6.fault.release.provenance_rollback.v2",
        "release_activation_failure",
        "error",
        ("filesystem", "release", "process"),
        "linux-sqlite",
        (("temp_dir", "filesystem"), ("release_bundle", "release")),
        ("core", "release"),
        "python_test",
        ("tools.test_pe6_release_drill.ReleaseOwnerDrillTests.test_release_verification_precedes_activation_and_rolls_back",),
        60_000,
    ),
)

SCENARIOS_BY_ID: Mapping[str, ScenarioSpec] = {item.scenario_id: item for item in REGISTRY}
SUITES: Mapping[str, tuple[str, ...]] = {
    "core": tuple(item.scenario_id for item in REGISTRY if "core" in item.suites),
    "storage": tuple(item.scenario_id for item in REGISTRY if "storage" in item.suites),
    "workflow": tuple(item.scenario_id for item in REGISTRY if "workflow" in item.suites),
    "provider": tuple(item.scenario_id for item in REGISTRY if "provider" in item.suites),
    "release": tuple(item.scenario_id for item in REGISTRY if "release" in item.suites),
    "all": tuple(item.scenario_id for item in REGISTRY),
}
SUITE_NAMES = frozenset(SUITES)


def get_scenario(scenario_id: str) -> ScenarioSpec:
    try:
        return SCENARIOS_BY_ID[scenario_id]
    except KeyError as exc:
        raise ContractError("unknown scenario id") from exc


def scenario_ids_for(*, suite: str | None = None, scenario_id: str | None = None) -> tuple[str, ...]:
    if (suite is None) == (scenario_id is None):
        raise ContractError("select exactly one allowlisted suite or scenario")
    if suite is not None:
        try:
            return SUITES[suite]
        except KeyError as exc:
            raise ContractError("unknown suite") from exc
    if scenario_id not in SCENARIOS_BY_ID:
        raise ContractError("unknown scenario id")
    return (scenario_id,)  # type: ignore[arg-type]


def _resource_id(scenario_id: str, seed: int, worker_id: int, index: int) -> str:
    slug = scenario_id.replace(":", ".")
    return f"pe6.{slug}.s{seed:08x}.w{worker_id:08x}.r{index}"


def scenario_for(spec: ScenarioSpec, *, source_head: str, seed: int, worker_id: int) -> dict[str, object]:
    if any(capability not in CAPABILITIES for capability in spec.required_capabilities):
        raise ContractError("registry contains an unknown required capability")
    resources = [
        {
            "kind": kind,
            "resource_id": _resource_id(spec.scenario_id, seed, worker_id, index),
            "capability": capability,
            "disposable": True,
            "created_by": "pe6-harness",
        }
        for index, (kind, capability) in enumerate(spec.resource_kinds, start=1)
    ]
    scenario = {
        "schema_version": SCENARIO_SCHEMA_VERSION_V2,
        "scenario_id": spec.scenario_id,
        "scenario_version": "v2",
        "seed": seed,
        "worker_id": worker_id,
        "source_head": source_head,
        "subsystem": spec.subsystem,
        "owner": spec.owner,
        "environment": {
            "name": spec.environment,
            "capabilities": list(spec.required_capabilities),
        },
        "resources": resources,
        "fault": {
            "fault_id": spec.fault_id,
            "injection_point": spec.injection_point,
            "mode": spec.mode,
        },
        "invariants": {
            "normal": "existing owner remains the sole authoritative normal-state transition",
            "detection": "the registered fault is detected within the bounded timeout",
            "recovery": "the owner reaches a safe recovered or explicitly failed state",
            "rollback": "the previous known-good state remains recoverable on failure",
            "integrity": "no partial authority or unbound evidence is accepted",
            "audit": "state, fault, recovery, and cleanup are attributable without secrets",
            "restart_concurrency_idempotency": "restart, replay, and concurrent attempts do not duplicate authority",
            "abort": "timeout or unavailable capability aborts closed with an explicit result",
            "cleanup": "every harness-owned disposable resource is removed and verified",
        },
        "timeout_ms": spec.timeout_ms,
        "max_retries": 2,
        "max_processes": 2,
        "max_files": 128,
        "max_bytes": 2 * 1024 * 1024,
        "max_events": 128,
        "max_evidence_refs": 16,
    }
    return validate_scenario_v2(scenario)


def validate_registry() -> None:
    if len(REGISTRY) != len(SCENARIOS_BY_ID) or not REGISTRY:
        raise ContractError("registry scenario IDs must be unique and non-empty")
    for spec in REGISTRY:
        if not spec.command_args or spec.command_kind not in {"cargo_test", "cargo_pg_test", "python_test"}:
            raise ContractError("registry command is not a fixed supported owner command")
        if spec.timeout_ms < 1:
            raise ContractError("registry timeout must be positive")
        if not spec.suites or any(suite not in {"core", "storage", "workflow", "provider", "release"} for suite in spec.suites):
            raise ContractError("registry suite is not allowlisted")
        scenario_for(spec, source_head="a" * 40, seed=0, worker_id=0)


validate_registry()
