"""Tests for pasteback_parser.py — PastebackSubmission schema and PastebackParser."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.pasteback_parser import PastebackParser, PastebackSubmission


class PastebackSubmissionSchemaTests(unittest.TestCase):
    def test_to_dict_roundtrip(self):
        parser = PastebackParser()
        sub = parser.parse("disp-001", "This is the model output")
        d = sub.to_dict()
        self.assertIn("submission_id", d)
        self.assertEqual(d["dispatch_id"], "disp-001")
        self.assertIn("output_hash", d)

    def test_has_required_fields(self):
        parser = PastebackParser()
        sub = parser.parse("disp-001", "Output text")
        self.assertTrue(sub.submission_id)
        self.assertTrue(sub.output_hash)
        self.assertTrue(sub.submitted_at)


class PastebackParserTests(unittest.TestCase):
    def setUp(self):
        self.parser = PastebackParser()

    def test_parse_valid_output(self):
        sub = self.parser.parse("disp-001", "The answer is 42")
        self.assertEqual(sub.raw_output, "The answer is 42")
        self.assertEqual(sub.dispatch_id, "disp-001")

    def test_parse_strips_whitespace(self):
        sub = self.parser.parse("disp-001", "  output  ")
        self.assertEqual(sub.raw_output, "output")

    def test_parse_empty_raises(self):
        with self.assertRaises(ValueError):
            self.parser.parse("disp-001", "")

    def test_parse_whitespace_only_raises(self):
        with self.assertRaises(ValueError):
            self.parser.parse("disp-001", "   ")

    def test_parse_too_long_raises(self):
        long_output = "x" * 100_001
        with self.assertRaises(ValueError):
            self.parser.parse("disp-001", long_output)

    def test_output_hash_deterministic(self):
        sub1 = self.parser.parse("disp-001", "Same output")
        sub2 = self.parser.parse("disp-001", "Same output")
        self.assertEqual(sub1.output_hash, sub2.output_hash)

    def test_output_hash_different_for_different_output(self):
        sub1 = self.parser.parse("disp-001", "Output A")
        sub2 = self.parser.parse("disp-001", "Output B")
        self.assertNotEqual(sub1.output_hash, sub2.output_hash)

    def test_estimate_tokens(self):
        tokens = self.parser.estimate_tokens("a" * 100)
        self.assertEqual(tokens, 25)

    def test_estimate_cost(self):
        cost = self.parser.estimate_cost(1000, 500)
        self.assertGreater(cost, 0)

    def test_optional_fields(self):
        sub = self.parser.parse(
            "disp-001", "output",
            model_used="gpt-4",
            provider_used="openai",
            claimed_input_tokens=100,
            claimed_output_tokens=50,
            claimed_cost=0.01,
        )
        self.assertEqual(sub.model_used, "gpt-4")
        self.assertEqual(sub.claimed_cost, 0.01)


if __name__ == "__main__":
    unittest.main()
