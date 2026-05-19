"""Governance Approval Path Enforcement schema and validation helpers.

Ensures policy candidates only reach active status after passing all gates:
evidence, approval, rollback, scope safety, and unknown_error checks.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple


# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

GOVERNANCE_DECISION_VERSION = "governance_decision.v1"


# ---------------------------------------------------------------------------
# Enums
# ---------------------------------------------------------------------------

DECISION_VALUES: Tuple[str, ...] = (
    "approve_activation", "reject_activation",
    "defer_activation", "require_more_evidence",
)

GATE_RESULTS: Tuple[str, ...] = ("pass", "fail")

ROLLBACK_SCOPES_HARNESS_LEVEL: frozenset[str] = frozenset({
    "docs_only", "config", "schema", "profile", "skill", "eval_gate", "runtime_guard",
})

USER_PROJECT_INDICATORS: Tuple[str, ...] = (
    "/src/", "/lib/", "/app/", "/home/", "/Users/", "/workspace/",
)


# ---------------------------------------------------------------------------
# Required fields
# ---------------------------------------------------------------------------

GOVERNANCE_DECISION_REQUIRED: Sequence[str] = (
    "schema_version", "decision_id", "candidate_id", "policy_id",
    "decision", "decision_basis", "gate_results", "blocked_reasons",
    "allowed_next_actions", "forbidden_next_actions", "decided_by", "decided_at",
)

DECISION_BASIS_REQUIRED: Sequence[str] = (
    "admitted_evidence_refs", "diagnostic_evidence_refs",
    "approval_ref", "rollback_plan_ref", "registry_entry_ref",
)

GATE_RESULTS_REQUIRED: Sequence[str] = (
    "evidence_gate", "approval_gate", "rollback_gate",
    "scope_gate", "unknown_error_gate",
)


# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------

def validate_governance_decision(data: Dict[str, Any]) -> List[str]:
    violations: List[str] = []
    for f in GOVERNANCE_DECISION_REQUIRED:
        if f not in data:
            violations.append(f"missing required field: {f}")
    if violations:
        return violations

    if data["schema_version"] != GOVERNANCE_DECISION_VERSION:
        violations.append(f"schema_version must be {GOVERNANCE_DECISION_VERSION}")
    if data["decision"] not in DECISION_VALUES:
        violations.append(f"decision {data['decision']!r} not in {DECISION_VALUES}")

    # Validate decision_basis
    basis = data.get("decision_basis", {})
    if not isinstance(basis, dict):
        violations.append("decision_basis must be a dict")
    else:
        for f in DECISION_BASIS_REQUIRED:
            if f not in basis:
                violations.append(f"decision_basis missing required field: {f}")

    # Validate gate_results
    gates = data.get("gate_results", {})
    if not isinstance(gates, dict):
        violations.append("gate_results must be a dict")
    else:
        for f in GATE_RESULTS_REQUIRED:
            if f not in gates:
                violations.append(f"gate_results missing required field: {f}")
            elif gates[f] not in GATE_RESULTS:
                violations.append(f"gate_results.{f} must be pass or fail, got {gates[f]!r}")

    # Validate lists
    if not isinstance(data.get("blocked_reasons", []), list):
        violations.append("blocked_reasons must be a list")
    if not isinstance(data.get("allowed_next_actions", []), list):
        violations.append("allowed_next_actions must be a list")
    if not isinstance(data.get("forbidden_next_actions", []), list):
        violations.append("forbidden_next_actions must be a list")

    return violations


# ---------------------------------------------------------------------------
# Gate evaluation helpers
# ---------------------------------------------------------------------------

def evaluate_evidence_gate(evidence_pack: Dict[str, Any]) -> Tuple[str, str]:
    """Evaluate if evidence pack passes the evidence gate.

    Returns (result, reason). pass requires admitted_evidence_refs non-empty.
    """
    admitted = evidence_pack.get("admitted_evidence_refs", [])
    if admitted:
        return "pass", f"evidence_gate passed: {len(admitted)} admitted evidence refs"
    return "fail", "evidence_gate failed: no admitted_evidence_refs (diagnostic-only evidence cannot drive adoption)"


def evaluate_approval_gate(approval_record: Optional[Dict[str, Any]]) -> Tuple[str, str]:
    """Evaluate if approval record passes the approval gate.

    Returns (result, reason). pass requires decision=approved.
    """
    if approval_record is None:
        return "fail", "approval_gate failed: no approval_record provided"
    decision = approval_record.get("decision", "")
    if decision == "approved":
        return "pass", "approval_gate passed: decision is approved"
    return "fail", f"approval_gate failed: decision is {decision!r} (must be approved)"


def evaluate_rollback_gate(rollback_plan: Optional[Dict[str, Any]]) -> Tuple[str, str]:
    """Evaluate if rollback plan passes the rollback gate.

    Returns (result, reason). pass requires status=approved and non-empty rollback_steps.
    """
    if rollback_plan is None:
        return "fail", "rollback_gate failed: no rollback_plan provided"
    status = rollback_plan.get("status", "")
    steps = rollback_plan.get("rollback_steps", [])
    if status != "approved":
        return "fail", f"rollback_gate failed: status is {status!r} (must be approved)"
    if not steps:
        return "fail", "rollback_gate failed: rollback_steps is empty"
    return "pass", "rollback_gate passed: status=approved with rollback_steps"


def evaluate_scope_gate(
    registry_entry: Dict[str, Any],
    rollback_plan: Optional[Dict[str, Any]],
) -> Tuple[str, str]:
    """Evaluate if scope is harness-level only.

    Returns (result, reason). pass requires no user project file paths.
    """
    if rollback_plan is None:
        return "fail", "scope_gate failed: no rollback_plan to check"

    impacted = rollback_plan.get("impacted_refs", [])
    for ref in impacted:
        if isinstance(ref, dict):
            path = ref.get("path_or_registry_key", "")
            for indicator in USER_PROJECT_INDICATORS:
                if indicator in path:
                    return "fail", f"scope_gate failed: impacted_ref {path!r} points to user project file"

    return "pass", "scope_gate passed: all impacted_refs are harness-level"


def evaluate_unknown_error_gate(evidence_pack: Dict[str, Any]) -> Tuple[str, str]:
    """Evaluate if unknown_error evidence requires human review.

    Returns (result, reason). If evidence contains unknown_error refs,
    requires human approval (approval_ref in evidence or human_review_refs).
    """
    diagnostic = evidence_pack.get("diagnostic_evidence_refs", [])
    human_refs = evidence_pack.get("human_review_refs", [])

    has_unknown = any("unknown_error" in str(ref) for ref in diagnostic)

    if has_unknown:
        if human_refs:
            return "pass", "unknown_error_gate passed: unknown_error evidence has human_review_refs"
        return "fail", "unknown_error_gate failed: unknown_error evidence requires human review"

    return "pass", "unknown_error_gate passed: no unknown_error evidence"


# ---------------------------------------------------------------------------
# Decision helpers
# ---------------------------------------------------------------------------

def decide_policy_activation(
    candidate: Dict[str, Any],
    evidence_pack: Dict[str, Any],
    approval_record: Optional[Dict[str, Any]],
    rollback_plan: Optional[Dict[str, Any]],
    registry_entry: Dict[str, Any],
) -> Dict[str, Any]:
    """Evaluate all gates and produce a governance_decision."""
    # Run all gates
    ev_result, ev_reason = evaluate_evidence_gate(evidence_pack)
    ap_result, ap_reason = evaluate_approval_gate(approval_record)
    rb_result, rb_reason = evaluate_rollback_gate(rollback_plan)
    sc_result, sc_reason = evaluate_scope_gate(registry_entry, rollback_plan)
    ue_result, ue_reason = evaluate_unknown_error_gate(evidence_pack)

    gates = {
        "evidence_gate": ev_result,
        "approval_gate": ap_result,
        "rollback_gate": rb_result,
        "scope_gate": sc_result,
        "unknown_error_gate": ue_result,
    }

    all_pass = all(v == "pass" for v in gates.values())
    blocked = []
    if ev_result == "fail":
        blocked.append(ev_reason)
    if ap_result == "fail":
        blocked.append(ap_reason)
    if rb_result == "fail":
        blocked.append(rb_reason)
    if sc_result == "fail":
        blocked.append(sc_reason)
    if ue_result == "fail":
        blocked.append(ue_reason)

    if all_pass:
        decision = "approve_activation"
        allowed = ["activate_policy"]
        forbidden = []
    elif any("unknown_error" in r for r in blocked):
        decision = "require_more_evidence"
        allowed = ["collect_human_review"]
        forbidden = ["activate_policy"]
    elif ev_result == "fail":
        decision = "require_more_evidence"
        allowed = ["collect_admitted_evidence"]
        forbidden = ["activate_policy"]
    else:
        decision = "reject_activation"
        allowed = ["revise_candidate"]
        forbidden = ["activate_policy"]

    return {
        "schema_version": GOVERNANCE_DECISION_VERSION,
        "decision_id": f"gov-{candidate.get('candidate_id', 'unknown')}",
        "candidate_id": candidate.get("candidate_id", ""),
        "policy_id": candidate.get("policy_id", registry_entry.get("policy_id", "")),
        "decision": decision,
        "decision_basis": {
            "admitted_evidence_refs": evidence_pack.get("admitted_evidence_refs", []),
            "diagnostic_evidence_refs": evidence_pack.get("diagnostic_evidence_refs", []),
            "approval_ref": approval_record.get("candidate_id", "") if approval_record else "",
            "rollback_plan_ref": rollback_plan.get("rollback_plan_id", "") if rollback_plan else "",
            "registry_entry_ref": registry_entry.get("policy_id", ""),
        },
        "gate_results": gates,
        "blocked_reasons": blocked,
        "allowed_next_actions": allowed,
        "forbidden_next_actions": forbidden,
        "decided_by": "governance_engine",
        "decided_at": "",
    }


def governance_allows_activation(decision: Dict[str, Any]) -> Tuple[bool, str]:
    """Check if a governance decision allows policy activation."""
    if decision.get("decision") == "approve_activation":
        return True, "governance decision is approve_activation"
    return False, f"governance decision is {decision.get('decision')!r}"


def governance_blocks_activation(decision: Dict[str, Any]) -> Tuple[bool, str]:
    """Check if a governance decision blocks policy activation."""
    if decision.get("decision") != "approve_activation":
        blocked = decision.get("blocked_reasons", [])
        return True, f"governance blocks activation: {'; '.join(blocked) if blocked else decision.get('decision')}"
    return False, "governance does not block activation"


def explain_blocked_reasons(decision: Dict[str, Any]) -> List[str]:
    """Return list of reasons why activation is blocked."""
    if decision.get("decision") == "approve_activation":
        return []
    return decision.get("blocked_reasons", [])


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
        violations = validate_governance_decision(data)
        results.append((p.name, data, violations))
    return results
