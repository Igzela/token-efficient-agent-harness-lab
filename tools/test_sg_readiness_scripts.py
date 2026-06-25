import importlib.util
import io
import sys
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def load_script(name):
    path = ROOT / "scripts" / name
    spec = importlib.util.spec_from_file_location(path.stem, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[path.stem] = module
    spec.loader.exec_module(module)
    return module


class FakeClient:
    def __init__(self, responses):
        self.responses = list(responses)
        self.calls = []

    def call(self, method, path, body=None):
        self.calls.append((method, path, body))
        return self.responses.pop(0)


class Sg2SoakProbeTests(unittest.TestCase):
    def setUp(self):
        self.soak = load_script("soak_ops_drill.py")

    def test_dynamic_recovery_probe_requires_graph_mutation(self):
        client = FakeClient([
            (200, {"plan": {"plan_id": "plan-1"}}),
            (200, {"run": {"run_id": "run-1"}}),
            (200, {"tick": {"result": {"status": "failed"}}}),
            (200, {"tick": {"mutations_applied": 2, "actions": [{"type": "graph_mutated"}]}}),
        ])
        result = self.soak.run_dynamic_recovery(client)
        self.assertTrue(result["success"])
        self.assertEqual(result["run_id"], "run-1")

    def test_retry_exhaustion_probe_requires_failed_final_status(self):
        client = FakeClient([
            (200, {"plan": {"plan_id": "plan-1"}}),
            (200, {"run": {"run_id": "run-1"}}),
            (200, {"tick": {"action": "node_retry", "result": {"status": "failed"}}}),
            (200, {"tick": {"action": "node_executed", "result": {"status": "failed"}}}),
            (409, {"code": "run_terminal"}),
            (200, {"run": {"status": "failed"}}),
        ])
        result = self.soak.run_retry_exhaustion(client)
        self.assertTrue(result["success"])
        self.assertEqual(result["final_status"], "failed")

    def test_queue_pressure_requires_real_queue_evidence(self):
        client = FakeClient([
            (200, {"plan": {"plan_id": "plan-1"}}),
            (200, {"run": {"run_id": "run-1"}}),
            (200, {"plan": {"plan_id": "plan-2"}}),
            (200, {"run": {"run_id": "run-2"}}),
            (200, {"queue": {"total_queued": 0}}),
        ])
        result = self.soak.run_queue_pressure(client, 2)
        self.assertFalse(result["success"])
        self.assertEqual(result["error"], "queue_pressure_not_observed")

    def test_restart_recovery_requires_restart_command(self):
        client = FakeClient([])
        result = self.soak.run_restart_recovery(client, "noop", None)
        self.assertFalse(result["success"])
        self.assertEqual(result["error"], "restart_command_required")


if __name__ == "__main__":
    unittest.main()
