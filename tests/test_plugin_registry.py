"""Tests for dispatch/plugin_registry.py — register, discover, validate, search."""

import json
import sys
import tempfile
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.plugin_registry import (
    PLUGIN_REGISTRY_SCHEMA_VERSION,
    PluginRegistry,
)
from harness_core.dispatch.plugin_system import PluginManifest

FIXTURES_DIR = Path(__file__).resolve().parents[1] / "tests" / "fixtures"


def _valid_manifest(**overrides) -> PluginManifest:
    defaults = dict(
        plugin_id="p1", name="P1", version="1.0.0", author="a",
        permissions=("dispatch:read",), entrypoints=("main.py",),
        compatible_dispatcher_versions=("dispatcher.v1",), required_env=(),
        network_access=False, filesystem_access=False, trust_level="community",
    )
    defaults.update(overrides)
    return PluginManifest(**defaults)


class SchemaVersionTests(unittest.TestCase):
    def test_registry_schema_version(self):
        self.assertEqual(PLUGIN_REGISTRY_SCHEMA_VERSION, "plugin_registry.v1")


class RegisterUnregisterTests(unittest.TestCase):
    def test_register_valid(self):
        reg = PluginRegistry()
        self.assertTrue(reg.register_plugin(_valid_manifest()))
        self.assertEqual(len(reg.list_registered()), 1)

    def test_register_duplicate_rejected(self):
        reg = PluginRegistry()
        self.assertTrue(reg.register_plugin(_valid_manifest()))
        self.assertFalse(reg.register_plugin(_valid_manifest()))
        self.assertEqual(len(reg.list_registered()), 1)

    def test_register_invalid_manifest_rejected(self):
        reg = PluginRegistry()
        m = _valid_manifest(plugin_id="")
        self.assertFalse(reg.register_plugin(m))
        self.assertEqual(len(reg.list_registered()), 0)

    def test_unregister_existing(self):
        reg = PluginRegistry()
        reg.register_plugin(_valid_manifest())
        self.assertTrue(reg.unregister_plugin("p1"))
        self.assertEqual(len(reg.list_registered()), 0)

    def test_unregister_nonexistent(self):
        reg = PluginRegistry()
        self.assertFalse(reg.unregister_plugin("nope"))

    def test_get_plugin(self):
        reg = PluginRegistry()
        reg.register_plugin(_valid_manifest())
        self.assertIsNotNone(reg.get_plugin("p1"))
        self.assertIsNone(reg.get_plugin("nope"))

    def test_list_registered(self):
        reg = PluginRegistry()
        reg.register_plugin(_valid_manifest(plugin_id="a", name="A"))
        reg.register_plugin(_valid_manifest(plugin_id="b", name="B"))
        registered = reg.list_registered()
        self.assertEqual(len(registered), 2)
        ids = {m.plugin_id for m in registered}
        self.assertEqual(ids, {"a", "b"})


class ValidateManifestTests(unittest.TestCase):
    def test_valid_manifest_no_errors(self):
        reg = PluginRegistry()
        errors = reg.validate_manifest(_valid_manifest())
        self.assertEqual(errors, [])

    def test_missing_plugin_id(self):
        reg = PluginRegistry()
        errors = reg.validate_manifest(_valid_manifest(plugin_id=""))
        self.assertTrue(any("plugin_id" in e for e in errors))

    def test_missing_name(self):
        reg = PluginRegistry()
        errors = reg.validate_manifest(_valid_manifest(name=""))
        self.assertTrue(any("name" in e for e in errors))

    def test_missing_version(self):
        reg = PluginRegistry()
        errors = reg.validate_manifest(_valid_manifest(version=""))
        self.assertTrue(any("version" in e for e in errors))

    def test_missing_author(self):
        reg = PluginRegistry()
        errors = reg.validate_manifest(_valid_manifest(author=""))
        self.assertTrue(any("author" in e for e in errors))

    def test_invalid_trust_level(self):
        reg = PluginRegistry()
        errors = reg.validate_manifest(_valid_manifest(trust_level="superadmin"))
        self.assertTrue(any("trust_level" in e for e in errors))

    def test_unknown_permission(self):
        reg = PluginRegistry()
        errors = reg.validate_manifest(_valid_manifest(permissions=("magic:perm",)))
        self.assertTrue(any("unknown permission" in e for e in errors))


