"""Fixture-first OpenCode external coding adapter (ACP-owned).

Rust owns leases, authority, budgets, and finalization. This process only
validates a bounded request and returns a bounded fixture result. Network,
MCP, provider, and repository mutation paths are rejected.
"""

from __future__ import annotations

__all__ = ["ADAPTER_VERSION", "RUNTIME_KIND"]

ADAPTER_VERSION = "0.1.0"
RUNTIME_KIND = "opencode"
