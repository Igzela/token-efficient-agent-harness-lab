"""Live-state validation before delivery and before comment posting (pure logic).

The caller supplies observed facts (PR state, diff, evidence index); this
module decides only whether the facts satisfy the request contract.  It never
fetches anything itself, which keeps CI provider-free and tests deterministic.
"""

from __future__ import annotations

import hashlib
import json
import os
import pathlib
from typing import Any


def validate_pr_live_state(
    *,
    repository: str,
    pr_number: int,
    observed_repository: str,
    observed_pr_number: int,
    observed_state: str,
    observed_is_draft: bool,
    observed_base_sha: str,
    observed_head_sha: str,
    expected_base_sha: str,
    expected_head_sha: str,
    observed_merged: bool,
) -> list[str]:
    """Reject drift between the envelope and the live PR before delivery/post.

    The observed identity (repository, PR number) comes from the read-only
    adapter's returned facts, never from the caller, so a misrouted or cached
    fetch cannot be accepted (R2-B8).

    An open unmerged Draft PR with unchanged base/head is the only accepted
    live state for evidence delivery and receipt posting.
    """
    errors = []
    if observed_repository != repository:
        errors.append(f"observed repository mismatch: {observed_repository} != {repository}")
    if observed_pr_number != pr_number:
        errors.append(f"observed PR mismatch: {observed_pr_number} != {pr_number}")
    if observed_state != "OPEN":
        errors.append(f"PR is not OPEN: {observed_state}")
    if observed_merged:
        errors.append("PR is merged")
    if observed_is_draft is not True:
        errors.append("PR is not a Draft")
    if observed_base_sha != expected_base_sha:
        errors.append(f"base drifted {observed_base_sha} != {expected_base_sha}")
    if observed_head_sha != expected_head_sha:
        errors.append(f"head drifted {observed_head_sha} != {expected_head_sha}")
    return errors


def validate_diff_scope(
    changed_files: list[str],
    allowed_paths: tuple[str, ...],
    *,
    min_files: int = 1,
) -> list[str]:
    """The reviewed diff must be complete and stay inside allowed paths."""
    errors = []
    if len(changed_files) < min_files:
        errors.append(f"diff has fewer than {min_files} changed files")
    for path in changed_files:
        if path not in allowed_paths:
            errors.append(f"changed file outside allowed paths: {path}")
    return errors


def sha256_file(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(65536), b""):
            digest.update(chunk)
    return digest.hexdigest()


def check_symlink_escape(root: pathlib.Path, paths: list[str]) -> list[str]:
    """Reject any path that escapes the evidence root or is a symlink.

    Checks every component: a `..` traversal or a symlinked parent must fail.
    """
    errors = []
    resolved_root = root.resolve()
    for rel in paths:
        if ".." in pathlib.PurePosixPath(rel).parts:
            errors.append(f"path contains ..: {rel}")
            continue
        candidate = (root / rel)
        if not candidate.exists() and not candidate.is_symlink():
            errors.append(f"evidence path missing: {rel}")
            continue
        resolved = candidate.resolve()
        if not resolved.is_relative_to(resolved_root):
            errors.append(f"path escapes evidence root: {rel}")
            continue
        if candidate.is_symlink():
            errors.append(f"path is a symlink: {rel}")
            continue
        for parent in candidate.parents:
            if parent.is_symlink():
                errors.append(f"parent component is a symlink: {parent}")
                break
    return errors


def validate_evidence_index(
    index_path: pathlib.Path,
    expected_index_sha256: str,
    *,
    max_files: int = 200,
    max_total_bytes: int = 32 * 1024 * 1024,
) -> tuple[list[str], dict[str, Any]]:
    """Validate the hash-bound evidence index and all referenced files.

    Returns (errors, index_dict).  All files must exist, be regular files,
    not symlinks, stay under the evidence root, and match their recorded
    sha256.  Fails closed on missing/oversized/ambiguous entries.
    """
    errors: list[str] = []
    if not index_path.exists():
        return ["evidence index missing"], {}
    if index_path.is_symlink():
        return ["evidence index is a symlink"], {}
    actual = sha256_file(index_path)
    if actual != expected_index_sha256:
        errors.append(f"evidence index sha256 mismatch {actual[:12]}... != {expected_index_sha256[:12]}...")
    try:
        data = json.loads(index_path.read_text(encoding="utf-8"))
    except Exception as exc:
        return [f"evidence index unreadable: {exc}"], {}
    if not isinstance(data, dict):
        return ["evidence index is not an object"], {}
    entries = data.get("files")
    if not isinstance(entries, list) or not entries:
        return ["evidence index has no file entries"], {}
    if len(entries) > max_files:
        errors.append(f"evidence index has too many entries: {len(entries)}")
    root = index_path.resolve().parent
    total = 0
    seen = set()
    rel_paths = []
    for entry in entries:
        if not isinstance(entry, dict):
            errors.append("evidence entry is not an object")
            continue
        rel = entry.get("path")
        digest = entry.get("sha256")
        if not isinstance(rel, str) or not isinstance(digest, str):
            errors.append(f"invalid evidence entry: {entry!r}")
            continue
        rel_paths.append(rel)
        if rel in seen:
            errors.append(f"duplicate evidence path: {rel}")
            continue
        seen.add(rel)
        if ".." in pathlib.PurePosixPath(rel).parts:
            errors.append(f"evidence path contains ..: {rel}")
            continue
        candidate = root / rel
        if not candidate.exists() and not candidate.is_symlink():
            errors.append(f"evidence file missing: {rel}")
            continue
        resolved = candidate.resolve()
        if not resolved.is_relative_to(root.resolve()):
            errors.append(f"evidence file escapes root: {rel}")
            continue
        if candidate.is_symlink():
            errors.append(f"evidence file is a symlink: {rel}")
            continue
        for parent in candidate.parents:
            if parent.is_symlink():
                errors.append(f"evidence parent component is a symlink: {rel}")
                break
        if not candidate.is_file():
            errors.append(f"evidence path is not a regular file: {rel}")
            continue
        try:
            size = candidate.stat().st_size
        except OSError as exc:
            errors.append(f"evidence file unreadable {rel}: {exc}")
            continue
        total += size
        if sha256_file(candidate) != digest:
            errors.append(f"evidence file sha256 mismatch: {rel}")
    if total > max_total_bytes:
        errors.append(f"evidence total size exceeds limit: {total}")
    return errors, data
