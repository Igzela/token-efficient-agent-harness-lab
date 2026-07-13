#!/usr/bin/env python3
"""Bounded release evidence, SBOM, and verification contract.

This module is intentionally dependency-free.  It extends the existing release
workflow and package scripts; it is not a second release authority.  Production
attestation authority remains GitHub's ephemeral OIDC-backed artifact
attestation service.  The local fixture mode is structural evidence only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tarfile
from pathlib import Path
import tomllib
from typing import Any, Callable, Iterable, Mapping, NamedTuple


SCHEMA_VERSION = "release_provenance.v1"
RELEASE_MANIFEST_SCHEMA_VERSION = "release_provenance.v2"
SBOM_SCHEMA_VERSION = "SPDX-2.3"
ATTESTATION_SCHEMA_VERSION = "acp_attestation_fixture.v1"
ATTESTATION_FIXTURE_V2_SCHEMA_VERSION = "acp_attestation_fixture.v2"
VERIFICATION_SCHEMA_VERSION = "release_verification.v1"
RELEASE_VERIFICATION_V2_SCHEMA_VERSION = "release_verification.v2"
TOOL_VERSION = "acp-release-provenance/1"
SPDX_PREDICATE_TYPE = "https://spdx.dev/Document/v2.3"
SLSA_PREDICATE_TYPE = "https://slsa.dev/provenance/v1"
RELEASE_MANIFEST_PREDICATE_TYPE = (
    "https://github.com/Igzela/token-efficient-agent-harness-lab/"
    "attestations/release-manifest/v2"
)
ATTESTATION_ROLES = {
    "slsa": SLSA_PREDICATE_TYPE,
    "spdx": SPDX_PREDICATE_TYPE,
    "release_manifest": RELEASE_MANIFEST_PREDICATE_TYPE,
}
PRODUCTION_ISSUER = "https://token.actions.githubusercontent.com"
PRODUCTION_REPOSITORY = "Igzela/token-efficient-agent-harness-lab"
PRODUCTION_WORKFLOW = ".github/workflows/release.yml"
REQUIRED_BOOTSTRAP_ASSETS = frozenset(
    {"install-from-release.sh", "release_provenance.py"}
)
REQUIRED_RELEASE_LOCKFILES = frozenset(
    {"Cargo.lock", "dashboard/bun.lock", "sdk/typescript/bun.lock"}
)
MAX_JSON_BYTES = 16 * 1024 * 1024
MAX_STRING_BYTES = 4096
MAX_ARRAY_ITEMS = 4096
MAX_OBJECT_FIELDS = 256
MAX_JSON_DEPTH = 16
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")
TAG_RE = re.compile(r"^v[0-9]+(?:\.[0-9]+){2}(?:[-+][0-9A-Za-z.-]+)?$")


class ArchiveLimits(NamedTuple):
    """Bounds applied before a release archive member is extracted."""

    max_archive_bytes: int = 256 * 1024 * 1024
    max_members: int = 4096
    max_member_bytes: int = 128 * 1024 * 1024
    max_total_uncompressed_bytes: int = 512 * 1024 * 1024
    max_path_bytes: int = 512

REASON_CODES = frozenset(
    {
        "VERIFIED_EXTERNAL_EPHEMERAL_IDENTITY",
        "FIXTURE_IDENTITY_NON_AUTHORITATIVE",
        "SCHEMA_UNSUPPORTED",
        "PROVENANCE_INVALID",
        "ARTIFACT_EVIDENCE_MISSING",
        "ARTIFACT_DIGEST_MISMATCH",
        "ARTIFACT_SIZE_MISMATCH",
        "ARTIFACT_MEDIA_TYPE_MISMATCH",
        "SBOM_EVIDENCE_MISSING",
        "SBOM_DIGEST_MISMATCH",
        "SBOM_SCHEMA_MISMATCH",
        "SBOM_SUBJECT_MISMATCH",
        "ATTESTATION_EVIDENCE_MISSING",
        "ATTESTATION_DIGEST_MISMATCH",
        "ATTESTATION_SUBJECT_MISMATCH",
        "ATTESTATION_PREDICATE_MISMATCH",
        "ATTESTATION_NOT_EXTERNALLY_VERIFIED",
        "UNTRUSTED_IDENTITY",
        "SOURCE_BINDING_MISMATCH",
        "WORKFLOW_BINDING_MISMATCH",
        "TARGET_BINDING_MISMATCH",
        "DEPENDENCY_BINDING_MISMATCH",
        "ROLLBACK_TARGET_MISSING",
        "EXTERNAL_VERIFICATION_INVALID",
        "EXTERNAL_VERIFICATION_UNAVAILABLE",
        "VERIFICATION_POLICY_MISMATCH",
    }
)


class ContractError(ValueError):
    """Raised for malformed, unbounded, or internally inconsistent evidence."""


def canonical_json_bytes(value: Any) -> bytes:
    """Return the one canonical JSON representation used for all hashes."""

    return (
        json.dumps(
            value,
            ensure_ascii=True,
            sort_keys=True,
            separators=(",", ":"),
            allow_nan=False,
        )
        + "\n"
    ).encode("utf-8")


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write_canonical_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical_json_bytes(value))


def _duplicate_rejecting_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"duplicate JSON key: {key}")
        result[key] = value
    return result


def _check_bounds(value: Any, depth: int = 0) -> None:
    if depth > MAX_JSON_DEPTH:
        raise ContractError("JSON nesting exceeds contract bound")
    if isinstance(value, str):
        if len(value.encode("utf-8")) > MAX_STRING_BYTES:
            raise ContractError("JSON string exceeds contract bound")
    elif isinstance(value, list):
        if len(value) > MAX_ARRAY_ITEMS:
            raise ContractError("JSON array exceeds contract bound")
        for item in value:
            _check_bounds(item, depth + 1)
    elif isinstance(value, dict):
        if len(value) > MAX_OBJECT_FIELDS:
            raise ContractError("JSON object exceeds contract bound")
        for key, item in value.items():
            _check_bounds(key, depth + 1)
            _check_bounds(item, depth + 1)


def read_json(path: Path) -> Any:
    try:
        if not path.is_file():
            raise FileNotFoundError(path)
        if path.stat().st_size > MAX_JSON_BYTES:
            raise ContractError(f"JSON file exceeds {MAX_JSON_BYTES} bytes")
        value = json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=_duplicate_rejecting_pairs)
        _check_bounds(value)
        return value
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, ContractError) as exc:
        raise ContractError(f"cannot read contract JSON {path}: {exc}") from exc


def _required_string(mapping: Mapping[str, Any], key: str) -> str:
    value = mapping.get(key)
    if not isinstance(value, str) or not value:
        raise ContractError(f"required string field missing: {key}")
    return value


def _required_sha(mapping: Mapping[str, Any], key: str) -> str:
    value = _required_string(mapping, key)
    if not SHA256_RE.fullmatch(value):
        raise ContractError(f"invalid SHA-256 field: {key}")
    return value


def _safe_relative_path(value: Any) -> bool:
    if not isinstance(value, str) or not value or "\\" in value:
        return False
    path = Path(value)
    return not path.is_absolute() and ".." not in path.parts and path.name not in {"", "."}


def _safe_artifact_name(value: Any) -> bool:
    return (
        isinstance(value, str)
        and bool(value)
        and "\\" not in value
        and "/" not in value
        and Path(value).name == value
    )


def _descriptor_list_valid(value: Any) -> bool:
    if not isinstance(value, list) or not value:
        return False
    for item in value:
        if not isinstance(item, dict):
            return False
        if not _safe_relative_path(item.get("path")):
            return False
        digest = item.get("sha256")
        if not isinstance(digest, str) or not SHA256_RE.fullmatch(digest):
            return False
    return True


def _metadata_identity(metadata: Mapping[str, Any], identity_class: str) -> dict[str, Any]:
    repository = _required_string(metadata, "repository")
    ref = _required_string(metadata, "ref")
    workflow = _required_string(metadata, "workflow")
    if identity_class == "fixture":
        return {
            "class": "fixture",
            "issuer": "https://example.invalid/acp-fixture",
            "subject": f"fixture://{repository}/{ref}",
            "repository": repository,
            "ref": ref,
            "workflow": workflow,
            "policy": "isolated-test-only",
        }
    if identity_class == "production-ephemeral-oidc":
        return {
            "class": "production-ephemeral-oidc",
            "issuer": PRODUCTION_ISSUER,
            "subject": f"repo:{repository}:ref:{ref}",
            "repository": repository,
            "ref": ref,
            "workflow": workflow,
            "workflow_ref": metadata.get("workflow_ref", ""),
            "policy": "github-actions-oidc-release-v1",
        }
    raise ContractError(f"unsupported identity class: {identity_class}")


def fixture_identity(metadata: Mapping[str, Any]) -> dict[str, Any]:
    return _metadata_identity(metadata, "fixture")


def production_identity(metadata: Mapping[str, Any]) -> dict[str, Any]:
    return _metadata_identity(metadata, "production-ephemeral-oidc")


def _lockfile_descriptor(path: Path, repository_root: Path | None = None) -> dict[str, str]:
    root = repository_root or Path.cwd()
    resolved_path = path if path.is_absolute() else root / path
    if not resolved_path.is_file():
        raise ContractError(f"lockfile missing: {resolved_path}")
    try:
        relative = resolved_path.resolve().relative_to(root.resolve()).as_posix()
    except ValueError:
        relative = resolved_path.name
    return {"path": relative, "sha256": sha256_file(resolved_path)}


def _strip_json_comments_and_trailing_commas(text: str) -> str:
    """Normalize Bun's JSONC lock format without changing string content."""

    output: list[str] = []
    index = 0
    in_string = False
    escaped = False
    while index < len(text):
        character = text[index]
        if in_string:
            output.append(character)
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == '"':
                in_string = False
            index += 1
            continue
        if character == '"':
            in_string = True
            output.append(character)
            index += 1
            continue
        if character == "/" and index + 1 < len(text):
            following = text[index + 1]
            if following == "/":
                index += 2
                while index < len(text) and text[index] not in "\r\n":
                    index += 1
                continue
            if following == "*":
                end = text.find("*/", index + 2)
                if end < 0:
                    raise ContractError("unterminated JSONC block comment")
                index = end + 2
                continue
        output.append(character)
        index += 1
    if in_string:
        raise ContractError("unterminated JSONC string")

    normalized = "".join(output)
    output = []
    index = 0
    in_string = False
    escaped = False
    while index < len(normalized):
        character = normalized[index]
        if in_string:
            output.append(character)
            if escaped:
                escaped = False
            elif character == "\\":
                escaped = True
            elif character == '"':
                in_string = False
            index += 1
            continue
        if character == '"':
            in_string = True
            output.append(character)
            index += 1
            continue
        if character == ",":
            following = index + 1
            while following < len(normalized) and normalized[following].isspace():
                following += 1
            if following < len(normalized) and normalized[following] in "}]":
                index += 1
                continue
        output.append(character)
        index += 1
    return "".join(output)


