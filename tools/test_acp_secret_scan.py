from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


def load_scanner():
    repo_root = Path(__file__).resolve().parents[1]
    script = repo_root / "scripts" / "acp_secret_scan.py"
    spec = importlib.util.spec_from_file_location("acp_secret_scan", script)
    assert spec is not None
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class SecretScanTests(unittest.TestCase):
    def test_masked_assignment_placeholder_is_allowed(self) -> None:
        scanner = load_scanner()
        with tempfile.TemporaryDirectory() as raw_dir:
            root = Path(raw_dir)
            source = root / "fixture.rs"
            source.write_text('let line = "api_' + 'key=***";\n', encoding="utf-8")

            findings = scanner.scan_file(root, source)

        self.assertEqual(findings, [])

    def test_real_key_inside_assignment_is_reported(self) -> None:
        scanner = load_scanner()
        with tempfile.TemporaryDirectory() as raw_dir:
            root = Path(raw_dir)
            source = root / "fixture.txt"
            source.write_text("api_" + "key=sk-" + ("A" * 32) + "\n", encoding="utf-8")

            findings = scanner.scan_file(root, source)

        self.assertEqual(len(findings), 2)
        self.assertEqual({finding.kind for finding in findings}, {"openai_key", "sensitive_assignment"})


if __name__ == "__main__":
    unittest.main()