class DiscoverPluginsTests(unittest.TestCase):
    def test_discover_valid_manifests(self):
        reg = PluginRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            mp = Path(tmpdir) / "good.json"
            mp.write_text(json.dumps({
                "plugin_id": "p1", "name": "P1", "version": "1.0.0",
                "author": "a", "permissions": ["dispatch:read"],
                "entrypoints": [], "compatible_dispatcher_versions": [],
                "required_env": [], "network_access": False,
                "filesystem_access": False, "trust_level": "community",
            }))
            manifests = reg.discover_plugins(tmpdir)
            self.assertEqual(len(manifests), 1)
            self.assertEqual(manifests[0].plugin_id, "p1")

    def test_discover_skips_invalid(self):
        reg = PluginRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            bad = Path(tmpdir) / "bad.json"
            bad.write_text(json.dumps({"schema_version": "plugin_manifest.v1"}))
            good = Path(tmpdir) / "good.json"
            good.write_text(json.dumps({
                "plugin_id": "ok", "name": "OK", "version": "1.0.0",
                "author": "a", "permissions": [],
                "entrypoints": [], "compatible_dispatcher_versions": [],
                "required_env": [], "network_access": False,
                "filesystem_access": False, "trust_level": "community",
            }))
            manifests = reg.discover_plugins(tmpdir)
            self.assertEqual(len(manifests), 1)
            self.assertEqual(manifests[0].plugin_id, "ok")

    def test_discover_nonexistent_dir(self):
        reg = PluginRegistry()
        self.assertEqual(reg.discover_plugins("/no/such/dir"), [])

    def test_discover_skips_non_json(self):
        reg = PluginRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            Path(tmpdir, "readme.txt").write_text("not json")
            manifests = reg.discover_plugins(tmpdir)
            self.assertEqual(manifests, [])

    def test_discover_sorts_alphabetically(self):
        reg = PluginRegistry()
        with tempfile.TemporaryDirectory() as tmpdir:
            for pid in ["z_plugin", "a_plugin"]:
                mp = Path(tmpdir) / f"{pid}.json"
                mp.write_text(json.dumps({
                    "plugin_id": pid, "name": pid, "version": "1.0.0",
                    "author": "a", "permissions": [],
                    "entrypoints": [], "compatible_dispatcher_versions": [],
                    "required_env": [], "network_access": False,
                    "filesystem_access": False, "trust_level": "community",
                }))
            manifests = reg.discover_plugins(tmpdir)
            self.assertEqual(manifests[0].plugin_id, "a_plugin")
            self.assertEqual(manifests[1].plugin_id, "z_plugin")


class SearchPluginsTests(unittest.TestCase):
    def test_search_by_name(self):
        reg = PluginRegistry()
        reg.register_plugin(_valid_manifest(plugin_id="p1", name="Logger"))
        reg.register_plugin(_valid_manifest(plugin_id="p2", name="Tracer"))
        results = reg.search_plugins("log")
        self.assertEqual(len(results), 1)
        self.assertEqual(results[0].plugin_id, "p1")

    def test_search_by_author(self):
        reg = PluginRegistry()
        reg.register_plugin(_valid_manifest(plugin_id="p1", author="Alice"))
        reg.register_plugin(_valid_manifest(plugin_id="p2", author="Bob"))
        results = reg.search_plugins("alice")
        self.assertEqual(len(results), 1)
        self.assertEqual(results[0].plugin_id, "p1")

    def test_search_no_results(self):
        reg = PluginRegistry()
        reg.register_plugin(_valid_manifest(plugin_id="p1", name="X"))
        self.assertEqual(reg.search_plugins("nonexistent"), [])

    def test_search_case_insensitive(self):
        reg = PluginRegistry()
        reg.register_plugin(_valid_manifest(plugin_id="p1", name="MyPlugin"))
        results = reg.search_plugins("myplugin")
        self.assertEqual(len(results), 1)


class PluginRegistryThreadSafety(unittest.TestCase):
    def test_concurrent_register_unregister(self):
        reg = PluginRegistry()
        errors: list[Exception] = []
        num_threads = 10

        def register_and_unregister(idx: int) -> None:
            try:
                m = _valid_manifest(plugin_id=f"t{idx}", name=f"Thread{idx}")
                reg.register_plugin(m)
                reg.unregister_plugin(f"t{idx}")
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=register_and_unregister, args=(i,))
                   for i in range(num_threads)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=10)

        self.assertEqual(errors, [], f"Thread errors: {errors}")
        self.assertEqual(len(reg.list_registered()), 0)

    def test_concurrent_search(self):
        reg = PluginRegistry()
        errors: list[Exception] = []
        stop_event = threading.Event()

        for i in range(10):
            reg.register_plugin(_valid_manifest(
                plugin_id=f"s{i}", name=f"Searchable{i}",
            ))

        def searcher_loop() -> None:
            try:
                while not stop_event.is_set():
                    reg.search_plugins("searchable")
                    reg.search_plugins("nonexistent")
            except Exception as e:
                errors.append(e)

        def modifier() -> None:
            try:
                for i in range(10, 20):
                    reg.register_plugin(_valid_manifest(
                        plugin_id=f"s{i}", name=f"Searchable{i}",
                    ))
                for i in range(10, 20):
                    reg.unregister_plugin(f"s{i}")
            except Exception as e:
                errors.append(e)

        t_searcher = threading.Thread(target=searcher_loop)
        t_modifier = threading.Thread(target=modifier)
        t_searcher.start()
        t_modifier.start()
        t_modifier.join(timeout=10)
        stop_event.set()
        t_searcher.join(timeout=10)

        self.assertEqual(errors, [], f"Thread errors: {errors}")

    def test_concurrent_register_and_list(self):
        reg = PluginRegistry()
        errors: list[Exception] = []
        num_threads = 8

        def register_loop(idx: int) -> None:
            try:
                for j in range(5):
                    pid = f"t{idx}_{j}"
                    reg.register_plugin(_valid_manifest(
                        plugin_id=pid, name=f"Plugin{idx}_{j}",
                    ))
                    reg.list_registered()
            except Exception as e:
                errors.append(e)

        threads = [threading.Thread(target=register_loop, args=(i,))
                   for i in range(num_threads)]
        for t in threads:
            t.start()
        for t in threads:
            t.join(timeout=10)

        self.assertEqual(errors, [], f"Thread errors: {errors}")
        self.assertEqual(len(reg.list_registered()), num_threads * 5)


if __name__ == "__main__":
    unittest.main()