def _percent_encode(value: str, *, safe: str = "") -> str:
    allowed = set(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~")
    allowed.update(safe.encode("ascii"))
    return "".join(
        chr(byte) if byte in allowed else f"%{byte:02X}"
        for byte in value.encode("utf-8")
    )


def _purl(ecosystem: str, name: str, version: str) -> str:
    encoded_name = _percent_encode(name, safe="/")
    return f"pkg:{ecosystem}/{encoded_name}@{_percent_encode(version)}"


def _package_identity(package: Mapping[str, Any]) -> tuple[str, str, str, str]:
    return (
        str(package.get("ecosystem", "")),
        str(package.get("name", "")),
        str(package.get("version", "")),
        str(package.get("source_lockfile", "")),
    )


def _append_inventory_package(
    packages: dict[tuple[str, str, str, str], dict[str, Any]], package: dict[str, Any]
) -> None:
    identity = _package_identity(package)
    if not all(identity):
        raise ContractError("dependency package identity is incomplete")
    existing = packages.get(identity)
    if existing is not None and existing != package:
        raise ContractError(f"conflicting dependency package identity: {identity}")
    if existing is not None:
        raise ContractError(f"duplicate dependency package identity: {identity}")
    packages[identity] = package


def _load_cargo_inventory(
    lock_path: Path, relative: str
) -> tuple[list[dict[str, Any]], list[dict[str, str]]]:
    try:
        data = tomllib.loads(lock_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, tomllib.TOMLDecodeError) as exc:
        raise ContractError(f"malformed Cargo lockfile {relative}: {exc}") from exc
    records = data.get("package")
    if not isinstance(records, list) or not records:
        raise ContractError(f"Cargo lockfile has no packages: {relative}")
    packages: list[dict[str, Any]] = []
    by_name: dict[str, list[dict[str, Any]]] = {}
    raw_dependencies: list[tuple[dict[str, Any], list[str]]] = []
    for record in records:
        if not isinstance(record, dict):
            raise ContractError(f"malformed Cargo package in {relative}")
        name = _required_string(record, "name")
        version = _required_string(record, "version")
        dependencies = record.get("dependencies", [])
        if not isinstance(dependencies, list) or not all(
            isinstance(value, str) and value for value in dependencies
        ):
            raise ContractError(f"malformed Cargo dependencies for {name}@{version}")
        package = {
            "ecosystem": "cargo",
            "name": name,
            "version": version,
            "purl": _purl("cargo", name, version),
            "source": str(record.get("source", "workspace")),
            "source_lockfile": relative,
        }
        checksum = record.get("checksum")
        if checksum is not None:
            if not isinstance(checksum, str) or not checksum:
                raise ContractError(f"malformed Cargo checksum for {name}@{version}")
            package["checksum"] = checksum
        packages.append(package)
        by_name.setdefault(name, []).append(package)
        raw_dependencies.append((package, dependencies))

    relationships: list[dict[str, str]] = []
    for source, dependencies in raw_dependencies:
        for dependency in dependencies:
            pieces = dependency.rsplit(" ", 1)
            candidates = by_name.get(pieces[0], [])
            if len(pieces) == 2:
                exact = [item for item in candidates if item["version"] == pieces[1]]
                if exact:
                    candidates = exact
            if len(candidates) != 1:
                raise ContractError(
                    f"Cargo dependency cannot be resolved exactly: {dependency} in {relative}"
                )
            relationships.append(
                {
                    "from": source["purl"],
                    "to": candidates[0]["purl"],
                    "relationship": "DEPENDS_ON",
                    "source_lockfile": relative,
                }
            )
    return packages, relationships


def _split_npm_resolution(resolution: str) -> tuple[str, str]:
    if not resolution or "@" not in resolution[1:]:
        raise ContractError(f"malformed Bun package resolution: {resolution}")
    name, version = resolution.rsplit("@", 1)
    if not name or not version:
        raise ContractError(f"malformed Bun package resolution: {resolution}")
    return name, version


def _load_bun_inventory(
    lock_path: Path, relative: str
) -> tuple[list[dict[str, Any]], list[dict[str, str]]]:
    try:
        raw = lock_path.read_text(encoding="utf-8")
        data = json.loads(
            _strip_json_comments_and_trailing_commas(raw),
            object_pairs_hook=_duplicate_rejecting_pairs,
        )
        _check_bounds(data)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError, ContractError) as exc:
        raise ContractError(f"malformed Bun lockfile {relative}: {exc}") from exc
    records = data.get("packages") if isinstance(data, dict) else None
    if not isinstance(records, dict) or not records:
        raise ContractError(f"Bun lockfile has no packages: {relative}")
    packages: list[dict[str, Any]] = []
    by_name_version: dict[tuple[str, str], dict[str, Any]] = {}
    by_name: dict[str, list[dict[str, Any]]] = {}
    raw_dependencies: list[tuple[dict[str, Any], dict[str, str]]] = []
    for lock_key, record in records.items():
        if not isinstance(lock_key, str) or not isinstance(record, list) or len(record) < 4:
            raise ContractError(f"malformed Bun package record in {relative}")
        resolution, _registry, metadata, integrity = record[:4]
        if not isinstance(resolution, str) or not isinstance(metadata, dict):
            raise ContractError(f"malformed Bun package record in {relative}")
        if not isinstance(integrity, str) or not integrity:
            raise ContractError(f"Bun package integrity missing for {lock_key}")
        name, version = _split_npm_resolution(resolution)
        package = {
            "ecosystem": "npm",
            "name": name,
            "version": version,
            "purl": _purl("npm", name, version),
            "source": "bun-resolved",
            "source_lockfile": relative,
            "integrity": integrity,
        }
        identity = (name, version)
        if identity in by_name_version:
            raise ContractError(
                f"duplicate/conflicting Bun package identity: {name}@{version} in {relative}"
            )
        by_name_version[identity] = package
        by_name.setdefault(name, []).append(package)
        packages.append(package)
        dependencies: dict[str, str] = {}
        for field in ("dependencies", "optionalDependencies", "peerDependencies"):
            values = metadata.get(field, {})
            if not isinstance(values, dict) or not all(
                isinstance(key, str) and isinstance(value, str)
                for key, value in values.items()
            ):
                raise ContractError(f"malformed Bun {field} for {name}@{version}")
            dependencies.update(values)
        raw_dependencies.append((package, dependencies))

    relationships: list[dict[str, str]] = []
    for source, dependencies in raw_dependencies:
        for name, version in dependencies.items():
            target = by_name_version.get((name, version))
            if target is None:
                candidates = by_name.get(name, [])
                if len(candidates) != 1:
                    raise ContractError(
                        f"Bun dependency cannot be resolved uniquely: {name}@{version} in {relative}"
                    )
                target = candidates[0]
            relationships.append(
                {
                    "from": source["purl"],
                    "to": target["purl"],
                    "relationship": "DEPENDS_ON",
                    "source_lockfile": relative,
                }
            )
    return packages, relationships


def load_dependency_inventory(repository_root: Path, lockfiles: Iterable[str]) -> dict[str, Any]:
    """Parse supported lockfiles into a deterministic, offline dependency graph."""

    root = repository_root.resolve()
    requested = sorted(set(lockfiles))
    if not requested:
        raise ContractError("at least one lockfile is required")
    packages: dict[tuple[str, str, str, str], dict[str, Any]] = {}
    relationships: list[dict[str, str]] = []
    descriptors: list[dict[str, str]] = []
    for relative in requested:
        if not _safe_relative_path(relative):
            raise ContractError(f"unsafe lockfile path: {relative}")
        path = (root / relative).resolve()
        try:
            path.relative_to(root)
        except ValueError as exc:
            raise ContractError(f"lockfile escapes repository root: {relative}") from exc
        if not path.is_file():
            raise ContractError(f"lockfile missing: {relative}")
        if path.stat().st_size > MAX_JSON_BYTES:
            raise ContractError(f"lockfile exceeds {MAX_JSON_BYTES} bytes: {relative}")
        if path.name == "Cargo.lock":
            loaded, edges = _load_cargo_inventory(path, relative)
        elif path.name == "bun.lock":
            loaded, edges = _load_bun_inventory(path, relative)
        else:
            raise ContractError(f"unsupported lockfile type: {relative}")
        for package in loaded:
            _append_inventory_package(packages, package)
        relationships.extend(edges)
        descriptors.append({"path": relative, "sha256": sha256_file(path)})
    if len(packages) > MAX_ARRAY_ITEMS or len(relationships) > MAX_ARRAY_ITEMS:
        raise ContractError("dependency inventory exceeds contract bound")
    return {
        "schema_version": "acp_dependency_inventory.v1",
        "lockfiles": sorted(descriptors, key=lambda item: item["path"]),
        "packages": sorted(packages.values(), key=_package_identity),
        "relationships": sorted(
            relationships,
            key=lambda item: (
                item["source_lockfile"], item["from"], item["to"], item["relationship"]
            ),
        ),
    }


def _component_sort_key(component: Mapping[str, Any]) -> tuple[str, str, str]:
    return (
        str(component.get("name", "")),
        str(component.get("version", "")),
        str(component.get("source", "")),
    )


def build_spdx_sbom(
    *,
    metadata: Mapping[str, Any],
    artifact_sha256: str,
    artifact_size: int,
    components: Iterable[Mapping[str, Any]] = (),
    inventory: Mapping[str, Any] | None = None,
) -> dict[str, Any]:
    """Build a deterministic SPDX 2.3 document for a package/container subject."""

    if not SHA256_RE.fullmatch(artifact_sha256):
        raise ContractError("invalid artifact SHA-256")
    if artifact_size < 0:
        raise ContractError("artifact size must be non-negative")
    repository = _required_string(metadata, "repository")
    commit = _required_string(metadata, "source_commit")
    if not COMMIT_RE.fullmatch(commit):
        raise ContractError("source_commit must be a full commit SHA")
    target = _required_string(metadata, "target_triple")
    artifact_name = _required_string(metadata, "artifact_name")
    package_kind = _required_string(metadata, "package_kind")
    if package_kind not in {"package", "container"}:
        raise ContractError("package_kind must be package or container")

    normalized: list[dict[str, str]] = []
    inventory_relationships: list[Mapping[str, Any]] = []
    if inventory is not None:
        if inventory.get("schema_version") != "acp_dependency_inventory.v1":
            raise ContractError("unsupported dependency inventory schema")
        inventory_packages = inventory.get("packages")
        inventory_relationships_value = inventory.get("relationships")
        if not isinstance(inventory_packages, list) or not isinstance(
            inventory_relationships_value, list
        ):
            raise ContractError("dependency inventory packages and relationships are required")
        for component in inventory_packages:
            if not isinstance(component, dict):
                raise ContractError("dependency inventory package must be an object")
            normalized.append(
                {
                    "ecosystem": _required_string(component, "ecosystem"),
                    "name": _required_string(component, "name"),
                    "version": _required_string(component, "version"),
                    "source": _required_string(component, "source"),
                    "source_lockfile": _required_string(component, "source_lockfile"),
                    "purl": _required_string(component, "purl"),
                    "integrity": str(component.get("integrity", component.get("checksum", ""))),
                }
            )
        inventory_relationships = inventory_relationships_value
    else:
        for component in components:
            name = _required_string(component, "name")
            version = _required_string(component, "version")
            source = _required_string(component, "source")
            normalized.append(
                {
                    "ecosystem": "generic",
                    "name": name,
                    "version": version,
                    "source": source,
                    "source_lockfile": source,
                    "purl": _purl("generic", name, version),
                    "integrity": "",
                }
            )
    normalized.sort(
        key=lambda item: (
            item["ecosystem"], item["name"], item["version"], item["source_lockfile"]
        )
    )

    packages: list[dict[str, Any]] = []
    subject_id = "SPDXRef-Subject-" + artifact_sha256[:16]
    packages.append(
        {
            "SPDXID": subject_id,
            "name": artifact_name,
            "versionInfo": str(metadata.get("ref", "unknown")),
            "downloadLocation": "NOASSERTION",
            "filesAnalyzed": False,
            "licenseConcluded": "NOASSERTION",
            "licenseDeclared": "NOASSERTION",
            "supplier": "NOASSERTION",
            "checksums": [
                {"algorithm": "SHA256", "checksumValue": artifact_sha256}
            ],
        }
    )
    relationships = [
        {
            "spdxElementId": "SPDXRef-DOCUMENT",
            "relationshipType": "DESCRIBES",
            "relatedSpdxElement": subject_id,
        }
    ]
    seen: set[tuple[str, str, str, str]] = set()
    package_ids: dict[tuple[str, str], list[str]] = {}
    for component in normalized:
        key = (
            component["ecosystem"],
            component["name"],
            component["version"],
            component["source_lockfile"],
        )
        if key in seen:
            raise ContractError(f"duplicate SPDX package identity: {key}")
        seen.add(key)
        component_id = "SPDXRef-Package-" + sha256_bytes(
            canonical_json_bytes(component)
        )[:16]
        packages.append(
            {
                "SPDXID": component_id,
                "name": component["name"],
                "versionInfo": component["version"],
                "downloadLocation": "NOASSERTION",
                "filesAnalyzed": False,
                "licenseConcluded": "NOASSERTION",
                "licenseDeclared": "NOASSERTION",
                "supplier": "NOASSERTION",
                "comment": (
                    f"ecosystem={component['ecosystem']};"
                    f"source_lockfile={component['source_lockfile']};"
                    f"source={component['source']}"
                ),
                "externalRefs": [
                    {
                        "referenceCategory": "PACKAGE-MANAGER",
                        "referenceLocator": component["purl"],
                        "referenceType": "purl",
                    },
                    {
                        "referenceCategory": "OTHER",
                        "referenceLocator": component["source_lockfile"],
                        "referenceType": "acp:lockfile-source",
                    }
                ],
            }
        )
        relationships.append(
            {
                "spdxElementId": subject_id,
                "relationshipType": "DEPENDS_ON",
                "relatedSpdxElement": component_id,
            }
        )
        package_ids.setdefault((component["source_lockfile"], component["purl"]), []).append(
            component_id
        )

    for relationship in inventory_relationships:
        if not isinstance(relationship, dict):
            raise ContractError("dependency relationship must be an object")
        source_lockfile = _required_string(relationship, "source_lockfile")
        if relationship.get("relationship") != "DEPENDS_ON":
            raise ContractError("unsupported dependency relationship")
        source_ids = package_ids.get((source_lockfile, _required_string(relationship, "from")), [])
        target_ids = package_ids.get((source_lockfile, _required_string(relationship, "to")), [])
        if len(source_ids) != 1 or len(target_ids) != 1:
            raise ContractError("dependency relationship package identity is ambiguous")
        relationships.append(
            {
                "spdxElementId": source_ids[0],
                "relationshipType": "DEPENDS_ON",
                "relatedSpdxElement": target_ids[0],
            }
        )

    lockfiles = metadata.get("lockfiles", [])
    if not isinstance(lockfiles, list):
        raise ContractError("lockfiles must be an array")
    lock_text = ";".join(
        f"{item.get('path')}={item.get('sha256')}" for item in lockfiles
    )
    comment = (
        f"acp.release.v2 repository={repository};source_commit={commit};"
        f"ref={metadata.get('ref', '')};workflow={metadata.get('workflow', '')};"
        f"target={target};package_kind={package_kind};artifact_sha256={artifact_sha256};"
        f"artifact_size={artifact_size};lockfiles={lock_text}"
    )
    return {
        "SPDXID": "SPDXRef-DOCUMENT",
        "spdxVersion": SBOM_SCHEMA_VERSION,
        "creationInfo": {
            "created": "1970-01-01T00:00:00Z",
            "creators": [f"Tool: {TOOL_VERSION}"],
        },
        "dataLicense": "CC0-1.0",
        "documentComment": comment,
        "documentNamespace": (
            f"https://github.com/{repository}/sbom/{package_kind}/{target}/{artifact_sha256}"
        ),
        "name": f"{artifact_name}.spdx",
        "packages": packages,
        "relationships": relationships,
    }


