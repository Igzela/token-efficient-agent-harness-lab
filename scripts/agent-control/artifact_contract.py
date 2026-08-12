"""Create and validate the untrusted Codex patch artifact contract."""

from __future__ import annotations

import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any


SCHEMA_VERSION = 1
PATCH_NAME = "agent.patch"
MANIFEST_NAME = "agent-result.json"
WORKER_TYPES = frozenset({"implementation", "ci-repair"})
FORBIDDEN_PREFIXES = (".git/", ".codex/", ".github/workflows/", ".github/actions/")
ISSUE_SCOPE_PATTERN = re.compile(r"<!--\s*agent-orchestrator-scope:v1\s*(\{.*?\})\s*-->", re.DOTALL)
MAX_PATCH_BYTES = 10 * 1024 * 1024
MAX_MANIFEST_BYTES = 256 * 1024
MAX_CHANGED_FILES = 100
MAX_PATH_CHARS = 1024
MAX_LOCAL_CHECKS = 50
MAX_CHECK_COMMAND_CHARS = 1000
MAX_ALLOWED_PATHS = 100


class ArtifactContractError(RuntimeError):
    """Raised when an untrusted artifact fails deterministic validation."""


def _artifact_path_safe(path: Path, *, directory: bool = False) -> None:
    """Reject symlinked artifact paths before reading or replacing them."""

    if path.is_symlink():
        raise ArtifactContractError("artifact path is a symlink")
    if directory and path.exists() and not path.is_dir():
        raise ArtifactContractError("artifact directory is not a directory")
    for parent in (path.parent, *path.parent.parents):
        if parent == parent.parent:
            break
        if parent.is_symlink():
            raise ArtifactContractError("artifact parent is a symlink")


def _atomic_write(path: Path, data: bytes) -> None:
    """Write bounded artifact data atomically with restricted permissions."""

    _artifact_path_safe(path)
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(path.parent, 0o700)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="wb", dir=path.parent, prefix=f".{path.name}.", delete=False
        ) as handle:
            temporary = Path(handle.name)
            os.chmod(handle.name, 0o600)
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
        os.chmod(path, 0o600)
    except OSError as exc:
        raise ArtifactContractError("artifact write failed") from exc
    finally:
        if temporary is not None and temporary.exists():
            temporary.unlink(missing_ok=True)


def ensure_private_directory(directory: Path) -> None:
    """Create an artifact-owned directory without accepting symlink paths."""

    _artifact_path_safe(directory, directory=True)
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    _artifact_path_safe(directory, directory=True)
    os.chmod(directory, 0o700)


def atomic_write_json(path: Path, value: dict[str, Any]) -> None:
    """Persist a small ownership record atomically and owner-readably only."""

    if not isinstance(value, dict):
        raise ArtifactContractError("artifact JSON value is not an object")
    ensure_private_directory(path.parent)
    _atomic_write(path, (json.dumps(value, sort_keys=True) + "\n").encode("utf-8"))


@dataclass(frozen=True)
class ArtifactManifest:
    """Versioned wire schema emitted by an untrusted patch worker."""

    worker_type: str
    issue_number: int
    pr_number: int
    base_sha: str
    expected_remote_sha: str | None
    branch: str
    changed_files: list[str]
    file_count: int
    patch_sha256: str
    patch_size_bytes: int
    codex_exit_code: int
    local_checks: list[dict[str, Any]]
    failed_run_id: int | None = None
    repair_attempt: int | None = None
    subject_kind: str = "issue"
    subject_id: str | None = None

    def to_wire(self) -> dict[str, Any]:
        wire = {"schema_version": SCHEMA_VERSION, **asdict(self)}
        if self.worker_type != "ci-repair":
            wire.pop("failed_run_id")
            wire.pop("repair_attempt")
        if self.subject_kind == "issue":
            wire.pop("subject_kind")
            wire.pop("subject_id")
        return wire


def _git(repo: Path, *args: str) -> bytes:
    result = subprocess.run(["git", *args], cwd=repo, capture_output=True, timeout=30)
    if result.returncode != 0:
        raise ArtifactContractError(result.stderr.decode(errors="replace").strip() or "git command failed")
    return result.stdout


def _safe_path(path: str) -> bool:
    return (
        bool(path)
        and len(path) <= MAX_PATH_CHARS
        and not path.startswith("/")
        and "\\" not in path
        and ".." not in Path(path).parts
        and not path.startswith(FORBIDDEN_PREFIXES)
    )


