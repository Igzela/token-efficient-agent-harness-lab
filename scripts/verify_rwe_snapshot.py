#!/usr/bin/env python3
"""Verify the hash-bound, provider-free pre-AC snapshot inputs."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
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


def git_output(root: Path, *args: str) -> str | None:
    result = subprocess.run(
        ["git", "-C", str(root), *args],
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode:
        return None
    return result.stdout.strip()


def git_blob(root: Path, revision: str, relative: str) -> bytes | None:
    result = subprocess.run(
        ["git", "-C", str(root), "show", f"{revision}:{relative}"],
        check=False,
        capture_output=True,
    )
    return result.stdout if result.returncode == 0 else None


def git_tree_hash(root: Path, revision: str) -> str | None:
    result = subprocess.run(
        ["git", "-C", str(root), "ls-tree", "-r", "-z", "--full-tree", revision],
        check=False,
        capture_output=True,
    )
    if result.returncode:
        return None
    entries: list[dict[str, object]] = []
    for record in result.stdout.split(b"\0"):
        if not record:
            continue
        header, relative_bytes = record.split(b"\t", 1)
        mode, object_type, object_id = header.split()
        if object_type != b"blob" or mode not in {b"100644", b"100755"}:
            return None
        relative = relative_bytes.decode("utf-8")
        content = git_blob(root, revision, relative)
        if content is None:
            return None
        entries.append(
            {
                "relative_path": relative,
                "sha256": hashlib.sha256(content).hexdigest(),
                "executable": mode == b"100755",
            }
        )
    entries.sort(key=lambda item: item["relative_path"])
    encoded = json.dumps(entries, separators=(",", ":")).encode()
    return hashlib.sha256(encoded).hexdigest()


def required_manifest_failures(manifest: dict[str, object]) -> list[str]:
    failures: list[str] = []
    required = {
        "repository": ("source_commit", "source_tree_hash", "source_tree_hash_method", "reconstruction_base_tree_hash", "reconstruction_recipe_tree_hash"),
        "reconstruction_recipe": ("base_commit", "recipe_commit", "recipe_paths", "recipe_sha256"),
        "build_inputs": ("dependency_lockfiles", "tracked_configuration", "source_configuration", "source_dependency_lockfiles"),
        "generated_active_baseline": ("files",),
        "frozen_rwe": (
            "frozen_task_source_tree_hash",
            "protocol_path",
            "protocol_file_sha256",
            "schedule_path",
            "schedule_file_sha256",
            "task_definitions",
        ),
    }
    for section, fields in required.items():
        value = manifest.get(section)
        if not isinstance(value, dict):
            failures.append(f"manifest section is missing or malformed: {section}")
            continue
        for field in fields:
            if field not in value:
                failures.append(f"manifest field is missing: {section}.{field}")
    return failures


def verify_git_overlay(source_root: Path, manifest: dict[str, object], failures: list[str]) -> None:
    repository = manifest["repository"]
    recipe = manifest["reconstruction_recipe"]
    base_commit = repository["source_commit"]
    if recipe["base_commit"] != base_commit:
        failures.append("recipe base commit differs from source commit")
        return
    recipe_commit = recipe["recipe_commit"]
    if git_output(source_root, "rev-parse", "HEAD") != base_commit:
        failures.append("source checkout HEAD differs from the bound base commit")
    base_tree_hash = git_tree_hash(source_root, base_commit)
    if base_tree_hash != repository["source_tree_hash"]:
        failures.append("source tree hash differs from the bound source_tree_hash")
    if base_tree_hash != repository["reconstruction_base_tree_hash"]:
        failures.append("source base tree hash differs from the bound reconstruction hash")
    if git_tree_hash(source_root, recipe_commit) != repository["reconstruction_recipe_tree_hash"]:
        failures.append("recipe tree hash differs from the bound reconstruction hash")
    if git_output(source_root, "cat-file", "-e", f"{recipe_commit}^{{commit}}") is None:
        failures.append("bound recipe commit is unavailable")
        return
    if git_output(source_root, "merge-base", "--is-ancestor", base_commit, recipe_commit) is not None:
        pass
    else:
        failures.append("recipe commit is not descended from the bound base commit")
    paths = sorted(recipe["recipe_paths"])
    recipe_changed = git_output(source_root, "diff", "--name-only", base_commit, recipe_commit)
    if recipe_changed is None or sorted(filter(None, recipe_changed.splitlines())) != paths:
        failures.append("recipe commit changes differ from the recipe path set")
    tracked = git_output(source_root, "diff", "--name-only") or ""
    untracked = git_output(source_root, "ls-files", "--others", "--exclude-standard") or ""
    generated_prefix = "apps/api/src/alters_lab_api.egg-info/"
    changed = sorted(
        path
        for path in {*tracked.splitlines(), *untracked.splitlines()}
        if path and not path.startswith(generated_prefix)
    )
    if changed != paths:
        failures.append("source checkout overlay paths differ from the recipe path set")
    for relative in paths:
        candidate = source_root / relative
        expected = git_blob(source_root, recipe_commit, relative)
        if expected is None or not candidate.is_file() or candidate.read_bytes() != expected:
            failures.append(f"source checkout overlay differs from the bound recipe commit: {relative}")


def verify_frozen_task_bindings(harness_root: Path, manifest: dict[str, object], failures: list[str]) -> None:
    frozen = manifest["frozen_rwe"]
    expected = frozen["frozen_task_source_tree_hash"]
    expected_commit = manifest["repository"]["source_commit"]
    for item in frozen["task_definitions"]:
        verify_file(harness_root, item["path"], item["sha256"], failures)
    verify_file(harness_root, frozen["protocol_path"], frozen["protocol_file_sha256"], failures)
    verify_file(harness_root, frozen["schedule_path"], frozen["schedule_file_sha256"], failures)
    try:
        protocol = json.loads((harness_root / frozen["protocol_path"]).read_text(encoding="utf-8"))
        schedule = json.loads((harness_root / frozen["schedule_path"]).read_text(encoding="utf-8"))
        if protocol.get("authority_corpus_sha256") != manifest["frozen_rwe"]["corpus_sha256"]:
            failures.append("protocol corpus binding differs")
        if schedule.get("corpus_sha256") != manifest["frozen_rwe"]["corpus_sha256"]:
            failures.append("schedule corpus binding differs")
        if schedule.get("protocol_sha256") != manifest["frozen_rwe"]["protocol_sha256"]:
            failures.append("schedule protocol binding differs")
        if schedule.get("schedule_sha256") != manifest["frozen_rwe"]["schedule_sha256"]:
            failures.append("schedule identity binding differs")
    except (OSError, json.JSONDecodeError):
        failures.append("frozen protocol or schedule is unreadable")
    task_root = harness_root / "engine/rwe/corpora/rwe-minimum-first-corpus/v2/tasks"
    task_paths = sorted(task_root.glob("*.json"))
    if not task_paths:
        failures.append("frozen task definitions are unavailable")
        return
    for path in task_paths:
        try:
            task = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            failures.append(f"frozen task definition is unreadable: {path.name}")
            continue
        if task.get("source_commit") != expected_commit:
            failures.append(f"frozen task source commit differs: {path.name}")
        if task.get("source_tree_hash") != expected:
            failures.append(f"frozen task source tree binding differs: {path.name}")


def verify(manifest_path: Path, source_root: Path, harness_root: Path) -> list[str]:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    failures: list[str] = []
    failures.extend(required_manifest_failures(manifest))
    expected_manifest = manifest.get("manifest_sha256")
    if not isinstance(expected_manifest, str):
        failures.append("manifest_sha256 is missing")
    elif canonical_manifest_digest(manifest) != expected_manifest:
        failures.append("snapshot manifest canonical digest mismatch")

    if not failures:
        try:
            verify_git_overlay(source_root, manifest, failures)
            verify_frozen_task_bindings(harness_root, manifest, failures)
        except (KeyError, TypeError):
            failures.append("snapshot Git overlay binding is malformed")

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