def validate_rollback_state(rollback: Any, metadata: Mapping[str, Any]) -> None:
    """Require either an explicit first release or a fully immutable prior target."""

    if not isinstance(rollback, dict):
        raise ContractError("rollback state must be an object")
    state = rollback.get("state")
    previous = rollback.get("previous")
    if state == "first_release":
        if previous is not None:
            raise ContractError("first release must not invent a previous target")
        return
    if state != "previous_release" or not isinstance(previous, dict):
        raise ContractError("rollback state must be first_release or previous_release")
    tag = _required_string(previous, "tag")
    if not TAG_RE.fullmatch(tag):
        raise ContractError("previous release tag is not an immutable release tag")
    commit = _required_string(previous, "source_commit")
    if not COMMIT_RE.fullmatch(commit):
        raise ContractError("previous release source commit must be a full commit SHA")
    if previous.get("target_triple") != metadata.get("target_triple"):
        raise ContractError("previous release target is incompatible")
    if previous.get("package_kind") != metadata.get("package_kind"):
        raise ContractError("previous release package kind is incompatible")
    current_ref = str(metadata.get("ref", ""))
    if tag == current_ref.removeprefix("refs/tags/"):
        raise ContractError("previous release tag cannot equal the current release")
    if commit == metadata.get("source_commit"):
        raise ContractError("previous release commit cannot equal the current release")
    artifact = previous.get("artifact")
    if not isinstance(artifact, dict) or not _safe_artifact_name(artifact.get("filename")):
        raise ContractError("previous release artifact identity is invalid")
    _required_sha(artifact, "sha256")


def build_release_manifest(
    *,
    metadata: Mapping[str, Any],
    artifact_sha256: str,
    artifact_size: int,
    sbom: Mapping[str, Any],
    sbom_path: Path,
    bootstrap_assets: Iterable[Mapping[str, Any]],
) -> dict[str, Any]:
    """Build the canonical predicate signed by the release-manifest attestation."""

    _required_sha({"sha256": artifact_sha256}, "sha256")
    if artifact_size < 0 or not sbom_path.is_file():
        raise ContractError("artifact and SBOM evidence must exist")
    if sbom.get("spdxVersion") != SBOM_SCHEMA_VERSION:
        raise ContractError("release manifest requires an SPDX 2.3 SBOM")
    source_commit = _required_string(metadata, "source_commit")
    if not COMMIT_RE.fullmatch(source_commit):
        raise ContractError("source_commit must be a full commit SHA")
    ref = _required_string(metadata, "ref")
    if not ref.startswith("refs/tags/") or not TAG_RE.fullmatch(ref.removeprefix("refs/tags/")):
        raise ContractError("release manifest requires an immutable version tag ref")
    lockfiles = metadata.get("lockfiles")
    build_inputs = metadata.get("build_inputs")
    if not _descriptor_list_valid(lockfiles) or not _descriptor_list_valid(build_inputs):
        raise ContractError("release manifest dependency and build inputs are invalid")
    rollback = metadata.get("rollback")
    validate_rollback_state(rollback, metadata)

    bootstrap: list[dict[str, str]] = []
    for asset in bootstrap_assets:
        filename = asset.get("filename")
        if not _safe_artifact_name(filename):
            raise ContractError("bootstrap filename is invalid")
        digest = _required_sha(asset, "sha256")
        commit = _required_string(asset, "source_commit")
        if commit != source_commit:
            raise ContractError("bootstrap source commit differs from release source commit")
        if asset.get("predicate_type") != SLSA_PREDICATE_TYPE:
            raise ContractError("bootstrap asset requires SLSA provenance")
        bootstrap.append(
            {
                "filename": str(filename),
                "sha256": digest,
                "source_commit": commit,
                "predicate_type": SLSA_PREDICATE_TYPE,
            }
        )
    if {item["filename"] for item in bootstrap} != REQUIRED_BOOTSTRAP_ASSETS:
        raise ContractError("the installer and verifier bootstrap assets are both required")
    bootstrap.sort(key=lambda item: item["filename"])
    if len({item["filename"] for item in bootstrap}) != len(bootstrap):
        raise ContractError("duplicate bootstrap asset")

    return {
        "schema_version": RELEASE_MANIFEST_SCHEMA_VERSION,
        "predicate_type": RELEASE_MANIFEST_PREDICATE_TYPE,
        "tool": TOOL_VERSION,
        "repository": _required_string(metadata, "repository"),
        "source": {"commit_sha": source_commit, "ref": ref, "tag": ref.removeprefix("refs/tags/")},
        "workflow": {
            "path": _required_string(metadata, "workflow"),
            "workflow_ref": _required_string(metadata, "workflow_ref"),
            "run_id": str(metadata.get("run_id", "")),
            "run_attempt": int(metadata.get("run_attempt", 0)),
            "job": _required_string(metadata, "job"),
            "builder_id": _required_string(metadata, "builder_id"),
        },
        "target": {
            "os": _required_string(metadata, "target_os"),
            "architecture": _required_string(metadata, "target_architecture"),
            "triple": _required_string(metadata, "target_triple"),
            "package_kind": _required_string(metadata, "package_kind"),
        },
        "artifact": {
            "filename": _required_string(metadata, "artifact_name"),
            "sha256": artifact_sha256,
            "size": artifact_size,
            "media_type": _required_string(metadata, "artifact_media_type"),
        },
        "sbom": {
            "filename": sbom_path.name,
            "sha256": sha256_file(sbom_path),
            "size": sbom_path.stat().st_size,
            "predicate_type": SPDX_PREDICATE_TYPE,
        },
        "dependencies": {
            "lockfiles": sorted(lockfiles, key=lambda item: str(item.get("path", ""))),
            "inventory_sha256": sha256_bytes(canonical_json_bytes({
                "packages": sbom.get("packages", []),
                "relationships": sbom.get("relationships", []),
            })),
        },
        "build_inputs": sorted(build_inputs, key=lambda item: str(item.get("path", ""))),
        "bootstrap": bootstrap,
        "publication": {
            "mode": _required_string(metadata, "publication_mode"),
            "external_action_authorized": False,
        },
        "rollback": rollback,
        "verification_policy": {
            "bundle_roles": dict(ATTESTATION_ROLES),
            "issuer": PRODUCTION_ISSUER,
            "repository": _required_string(metadata, "repository"),
            "workflow": _required_string(metadata, "workflow"),
            "exact_local_bundles_required": True,
            "unsigned_local_files_authoritative": False,
        },
    }


def build_attestation_fixture(
    *,
    metadata: Mapping[str, Any],
    artifact_sha256: str,
    sbom_sha256: str | None = None,
    identity: Mapping[str, Any],
    role: str | None = None,
    predicate_type: str | None = None,
    predicate: Any = None,
) -> dict[str, Any]:
    _required_sha({"sha256": artifact_sha256}, "sha256")
    if role is not None:
        expected_type = ATTESTATION_ROLES.get(role)
        if expected_type is None or predicate_type != expected_type:
            raise ContractError("fixture attestation role and predicate type differ")
        if not isinstance(predicate, dict):
            raise ContractError("fixture attestation predicate must be an object")
        return {
            "schema_version": ATTESTATION_FIXTURE_V2_SCHEMA_VERSION,
            "role": role,
            "predicate_type": predicate_type,
            "subject": {
                "name": _required_string(metadata, "artifact_name"),
                "digest": {"sha256": artifact_sha256},
            },
            "predicate": predicate,
            "identity": dict(identity),
            "fixture": True,
        }
    if sbom_sha256 is None:
        raise ContractError("legacy fixture requires an SBOM digest")
    _required_sha({"sha256": sbom_sha256}, "sha256")
    return {
        "schema_version": ATTESTATION_SCHEMA_VERSION,
        "predicate_type": SLSA_PREDICATE_TYPE,
        "subject": {
            "name": _required_string(metadata, "artifact_name"),
            "digest": {"sha256": artifact_sha256},
        },
        "sbom": {"predicate_type": SPDX_PREDICATE_TYPE, "sha256": sbom_sha256},
        "identity": dict(identity),
        "fixture": identity.get("class") == "fixture",
    }


