"""Phase 7: ToolAdapterManager — register, validate, and execute external tools."""

from __future__ import annotations

import threading
import time
from dataclasses import dataclass, field

TOOL_ADAPTER_SCHEMA_VERSION = "tool_adapter.v1"


@dataclass(frozen=True)
class ToolDefinition:
    tool_id: str
    name: str
    description: str
    input_schema: dict
    output_schema: dict
    timeout_seconds: int = 30
    requires_network: bool = False
    schema_version: str = TOOL_ADAPTER_SCHEMA_VERSION


@dataclass(frozen=True)
class ToolExecutionRequest:
    tool_id: str
    arguments: dict
    request_id: str


@dataclass(frozen=True)
class ToolExecutionResult:
    request_id: str
    tool_id: str
    success: bool
    output: dict | None
    error: str | None
    duration_ms: float


class ToolAdapterManager:
    """In-memory registry for external tool definitions with stub execution."""

    def __init__(self) -> None:
        self._registered: dict[str, ToolDefinition] = {}
        self._lock = threading.Lock()

    def register_tool(self, tool: ToolDefinition) -> bool:
        errors = self.validate_tool(tool)
        if errors:
            return False
        with self._lock:
            if tool.tool_id in self._registered:
                return False
            self._registered[tool.tool_id] = tool
            return True

    def unregister_tool(self, tool_id: str) -> bool:
        with self._lock:
            if tool_id in self._registered:
                del self._registered[tool_id]
                return True
            return False

    def get_tool(self, tool_id: str) -> ToolDefinition | None:
        with self._lock:
            return self._registered.get(tool_id)

    def list_tools(self) -> list[ToolDefinition]:
        with self._lock:
            return list(self._registered.values())

    def validate_tool(self, tool: ToolDefinition) -> list[str]:
        errors: list[str] = []

        if not tool.tool_id:
            errors.append("tool_id is required")
        if not tool.name:
            errors.append("name is required")
        if not tool.description:
            errors.append("description is required")
        if tool.timeout_seconds <= 0:
            errors.append("timeout_seconds must be positive")
        if tool.schema_version != TOOL_ADAPTER_SCHEMA_VERSION:
            errors.append(f"invalid schema_version: '{tool.schema_version}'")

        return errors

    def execute_tool(self, request: ToolExecutionRequest) -> ToolExecutionResult:
        start = time.monotonic()
        with self._lock:
            tool = self._registered.get(request.tool_id)

        if tool is None:
            duration_ms = (time.monotonic() - start) * 1000
            return ToolExecutionResult(
                request_id=request.request_id,
                tool_id=request.tool_id,
                success=False,
                output=None,
                error=f"tool not found: {request.tool_id}",
                duration_ms=duration_ms,
            )

        duration_ms = (time.monotonic() - start) * 1000
        return ToolExecutionResult(
            request_id=request.request_id,
            tool_id=request.tool_id,
            success=True,
            output={},
            error=None,
            duration_ms=duration_ms,
        )


def make_tool(**kwargs) -> ToolDefinition:
    defaults = dict(
        tool_id="test-tool",
        name="Test Tool",
        description="A test tool",
        input_schema={"type": "object", "properties": {}},
        output_schema={"type": "object", "properties": {}},
    )
    defaults.update(kwargs)
    return ToolDefinition(**defaults)
