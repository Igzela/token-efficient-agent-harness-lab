"""Tests for pyproject.toml packaging metadata."""

import tomllib
import unittest
from pathlib import Path

PYPROJECT_PATH = Path(__file__).resolve().parent.parent / "pyproject.toml"


class TestPackagingMetadata(unittest.TestCase):
    """Validate pyproject.toml packaging metadata."""

    @classmethod
    def setUpClass(cls):
        with open(PYPROJECT_PATH, "rb") as f:
            cls.config = tomllib.load(f)

    def test_project_name(self):
        self.assertEqual(self.config["project"]["name"], "token-efficient-agent-harness-lab")

    def test_project_version(self):
        self.assertEqual(self.config["project"]["version"], "0.1.0")

    def test_requires_python(self):
        self.assertEqual(self.config["project"]["requires-python"], ">=3.11")

    def test_dependencies_empty(self):
        self.assertEqual(self.config["project"]["dependencies"], [])

    def test_build_system_requires_setuptools(self):
        requires = self.config["build-system"]["requires"]
        self.assertTrue(any(s.startswith("setuptools") for s in requires))

    def test_build_system_requires_wheel(self):
        self.assertIn("wheel", self.config["build-system"]["requires"])

    def test_build_backend(self):
        self.assertIn("setuptools", self.config["build-system"]["build-backend"])

    def test_setuptools_packages_find_where(self):
        where = self.config["tool"]["setuptools"]["packages"]["find"]["where"]
        self.assertEqual(where, ["src"])

    def test_project_has_description(self):
        self.assertIsInstance(self.config["project"]["description"], str)
        self.assertTrue(len(self.config["project"]["description"]) > 0)

    def test_project_has_build_system(self):
        self.assertIn("build-system", self.config)


if __name__ == "__main__":
    unittest.main()