def _changed_files(repo: Path) -> list[str]:
    raw = _git(repo, "diff", "--cached", "--name-only", "-z")
    paths = [part.decode("utf-8", errors="strict") for part in raw.split(b"\0") if part]
    if paths != sorted(set(paths)) or not all(_safe_path(path) for path in paths):
        raise ArtifactContractError("staged paths are unsafe or non-deterministic")
    return paths


def _patch_paths(patch: bytes) -> list[str]:
    if not patch or len(patch) > MAX_PATCH_BYTES:
        raise ArtifactContractError("patch size is outside the bounded contract")
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
    if (
        not result
        or len(result) > MAX_CHANGED_FILES
        or not all(_safe_path(path) for path in result)
    ):
        raise ArtifactContractError("patch changes forbidden or invalid paths")
    return result


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _validate_local_checks(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list) or len(value) > MAX_LOCAL_CHECKS:
        raise ArtifactContractError("artifact local checks exceed the bounded contract")
    for check in value:
        if not isinstance(check, dict) or set(check) != {"command", "exit_code"}:
            raise ArtifactContractError("artifact local check is malformed")
        command = check.get("command")
        exit_code = check.get("exit_code")
        if (
            not isinstance(command, str)
            or not command
            or len(command) > MAX_CHECK_COMMAND_CHARS
            or type(exit_code) is not int
        ):
            raise ArtifactContractError("artifact local check is malformed")
    return value


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
    subject_kind: str = "issue",
    subject_id: str | None = None,
) -> dict[str, Any]:
    if worker_type not in WORKER_TYPES:
        raise ArtifactContractError("unknown worker type")
    if _git(repo, "rev-parse", "HEAD").decode().strip() != base_sha:
        raise ArtifactContractError("worktree HEAD moved after the trusted base was recorded")
    if subject_kind == "issue":
        if branch != f"agent/issue-{issue_number}":
            raise ArtifactContractError("artifact branch is not canonical for its Issue")
    elif subject_kind == "plan-packet":
        if not isinstance(subject_id, str) or branch != f"agent/packet-{subject_id.lower()}":
            raise ArtifactContractError("artifact branch is not canonical for its plan packet")
    else:
        raise ArtifactContractError("artifact subject kind is invalid")
    _validate_local_checks(local_checks)
    _git(repo, "add", "-A")
    patch = _git(repo, "diff", "--cached", "--binary", "--full-index")
    changed_files = _changed_files(repo)
    if not changed_files:
        raise ArtifactContractError("Codex produced no staged changes")
    if changed_files != _patch_paths(patch):
        raise ArtifactContractError("staged file list does not match binary patch")
    ensure_private_directory(artifact_dir)
    _atomic_write(artifact_dir / PATCH_NAME, patch)
    if worker_type == "ci-repair":
        if failed_run_id is None or repair_attempt is None:
            raise ArtifactContractError("CI repair artifact lacks failed-run binding")
    manifest = ArtifactManifest(
        worker_type=worker_type,
        issue_number=issue_number,
        pr_number=pr_number,
        base_sha=base_sha,
        expected_remote_sha=expected_remote_sha,
        branch=branch,
        changed_files=changed_files,
        file_count=len(changed_files),
        patch_sha256=_sha256(patch),
        patch_size_bytes=len(patch),
        codex_exit_code=codex_exit_code,
        local_checks=local_checks,
        failed_run_id=failed_run_id,
        repair_attempt=repair_attempt,
        subject_kind=subject_kind,
        subject_id=subject_id,
    ).to_wire()
    _atomic_write(
        artifact_dir / MANIFEST_NAME,
        (json.dumps(manifest, sort_keys=True) + "\n").encode("utf-8"),
    )
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
    allowed = required | {"failed_run_id", "repair_attempt", "subject_kind", "subject_id"}
    if set(manifest) - allowed:
        raise ArtifactContractError("artifact manifest has unsupported fields")
    if manifest["schema_version"] != SCHEMA_VERSION or manifest["worker_type"] not in WORKER_TYPES:
        raise ArtifactContractError("artifact schema version or worker type is invalid")
    if not all(type(manifest[key]) is int for key in ("issue_number", "pr_number", "file_count", "patch_size_bytes", "codex_exit_code")):
        raise ArtifactContractError("artifact numeric fields are invalid")
    if not isinstance(manifest["base_sha"], str) or not re.fullmatch(r"[0-9a-f]{40}", manifest["base_sha"]):
        raise ArtifactContractError("artifact base SHA is invalid")
    if manifest["expected_remote_sha"] is not None and (
        not isinstance(manifest["expected_remote_sha"], str)
        or not re.fullmatch(r"[0-9a-f]{40}", manifest["expected_remote_sha"])
    ):
        raise ArtifactContractError("artifact expected remote SHA is invalid")
    if not isinstance(manifest["branch"], str) or not isinstance(manifest["changed_files"], list):
        raise ArtifactContractError("artifact branch or changed files are invalid")
    if manifest["branch"] != f"agent/issue-{manifest['issue_number']}":
        if manifest.get("subject_kind") != "plan-packet":
            raise ArtifactContractError("artifact branch is not canonical for its Issue")
    if manifest.get("subject_kind", "issue") == "plan-packet":
        subject_id = manifest.get("subject_id")
        if not isinstance(subject_id, str) or manifest["branch"] != f"agent/packet-{subject_id.lower()}":
            raise ArtifactContractError("artifact plan subject binding is invalid")
    elif manifest.get("subject_kind", "issue") != "issue" or "subject_id" in manifest:
        raise ArtifactContractError("artifact subject binding is invalid")
    paths = manifest["changed_files"]
    if (
        len(paths) > MAX_CHANGED_FILES
        or paths != sorted(set(paths))
        or not all(isinstance(path, str) and _safe_path(path) for path in paths)
    ):
        raise ArtifactContractError("artifact changed paths are invalid")
    if (
        manifest["file_count"] != len(paths)
        or manifest["patch_size_bytes"] < 1
        or manifest["patch_size_bytes"] > MAX_PATCH_BYTES
    ):
        raise ArtifactContractError("artifact file count or patch size is invalid")
    if not isinstance(manifest["patch_sha256"], str) or not re.fullmatch(r"[0-9a-f]{64}", manifest["patch_sha256"]):
        raise ArtifactContractError("artifact checksum is invalid")
    _validate_local_checks(manifest["local_checks"])
    if manifest["worker_type"] == "ci-repair" and not {
        "failed_run_id", "repair_attempt"
    } <= set(manifest):
        raise ArtifactContractError("CI repair artifact misses repair binding")
    if manifest["worker_type"] == "ci-repair" and not all(
        type(manifest[key]) is int and manifest[key] >= 0
        for key in ("failed_run_id", "repair_attempt")
    ):
        raise ArtifactContractError("CI repair binding is invalid")
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
    subject_kind: str = "issue",
    subject_id: str | None = None,
) -> dict[str, Any]:
    patch_path, manifest_path = artifact_dir / PATCH_NAME, artifact_dir / MANIFEST_NAME
    _artifact_path_safe(artifact_dir, directory=True)
    _artifact_path_safe(patch_path)
    _artifact_path_safe(manifest_path)
    if not patch_path.is_file() or not manifest_path.is_file():
        raise ArtifactContractError("agent.patch and agent-result.json are both required")
    if manifest_path.stat().st_size > MAX_MANIFEST_BYTES:
        raise ArtifactContractError("artifact manifest exceeds the bounded contract")
    if patch_path.stat().st_size > MAX_PATCH_BYTES:
        raise ArtifactContractError("artifact patch exceeds the bounded contract")
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
        "subject_kind": subject_kind,
        "subject_id": subject_id,
    }
    if any(
        (manifest.get(key, "issue") if key == "subject_kind" else manifest.get(key)) != value
        for key, value in expected.items()
    ):
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

    _validate_paths_within(parse_issue_scope(issue_body), manifest["changed_files"])


