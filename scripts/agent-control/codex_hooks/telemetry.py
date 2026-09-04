"""Minimal, deterministic ROI telemetry for Codex Lifecycle Hooks.

Records token/byte reductions, guarded tool operations, blocked policy violations,
and intercepted premature stops during WorkCard execution.
Persists metrics only to the worker's ephemeral state directory.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
import json
import os
from pathlib import Path
from typing import Any


@dataclass
class HookTelemetryData:
    """Ephemeral ROI metrics data."""

    schema_version: str = "codex_hooks_telemetry.v1"
    workcard_id: str = ""
    session_id: str = ""
    start_time: str = ""
    last_update_time: str = ""
    tools_intercepted: int = 0
    tools_blocked: int = 0
    premature_stops_intercepted: int = 0
    compaction_rehydrations: int = 0
    bootstrap_bytes_saved: int = 0
    receipt_bytes_saved: int = 0
    events: list[dict[str, Any]] = field(default_factory=list)


class HookTelemetry:
    """Manages telemetry recording to worker's ephemeral state directory."""

    def __init__(self, state_dir: Path | str | None = None):
        if state_dir is not None:
            self.state_dir = Path(state_dir)
        else:
            env_dir = os.environ.get("STEWARD_SESSION_STATE_DIR")
            self.state_dir = Path(env_dir) if env_dir else Path("/tmp/codex_hooks_state")
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self.telemetry_path = self.state_dir / "telemetry.json"
        self._data = self._load()

    def _load(self) -> HookTelemetryData:
        if self.telemetry_path.is_file() and not self.telemetry_path.is_symlink():
            try:
                raw = json.loads(self.telemetry_path.read_text(encoding="utf-8"))
                return HookTelemetryData(
                    schema_version=raw.get("schema_version", "codex_hooks_telemetry.v1"),
                    workcard_id=raw.get("workcard_id", ""),
                    session_id=raw.get("session_id", ""),
                    start_time=raw.get("start_time", ""),
                    last_update_time=raw.get("last_update_time", ""),
                    tools_intercepted=raw.get("tools_intercepted", 0),
                    tools_blocked=raw.get("tools_blocked", 0),
                    premature_stops_intercepted=raw.get("premature_stops_intercepted", 0),
                    compaction_rehydrations=raw.get("compaction_rehydrations", 0),
                    bootstrap_bytes_saved=raw.get("bootstrap_bytes_saved", 0),
                    receipt_bytes_saved=raw.get("receipt_bytes_saved", 0),
                    events=raw.get("events", []),
                )
            except (OSError, json.JSONDecodeError):
                pass
        now = datetime.now(timezone.utc).isoformat()
        return HookTelemetryData(
            workcard_id=os.environ.get("STEWARD_WORKCARD_ID", ""),
            start_time=now,
            last_update_time=now,
        )

    def save(self) -> None:
        """Persist telemetry data to disk."""
        self._data.last_update_time = datetime.now(timezone.utc).isoformat()
        self.telemetry_path.write_text(
            json.dumps(asdict(self._data), indent=2, sort_keys=True),
            encoding="utf-8",
        )

    def record_bootstrap(self, injected_bytes: int, baseline_doc_bytes: int = 120_000) -> None:
        """Record context bootstrap efficiency."""
        saved = max(0, baseline_doc_bytes - injected_bytes)
        self._data.bootstrap_bytes_saved += saved
        self._data.events.append({
            "event": "SessionStart",
            "injected_bytes": injected_bytes,
            "estimated_saved_bytes": saved,
            "timestamp": datetime.now(timezone.utc).isoformat(),
        })
        self.save()

    def record_tool_receipt(self, tool_name: str, raw_len: int, summary_len: int) -> None:
        """Record post-tool receipt compression."""
        self._data.tools_intercepted += 1
        saved = max(0, raw_len - summary_len)
        self._data.receipt_bytes_saved += saved
        self._data.events.append({
            "event": "PostToolUse",
            "tool": tool_name,
            "raw_len": raw_len,
            "summary_len": summary_len,
            "saved_bytes": saved,
            "timestamp": datetime.now(timezone.utc).isoformat(),
        })
        self.save()

    def record_tool_block(self, tool_name: str, reason: str) -> None:
        """Record pre-tool policy block."""
        self._data.tools_intercepted += 1
        self._data.tools_blocked += 1
        self._data.events.append({
            "event": "PreToolUse_Block",
            "tool": tool_name,
            "reason": reason,
            "timestamp": datetime.now(timezone.utc).isoformat(),
        })
        self.save()

    def record_stop_intercept(self, attempt: int, reason: str) -> None:
        """Record premature stop prevention."""
        self._data.premature_stops_intercepted += 1
        self._data.events.append({
            "event": "Stop_Intercepted",
            "attempt": attempt,
            "reason": reason,
            "timestamp": datetime.now(timezone.utc).isoformat(),
        })
        self.save()

    def record_compaction(self, rehydrated_bytes: int) -> None:
        """Record context compaction rehydration."""
        self._data.compaction_rehydrations += 1
        self._data.events.append({
            "event": "PostCompact_Rehydrate",
            "rehydrated_bytes": rehydrated_bytes,
            "timestamp": datetime.now(timezone.utc).isoformat(),
        })
        self.save()

    @property
    def metrics(self) -> dict[str, Any]:
        """Return current snapshot of metrics."""
        return asdict(self._data)
