"""Create and validate the untrusted Codex patch artifact contract."""

from __future__ import annotations

import hashlib
import json
import re
import shlex
import subprocess
import sys
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1
PATCH_NAME = "agent.patch"
MANIFEST_NAME = "agent-result.json"
WORKER_TYPES = frozenset({"implementation", "ci-repair"})
FORBIDDEN_PREFIXES = (".git/", ".codex/", ".github/workflows/", ".github/actions/")
ISSUE_SCOPE_PATTERN = re.compile(r"<!--\s*agent-orchestrator-scope:v1\s*(\{.*?\})\s*-->", re.DOTALL)


class ArtifactContractError(RuntimeError):
    """Raised when an untrusted artifact fails deterministic validation."""


def _git(repo: Path, *args: str) -> bytes:
    result = subprocess.run(["git", *args], cwd=repo, capture_output=True, timeout=30)
    if result.returncode != 0:
        raise ArtifactContractError(result.stderr.decode(errors="replace").strip() or "git command failed")
    return result.stdout


def _safe_path(path: str) -> bool:
    return bool(path) and not path.startswith("/") and "\\" not in path and ".." not in Path(path).parts and not path.startswith(FORBIDDEN_PREFIXES)


def _changed_files(repo: Path) -> list[str]:
    raw = _git(repo, "diff", "--cached", "--name-only", "-z")
    paths = [part.decode("utf-8", errors="strict") for part in raw.split(b"\0") if part]
    if paths != sorted(set(paths)) or not all(_safe_path(path) for path in paths):
        raise ArtifactContractError("staged paths are unsafe or non-deterministic")
    return paths


def _patch_paths(patch: bytes) -> list[str]:
    paths: set[str] = set()
    for line in patch.decode("utf-8", errors="surrogateescape").splitlines():
        if not line.startswith("diff --git "):
            continue
        try:
            before, after = shlex.split(line[len("diff --git ") :])
        except ValueError as exc:
            raise ArtifactContractError("patch diff header is malformed") from exc
        for candidate in (before, after):
            if candidate.startswith(("a/", "b/")):
                paths.add(candidate[2:])
    result = sorted(paths)
    if not result or not all(_safe_path(path) for path in result):
        raise ArtifactContractError("patch changes forbidden or invalid paths")
    return result


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def create_artifact(
    *,
    repo: Path,
    artifact_dir: Path,
    worker_type: str,
    issue_number: int,
    pr_number: int,
    base_sha: str,
    expected_remote_sha: str | None,
    branch: str,
    codex_exit_code: int,
    local_checks: list[dict[str, Any]],
    failed_run_id: int | None = None,
    repair_attempt: int | None = None,
) -> dict[str, Any]:
    if worker_type not in WORKER_TYPES:
        raise ArtifactContractError("unknown worker type")
    if _git(repo, "rev-parse", "HEAD").decode().strip() != base_sha:
        raise ArtifactContractError("worktree HEAD moved after the trusted base was recorded")
    if branch != f"agent/issue-{issue_number}":
        raise ArtifactContractError("artifact branch is not canonical for its Issue")
    _git(repo, "add", "-A")
    patch = _git(repo, "diff", "--cached", "--binary", "--full-index")
    changed_files = _changed_files(repo)
    if not changed_files:
        raise ArtifactContractError("Codex produced no staged changes")
    if changed_files != _patch_paths(patch):
        raise ArtifactContractError("staged file list does not match binary patch")
    artifact_dir.mkdir(parents=True, exist_ok=True)
    (artifact_dir / PATCH_NAME).write_bytes(patch)
    manifest: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "worker_type": worker_type,
        "issue_number": issue_number,
        "pr_number": pr_number,
        "base_sha": base_sha,
        "expected_remote_sha": expected_remote_sha,
        "branch": branch,
        "changed_files": changed_files,
        "file_count": len(changed_files),
        "patch_sha256": _sha256(patch),
        "patch_size_bytes": len(patch),
        "codex_exit_code": codex_exit_code,
        "local_checks": local_checks,
    }
    if worker_type == "ci-repair":
        if failed_run_id is None or repair_attempt is None:
            raise ArtifactContractError("CI repair artifact lacks failed-run binding")
        manifest.update({"failed_run_id": failed_run_id, "repair_attempt": repair_attempt})
    (artifact_dir / MANIFEST_NAME).write_text(json.dumps(manifest, sort_keys=True) + "\n")
    return manifest