def build_provenance(
    *,
    metadata: Mapping[str, Any],
    artifact_sha256: str,
    artifact_size: int,
    sbom: Mapping[str, Any],
    sbom_path: Path,
    attestation: Mapping[str, Any] | None,
    attestation_path: Path,
) -> dict[str, Any]:
    _required_sha({"sha256": artifact_sha256}, "sha256")
    if artifact_size < 0:
        raise ContractError("artifact size must be non-negative")
    identity = production_identity(metadata)
    if attestation is not None and isinstance(attestation.get("identity"), dict):
        identity = dict(attestation["identity"])
    predicate_type = SLSA_PREDICATE_TYPE
    if attestation is not None and isinstance(attestation.get("predicate_type"), str):
        predicate_type = attestation["predicate_type"]
    lockfiles = metadata.get("lockfiles", [])
    build_inputs = metadata.get("build_inputs", [])
    if not isinstance(lockfiles, list) or not isinstance(build_inputs, list):
        raise ContractError("lockfiles and build_inputs must be arrays")
    return {
        "schema_version": SCHEMA_VERSION,
        "tool": TOOL_VERSION,
        "repository": _required_string(metadata, "repository"),
        "source": {
            "commit_sha": _required_string(metadata, "source_commit"),
            "ref": _required_string(metadata, "ref"),
            "tag": metadata.get("ref", "")
            if str(metadata.get("ref", "")).startswith("refs/tags/")
            else None,
        },
        "workflow": {
            "path": _required_string(metadata, "workflow"),
            "workflow_ref": metadata.get("workflow_ref", ""),
            "run_id": str(metadata.get("run_id", "dry-run")),
            "run_attempt": int(metadata.get("run_attempt", 1)),
            "job": metadata.get("job", "dry-run"),
            "builder_id": _required_string(metadata, "builder_id"),
        },
        "target": {
            "os": _required_string(metadata, "target_os"),
            "architecture": _required_string(metadata, "target_architecture"),
            "triple": _required_string(metadata, "target_triple"),
        },
        "package": {
            "kind": _required_string(metadata, "package_kind"),
            "name": _required_string(metadata, "artifact_name"),
            "media_type": _required_string(metadata, "artifact_media_type"),
        },
        "dependencies": {"lockfiles": lockfiles},
        "build_inputs": build_inputs,
        "artifacts": [
            {
                "filename": _required_string(metadata, "artifact_name"),
                "digest": {"sha256": artifact_sha256},
                "media_type": _required_string(metadata, "artifact_media_type"),
                "size": artifact_size,
            }
        ],
        "sbom": {
            "schema": sbom.get("spdxVersion"),
            "predicate_type": SPDX_PREDICATE_TYPE,
            "digest": {"sha256": sha256_file(sbom_path)},
            "media_type": "application/spdx+json",
            "size": sbom_path.stat().st_size,
        },
        "attestation": {
            "predicate_type": predicate_type,
            "digest": {"sha256": sha256_file(attestation_path)},
            "media_type": "application/vnd.in-toto+json",
            "size": attestation_path.stat().st_size,
            "identity": identity,
        },
        "rollback": {
            "previous_known_good": _required_string(metadata, "previous_known_good"),
            "target": _required_string(metadata, "rollback_target"),
        },
        "publication": {
            "mode": _required_string(metadata, "publication_mode"),
            "external_action_authorized": False,
        },
        "verification_policy": {
            "production_identity_class": "production-ephemeral-oidc",
            "issuer": PRODUCTION_ISSUER,
            "repository": _required_string(metadata, "repository"),
            "workflow": _required_string(metadata, "workflow"),
            "tag_ref_required": True,
            "unsigned_is_verified": False,
        },
    }


def _v2_result(status: str, reasons: Iterable[str], inputs: Mapping[str, Any]) -> dict[str, Any]:
    return {
        "schema_version": RELEASE_VERIFICATION_V2_SCHEMA_VERSION,
        "status": status,
        "reason_codes": list(dict.fromkeys(reasons)),
        "inputs": dict(inputs),
    }


def _manifest_metadata(manifest: Mapping[str, Any]) -> dict[str, Any]:
    source = manifest.get("source")
    workflow = manifest.get("workflow")
    target = manifest.get("target")
    artifact = manifest.get("artifact")
    if not all(isinstance(value, dict) for value in (source, workflow, target, artifact)):
        raise ContractError("release manifest binding objects are missing")
    repository = _required_string(manifest, "repository")
    workflow_path = _required_string(workflow, "path")
    if repository != PRODUCTION_REPOSITORY or workflow_path != PRODUCTION_WORKFLOW:
        raise ContractError("release authority is not the canonical repository workflow")
    return {
        "repository": repository,
        "source_commit": _required_string(source, "commit_sha"),
        "ref": _required_string(source, "ref"),
        "workflow": workflow_path,
        "target_triple": _required_string(target, "triple"),
        "package_kind": _required_string(target, "package_kind"),
        "artifact_name": _required_string(artifact, "filename"),
    }


