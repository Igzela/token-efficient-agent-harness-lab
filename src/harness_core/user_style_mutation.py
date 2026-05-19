"""User-style mutation evaluation helpers.

Provides schema validation, fixture loading, and admission grouping for
user-style mutation cases that exercise how the harness handles different
input expression styles (formal, chat, terse).
"""

from __future__ import annotations

import json
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence, Tuple


# ---------------------------------------------------------------------------
# Schema constants
# ---------------------------------------------------------------------------

SCHEMA_VERSION = "user_style_mutation.v1"

VARIANT_TYPES: Tuple[str, ...] = (
    "formal_issue",
    "user_style_chat_request",
    "terse_ticket",
)

ADMISSION_OUTCOMES: Tuple[str, ...] = (
    "admitted",
    "diagnostic",
    "needs_clarification",
    "rejected",
)

CONTAMINATION_RISKS: Tuple[str, ...] = (
    "low",
    "medium",
    "high",
    "unknown",
)

ADMISSION_SCOPES: Tuple[str, ...] = (
    "admitted",
    "diagnostic",
)

REQUIRED_MUTATION_FIELDS: Sequence[str] = (
    "case_id",
    "base_fixture_id",
    "variant_type",
    "user_prompt",
    "expected_task_family",
    "expected_required_fields",
    "expected_missing_fields",
    "admission_expectation",
    "evidence_refs",
    "fixture_metadata",
)

REQUIRED_METADATA_FIELDS: Sequence[str] = (
    "fixture_id",
    "source_type",
    "freshness",
    "estimated_human_minutes",
    "difficulty",
    "contamination_risk",
    "admission_scope",
)

SOURCE_TYPES: Tuple[str, ...] = (
    "synthetic",
    "copied_real_read_only",
    "mutated_user_style",
)


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------

@dataclass
class FixtureMetadata:
    """Metadata for a mutation fixture."""

    fixture_id: str
    source_type: str
    freshness: str
    estimated_human_minutes: float
    difficulty: str
    contamination_risk: str
    admission_scope: str

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)


@dataclass
class MutationCase:
    """A single user-style mutation case."""

    case_id: str
    base_fixture_id: str
    variant_type: str
    user_prompt: str
    expected_task_family: str
    expected_required_fields: List[str]
    expected_missing_fields: List[str]
    admission_expectation: str
    evidence_refs: List[str]
    fixture_metadata: FixtureMetadata
    schema_version: str = SCHEMA_VERSION

    def to_dict(self) -> Dict[str, Any]:
        d = asdict(self)
        return d

    def to_json(self, **kw: Any) -> str:
        return json.dumps(self.to_dict(), **kw)


# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------

def validate_fixture_metadata(data: Dict[str, Any]) -> List[str]:
    """Validate a fixture_metadata dict. Returns list of violations."""
    violations: List[str] = []
    for f in REQUIRED_METADATA_FIELDS:
        if f not in data:
            violations.append(f"fixture_metadata missing required field: {f}")
    if violations:
        return violations

    if data["source_type"] not in SOURCE_TYPES:
        violations.append(
            f"source_type {data['source_type']!r} not in {SOURCE_TYPES}"
        )
    if data["contamination_risk"] not in CONTAMINATION_RISKS:
        violations.append(
            f"contamination_risk {data['contamination_risk']!r} not in {CONTAMINATION_RISKS}"
        )
    if data["admission_scope"] not in ADMISSION_SCOPES:
        violations.append(
            f"admission_scope {data['admission_scope']!r} not in {ADMISSION_SCOPES}"
        )
    if not isinstance(data["estimated_human_minutes"], (int, float)):
        violations.append("estimated_human_minutes must be numeric")
    return violations


def validate_mutation_case(data: Dict[str, Any]) -> List[str]:
    """Validate a mutation case dict against the schema. Returns violations."""
    violations: List[str] = []

    for f in REQUIRED_MUTATION_FIELDS:
        if f not in data:
            violations.append(f"missing required field: {f}")
    if violations:
        return violations

    # schema_version
    if data["schema_version"] != SCHEMA_VERSION:
        violations.append(
            f"schema_version must be {SCHEMA_VERSION}, got {data['schema_version']!r}"
        )

    # variant_type
    if data["variant_type"] not in VARIANT_TYPES:
        violations.append(
            f"variant_type {data['variant_type']!r} not in {VARIANT_TYPES}"
        )

    # admission_expectation
    if data["admission_expectation"] not in ADMISSION_OUTCOMES:
        violations.append(
            f"admission_expectation {data['admission_expectation']!r} not in {ADMISSION_OUTCOMES}"
        )

    # evidence_refs must be a list
    if not isinstance(data["evidence_refs"], list):
        violations.append("evidence_refs must be a list")

    # expected_required_fields must be a list
    if not isinstance(data["expected_required_fields"], list):
        violations.append("expected_required_fields must be a list")

    # expected_missing_fields must be a list
    if not isinstance(data["expected_missing_fields"], list):
        violations.append("expected_missing_fields must be a list")

    # fixture_metadata sub-validation
    if isinstance(data.get("fixture_metadata"), dict):
        violations.extend(validate_fixture_metadata(data["fixture_metadata"]))
    else:
        violations.append("fixture_metadata must be a dict")

    return violations


