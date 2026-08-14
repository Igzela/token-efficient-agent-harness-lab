#!/usr/bin/env python3
"""Verify the hash-bound, provider-free pre-AC snapshot inputs."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_manifest_digest(manifest: dict[str, object]) -> str:
    body = dict(manifest)
    body.pop("manifest_sha256", None)
    encoded = json.dumps(body, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def verify_file(root: Path, relative: str, expected: str, failures: list[str]) -> None:
    candidate = root / relative
    try:
        resolved = candidate.resolve(strict=True)
        resolved.relative_to(root)
    except (OSError, ValueError):
        failures.append(f"missing or escaping snapshot input: {relative}")
        return
    if candidate.is_symlink() or not resolved.is_file():
        failures.append(f"snapshot input is not a regular file: {relative}")
        return
    actual = sha256_file(resolved)
    if actual != expected:
        failures.append(f"snapshot hash mismatch: {relative}")


def verify(manifest_path: Path, source_root: Path, harness_root: Path) -> list[str]:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    failures: list[str] = []
    expected_manifest = manifest.get("manifest_sha256")
    if not isinstance(expected_manifest, str):
        failures.append("manifest_sha256 is missing")
    elif canonical_manifest_digest(manifest) != expected_manifest:
        failures.append("snapshot manifest canonical digest mismatch")

    build_inputs = manifest.get("build_inputs", {})
    for section in ("dependency_lockfiles", "tracked_configuration"):
        for item in build_inputs.get(section, []):
            verify_file(harness_root, item["path"], item["sha256"], failures)
    for item in build_inputs.get("source_configuration", []):
        verify_file(source_root, item["path"], item["sha256"], failures)
    for relative in build_inputs.get("source_dependency_lockfiles", {}).get("paths", []):
        verify_file(
            source_root,
            relative,
            build_inputs["source_dependency_lockfiles"]["sha256"],
            failures,
        )

    recipe = manifest.get("reconstruction_recipe", {})
    for relative, expected in recipe.get("recipe_sha256", {}).items():
        verify_file(source_root, relative, expected, failures)
    for relative, expected in manifest.get("generated_active_baseline", {}).get("files", {}).items():
        verify_file(source_root, relative, expected, failures)
    return failures


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--harness-root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    try:
        failures = verify(
            args.manifest.resolve(), args.source_root.resolve(), args.harness_root.resolve()
        )
    except (OSError, KeyError, TypeError, json.JSONDecodeError) as error:
        print(f"snapshot verification failed: {error}", file=sys.stderr)
        return 1
    if failures:
        print("\n".join(f"snapshot verification failed: {failure}" for failure in failures), file=sys.stderr)
        return 1
    print("RWE snapshot verification passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
