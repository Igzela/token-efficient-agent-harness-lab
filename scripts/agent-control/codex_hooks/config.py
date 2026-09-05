"""Configuration generator and trust manager for Codex Lifecycle Hooks.

Generates the TOML configuration block linking Codex hook events to the
dispatcher, establishes cryptographically verified per-handler trust entries
via native Codex hook discovery, and strictly avoids dangerous bypass flags.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
from typing import Any, Mapping, Sequence

DEFAULT_HOOK_EVENTS = (
    "SessionStart",
    "PreCompact",
    "PostCompact",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
)

EVENT_NAME_NORMALIZATION: dict[str, str] = {
    "SessionStart": "session_start",
    "SessionEnd": "session_end",
    "PreToolUse": "pre_tool_use",
    "PostToolUse": "post_tool_use",
    "PermissionRequest": "permission_request",
    "PreCompact": "pre_compact",
    "PostCompact": "post_compact",
    "Stop": "stop",
    "Interrupt": "interrupt",
    "SubagentStart": "subagent_start",
    "SubagentStop": "subagent_stop",
    "UserPromptSubmit": "user_prompt_submit",
}


def normalize_event_name(event_name: str) -> str:
    """Map PascalCase hook event name to snake_case identifier used in hook keys."""
    if event_name in EVENT_NAME_NORMALIZATION:
        return EVENT_NAME_NORMALIZATION[event_name]
    # Generic fallback: PascalCase to snake_case
    out: list[str] = []
    for i, ch in enumerate(event_name):
        if ch.isupper() and i > 0 and not event_name[i - 1].isupper():
            out.append("_")
        out.append(ch.lower())
    return "".join(out)


def compute_file_sha256(file_path: Path | str) -> str:
    """Compute sha256:hex digest of a single file."""
    p = Path(file_path).resolve()
    if not p.is_file():
        raise FileNotFoundError(f"file_not_found: {p}")
    digest = hashlib.sha256(p.read_bytes()).hexdigest()
    return f"sha256:{digest}"


def hook_key(
    config_path: Path | str,
    event_name: str,
    matcher_idx: int = 0,
    hook_idx: int = 0,
) -> str:
    """Predict native hook key assigned by Codex discovery.

    Format: `<config_path>:<normalized_event_name>:<matcher_idx>:<hook_idx>`
    """
    abs_config = str(Path(config_path).resolve())
    norm_event = normalize_event_name(event_name)
    return f"{abs_config}:{norm_event}:{matcher_idx}:{hook_idx}"


def discover_hooks(
    codex_home: Path | str,
    codex_binary: str | Path = "/home/igzela/.local/bin/codex",
    timeout_seconds: int = 15,
) -> list[dict[str, Any]]:
    """Query Codex CLI app-server to discover configured hooks, keys, and hashes."""
    bin_path = str(codex_binary)
    if not Path(bin_path).is_file():
        resolved = shutil.which(bin_path) or shutil.which("codex")
        if not resolved or not Path(resolved).is_file():
            raise FileNotFoundError(f"codex_binary_unavailable: {bin_path}")
        bin_path = resolved

    home_dir = str(Path(codex_home).resolve())
    env = dict(os.environ)
    env["CODEX_HOME"] = home_dir

    proc = subprocess.Popen(
        [bin_path, "app-server", "--stdio"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    )

    def send_and_wait(req: dict[str, Any]) -> dict[str, Any]:
        if proc.stdin is None or proc.stdout is None:
            raise RuntimeError("proc_pipes_unavailable")
        proc.stdin.write(json.dumps(req) + "\n")
        proc.stdin.flush()
        while True:
            line = proc.stdout.readline()
            if not line:
                raise RuntimeError("codex_app_server_eof")
            try:
                data = json.loads(line)
            except json.JSONDecodeError:
                continue
            if data.get("id") == req.get("id"):
                return data

    try:
        init_req = {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"clientInfo": {"name": "steward-discovery", "version": "1.0.0"}},
        }
        send_and_wait(init_req)

        hooks_req = {"jsonrpc": "2.0", "id": 2, "method": "hooks/list", "params": {}}
        res = send_and_wait(hooks_req)

        if "error" in res:
            raise RuntimeError(f"hooks_list_error: {res['error']}")
        result = res.get("result", {})
        data = result.get("data", [])
        if data and isinstance(data, list):
            hooks = data[0].get("hooks", [])
            return list(hooks)
        return []
    finally:
        try:
            if proc.stdin and not proc.stdin.closed:
                proc.stdin.close()
            if proc.stdout and not proc.stdout.closed:
                proc.stdout.close()
            if proc.stderr and not proc.stderr.closed:
                proc.stderr.close()
        except Exception:
            pass
        try:
            proc.terminate()
            proc.wait(timeout=3)
        except Exception:
            try:
                proc.kill()
            except Exception:
                pass


def provision_trust(
    config_path: Path | str,
    codex_binary: str | Path = "/home/igzela/.local/bin/codex",
    timeout_seconds: int = 15,
) -> dict[str, str]:
    """Discover hooks via Codex, write per-handler trust entries, and verify readback.

    Fail-closed: raises on any discovery/provisioning/verification failure.
    A config that declares hooks but yields zero discovered hooks raises
    ``hook_discovery_empty`` instead of silently succeeding.
    """
    cfg = Path(config_path).resolve()
    if not cfg.is_file():
        raise FileNotFoundError(f"config_not_found: {cfg}")
    codex_home = cfg.parent

    # Step 1: Run discovery to get active hook keys and current hashes
    hooks = discover_hooks(codex_home, codex_binary=codex_binary, timeout_seconds=timeout_seconds)
    if not hooks:
        raise RuntimeError(f"hook_discovery_empty: no hooks discovered for config {cfg}")

    trust_entries: dict[str, str] = {}
    lines: list[str] = ["", "# Per-handler trust entries generated from Codex discovery"]
    for h in hooks:
        key = h.get("key", "")
        current_hash = h.get("currentHash", "")
        if key and current_hash:
            trust_entries[key] = current_hash
            lines.append(f'[hooks.state."{key}"]')
            lines.append(f'trusted_hash = "{current_hash}"')
            lines.append("")

    with open(cfg, "a", encoding="utf-8") as f:
        f.write("\n".join(lines))

    # Step 2: Read back from Codex app-server to verify every hook is Trusted
    verified = discover_hooks(codex_home, codex_binary=codex_binary, timeout_seconds=timeout_seconds)
    untrusted: list[dict[str, Any]] = []
    for h in verified:
        status = h.get("trustStatus", "")
        if status != "trusted":
            untrusted.append({"key": h.get("key"), "status": status, "hash": h.get("currentHash")})

    if untrusted:
        raise RuntimeError(f"hook_trust_readback_verification_failed: {untrusted}")

    return trust_entries


class HookConfigGenerator:
    """Generates TOML configuration and manages discovery-based trust state."""

    def __init__(
        self,
        dispatcher_path: Path | str,
        *,
        worktree_path: Path | str | None = None,
        python_executable: str = "/usr/bin/python3",
        timeout_seconds: int = 30,
        hook_events: Sequence[str] = DEFAULT_HOOK_EVENTS,
    ):
        self.dispatcher_path = Path(dispatcher_path).resolve()
        self.worktree_path = Path(worktree_path).resolve() if worktree_path else None
        self.python_executable = python_executable
        self.timeout_seconds = timeout_seconds
        self.hook_events = tuple(hook_events)

    def generate_toml(self, per_handler_trust: Mapping[str, str] | None = None) -> str:
        """Produce valid TOML string configuring hooks and optional trust state."""
        lines: list[str] = [
            "# Generated by Codex Lifecycle Hooks Control Plane",
            "[features]",
            "hooks = true",
            "",
            "[hooks]",
        ]

        # Configure event matchers
        for event in self.hook_events:
            cmd = f"{self.python_executable} {self.dispatcher_path} {event}"
            lines.extend([
                f"{event} = [",
                '  { matcher = "*", hooks = [',
                f'    {{ type = "command", command = "{cmd}", timeout = {self.timeout_seconds} }}',
                "  ] }",
                "]",
            ])

        # If per-handler trust mapping provided, write explicit per-handler entries
        if per_handler_trust:
            lines.append("")
            lines.append("# Cryptographic per-handler trust state")
            for key, chash in sorted(per_handler_trust.items()):
                lines.append(f'[hooks.state."{key}"]')
                lines.append(f'trusted_hash = "{chash}"')
                lines.append("")

        lines.append("")
        return "\n".join(lines)

    def write_config(
        self,
        target_path: Path | str,
        codex_binary: str | Path | None = None,
        auto_trust: bool = True,
    ) -> Path:
        """Write generated configuration to destination and optionally bootstrap trust.

        Fail-closed contract:
        - ``auto_trust=True`` (default): native discovery + trust provisioning +
          readback verification must all succeed, otherwise a RuntimeError is
          raised. No synthetic or predicted trust is ever written.
        - ``auto_trust=False``: writes the hook configuration with NO trust
          entries. Hooks remain untrusted until the caller explicitly provisions
          trust via :func:`provision_trust`. Use only for tests that assert
          untrusted discovery or that inject an explicit mock provisioner.
        """
        target = Path(target_path).resolve()
        target.parent.mkdir(parents=True, exist_ok=True)
        content = self.generate_toml()
        target.write_text(content, encoding="utf-8")

        if auto_trust:
            # No hardcoded fallback path: the binary must be explicitly given
            # or resolvable via PATH. CI runners without Codex must use an
            # explicit mock provisioner instead of receiving fabricated trust.
            bin_path = codex_binary or shutil.which("codex")
            if bin_path is None:
                raise RuntimeError("hook_trust_provisioning_failed: codex_binary_unavailable")
            try:
                provision_trust(target, codex_binary=bin_path)
            except Exception as exc:
                raise RuntimeError(f"hook_trust_provisioning_failed: {exc}") from exc

        return target
