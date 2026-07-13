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
import sys
from pathlib import Path
from typing import Any, Iterable, Mapping


SCHEMA_VERSION = "release_provenance.v1"
SBOM_SCHEMA_VERSION = "SPDX-2.3"
ATTESTATION_SCHEMA_VERSION = "acp_attestation_fixture.v1"
VERIFICATION_SCHEMA_VERSION = "release_verification.v1"
TOOL_VERSION = "acp-release-provenance/1"
SPDX_PREDICATE_TYPE = "https://spdx.dev/Document/v2.3"
SLSA_PREDICATE_TYPE = "https://slsa.dev/provenance/v1"
PRODUCTION_ISSUER = "https://token.actions.githubusercontent.com"
MAX_JSON_BYTES = 16 * 1024 * 1024
MAX_STRING_BYTES = 4096
MAX_ARRAY_ITEMS = 4096
MAX_OBJECT_FIELDS = 256
MAX_JSON_DEPTH = 16
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
COMMIT_RE = re.compile(r"^[0-9a-f]{40}$")

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
    components: Iterable[Mapping[str, Any]],
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
    for component in components:
        name = _required_string(component, "name")
        version = _required_string(component, "version")
        source = _required_string(component, "source")
        normalized.append({"name": name, "version": version, "source": source})
    normalized.sort(key=_component_sort_key)

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
        }
    )
    relationships = [
        {
            "spdxElementId": "SPDXRef-DOCUMENT",
            "relationshipType": "DESCRIBES",
            "relatedSpdxElement": subject_id,
        }
    ]
    seen: set[tuple[str, str, str]] = set()
    for component in normalized:
        key = (component["name"], component["version"], component["source"])
        if key in seen:
            continue
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
                "externalRefs": [
                    {
                        "referenceCategory": "OTHER",
                        "referenceLocator": component["source"],
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

    lockfiles = metadata.get("lockfiles", [])
    if not isinstance(lockfiles, list):
        raise ContractError("lockfiles must be an array")
    lock_text = ";".join(
        f"{item.get('path')}={item.get('sha256')}" for item in lockfiles
    )
    comment = (
        f"acp.release.v1 repository={repository};source_commit={commit};"
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
        "documentNamespace": f"https://github.com/{repository}/sbom/{artifact_sha256}",
        "name": f"{artifact_name}.spdx",
        "packages": packages,
        "relationships": relationships,
    }


def build_attestation_fixture(
    *,
    metadata: Mapping[str, Any],
    artifact_sha256: str,
    sbom_sha256: str,
    identity: Mapping[str, Any],
) -> dict[str, Any]:
    _required_sha({"sha256": artifact_sha256}, "sha256")
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
    return {
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
    sbom = build_spdx_sbom(
        metadata=metadata,
        artifact_sha256=sha256_file(artifact),
        artifact_size=artifact.stat().st_size,
        components=_components_from_lockfiles(metadata),
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

    verify = subparsers.add_parser("verify")
    verify.add_argument("--artifact", required=True)
    verify.add_argument("--sbom", required=True)
    verify.add_argument("--attestation", required=True)
    verify.add_argument("--provenance", required=True)
    verify.add_argument("--mode", choices=("fixture", "production"), required=True)
    verify.add_argument("--external-verification")
    verify.add_argument("--output")
    verify.set_defaults(function=command_verify)
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
