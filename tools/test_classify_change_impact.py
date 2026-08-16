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
    def test_docs_only_ready_uses_canonical_docs_lane(self) -> None:
        result = module.classify(["README.md", "docs/NEXT_DECISION.md"], draft=False)
        self.assertEqual(result["schema_version"], "ci_change_impact.v2")
        self.assertEqual(result["mode"], "docs_only")
        self.assertTrue(result["docs_only"])
        self.assertFalse(result["fast_only"])
        self.assertFalse(result["has_rust"])
        self.assertFalse(result["has_ts"])

    def test_draft_code_uses_noncanonical_fast_lane(self) -> None:
        result = module.classify(["engine/src/lib.rs"], draft=True)
        self.assertEqual(result["mode"], "fast_draft")
        self.assertFalse(result["docs_only"])
        self.assertTrue(result["fast_only"])
        self.assertTrue(result["has_rust"])

    def test_ready_code_requires_full_matrix(self) -> None:
        result = module.classify(["engine/src/lib.rs"], draft=False)
        self.assertEqual(result["mode"], "full")
        self.assertFalse(result["fast_only"])
        self.assertTrue(result["has_rust"])
        self.assertFalse(result["has_ts"])

    def test_frontend_only_diff_has_no_rust_impact(self) -> None:
        result = module.classify(["dashboard/src/App.tsx", "dashboard/package.json"], draft=False)
        self.assertEqual(result["mode"], "full")
        self.assertFalse(result["has_rust"])
        self.assertTrue(result["has_ts"])

    def test_python_sdk_only_diff_has_no_rust_or_ts_impact(self) -> None:
        result = module.classify(["sdk/python/src/client.py"], draft=False)
        self.assertEqual(result["mode"], "full")
        self.assertFalse(result["has_rust"])
        self.assertFalse(result["has_ts"])

    def test_mixed_or_executable_diff_requires_full_matrix(self) -> None:
        for paths in (
            ["docs/guide.md", "scripts/check.py"],
            ["README.md", ".github/workflows/tests.yml"],
            ["docs/diagram.svg"],
        ):
            with self.subTest(paths=paths):
                result = module.classify(paths, draft=False)
                self.assertEqual(result["mode"], "full")
                self.assertFalse(result["docs_only"])

    def test_empty_or_unsafe_path_fails_closed(self) -> None:
        self.assertEqual(module.classify([], draft=False)["mode"], "full")
        self.assertTrue(module.classify([], draft=False)["has_rust"])
        self.assertFalse(module.is_documentation_path("../README.md"))
        self.assertFalse(module.is_documentation_path("/README.md"))


if __name__ == "__main__":
    unittest.main()
