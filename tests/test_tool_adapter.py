"""Tests for dispatch/tool_adapter.py — schema, CRUD, validation, stub execution, thread safety."""

import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.tool_adapter import (
    TOOL_ADAPTER_SCHEMA_VERSION,
    ToolDefinition,
    ToolExecutionRequest,
    ToolExecutionResult,
    ToolAdapterManager,
    make_tool,
)


class SchemaVersionTests(unittest.TestCase):
    def test_schema_version(self):
        self.assertEqual(TOOL_ADAPTER_SCHEMA_VERSION, "tool_adapter.v1")

    def test_default_schema_version(self):
        t = make_tool()
        self.assertEqual(t.schema_version, TOOL_ADAPTER_SCHEMA_VERSION)


class DataclassTests(unittest.TestCase):
    def test_tool_definition_fields(self):
        t = make_tool(
            tool_id="web-search", name="Web Search",
            description="Search the web",
            input_schema={"type": "object", "properties": {"query": {"type": "string"}}},
            output_schema={"type": "object", "properties": {"results": {"type": "array"}}},
            timeout_seconds=15,
            requires_network=True,
        )
        self.assertEqual(t.tool_id, "web-search")
        self.assertEqual(t.name, "Web Search")
        self.assertEqual(t.description, "Search the web")
        self.assertTrue(t.requires_network)
        self.assertEqual(t.timeout_seconds, 15)

    def test_tool_definition_defaults(self):
        t = make_tool()
        self.assertEqual(t.timeout_seconds, 30)
        self.assertFalse(t.requires_network)

    def test_frozen(self):
        t = make_tool()
        with self.assertRaises(AttributeError):
            t.name = "changed"

    def test_execution_request(self):
        req = ToolExecutionRequest(
            tool_id="t1", arguments={"q": "test"}, request_id="r1",
        )
        self.assertEqual(req.tool_id, "t1")
        self.assertEqual(req.arguments, {"q": "test"})
        self.assertEqual(req.request_id, "r1")

    def test_execution_result(self):
        res = ToolExecutionResult(
            request_id="r1", tool_id="t1", success=True,
            output={"count": 5}, error=None, duration_ms=1.2,
        )
        self.assertTrue(res.success)
        self.assertEqual(res.output, {"count": 5})


class RegisterUnregisterTests(unittest.TestCase):
    def test_register_valid(self):
        mgr = ToolAdapterManager()
        self.assertTrue(mgr.register_tool(make_tool()))
        self.assertEqual(len(mgr.list_tools()), 1)

    def test_register_duplicate_rejected(self):
        mgr = ToolAdapterManager()
        self.assertTrue(mgr.register_tool(make_tool()))
        self.assertFalse(mgr.register_tool(make_tool()))
        self.assertEqual(len(mgr.list_tools()), 1)

    def test_register_invalid_rejected(self):
        mgr = ToolAdapterManager()
        self.assertFalse(mgr.register_tool(make_tool(tool_id="")))
        self.assertEqual(len(mgr.list_tools()), 0)

    def test_unregister_existing(self):
        mgr = ToolAdapterManager()
        mgr.register_tool(make_tool())
        self.assertTrue(mgr.unregister_tool("test-tool"))
        self.assertEqual(len(mgr.list_tools()), 0)

    def test_unregister_nonexistent(self):
        mgr = ToolAdapterManager()
        self.assertFalse(mgr.unregister_tool("nope"))

    def test_get_tool(self):
        mgr = ToolAdapterManager()
        mgr.register_tool(make_tool())
        self.assertIsNotNone(mgr.get_tool("test-tool"))
        self.assertIsNone(mgr.get_tool("nope"))

    def test_list_tools(self):
        mgr = ToolAdapterManager()
        mgr.register_tool(make_tool(tool_id="a", name="A"))
        mgr.register_tool(make_tool(tool_id="b", name="B"))
        tools = mgr.list_tools()
        self.assertEqual(len(tools), 2)
        ids = {t.tool_id for t in tools}
        self.assertEqual(ids, {"a", "b"})

    def test_list_returns_copy(self):
        mgr = ToolAdapterManager()
        mgr.register_tool(make_tool())
        tools = mgr.list_tools()
        tools.clear()
        self.assertEqual(len(mgr.list_tools()), 1)


