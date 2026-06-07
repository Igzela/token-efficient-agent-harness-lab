import importlib.util
import sys
import unittest
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


class Sg1PilotMatrixTests(unittest.TestCase):
    def setUp(self):
        self.sg1 = load_script("pilot_dynamic_cli_matrix.py")

    def test_dynamic_graph_mutated_requires_mutation_action_and_count(self):
        body = {
            "tick": {
                "mutations_applied": 1,
                "actions": [{"type": "graph_mutated", "proposal_id": "p1"}],
            }
        }
        self.assertTrue(self.sg1.dynamic_graph_mutated(body))
        self.assertFalse(self.sg1.dynamic_graph_mutated({"tick": {"mutations_applied": 0, "actions": []}}))

    def test_parse_args_defaults_to_all_executors_and_tasks(self):
        args = self.sg1.parse_args([])
        self.assertIsNone(args.executors)
        self.assertIsNone(args.task_classes)
        self.assertEqual(args.base_url, "http://127.0.0.1:8080")

    def test_tick_result_status_reads_nested_tick_result(self):
        body = {"tick": {"result": {"status": "failed"}}}
        self.assertTrue(self.sg1.tick_result_status(body, "failed"))
        self.assertFalse(self.sg1.tick_result_status(body, "completed"))


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


if __name__ == "__main__":
    unittest.main()
