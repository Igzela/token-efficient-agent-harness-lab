"""Tests for dispatch/plugin_system.py — plugin loading, unloading, permission enforcement."""

import json
import sys
import tempfile
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

import unittest
from harness_core.dispatch.plugin_system import (
    ALL_KNOWN_PERMISSIONS,
    PLUGIN_SYSTEM_SCHEMA_VERSION,
    TRUST_LEVEL_COMMUNITY,
    TRUST_LEVEL_OFFICIAL,
    TRUST_LEVEL_VERIFIED,
    VALID_TRUST_LEVELS,
    LoadedPlugin,
    PluginManifest,
    PluginSystem,
    TrustLevel,
)

FIXTURES_DIR = Path(__file__).resolve().parents[1] / "tests" / "fixtures"


class TrustLevelTests(unittest.TestCase):
    def test_enum_values(self):
        self.assertEqual(TrustLevel.COMMUNITY.value, "community")
        self.assertEqual(TrustLevel.VERIFIED.value, "verified")
        self.assertEqual(TrustLevel.OFFICIAL.value, "official")

    def test_valid_trust_levels_constant(self):
        self.assertIn(TRUST_LEVEL_COMMUNITY, VALID_TRUST_LEVELS)
        self.assertIn(TRUST_LEVEL_VERIFIED, VALID_TRUST_LEVELS)
        self.assertIn(TRUST_LEVEL_OFFICIAL, VALID_TRUST_LEVELS)


class PluginManifestTests(unittest.TestCase):
    def test_create_with_defaults(self):
        m = PluginManifest(plugin_id="p1", name="P1", version="1.0.0", author="a")
        self.assertEqual(m.plugin_id, "p1")
        self.assertEqual(m.trust_level, TRUST_LEVEL_COMMUNITY)
        self.assertFalse(m.network_access)
        self.assertIsNone(m.signature)

    def test_to_dict_roundtrip(self):
        m = PluginManifest(
            plugin_id="p1", name="P1", version="1.0.0", author="a",
            permissions=("dispatch:read",), trust_level=TRUST_LEVEL_VERIFIED,
        )
        d = m.to_dict()
        self.assertEqual(d["plugin_id"], "p1")
        self.assertEqual(d["permissions"], ["dispatch:read"])
        self.assertEqual(d["trust_level"], "verified")

    def test_frozen(self):
        m = PluginManifest(plugin_id="p1", name="P1", version="1.0.0", author="a")
        with self.assertRaises(AttributeError):
            m.name = "changed"  # type: ignore[misc]


class SchemaVersionTests(unittest.TestCase):
    def test_plugin_system_schema_version(self):
        self.assertEqual(PLUGIN_SYSTEM_SCHEMA_VERSION, "plugin_system.v1")


class LoadPluginTests(unittest.TestCase):
    def test_load_valid_manifest(self):
        system = PluginSystem()
        manifest_path = FIXTURES_DIR / "sample_plugin_manifest.json"
        with tempfile.TemporaryDirectory() as tmpdir:
            loaded = system.load_plugin(manifest_path, tmpdir)
        self.assertEqual(loaded.manifest.plugin_id, "sample_community_plugin")
        self.assertEqual(loaded.status, "loaded")
        self.assertIsInstance(loaded.loaded_at, float)
        self.assertIn("sample_community_plugin", loaded.module_name)

    def test_load_manifest_not_found(self):
        system = PluginSystem()
        with tempfile.TemporaryDirectory() as tmpdir:
            with self.assertRaises(FileNotFoundError):
                system.load_plugin("/nonexistent/manifest.json", tmpdir)

    def test_load_plugin_dir_not_found(self):
        system = PluginSystem()
        manifest_path = FIXTURES_DIR / "sample_plugin_manifest.json"
        with self.assertRaises(NotADirectoryError):
            system.load_plugin(manifest_path, "/nonexistent/plugin_dir")

    def test_load_invalid_manifest_missing_fields(self):
        system = PluginSystem()
        with tempfile.TemporaryDirectory() as tmpdir:
            manifest_path = Path(tmpdir) / "bad.json"
            manifest_path.write_text(json.dumps({"schema_version": "plugin_manifest.v1"}))
            with self.assertRaises(ValueError) as ctx:
                system.load_plugin(manifest_path, tmpdir)
            self.assertIn("plugin_id", str(ctx.exception))

    def test_load_replaces_existing_plugin(self):
        system = PluginSystem()
        with tempfile.TemporaryDirectory() as tmpdir:
            mp = Path(tmpdir) / "m.json"
            mp.write_text(json.dumps({
                "plugin_id": "p1", "name": "P1", "version": "1.0.0",
                "author": "a", "permissions": ["dispatch:read"],
                "entrypoints": [], "compatible_dispatcher_versions": [],
                "required_env": [], "network_access": False,
                "filesystem_access": False, "trust_level": "community",
            }))
            loaded1 = system.load_plugin(mp, tmpdir)
            loaded2 = system.load_plugin(mp, tmpdir)
            self.assertEqual(len(system.list_plugins()), 1)
            self.assertGreaterEqual(loaded2.loaded_at, loaded1.loaded_at)


