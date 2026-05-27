"""Tests for prompt_pack_gen.py — PromptPack schema and PromptPackGenerator."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_engine import DispatchEngine
from harness_core.dispatch.prompt_pack_gen import PromptPack, PromptPackGenerator


def _make_decision(raw_request: str = "Summarize the README"):
    engine = DispatchEngine()
    bundle = engine.dispatch(raw_request)
    return bundle.decision


class PromptPackSchemaTests(unittest.TestCase):
    def test_to_dict_roundtrip(self):
        decision = _make_decision()
        gen = PromptPackGenerator()
        pack = gen.generate(decision, "Summarize the README")
        d = pack.to_dict()
        self.assertIn("prompt_pack_id", d)
        self.assertIn("dispatch_id", d)
        self.assertIn("recommended_model_tier", d)
        self.assertIn("evaluation_checklist", d)
        self.assertIsInstance(d["evaluation_checklist"], list)

    def test_has_required_fields(self):
        decision = _make_decision()
        gen = PromptPackGenerator()
        pack = gen.generate(decision, "Test")
        self.assertTrue(pack.prompt_pack_id)
        self.assertTrue(pack.dispatch_id)
        self.assertTrue(pack.system_prompt)
        self.assertTrue(pack.user_prompt)
        self.assertTrue(pack.pasteback_instructions)
        self.assertGreater(pack.max_input_tokens, 0)
        self.assertGreater(pack.max_output_tokens, 0)


class PromptPackGeneratorTests(unittest.TestCase):
    def setUp(self):
        self.gen = PromptPackGenerator()

    def test_low_risk_default_checklist(self):
        decision = _make_decision("Summarize the README")
        pack = self.gen.generate(decision, "Summarize the README")
        self.assertIn("schema_validity", pack.evaluation_checklist)
        self.assertNotIn("human_review_required", pack.evaluation_checklist)

    def test_high_risk_includes_human_review(self):
        decision = _make_decision("Fix the bug and commit changes to main")
        pack = self.gen.generate(decision, "Fix the bug and commit changes to main")
        self.assertIn("human_review_required", pack.evaluation_checklist)

    def test_forbidden_outputs_from_constraints(self):
        decision = _make_decision("Fix the bug and commit changes to main")
        pack = self.gen.generate(decision, "Fix the bug and commit changes to main")
        self.assertTrue(len(pack.forbidden_outputs) > 0)

    def test_user_prompt_equals_raw_request(self):
        decision = _make_decision("Summarize the README")
        pack = self.gen.generate(decision, "Summarize the README")
        self.assertEqual(pack.user_prompt, "Summarize the README")

    def test_deterministic_same_input(self):
        decision = _make_decision("Summarize the README")
        p1 = self.gen.generate(decision, "Summarize the README")
        p2 = self.gen.generate(decision, "Summarize the README")
        self.assertEqual(p1.user_prompt, p2.user_prompt)
        self.assertEqual(p1.max_input_tokens, p2.max_input_tokens)


if __name__ == "__main__":
    unittest.main()
