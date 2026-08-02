"""Strict versioned receipt parsing (pure logic).

Only an exact structured PASS with matching identities and no blockers or
objections is acceptable.  Natural-language "PASS" without the JSON receipt is
unusable, and PASS_WITH_NOTES / NEEDS_CHANGES / UNREVIEWABLE are rejected.
"""

from __future__ import annotations

import json
import re
from typing import Any

from . import models

_JSON_BLOCK_RE = re.compile(
    r"```(?:json)?\s*\n(?P<body>\{.*?\})\s*\n```", re.DOTALL
)


def _coerce_string_list(value: Any, field: str) -> tuple[list[str], str | None]:
    if not isinstance(value, list):
        return [], f"{field} is not a list"
    items: list[str] = []
    for item in value:
        if not isinstance(item, str):
            return [], f"{field} contains a non-string"
        items.append(item)
    return items, None


def parse_receipt(markdown: str) -> tuple[models.ReviewReceipt | None, list[str]]:
    """Parse a structured receipt from reviewer markdown.

    Returns (receipt, errors).  receipt is None unless a valid, complete,
    schema-matching JSON block exists and validates.
    """
    errors: list[str] = []
    blocks = _JSON_BLOCK_RE.findall(markdown or "")
    if not blocks:
        return None, ["no JSON receipt block found in reviewer output"]

    candidates = []
    for body in blocks:
        try:
            data = json.loads(body)
        except Exception as exc:
            errors.append(f"JSON block unparseable: {exc}")
            continue
        if not isinstance(data, dict):
            errors.append("JSON block is not an object")
            continue
        if data.get("schema_version") != models.RECEIPT_SCHEMA:
            errors.append(
                f"receipt schema_version mismatch: {data.get('schema_version')!r}"
            )
            continue
        candidates.append(data)

    if not candidates:
        return None, errors or ["no receipt block matched the schema"]

    # B6: exactly one schema-matching receipt object is required; multiple
    # receipts (even identical) fail closed.
    if len(candidates) != 1:
        return None, [f"expected exactly one receipt object, found {len(candidates)}"]

    data = candidates[0]
    blockers, err = _coerce_string_list(data.get("blockers"), "blockers")
    if err:
        return None, [err]
    objections, err = _coerce_string_list(
        data.get("unresolved_objections"), "unresolved_objections"
    )
    if err:
        return None, [err]
    receipt = models.ReviewReceipt(
        schema_version=str(data.get("schema_version", "")),
        verdict=str(data.get("verdict", "")),
        repository=str(data.get("repository", "")),
        pr_number=int(data.get("pr_number", 0)),
        base_sha=str(data.get("base_sha", "")),
        head_sha=str(data.get("head_sha", "")),
        diff_scope=str(data.get("diff_scope", "")),
        blockers=tuple(blockers),
        unresolved_objections=tuple(objections),
        reviewer_session_id=str(data.get("reviewer_session_id", "")),
        implementation_session_id=str(data.get("implementation_session_id", "")),
        transport=str(data.get("transport", "")),
    )
    validation = receipt.validate()
    if validation:
        return None, validation
    return receipt, []
