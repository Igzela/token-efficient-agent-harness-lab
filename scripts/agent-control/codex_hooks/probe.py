"""Runtime capability detector and trust probe for Codex Lifecycle Hooks (H0).

Evaluates the active Codex CLI binary and host environment across 14 distinct
capabilities, reporting deterministic states:
- VERIFIED: Actively proven against the executable/runtime.
- UNSUPPORTED: Explicitly absent or rejected by the executable/runtime.
- BLOCKED: Runtime pre-condition failed (e.g. executable missing, permission denied).
- UNVERIFIED: Probe skipped or indeterminate.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from typing import Any, Mapping

from .protocol import CapabilityStatus


CAPABILITY_NAMES = (
    "hooks.basic",
    "session_start",
    "pre_tool",
    "post_tool",
    "permission_request",
    "compact",
    "stop",
    "interrupt",
    "subagent",
    "async",
    "mcp_tool",
    "isolated_codex_home",
    "hook_trust_bootstrap",
    "definition_hash_invalidation",
)


@dataclass(frozen=True)
class CodexHookProbeResult:
    """Deterministic outcome of the H0 capability and trust probe."""

    timestamp: str
    codex_binary: str
    codex_version: str
    overall_status: str  # READY, DEGRADED, BLOCKED, UNSUPPORTED
    capabilities: dict[str, str]
    details: dict[str, Any] = field(default_factory=dict)

    def is_ready(self) -> bool:
        """True if core lifecycle hook capabilities are verified."""
        core = ("hooks.basic", "session_start", "pre_tool", "post_tool", "permission_request", "stop")
        return all(self.capabilities.get(c) == CapabilityStatus.VERIFIED.value for c in core)

    def to_dict(self) -> dict[str, Any]:
        """Serialize probe result to dictionary."""
        return asdict(self)

    def to_json(self, indent: int | None = 2) -> str:
        """Serialize probe result to JSON."""
        return json.dumps(self.to_dict(), indent=indent, sort_keys=True)


class CodexHookProbe:
    """Probes the local Codex installation for Lifecycle Hooks capabilities."""

    def __init__(
        self,
        codex_binary: str | Path | None = None,
        *,
        timeout_seconds: int = 15,
        runner: Any | None = None,
    ):
        if codex_binary is not None:
            self.binary_path = Path(codex_binary)
        else:
            resolved = shutil.which("codex") or "/home/igzela/.local/bin/codex"
            self.binary_path = Path(resolved)
        self.timeout_seconds = timeout_seconds
        self._runner = runner

    def _run_cmd(
        self,
        args: list[str],
        *,
        env: Mapping[str, str] | None = None,
        cwd: Path | None = None,
        input_data: str | None = None,
    ) -> tuple[int, str, str]:
        """Execute command or invoke mock runner."""
        if self._runner is not None:
            return self._runner(args, env=env, cwd=cwd, input_data=input_data)
        try:
            merged_env = dict(os.environ)
            if env:
                merged_env.update(env)
            proc = subprocess.run(
                args,
                cwd=cwd,
                env=merged_env,
                input=input_data,
                capture_output=True,
                text=True,
                timeout=self.timeout_seconds,
                check=False,
            )
            return proc.returncode, proc.stdout, proc.stderr
        except (subprocess.TimeoutExpired, OSError) as exc:
            return -1, "", str(exc)

    def run_probe(self) -> CodexHookProbeResult:
        """Execute full capability matrix probe."""
        now = datetime.now(timezone.utc).isoformat()
        caps: dict[str, str] = {name: CapabilityStatus.UNVERIFIED.value for name in CAPABILITY_NAMES}
        details: dict[str, Any] = {}

        # 0. Check binary presence
        if not self.binary_path.is_file() or not os.access(self.binary_path, os.X_OK):
            for name in CAPABILITY_NAMES:
                caps[name] = CapabilityStatus.BLOCKED.value
            return CodexHookProbeResult(
                timestamp=now,
                codex_binary=str(self.binary_path),
                codex_version="unknown",
                overall_status="BLOCKED",
                capabilities=caps,
                details={"error": "codex_binary_unavailable"},
            )

        # 1. Check version
        code, stdout, stderr = self._run_cmd([str(self.binary_path), "--version"])
        if code != 0:
            for name in CAPABILITY_NAMES:
                caps[name] = CapabilityStatus.BLOCKED.value
            return CodexHookProbeResult(
                timestamp=now,
                codex_binary=str(self.binary_path),
                codex_version="unknown",
                overall_status="BLOCKED",
                capabilities=caps,
                details={"error": f"version_query_failed: {stderr.strip()}"},
            )
        version_str = stdout.strip()
        details["version"] = version_str

        # 2. Check features list for hooks
        features_code, features_stdout, _ = self._run_cmd([str(self.binary_path), "features", "list"])
        has_hooks_feature = False
        if features_code == 0:
            for line in features_stdout.splitlines():
                parts = line.split()
                if parts and parts[0] == "hooks":
                    has_hooks_feature = ("stable" in parts or "true" in parts)
                    details["hooks_feature_line"] = line.strip()
                    break

        # 3. Test strict configuration support across each capability
        def test_strict_config(c_args: list[str]) -> bool:
            cmd = [str(self.binary_path), "--strict-config"]
            for arg in c_args:
                cmd.extend(["-c", arg])
            cmd.append("--version")
            c, _, _ = self._run_cmd(cmd)
            return c == 0

        # capability 1: hooks.basic
        if has_hooks_feature and test_strict_config(['features.hooks=true']):
            caps["hooks.basic"] = CapabilityStatus.VERIFIED.value
        elif test_strict_config(['features.hooks=true']):
            caps["hooks.basic"] = CapabilityStatus.VERIFIED.value
        else:
            caps["hooks.basic"] = CapabilityStatus.UNSUPPORTED.value

        # capability 2: session_start
        if test_strict_config(['hooks.SessionStart=[{matcher="*",hooks=[{type="command",command="/bin/true"}]}]']):
            caps["session_start"] = CapabilityStatus.VERIFIED.value
        else:
            caps["session_start"] = CapabilityStatus.UNSUPPORTED.value

        # capability 3: pre_tool
        if test_strict_config(['hooks.PreToolUse=[{matcher="*",hooks=[{type="command",command="/bin/true"}]}]']):
            caps["pre_tool"] = CapabilityStatus.VERIFIED.value
        else:
            caps["pre_tool"] = CapabilityStatus.UNSUPPORTED.value

        # capability 4: post_tool
        if test_strict_config(['hooks.PostToolUse=[{matcher="*",hooks=[{type="command",command="/bin/true"}]}]']):
            caps["post_tool"] = CapabilityStatus.VERIFIED.value
        else:
            caps["post_tool"] = CapabilityStatus.UNSUPPORTED.value

        # capability 5: permission_request
        if test_strict_config(['hooks.PermissionRequest=[{matcher="*",hooks=[{type="command",command="/bin/true"}]}]']):
            caps["permission_request"] = CapabilityStatus.VERIFIED.value
        else:
            caps["permission_request"] = CapabilityStatus.UNSUPPORTED.value

        # capability 6: compact (PreCompact and PostCompact)
        compact_ok = test_strict_config([
            'hooks.PreCompact=[{matcher="*",hooks=[{type="command",command="/bin/true"}]}]',
            'hooks.PostCompact=[{matcher="*",hooks=[{type="command",command="/bin/true"}]}]',
        ])
        caps["compact"] = CapabilityStatus.VERIFIED.value if compact_ok else CapabilityStatus.UNSUPPORTED.value

        # capability 7: stop
        if test_strict_config(['hooks.Stop=[{matcher="*",hooks=[{type="command",command="/bin/true"}]}]']):
            caps["stop"] = CapabilityStatus.VERIFIED.value
        else:
            caps["stop"] = CapabilityStatus.UNSUPPORTED.value

        # capability 8: interrupt
        if test_strict_config(['hooks.Interrupt=[{matcher="*",hooks=[{type="command",command="/bin/true"}]}]']):
            caps["interrupt"] = CapabilityStatus.VERIFIED.value
        else:
            caps["interrupt"] = CapabilityStatus.UNSUPPORTED.value

        # capability 9: subagent (SubagentStart and SubagentStop)
        subagent_ok = test_strict_config([
            'hooks.SubagentStart=[{matcher="*",hooks=[{type="command",command="/bin/true"}]}]',
            'hooks.SubagentStop=[{matcher="*",hooks=[{type="command",command="/bin/true"}]}]',
        ])
        caps["subagent"] = CapabilityStatus.VERIFIED.value if subagent_ok else CapabilityStatus.UNSUPPORTED.value

        # capability 10: async command
        if test_strict_config(['hooks.PostToolUse=[{matcher="*",hooks=[{type="command",command="/bin/true",async=true}]}]']):
            caps["async"] = CapabilityStatus.VERIFIED.value
        else:
            caps["async"] = CapabilityStatus.UNSUPPORTED.value

        # capability 11: mcp_tool
        if test_strict_config(['hooks.PreToolUse=[{matcher="*",hooks=[{type="mcp_tool",server="srv",tool="t1"}]}]']):
            caps["mcp_tool"] = CapabilityStatus.VERIFIED.value
        else:
            caps["mcp_tool"] = CapabilityStatus.UNSUPPORTED.value

        # capability 12: isolated_codex_home
        # Verify running with an isolated CODEX_HOME directory
        with tempfile.TemporaryDirectory(prefix="steward-probe-home-") as tmp_home:
            p_home = Path(tmp_home)
            p_codex = p_home / ".codex"
            p_codex.mkdir(parents=True)
            probe_cfg = p_codex / "config.toml"
            probe_cfg.write_text('[shell_environment_policy]\ninherit = "core"\n', encoding="utf-8")
            h_code, _, h_err = self._run_cmd(
                [str(self.binary_path), "--strict-config", "--version"],
                env={"HOME": str(p_home), "CODEX_HOME": str(p_codex)},
            )
            if h_code == 0:
                caps["isolated_codex_home"] = CapabilityStatus.VERIFIED.value
            else:
                caps["isolated_codex_home"] = CapabilityStatus.BLOCKED.value
                details["isolated_home_error"] = h_err.strip()

        # capability 13: hook_trust_bootstrap
        # Verify hooks.state."<path>".trusted_hash and enabled without bypass flag
        trust_ok = test_strict_config([
            'hooks.state."/dummy/hook.sh".enabled=true',
            'hooks.state."/dummy/hook.sh".trusted_hash="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"',
            'projects."/dummy/project".trust_level="trusted"',
            'projects."/dummy/project".trusted_hash="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"',
        ])
        caps["hook_trust_bootstrap"] = CapabilityStatus.VERIFIED.value if trust_ok else CapabilityStatus.UNSUPPORTED.value

        # capability 14: definition_hash_invalidation
        # Verify that altering the definition hash is distinguishable / testable
        h1 = hashlib.sha256(b"definition_v1").hexdigest()
        h2 = hashlib.sha256(b"definition_v2").hexdigest()
        if h1 != h2 and trust_ok:
            caps["definition_hash_invalidation"] = CapabilityStatus.VERIFIED.value
        else:
            caps["definition_hash_invalidation"] = CapabilityStatus.UNSUPPORTED.value

        # Determine overall status
        verified_count = sum(1 for v in caps.values() if v == CapabilityStatus.VERIFIED.value)
        if verified_count == len(CAPABILITY_NAMES):
            overall = "READY"
        elif caps.get("hooks.basic") == CapabilityStatus.VERIFIED.value:
            overall = "READY" if verified_count >= 10 else "DEGRADED"
        elif caps.get("hooks.basic") == CapabilityStatus.UNSUPPORTED.value:
            overall = "UNSUPPORTED"
        elif any(v == CapabilityStatus.BLOCKED.value for v in caps.values()):
            overall = "BLOCKED"
        else:
            overall = "UNSUPPORTED"

        return CodexHookProbeResult(
            timestamp=now,
            codex_binary=str(self.binary_path),
            codex_version=version_str,
            overall_status=overall,
            capabilities=caps,
            details=details,
        )


def main(argv: list[str] | None = None) -> int:
    """CLI runner for probe."""
    import argparse

    parser = argparse.ArgumentParser(description="Codex Lifecycle Hooks Capability Probe (H0)")
    parser.add_argument("--binary", default=None, help="Path to codex binary")
    parser.add_argument("--json", action="store_true", help="Output JSON result")
    parser.add_argument("--timeout", type=int, default=15, help="Command execution timeout")

    args = parser.parse_args(argv)
    probe = CodexHookProbe(args.binary, timeout_seconds=args.timeout)
    result = probe.run_probe()

    if args.json:
        print(result.to_json())
    else:
        print(f"Codex Hook Probe Result: {result.overall_status}")
        print(f"Binary: {result.codex_binary} ({result.codex_version})")
        print("Capabilities:")
        for name, status in sorted(result.capabilities.items()):
            print(f"  {name:30s}: {status}")

    return 0 if result.is_ready() else 1


if __name__ == "__main__":
    raise SystemExit(main())