def _validate_v2_local_files(
    artifact_path: Path, sbom_path: Path, manifest_path: Path
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    if not artifact_path.is_file() or not sbom_path.is_file() or not manifest_path.is_file():
        raise ContractError("artifact, SBOM, and release manifest must all exist")
    sbom = read_json(sbom_path)
    manifest = read_json(manifest_path)
    if not isinstance(sbom, dict) or sbom.get("spdxVersion") != SBOM_SCHEMA_VERSION:
        raise ContractError("canonical local SBOM is not SPDX 2.3")
    if not isinstance(manifest, dict) or manifest.get("schema_version") != RELEASE_MANIFEST_SCHEMA_VERSION:
        raise ContractError("canonical local release manifest is not release_provenance.v2")
    if manifest.get("predicate_type") != RELEASE_MANIFEST_PREDICATE_TYPE:
        raise ContractError("release manifest predicate type is incorrect")
    metadata = _manifest_metadata(manifest)
    source_commit = metadata["source_commit"]
    ref = metadata["ref"]
    if not COMMIT_RE.fullmatch(source_commit) or not ref.startswith("refs/tags/"):
        raise ContractError("release source commit or tag ref is invalid")
    if not TAG_RE.fullmatch(ref.removeprefix("refs/tags/")):
        raise ContractError("release tag is not a version tag")
    source = manifest["source"]
    workflow = manifest["workflow"]
    if source.get("tag") != ref.removeprefix("refs/tags/"):
        raise ContractError("release tag and ref differ")
    expected_workflow_ref = (
        f"{PRODUCTION_REPOSITORY}/{PRODUCTION_WORKFLOW}@{ref}"
    )
    if workflow.get("workflow_ref") != expected_workflow_ref:
        raise ContractError("release workflow ref is not bound to the release tag")
    if not isinstance(workflow.get("run_attempt"), int) or workflow.get("run_attempt") < 1:
        raise ContractError("release workflow run attempt is invalid")
    actual_artifact_sha = sha256_file(artifact_path)
    artifact = manifest["artifact"]
    if (
        artifact.get("filename") != artifact_path.name
        or artifact.get("sha256") != actual_artifact_sha
        or artifact.get("size") != artifact_path.stat().st_size
        or not isinstance(artifact.get("media_type"), str)
        or not artifact.get("media_type")
    ):
        raise ContractError("release artifact differs from signed manifest predicate")
    target = manifest["target"]
    if metadata["package_kind"] not in {"package", "container"} or not all(
        isinstance(target.get(field), str) and target.get(field)
        for field in ("os", "architecture", "triple")
    ):
        raise ContractError("release target binding is invalid")
    sbom_record = manifest.get("sbom")
    if not isinstance(sbom_record, dict) or (
        sbom_record.get("filename") != sbom_path.name
        or sbom_record.get("sha256") != sha256_file(sbom_path)
        or sbom_record.get("size") != sbom_path.stat().st_size
        or sbom_record.get("predicate_type") != SPDX_PREDICATE_TYPE
    ):
        raise ContractError("canonical local SBOM differs from release manifest")
    packages = sbom.get("packages")
    relationships = sbom.get("relationships")
    if not isinstance(packages, list) or not isinstance(relationships, list):
        raise ContractError("SPDX packages or relationships are missing")
    subjects = [
        package
        for package in packages
        if isinstance(package, dict) and package.get("name") == artifact_path.name
    ]
    if len(subjects) != 1:
        raise ContractError("SPDX must contain exactly one release artifact subject")
    subject = subjects[0]
    if subject.get("checksums") != [
        {"algorithm": "SHA256", "checksumValue": actual_artifact_sha}
    ]:
        raise ContractError("SPDX subject digest differs from release artifact")
    subject_id = subject.get("SPDXID")
    if not isinstance(subject_id, str) or not any(
        isinstance(relationship, dict)
        and relationship.get("spdxElementId") == "SPDXRef-DOCUMENT"
        and relationship.get("relationshipType") == "DESCRIBES"
        and relationship.get("relatedSpdxElement") == subject_id
        for relationship in relationships
    ):
        raise ContractError("SPDX document does not describe the release artifact subject")
    dependency_packages = [package for package in packages if package is not subject]
    if not dependency_packages:
        raise ContractError("SPDX dependency inventory is empty")
    dependency_ids: set[str] = set()
    dependency_identities: set[tuple[str, str]] = set()
    for package in dependency_packages:
        if not isinstance(package, dict):
            raise ContractError("SPDX dependency package is malformed")
        package_id = _required_string(package, "SPDXID")
        version = _required_string(package, "versionInfo")
        if version == "locked" or package_id in dependency_ids:
            raise ContractError("SPDX dependency identity is placeholder or duplicated")
        dependency_ids.add(package_id)
        external_refs = package.get("externalRefs")
        if not isinstance(external_refs, list):
            raise ContractError("SPDX dependency external references are missing")
        purls = [
            ref.get("referenceLocator")
            for ref in external_refs
            if isinstance(ref, dict) and ref.get("referenceType") == "purl"
        ]
        lock_sources = [
            ref.get("referenceLocator")
            for ref in external_refs
            if isinstance(ref, dict) and ref.get("referenceType") == "acp:lockfile-source"
        ]
        if (
            len(purls) != 1
            or not isinstance(purls[0], str)
            or not purls[0].startswith(("pkg:cargo/", "pkg:npm/"))
            or len(lock_sources) != 1
            or lock_sources[0] not in REQUIRED_RELEASE_LOCKFILES
        ):
            raise ContractError("SPDX dependency purl or lockfile source is invalid")
        identity = (purls[0], str(lock_sources[0]))
        if identity in dependency_identities:
            raise ContractError("SPDX dependency identity is duplicated")
        dependency_identities.add(identity)
    described_dependencies = {
        relationship.get("relatedSpdxElement")
        for relationship in relationships
        if isinstance(relationship, dict)
        and relationship.get("spdxElementId") == subject_id
        and relationship.get("relationshipType") == "DEPENDS_ON"
    }
    if described_dependencies != dependency_ids:
        raise ContractError("SPDX artifact dependency relationships are incomplete")
    validate_rollback_state(manifest.get("rollback"), metadata)
    dependencies = manifest.get("dependencies")
    if not isinstance(dependencies, dict) or not _descriptor_list_valid(
        dependencies.get("lockfiles")
    ):
        raise ContractError("release dependency lockfile bindings are invalid")
    lockfiles = dependencies["lockfiles"]
    if {item["path"] for item in lockfiles} != REQUIRED_RELEASE_LOCKFILES:
        raise ContractError("release dependency lockfile set is incomplete")
    expected_inventory_sha = sha256_bytes(
        canonical_json_bytes(
            {"packages": packages, "relationships": relationships}
        )
    )
    if dependencies.get("inventory_sha256") != expected_inventory_sha:
        raise ContractError("release dependency inventory digest differs from the SBOM")
    sbom_comment = str(sbom.get("documentComment", ""))
    for expected in (
        metadata["source_commit"],
        metadata["ref"],
        metadata["workflow"],
        metadata["target_triple"],
        f"package_kind={metadata['package_kind']}",
        f"artifact_sha256={actual_artifact_sha}",
        *(f"{item['path']}={item['sha256']}" for item in lockfiles),
    ):
        if expected not in sbom_comment:
            raise ContractError("SPDX release or dependency binding is incomplete")
    if not _descriptor_list_valid(manifest.get("build_inputs")):
        raise ContractError("release build input bindings are invalid")
    policy = manifest.get("verification_policy")
    if policy != {
        "bundle_roles": dict(ATTESTATION_ROLES),
        "issuer": PRODUCTION_ISSUER,
        "repository": PRODUCTION_REPOSITORY,
        "workflow": PRODUCTION_WORKFLOW,
        "exact_local_bundles_required": True,
        "unsigned_local_files_authoritative": False,
    }:
        raise ContractError("release verification policy is not canonical")
    publication = manifest.get("publication")
    if not isinstance(publication, dict) or publication.get("external_action_authorized") is not False:
        raise ContractError("release manifest cannot authorize external publication")
    bootstrap = manifest.get("bootstrap")
    if not isinstance(bootstrap, list) or not bootstrap:
        raise ContractError("release manifest has no immutable bootstrap assets")
    for asset in bootstrap:
        if not isinstance(asset, dict):
            raise ContractError("bootstrap asset record is malformed")
        _required_sha(asset, "sha256")
        if asset.get("source_commit") != source_commit:
            raise ContractError("bootstrap source commit differs from release source commit")
        if asset.get("predicate_type") != SLSA_PREDICATE_TYPE:
            raise ContractError("bootstrap lacks SLSA provenance binding")
    names = [_required_string(asset, "filename") for asset in bootstrap]
    if len(names) != len(set(names)) or set(names) != REQUIRED_BOOTSTRAP_ASSETS:
        raise ContractError("release manifest bootstrap asset set is not canonical")
    return sbom, manifest, metadata


def _fixture_bundle_valid(
    bundle_path: Path,
    *,
    role: str,
    predicate_type: str,
    artifact_name: str,
    artifact_sha256: str,
    expected_predicate: Mapping[str, Any] | None,
    metadata: Mapping[str, Any],
) -> bool:
    try:
        bundle = read_json(bundle_path)
    except ContractError:
        return False
    if not isinstance(bundle, dict) or bundle.get("schema_version") != ATTESTATION_FIXTURE_V2_SCHEMA_VERSION:
        return False
    identity = bundle.get("identity")
    if not isinstance(identity, dict) or identity.get("class") != "fixture":
        return False
    if (
        identity.get("repository") != metadata["repository"]
        or identity.get("ref") != metadata["ref"]
        or identity.get("workflow") != metadata["workflow"]
    ):
        return False
    if bundle.get("role") != role or bundle.get("predicate_type") != predicate_type:
        return False
    subject = bundle.get("subject")
    if not isinstance(subject, dict) or (
        subject.get("name") != artifact_name
        or subject.get("digest", {}).get("sha256") != artifact_sha256
    ):
        return False
    predicate = bundle.get("predicate")
    if not isinstance(predicate, dict):
        return False
    return expected_predicate is None or predicate == expected_predicate


def _default_gh_attestation_runner(command: list[str]) -> Any:
    completed = subprocess.run(
        command,
        check=False,
        capture_output=True,
        text=True,
        timeout=120,
    )
    if completed.returncode != 0:
        raise ContractError(
            f"gh attestation verify failed ({completed.returncode}): {completed.stderr.strip()}"
        )
    try:
        return json.loads(completed.stdout, object_pairs_hook=_duplicate_rejecting_pairs)
    except json.JSONDecodeError as exc:
        raise ContractError("gh attestation verify did not return JSON") from exc


def _production_verification_valid(
    value: Any,
    *,
    predicate_type: str,
    expected_predicate: Mapping[str, Any] | None,
    artifact_name: str,
    artifact_sha256: str,
    metadata: Mapping[str, Any],
) -> bool:
    entries = _external_verification_entries(value)
    if len(entries) != 1:
        return False
    verification = entries[0].get("verificationResult")
    if not isinstance(verification, dict):
        return False
    timestamps = verification.get("verifiedTimestamps")
    if not isinstance(timestamps, list) or not timestamps:
        return False
    statement = verification.get("statement")
    signature = verification.get("signature")
    if not isinstance(statement, dict) or not isinstance(signature, dict):
        return False
    if statement.get("predicateType") != predicate_type:
        return False
    subjects = statement.get("subject")
    if not isinstance(subjects, list) or len(subjects) != 1:
        return False
    subject = subjects[0]
    if not isinstance(subject, dict) or (
        subject.get("name") != artifact_name
        or subject.get("digest", {}).get("sha256") != artifact_sha256
    ):
        return False
    predicate = statement.get("predicate")
    if not isinstance(predicate, dict):
        return False
    if expected_predicate is not None and predicate != expected_predicate:
        return False
    certificate = signature.get("certificate")
    if not isinstance(certificate, dict):
        return False
    repository_uri = f"https://github.com/{metadata['repository']}"
    workflow_uri = (
        f"{repository_uri}/{metadata['workflow']}@{metadata['ref']}"
    )
    return (
        certificate.get("issuer") == PRODUCTION_ISSUER
        and certificate.get("sourceRepositoryURI") == repository_uri
        and certificate.get("sourceRepositoryRef") == metadata["ref"]
        and certificate.get("sourceRepositoryDigest") == metadata["source_commit"]
        and certificate.get("buildConfigURI") == workflow_uri
        and certificate.get("buildSignerURI") == workflow_uri
        and certificate.get("subjectAlternativeName") == workflow_uri
        and certificate.get("runnerEnvironment") == "github-hosted"
    )


def verify_release(
    *,
    artifact_path: Path,
    sbom_path: Path,
    manifest_path: Path,
    slsa_bundle_path: Path,
    spdx_bundle_path: Path,
    manifest_bundle_path: Path,
    mode: str,
    gh_runner: Callable[[list[str]], Any] | None = None,
) -> dict[str, Any]:
    """Verify the exact three local bundles; fixture mode never grants production authority."""

    paths = {
        "artifact": artifact_path,
        "sbom": sbom_path,
        "manifest": manifest_path,
        "slsa_bundle": slsa_bundle_path,
        "spdx_bundle": spdx_bundle_path,
        "release_manifest_bundle": manifest_bundle_path,
    }
    inputs: dict[str, Any] = {f"{name}_path": str(path) for name, path in paths.items()}
    try:
        sbom, manifest, metadata = _validate_v2_local_files(
            artifact_path, sbom_path, manifest_path
        )
        bundle_paths = [slsa_bundle_path, spdx_bundle_path, manifest_bundle_path]
        if not all(path.is_file() for path in bundle_paths):
            raise ContractError("all three exact local attestation bundles are required")
        bundle_hashes = [sha256_file(path) for path in bundle_paths]
        if len(set(bundle_hashes)) != len(bundle_hashes):
            raise ContractError("one attestation bundle cannot satisfy two evidence roles")
        for name, path in paths.items():
            inputs[f"{name}_sha256"] = sha256_file(path)
        artifact_sha = inputs["artifact_sha256"]
        roles = [
            ("slsa", slsa_bundle_path, None),
            ("spdx", spdx_bundle_path, sbom),
            ("release_manifest", manifest_bundle_path, manifest),
        ]
        if mode == "fixture":
            valid = all(
                _fixture_bundle_valid(
                    path,
                    role=role,
                    predicate_type=ATTESTATION_ROLES[role],
                    artifact_name=artifact_path.name,
                    artifact_sha256=artifact_sha,
                    expected_predicate=predicate,
                    metadata=metadata,
                )
                for role, path, predicate in roles
            )
            if not valid:
                raise ContractError("fixture bundle role, subject, or predicate mismatch")
            return _v2_result(
                "verified_fixture", ["FIXTURE_IDENTITY_NON_AUTHORITATIVE"], inputs
            )
        if mode != "production":
            raise ContractError("verification mode must be fixture or production")
        if manifest.get("publication", {}).get("mode") != "github-release":
            raise ContractError("production verification requires GitHub release mode")

        for path in bundle_paths:
            try:
                local_value = read_json(path)
            except ContractError:
                local_value = None
            if isinstance(local_value, dict) and local_value.get("schema_version") in {
                ATTESTATION_SCHEMA_VERSION,
                ATTESTATION_FIXTURE_V2_SCHEMA_VERSION,
            }:
                raise ContractError("fixture evidence cannot authorize production verification")

        runner = gh_runner or _default_gh_attestation_runner
        for role, path, predicate in roles:
            command = [
                "gh",
                "attestation",
                "verify",
                str(artifact_path),
                "--bundle",
                str(path),
                "--predicate-type",
                ATTESTATION_ROLES[role],
                "--repo",
                metadata["repository"],
                "--source-ref",
                metadata["ref"],
                "--source-digest",
                metadata["source_commit"],
                "--signer-workflow",
                f"{metadata['repository']}/{metadata['workflow']}",
                "--cert-oidc-issuer",
                PRODUCTION_ISSUER,
                "--deny-self-hosted-runners",
                "--format",
                "json",
            ]
            value = runner(command)
            if not _production_verification_valid(
                value,
                predicate_type=ATTESTATION_ROLES[role],
                expected_predicate=predicate,
                artifact_name=artifact_path.name,
                artifact_sha256=artifact_sha,
                metadata=metadata,
            ):
                raise ContractError(f"{role} exact-bundle verification output is mismatched")
        return _v2_result("verified", ["VERIFIED_EXACT_LOCAL_BUNDLES"], inputs)
    except (ContractError, OSError, ValueError, subprocess.SubprocessError) as exc:
        inputs["error"] = str(exc)
        return _v2_result("rejected", ["EXACT_BUNDLE_VERIFICATION_FAILED"], inputs)


def _normalized_archive_path(name: str, limits: ArchiveLimits) -> tuple[str, ...]:
    if not name or "\\" in name or name.startswith("/") or "\x00" in name:
        raise ContractError(f"unsafe archive member path: {name!r}")
    if len(name.encode("utf-8")) > limits.max_path_bytes:
        raise ContractError("archive member path exceeds contract bound")
    parts = tuple(name.rstrip("/").split("/"))
    if not parts or any(part in {"", ".", ".."} for part in parts):
        raise ContractError(f"non-normal archive member path: {name!r}")
    return parts


def validate_release_archive(
    archive_path: Path,
    expected_top_level: str,
    *,
    limits: ArchiveLimits = ArchiveLimits(),
) -> dict[str, Any]:
    """Inspect every archive header and enforce bounds before extraction."""

    if not archive_path.is_file():
        raise ContractError("release archive is missing")
    if archive_path.stat().st_size > limits.max_archive_bytes:
        raise ContractError("release archive exceeds compressed-byte bound")
    if not _safe_artifact_name(expected_top_level):
        raise ContractError("expected top-level directory is invalid")
    seen: dict[tuple[str, ...], str] = {}
    total_size = 0
    member_count = 0
    required = {
        (expected_top_level, "engine"),
        (expected_top_level, "release_provenance.py"),
        (expected_top_level, "install.sh"),
        (expected_top_level, "upgrade.sh"),
    }
    present_required: set[tuple[str, ...]] = set()
    try:
        with tarfile.open(archive_path, "r:gz") as archive:
            for member in archive:
                member_count += 1
                if member_count > limits.max_members:
                    raise ContractError("release archive member count exceeds contract bound")
                parts = _normalized_archive_path(member.name, limits)
                if parts[0] != expected_top_level:
                    raise ContractError("release archive has an unexpected top-level directory")
                if parts in seen:
                    raise ContractError("release archive contains a duplicate normalized path")
                if member.issym() or member.islnk() or member.isdev() or member.isfifo():
                    raise ContractError("release archive contains a link, device, or FIFO")
                if getattr(member, "sparse", None):
                    raise ContractError("release archive contains a sparse member")
                if not (member.isfile() or member.isdir()):
                    raise ContractError("release archive contains an unsupported member type")
                kind = "file" if member.isfile() else "directory"
                for existing, existing_kind in seen.items():
                    if existing_kind == "file" and len(existing) < len(parts) and parts[: len(existing)] == existing:
                        raise ContractError("archive file conflicts with a descendant path")
                    if kind == "file" and len(parts) < len(existing) and existing[: len(parts)] == parts:
                        raise ContractError("archive file conflicts with an existing descendant")
                seen[parts] = kind
                if member.isfile():
                    if member.size < 0 or member.size > limits.max_member_bytes:
                        raise ContractError("archive member exceeds uncompressed-byte bound")
                    total_size += member.size
                    if total_size > limits.max_total_uncompressed_bytes:
                        raise ContractError("archive total uncompressed size exceeds contract bound")
                    if parts in required:
                        present_required.add(parts)
    except (tarfile.TarError, OSError) as exc:
        raise ContractError(f"cannot inspect release archive: {exc}") from exc
    if not seen or {parts[0] for parts in seen} != {expected_top_level}:
        raise ContractError("release archive must contain exactly one expected top-level directory")
    if present_required != required:
        missing = sorted("/".join(path) for path in required - present_required)
        raise ContractError(f"release archive required files are missing: {missing}")
    return {
        "archive_sha256": sha256_file(archive_path),
        "archive_bytes": archive_path.stat().st_size,
        "member_count": member_count,
        "total_uncompressed_bytes": total_size,
        "top_level": expected_top_level,
        "required_files": len(present_required),
    }


def extract_release_archive(
    archive_path: Path,
    destination: Path,
    expected_top_level: str,
    *,
    limits: ArchiveLimits = ArchiveLimits(),
) -> dict[str, Any]:
    """Validate then manually extract ordinary files/directories with safe modes."""

    summary = validate_release_archive(
        archive_path, expected_top_level, limits=limits
    )
    before_digest = summary["archive_sha256"]
    destination.mkdir(parents=True, exist_ok=True)
    destination_resolved = destination.resolve()
    try:
        with tarfile.open(archive_path, "r:gz") as archive:
            for member in archive:
                parts = _normalized_archive_path(member.name, limits)
                target = destination.joinpath(*parts)
                resolved_parent = target.parent.resolve()
                try:
                    resolved_parent.relative_to(destination_resolved)
                except ValueError as exc:
                    raise ContractError("archive extraction path escapes destination") from exc
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True, mode=0o755)
                    continue
                target.parent.mkdir(parents=True, exist_ok=True, mode=0o755)
                source = archive.extractfile(member)
                if source is None:
                    raise ContractError("archive file content is unavailable")
                remaining = member.size
                with target.open("xb") as output:
                    while remaining:
                        chunk = source.read(min(1024 * 1024, remaining))
                        if not chunk:
                            raise ContractError("archive file ended before its declared size")
                        output.write(chunk)
                        remaining -= len(chunk)
                    if source.read(1):
                        raise ContractError("archive file exceeds its declared size")
                target.chmod(0o755 if member.mode & 0o111 else 0o644)
    except (tarfile.TarError, OSError) as exc:
        raise ContractError(f"cannot extract release archive: {exc}") from exc
    if sha256_file(archive_path) != before_digest:
        raise ContractError("release archive changed during extraction")
    return summary


