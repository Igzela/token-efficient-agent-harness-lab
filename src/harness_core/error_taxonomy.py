"""Tool/Error Taxonomy schema and validation helpers.

Defines canonical error domains, the error_record schema, and validation
rules for classifying tool execution failures in the harness pipeline.
"""

from __future__ import annotations

import json
import uuid
from dataclasses import dataclass, field, asdict
from datetime import datetime, timezone
from enum import Enum
from pathlib import Path
from typing import Any, Dict, List, Optional, Sequence


# ---------------------------------------------------------------------------
# Canonical error domains
# ---------------------------------------------------------------------------

class ErrorDomain(str, Enum):
    """All recognised error domains in the taxonomy."""

    TOOL_CONTRACT_ERROR = "tool_contract_error"
    ENVIRONMENT_ERROR = "environment_error"
    CONTEXT_ERROR = "context_error"
    MODEL_JUDGMENT_ERROR = "model_judgment_error"
    EVALUATION_ERROR = "evaluation_error"
    HARNESS_BUG = "harness_bug"
    USER_ABORT = "user_abort"
    PROVIDER_ERROR = "provider_error"
    TIMEOUT = "timeout"
    UNKNOWN_ERROR = "unknown_error"


CANONICAL_DOMAINS: List[str] = [d.value for d in ErrorDomain]

# Domains that are never retryable
NON_RETRYABLE_DOMAINS: frozenset[str] = frozenset({
    ErrorDomain.USER_ABORT.value,
    ErrorDomain.HARNESS_BUG.value,
    ErrorDomain.UNKNOWN_ERROR.value,
})

# Domains where requires_human_triage is always True
MANDATORY_TRIAGE_DOMAINS: frozenset[str] = frozenset({
    ErrorDomain.UNKNOWN_ERROR.value,
    ErrorDomain.HARNESS_BUG.value,
})

# Domains that must never drive policy candidate adoption
NON_ADOPTABLE_DOMAINS: frozenset[str] = frozenset({
    ErrorDomain.UNKNOWN_ERROR.value,
})


# ---------------------------------------------------------------------------
# Schema version
# ---------------------------------------------------------------------------

SCHEMA_VERSION = "error_record.v1"

REQUIRED_FIELDS: Sequence[str] = (
    "schema_version",
    "error_id",
    "error_domain",
    "error_class",
    "retryable",
    "counts_against_model",
    "requires_human_triage",
    "tool_name",
    "model_profile_id",
    "context_pack_id",
    "event_id",
    "evidence_refs",
    "created_at",
)


# ---------------------------------------------------------------------------
# Data class
# ---------------------------------------------------------------------------

@dataclass
class ErrorRecord:
    """Typed representation of a single error_record."""

    error_id: str
    error_domain: str
    error_class: str
    retryable: bool
    counts_against_model: bool
    requires_human_triage: bool
    tool_name: str
    model_profile_id: str
    context_pack_id: str
    event_id: str
    evidence_refs: List[str]
    created_at: str
    schema_version: str = field(default=SCHEMA_VERSION)

    def to_dict(self) -> Dict[str, Any]:
        return asdict(self)

    def to_json(self, **kw: Any) -> str:
        return json.dumps(self.to_dict(), **kw)


# ---------------------------------------------------------------------------
# Validation helpers
# ---------------------------------------------------------------------------

def validate_error_record(data: Dict[str, Any]) -> List[str]:
    """Validate a dict against the error_record schema.

    Returns a list of violation strings.  Empty list means valid.
    """
    violations: List[str] = []

    # 1. Check all required fields are present
    for f in REQUIRED_FIELDS:
        if f not in data:
            violations.append(f"missing required field: {f}")

    if violations:
        return violations  # can't continue without required fields

    # 2. schema_version must match
    if data["schema_version"] != SCHEMA_VERSION:
        violations.append(
            f"schema_version must be {SCHEMA_VERSION}, got {data['schema_version']!r}"
        )

    # 3. error_domain must be canonical
    if data["error_domain"] not in CANONICAL_DOMAINS:
        violations.append(
            f"error_domain {data['error_domain']!r} not in canonical domains"
        )

    # 4. error_id must be a non-empty string
    if not isinstance(data["error_id"], str) or not data["error_id"]:
        violations.append("error_id must be a non-empty string")

    # 5. Boolean fields must be bool
    for bf in ("retryable", "counts_against_model", "requires_human_triage"):
        if not isinstance(data[bf], bool):
            violations.append(f"{bf} must be a bool, got {type(data[bf]).__name__}")

    # 6. evidence_refs must be a list
    if not isinstance(data["evidence_refs"], list):
        violations.append("evidence_refs must be a list")

    # 7. Domain-specific constraints
    domain = data["error_domain"]
    violations.extend(_validate_domain_constraints(data, domain))

    return violations


