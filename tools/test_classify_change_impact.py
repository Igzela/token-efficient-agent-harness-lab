from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest

MODULE_PATH = Path(__file__).resolve().parents[1] / "scripts" / "ci" / "classify_change_impact.py"
spec = importlib.util.spec_from_file_location("classify_change_impact", MODULE_PATH)
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ClassifyChangeImpactTests(unittest.TestCase):
    def test_docs_only_ready_uses_fast_docs(self) -> None:
        result = module.classify(["README.md", "docs/NEXT_DECISION.md"], draft=False)
        self.assertEqual(result["mode"], "fast_docs")
        self.assertTrue(result["docs_only"])

    def test_draft_code_uses_fast_draft(self) -> None:
        result = module.classify(["engine/src/lib.rs"], draft=True)
        self.assertEqual(result["mode"], "fast_draft")
        self.assertFalse(result["docs_only"])

    def test_ready_code_requires_full_matrix(self) -> None:
        result = module.classify(["engine/src/lib.rs"], draft=False)
        self.assertEqual(result["mode"], "full")
        self.assertFalse(result["fast_only"])

    def test_empty_or_unsafe_path_fails_closed(self) -> None:
        self.assertEqual(module.classify([], draft=False)["mode"], "full")
        self.assertFalse(module.is_documentation_path("../README.md"))
        self.assertFalse(module.is_documentation_path("/README.md"))


if __name__ == "__main__":
    unittest.main()