def create_mutation_case(
    *,
    case_id: str,
    base_fixture_id: str,
    variant_type: str,
    user_prompt: str,
    expected_task_family: str,
    expected_required_fields: Optional[List[str]] = None,
    expected_missing_fields: Optional[List[str]] = None,
    admission_expectation: str,
    evidence_refs: Optional[List[str]] = None,
    fixture_metadata: Optional[Dict[str, Any]] = None,
) -> MutationCase:
    """Factory for MutationCase with defaults."""
    meta = fixture_metadata or {}
    return MutationCase(
        case_id=case_id,
        base_fixture_id=base_fixture_id,
        variant_type=variant_type,
        user_prompt=user_prompt,
        expected_task_family=expected_task_family,
        expected_required_fields=expected_required_fields or [],
        expected_missing_fields=expected_missing_fields or [],
        admission_expectation=admission_expectation,
        evidence_refs=evidence_refs or [],
        fixture_metadata=FixtureMetadata(
            fixture_id=meta.get("fixture_id", case_id),
            source_type=meta.get("source_type", "mutated_user_style"),
            freshness=meta.get("freshness", "2026-05-19"),
            estimated_human_minutes=meta.get("estimated_human_minutes", 0.0),
            difficulty=meta.get("difficulty", "unknown"),
            contamination_risk=meta.get("contamination_risk", "low"),
            admission_scope=meta.get("admission_scope", "admitted"),
        ),
    )


# ---------------------------------------------------------------------------
# Fixture loading
# ---------------------------------------------------------------------------

def load_fixture(path: Path) -> Dict[str, Any]:
    """Load a single JSON fixture file."""
    with open(path, "r") as f:
        return json.load(f)


def load_and_validate_fixture(path: Path) -> Tuple[Dict[str, Any], List[str]]:
    """Load a mutation fixture and return (data, violations)."""
    data = load_fixture(path)
    return data, validate_mutation_case(data)


def load_all_fixtures(fixture_dir: Path) -> List[Tuple[str, Dict[str, Any], List[str]]]:
    """Load and validate every .json fixture in a directory.

    Returns list of (filename, data, violations) tuples.
    """
    results = []
    for p in sorted(fixture_dir.glob("*.json")):
        data = load_fixture(p)
        violations = validate_mutation_case(data)
        results.append((p.name, data, violations))
    return results


# ---------------------------------------------------------------------------
# Admission grouping
# ---------------------------------------------------------------------------

def group_by_admission(
    fixtures: List[Tuple[str, Dict[str, Any], List[str]]],
) -> Dict[str, List[Dict[str, Any]]]:
    """Group valid fixtures by their admission_expectation.

    Only includes fixtures with no validation violations.
    """
    groups: Dict[str, List[Dict[str, Any]]] = {
        outcome: [] for outcome in ADMISSION_OUTCOMES
    }
    for _filename, data, violations in fixtures:
        if not violations:
            outcome = data["admission_expectation"]
            groups[outcome].append(data)
    return groups


def group_by_variant(
    fixtures: List[Tuple[str, Dict[str, Any], List[str]]],
) -> Dict[str, List[Dict[str, Any]]]:
    """Group valid fixtures by variant_type."""
    groups: Dict[str, List[Dict[str, Any]]] = {
        vt: [] for vt in VARIANT_TYPES
    }
    for _filename, data, violations in fixtures:
        if not violations:
            vt = data["variant_type"]
            groups[vt].append(data)
    return groups


def group_by_base_fixture(
    fixtures: List[Tuple[str, Dict[str, Any], List[str]]],
) -> Dict[str, List[Dict[str, Any]]]:
    """Group valid fixtures by base_fixture_id."""
    groups: Dict[str, List[Dict[str, Any]]] = {}
    for _filename, data, violations in fixtures:
        if not violations:
            base = data["base_fixture_id"]
            groups.setdefault(base, []).append(data)
    return groups