def _validate_manifest(manifest: Any) -> dict[str, Any]:
    if not isinstance(manifest, dict):
        raise ArtifactContractError("artifact manifest is not an object")
    required = {
        "schema_version", "worker_type", "issue_number", "pr_number", "base_sha", "expected_remote_sha",
        "branch", "changed_files", "file_count", "patch_sha256", "patch_size_bytes", "codex_exit_code", "local_checks",
    }
    if not required <= set(manifest):
        raise ArtifactContractError("artifact manifest misses required fields")
    if manifest["schema_version"] != SCHEMA_VERSION or manifest["worker_type"] not in WORKER_TYPES:
        raise ArtifactContractError("artifact schema version or worker type is invalid")
    if not all(isinstance(manifest[key], int) for key in ("issue_number", "pr_number", "file_count", "patch_size_bytes", "codex_exit_code")):
        raise ArtifactContractError("artifact numeric fields are invalid")
    if not isinstance(manifest["base_sha"], str) or not re.fullmatch(r"[0-9a-f]{40}", manifest["base_sha"]):
        raise ArtifactContractError("artifact base SHA is invalid")
    if manifest["expected_remote_sha"] is not None and not isinstance(manifest["expected_remote_sha"], str):
        raise ArtifactContractError("artifact expected remote SHA is invalid")
    if not isinstance(manifest["branch"], str) or not isinstance(manifest["changed_files"], list):
        raise ArtifactContractError("artifact branch or changed files are invalid")
    if manifest["branch"] != f"agent/issue-{manifest['issue_number']}":
        raise ArtifactContractError("artifact branch is not canonical for its Issue")
    paths = manifest["changed_files"]
    if paths != sorted(set(paths)) or not all(isinstance(path, str) and _safe_path(path) for path in paths):
        raise ArtifactContractError("artifact changed paths are invalid")
    if manifest["file_count"] != len(paths) or manifest["patch_size_bytes"] < 1:
        raise ArtifactContractError("artifact file count or patch size is invalid")
    if not isinstance(manifest["patch_sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", manifest["patch_sha256"]):
        raise ArtifactContractError("artifact checksum is invalid")
    if not isinstance(manifest["local_checks"], list):
        raise ArtifactContractError("artifact local checks are invalid")
    if manifest["worker_type"] == "ci-repair" and not {
        "failed_run_id", "repair_attempt"
    } <= set(manifest):
        raise ArtifactContractError("CI repair artifact misses repair binding")
    return manifest


def validate_artifact(
    *,
    artifact_dir: Path,
    expected_worker_type: str,
    issue_number: int,
    pr_number: int,
    base_sha: str,
    expected_remote_sha: str | None,
    branch: str,
) -> dict[str, Any]:
    patch_path, manifest_path = artifact_dir / PATCH_NAME, artifact_dir / MANIFEST_NAME
    if not patch_path.is_file() or not manifest_path.is_file():
        raise ArtifactContractError("agent.patch and agent-result.json are both required")
    try:
        manifest = _validate_manifest(json.loads(manifest_path.read_text()))
    except json.JSONDecodeError as exc:
        raise ArtifactContractError("artifact manifest is not JSON") from exc
    expected = {
        "worker_type": expected_worker_type,
        "issue_number": issue_number,
        "pr_number": pr_number,
        "base_sha": base_sha,
        "expected_remote_sha": expected_remote_sha,
        "branch": branch,
    }
    if any(manifest[key] != value for key, value in expected.items()):
        raise ArtifactContractError("artifact binding does not match workflow inputs")
    patch = patch_path.read_bytes()
    if manifest["patch_sha256"] != _sha256(patch) or manifest["patch_size_bytes"] != len(patch):
        raise ArtifactContractError("artifact patch checksum or size does not match")
    if manifest["changed_files"] != _patch_paths(patch):
        raise ArtifactContractError("artifact file list does not match patch")
    return manifest


def validate_index(repo: Path, manifest: dict[str, Any]) -> None:
    if _changed_files(repo) != manifest["changed_files"]:
        raise ArtifactContractError("applied patch changed files differ from manifest")


def validate_issue_scope(issue_body: str, manifest: dict[str, Any]) -> None:
    """Require every changed path to be declared by the task Issue itself."""

    allowed = parse_issue_scope(issue_body)
    for changed in manifest["changed_files"]:
        if not any(changed == path or (path.endswith("/") and changed.startswith(path)) for path in allowed):
            raise ArtifactContractError(f"artifact path is outside the task Issue scope: {changed}")


def _scope_path_safe(path: str) -> bool:
    if not _safe_path(path) or path in {".", "./", "/"} or any(char in path for char in "*?[]"):
        return False
    if path.endswith("/") and path[:-1] in {"", "."}:
        return False
    return True


def parse_issue_scope(issue_body: str) -> list[str]:
    """Parse and validate the one canonical editable Issue scope marker."""

    match = ISSUE_SCOPE_PATTERN.search(issue_body or "")
    if not match:
        raise ArtifactContractError("task Issue lacks an agent-orchestrator scope marker")
    try:
        scope = json.loads(match.group(1))
    except json.JSONDecodeError as exc:
        raise ArtifactContractError("task Issue scope marker is invalid JSON") from exc
    allowed = scope.get("allowed_paths") if isinstance(scope, dict) else None
    if not isinstance(allowed, list) or not allowed or not all(
        isinstance(path, str) and _scope_path_safe(path) for path in allowed
    ):
        raise ArtifactContractError("task Issue scope has no valid allowed_paths")
    if len(allowed) != len(set(allowed)):
        raise ArtifactContractError("task Issue scope has duplicate allowed paths")
    return allowed


def _optional_sha(value: str) -> str | None:
    return value or None


def main() -> None:
    if len(sys.argv) < 2:
        raise SystemExit("Usage: artifact_contract.py <create|validate|validate-index|validate-scope-definition> ...")
    command = sys.argv[1]
    try:
        if command == "create":
            # worker_type issue pr base expected_remote branch codex_exit local_checks artifact_dir [run attempt]
            if len(sys.argv) not in {11, 13}:
                raise ArtifactContractError("invalid create arguments")
            worker_type, issue, pr, base, remote, branch, exit_code, checks, directory = sys.argv[2:11]
            extra = sys.argv[11:]
            manifest = create_artifact(
                repo=Path.cwd(), artifact_dir=Path(directory), worker_type=worker_type,
                issue_number=int(issue), pr_number=int(pr), base_sha=base,
                expected_remote_sha=_optional_sha(remote), branch=branch, codex_exit_code=int(exit_code),
                local_checks=json.loads(checks),
                failed_run_id=int(extra[0]) if extra else None,
                repair_attempt=int(extra[1]) if extra else None,
            )
            print(json.dumps(manifest, sort_keys=True))
        elif command == "validate":
            # worker_type issue pr base expected_remote branch artifact_dir
            if len(sys.argv) != 9:
                raise ArtifactContractError("invalid validate arguments")
            worker_type, issue, pr, base, remote, branch, directory = sys.argv[2:9]
            manifest = validate_artifact(
                artifact_dir=Path(directory), expected_worker_type=worker_type,
                issue_number=int(issue), pr_number=int(pr), base_sha=base,
                expected_remote_sha=_optional_sha(remote), branch=branch,
            )
            print(json.dumps(manifest, sort_keys=True))
        elif command == "validate-index":
            if len(sys.argv) != 3:
                raise ArtifactContractError("invalid validate-index arguments")
            manifest = _validate_manifest(json.loads(Path(sys.argv[2]).read_text()))
            validate_index(Path.cwd(), manifest)
        elif command == "validate-scope":
            if len(sys.argv) != 4:
                raise ArtifactContractError("invalid validate-scope arguments")
            manifest = _validate_manifest(json.loads(Path(sys.argv[3]).read_text()))
            validate_issue_scope(Path(sys.argv[2]).read_text(), manifest)
        elif command == "validate-scope-definition":
            if len(sys.argv) != 3:
                raise ArtifactContractError("invalid validate-scope-definition arguments")
            print(json.dumps({"allowed_paths": parse_issue_scope(Path(sys.argv[2]).read_text())}, sort_keys=True))
        else:
            raise ArtifactContractError(f"unknown artifact command: {command}")
    except (ArtifactContractError, ValueError, json.JSONDecodeError) as exc:
        print(f"ARTIFACT_CONTRACT_ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