def validate_scope_binding(binding: Any, manifest: dict[str, Any]) -> None:
    """Require every changed path to be declared by the claim-bound task scope."""

    normalized = validate_issue_scope_binding(binding)
    _validate_paths_within(normalized["allowed_paths"], manifest["changed_files"])


def validate_artifact_scope(allowed_paths: Any, manifest: dict[str, Any]) -> None:
    """Apply the canonical path-containment algorithm to a validated artifact.

    Plan packets bind ``allowed_paths`` through their claim/candidate contract
    rather than an Issue body.  This public helper keeps normalization and
    file/directory containment in this module instead of duplicating path
    semantics in the plan runner.
    """

    normalized = validate_allowed_paths(allowed_paths)
    changed_files = manifest.get("changed_files") if isinstance(manifest, dict) else None
    if not isinstance(changed_files, list) or not all(
        isinstance(path, str) and _safe_path(path) for path in changed_files
    ):
        raise ArtifactContractError("artifact changed_files are invalid")
    _validate_paths_within(normalized, changed_files)


def _scope_path_safe(path: str) -> bool:
    if not _safe_path(path) or path in {".", "./", "/"} or any(char in path for char in "*?[]"):
        return False
    if path.endswith("/") and path[:-1] in {"", "."}:
        return False
    return True