class UnloadPluginTests(unittest.TestCase):
    def test_unload_existing(self):
        system = PluginSystem()
        with tempfile.TemporaryDirectory() as tmpdir:
            mp = Path(tmpdir) / "m.json"
            mp.write_text(json.dumps({
                "plugin_id": "p1", "name": "P1", "version": "1.0.0",
                "author": "a", "permissions": ["dispatch:read"],
                "entrypoints": [], "compatible_dispatcher_versions": [],
                "required_env": [], "network_access": False,
                "filesystem_access": False, "trust_level": "community",
            }))
            system.load_plugin(mp, tmpdir)
            self.assertTrue(system.unload_plugin("p1"))
            self.assertIsNone(system.get_plugin("p1"))

    def test_unload_nonexistent(self):
        system = PluginSystem()
        self.assertFalse(system.unload_plugin("nope"))


class PermissionEnforcementTests(unittest.TestCase):
    def _make_system_with_plugin(self, trust_level: str, perms: list[str]) -> PluginSystem:
        system = PluginSystem()
        with tempfile.TemporaryDirectory() as tmpdir:
            mp = Path(tmpdir) / "m.json"
            mp.write_text(json.dumps({
                "plugin_id": "p1", "name": "P1", "version": "1.0.0",
                "author": "a", "permissions": perms,
                "entrypoints": [], "compatible_dispatcher_versions": [],
                "required_env": [], "network_access": False,
                "filesystem_access": False, "trust_level": trust_level,
            }))
            system.load_plugin(mp, tmpdir)
        return system

    def test_community_dispatch_read_allowed(self):
        system = self._make_system_with_plugin("community", ["dispatch:read"])
        self.assertTrue(system.check_permission("p1", "dispatch:read"))

    def test_community_dispatch_write_blocked(self):
        system = PluginSystem()
        with tempfile.TemporaryDirectory() as tmpdir:
            mp = Path(tmpdir) / "m.json"
            mp.write_text(json.dumps({
                "plugin_id": "p1", "name": "P1", "version": "1.0.0",
                "author": "a", "permissions": ["dispatch:write"],
                "entrypoints": [], "compatible_dispatcher_versions": [],
                "required_env": [], "network_access": False,
                "filesystem_access": False, "trust_level": "community",
            }))
            with self.assertRaises(PermissionError):
                system.load_plugin(mp, tmpdir)

    def test_verified_dispatch_write_allowed(self):
        system = self._make_system_with_plugin("verified", ["dispatch:read", "dispatch:write"])
        self.assertTrue(system.check_permission("p1", "dispatch:write"))

    def test_verified_provider_execute_blocked(self):
        system = PluginSystem()
        with tempfile.TemporaryDirectory() as tmpdir:
            mp = Path(tmpdir) / "m.json"
            mp.write_text(json.dumps({
                "plugin_id": "p1", "name": "P1", "version": "1.0.0",
                "author": "a", "permissions": ["provider:execute"],
                "entrypoints": [], "compatible_dispatcher_versions": [],
                "required_env": [], "network_access": False,
                "filesystem_access": False, "trust_level": "verified",
            }))
            with self.assertRaises(PermissionError):
                system.load_plugin(mp, tmpdir)

    def test_official_any_permission_allowed(self):
        system = self._make_system_with_plugin("official", ["dispatch:read", "dispatch:write", "provider:execute"])
        self.assertTrue(system.check_permission("p1", "dispatch:read"))
        self.assertTrue(system.check_permission("p1", "dispatch:write"))
        self.assertTrue(system.check_permission("p1", "provider:execute"))

    def test_check_permission_unknown_plugin(self):
        system = PluginSystem()
        self.assertFalse(system.check_permission("nope", "dispatch:read"))

    def test_check_permission_unknown_permission(self):
        system = self._make_system_with_plugin("official", ["dispatch:read"])
        self.assertFalse(system.check_permission("p1", "bogus:perm"))

    def test_community_load_rejects_write_permission(self):
        system = PluginSystem()
        with tempfile.TemporaryDirectory() as tmpdir:
            mp = Path(tmpdir) / "m.json"
            mp.write_text(json.dumps({
                "plugin_id": "p1", "name": "P1", "version": "1.0.0",
                "author": "a", "permissions": ["dispatch:read", "dispatch:write"],
                "entrypoints": [], "compatible_dispatcher_versions": [],
                "required_env": [], "network_access": False,
                "filesystem_access": False, "trust_level": "community",
            }))
            with self.assertRaises(PermissionError):
                system.load_plugin(mp, tmpdir)


