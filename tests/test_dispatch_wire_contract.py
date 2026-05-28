"""Tests for frozen dispatch wire contract v1 and Python golden parity."""

from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path

PARITY_DIR = Path(__file__).resolve().parent / "integration" / "parity"
if str(PARITY_DIR) not in sys.path:
    sys.path.insert(0, str(PARITY_DIR))

from common import (  # noqa: E402
    EXPECTED_SCHEMA_FILES,
    GOLDEN_DIR,
    SCHEMA_DIR,
    base_dispatch_fixture_paths,
    golden_fixture_path,
    load_golden_entries,
    load_schema,
    run_parity_checks,
    schema_path,
    validate_golden_entry,
)


class DispatchWireContractTests(unittest.TestCase):
    def test_expected_schema_files_exist(self):
        for name in EXPECTED_SCHEMA_FILES:
            self.assertTrue(schema_path(name).is_file(), name)

    def test_schema_files_parse_as_json(self):
        for name in EXPECTED_SCHEMA_FILES:
            schema = load_schema(name)
            self.assertEqual(schema["$schema"], "https://json-schema.org/draft/2020-12/schema")
            self.assertEqual(schema["type"], "object")

    def test_python_golden_fixtures_exist_for_dispatch_fixtures(self):
        base_paths = base_dispatch_fixture_paths()
        self.assertEqual(20, len(base_paths))
        for base_path in base_paths:
            self.assertTrue(golden_fixture_path(base_path).is_file(), base_path.name)

    def test_python_golden_fixtures_match_wire_schemas(self):
        entries = load_golden_entries()
        self.assertEqual(20, len(entries))
        for entry in entries:
            validate_golden_entry(entry)

    def test_python_reference_matches_golden_fixtures(self):
        report = run_parity_checks()
        self.assertEqual(20, report["checked_fixtures"])
        self.assertEqual(len(EXPECTED_SCHEMA_FILES), report["schema_files"])

    def test_standalone_parity_runner_passes(self):
        runner = PARITY_DIR / "run.py"
        result = subprocess.run(
            [sys.executable, str(runner)],
            cwd=Path(__file__).resolve().parents[1],
            check=True,
            capture_output=True,
            text=True,
        )
        self.assertIn("dispatch wire parity passed", result.stdout)


if __name__ == "__main__":
    unittest.main()