def _invalid_result(reason_codes: list[str], inputs: Mapping[str, Any], status: str = "rejected") -> dict[str, Any]:
    unique = []
    for reason in reason_codes:
        if reason not in unique:
            unique.append(reason)
    return {
        "schema_version": VERIFICATION_SCHEMA_VERSION,
        "status": status,
        "reason_codes": unique,
        "inputs": dict(inputs),
    }


def _external_verification_entries(value: Any) -> list[Mapping[str, Any]]:
    if isinstance(value, list):
        return [item for item in value if isinstance(item, dict)]
    if isinstance(value, dict) and isinstance(value.get("verificationResult"), dict):
        return [value]
    if isinstance(value, dict):
        entries: list[Mapping[str, Any]] = []
        for key in ("provenance", "sbom"):
            nested = value.get(key)
            if isinstance(nested, list):
                entries.extend(item for item in nested if isinstance(item, dict))
            elif isinstance(nested, dict):
                entries.append(nested)
        return entries
    return []


def _external_verification_is_valid(
    path: Path | None,
    provenance: Mapping[str, Any],
    artifact_sha256: str,
) -> bool:
    if path is None or not path.is_file() or path.stat().st_size > MAX_JSON_BYTES:
        return False
    try:
        value = read_json(path)
    except ContractError:
        return False
    entries = _external_verification_entries(value)
    if not entries:
        return False

    repository = provenance.get("repository")
    ref = provenance.get("source", {}).get("ref")
    workflow = provenance.get("workflow", {}).get("path")
    predicates: set[str] = set()
    for entry in entries:
        verification = entry.get("verificationResult")
        if not isinstance(verification, dict):
            return False
        statement = verification.get("statement")
        signature = verification.get("signature")
        certificate = signature.get("certificate") if isinstance(signature, dict) else None
        timestamps = verification.get("verifiedTimestamps")
        if not isinstance(statement, dict) or not isinstance(certificate, dict):
            return False
        if not isinstance(timestamps, list) or not timestamps:
            return False

        predicate = statement.get("predicateType")
        if not isinstance(predicate, str):
            return False
        predicates.add(predicate)
        subjects = statement.get("subject")
        if not isinstance(subjects, list) or not subjects:
            return False
        if not any(
            isinstance(subject, dict)
            and isinstance(subject.get("digest"), dict)
            and subject["digest"].get("sha256") == artifact_sha256
            for subject in subjects
        ):
            return False

        if certificate.get("oidcIssuer") != PRODUCTION_ISSUER:
            return False
        if certificate.get("sourceRepository") != repository:
            return False
        if certificate.get("sourceRepositoryRef") != ref:
            return False
        san = certificate.get("subjectAlternativeName")
        workflow_claim = certificate.get("sourceRepositoryWorkflow")
        if workflow not in str(san) and workflow not in str(workflow_claim):
            return False

    return {SLSA_PREDICATE_TYPE, SPDX_PREDICATE_TYPE}.issubset(predicates)


def _production_identity_valid(identity: Mapping[str, Any], provenance: Mapping[str, Any]) -> bool:
    policy = provenance.get("verification_policy", {})
    repository = provenance.get("repository")
    workflow = provenance.get("workflow", {}).get("path")
    ref = provenance.get("source", {}).get("ref")
    return (
        identity.get("class") == "production-ephemeral-oidc"
        and identity.get("issuer") == PRODUCTION_ISSUER
        and identity.get("repository") == repository
        and identity.get("workflow") == workflow
        and identity.get("ref") == ref
        and identity.get("subject") == f"repo:{repository}:ref:{ref}"
        and policy.get("production_identity_class") == "production-ephemeral-oidc"
        and policy.get("issuer") == PRODUCTION_ISSUER
        and policy.get("repository") == repository
        and policy.get("workflow") == workflow
        and ref is not None
        and str(ref).startswith("refs/tags/v")
    )


