"""Policy Candidate Lifecycle schema and validation helpers.

Defines the full lifecycle from candidate proposal through evidence collection,
approval, rollback planning, and policy registry activation.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple


# ---------------------------------------------------------------------------
# Schema versions
# ---------------------------------------------------------------------------

CANDIDATE_MANIFEST_VERSION = "policy_candidate.v1"
EVIDENCE_PACK_VERSION = "candidate_evidence.v1"
APPROVAL_RECORD_VERSION = "approval_record.v1"
ROLLBACK_PLAN_VERSION = "rollback_plan.v1"
POLICY_REGISTRY_VERSION = "policy_registry.v1"


# ---------------------------------------------------------------------------
# Enums
# ---------------------------------------------------------------------------

CANDIDATE_TYPES: Tuple[str, ...] = (
    "context_pack", "tool_contract", "routing_rule", "skill_package",
    "eval_gate", "error_taxonomy", "model_profile",
)

EVIDENCE_RECOMMENDATION: Tuple[str, ...] = (
    "accept", "reject", "revise", "needs_more_evidence",
)

APPROVAL_DECISIONS: Tuple[str, ...] = ("approved", "rejected", "deferred")

ROLLBACK_SCOPES: Tuple[str, ...] = (
    "docs_only", "config", "schema", "profile", "skill", "eval_gate", "runtime_guard",
)

ROLLBACK_TRIGGER_CONDITIONS: Tuple[str, ...] = (
    "regression_detected", "cost_threshold_exceeded", "quality_gate_failure",
    "human_rejection", "unknown_error_increase",
)

IMPACTED_REF_TYPES: Tuple[str, ...] = (
    "file", "registry_entry", "config_key", "schema_id", "skill_id", "model_profile_id",
)

ROLLBACK_ACTIONS: Tuple[str, ...] = (
    "revert_file", "restore_registry_entry", "disable_policy",
    "restore_schema", "retire_skill", "restore_profile",
)

REGISTRY_STATUSES: Tuple[str, ...] = (
    "proposed", "approved", "active", "rolled_back", "retired",
)

ROLLBACK_STATUSES: Tuple[str, ...] = (
    "proposed", "approved", "executed", "failed", "obsolete",
)


# ---------------------------------------------------------------------------
# Required fields
# ---------------------------------------------------------------------------

MANIFEST_REQUIRED: Sequence[str] = (
    "schema_version", "candidate_id", "candidate_type", "title", "rationale",
    "source_refs", "proposed_change_summary", "affected_components",
    "expected_benefit", "expected_risk", "required_evidence",
    "evaluation_plan", "rollback_plan_ref", "approval_required", "created_at",
)

EVIDENCE_PACK_REQUIRED: Sequence[str] = (
    "schema_version", "candidate_id", "admitted_evidence_refs",
    "diagnostic_evidence_refs", "fixture_results", "quality_deltas",
    "cost_deltas", "failure_cluster_refs", "human_review_refs",
    "recommendation",
)

APPROVAL_REQUIRED: Sequence[str] = (
    "schema_version", "candidate_id", "approver", "decision", "rationale",
    "required_followups", "deployment_scope", "rollback_required", "approved_at",
)

ROLLBACK_PLAN_REQUIRED: Sequence[str] = (
    "schema_version", "rollback_plan_id", "candidate_id", "policy_id",
    "rollback_scope", "trigger_conditions", "impacted_refs", "rollback_steps",
    "validation_steps", "rollback_owner", "max_rollback_time", "fallback_policy",
    "status", "created_at",
)

REGISTRY_REQUIRED: Sequence[str] = (
    "schema_version", "policy_id", "candidate_id", "policy_type", "status",
    "active_scope", "version", "evidence_pack_ref", "approval_ref",
    "rollback_plan_ref", "activated_at", "retired_at",
)


# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------

def validate_policy_candidate_manifest(data: Dict[str, Any]) -> List[str]:
    violations: List[str] = []
    for f in MANIFEST_REQUIRED:
        if f not in data:
            violations.append(f"missing required field: {f}")
    if violations:
        return violations

    if data["schema_version"] != CANDIDATE_MANIFEST_VERSION:
        violations.append(f"schema_version must be {CANDIDATE_MANIFEST_VERSION}")
    if data["candidate_type"] not in CANDIDATE_TYPES:
        violations.append(f"candidate_type {data['candidate_type']!r} not in {CANDIDATE_TYPES}")
    if data["approval_required"] is not True:
        violations.append(f"approval_required must be true, got {data['approval_required']!r}")
    if not isinstance(data["source_refs"], list):
        violations.append("source_refs must be a list")
    return violations


def validate_candidate_evidence_pack(data: Dict[str, Any]) -> List[str]:
    violations: List[str] = []
    for f in EVIDENCE_PACK_REQUIRED:
        if f not in data:
            violations.append(f"missing required field: {f}")
    if violations:
        return violations

    if data["schema_version"] != EVIDENCE_PACK_VERSION:
        violations.append(f"schema_version must be {EVIDENCE_PACK_VERSION}")
    if data["recommendation"] not in EVIDENCE_RECOMMENDATION:
        violations.append(f"recommendation {data['recommendation']!r} not in {EVIDENCE_RECOMMENDATION}")
    if not isinstance(data["admitted_evidence_refs"], list):
        violations.append("admitted_evidence_refs must be a list")
    if not isinstance(data["diagnostic_evidence_refs"], list):
        violations.append("diagnostic_evidence_refs must be a list")
    return violations


def validate_approval_record(data: Dict[str, Any]) -> List[str]:
    violations: List[str] = []
    for f in APPROVAL_REQUIRED:
        if f not in data:
            violations.append(f"missing required field: {f}")
    if violations:
        return violations

    if data["schema_version"] != APPROVAL_RECORD_VERSION:
        violations.append(f"schema_version must be {APPROVAL_RECORD_VERSION}")
    if data["decision"] not in APPROVAL_DECISIONS:
        violations.append(f"decision {data['decision']!r} not in {APPROVAL_DECISIONS}")
    if data["rollback_required"] is not True:
        violations.append(f"rollback_required must be true, got {data['rollback_required']!r}")
    return violations


def validate_rollback_plan(data: Dict[str, Any]) -> List[str]:
    violations: List[str] = []
    for f in ROLLBACK_PLAN_REQUIRED:
        if f not in data:
            violations.append(f"missing required field: {f}")
    if violations:
        return violations

    if data["schema_version"] != ROLLBACK_PLAN_VERSION:
        violations.append(f"schema_version must be {ROLLBACK_PLAN_VERSION}")
    if data["rollback_scope"] not in ROLLBACK_SCOPES:
        violations.append(f"rollback_scope {data['rollback_scope']!r} not in {ROLLBACK_SCOPES}")
    if data["status"] not in ROLLBACK_STATUSES:
        violations.append(f"status {data['status']!r} not in {ROLLBACK_STATUSES}")
    if not isinstance(data["trigger_conditions"], list):
        violations.append("trigger_conditions must be a list")
    if not isinstance(data["rollback_steps"], list):
        violations.append("rollback_steps must be a list")
    elif len(data["rollback_steps"]) == 0:
        violations.append("rollback_steps must not be empty")
    if not isinstance(data["validation_steps"], list):
        violations.append("validation_steps must be a list")

    # Check rollback_steps have required fields
    for i, step in enumerate(data.get("rollback_steps", [])):
        if not isinstance(step, dict):
            violations.append(f"rollback_steps[{i}] must be a dict")
            continue
        if "step_id" not in step:
            violations.append(f"rollback_steps[{i}] missing step_id")
        if "action" not in step:
            violations.append(f"rollback_steps[{i}] missing action")
        elif step["action"] not in ROLLBACK_ACTIONS:
            violations.append(f"rollback_steps[{i}].action {step['action']!r} not in {ROLLBACK_ACTIONS}")

    # Check impacted_refs for user project paths
    for i, ref in enumerate(data.get("impacted_refs", [])):
        if isinstance(ref, dict):
            path = ref.get("path_or_registry_key", "")
            if _is_user_project_path(path):
                violations.append(
                    f"impacted_refs[{i}] points to user project path {path!r}; "
                    "rollback targets must be harness-level only"
                )

    return violations


def validate_policy_registry_entry(data: Dict[str, Any]) -> List[str]:
    violations: List[str] = []
    for f in REGISTRY_REQUIRED:
        if f not in data:
            violations.append(f"missing required field: {f}")
    if violations:
        return violations

    if data["schema_version"] != POLICY_REGISTRY_VERSION:
        violations.append(f"schema_version must be {POLICY_REGISTRY_VERSION}")
    if data["status"] not in REGISTRY_STATUSES:
        violations.append(f"status {data['status']!r} not in {REGISTRY_STATUSES}")

    # Active policy must have approval_ref and rollback_plan_ref
    if data["status"] == "active":
        if not data.get("approval_ref"):
            violations.append("active policy must have approval_ref")
        if not data.get("rollback_plan_ref"):
            violations.append("active policy must have rollback_plan_ref")

    return violations


def _is_user_project_path(path: str) -> bool:
    """Check if a path looks like a real user project file (not harness-level)."""
    user_indicators = ("/src/", "/lib/", "/app/", "/home/", "/Users/", "/workspace/")
    return any(ind in path for ind in user_indicators)


# ---------------------------------------------------------------------------
# Lifecycle helpers
# ---------------------------------------------------------------------------

def candidate_has_required_evidence(manifest: Dict[str, Any], evidence: Dict[str, Any]) -> Tuple[bool, str]:
    """Check if a candidate has the required evidence for adoption."""
    if not evidence.get("admitted_evidence_refs"):
        return False, "no admitted_evidence_refs; adoption requires at least one admitted evidence"
    return True, "candidate has admitted evidence"


def evidence_pack_is_adoptable(evidence: Dict[str, Any]) -> Tuple[bool, str]:
    """Check if an evidence pack supports adoption."""
    if not evidence.get("admitted_evidence_refs"):
        if evidence.get("recommendation") == "accept":
            return False, "diagnostic-only evidence pack cannot recommend accept"
        return False, "no admitted evidence for adoption"
    if evidence.get("recommendation") not in ("accept",):
        return False, f"recommendation is {evidence.get('recommendation')!r}, not accept"
    return True, "evidence pack is adoptable"


def approval_allows_activation(approval: Dict[str, Any]) -> Tuple[bool, str]:
    """Check if an approval record allows policy activation."""
    if approval.get("decision") != "approved":
        return False, f"decision is {approval.get('decision')!r}, not approved"
    return True, "approval allows activation"


def rollback_plan_is_ready(plan: Dict[str, Any]) -> Tuple[bool, str]:
    """Check if a rollback plan is ready for use."""
    if not plan.get("rollback_steps"):
        return False, "rollback_plan has no rollback_steps"
    if plan.get("status") in ("failed", "obsolete"):
        return False, f"rollback_plan status is {plan.get('status')!r}"
    return True, "rollback plan is ready"


def can_activate_policy(
    registry: Dict[str, Any],
    approval: Optional[Dict[str, Any]] = None,
    rollback: Optional[Dict[str, Any]] = None,
    evidence: Optional[Dict[str, Any]] = None,
) -> Tuple[bool, str]:
    """Check if a policy can be activated."""
    if registry.get("status") == "active":
        return True, "already active"

    if not registry.get("approval_ref") and approval:
        if approval.get("decision") != "approved":
            return False, "approval is not approved"

    if not registry.get("rollback_plan_ref") and rollback:
        ok, reason = rollback_plan_is_ready(rollback)
        if not ok:
            return False, f"rollback_plan not ready: {reason}"

    if evidence:
        ok, reason = evidence_pack_is_adoptable(evidence)
        if not ok:
            return False, f"evidence not adoptable: {reason}"

    return True, "policy can be activated"


def should_reject_diagnostic_only_candidate(evidence: Dict[str, Any]) -> Tuple[bool, str]:
    """Check if a candidate should be rejected because it only has diagnostic evidence."""
    if evidence.get("admitted_evidence_refs"):
        return False, "candidate has admitted evidence"
    if evidence.get("diagnostic_evidence_refs"):
        return True, "candidate only has diagnostic evidence; cannot be adopted"
    return False, "no evidence at all"


def create_policy_registry_entry(
    *,
    policy_id: str,
    candidate_id: str,
    policy_type: str,
    active_scope: str = "",
    version: str = "1.0",
    evidence_pack_ref: str = "",
    approval_ref: str = "",
    rollback_plan_ref: str = "",
) -> Dict[str, Any]:
    """Create a new policy registry entry with status=proposed."""
    return {
        "schema_version": POLICY_REGISTRY_VERSION,
        "policy_id": policy_id,
        "candidate_id": candidate_id,
        "policy_type": policy_type,
        "status": "proposed",
        "active_scope": active_scope,
        "version": version,
        "evidence_pack_ref": evidence_pack_ref,
        "approval_ref": approval_ref,
        "rollback_plan_ref": rollback_plan_ref,
        "activated_at": "",
        "retired_at": "",
    }


# ---------------------------------------------------------------------------
# Fixture loading
# ---------------------------------------------------------------------------

def load_fixture(path: Path) -> Dict[str, Any]:
    with open(path, "r") as f:
        return json.load(f)


def load_all_fixtures(fixture_dir: Path) -> List[Tuple[str, Dict[str, Any], List[str]]]:
    results = []
    for p in sorted(fixture_dir.glob("*.json")):
        data = load_fixture(p)
        sv = data.get("schema_version", "")
        if sv == CANDIDATE_MANIFEST_VERSION:
            violations = validate_policy_candidate_manifest(data)
        elif sv == EVIDENCE_PACK_VERSION:
            violations = validate_candidate_evidence_pack(data)
        elif sv == APPROVAL_RECORD_VERSION:
            violations = validate_approval_record(data)
        elif sv == ROLLBACK_PLAN_VERSION:
            violations = validate_rollback_plan(data)
        elif sv == POLICY_REGISTRY_VERSION:
            violations = validate_policy_registry_entry(data)
        else:
            violations = [f"unknown schema_version: {sv!r}"]
        results.append((p.name, data, violations))
    return results