class ValidationTests(unittest.TestCase):
    def test_valid_tool_no_errors(self):
        mgr = ToolAdapterManager()
        errors = mgr.validate_tool(make_tool())
        self.assertEqual(errors, [])

    def test_missing_tool_id(self):
        mgr = ToolAdapterManager()
        errors = mgr.validate_tool(make_tool(tool_id=""))
        self.assertTrue(any("tool_id" in e for e in errors))

    def test_missing_name(self):
        mgr = ToolAdapterManager()
        errors = mgr.validate_tool(make_tool(name=""))
        self.assertTrue(any("name" in e for e in errors))

    def test_missing_description(self):
        mgr = ToolAdapterManager()
        errors = mgr.validate_tool(make_tool(description=""))
        self.assertTrue(any("description" in e for e in errors))

    def test_zero_timeout(self):
        mgr = ToolAdapterManager()
        errors = mgr.validate_tool(make_tool(timeout_seconds=0))
        self.assertTrue(any("timeout" in e for e in errors))

    def test_negative_timeout(self):
        mgr = ToolAdapterManager()
        errors = mgr.validate_tool(make_tool(timeout_seconds=-5))
        self.assertTrue(any("timeout" in e for e in errors))

    def test_invalid_schema_version(self):
        mgr = ToolAdapterManager()
        errors = mgr.validate_tool(make_tool(schema_version="wrong.v1"))
        self.assertTrue(any("schema_version" in e for e in errors))


class StubExecutionTests(unittest.TestCase):
    def test_execute_registered_tool(self):
        mgr = ToolAdapterManager()
        mgr.register_tool(make_tool())
        req = ToolExecutionRequest(tool_id="test-tool", arguments={"q": "hi"}, request_id="r1")
        result = mgr.execute_tool(req)
        self.assertTrue(result.success)
        self.assertEqual(result.output, {})
        self.assertIsNone(result.error)
        self.assertEqual(result.request_id, "r1")
        self.assertEqual(result.tool_id, "test-tool")
        self.assertGreaterEqual(result.duration_ms, 0)

    def test_execute_unknown_tool(self):
        mgr = ToolAdapterManager()
        req = ToolExecutionRequest(tool_id="missing", arguments={}, request_id="r2")
        result = mgr.execute_tool(req)
        self.assertFalse(result.success)
        self.assertIsNone(result.output)
        self.assertIn("not found", result.error)
        self.assertEqual(result.request_id, "r2")

    def test_execute_after_unregister(self):
        mgr = ToolAdapterManager()
        mgr.register_tool(make_tool())
        mgr.unregister_tool("test-tool")
        req = ToolExecutionRequest(tool_id="test-tool", arguments={}, request_id="r3")
        result = mgr.execute_tool(req)
        self.assertFalse(result.success)
        self.assertIn("not found", result.error)

    def test_execute_multiple_tools(self):
        mgr = ToolAdapterManager()
        mgr.register_tool(make_tool(tool_id="t1", name="Tool 1"))
        mgr.register_tool(make_tool(tool_id="t2", name="Tool 2"))
        r1 = mgr.execute_tool(ToolExecutionRequest(tool_id="t1", arguments={}, request_id="r1"))
        r2 = mgr.execute_tool(ToolExecutionRequest(tool_id="t2", arguments={}, request_id="r2"))
        self.assertTrue(r1.success)
        self.assertTrue(r2.success)
        self.assertEqual(r1.tool_id, "t1")
        self.assertEqual(r2.tool_id, "t2")


class ThreadSafetyTests(unittest.TestCase):
    def test_concurrent_register_and_execute(self):
        mgr = ToolAdapterManager()
        errors = []

        def register_many(start: int, count: int) -> None:
            try:
                for i in range(start, start + count):
                    tool = make_tool(tool_id=f"t-{i}", name=f"Tool {i}")
                    mgr.register_tool(tool)
            except Exception as e:
                errors.append(str(e))

        def execute_loop() -> None:
            try:
                for i in range(50):
                    req = ToolExecutionRequest(
                        tool_id=f"t-{i % 100}", arguments={}, request_id=f"r-{i}",
                    )
                    mgr.execute_tool(req)
            except Exception as e:
                errors.append(str(e))

        def list_loop() -> None:
            try:
                for _ in range(50):
                    mgr.list_tools()
            except Exception as e:
                errors.append(str(e))

        threads = [
            threading.Thread(target=register_many, args=(0, 50)),
            threading.Thread(target=register_many, args=(50, 50)),
            threading.Thread(target=execute_loop),
            threading.Thread(target=list_loop),
        ]
        for t in threads:
            t.start()
        for t in threads:
            t.join()

        self.assertEqual(errors, [])
        self.assertEqual(len(mgr.list_tools()), 100)


if __name__ == "__main__":
    unittest.main()