def verify_bundle(
    *,
    artifact_path: Path,
    sbom_path: Path,
    attestation_path: Path,
    provenance_path: Path,
    mode: str,
    external_verification_path: Path | None = None,
) -> dict[str, Any]:
    """Verify all local bindings and return a machine-readable fail-closed result."""

    inputs: dict[str, Any] = {}
    for label, path in (
        ("artifact", artifact_path),
        ("sbom", sbom_path),
        ("attestation", attestation_path),
        ("provenance", provenance_path),
    ):
        inputs[f"{label}_path"] = str(path)
        if path.is_file():
            inputs[f"{label}_sha256"] = sha256_file(path)
    if external_verification_path is not None and external_verification_path.is_file():
        inputs["external_verification_sha256"] = sha256_file(external_verification_path)
    if not artifact_path.is_file():
        return _invalid_result(["ARTIFACT_EVIDENCE_MISSING"], inputs, "unsupported")
    if not sbom_path.is_file():
        return _invalid_result(["SBOM_EVIDENCE_MISSING"], inputs, "unsupported")
    if not attestation_path.is_file():
        return _invalid_result(["ATTESTATION_EVIDENCE_MISSING"], inputs, "unsupported")
    if not provenance_path.is_file():
        return _invalid_result(["PROVENANCE_INVALID"], inputs, "unsupported")

    try:
        provenance = read_json(provenance_path)
        sbom = read_json(sbom_path)
        attestation = read_json(attestation_path)
    except ContractError:
        # The production actions/attest output is an in-toto bundle. Its
        # opaque bytes remain digest-bound here; GitHub CLI verification is
        # the authority for its signature and claims. Fixture mode requires
        # the normalized JSON form so tests cannot accidentally authorize it.
        if mode == "production":
            try:
                provenance = read_json(provenance_path)
                sbom = read_json(sbom_path)
            except ContractError:
                return _invalid_result(["PROVENANCE_INVALID"], inputs)
            attestation = {"opaque_production_bundle": True}
        else:
            return _invalid_result(["PROVENANCE_INVALID"], inputs)
    if not isinstance(provenance, dict) or not isinstance(sbom, dict):
        return _invalid_result(["PROVENANCE_INVALID"], inputs)
    if provenance.get("schema_version") != SCHEMA_VERSION:
        return _invalid_result(["SCHEMA_UNSUPPORTED"], inputs, "unsupported")
    if sbom.get("spdxVersion") != SBOM_SCHEMA_VERSION:
        return _invalid_result(["SBOM_SCHEMA_MISMATCH"], inputs)
    if not isinstance(attestation, dict):
        return _invalid_result(["ATTESTATION_SUBJECT_MISMATCH"], inputs)

    reasons: list[str] = []
    actual_artifact_sha = sha256_file(artifact_path)
    actual_sbom_sha = sha256_file(sbom_path)
    actual_attestation_sha = sha256_file(attestation_path)
    inputs["artifact_sha256"] = actual_artifact_sha
    inputs["sbom_sha256"] = actual_sbom_sha
    inputs["attestation_sha256"] = actual_attestation_sha
    inputs["provenance_sha256"] = sha256_file(provenance_path)

    try:
        artifact_record = provenance["artifacts"][0]
        expected_artifact_sha = artifact_record["digest"]["sha256"]
        expected_artifact_name = artifact_record["filename"]
        expected_artifact_size = artifact_record["size"]
        sbom_record = provenance["sbom"]
        attestation_record = provenance["attestation"]
        identity = attestation_record["identity"]
        source = provenance["source"]
        workflow = provenance["workflow"]
        target = provenance["target"]
        package = provenance["package"]
        dependencies = provenance["dependencies"]
        build_inputs = provenance["build_inputs"]
        rollback = provenance["rollback"]
        publication = provenance["publication"]
        policy = provenance["verification_policy"]
    except (KeyError, IndexError, TypeError):
        return _invalid_result(["PROVENANCE_INVALID"], inputs)

    if not all(
        isinstance(value, dict)
        for value in (artifact_record, sbom_record, attestation_record, identity, source, workflow, target, package, dependencies, rollback, publication, policy)
    ):
        return _invalid_result(["PROVENANCE_INVALID"], inputs)
    if not isinstance(build_inputs, list):
        return _invalid_result(["PROVENANCE_INVALID"], inputs)
    if not isinstance(identity, dict):
        return _invalid_result(["PROVENANCE_INVALID"], inputs)
    if not _safe_artifact_name(expected_artifact_name):
        reasons.append("PROVENANCE_INVALID")
    if package.get("kind") not in {"package", "container"}:
        reasons.append("PROVENANCE_INVALID")
    if package.get("name") != expected_artifact_name:
        reasons.append("SOURCE_BINDING_MISMATCH")
    if not isinstance(package.get("media_type"), str) or not package.get("media_type"):
        reasons.append("ARTIFACT_MEDIA_TYPE_MISMATCH")
    if artifact_record.get("media_type") != package.get("media_type"):
        reasons.append("ARTIFACT_MEDIA_TYPE_MISMATCH")
    if not isinstance(expected_artifact_sha, str) or not SHA256_RE.fullmatch(expected_artifact_sha):
        reasons.append("ARTIFACT_DIGEST_MISMATCH")
    if not isinstance(expected_artifact_size, int) or isinstance(expected_artifact_size, bool):
        reasons.append("ARTIFACT_SIZE_MISMATCH")
    if not isinstance(sbom_record, dict) or sbom_record.get("predicate_type") != SPDX_PREDICATE_TYPE:
        reasons.append("SBOM_SCHEMA_MISMATCH")
    if sbom_record.get("media_type") != "application/spdx+json":
        reasons.append("SBOM_SCHEMA_MISMATCH")
    if not isinstance(attestation_record, dict) or attestation_record.get("predicate_type") not in {
        SLSA_PREDICATE_TYPE,
        SPDX_PREDICATE_TYPE,
    }:
        reasons.append("ATTESTATION_PREDICATE_MISMATCH")
    if attestation_record.get("media_type") != "application/vnd.in-toto+json":
        reasons.append("ATTESTATION_PREDICATE_MISMATCH")
    if not isinstance(source, dict) or not COMMIT_RE.fullmatch(str(source.get("commit_sha", ""))):
        reasons.append("SOURCE_BINDING_MISMATCH")
    if not isinstance(source.get("ref"), str) or not source.get("ref"):
        reasons.append("SOURCE_BINDING_MISMATCH")
    if str(source.get("ref", "")).startswith("refs/tags/") and source.get("tag") != source.get("ref"):
        reasons.append("SOURCE_BINDING_MISMATCH")
    if not isinstance(workflow, dict) or not all(
        isinstance(workflow.get(key), str) and workflow.get(key)
        for key in ("path", "workflow_ref", "run_id", "job", "builder_id")
    ):
        reasons.append("WORKFLOW_BINDING_MISMATCH")
    if not isinstance(workflow.get("run_attempt"), int) or isinstance(workflow.get("run_attempt"), bool) or workflow.get("run_attempt") < 1:
        reasons.append("WORKFLOW_BINDING_MISMATCH")
    if not isinstance(target, dict) or not all(
        isinstance(target.get(key), str) and target.get(key)
        for key in ("os", "architecture", "triple")
    ):
        reasons.append("TARGET_BINDING_MISMATCH")
    if not isinstance(dependencies, dict) or not _descriptor_list_valid(dependencies.get("lockfiles")):
        reasons.append("DEPENDENCY_BINDING_MISMATCH")
    if not _descriptor_list_valid(build_inputs):
        reasons.append("DEPENDENCY_BINDING_MISMATCH")
    if not isinstance(publication, dict) or publication.get("external_action_authorized") is not False:
        reasons.append("VERIFICATION_POLICY_MISMATCH")
    if not isinstance(policy, dict) or policy != {
        "production_identity_class": "production-ephemeral-oidc",
        "issuer": PRODUCTION_ISSUER,
        "repository": provenance.get("repository"),
        "workflow": workflow.get("path"),
        "tag_ref_required": True,
        "unsigned_is_verified": False,
    }:
        reasons.append("VERIFICATION_POLICY_MISMATCH")

    if expected_artifact_sha != actual_artifact_sha:
        reasons.append("ARTIFACT_DIGEST_MISMATCH")
    if expected_artifact_name != artifact_path.name:
        reasons.append("SOURCE_BINDING_MISMATCH")
    if isinstance(expected_artifact_size, int) and expected_artifact_size != artifact_path.stat().st_size:
        reasons.append("ARTIFACT_SIZE_MISMATCH")
    if sbom_record.get("digest", {}).get("sha256") != actual_sbom_sha:
        reasons.append("SBOM_DIGEST_MISMATCH")
    if sbom_record.get("size") != sbom_path.stat().st_size:
        reasons.append("SBOM_DIGEST_MISMATCH")
    if sbom_record.get("schema") != SBOM_SCHEMA_VERSION:
        reasons.append("SBOM_SCHEMA_MISMATCH")
    if attestation_record.get("digest", {}).get("sha256") != actual_attestation_sha:
        reasons.append("ATTESTATION_DIGEST_MISMATCH")
    if attestation_record.get("size") != attestation_path.stat().st_size:
        reasons.append("ATTESTATION_DIGEST_MISMATCH")
    if attestation.get("schema_version") == ATTESTATION_SCHEMA_VERSION:
        subject = attestation.get("subject", {})
        if subject.get("digest", {}).get("sha256") != actual_artifact_sha:
            reasons.append("ATTESTATION_SUBJECT_MISMATCH")
        if attestation.get("sbom", {}).get("sha256") != actual_sbom_sha:
            reasons.append("ATTESTATION_SUBJECT_MISMATCH")
    if actual_artifact_sha not in str(sbom.get("documentNamespace", "")):
        reasons.append("SBOM_SUBJECT_MISMATCH")
    if actual_artifact_sha not in str(sbom.get("documentComment", "")):
        reasons.append("SBOM_SUBJECT_MISMATCH")
    source = provenance.get("source", {})
    workflow = provenance.get("workflow", {})
    if not COMMIT_RE.fullmatch(str(source.get("commit_sha", ""))):
        reasons.append("SOURCE_BINDING_MISMATCH")
    if not workflow.get("path") or not workflow.get("builder_id"):
        reasons.append("WORKFLOW_BINDING_MISMATCH")
    if not provenance.get("target", {}).get("triple"):
        reasons.append("TARGET_BINDING_MISMATCH")
    if not provenance.get("dependencies", {}).get("lockfiles"):
        reasons.append("DEPENDENCY_BINDING_MISMATCH")
    if not provenance.get("build_inputs"):
        reasons.append("DEPENDENCY_BINDING_MISMATCH")
    sbom_comment = str(sbom.get("documentComment", ""))
    for expected in (
        source.get("commit_sha"),
        source.get("ref"),
        workflow.get("path"),
        provenance.get("target", {}).get("triple"),
    ):
        if not expected or str(expected) not in sbom_comment:
            reasons.append("SOURCE_BINDING_MISMATCH")
    for lockfile in provenance.get("dependencies", {}).get("lockfiles", []):
        if not isinstance(lockfile, dict):
            reasons.append("DEPENDENCY_BINDING_MISMATCH")
            continue
        marker = f"{lockfile.get('path')}={lockfile.get('sha256')}"
        if marker not in sbom_comment:
            reasons.append("DEPENDENCY_BINDING_MISMATCH")
    rollback = provenance.get("rollback", {})
    if not rollback.get("previous_known_good") or not rollback.get("target"):
        reasons.append("ROLLBACK_TARGET_MISSING")
    if mode == "production" and (
        rollback.get("previous_known_good") in {"unknown", "not-published-dry-run"}
        or rollback.get("target") in {"unknown", "not-published-dry-run"}
    ):
        reasons.append("ROLLBACK_TARGET_MISSING")

    if mode not in {"fixture", "production"}:
        reasons.append("SCHEMA_UNSUPPORTED")
    if mode == "fixture":
        if identity.get("class") != "fixture":
            reasons.append("UNTRUSTED_IDENTITY")
        else:
            reasons.append("FIXTURE_IDENTITY_NON_AUTHORITATIVE")
    else:
        if identity.get("class") == "fixture" or not _production_identity_valid(identity, provenance):
            reasons.append("UNTRUSTED_IDENTITY")
        if external_verification_path is None or not external_verification_path.is_file():
            reasons.extend(
                ["ATTESTATION_NOT_EXTERNALLY_VERIFIED", "EXTERNAL_VERIFICATION_UNAVAILABLE"]
            )
        elif not _external_verification_is_valid(
            external_verification_path, provenance, actual_artifact_sha
        ):
            reasons.extend(
                ["ATTESTATION_NOT_EXTERNALLY_VERIFIED", "EXTERNAL_VERIFICATION_INVALID"]
            )

    if reasons:
        status = "verified_fixture" if mode == "fixture" and reasons == ["FIXTURE_IDENTITY_NON_AUTHORITATIVE"] else "rejected"
        return _invalid_result(reasons, inputs, status)
    return _invalid_result(
        ["VERIFIED_EXTERNAL_EPHEMERAL_IDENTITY"], inputs, "verified"
    )


def _metadata_from_args(args: argparse.Namespace, root: Path) -> dict[str, Any]:
    lockfiles = [_lockfile_descriptor(Path(path), root) for path in args.lockfile]
    build_inputs = [_lockfile_descriptor(Path(path), root) for path in args.build_input]
    metadata = {
        "repository": args.repository,
        "source_commit": args.source_commit,
        "ref": args.ref,
        "workflow": args.workflow,
        "workflow_ref": args.workflow_ref,
        "run_id": args.run_id,
        "run_attempt": args.run_attempt,
        "job": args.job,
        "builder_id": args.builder_id,
        "target_os": args.target_os,
        "target_architecture": args.target_architecture,
        "target_triple": args.target_triple,
        "package_kind": args.package_kind,
        "artifact_name": args.artifact_name,
        "artifact_media_type": args.artifact_media_type,
        "previous_known_good": args.previous_known_good,
        "rollback_target": args.rollback_target,
        "publication_mode": args.publication_mode,
        "lockfiles": lockfiles,
        "build_inputs": build_inputs,
    }
    rollback_file = getattr(args, "rollback_file", None)
    if rollback_file:
        rollback = read_json(Path(rollback_file))
        validate_rollback_state(rollback, metadata)
        metadata["rollback"] = rollback
    return metadata


def _components_from_lockfiles(metadata: Mapping[str, Any]) -> list[dict[str, str]]:
    components: list[dict[str, str]] = []
    for item in metadata.get("lockfiles", []):
        path = str(item.get("path", ""))
        if path.endswith("Cargo.lock"):
            components.append({"name": "engine", "version": "locked", "source": path})
        elif path.endswith("bun.lock"):
            components.append({"name": "javascript-dependencies", "version": "locked", "source": path})
        else:
            components.append({"name": Path(path).name, "version": "locked", "source": path})
    return components


