#!/usr/bin/env python3
"""Verify the hash-bound, provider-free pre-AC snapshot inputs."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


FROZEN_RWE_MANIFEST_SHA256 = (
    "a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c"
)
FROZEN_POST_AC_IDENTITY = {
    "main_sha": "42fcfa5ad7e349d27d3caa815163340f9c0d5c0b",
    "tree_sha": "c81a2e4e635da05a8a1c15630371e98943c70c86",
    "cargo_lock_sha256": "cf68982734f8a72148950f119408b676dd5b42ce65d7af69c02eca017a551653",
    "rust_toolchain_sha256": "e59c5da37d1f9f4e0f815bc188cb6056fc7410c9cdaa9673c2d44da557c75d12",
}


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


def git_overlay_paths(root: Path) -> list[str]:
    tracked = git_output(root, "diff", "--name-only") or ""
    staged = git_output(root, "diff", "--cached", "--name-only") or ""
    untracked = git_output(root, "ls-files", "--others", "--exclude-standard") or ""
    generated_prefix = "apps/api/src/alters_lab_api.egg-info/"
    return sorted(
        path
        for path in {
            *tracked.splitlines(),
            *staged.splitlines(),
            *untracked.splitlines(),
        }
        if path and not path.startswith(generated_prefix)
    )


def required_manifest_failures(manifest: dict[str, object]) -> list[str]:
    failures: list[str] = []
    required = {
        "repository": ("source_commit", "source_tree_hash", "source_tree_hash_method", "reconstruction_base_tree_hash", "reconstruction_recipe_tree_hash"),
        "reconstruction_recipe": ("base_commit", "recipe_commit", "recipe_paths", "recipe_sha256", "overlay_rule", "recipe_state", "target_default_branch_write"),
        "build_inputs": ("dependency_lockfiles", "tracked_configuration", "source_configuration", "source_dependency_lockfiles"),
        "authority": ("external_effects", "provider_calls", "rwe_authority_consumed", "target_writes"),
        "rebuild": ("commands", "provider_free", "source_checkout_rule"),
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


def verify_reconstruction_metadata(manifest: dict[str, object], failures: list[str]) -> None:
    if manifest.get("status") != "RECONSTRUCTABLE":
        failures.append("snapshot status is not RECONSTRUCTABLE")
    if manifest.get("reconstructable") is not True:
        failures.append("snapshot is not marked reconstructable")

    authority = manifest.get("authority")
    if isinstance(authority, dict):
        for field in ("external_effects", "provider_calls", "rwe_authority_consumed", "target_writes"):
            if authority.get(field) is not False:
                failures.append(f"snapshot authority must deny {field}")

    rebuild = manifest.get("rebuild")
    if isinstance(rebuild, dict) and rebuild.get("provider_free") is not True:
        failures.append("snapshot rebuild is not provider-free")

    recipe = manifest.get("reconstruction_recipe")
    if isinstance(recipe, dict) and recipe.get("target_default_branch_write") is not False:
        failures.append("reconstruction recipe permits a target-default-branch write")


def verify_isolated_roots(source_root: Path, harness_root: Path, failures: list[str]) -> None:
    source = source_root.resolve()
    harness = harness_root.resolve()
    if source == harness:
        failures.append("pre-AC source and post-AC harness must use distinct roots")
        return
    if source in harness.parents or harness in source.parents:
        failures.append("pre-AC source and post-AC harness roots must not be nested")

    source_top = git_output(source, "rev-parse", "--show-toplevel")
    harness_top = git_output(harness, "rev-parse", "--show-toplevel")
    if source_top is None:
        failures.append("pre-AC source checkout is not a Git worktree")
    if harness_top is None:
        failures.append("post-AC harness checkout is not a Git worktree")
    if source_top is not None and harness_top is not None:
        if Path(source_top).resolve() == Path(harness_top).resolve():
            failures.append("pre-AC source and post-AC harness resolve to one Git worktree")


def verify_post_ac_harness(
    harness_root: Path, expected: dict[str, str], failures: list[str]
) -> None:
    if git_output(harness_root, "rev-parse", "HEAD") != expected["main_sha"]:
        failures.append("post-AC harness HEAD differs from the bound accepted main")
    if git_output(harness_root, "rev-parse", "HEAD^{tree}") != expected["tree_sha"]:
        failures.append("post-AC harness tree differs from the bound accepted tree")
    verify_file(harness_root, "Cargo.lock", expected["cargo_lock_sha256"], failures)
    verify_file(
        harness_root,
        "rust-toolchain.toml",
        expected["rust_toolchain_sha256"],
        failures,
    )


def _provider_free_environment(home: Path, target: Path) -> dict[str, str]:
    """Return an allowlisted environment with no host configuration authority."""
    environment = {
        "PATH": os.environ.get("PATH", ""),
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(home / "config"),
        "CARGO_HOME": os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")),
        "RUSTUP_HOME": os.environ.get("RUSTUP_HOME", str(Path.home() / ".rustup")),
        "UV_CACHE_DIR": os.environ.get("UV_CACHE_DIR", str(Path.home() / ".cache" / "uv")),
        "CARGO_TARGET_DIR": str(target),
        "CARGO_BUILD_JOBS": "2",
        "CARGO_NET_OFFLINE": "true",
        "UV_OFFLINE": "true",
        "CARGO_TERM_COLOR": "never",
        "PYTHONDONTWRITEBYTECODE": "1",
        "NO_COLOR": "1",
    }
    return environment


def _run_provider_free_trace(
    name: str,
    command: list[str],
    cwd: Path,
    environment: dict[str, str],
    mounts: dict[Path, str],
    writable_mounts: set[Path],
    sandbox: bool,
    failures: list[str],
) -> None:
    if not sandbox:
        try:
            result = subprocess.run(
                command,
                cwd=cwd,
                env=environment,
                check=False,
                capture_output=True,
                text=True,
                timeout=3_600,
            )
        except (OSError, subprocess.TimeoutExpired) as error:
            failures.append(f"provider-free trace unavailable: {name}: {type(error).__name__}")
            return
        if result.returncode:
            failures.append(f"provider-free trace failed: {name}: exit={result.returncode}")
        return

    bubblewrap = shutil.which("bwrap")
    if bubblewrap is None:
        failures.append("provider-free trace unavailable: bwrap is required")
        return
    cache = Path(environment["UV_CACHE_DIR"])
    if not cache.is_dir():
        failures.append("provider-free trace unavailable: UV cache is not readable")
        return
    cargo_target = Path(environment["CARGO_TARGET_DIR"])
    if not cargo_target.is_dir():
        failures.append("provider-free trace unavailable: Cargo target cache is not readable")
        return
    mapped_cwd = mounts[cwd]
    mapped_environment = dict(environment)
    mapped_environment["UV_CACHE_DIR"] = "/tmp/rwe-uv-cache"
    if "PYTHONPATH" in mapped_environment:
        for host, guest in mounts.items():
            mapped_environment["PYTHONPATH"] = mapped_environment["PYTHONPATH"].replace(
                str(host), guest
            )
    sandbox_command = [
        bubblewrap,
        "--die-with-parent",
        "--unshare-net",
        "--clearenv",
        "--ro-bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
        "--tmpfs",
        "/tmp",
        "--dir",
        "/tmp/rwe-home",
        "--dir",
        "/tmp/rwe-home/config",
        "--dir",
        "/tmp/rwe-target",
        "--dir",
        "/tmp/rwe-source",
        "--dir",
        "/tmp/rwe-harness",
        "--bind",
        str(cache),
        "/tmp/rwe-uv-cache",
    ]
    for host, guest in mounts.items():
        mode = "--bind" if host in writable_mounts else "--ro-bind"
        sandbox_command.extend((mode, str(host), guest))
    sandbox_command.extend(("--chdir", mapped_cwd))
    for key, value in mapped_environment.items():
        sandbox_command.extend(("--setenv", key, value))
    sandbox_command.append("--")
    sandbox_command.extend(command)
    try:
        result = subprocess.run(
            sandbox_command,
            cwd=Path("/"),
            env={"PATH": environment["PATH"]},
            check=False,
            capture_output=True,
            text=True,
            timeout=3_600,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        failures.append(f"provider-free trace unavailable: {name}: {type(error).__name__}")
        return
    if result.returncode:
        failures.append(f"provider-free trace failed: {name}: exit={result.returncode}")


def verify_provider_free_traces(
    source_root: Path, harness_root: Path, failures: list[str]
) -> None:
    with tempfile.TemporaryDirectory(prefix="pe7-rwe-trace-") as directory:
        temporary = Path(directory)
        trace_source = temporary / "source"
        shutil.copytree(source_root, trace_source, symlinks=True)
        target = temporary / "target"
        target.mkdir()
        environment = _provider_free_environment(temporary / "home", target)
        source_environment = dict(environment)
        source_environment["PYTHONPATH"] = str(trace_source / "apps/api/src")
        mounts = {trace_source: "/tmp/rwe-source", harness_root: "/tmp/rwe-harness"}
        _run_provider_free_trace(
            "pre_ac_source_pytest",
            [
                "uv",
                "run",
                "--locked",
                "--project",
                "apps/api/pyproject.toml",
                "--extra",
                "dev",
                "python",
                "-m",
                "pytest",
                "apps/api/tests/",
                "-q",
            ],
            trace_source,
        source_environment,
        mounts,
        {trace_source},
        True,
        failures,
        )
        _run_provider_free_trace(
            "post_ac_engine_tests",
            ["cargo", "test", "-p", "engine"],
            harness_root,
        environment,
        mounts,
        set(),
        False,
        failures,
        )


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
    if git_overlay_paths(source_root) != paths:
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
    declared_tasks = {item["path"] for item in frozen["task_definitions"]}
    for item in frozen["task_definitions"]:
        verify_file(harness_root, item["path"], item["sha256"], failures)
    verify_file(harness_root, frozen["protocol_path"], frozen["protocol_file_sha256"], failures)
    verify_file(harness_root, frozen["schedule_path"], frozen["schedule_file_sha256"], failures)
    protocol: dict[str, object] = {}
    try:
        protocol = json.loads((harness_root / frozen["protocol_path"]).read_text(encoding="utf-8"))
        schedule = json.loads((harness_root / frozen["schedule_path"]).read_text(encoding="utf-8"))
        if protocol.get("fixture_only") is not False:
            failures.append("frozen protocol must not be fixture-only")
        if protocol.get("frozen_before_results") is not True:
            failures.append("frozen protocol must be frozen before results")
        if protocol.get("live_execution_authorized") is not False:
            failures.append("frozen protocol must not authorize live execution")
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
    actual_tasks = {
        path.relative_to(harness_root).as_posix()
        for path in task_paths
    }
    if actual_tasks != declared_tasks:
        failures.append("frozen task definition path set differs from the manifest")
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
        matching = next(
            (
                item
                for item in protocol.get("tasks", [])
                if isinstance(item, dict) and item.get("task_id") == task.get("task_id")
            ),
            None,
        )
        if matching is None:
            failures.append(f"frozen task is absent from protocol: {path.name}")
        elif matching.get("task_definition_sha256") != sha256_file(path):
            failures.append(f"frozen task definition digest differs from protocol: {path.name}")


def verify(
    manifest_path: Path,
    source_root: Path,
    harness_root: Path,
    post_ac_identity: dict[str, str] | None = None,
    execute_traces: bool = False,
) -> list[str]:
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    failures: list[str] = []
    failures.extend(required_manifest_failures(manifest))
    verify_reconstruction_metadata(manifest, failures)
    expected_manifest = manifest.get("manifest_sha256")
    if not isinstance(expected_manifest, str):
        failures.append("manifest_sha256 is missing")
    elif expected_manifest != FROZEN_RWE_MANIFEST_SHA256:
        failures.append("snapshot manifest digest differs from the frozen RWE binding")
    elif canonical_manifest_digest(manifest) != expected_manifest:
        failures.append("snapshot manifest canonical digest mismatch")

    if not failures:
        try:
            verify_isolated_roots(source_root, harness_root, failures)
            if post_ac_identity is None:
                failures.append("post-AC identity binding is required")
            elif post_ac_identity != FROZEN_POST_AC_IDENTITY:
                failures.append("post-AC identity differs from the frozen RWE binding")
            else:
                verify_post_ac_harness(harness_root, post_ac_identity, failures)
            verify_git_overlay(source_root, manifest, failures)
            verify_frozen_task_bindings(harness_root, manifest, failures)
            if execute_traces and not failures:
                verify_provider_free_traces(source_root, harness_root, failures)
                verify_post_ac_harness(harness_root, FROZEN_POST_AC_IDENTITY, failures)
                verify_git_overlay(source_root, manifest, failures)
                verify_frozen_task_bindings(harness_root, manifest, failures)
            elif not execute_traces:
                failures.append("provider-free trace execution is required")
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
    parser.add_argument("--post-ac-main-sha", required=True)
    parser.add_argument("--post-ac-tree-sha", required=True)
    parser.add_argument("--post-ac-cargo-lock-sha256", required=True)
    parser.add_argument("--post-ac-rust-toolchain-sha256", required=True)
    parser.add_argument("--execute-traces", action="store_true")
    args = parser.parse_args()
    try:
        failures = verify(
            args.manifest.resolve(),
            args.source_root.resolve(),
            args.harness_root.resolve(),
            {
                "main_sha": args.post_ac_main_sha,
                "tree_sha": args.post_ac_tree_sha,
                "cargo_lock_sha256": args.post_ac_cargo_lock_sha256,
                "rust_toolchain_sha256": args.post_ac_rust_toolchain_sha256,
            },
            args.execute_traces,
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
