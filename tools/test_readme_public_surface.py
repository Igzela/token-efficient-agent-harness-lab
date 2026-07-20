"""Tests for tools/check_readme_public_surface.py."""

from __future__ import annotations

import importlib.util
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools" / "check_readme_public_surface.py"


def _load():
    spec = importlib.util.spec_from_file_location("check_readme_public_surface", SCRIPT)
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class TestReadmePublicSurface(unittest.TestCase):
    def test_current_readme_passes(self):
        mod = _load()
        self.assertEqual(mod.main(), 0)


if __name__ == "__main__":
    unittest.main()