class ListGetPluginTests(unittest.TestCase):
    def test_list_plugins_empty(self):
        system = PluginSystem()
        self.assertEqual(system.list_plugins(), [])

    def test_list_plugins(self):
        system = PluginSystem()
        with tempfile.TemporaryDirectory() as tmpdir:
            mp = Path(tmpdir) / "m.json"
            mp.write_text(json.dumps({
                "plugin_id": "p1", "name": "P1", "version": "1.0.0",
                "author": "a", "permissions": ["dispatch:read"],
                "entrypoints": [], "compatible_dispatcher_versions": [],
                "required_env": [], "network_access": False,
                "filesystem_access": False, "trust_level": "community",
            }))
            system.load_plugin(mp, tmpdir)
            plugins = system.list_plugins()
            self.assertEqual(len(plugins), 1)
            self.assertEqual(plugins[0].manifest.plugin_id, "p1")

    def test_get_plugin(self):
        system = PluginSystem()
        with tempfile.TemporaryDirectory() as tmpdir:
            mp = Path(tmpdir) / "m.json"
            mp.write_text(json.dumps({
                "plugin_id": "p1", "name": "P1", "version": "1.0.0",
                "author": "a", "permissions": ["dispatch:read"],
                "entrypoints": [], "compatible_dispatcher_versions": [],
                "required_env": [], "network_access": False,
                "filesystem_access": False, "trust_level": "community",
            }))
            system.load_plugin(mp, tmpdir)
            self.assertIsNotNone(system.get_plugin("p1"))
            self.assertIsNone(system.get_plugin("nope"))


def _make_manifest_json(plugin_id: str, trust_level: str = "community",
                        permissions: list[str] | None = None) -> dict:
    return {
        "plugin_id": plugin_id, "name": plugin_id, "version": "1.0.0",
        "author": "a", "permissions": permissions or ["dispatch:read"],
        "entrypoints": [], "compatible_dispatcher_versions": [],
        "required_env": [], "network_access": False,
        "filesystem_access": False, "trust_level": trust_level,
    }


class PluginSystemThreadSafety(unittest.TestCase):
    def test_concurrent_load_unload(self):
        system = PluginSystem()
        errors: list[Exception] = []
        num_threads = 8

        with tempfile.TemporaryDirectory() as tmpdir:
            for i in range(num_threads):
                mp = Path(tmpdir) / f"p{i}.json"
                mp.write_text(json.dumps(_make_manifest_json(f"p{i}")))

            def load_then_unload(idx: int) -> None:
                try:
                    mp = Path(tmpdir) / f"p{idx}.json"
                    system.load_plugin(mp, tmpdir)
                    system.unload_plugin(f"p{idx}")
                except Exception as e:
                    errors.append(e)

            threads = [threading.Thread(target=load_then_unload, args=(i,))
                       for i in range(num_threads)]
            for t in threads:
                t.start()
            for t in threads:
                t.join(timeout=10)

        self.assertEqual(errors, [], f"Thread errors: {errors}")
        self.assertEqual(len(system.list_plugins()), 0)

    def test_concurrent_check_permission(self):
        system = PluginSystem()
        errors: list[Exception] = []
        results_lock = threading.Lock()
        permission_results: dict[str, bool] = {}
        num_threads = 8

        with tempfile.TemporaryDirectory() as tmpdir:
            for i in range(num_threads):
                mp = Path(tmpdir) / f"p{i}.json"
                mp.write_text(json.dumps(_make_manifest_json(f"p{i}")))

            def load_and_check(idx: int) -> None:
                try:
                    mp = Path(tmpdir) / f"p{idx}.json"
                    system.load_plugin(mp, tmpdir)
                    has_perm = system.check_permission(f"p{idx}", "dispatch:read")
                    with results_lock:
                        permission_results[f"p{idx}"] = has_perm
                    system.unload_plugin(f"p{idx}")
                except Exception as e:
                    errors.append(e)

            threads = [threading.Thread(target=load_and_check, args=(i,))
                       for i in range(num_threads)]
            for t in threads:
                t.start()
            for t in threads:
                t.join(timeout=10)

        self.assertEqual(errors, [], f"Thread errors: {errors}")
        for pid, has_perm in permission_results.items():
            self.assertTrue(has_perm, f"{pid} should have dispatch:read")

    def test_concurrent_list_get_during_unload(self):
        system = PluginSystem()
        errors: list[Exception] = []
        stop_event = threading.Event()

        with tempfile.TemporaryDirectory() as tmpdir:
            for i in range(10):
                mp = Path(tmpdir) / f"p{i}.json"
                mp.write_text(json.dumps(_make_manifest_json(f"p{i}")))
                system.load_plugin(mp, tmpdir)

            def reader_loop() -> None:
                try:
                    while not stop_event.is_set():
                        system.list_plugins()
                        for i in range(10):
                            system.get_plugin(f"p{i}")
                except Exception as e:
                    errors.append(e)

            def unloader() -> None:
                try:
                    for i in range(10):
                        system.unload_plugin(f"p{i}")
                except Exception as e:
                    errors.append(e)

            t_reader = threading.Thread(target=reader_loop)
            t_unloader = threading.Thread(target=unloader)
            t_reader.start()
            t_unloader.start()
            t_unloader.join(timeout=10)
            stop_event.set()
            t_reader.join(timeout=10)

        self.assertEqual(errors, [], f"Thread errors: {errors}")


if __name__ == "__main__":
    unittest.main()
