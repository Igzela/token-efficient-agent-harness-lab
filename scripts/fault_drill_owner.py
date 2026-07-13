#!/usr/bin/env python3
"""Owner-side helper for emitting canonical PE-6 v2 evidence in tests."""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any, Mapping

from scripts.fault_drill_contract import (
    OWNER_EVIDENCE_SCHEMA_VERSION_V2,
    ContractError,
    scenario_sha256_v2,
    read_json,
    validate_owner_evidence_v2,
    validate_scenario_v2,
    write_canonical_json,
)


def owner_evidence_environment() -> tuple[dict[str, Any], Path] | None:
    scenario_path = os.environ.get("ACP_PE6_SCENARIO_PATH")
    evidence_path = os.environ.get("ACP_PE6_EVIDENCE_PATH")
    if not scenario_path and not evidence_path:
        return None
    if not scenario_path or not evidence_path:
        raise ContractError("owner evidence paths must be supplied together")
    scenario_file = Path(scenario_path)
    output_file = Path(evidence_path)
    if not scenario_file.exists() and not output_file.parent.exists():
        return None
    scenario = read_json(scenario_file)
    if not isinstance(scenario, dict):
        raise ContractError("owner scenario must be an object")
    return validate_scenario_v2(scenario), output_file


def emit_owner_evidence(
    *,
    observed_state_before_fault: str,
    observed_fault: str,
    observed_recovery_or_refusal: str,
    checks: list[Mapping[str, str]],
    cleanup_outcome: str,
    cleanup_observation: str,
) -> None:
    environment = owner_evidence_environment()
    if environment is None:
        return
    scenario, output = environment
    evidence = {
        "schema_version": OWNER_EVIDENCE_SCHEMA_VERSION_V2,
        "scenario_id": scenario["scenario_id"],
        "scenario_version": scenario["scenario_version"],
        "scenario_sha256": scenario_sha256_v2(scenario),
        "source_head": scenario["source_head"],
        "fault": {
            "fault_id": scenario["fault"]["fault_id"],
            "injection_point": scenario["fault"]["injection_point"],
        },
        "owner": {
            "identity": scenario["owner"],
            "resource_ids": [resource["resource_id"] for resource in scenario["resources"]],
        },
        "observed_state_before_fault": observed_state_before_fault,
        "observed_fault": observed_fault,
        "observed_recovery_or_refusal": observed_recovery_or_refusal,
        "checks": [dict(check) for check in checks],
        "cleanup": {"outcome": cleanup_outcome, "observation": cleanup_observation},
    }
    validate_owner_evidence_v2(evidence, scenario)
    write_canonical_json(output, evidence)
