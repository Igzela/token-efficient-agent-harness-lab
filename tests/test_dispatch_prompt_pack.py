"""Tests for prompt_pack_gen.py — PromptPack schema and PromptPackGenerator."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.dispatch_engine import DispatchEngine
from harness_core.dispatch.prompt_pack_gen import PromptPack, PromptPackGenerator


def _make_bundle(raw_request: str = "Summarize the README"):
    engine = DispatchEngine()
    return engine.dispatch(raw_request)


class PromptPackSchemaTests(unittest.TestCase):
    def test_to_dict_roundtrip(self):
        bundle = _make_bundle()
        gen = PromptPackGenerator()
        pack = gen.generate(bundle.decision, "Summarize the README", dispatch_id=bundle.record.dispatch_id)
        d = pack.to_dict()
        self.assertIn("prompt_pack_id", d)
        self.assertIn("dispatch_id", d)
        self.assertEqual(d["dispatch_id"], bundle.record.dispatch_id)
        self.assertIn("recommended_model_tier", d)
        self.assertIn("evaluation_checklist", d)
        self.assertIsInstance(d["evaluation_checklist"], list)

    def test_has_required_fields(self):
        bundle = _make_bundle()
        gen = PromptPackGenerator()
        pack = gen.generate(bundle.decision, "Test", dispatch_id=bundle.record.dispatch_id)
        self.assertTrue(pack.prompt_pack_id)
        self.assertTrue(pack.dispatch_id)
        self.assertTrue(pack.system_prompt)
        self.assertTrue(pack.user_prompt)
        self.assertTrue(pack.pasteback_instructions)
        self.assertGreater(pack.max_input_tokens, 0)
        self.assertGreater(pack.max_output_tokens, 0)

    def test_dispatch_id_required(self):
        gen = PromptPackGenerator()
        decision = _make_bundle().decision
        with self.assertRaises(ValueError):
            gen.generate(decision, "Test")


class PromptPackGeneratorTests(unittest.TestCase):
    def setUp(self):
        self.gen = PromptPackGenerator()

    def test_low_risk_default_checklist(self):
        bundle = _make_bundle("Summarize the README")
        pack = self.gen.generate(bundle.decision, "Summarize the README", dispatch_id=bundle.record.dispatch_id)
        self.assertIn("schema_validity", pack.evaluation_checklist)
        self.assertNotIn("human_review_required", pack.evaluation_checklist)

    def test_high_risk_includes_human_review(self):
        bundle = _make_bundle("Fix the bug and commit changes to main")
        pack = self.gen.generate(bundle.decision, "Fix the bug and commit changes to main", dispatch_id=bundle.record.dispatch_id)
        self.assertIn("human_review_required", pack.evaluation_checklist)

    def test_forbidden_outputs_from_constraints(self):
        bundle = _make_bundle("Fix the bug and commit changes to main")
        pack = self.gen.generate(bundle.decision, "Fix the bug and commit changes to main", dispatch_id=bundle.record.dispatch_id)
        self.assertTrue(len(pack.forbidden_outputs) > 0)

    def test_user_prompt_equals_raw_request(self):
        bundle = _make_bundle("Summarize the README")
        pack = self.gen.generate(bundle.decision, "Summarize the README", dispatch_id=bundle.record.dispatch_id)
        self.assertEqual(pack.user_prompt, "Summarize the README")

    def test_deterministic_same_input(self):
        bundle = _make_bundle("Summarize the README")
        p1 = self.gen.generate(bundle.decision, "Summarize the README", dispatch_id=bundle.record.dispatch_id)
        p2 = self.gen.generate(bundle.decision, "Summarize the README", dispatch_id=bundle.record.dispatch_id)
        self.assertEqual(p1.user_prompt, p2.user_prompt)
        self.assertEqual(p1.max_input_tokens, p2.max_input_tokens)


if __name__ == "__main__":
    unittest.main()