def _common_parser(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--repository", required=True)
    parser.add_argument("--source-commit", required=True)
    parser.add_argument("--ref", required=True)
    parser.add_argument("--workflow", default=".github/workflows/release.yml")
    parser.add_argument("--workflow-ref", default="")
    parser.add_argument("--run-id", default="dry-run")
    parser.add_argument("--run-attempt", type=int, default=1)
    parser.add_argument("--job", default="dry-run")
    parser.add_argument("--builder-id", default="local-dry-run")
    parser.add_argument("--target-os", default="linux")
    parser.add_argument("--target-architecture", default="x86_64")
    parser.add_argument("--target-triple", default="x86_64-unknown-linux-gnu")
    parser.add_argument("--package-kind", default="package")
    parser.add_argument("--artifact-name", required=True)
    parser.add_argument("--artifact-media-type", default="application/vnd.acp.release+tar")
    parser.add_argument("--previous-known-good", default="unknown")
    parser.add_argument("--rollback-target", default="unknown")
    parser.add_argument("--rollback-file")
    parser.add_argument("--publication-mode", default="dry-run")
    parser.add_argument("--lockfile", action="append", default=[])
    parser.add_argument("--build-input", action="append", default=[])


def command_write_metadata(args: argparse.Namespace) -> int:
    root = Path(args.root).resolve()
    metadata = _metadata_from_args(args, root)
    write_canonical_json(Path(args.output), metadata)
    return 0


def command_create_sbom(args: argparse.Namespace) -> int:
    metadata = read_json(Path(args.metadata))
    artifact = Path(args.artifact)
    lockfiles = metadata.get("lockfiles")
    if not isinstance(lockfiles, list):
        raise ContractError("metadata lockfiles must be an array")
    inventory = load_dependency_inventory(
        Path(args.root), [_required_string(item, "path") for item in lockfiles]
    )
    if inventory["lockfiles"] != sorted(lockfiles, key=lambda item: item["path"]):
        raise ContractError("lockfiles changed after metadata generation")
    sbom = build_spdx_sbom(
        metadata=metadata,
        artifact_sha256=sha256_file(artifact),
        artifact_size=artifact.stat().st_size,
        inventory=inventory,
    )
    write_canonical_json(Path(args.output), sbom)
    return 0


def command_create_attestation(args: argparse.Namespace) -> int:
    metadata = read_json(Path(args.metadata))
    artifact = Path(args.artifact)
    sbom = Path(args.sbom)
    identity = fixture_identity(metadata) if args.identity == "fixture" else production_identity(metadata)
    attestation = build_attestation_fixture(
        metadata=metadata,
        artifact_sha256=sha256_file(artifact),
        sbom_sha256=sha256_file(sbom),
        identity=identity,
    )
    write_canonical_json(Path(args.output), attestation)
    return 0


def command_create_provenance(args: argparse.Namespace) -> int:
    metadata = read_json(Path(args.metadata))
    artifact = Path(args.artifact)
    sbom_path = Path(args.sbom)
    attestation_path = Path(args.attestation)
    try:
        attestation = read_json(attestation_path)
    except ContractError:
        attestation = None
    provenance = build_provenance(
        metadata=metadata,
        artifact_sha256=sha256_file(artifact),
        artifact_size=artifact.stat().st_size,
        sbom=read_json(sbom_path),
        sbom_path=sbom_path,
        attestation=attestation,
        attestation_path=attestation_path,
    )
    write_canonical_json(Path(args.output), provenance)
    return 0


def command_create_manifest(args: argparse.Namespace) -> int:
    metadata = read_json(Path(args.metadata))
    artifact = Path(args.artifact)
    sbom_path = Path(args.sbom)
    bootstrap = read_json(Path(args.bootstrap))
    if not isinstance(bootstrap, list):
        raise ContractError("bootstrap descriptor must be an array")
    manifest = build_release_manifest(
        metadata=metadata,
        artifact_sha256=sha256_file(artifact),
        artifact_size=artifact.stat().st_size,
        sbom=read_json(sbom_path),
        sbom_path=sbom_path,
        bootstrap_assets=bootstrap,
    )
    write_canonical_json(Path(args.output), manifest)
    return 0


def command_create_fixture_bundle(args: argparse.Namespace) -> int:
    metadata = read_json(Path(args.metadata))
    artifact = Path(args.artifact)
    predicate = read_json(Path(args.predicate)) if args.predicate else {
        "buildDefinition": {"buildType": "fixture"},
        "runDetails": {},
    }
    bundle = build_attestation_fixture(
        metadata=metadata,
        artifact_sha256=sha256_file(artifact),
        identity=fixture_identity(metadata),
        role=args.role,
        predicate_type=ATTESTATION_ROLES[args.role],
        predicate=predicate,
    )
    write_canonical_json(Path(args.output), bundle)
    return 0


def command_verify_release(args: argparse.Namespace) -> int:
    result = verify_release(
        artifact_path=Path(args.artifact),
        sbom_path=Path(args.sbom),
        manifest_path=Path(args.manifest),
        slsa_bundle_path=Path(args.slsa_bundle),
        spdx_bundle_path=Path(args.spdx_bundle),
        manifest_bundle_path=Path(args.manifest_bundle),
        mode=args.mode,
    )
    if args.output:
        write_canonical_json(Path(args.output), result)
    print(canonical_json_bytes(result).decode("utf-8"), end="")
    return 0 if result["status"] in {"verified", "verified_fixture"} else 1


def command_validate_archive(args: argparse.Namespace) -> int:
    summary = validate_release_archive(Path(args.archive), args.expected_top_level)
    print(canonical_json_bytes(summary).decode("utf-8"), end="")
    return 0


def command_extract_archive(args: argparse.Namespace) -> int:
    summary = extract_release_archive(
        Path(args.archive), Path(args.destination), args.expected_top_level
    )
    print(canonical_json_bytes(summary).decode("utf-8"), end="")
    return 0


def command_verify_bootstrap(args: argparse.Namespace) -> int:
    manifest = read_json(Path(args.manifest))
    if not isinstance(manifest, dict) or manifest.get("schema_version") != RELEASE_MANIFEST_SCHEMA_VERSION:
        raise ContractError("bootstrap verification requires release_provenance.v2")
    metadata = _manifest_metadata(manifest)
    if metadata["source_commit"] != args.source_commit:
        raise ContractError("bootstrap commit differs from release source commit")
    expected = manifest.get("bootstrap")
    if not isinstance(expected, list):
        raise ContractError("signed bootstrap records are missing")
    expected_by_name = {
        _required_string(item, "filename"): item
        for item in expected
        if isinstance(item, dict)
    }
    observed_names: set[str] = set()
    for descriptor in args.asset:
        if "=" not in descriptor:
            raise ContractError("bootstrap asset must use filename=path")
        filename, raw_path = descriptor.split("=", 1)
        if not _safe_artifact_name(filename) or filename in observed_names:
            raise ContractError("bootstrap asset name is invalid or duplicated")
        observed_names.add(filename)
        path = Path(raw_path)
        record = expected_by_name.get(filename)
        if record is None or not path.is_file():
            raise ContractError(f"signed bootstrap asset is missing: {filename}")
        if (
            record.get("sha256") != sha256_file(path)
            or record.get("source_commit") != args.source_commit
            or record.get("predicate_type") != SLSA_PREDICATE_TYPE
        ):
            raise ContractError(f"bootstrap asset binding differs: {filename}")
    if observed_names != set(expected_by_name):
        raise ContractError("every signed bootstrap asset must be supplied exactly once")
    return 0


def command_validate_previous_release(args: argparse.Namespace) -> int:
    current = read_json(Path(args.current_manifest))
    previous = read_json(Path(args.previous_manifest))
    if not isinstance(current, dict) or not isinstance(previous, dict):
        raise ContractError("current and previous manifests must be objects")
    if (
        current.get("schema_version") != RELEASE_MANIFEST_SCHEMA_VERSION
        or previous.get("schema_version") != RELEASE_MANIFEST_SCHEMA_VERSION
    ):
        raise ContractError("rollback compatibility requires v2 manifests")
    current_metadata = _manifest_metadata(current)
    previous_metadata = _manifest_metadata(previous)
    rollback = current.get("rollback")
    validate_rollback_state(rollback, current_metadata)
    if rollback.get("state") != "previous_release":
        raise ContractError("previous release validation requires previous_release state")
    record = rollback["previous"]
    artifact_record = record["artifact"]
    previous_artifact = Path(args.previous_artifact)
    previous_manifest_artifact = previous.get("artifact")
    previous_source = previous.get("source")
    previous_target = previous.get("target")
    if not all(
        isinstance(value, dict)
        for value in (previous_manifest_artifact, previous_source, previous_target)
    ):
        raise ContractError("previous manifest bindings are malformed")
    if not previous_artifact.is_file() or (
        artifact_record.get("filename") != previous_artifact.name
        or artifact_record.get("sha256") != sha256_file(previous_artifact)
        or previous_manifest_artifact.get("filename") != previous_artifact.name
        or previous_manifest_artifact.get("sha256") != sha256_file(previous_artifact)
    ):
        raise ContractError("previous artifact identity or digest differs")
    if (
        record.get("tag") != previous_source.get("tag")
        or record.get("source_commit") != previous_source.get("commit_sha")
        or record.get("target_triple") != previous_metadata["target_triple"]
        or record.get("package_kind") != previous_metadata["package_kind"]
        or previous_metadata["target_triple"] != current_metadata["target_triple"]
        or previous_metadata["package_kind"] != current_metadata["package_kind"]
    ):
        raise ContractError("previous release source or target is incompatible")
    return 0


def command_verify(args: argparse.Namespace) -> int:
    result = verify_bundle(
        artifact_path=Path(args.artifact),
        sbom_path=Path(args.sbom),
        attestation_path=Path(args.attestation),
        provenance_path=Path(args.provenance),
        mode=args.mode,
        external_verification_path=Path(args.external_verification)
        if args.external_verification
        else None,
    )
    if args.output:
        write_canonical_json(Path(args.output), result)
    print(canonical_json_bytes(result).decode("utf-8"), end="")
    return 0 if result["status"] in {"verified", "verified_fixture"} else 1


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    metadata = subparsers.add_parser("write-metadata")
    _common_parser(metadata)
    metadata.add_argument("--root", default=".")
    metadata.add_argument("--output", required=True)
    metadata.set_defaults(function=command_write_metadata)

    sbom = subparsers.add_parser("create-sbom")
    sbom.add_argument("--metadata", required=True)
    sbom.add_argument("--artifact", required=True)
    sbom.add_argument("--root", default=".")
    sbom.add_argument("--output", required=True)
    sbom.set_defaults(function=command_create_sbom)

    attestation = subparsers.add_parser("create-attestation")
    attestation.add_argument("--metadata", required=True)
    attestation.add_argument("--artifact", required=True)
    attestation.add_argument("--sbom", required=True)
    attestation.add_argument("--output", required=True)
    attestation.add_argument("--identity", choices=("fixture", "production"), default="fixture")
    attestation.set_defaults(function=command_create_attestation)

    provenance = subparsers.add_parser("create-provenance")
    provenance.add_argument("--metadata", required=True)
    provenance.add_argument("--artifact", required=True)
    provenance.add_argument("--sbom", required=True)
    provenance.add_argument("--attestation", required=True)
    provenance.add_argument("--output", required=True)
    provenance.set_defaults(function=command_create_provenance)

    manifest = subparsers.add_parser("create-manifest")
    manifest.add_argument("--metadata", required=True)
    manifest.add_argument("--artifact", required=True)
    manifest.add_argument("--sbom", required=True)
    manifest.add_argument("--bootstrap", required=True)
    manifest.add_argument("--output", required=True)
    manifest.set_defaults(function=command_create_manifest)

    fixture_bundle = subparsers.add_parser("create-fixture-bundle")
    fixture_bundle.add_argument("--metadata", required=True)
    fixture_bundle.add_argument("--artifact", required=True)
    fixture_bundle.add_argument("--role", choices=tuple(ATTESTATION_ROLES), required=True)
    fixture_bundle.add_argument("--predicate")
    fixture_bundle.add_argument("--output", required=True)
    fixture_bundle.set_defaults(function=command_create_fixture_bundle)

    verify = subparsers.add_parser("verify")
    verify.add_argument("--artifact", required=True)
    verify.add_argument("--sbom", required=True)
    verify.add_argument("--attestation", required=True)
    verify.add_argument("--provenance", required=True)
    verify.add_argument("--mode", choices=("fixture", "production"), required=True)
    verify.add_argument("--external-verification")
    verify.add_argument("--output")
    verify.set_defaults(function=command_verify)

    verify_release_parser = subparsers.add_parser("verify-release")
    verify_release_parser.add_argument("--artifact", required=True)
    verify_release_parser.add_argument("--sbom", required=True)
    verify_release_parser.add_argument("--manifest", required=True)
    verify_release_parser.add_argument("--slsa-bundle", required=True)
    verify_release_parser.add_argument("--spdx-bundle", required=True)
    verify_release_parser.add_argument("--manifest-bundle", required=True)
    verify_release_parser.add_argument(
        "--mode", choices=("fixture", "production"), required=True
    )
    verify_release_parser.add_argument("--output")
    verify_release_parser.set_defaults(function=command_verify_release)

    validate_archive_parser = subparsers.add_parser("validate-archive")
    validate_archive_parser.add_argument("--archive", required=True)
    validate_archive_parser.add_argument("--expected-top-level", required=True)
    validate_archive_parser.set_defaults(function=command_validate_archive)

    extract_archive_parser = subparsers.add_parser("extract-archive")
    extract_archive_parser.add_argument("--archive", required=True)
    extract_archive_parser.add_argument("--destination", required=True)
    extract_archive_parser.add_argument("--expected-top-level", required=True)
    extract_archive_parser.set_defaults(function=command_extract_archive)

    verify_bootstrap_parser = subparsers.add_parser("verify-bootstrap")
    verify_bootstrap_parser.add_argument("--manifest", required=True)
    verify_bootstrap_parser.add_argument("--source-commit", required=True)
    verify_bootstrap_parser.add_argument("--asset", action="append", default=[], required=True)
    verify_bootstrap_parser.set_defaults(function=command_verify_bootstrap)

    previous_release_parser = subparsers.add_parser("validate-previous-release")
    previous_release_parser.add_argument("--current-manifest", required=True)
    previous_release_parser.add_argument("--previous-manifest", required=True)
    previous_release_parser.add_argument("--previous-artifact", required=True)
    previous_release_parser.set_defaults(function=command_validate_previous_release)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        return int(args.function(args))
    except (ContractError, OSError, ValueError) as exc:
        print(f"release provenance contract failed: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
