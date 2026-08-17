#!/usr/bin/env python3
"""Verify the hash-bound, provider-free pre-AC snapshot inputs."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import shlex
import signal
import subprocess
import sys
import tempfile
import threading


FROZEN_RWE_MANIFEST_SHA256 = (
    "a423ea9889dfc32680f660312bf61d95e5c2a26c49fc52143b26b8d9847c9c8c"
)
FROZEN_POST_AC_IDENTITY = {
    "main_sha": "42fcfa5ad7e349d27d3caa815163340f9c0d5c0b",
    "tree_sha": "c81a2e4e635da05a8a1c15630371e98943c70c86",
    "cargo_lock_sha256": "cf68982734f8a72148950f119408b676dd5b42ce65d7af69c02eca017a551653",
    "rust_toolchain_sha256": "e59c5da37d1f9f4e0f815bc188cb6056fc7410c9cdaa9673c2d44da557c75d12",
}
FROZEN_OBSERVED_TOOLCHAIN = {
    "rustc": "rustc 1.96.0 (ac68faa20 2026-05-25)",
    "rustdoc": "rustdoc 1.96.0 (ac68faa20 2026-05-25)",
    "cargo": "cargo 1.96.0 (30a34c682 2026-05-25)",
    "python": "Python 3.14.4",
    "uv": "uv 0.11.17",
    "git": "git version 2.53.0",
}
FROZEN_TRACE_BINARY_IDENTITIES = {
    "rustc": {
        "path": str(
            Path.home()
            / ".rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin/rustc"
        ),
        "sha256": "ba4b837efb6612dfa8d941c5a72b8a50d1d03a0f36216743b173949aa8d9eb75",
    },
    "rustdoc": {
        "path": str(
            Path.home()
            / ".rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin/rustdoc"
        ),
        "sha256": "ead78a0e00004d88ef7a3209a20552ba805cc9cb7cde7b061093a1b2dfb037c0",
    },
    "cargo": {
        "path": str(
            Path.home()
            / ".rustup/toolchains/1.96.0-x86_64-unknown-linux-gnu/bin/cargo"
        ),
        "sha256": "f30f9fd1b1d0b8fd10dc33219eb4cd4bec3543f40e434ac71f5a03fd0359063f",
    },
    "python": {
        "path": "/usr/bin/python3.14",
        "sha256": "b8d8288faefdd300201f43fcf00f6f539a27218eeed3a3dff5ab10b9c4c99700",
    },
    "uv": {
        "path": str(Path.home() / ".local/bin/uv"),
        "sha256": "8ac91b3913a96c6d98d65b2fc6996064c85d0dc42a626977d337046be796c75d",
    },
    "git": {
        "path": "/usr/bin/git",
        "sha256": "5516c9f362c29376ab9a499a33082f9f611941d8c75930c880e30ad109e39c9a",
    },
    "bwrap": {
        "path": "/usr/bin/bwrap",
        "sha256": "0abea81db798ebf6b4742ac0664802d97521547a353c2a0dbdc21d76cbbfd2c0",
    },
}
GIT_BINARY = FROZEN_TRACE_BINARY_IDENTITIES["git"]["path"]
MAX_TRACE_OUTPUT_BYTES = 128 * 1024
PROCESS_TERMINATION_GRACE_SECONDS = 5
CURRENT_REPOSITORY_ROOT = Path(__file__).resolve().parents[1]


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


def _git_environment() -> dict[str, str]:
    """Disable host Git configuration and repository-selection overrides."""
    return {
        "PATH": os.defpath,
        "LC_ALL": "C",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": os.devnull,
        "GIT_CONFIG_SYSTEM": os.devnull,
        "GIT_CONFIG_COUNT": "0",
        "GIT_TERMINAL_PROMPT": "0",
        "GIT_OPTIONAL_LOCKS": "0",
    }


def git_output(root: Path, *args: str) -> str | None:
    result = subprocess.run(
        [GIT_BINARY, "-C", str(root), *args],
        check=False,
        capture_output=True,
        text=True,
        env=_git_environment(),
    )
    if result.returncode:
        return None
    return result.stdout.strip()


def git_blob(root: Path, revision: str, relative: str) -> bytes | None:
    result = subprocess.run(
        [GIT_BINARY, "-C", str(root), "show", f"{revision}:{relative}"],
        check=False,
        capture_output=True,
        env=_git_environment(),
    )
    return result.stdout if result.returncode == 0 else None


def git_tree_hash(root: Path, revision: str) -> str | None:
    result = subprocess.run(
        [GIT_BINARY, "-C", str(root), "ls-tree", "-r", "-z", "--full-tree", revision],
        check=False,
        capture_output=True,
        env=_git_environment(),
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


def git_revision_paths(root: Path, revision: str) -> list[str]:
    result = subprocess.run(
        [GIT_BINARY, "-C", str(root), "ls-tree", "-r", "-z", "--name-only", revision],
        check=False,
        capture_output=True,
        env=_git_environment(),
    )
    if result.returncode:
        raise OSError(f"cannot enumerate Git revision: {revision}")
    return [item.decode("utf-8") for item in result.stdout.split(b"\0") if item]


def git_revision_modes(root: Path, revision: str) -> dict[str, int]:
    result = subprocess.run(
        [GIT_BINARY, "-C", str(root), "ls-tree", "-r", "-z", "--full-tree", revision],
        check=False,
        capture_output=True,
        env=_git_environment(),
    )
    if result.returncode:
        raise OSError(f"cannot enumerate Git revision modes: {revision}")
    modes: dict[str, int] = {}
    for record in result.stdout.split(b"\0"):
        if not record:
            continue
        header, relative_bytes = record.split(b"\t", 1)
        mode, object_type, _ = header.split()
        if object_type != b"blob" or mode not in {b"100644", b"100755"}:
            raise OSError(f"unsupported Git tree entry: {relative_bytes.decode('utf-8')}")
        modes[relative_bytes.decode("utf-8")] = 0o755 if mode == b"100755" else 0o644
    return modes


def copy_git_revision(root: Path, revision: str, destination: Path) -> None:
    """Copy only the bound Git revision, excluding ignored host artifacts."""
    destination.mkdir(parents=True, exist_ok=True)
    modes = git_revision_modes(root, revision)
    for relative in git_revision_paths(root, revision):
        content = git_blob(root, revision, relative)
        if content is None:
            raise OSError(f"cannot read Git revision path: {relative}")
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
        target.chmod(modes[relative])


def copy_recipe_overlay(
    root: Path, destination: Path, paths: list[str], modes: dict[str, int]
) -> None:
    for relative in paths:
        source = root / relative
        if source.is_symlink() or not source.is_file():
            raise OSError(f"recipe overlay is not a regular file: {relative}")
        expected_mode = modes.get(relative)
        if expected_mode is None:
            raise OSError(f"recipe overlay mode is unavailable: {relative}")
        actual_executable = source.stat().st_mode & 0o111
        if bool(actual_executable) != bool(expected_mode & 0o111):
            raise OSError(f"recipe overlay executable mode differs: {relative}")
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
        target.chmod(expected_mode)


def copy_cache_snapshot(source: Path, destination: Path, label: str) -> None:
    """Materialize an independent cache copy without host symlink reach-through."""
    try:
        shutil.copytree(source, destination, symlinks=False)
    except OSError as error:
        raise OSError(f"cannot snapshot {label} cache: {error}") from error
    for path in destination.rglob("*"):
        if path.is_symlink():
            raise OSError(f"{label} cache snapshot contains a symlink: {path}")


def registered_source_commands(
    manifest: dict[str, object], uv_binary: str, failures: list[str]
) -> tuple[dict[str, str], list[str], dict[str, str], list[str]] | None:
    rebuild = manifest.get("rebuild")
    commands = rebuild.get("commands") if isinstance(rebuild, dict) else None
    if not isinstance(commands, list) or len(commands) < 4:
        failures.append("frozen rebuild command list is incomplete")
        return None
    raw_materializer, raw_pytest = commands[2:4]
    if not isinstance(raw_materializer, str) or not isinstance(raw_pytest, str):
        failures.append("frozen source trace commands are malformed")
        return None

    def split_command(raw: str) -> tuple[dict[str, str], list[str]]:
        tokens = shlex.split(raw)
        command_environment: dict[str, str] = {}
        while tokens and "=" in tokens[0] and tokens[0].split("=", 1)[0].isidentifier():
            key, value = tokens.pop(0).split("=", 1)
            command_environment[key] = value
        if not tokens or tokens[0] != "uv":
            raise ValueError("registered source trace must invoke uv")
        tokens[0] = uv_binary
        return command_environment, tokens

    try:
        materializer_environment, materializer = split_command(raw_materializer)
        pytest_environment, pytest = split_command(raw_pytest)
    except ValueError as error:
        failures.append(f"frozen source trace command is not registered uv execution: {error}")
        return None
    return materializer_environment, materializer, pytest_environment, pytest


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
    if git_output(harness_root, "status", "--porcelain=v1", "--untracked-files=all"):
        failures.append("post-AC harness worktree must be clean")


def verify_observed_toolchain(failures: list[str]) -> None:
    commands = {
        "rustc": ["--version"],
        "rustdoc": ["--version"],
        "cargo": ["--version"],
        "python": ["--version"],
        "uv": ["--version"],
        "git": ["--version"],
    }
    for name, arguments in commands.items():
        identity = FROZEN_TRACE_BINARY_IDENTITIES[name]
        path = Path(identity["path"])
        if not path.is_file() or not os.access(path, os.X_OK):
            failures.append(f"frozen toolchain binary is unavailable: {name}")
            continue
        if sha256_file(path) != identity["sha256"]:
            failures.append(f"frozen toolchain binary digest differs: {name}")
            continue
        try:
            result = _run_bounded_command(
                [str(path), *arguments],
                cwd=Path("/"),
                environment={"PATH": str(path.parent)},
                timeout=10,
            )
        except OSError as error:
            failures.append(f"observed toolchain probe unavailable: {name}: {type(error).__name__}")
            continue
        if result.error is not None or result.timed_out:
            error_name = type(result.error).__name__ if result.error else "TimeoutExpired"
            failures.append(f"observed toolchain probe unavailable: {name}: {error_name}")
            continue
        actual = result.stdout.strip()
        if name == "uv":
            actual = actual.split(" (", 1)[0]
        if result.returncode or actual != FROZEN_OBSERVED_TOOLCHAIN[name]:
            failures.append(f"observed toolchain differs from the frozen binding: {name}")
    bwrap_identity = FROZEN_TRACE_BINARY_IDENTITIES["bwrap"]
    bwrap_path = Path(bwrap_identity["path"])
    if not bwrap_path.is_file() or sha256_file(bwrap_path) != bwrap_identity["sha256"]:
        failures.append("frozen toolchain binary digest differs: bwrap")


def resolve_trace_binary(name: str, environment: dict[str, str], failures: list[str]) -> str | None:
    identity = FROZEN_TRACE_BINARY_IDENTITIES[name]
    path = Path(identity["path"])
    if not path.is_file() or not os.access(path, os.X_OK):
        failures.append(f"provider-free trace binary is unavailable: {name}")
        return None
    if sha256_file(path) != identity["sha256"]:
        failures.append(f"provider-free trace binary digest differs: {name}")
        return None
    return str(path)


def find_engine_test_binary(target: Path, failures: list[str]) -> Path | None:
    candidates = sorted(
        path
        for path in (target / "debug" / "deps").glob("engine-*")
        if path.is_file() and os.access(path, os.X_OK)
    )
    if not candidates:
        failures.append("provider-free trace engine test binary is unavailable")
        return None
    return candidates[-1]


def _provider_free_environment(home: Path, target: Path) -> dict[str, str]:
    """Return an allowlisted environment with no host configuration authority."""
    cargo_home = Path.home() / ".cargo"
    rustup_home = Path.home() / ".rustup"
    uv_cache = Path.home() / ".cache/uv"
    environment = {
        "PATH": os.pathsep.join(
            [
                str(Path.home() / ".local" / "bin"),
                "/usr/bin",
                "/bin",
            ]
        ),
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(home / "config"),
        "CARGO_HOME": str(cargo_home),
        "RUSTUP_HOME": str(rustup_home),
        "UV_CACHE_DIR": str(uv_cache),
        "CARGO_TARGET_DIR": str(target),
        "CARGO_BUILD_JOBS": "1",
        "CARGO_INCREMENTAL": "0",
        "CARGO_PROFILE_DEV_DEBUG": "0",
        "CARGO_PROFILE_TEST_DEBUG": "0",
        "CARGO_NET_OFFLINE": "true",
        "UV_OFFLINE": "true",
        "UV_PYTHON": FROZEN_TRACE_BINARY_IDENTITIES["python"]["path"],
        "RUSTC": FROZEN_TRACE_BINARY_IDENTITIES["rustc"]["path"],
        "RUSTDOC": FROZEN_TRACE_BINARY_IDENTITIES["rustdoc"]["path"],
        "LD_LIBRARY_PATH": str(
            Path(FROZEN_TRACE_BINARY_IDENTITIES["rustc"]["path"]).parent.parent / "lib"
        ),
        "CARGO_TERM_COLOR": "never",
        "PYTHONDONTWRITEBYTECODE": "1",
        "NO_COLOR": "1",
    }
    return environment


@dataclass(frozen=True)
class BoundedCommandResult:
    returncode: int | None
    stdout: str
    stderr: str
    timed_out: bool
    error: BaseException | None
    forbidden_markers: tuple[str, ...]
    error_lines: tuple[str, ...]


def _drain_output(
    stream: object,
    markers: tuple[str, ...],
    state: dict[str, object],
    output_key: str,
) -> None:
    marker_bytes = tuple((marker, marker.encode()) for marker in markers)
    overlap_limit = max((len(encoded) for _, encoded in marker_bytes), default=1) - 1
    overlap = b""
    tail = bytearray()
    line_buffer = ""
    while True:
        chunk = stream.read(64 * 1024)  # type: ignore[attr-defined]
        if not chunk:
            break
        scan = overlap + chunk
        for marker, encoded in marker_bytes:
            if encoded in scan:
                state["forbidden_markers"].add(marker)  # type: ignore[union-attr]
        overlap = scan[-overlap_limit:] if overlap_limit else b""
        text_chunk = chunk.decode(errors="replace")
        lines = (line_buffer + text_chunk).splitlines(keepends=True)
        line_buffer = ""
        if lines and not lines[-1].endswith(("\n", "\r")):
            line_buffer = lines.pop()
        for line in lines:
            stripped = line.strip()
            if stripped.lower().startswith(("error", "failed")) or "error:" in stripped.lower():
                error_lines = state["error_lines"]  # type: ignore[assignment]
                error_lines.append(stripped[:2000])  # type: ignore[union-attr]
                del error_lines[:-8]  # type: ignore[index]
        tail.extend(chunk)
        if len(tail) > MAX_TRACE_OUTPUT_BYTES:
            del tail[:-MAX_TRACE_OUTPUT_BYTES]
    if line_buffer:
        stripped = line_buffer.strip()
        if stripped.lower().startswith(("error", "failed")) or "error:" in stripped.lower():
            error_lines = state["error_lines"]  # type: ignore[assignment]
            error_lines.append(stripped[:2000])  # type: ignore[union-attr]
            del error_lines[:-8]  # type: ignore[index]
    state[output_key] = bytes(tail)


def _terminate_process_group(process: subprocess.Popen[bytes]) -> None:
    """Terminate only the process group created for this verifier child."""
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        process.wait(timeout=PROCESS_TERMINATION_GRACE_SECONDS)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            return
        process.wait()


def _run_bounded_command(
    command: list[str | Path],
    *,
    cwd: Path,
    environment: dict[str, str],
    timeout: int,
    forbidden_output: tuple[str, ...] = (),
) -> BoundedCommandResult:
    """Run one child with bounded output and private descendant cleanup."""
    try:
        process = subprocess.Popen(
            command,
            cwd=cwd,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
    except OSError as error:
        return BoundedCommandResult(None, "", "", False, error, (), ())

    state: dict[str, object] = {"forbidden_markers": set(), "error_lines": []}
    stdout_thread = threading.Thread(
        target=_drain_output,
        args=(process.stdout, forbidden_output, state, "stdout"),
        daemon=True,
    )
    stderr_thread = threading.Thread(
        target=_drain_output,
        args=(process.stderr, forbidden_output, state, "stderr"),
        daemon=True,
    )
    stdout_thread.start()
    stderr_thread.start()
    timed_out = False
    try:
        returncode = process.wait(timeout=timeout)
    except subprocess.TimeoutExpired:
        timed_out = True
        _terminate_process_group(process)
        returncode = process.returncode
    stdout_thread.join(PROCESS_TERMINATION_GRACE_SECONDS)
    stderr_thread.join(PROCESS_TERMINATION_GRACE_SECONDS)
    if stdout_thread.is_alive() or stderr_thread.is_alive():
        _terminate_process_group(process)
        stdout_thread.join(PROCESS_TERMINATION_GRACE_SECONDS)
        stderr_thread.join(PROCESS_TERMINATION_GRACE_SECONDS)
    if returncode and not timed_out:
        _terminate_process_group(process)
    if process.stdout is not None:
        process.stdout.close()
    if process.stderr is not None:
        process.stderr.close()
    stdout = bytes(state.get("stdout", b"")).decode(errors="replace")
    stderr = bytes(state.get("stderr", b"")).decode(errors="replace")
    markers = tuple(sorted(state["forbidden_markers"]))  # type: ignore[arg-type]
    error_lines = tuple(state["error_lines"])  # type: ignore[arg-type]
    return BoundedCommandResult(returncode, stdout, stderr, timed_out, None, markers, error_lines)


def _run_provider_free_trace(
    name: str,
    command: list[str],
    cwd: Path,
    environment: dict[str, str],
    mounts: dict[Path, str],
    writable_mounts: set[Path],
    sandbox: bool,
    failures: list[str],
    forbidden_output: tuple[str, ...] = (),
) -> None:
    if not sandbox:
        result = _run_bounded_command(
            command,
            cwd=cwd,
            environment=environment,
            timeout=3_600,
            forbidden_output=forbidden_output,
        )
    else:
        bubblewrap = Path(FROZEN_TRACE_BINARY_IDENTITIES["bwrap"]["path"])
        if not bubblewrap.is_file():
            failures.append("provider-free trace unavailable: bwrap is required")
            return
        cargo_target = Path(environment["CARGO_TARGET_DIR"])
        if not cargo_target.is_dir():
            failures.append("provider-free trace unavailable: Cargo target cache is not readable")
            return
        mapped_cwd = mounts.get(cwd, str(cwd))
        mapped_environment = dict(environment)
        mapped_environment["HOME"] = "/tmp/rwe-home"
        mapped_environment["XDG_CONFIG_HOME"] = "/tmp/rwe-home/config"
        mapped_environment["CARGO_HOME"] = "/tmp/rwe-cargo-home"
        mapped_environment["RUSTUP_HOME"] = "/tmp/rwe-home/rustup"
        mapped_environment["CARGO_TARGET_DIR"] = "/tmp/rwe-target"
        mapped_environment["UV_CACHE_DIR"] = "/tmp/rwe-host-uv-cache"
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
            "/usr",
            "/usr",
            "--ro-bind",
            "/bin",
            "/bin",
            "--ro-bind",
            "/lib",
            "/lib",
            "--ro-bind",
            "/lib64",
            "/lib64",
            "--ro-bind",
            "/etc",
            "/etc",
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
            "/tmp/rwe-home/rustup",
            "--dir",
            "/tmp/rwe-cargo-home",
            "--dir",
            "/tmp/rwe-target",
            "--dir",
            "/tmp/rwe-source",
            "--dir",
            "/tmp/rwe-harness",
            "--dir",
            "/tmp/rwe-host-uv-cache",
            "--dir",
            "/workspace",
            "--dir",
            "/home",
            "--dir",
            str(Path.home()),
            "--dir",
            str(Path.home() / ".rustup"),
            "--dir",
            str(Path.home() / ".rustup" / "toolchains"),
            "--dir",
            str(Path(FROZEN_TRACE_BINARY_IDENTITIES["rustc"]["path"]).parent.parent),
            "--ro-bind",
            str(Path(FROZEN_TRACE_BINARY_IDENTITIES["rustc"]["path"]).parent.parent),
            str(Path(FROZEN_TRACE_BINARY_IDENTITIES["rustc"]["path"]).parent.parent),
            "--dir",
            str(Path.home() / ".local"),
            "--dir",
            str(Path.home() / ".local" / "bin"),
            "--ro-bind",
            FROZEN_TRACE_BINARY_IDENTITIES["uv"]["path"],
            FROZEN_TRACE_BINARY_IDENTITIES["uv"]["path"],
        ]
        for host, guest in mounts.items():
            mode = "--bind" if host in writable_mounts else "--ro-bind"
            sandbox_command.extend((mode, str(host), guest))
        sandbox_command.extend(("--chdir", mapped_cwd))
        for key, value in mapped_environment.items():
            sandbox_command.extend(("--setenv", key, value))
        sandbox_command.append("--")
        sandbox_command.extend(command)
        result = _run_bounded_command(
            sandbox_command,
            cwd=Path("/"),
            environment={"PATH": environment["PATH"]},
            timeout=3_600,
            forbidden_output=forbidden_output,
        )
    if result.error is not None:
        failures.append(f"provider-free trace unavailable: {name}: {type(result.error).__name__}")
        return
    if result.timed_out:
        failures.append(f"provider-free trace unavailable: {name}: TimeoutExpired")
        return
    for marker in result.forbidden_markers:
        failures.append(f"provider-free trace emitted forbidden marker: {name}: {marker}")
    if result.returncode:
        detail = (result.stdout + "\n" + result.stderr).strip().splitlines()
        summary = list(result.error_lines) or detail[-20:]
        suffix = f": {' | '.join(summary)[-2000:]}" if summary else ""
        failures.append(
            f"provider-free trace failed: {name}: exit={result.returncode}{suffix}"
        )


def verify_bwrap_capability(environment: dict[str, str], failures: list[str]) -> None:
    """Require a real direct bwrap probe before running the nested test lane."""
    bubblewrap = Path(FROZEN_TRACE_BINARY_IDENTITIES["bwrap"]["path"])
    result = _run_bounded_command(
        [
            str(bubblewrap),
            "--die-with-parent",
            "--unshare-net",
            "--clearenv",
            "--ro-bind",
            "/usr",
            "/usr",
            "--ro-bind",
            "/bin",
            "/bin",
            "--ro-bind",
            "/lib",
            "/lib",
            "--ro-bind",
            "/lib64",
            "/lib64",
            "--ro-bind",
            "/etc",
            "/etc",
            "--tmpfs",
            "/home",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--",
            "/usr/bin/test",
            "!",
            "-e",
            str(Path.home() / ".codex"),
        ],
        cwd=Path("/"),
        environment={"PATH": environment["PATH"]},
        timeout=30,
    )
    if result.error is not None or result.timed_out or result.returncode:
        failures.append("provider-free bwrap capability probe did not establish isolation")


def verify_trace_generated_baseline(
    trace_source: Path, manifest: dict[str, object], failures: list[str]
) -> None:
    generated = manifest.get("generated_active_baseline", {})
    if not isinstance(generated, dict):
        failures.append("generated active baseline metadata is malformed")
        return
    for relative, expected in generated.get("files", {}).items():
        verify_file(trace_source, relative, expected, failures)


def verify_provider_free_traces(
    source_root: Path,
    harness_root: Path,
    manifest: dict[str, object],
    failures: list[str],
) -> None:
    with tempfile.TemporaryDirectory(prefix="pe7-rwe-trace-") as directory:
        temporary = Path(directory)
        trace_source = temporary / "source"
        copy_git_revision(source_root, manifest["repository"]["source_commit"], trace_source)
        copy_recipe_overlay(
            source_root,
            trace_source,
            manifest["reconstruction_recipe"]["recipe_paths"],
            git_revision_modes(
                source_root, manifest["reconstruction_recipe"]["recipe_commit"]
            ),
        )
        (trace_source / "source").symlink_to(".")
        trace_harness = temporary / "harness"
        copy_git_revision(harness_root, FROZEN_POST_AC_IDENTITY["main_sha"], trace_harness)
        target = temporary / "target"
        target.mkdir()
        home = temporary / "home"
        home.mkdir()
        trace_cargo = temporary / "cargo-home"
        trace_cargo.mkdir()
        host_cargo = Path.home() / ".cargo"
        for relative in ("registry", "git"):
            source_cache = host_cargo / relative
            if not source_cache.is_dir():
                failures.append(f"provider-free trace requires Cargo cache: {relative}")
                return
            copy_cache_snapshot(source_cache, trace_cargo / relative, f"Cargo {relative}")
        uv_cache = Path.home() / ".cache/uv"
        if not uv_cache.is_dir():
            failures.append("provider-free trace requires the bound read-only uv cache")
            return
        trace_cache = temporary / "uv-cache"
        copy_cache_snapshot(uv_cache, trace_cache, "uv")
        environment = _provider_free_environment(home, target)
        environment["CARGO_HOME"] = str(trace_cargo)
        environment["RUSTUP_HOME"] = str(home / "rustup")
        environment["UV_CACHE_DIR"] = str(trace_cache)
        uv_binary = resolve_trace_binary("uv", environment, failures)
        cargo_binary = resolve_trace_binary("cargo", environment, failures)
        rustdoc_binary = resolve_trace_binary("rustdoc", environment, failures)
        if uv_binary is None or cargo_binary is None or rustdoc_binary is None:
            return
        registered = registered_source_commands(manifest, uv_binary, failures)
        if registered is None:
            return
        (
            materializer_command_environment,
            materializer,
            pytest_command_environment,
            pytest,
        ) = registered
        source_environment = dict(environment)
        source_environment.update(materializer_command_environment)
        mounts = {
            trace_source: "/workspace/source",
            trace_harness: "/tmp/rwe-harness",
            target: "/tmp/rwe-target",
            trace_cargo: "/tmp/rwe-cargo-home",
            trace_cache: "/tmp/rwe-host-uv-cache",
        }
        _run_provider_free_trace(
            "pre_ac_source_materializer",
            materializer,
            trace_source,
            source_environment,
            mounts,
            {trace_source, target, trace_cache},
            True,
            failures,
        )
        verify_trace_generated_baseline(trace_source, manifest, failures)
        source_environment = dict(environment)
        source_environment.update(pytest_command_environment)
        _run_provider_free_trace(
            "pre_ac_source_pytest",
            pytest,
            trace_source,
            source_environment,
            mounts,
            {trace_source, target, trace_cache},
            True,
            failures,
        )
        _run_provider_free_trace(
            "post_ac_engine_tests",
            [
                cargo_binary,
                "test",
                "-p",
                "engine",
                "--",
                "--skip",
                "cli::codex_mediation_admission::tests::isolation_probe_hides_synthetic_auth_path",
            ],
            trace_harness,
            environment,
            mounts,
            {trace_harness, target, trace_cargo},
            True,
            failures,
        )
        engine_test_binary = find_engine_test_binary(target, failures)
        if engine_test_binary is None:
            return
        verify_bwrap_capability(environment, failures)
        # This registered test intentionally probes nested bubblewrap. Running
        # it inside the outer sandbox makes it observe the outer namespace.
        # The direct lane is allowed only after the independent bwrap probe
        # above proves that the test can establish its own filesystem boundary.
        nested_environment = {
            key: value
            for key, value in environment.items()
            if key not in {"CARGO_HOME", "RUSTUP_HOME", "CARGO_TARGET_DIR", "UV_CACHE_DIR"}
        }
        _run_provider_free_trace(
            "post_ac_nested_isolation_probe",
            [
                str(engine_test_binary),
                "cli::codex_mediation_admission::tests::isolation_probe_hides_synthetic_auth_path",
                "--exact",
            ],
            trace_harness,
            nested_environment,
            mounts,
            {trace_harness, target},
            False,
            failures,
            ("BLOCKED:bwrap_userns_unavailable",),
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
    try:
        recipe_modes = git_revision_modes(source_root, recipe_commit)
    except OSError:
        failures.append("bound recipe file modes are unavailable")
        recipe_modes = {}
    for relative in paths:
        candidate = source_root / relative
        expected = git_blob(source_root, recipe_commit, relative)
        if (
            expected is None
            or candidate.is_symlink()
            or not candidate.is_file()
            or candidate.read_bytes() != expected
            or relative not in recipe_modes
            or bool(candidate.stat().st_mode & 0o111)
            != bool(recipe_modes[relative] & 0o111)
        ):
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


def verify_rust_reconstruction_binding(
    harness_root: Path, manifest: dict[str, object], failures: list[str]
) -> None:
    """Fail closed if the Rust binding drifts from the manifest owner."""
    path = harness_root / "engine/src/rwe/frozen_rwe_bindings.rs"
    try:
        source = path.read_text(encoding="utf-8")
    except OSError:
        failures.append("Rust reconstruction binding is unavailable")
        return
    repository = manifest.get("repository", {})
    recipe = manifest.get("reconstruction_recipe", {})
    frozen = manifest.get("frozen_rwe", {})
    expected = {
        "FROZEN_RWE_TARGET_MAIN_SHA": repository.get("source_commit"),
        "FROZEN_RWE_TARGET_TREE_HASH": frozen.get("frozen_task_source_tree_hash"),
        "FROZEN_RWE_PRE_AC_SOURCE_TREE_HASH": repository.get("source_tree_hash"),
        "FROZEN_RWE_RECIPE_COMMIT": recipe.get("recipe_commit"),
        "FROZEN_RWE_RECIPE_TREE_HASH": repository.get("reconstruction_recipe_tree_hash"),
        "FROZEN_RWE_SNAPSHOT_MANIFEST_SHA256": FROZEN_RWE_MANIFEST_SHA256,
        "FROZEN_RWE_CORPUS_SHA256": frozen.get("corpus_sha256"),
        "FROZEN_RWE_PROTOCOL_SHA256": frozen.get("protocol_sha256"),
        "FROZEN_RWE_SCHEDULE_SHA256": frozen.get("schedule_sha256"),
        "FROZEN_RWE_POST_AC_MAIN_SHA": FROZEN_POST_AC_IDENTITY["main_sha"],
        "FROZEN_RWE_POST_AC_TREE_HASH": FROZEN_POST_AC_IDENTITY["tree_sha"],
        "FROZEN_RWE_POST_AC_CARGO_LOCK_SHA256": FROZEN_POST_AC_IDENTITY["cargo_lock_sha256"],
        "FROZEN_RWE_POST_AC_RUST_TOOLCHAIN_SHA256": FROZEN_POST_AC_IDENTITY[
            "rust_toolchain_sha256"
        ],
    }
    for name, value in expected.items():
        if not isinstance(value, str):
            failures.append(f"Rust reconstruction binding expectation is malformed: {name}")
            continue
        match = re.search(rf"pub const {re.escape(name)}: &str\s*=\s*\"([^\"]+)\"", source)
        if match is None:
            failures.append(f"Rust reconstruction binding constant is missing: {name}")
        elif match.group(1) != value:
            failures.append(f"Rust reconstruction binding differs from manifest: {name}")


def verify_snapshot_integrity(
    source_root: Path, harness_root: Path, manifest: dict[str, object], failures: list[str]
) -> None:
    verify_post_ac_harness(harness_root, FROZEN_POST_AC_IDENTITY, failures)
    verify_git_overlay(source_root, manifest, failures)
    verify_frozen_task_bindings(harness_root, manifest, failures)
    verify_rust_reconstruction_binding(CURRENT_REPOSITORY_ROOT, manifest, failures)


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
            verify_observed_toolchain(failures)
            verify_isolated_roots(source_root, harness_root, failures)
            if post_ac_identity is None:
                failures.append("post-AC identity binding is required")
            elif post_ac_identity != FROZEN_POST_AC_IDENTITY:
                failures.append("post-AC identity differs from the frozen RWE binding")
            else:
                verify_snapshot_integrity(source_root, harness_root, manifest, failures)
            if execute_traces and not failures:
                verify_provider_free_traces(source_root, harness_root, manifest, failures)
                verify_snapshot_integrity(source_root, harness_root, manifest, failures)
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