def _validate_domain_constraints(
    data: Dict[str, Any], domain: str
) -> List[str]:
    """Enforce domain-specific semantic rules."""
    violations: List[str] = []

    # unknown_error must always be non-retryable and require triage
    if domain == ErrorDomain.UNKNOWN_ERROR.value:
        if data["retryable"] is not False:
            violations.append(
                "unknown_error must have retryable=false (fail-hard)"
            )
        if data["requires_human_triage"] is not True:
            violations.append(
                "unknown_error must have requires_human_triage=true"
            )

    # user_abort is not a system failure — cannot be retryable
    if domain == ErrorDomain.USER_ABORT.value:
        if data["retryable"] is not False:
            violations.append("user_abort must have retryable=false")

    # harness_bug is not retryable and must require triage
    if domain == ErrorDomain.HARNESS_BUG.value:
        if data["retryable"] is not False:
            violations.append("harness_bug must have retryable=false")
        if data["requires_human_triage"] is not True:
            violations.append("harness_bug must have requires_human_triage=true")

    # provider_error and timeout must both be retryable by default
    if domain in (ErrorDomain.PROVIDER_ERROR.value, ErrorDomain.TIMEOUT.value):
        if data["retryable"] is not True:
            violations.append(
                f"{domain} should typically be retryable=true"
            )

    return violations


def create_error_record(
    *,
    error_domain: str,
    error_class: str,
    retryable: bool,
    counts_against_model: bool,
    requires_human_triage: bool,
    tool_name: str = "",
    model_profile_id: str = "",
    context_pack_id: str = "",
    event_id: str = "",
    evidence_refs: Optional[List[str]] = None,
    created_at: Optional[str] = None,
    error_id: Optional[str] = None,
) -> ErrorRecord:
    """Factory for ErrorRecord with auto-generated defaults."""
    return ErrorRecord(
        error_id=error_id or str(uuid.uuid4()),
        error_domain=error_domain,
        error_class=error_class,
        retryable=retryable,
        counts_against_model=counts_against_model,
        requires_human_triage=requires_human_triage,
        tool_name=tool_name,
        model_profile_id=model_profile_id,
        context_pack_id=context_pack_id,
        event_id=event_id,
        evidence_refs=evidence_refs or [],
        created_at=created_at or datetime.now(timezone.utc).isoformat(),
    )


def load_fixture(path: Path) -> Dict[str, Any]:
    """Load a single JSON fixture file and return the dict."""
    with open(path, "r") as f:
        return json.load(f)


def load_and_validate_fixture(path: Path) -> tuple[Dict[str, Any], List[str]]:
    """Load a fixture and return (data, violations)."""
    data = load_fixture(path)
    return data, validate_error_record(data)


def load_all_fixtures(fixture_dir: Path) -> List[tuple[str, Dict[str, Any], List[str]]]:
    """Load every .json fixture in a directory.

    Returns list of (filename, data, violations) tuples.
    """
    results = []
    for p in sorted(fixture_dir.glob("*.json")):
        data = load_fixture(p)
        violations = validate_error_record(data)
        results.append((p.name, data, violations))
    return results


def is_adoptable(domain: str) -> bool:
    """Return True if errors in this domain may drive policy candidate adoption."""
    return domain not in NON_ADOPTABLE_DOMAINS


def is_retryable(domain: str) -> bool:
    """Return True if errors in this domain are expected to be retryable."""
    return domain not in NON_RETRYABLE_DOMAINS


def requires_triage(domain: str) -> bool:
    """Return True if errors in this domain always require human triage."""
    return domain in MANDATORY_TRIAGE_DOMAINS
