"""Shared helpers for dispatch wire-contract parity checks.

The helpers intentionally use only the Python standard library so the parity
gate remains available before any Rust/TypeScript dependencies are introduced.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[3]
SRC = ROOT / "src"
if str(SRC) not in sys.path:
    sys.path.insert(0, str(SRC))

SCHEMA_DIR = ROOT / "wire_contract" / "v1"
BASE_FIXTURE_DIR = ROOT / "tests" / "fixtures" / "dispatch"
GOLDEN_DIR = ROOT / "tests" / "fixtures" / "dispatch_wire" / "v1"

EXPECTED_SCHEMA_FILES = (
    "dispatch_request.schema.json",
    "task_analysis.schema.json",
    "dispatch_decision.schema.json",
    "execution_result.schema.json",
    "evaluation_result.schema.json",
    "dispatch_bundle.schema.json",
)

ID_PREFIXES = (
    "analysis-",
    "disp-",
    "dec-",
    "res-",
    "exec-",
    "eval-",
    "gate-",
    "chk-",
)

TIMESTAMP_RE = re.compile(r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}")
NORMALIZED_TIMESTAMP = "2000-01-01T00:00:00+00:00"


class SchemaValidationError(AssertionError):
    """Raised when a fixture does not match the frozen wire contract."""


def load_json(path: Path) -> Any:
    return json.loads(path.read_text())


def dump_json(data: Any) -> str:
    return json.dumps(data, indent=2, sort_keys=True) + "\n"


def schema_path(name: str) -> Path:
    return SCHEMA_DIR / name


def load_schema(name: str) -> dict[str, Any]:
    return load_json(schema_path(name))


def base_dispatch_fixture_paths() -> list[Path]:
    return sorted(BASE_FIXTURE_DIR.glob("fixture_*.json"))


def golden_fixture_path(base_fixture_path: Path) -> Path:
    return GOLDEN_DIR / base_fixture_path.name


def build_dispatch_request(fixture: dict[str, Any]) -> dict[str, Any]:
    return {
        "schema_version": "dispatch_request.v1",
        "raw_request": fixture["raw_request"],
        "request_source": fixture.get("request_source", "test_fixture"),
    }


def build_python_bundle(raw_request: str, request_source: str) -> dict[str, Any]:
    from harness_core.dispatch.dispatch_engine import DispatchEngine

    bundle = DispatchEngine().dispatch(raw_request, request_source=request_source)
    return bundle.to_dict()


def normalize_dynamic_values(value: Any) -> Any:
    id_map: dict[str, str] = {}
    counters = {prefix: 0 for prefix in ID_PREFIXES}

    def normalize(inner: Any) -> Any:
        if isinstance(inner, dict):
            return {key: normalize(val) for key, val in inner.items()}
        if isinstance(inner, list):
            return [normalize(item) for item in inner]
        if isinstance(inner, str):
            if TIMESTAMP_RE.match(inner):
                return NORMALIZED_TIMESTAMP
            for prefix in ID_PREFIXES:
                if inner.startswith(prefix):
                    if inner not in id_map:
                        counters[prefix] += 1
                        id_map[inner] = f"{prefix}{counters[prefix]:04d}"
                    return id_map[inner]
        return inner

    return normalize(value)


def build_python_golden_entry(base_fixture_path: Path) -> dict[str, Any]:
    fixture = load_json(base_fixture_path)
    request = build_dispatch_request(fixture)
    bundle = build_python_bundle(request["raw_request"], request["request_source"])
    return {
        "fixture_id": fixture["fixture_id"],
        "name": fixture["name"],
        "request": request,
        "expected_analysis": fixture["expected_analysis"],
        "expected_gates": fixture["expected_gates"],
        "golden_bundle": normalize_dynamic_values(bundle),
    }


def write_python_golden_fixtures() -> list[Path]:
    GOLDEN_DIR.mkdir(parents=True, exist_ok=True)
    written: list[Path] = []
    for base_path in base_dispatch_fixture_paths():
        out_path = golden_fixture_path(base_path)
        out_path.write_text(dump_json(build_python_golden_entry(base_path)))
        written.append(out_path)
    return written


def _resolve_ref(ref: str, root_schema: dict[str, Any]) -> dict[str, Any]:
    prefix = "#/$defs/"
    if not ref.startswith(prefix):
        raise SchemaValidationError(f"unsupported $ref {ref!r}")
    name = ref[len(prefix):]
    try:
        resolved = root_schema["$defs"][name]
    except KeyError as exc:
        raise SchemaValidationError(f"unknown $ref {ref!r}") from exc
    return resolved


def _matches_type(instance: Any, schema_type: str) -> bool:
    if schema_type == "object":
        return isinstance(instance, dict)
    if schema_type == "array":
        return isinstance(instance, list)
    if schema_type == "string":
        return isinstance(instance, str)
    if schema_type == "integer":
        return isinstance(instance, int) and not isinstance(instance, bool)
    if schema_type == "number":
        return (isinstance(instance, int | float) and not isinstance(instance, bool))
    if schema_type == "boolean":
        return isinstance(instance, bool)
    if schema_type == "null":
        return instance is None
    raise SchemaValidationError(f"unsupported schema type {schema_type!r}")


def validate_instance(
    instance: Any,
    schema: dict[str, Any],
    root_schema: dict[str, Any] | None = None,
    path: str = "$",
) -> None:
    root = root_schema or schema
    if "$ref" in schema:
        validate_instance(instance, _resolve_ref(schema["$ref"], root), root, path)
        return

    if "const" in schema and instance != schema["const"]:
        raise SchemaValidationError(f"{path}: expected const {schema['const']!r}, got {instance!r}")

    if "enum" in schema and instance not in schema["enum"]:
        raise SchemaValidationError(f"{path}: {instance!r} not in enum {schema['enum']!r}")

    schema_type = schema.get("type")
    if isinstance(schema_type, list):
        if not any(_matches_type(instance, option) for option in schema_type):
            raise SchemaValidationError(f"{path}: {type(instance).__name__} not in {schema_type!r}")
    elif isinstance(schema_type, str):
        if not _matches_type(instance, schema_type):
            raise SchemaValidationError(f"{path}: expected {schema_type}, got {type(instance).__name__}")

    if isinstance(instance, dict):
        required = schema.get("required", [])
        for key in required:
            if key not in instance:
                raise SchemaValidationError(f"{path}: missing required key {key!r}")

        properties = schema.get("properties", {})
        additional = schema.get("additionalProperties", True)
        for key, val in instance.items():
            child_path = f"{path}.{key}"
            if key in properties:
                validate_instance(val, properties[key], root, child_path)
            elif additional is False:
                raise SchemaValidationError(f"{child_path}: additional property not allowed")
            elif isinstance(additional, dict):
                validate_instance(val, additional, root, child_path)

    if isinstance(instance, list):
        min_items = schema.get("minItems")
        max_items = schema.get("maxItems")
        if min_items is not None and len(instance) < min_items:
            raise SchemaValidationError(f"{path}: expected at least {min_items} items")
        if max_items is not None and len(instance) > max_items:
            raise SchemaValidationError(f"{path}: expected at most {max_items} items")
        item_schema = schema.get("items")
        if isinstance(item_schema, dict):
            for index, item in enumerate(instance):
                validate_instance(item, item_schema, root, f"{path}[{index}]")


def validate_golden_entry(entry: dict[str, Any]) -> None:
    validate_instance(entry["request"], load_schema("dispatch_request.schema.json"))
    bundle = entry["golden_bundle"]
    validate_instance(bundle, load_schema("dispatch_bundle.schema.json"))
    validate_instance(bundle["analysis"], load_schema("task_analysis.schema.json"))
    validate_instance(bundle["decision"], load_schema("dispatch_decision.schema.json"))
    validate_instance(bundle["execution_result"], load_schema("execution_result.schema.json"))
    validate_instance(bundle["evaluation_result"], load_schema("evaluation_result.schema.json"))


def load_golden_entries() -> list[dict[str, Any]]:
    return [load_json(golden_fixture_path(path)) for path in base_dispatch_fixture_paths()]


def run_parity_checks() -> dict[str, Any]:
    missing_schemas = [
        name for name in EXPECTED_SCHEMA_FILES if not schema_path(name).is_file()
    ]
    if missing_schemas:
        raise AssertionError(f"missing schema files: {missing_schemas}")

    mismatches: list[str] = []
    checked = 0
    for base_path in base_dispatch_fixture_paths():
        golden_path = golden_fixture_path(base_path)
        if not golden_path.is_file():
            raise AssertionError(f"missing golden fixture: {golden_path}")
        expected_entry = load_json(golden_path)
        validate_golden_entry(expected_entry)
        actual_entry = build_python_golden_entry(base_path)
        if actual_entry != expected_entry:
            mismatches.append(base_path.name)
        checked += 1

    if mismatches:
        raise AssertionError(f"python reference drifted from golden fixtures: {mismatches}")

    return {
        "checked_fixtures": checked,
        "schema_files": len(EXPECTED_SCHEMA_FILES),
        "golden_dir": str(GOLDEN_DIR),
    }
