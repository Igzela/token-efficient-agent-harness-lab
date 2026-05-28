"""Tests for harness_core/doc_generator.py — DocGenerator auto-documentation."""

import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.doc_generator import DOC_GENERATOR_SCHEMA_VERSION, DocGenerator


DISPATCH_DIR = Path(__file__).resolve().parents[1] / "src" / "harness_core" / "dispatch"
SAMPLE_FILE = DISPATCH_DIR / "dispatch_decision.py"
SAMPLE_MODULE = DISPATCH_DIR / "health_checker.py"


class DocGeneratorCreationTests(unittest.TestCase):
    def test_schema_version(self):
        self.assertEqual(DOC_GENERATOR_SCHEMA_VERSION, "doc_generator.v1")

    def test_instantiation(self):
        gen = DocGenerator()
        self.assertIsNotNone(gen)


class GenerateModuleDocsTests(unittest.TestCase):
    def setUp(self):
        self.gen = DocGenerator()

    def test_produces_string(self):
        result = self.gen.generate_module_docs(SAMPLE_FILE)
        self.assertIsInstance(result, str)

    def test_starts_with_heading(self):
        result = self.gen.generate_module_docs(SAMPLE_FILE)
        self.assertTrue(result.startswith("# "))

    def test_contains_schema_versions(self):
        result = self.gen.generate_module_docs(SAMPLE_FILE)
        self.assertIn("schema_version", result.lower())
        self.assertIn("Schema Versions", result)

    def test_contains_dataclasses(self):
        result = self.gen.generate_module_docs(SAMPLE_FILE)
        self.assertIn("Data Classes", result)
        self.assertIn("DispatchDecision", result)

    def test_dataclass_fields_table(self):
        result = self.gen.generate_module_docs(SAMPLE_FILE)
        self.assertIn("| Field | Type | Default |", result)
        self.assertIn("decision_id", result)

    def test_nonexistent_file(self):
        result = self.gen.generate_module_docs("/nonexistent/module.py")
        self.assertIn("could not read", result)

    def test_non_python_file(self):
        result = self.gen.generate_module_docs("/etc/hostname")
        self.assertIn("could not read", result)

    def test_syntax_error_handling(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
            f.write("def broken(\n")
            path = f.name
        result = self.gen.generate_module_docs(path)
        self.assertIn("syntax error", result)
        Path(path).unlink()

    def test_module_with_docstring(self):
        result = self.gen.generate_module_docs(SAMPLE_FILE)
        self.assertIn("Dispatch decision schemas", result)

    def test_health_checker_module(self):
        result = self.gen.generate_module_docs(SAMPLE_MODULE)
        self.assertIn("HealthChecker", result)
        self.assertIn("health_checker.v1", result)


class GenerateSchemaRegistryTests(unittest.TestCase):
    def setUp(self):
        self.gen = DocGenerator()

    def test_produces_table(self):
        result = self.gen.generate_schema_registry(DISPATCH_DIR)
        self.assertIn("| Module | Constant | Version |", result)

    def test_contains_known_schema(self):
        result = self.gen.generate_schema_registry(DISPATCH_DIR)
        self.assertIn("dispatch_decision.v1", result)

    def test_nonexistent_dir(self):
        result = self.gen.generate_schema_registry("/nonexistent/dir")
        self.assertIn("directory not found", result)

    def test_empty_dir(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            result = self.gen.generate_schema_registry(tmpdir)
            self.assertIn("No schema versions found", result)

    def test_multiple_modules(self):
        result = self.gen.generate_schema_registry(DISPATCH_DIR)
        self.assertIn("health_checker", result)
        self.assertIn("durable_store", result)

    def test_starts_with_heading(self):
        result = self.gen.generate_schema_registry(DISPATCH_DIR)
        self.assertTrue(result.startswith("# Schema Registry"))


class GenerateApiReferenceTests(unittest.TestCase):
    def setUp(self):
        self.gen = DocGenerator()

    def test_produces_string(self):
        result = self.gen.generate_api_reference(SAMPLE_MODULE)
        self.assertIsInstance(result, str)

    def test_starts_with_heading(self):
        result = self.gen.generate_api_reference(SAMPLE_MODULE)
        self.assertTrue(result.startswith("# API Reference"))

    def test_contains_class(self):
        result = self.gen.generate_api_reference(SAMPLE_MODULE)
        self.assertIn("HealthChecker", result)

    def test_contains_method(self):
        result = self.gen.generate_api_reference(SAMPLE_MODULE)
        self.assertIn("health", result)

    def test_nonexistent_file(self):
        result = self.gen.generate_api_reference("/nonexistent/module.py")
        self.assertIn("could not read", result)

    def test_syntax_error(self):
        with tempfile.NamedTemporaryFile(mode="w", suffix=".py", delete=False) as f:
            f.write("class Foo(\n")
            path = f.name
        result = self.gen.generate_api_reference(path)
        self.assertIn("syntax error", result)
        Path(path).unlink()

    def test_dispatch_engine_api(self):
        gen = DocGenerator()
        result = gen.generate_api_reference(DISPATCH_DIR / "dispatch_engine.py")
        self.assertIn("DispatchEngine", result)
        self.assertIn("dispatch", result)


class SaveDocsTests(unittest.TestCase):
    def setUp(self):
        self.gen = DocGenerator()

    def test_writes_files(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            written = self.gen.save_docs(tmpdir)
            self.assertGreater(len(written), 0)
            for fpath in written:
                self.assertTrue(Path(fpath).exists())

    def test_creates_schema_registry(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            written = self.gen.save_docs(tmpdir)
            registry = [f for f in written if "SCHEMA_REGISTRY" in f]
            self.assertEqual(len(registry), 1)
            content = Path(registry[0]).read_text()
            self.assertIn("Schema Registry", content)

    def test_creates_modules_dir(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            self.gen.save_docs(tmpdir)
            modules_dir = Path(tmpdir) / "modules"
            self.assertTrue(modules_dir.is_dir())

    def test_creates_module_files(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            written = self.gen.save_docs(tmpdir)
            module_files = [f for f in written if "/modules/" in f]
            self.assertGreater(len(module_files), 0)

    def test_returns_path_list(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            written = self.gen.save_docs(tmpdir)
            self.assertIsInstance(written, list)
            for item in written:
                self.assertIsInstance(item, str)


if __name__ == "__main__":
    unittest.main()
