"""Official Codex hook wire-schema extraction and validation.

Extracts the authoritative ``*.command.output`` JSON schemas embedded in the
installed Codex CLI binary and validates hook dispatcher outputs against them
with ``jsonschema``. This replaces home-grown serializer assertions with the
runtime's own contract: an output that the real Codex engine would reject is
reported as a violation here first.

Used by tests only; production hook handlers must already emit wire-compliant
payloads. All functions fail closed with explicit skip signals when the Codex
binary is unavailable (e.g. CI runners without Codex installed).
"""

from __future__ import annotations

import json
import re
from pathlib import Path
import shutil
from typing import Any

EVENT_TO_SCHEMA_ID = {
    "SessionStart": "session-start.command.output",
    "PreCompact": "pre-compact.command.output",
    "PostCompact": "post-compact.command.output",
    "PreToolUse": "pre-tool-use.command.output",
    "PostToolUse": "post-tool-use.command.output",
    "PermissionRequest": "permission-request.command.output",
    "Stop": "stop.command.output",
}

_SCHEMA_CACHE: dict[str, dict[str, Any]] = {}


def extract_official_output_schemas(codex_binary: str | Path) -> dict[str, dict[str, Any]]:
    """Extract official ``*.command.output`` schemas from the Codex binary.

    Raises FileNotFoundError when the binary is absent and ValueError when no
    schemas can be located inside it.
    """
    bin_path = Path(codex_binary)
    if not bin_path.is_file():
        resolved = shutil.which(str(codex_binary)) or shutil.which("codex")
        if not resolved or not Path(resolved).is_file():
            raise FileNotFoundError(f"codex_binary_unavailable: {codex_binary}")
        bin_path = Path(resolved)

    cache_key = str(bin_path.resolve())
    if cache_key in _SCHEMA_CACHE:
        return _SCHEMA_CACHE[cache_key]

    data = bin_path.read_bytes()
    text = data.decode("utf-8", errors="replace")
    decoder = json.JSONDecoder()
    found: dict[str, dict[str, Any]] = {}
    # Schemas are embedded pretty-printed; locate each object start.
    for match in re.finditer(r'\{\s*"\$schema"', text):
        try:
            obj, _end = decoder.raw_decode(text, match.start())
        except json.JSONDecodeError:
            continue
        if not isinstance(obj, dict):
            continue
        title = obj.get("title", "")
        if isinstance(title, str) and title.endswith(".command.output"):
            found[title] = obj

    if not found:
        raise ValueError(f"no_official_hook_schemas_found_in: {bin_path}")
    _SCHEMA_CACHE[cache_key] = found
    return found


def validate_hook_output(
    event_name: str,
    output: dict[str, Any],
    codex_binary: str | Path,
) -> list[str]:
    """Validate a hook output dict against the official schema for the event.

    Returns a list of violation descriptions (empty when compliant).
    Raises FileNotFoundError/ValueError per :func:`extract_official_output_schemas`
    when the binary or its schemas are unavailable.
    """
    try:
        import jsonschema
    except ImportError as exc:
        raise RuntimeError("jsonschema_package_required_for_official_validation") from exc

    schema_id = EVENT_TO_SCHEMA_ID.get(event_name)
    if schema_id is None:
        raise ValueError(f"unknown_production_event_for_validation: {event_name}")
    schemas = extract_official_output_schemas(codex_binary)
    schema = schemas.get(schema_id)
    if schema is None:
        raise ValueError(f"official_schema_missing_for_event: {event_name} ({schema_id})")
    validator = jsonschema.Draft7Validator(schema)
    return [f"{'/'.join(str(p) for p in e.absolute_path)}: {e.message}" for e in validator.iter_errors(output)]