def validate_allowed_paths(value: Any) -> list[str]:
    """Validate and return a non-empty, non-duplicated bounded path list."""

    if not isinstance(value, list) or not value or len(value) > MAX_ALLOWED_PATHS or not all(
        isinstance(path, str) and _scope_path_safe(path) for path in value
    ):
        raise ArtifactContractError("task Issue scope has no valid allowed_paths")
    if len(value) != len(set(value)):
        raise ArtifactContractError("task Issue scope has duplicate allowed paths")
    return value


def parse_issue_scope(issue_body: str) -> list[str]:
    """Parse and validate the one canonical editable Issue scope marker."""

    matches = ISSUE_SCOPE_PATTERN.findall(issue_body or "")
    if len(matches) != 1:
        raise ArtifactContractError("task Issue must contain exactly one agent-orchestrator scope marker")
    try:
        scope = json.loads(matches[0])
    except json.JSONDecodeError as exc:
        raise ArtifactContractError("task Issue scope marker is invalid JSON") from exc
    if not isinstance(scope, dict):
        raise ArtifactContractError("task Issue scope has no valid allowed_paths")
    return validate_allowed_paths(scope.get("allowed_paths"))


def build_issue_scope_binding(issue_body: str) -> dict[str, str]:
    """Bind the parsed Issue scope to a digest of the complete untrusted body."""

    if not isinstance(issue_body, str):
        raise ArtifactContractError("task Issue body is unavailable")
    return {
        "allowed_paths": parse_issue_scope(issue_body),
        "task_body_sha256": _sha256(issue_body.encode("utf-8")),
    }


def validate_issue_scope_binding(value: Any) -> dict[str, str]:
    """Validate a claim-bound scope binding extracted from untrusted JSON.

    The input may carry extra dispatch-state fields; only the two canonical
    binding fields are required and returned.
    """

    if not isinstance(value, dict):
        raise ArtifactContractError("task scope binding is not an object")
    allowed = validate_allowed_paths(value.get("allowed_paths"))
    digest = value.get("task_body_sha256")
    if not isinstance(digest, str) or re.fullmatch(r"[0-9a-f]{64}", digest) is None:
        raise ArtifactContractError("task scope binding digest is invalid")
    return {"allowed_paths": allowed, "task_body_sha256": digest}


def _validate_paths_within(allowed_paths: list[str], changed_files: list[str]) -> None:
    for changed in changed_files:
        if not any(
            changed == path or (path.endswith("/") and changed.startswith(path))
            for path in allowed_paths
        ):
            raise ArtifactContractError(f"artifact path is outside the task Issue scope: {changed}")


def scopes_overlap(left: list[str], right: list[str]) -> bool:
    """Return whether two validated Issue scopes can name the same path."""

    for first in left:
        for second in right:
            if first == second:
                return True
            if first.endswith("/") and second.startswith(first):
                return True
            if second.endswith("/") and first.startswith(second):
                return True
    return False


def _optional_sha(value: str) -> str | None:
    return value or None


def main() -> None:
    if len(sys.argv) < 2:
        raise SystemExit("Usage: artifact_contract.py <create|validate|validate-index|validate-scope|validate-scope-binding|validate-scope-definition> ...")
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
        elif command == "validate-scope-binding":
            if len(sys.argv) != 4:
                raise ArtifactContractError("invalid validate-scope-binding arguments")
            binding = json.loads(Path(sys.argv[2]).read_text())
            manifest = _validate_manifest(json.loads(Path(sys.argv[3]).read_text()))
            validate_scope_binding(binding, manifest)
        elif command == "validate-scope-definition":
            if len(sys.argv) != 3:
                raise ArtifactContractError("invalid validate-scope-definition arguments")
            print(json.dumps({"allowed_paths": parse_issue_scope(Path(sys.argv[2]).read_text())}, sort_keys=True))
        else:
            raise ArtifactContractError(f"unknown artifact command: {command}")
    except (ArtifactContractError, OSError, ValueError, json.JSONDecodeError) as exc:
        print(f"ARTIFACT_CONTRACT_ERROR: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc


if __name__ == "__main__":
    main()
